# RFC: Pluggable Storage Backend + Network Access for Vestige

**Status**: Draft / Discussion
**Author**: Jan
**Date**: 2026-02-26
**Vestige version**: v2.x (current main)

## Summary

Add a pluggable storage backend trait to Vestige, enabling PostgreSQL (+pgvector) as an alternative to the current SQLite+FTS5+USearch stack. Simultaneously add HTTP MCP transport with API key authentication to enable centralized/remote deployment.

This keeps the existing local-first SQLite mode fully intact while opening up a server deployment model.

## Motivation

Vestige currently runs as a local process per machine (MCP via stdio, SQLite in `~/.vestige/`). This works great for single-machine use but doesn't support:

- **Multi-machine access**: Same memory brain from laptop, desktop, and server
- **Multi-agent access**: Multiple AI clients hitting one memory store concurrently
- **Future federation**: Syncing memory between decentralized nodes (e.g., MOS/Threefold grid)

SQLite's single-writer model and lack of native network protocol make it unsuitable as a centralized server. PostgreSQL is a natural fit: built-in concurrency (MVCC), authentication, replication, and with `pgvector` + built-in FTS it collapses three separate storage layers into one.

## Design

### Storage Trait

The core abstraction. All 29 cognitive modules interact with storage exclusively through this trait (or a small family of traits).

```rust
use std::collections::HashMap;
use uuid::Uuid;

/// Core memory record, backend-agnostic
#[derive(Debug, Clone)]
pub struct MemoryRecord {
    pub id: Uuid,
    pub domains: Vec<String>, // [] = unclassified, ["dev"], ["dev", "infra"], etc.
    pub domain_scores: HashMap<String, f64>, // raw similarities: {"dev": 0.82, "infra": 0.71}
    pub content: String,
    pub node_type: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,  // dimensionality is runtime config
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

/// FSRS scheduling state, stored alongside each memory
#[derive(Debug, Clone)]
pub struct SchedulingState {
    pub memory_id: Uuid,
    pub stability: f64,
    pub difficulty: f64,
    pub retrievability: f64,
    pub last_review: Option<chrono::DateTime<chrono::Utc>>,
    pub next_review: Option<chrono::DateTime<chrono::Utc>>,
    pub reps: u32,
    pub lapses: u32,
}

/// Hybrid search request
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub domains: Option<Vec<String>>,   // None = search all domains
    pub text: Option<String>,           // FTS query
    pub embedding: Option<Vec<f32>>,    // vector similarity
    pub tags: Option<Vec<String>>,      // tag filter
    pub node_types: Option<Vec<String>>,
    pub limit: usize,
    pub min_retrievability: Option<f64>, // filter by FSRS state
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub record: MemoryRecord,
    pub score: f64,          // combined/fused score
    pub fts_score: Option<f64>,
    pub vector_score: Option<f64>,
}

/// Connection/edge between memories (for spreading activation)
#[derive(Debug, Clone)]
pub struct MemoryEdge {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub edge_type: String,
    pub weight: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Main storage trait — one impl per backend
/// trait_variant generates a Send-bound `MemoryStore` alias,
/// enabling Arc<dyn MemoryStore> without manual boxing.
#[trait_variant::make(MemoryStore: Send)]
pub trait LocalMemoryStore: Sync + 'static {
    // --- Lifecycle ---
    async fn init(&self) -> Result<()>;
    async fn health_check(&self) -> Result<HealthStatus>;

    // --- CRUD ---
    async fn insert(&self, record: &MemoryRecord) -> Result<Uuid>;
    async fn get(&self, id: Uuid) -> Result<Option<MemoryRecord>>;
    async fn update(&self, record: &MemoryRecord) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;

    // --- Search ---
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
    async fn fts_search(&self, text: &str, limit: usize) -> Result<Vec<SearchResult>>;
    async fn vector_search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;

    // --- FSRS Scheduling ---
    async fn get_scheduling(&self, memory_id: Uuid) -> Result<Option<SchedulingState>>;
    async fn update_scheduling(&self, state: &SchedulingState) -> Result<()>;
    async fn get_due_memories(&self, before: chrono::DateTime<chrono::Utc>, limit: usize) -> Result<Vec<(MemoryRecord, SchedulingState)>>;

    // --- Graph (spreading activation) ---
    async fn add_edge(&self, edge: &MemoryEdge) -> Result<()>;
    async fn get_edges(&self, node_id: Uuid, edge_type: Option<&str>) -> Result<Vec<MemoryEdge>>;
    async fn remove_edge(&self, source: Uuid, target: Uuid) -> Result<()>;
    async fn get_neighbors(&self, node_id: Uuid, depth: usize) -> Result<Vec<(MemoryRecord, f64)>>;

    // --- Bulk / Maintenance ---
    async fn count(&self) -> Result<usize>;
    async fn get_stats(&self) -> Result<StoreStats>;
    async fn vacuum(&self) -> Result<()>;
}
```

**Design notes:**

- `trait_variant::make` generates a `MemoryStore` trait alias with `Send`-bound futures, allowing `Arc<dyn MemoryStore>` for runtime backend selection. `LocalMemoryStore` is the base (usable in single-threaded contexts), `MemoryStore` is the Send variant for Axum/tokio.
- `embedding: Option<Vec<f32>>` — dimensions determined at runtime by the configured fastembed model. The backend stores whatever it gets.
- The trait is intentionally flat. The cognitive modules (FSRS-6, spreading activation, synaptic tagging, prediction error gating, etc.) sit *above* this trait and don't need to know about the backend.
- `search()` does hybrid RRF fusion at the backend level — both SQLite and Postgres implementations handle this internally.

### Backend: SQLite (existing, refactored)

Wraps the current implementation behind the trait:

```
SqliteMemoryStore
├── rusqlite connection pool (r2d2 or deadpool)
├── FTS5 virtual table (keyword search)
├── USearch HNSW index (vector search, behind RwLock)
└── WAL mode + busy timeout for concurrent readers
```

No behavioral changes — just the trait boundary.

### Backend: PostgreSQL (new)

```
PgMemoryStore
├── sqlx::PgPool (connection pool, compile-time checked queries)
├── tsvector + GIN index (keyword search)
├── pgvector + HNSW index (vector search)
└── Standard PostgreSQL MVCC concurrency
```

**Schema sketch:**

```sql
CREATE EXTENSION IF NOT EXISTS vector;

-- Domain registry — populated by clustering, not by user
CREATE TABLE domains (
    id          TEXT PRIMARY KEY,           -- auto-generated or user-named
    label       TEXT NOT NULL,              -- human label (suggested or user-provided)
    centroid    vector,                     -- mean embedding of domain members
    top_terms   TEXT[] NOT NULL DEFAULT '{}', -- top keywords for display
    memory_count INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata    JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE memories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domains     TEXT[] NOT NULL DEFAULT '{}', -- [] = unclassified
    domain_scores JSONB NOT NULL DEFAULT '{}', -- {"dev": 0.82, "infra": 0.71} raw similarities
    content     TEXT NOT NULL,
    node_type   TEXT NOT NULL DEFAULT 'general',
    tags        TEXT[] NOT NULL DEFAULT '{}',
    embedding   vector,  -- dimension set at table creation or unconstrained
    metadata    JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- FTS: auto-maintained tsvector column
    search_vec  TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', content), 'A') ||
        setweight(to_tsvector('english', coalesce(node_type, '')), 'B') ||
        setweight(array_to_tsvector(tags), 'C')
    ) STORED
);

-- FTS index
CREATE INDEX idx_memories_fts ON memories USING GIN (search_vec);

-- Vector similarity (HNSW)
CREATE INDEX idx_memories_embedding ON memories
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- Common filters
CREATE INDEX idx_memories_domains ON memories USING GIN (domains);
CREATE INDEX idx_memories_node_type ON memories (node_type);
CREATE INDEX idx_memories_tags ON memories USING GIN (tags);
CREATE INDEX idx_memories_created ON memories (created_at);

-- FSRS scheduling state
CREATE TABLE scheduling (
    memory_id       UUID PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    stability       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    difficulty      DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    retrievability  DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    last_review     TIMESTAMPTZ,
    next_review     TIMESTAMPTZ,
    reps            INTEGER NOT NULL DEFAULT 0,
    lapses          INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_scheduling_next ON scheduling (next_review);

-- Graph edges (spreading activation)
-- Edges can cross domain boundaries — spreading activation respects
-- domain filters when provided, traverses freely when searching all domains.
CREATE TABLE edges (
    source_id   UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_id   UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    edge_type   TEXT NOT NULL DEFAULT 'related',
    weight      DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, target_id, edge_type)
);

CREATE INDEX idx_edges_target ON edges (target_id);

-- API keys
CREATE TABLE api_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash    TEXT NOT NULL UNIQUE,  -- blake3
    label       TEXT NOT NULL,
    scopes      TEXT[] NOT NULL DEFAULT '{read,write}',
    domain_filter TEXT[] NOT NULL DEFAULT '{}', -- {} = access all domains
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used   TIMESTAMPTZ,
    active      BOOLEAN NOT NULL DEFAULT true
);
```

**Hybrid search in SQL:**

```sql
-- RRF (Reciprocal Rank Fusion) combining FTS + vector
-- $1 = query text, $2 = embedding, $3 = limit, $4 = domain filter (NULL for all)
WITH fts AS (
    SELECT id, ts_rank_cd(search_vec, websearch_to_tsquery('english', $1)) AS score,
           ROW_NUMBER() OVER (ORDER BY ts_rank_cd(search_vec, websearch_to_tsquery('english', $1)) DESC) AS rank
    FROM memories
    WHERE search_vec @@ websearch_to_tsquery('english', $1)
      AND ($4::text[] IS NULL OR domains && $4)  -- array overlap: any match
    LIMIT 50
),
vec AS (
    SELECT id, 1 - (embedding <=> $2::vector) AS score,
           ROW_NUMBER() OVER (ORDER BY embedding <=> $2::vector) AS rank
    FROM memories
    WHERE embedding IS NOT NULL
      AND ($4::text[] IS NULL OR domains && $4)
    LIMIT 50
)
SELECT COALESCE(f.id, v.id) AS id,
       COALESCE(1.0 / (60 + f.rank), 0) + COALESCE(1.0 / (60 + v.rank), 0) AS rrf_score,
       f.score AS fts_score,
       v.score AS vector_score
FROM fts f FULL OUTER JOIN vec v ON f.id = v.id
ORDER BY rrf_score DESC
LIMIT $3;
```

### Embedding Configuration

The embedding layer stays external to the storage backend. fastembed runs locally and produces vectors that get passed into `MemoryRecord.embedding`.

```toml
# vestige.toml
[embeddings]
provider = "fastembed"           # only local for now
model = "BAAI/bge-base-en-v1.5" # 768 dimensions
# model = "BAAI/bge-large-en-v1.5"  # 1024 dimensions
# model = "BAAI/bge-small-en-v1.5"  # 384 dimensions

[storage]
backend = "postgres"  # or "sqlite"

[storage.sqlite]
path = "~/.vestige/vestige.db"

[storage.postgres]
url = "postgresql://vestige:secret@localhost:5432/vestige"
max_connections = 10
```

On init, the backend reads the embedding dimension from the first stored vector (or from config) and validates consistency.

For pgvector: you can either create the column as `vector(768)` (fixed, faster) or unconstrained `vector` (flexible, slightly slower). Recommendation: fixed dimension derived from config, with a migration path if the model changes.

### Emergent Domain Model

Instead of user-defined tenants, domains emerge automatically from the data via clustering. The user never has to decide where a memory belongs — the system figures it out.

#### Pipeline

```
Phase 1: Accumulate (cold start, 0 → N memories)
│  All memories stored with domains = [] (unclassified)
│  No classification overhead, just embed and store
│  Threshold N is configurable, default ~150 memories
│
Phase 2: Discover (triggered once at threshold, or manually)
│  Run HDBSCAN on all embeddings:
│    - min_cluster_size: ~10
│    - min_samples: ~5
│    - No eps parameter needed (unlike DBSCAN)
│    - Automatically determines number of clusters
│    - Handles variable-density clusters
│    - Border points between clusters flagged naturally
│
│  For each cluster, extract:
│    - Centroid (mean embedding)
│    - Top terms (TF-IDF or frequency over cluster members)
│    - Suggested label from top terms
│
│  Present to user (via dashboard or CLI):
│    "I found 3 natural groupings in your memories:
│     ● cluster_0 (47 memories): BGP, SONiC, VLAN, FRR, peering...
│     ● cluster_1 (31 memories): solar, kWh, battery, pool, ESPHome...
│     ● cluster_2 (22 memories): Rust, trait, async, zinit, tokio..."
│
│  User can:
│    - Name them: cluster_0 → "infra", cluster_1 → "home", cluster_2 → "dev"
│    - Accept suggested names
│    - Merge clusters
│    - Do nothing (auto-names stick)
│
Phase 3: Soft-assign all existing memories
│  Now that centroids exist, re-score every memory (including
│  those from discovery) against all centroids.
│  This replaces HDBSCAN's hard labels with continuous scores:
│
│    For each memory:
│      similarities = [(domain, cosine_sim(embedding, centroid)) for each domain]
│      domains = [id for (id, score) in similarities if score >= threshold]
│
│  Memories in overlap zones get multiple domains.
│  Memories far from all centroids stay unclassified.
│
Phase 4: Classify (ongoing, after discovery)
│  New memory ingested:
│    1. Compute embedding
│    2. Compute similarity to ALL domain centroids
│    3. Store raw scores in domain_scores JSONB
│    4. Threshold into domains[] array
│    5. Update domain centroids incrementally (running mean)
│
│  Context signals as soft priors:
│    - Git repo / IDE metadata → boost similarity to code-related domains
│    - No workspace context → slight boost toward non-technical domains
│    - These shift the score, never override the embedding distance
│
Phase 5: Re-cluster (periodic, during dream consolidation)
   Re-run HDBSCAN on all embeddings including new ones
   Detect:
     - New clusters forming from previously unclassified memories
     - Existing clusters splitting (domain grew too broad)
     - Clusters merging (domains that were artificially separate)
   Propose changes to user:
     "Your 'dev' domain may have split into two groups:
      - systems (zinit, MOS, containers, VMs) — 34 memories
      - networking (BGP, SONiC, VLANs, MLAG) — 28 memories
      Split them? [yes / no / later]"
   Re-run soft assignment on all memories after structural changes
   Centroid vectors are updated regardless
```

#### Domain Storage

```rust
#[derive(Debug, Clone)]
pub struct Domain {
    pub id: String,
    pub label: String,
    pub centroid: Vec<f32>,
    pub top_terms: Vec<String>,
    pub memory_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

Added to the `MemoryStore` trait:

```rust
    // --- Domains ---
    async fn list_domains(&self) -> Result<Vec<Domain>>;
    async fn get_domain(&self, id: &str) -> Result<Option<Domain>>;
    async fn upsert_domain(&self, domain: &Domain) -> Result<()>;
    async fn delete_domain(&self, id: &str) -> Result<()>;
    async fn classify(&self, embedding: &[f32]) -> Result<Vec<(String, f64)>>;
    // Returns [(domain_id, similarity)] sorted by similarity desc.
    // Caller decides threshold for assignment.
```

#### Classification Module

A new cognitive module alongside FSRS, spreading activation, etc.:

```rust
pub struct DomainClassifier {
    /// Similarity threshold — domains scoring above this are assigned
    pub assign_threshold: f64,       // default: 0.65
    /// Minimum memories before running initial discovery
    pub discovery_threshold: usize,  // default: 150
    /// How often to re-cluster (in dream consolidation passes)
    pub recluster_interval: usize,   // default: every 5th consolidation
    /// HDBSCAN min_cluster_size
    pub min_cluster_size: usize,     // default: 10
}

/// Raw classification result — all scores, before thresholding
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// Similarity to every known domain centroid
    pub scores: HashMap<String, f64>,  // {"dev": 0.82, "infra": 0.71, "home": 0.34}
    /// Domains above assign_threshold
    pub domains: Vec<String>,          // ["dev", "infra"]
}

impl DomainClassifier {
    /// Score a memory against all domain centroids.
    /// Returns raw scores AND thresholded domain list.
    pub fn classify(
        &self,
        embedding: &[f32],
        domains: &[Domain],
    ) -> ClassificationResult {
        if domains.is_empty() {
            return ClassificationResult {
                scores: HashMap::new(),
                domains: vec![],  // still in accumulation phase
            };
        }

        let scores: HashMap<String, f64> = domains.iter()
            .map(|d| (d.id.clone(), cosine_similarity(embedding, &d.centroid)))
            .collect();

        let assigned: Vec<String> = scores.iter()
            .filter(|(_, &s)| s >= self.assign_threshold)
            .map(|(id, _)| id.clone())
            .collect();

        ClassificationResult { scores, domains: assigned }
    }

    /// Soft-assign all existing memories after discovery or re-clustering.
    /// Returns number of memories whose domains changed.
    pub async fn reassign_all(
        &self,
        store: &dyn MemoryStore,
        domains: &[Domain],
    ) -> Result<usize> {
        // Load all memories, re-score, update domains + domain_scores
        // Batched to avoid loading everything into memory at once
        todo!()
    }
}
```

**Key distinction from the previous design:** there's no "closest wins" or "margin" logic. Every domain gets a score, and *all* domains above threshold are assigned. A memory about "deploying zinit containers via BGP-routed network" might score 0.78 on "dev" and 0.72 on "infra" — it gets both. A memory about "solar panel output today" scores 0.85 on "home" and 0.31 on everything else — it only gets "home".

The raw `domain_scores` are always stored, so you (or the dashboard) can see *why* a memory was classified the way it was, and the threshold can be adjusted retroactively without re-computing embeddings.

#### Search Behavior

- **Default (no domain filter)**: searches all memories across all domains
- **Domain-scoped**: `domains: Some(vec!["dev"])` — only memories tagged with `dev`
- **Multi-domain**: `domains: Some(vec!["dev", "infra"])` — memories in either
- **MCP clients can set `X-Vestige-Domain` header** for default scoping, but the system works fine without it

#### HDBSCAN Implementation

HDBSCAN (Hierarchical DBSCAN) over the embedding vectors. Advantages over plain DBSCAN:

- **No `eps` parameter** — the hardest thing to tune in DBSCAN. HDBSCAN determines density thresholds from the data hierarchy.
- **Variable-density clusters** — a tight cluster of networking memories and a spread-out cluster of personal memories are both detected correctly.
- **Border points** — memories between clusters are identified as low-confidence members, which aligns perfectly with soft assignment.

Implementation: the `hdbscan` crate in Rust. Load all embeddings into memory (at 768d × f32 × 10k memories ≈ 30MB — fine), cluster, compute centroids, soft-assign all memories against the centroids.

```rust
use hdbscan::{Center, Hdbscan};

fn discover_domains(
    embeddings: &[Vec<f32>],
    min_cluster_size: usize,
) -> (Vec<Vec<usize>>, Vec<Vec<f32>>) {  // (cluster → member indices, centroids)
    let clusterer = Hdbscan::default(embeddings);
    let labels = clusterer.cluster().unwrap();
    let centroids = clusterer.calc_centers(Center::Centroid, &labels).unwrap();

    // Group indices by label, ignoring noise (-1)
    let mut clusters: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        if label >= 0 {
            clusters.entry(label).or_default().push(i);
        }
    }
    (clusters.into_values().collect(), centroids)
}
```

After HDBSCAN produces hard clusters, the soft-assignment pass (Phase 3) immediately re-scores all memories — including the ones HDBSCAN assigned — against the computed centroids. So HDBSCAN's hard labels are only used to *define* the centroids. The actual domain assignments always come from the continuous similarity scores.

This works identically for both SQLite and Postgres backends — clustering runs in Rust application code, results are written back to the storage layer.

### Network Transport

#### MCP over Streamable HTTP

Extend the existing Axum server:

```rust
// Alongside existing dashboard routes
let app = Router::new()
    // Existing dashboard
    .route("/api/health", get(health_handler))
    .route("/dashboard/*path", get(dashboard_handler))
    // New: MCP over HTTP
    .route("/mcp", post(mcp_handler).get(mcp_sse_handler))
    // New: REST API
    // X-Vestige-Domain header optionally scopes to a domain
    .route("/api/v1/memories", post(create_memory).get(list_memories))
    .route("/api/v1/memories/:id", get(get_memory).put(update_memory).delete(delete_memory))
    .route("/api/v1/search", post(search_memories))
    .route("/api/v1/consolidate", post(trigger_consolidation))
    .route("/api/v1/stats", get(get_stats))
    .route("/api/v1/domains", get(list_domains))
    .route("/api/v1/domains/discover", post(trigger_discovery))
    .route("/api/v1/domains/:id", put(rename_domain).delete(merge_domain))
    // Auth on everything except health
    .layer(middleware::from_fn(api_key_auth));
```

#### Auth Middleware

```rust
async fn api_key_auth(
    State(store): State<Arc<dyn MemoryStore>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    // Skip auth for health endpoint
    if request.uri().path() == "/api/health" {
        return Ok(next.run(request).await);
    }

    let key = request.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| request.headers()
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok()));

    match key {
        Some(k) if verify_api_key(store.as_ref(), k).await => {
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
```

#### Client Configuration

```json
// Claude Desktop / Claude Code — single key, all domains
{
  "mcpServers": {
    "vestige": {
      "url": "http://vestige.local:3927/mcp",
      "headers": {
        "Authorization": "Bearer vst_a1b2c3..."
      }
    }
  }
}
```

No domain header needed — searches all domains by default. The MCP tools include an optional `domain` parameter for scoped queries if the LLM or user wants to narrow down.

Alternatively, scope a connection to a specific domain:

```json
// Domain-scoped connection (e.g., for a home automation agent)
{
  "mcpServers": {
    "vestige-home": {
      "url": "http://vestige.local:3927/mcp",
      "headers": {
        "Authorization": "Bearer vst_e5f6g7...",
        "X-Vestige-Domain": "home"
      }
    }
  }
}
```

### Server Configuration

```toml
# vestige.toml — full example for server mode
[server]
bind = "0.0.0.0:3927"           # or mycelium IPv6 address
# tls_cert = "/path/to/cert.pem"  # optional
# tls_key = "/path/to/key.pem"

[auth]
enabled = true
# If false, no key required (local-only mode)

[storage]
backend = "postgres"

[storage.postgres]
url = "postgresql://vestige:secret@localhost:5432/vestige"
max_connections = 10

[embeddings]
provider = "fastembed"
model = "BAAI/bge-base-en-v1.5"
```

### CLI Extensions

```bash
# Domain management (mostly automatic, but user can inspect/rename)
vestige domains list
# → dev         Development (auto)     memories: 87    top: Rust, trait, async, tokio
# → infra       Infrastructure (auto)  memories: 47    top: BGP, SONiC, VLAN, FRR
# → home        Home (auto)            memories: 31    top: solar, kWh, pool, ESPHome
# → (unclassified)                     memories: 12

vestige domains rename cluster_0 infra --label "Infrastructure"
vestige domains merge home personal --into home
vestige domains discover --force   # re-run HDBSCAN now

# Key management
vestige keys create --label "macbook"
# → Created key: vst_a1b2c3d4... (store this, shown once)

vestige keys create --label "home-assistant" --scopes read --domains home
# → Created key: vst_e5f6g7h8... (read-only, home domain only)

vestige keys list
# → macbook         vst_a1b2...  scopes: [read,write]  domains: [all]
# → home-assistant  vst_e5f6...  scopes: [read]        domains: [home]

vestige keys revoke vst_a1b2c3d4...

# Migration
vestige migrate --from sqlite --to postgres \
    --sqlite-path ~/.vestige/vestige.db \
    --postgres-url postgresql://localhost/vestige
```

## Implementation Plan

### Phase 1: Storage Trait Extraction
- Define the `MemoryStore` trait (including domain methods)
- Refactor current SQLite code to implement it
- Add `domains TEXT[]` column to existing SQLite schema
- Verify all 29 modules work through the trait (no direct SQLite access)
- **No behavioral changes** — all memories start as unclassified

### Phase 2: PostgreSQL Backend
- Implement `PgMemoryStore`
- Schema migrations (sqlx or refinery)
- `vestige migrate` command for SQLite → Postgres
- Config file support for backend selection

### Phase 3: Network Access
- MCP Streamable HTTP endpoint on existing Axum server
- API key auth middleware + CLI management
- REST API endpoints
- Feature flags for stdio vs HTTP mode

### Phase 4: Emergent Domain Classification
- `DomainClassifier` cognitive module
- HDBSCAN clustering via `hdbscan` crate (runs on both backends)
- Soft assignment pass: score all memories against centroids, threshold into domains
- `domain_scores` JSONB stored per memory for transparency / retroactive re-thresholding
- Domain discovery CLI and dashboard UI
- Auto-classification on ingest (once domains exist)
- Re-clustering during dream consolidation passes
- Domain management CLI (rename, merge, inspect)

### Phase 5: Federation (future)
- Node discovery via Mycelium / mDNS
- Memory sync protocol (UUID-based, last-write-wins)
- Possibly Iroh for content-addressed replication
- FSRS state merge (review history append, not overwrite)

## Crate Dependencies (new)

```toml
# Phase 1 — trait abstraction
trait-variant = "0.1"

# Phase 2 — Postgres
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
pgvector = "0.4"  # sqlx integration for vector type

# Phase 3 — Auth
blake3 = "1"      # key hashing
rand = "0.8"      # key generation

# Phase 4 — Domain clustering
hdbscan = "0.10"   # HDBSCAN — no eps tuning, variable density, built-in centroid calc
```

## Open Questions

1. **Trait granularity**: One big `MemoryStore` trait or split into `MemoryStore + SchedulingStore + GraphStore + DomainStore`? Splitting is cleaner but means more `dyn` parameters threading through handlers.

2. **Embedding on insert**: Should the storage backend call fastembed, or should the caller always provide the embedding? Current design says caller provides it, keeping the backend pure storage. But this means every client needs fastembed locally even if the DB is remote. For the server model, having the server compute embeddings makes more sense.

3. **pgvector dimension**: Fixed (e.g., `vector(768)`) or unconstrained (`vector`)? Fixed is faster for HNSW but requires migration if model changes.

4. **Sync conflict resolution for federation**: LWW per-UUID is simple but lossy. CRDTs would be more correct but massively more complex. For FSRS state specifically, merging review event logs would be ideal.

5. **Dashboard auth**: The 3D dashboard currently runs unauthenticated on localhost. With remote access, it needs the same auth. Should it use the same API keys or have a separate session/cookie mechanism?

6. **HDBSCAN `min_cluster_size`**: The main tuning knob. Too small → noisy micro-clusters. Too large → distinct topics get merged. Default of 10 should work for most cases, but may need a manual override or auto-sweep (run with several values, pick the one with best silhouette score).

7. **Domain drift**: Over time, the character of a domain changes. How aggressively should re-clustering reshape existing domains? Conservative (only propose splits/merges, never auto-apply) vs. aggressive (auto-reassign memories whose scores drifted below threshold)?

8. **Spreading activation across domains**: When searching within a single domain, should graph edges that cross into other domains be followed? Probably yes for recall quality, but with decaying weight as you cross boundaries.

9. **Threshold tuning**: The `assign_threshold` (0.65 default) determines how many memories are multi-domain vs single-domain vs unclassified. Too low → everything is multi-domain (useless). Too high → too many unclassified. Could be auto-tuned per dataset by targeting a specific unclassified ratio (e.g., "keep fewer than 10% unclassified").
