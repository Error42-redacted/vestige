# Phase 1 Plan: Storage Trait Extraction

**Status**: Draft
**Depends on**: none
**Related**: docs/adr/0001-pluggable-storage-and-network-access.md (Phase 1)

---

## Scope

### In scope

- Introduce a new module `crates/vestige-core/src/storage/memory_store.rs` defining:
  - `LocalMemoryStore` base trait (Sync + 'static)
  - `MemoryStore` Send-bound alias generated via `#[trait_variant::make(MemoryStore: Send)]`
  - Supporting data types referenced by the trait: `MemoryRecord`, `SchedulingState`, `SearchQuery`, `SearchResult`, `MemoryEdge`, `Domain`, `ClassificationResult`, `StoreStats`, `HealthStatus`, `MemoryStoreError`.
- Introduce a new module `crates/vestige-core/src/embedder/` defining:
  - `Embedder` async trait with `embed`, `model_name`, `dimension` plus `model_hash` (for the registry) and optional `embed_batch` with a default implementation.
  - Move/adapt the existing `EmbeddingService` impl into a new struct `FastembedEmbedder` that implements `Embedder`.
- Refactor `Storage` (existing `crates/vestige-core/src/storage/sqlite.rs`) into `SqliteMemoryStore`:
  - Keep the struct, the `writer`/`reader` `Mutex<Connection>` pair, the `FSRSScheduler`, and the USearch `VectorIndex`.
  - Rename the type alias `Storage` to `SqliteMemoryStore` with a `pub type Storage = SqliteMemoryStore;` alias for backward source compatibility during the transition. (The trait method surface is the new public contract.)
  - Implement `LocalMemoryStore` by wrapping existing synchronous `rusqlite` methods inside `async fn` bodies that call a small `spawn_blocking`-or-inline adapter. Bodies MAY block; the `async fn` signature exists because `LocalMemoryStore` is async.
- Add a `schema_version = 12` migration that introduces two schema additions:
  1. `embedding_model` registry table (one-row constraint enforced in code).
  2. Two new TEXT columns on `knowledge_nodes`: `domains TEXT NOT NULL DEFAULT '[]'` and `domain_scores TEXT NOT NULL DEFAULT '{}'` (both JSON-encoded).
- Enforce model registry on every write path: on the first non-empty embedding write the model signature is recorded; subsequent writes whose `Embedder::model_name()` / `dimension()` / `model_hash()` disagree must fail with `MemoryStoreError::ModelMismatch` before touching the DB.
- Audit all 29 cognitive modules under `crates/vestige-core/src/neuroscience/` and `crates/vestige-core/src/advanced/` to confirm they hold no direct `rusqlite::Connection` references, no `Storage` struct field, and no SQL strings. Any that do get refactored to take `&dyn LocalMemoryStore` (local-only modules) or `&Arc<dyn MemoryStore>` (modules crossing `await` points).
- Add unit tests alongside each new trait method and integration tests in `tests/phase_1/`.

### Out of scope

- Implementing `PgMemoryStore` on sqlx + pgvector -- that is Phase 2.
- `vestige migrate --from sqlite --to postgres` and `vestige migrate --reembed` -- Phase 2.
- MCP over Streamable HTTP, API key middleware, `api_keys` table, `vestige keys create|list|revoke` -- Phase 3.
- `DomainClassifier` module, HDBSCAN clustering, `vestige domains discover|list|rename|merge` CLI, incremental soft-assignment, cross-domain spreading activation decay -- Phase 4.
- Federation, mycelium/mDNS node discovery, review event log table -- Phase 5.
- Removing the `pub type Storage = SqliteMemoryStore;` compatibility alias -- that cleanup happens at the end of Phase 4 when no consumers still spell the old name.

## Prerequisites

### Current code state

- Single concrete type `Storage` in `crates/vestige-core/src/storage/sqlite.rs` (4592 lines, 216 public symbols on the impl blocks, approximately 85 public methods) is the only storage surface the crate exposes.
- `EmbeddingService` in `crates/vestige-core/src/embeddings/local.rs` holds the fastembed singleton. No trait exists; callers type-erase via `&EmbeddingService`.
- Migrations live in `crates/vestige-core/src/storage/migrations.rs`; the current head is v11.
- All cognitive modules in `neuroscience/` and `advanced/` are pure (verified by `grep rusqlite|Connection::|execute\(|prepare\(` returning no matches in those trees). They operate on `KnowledgeNode`, `Vec<f32>`, `ConnectionRecord`, etc. passed in by the caller.
- `vestige-mcp` consumes `Arc<Storage>` in `crates/vestige-mcp/src/server.rs` and every tool under `crates/vestige-mcp/src/tools/`. These call sites will type-check unchanged after the alias is introduced because the trait methods preserve the exact signatures of the existing `pub fn` on `Storage`.
- Test count reported in `CLAUDE.md`: 758 tests (406 mcp + 352 core). This is the no-regression target.

### Required crates (add via `cargo add` under `crates/vestige-core`)

| Crate | Version | Why |
|-------|---------|-----|
| `trait-variant` | `0.1` | Generates the `Send`-bound `MemoryStore` alias from `LocalMemoryStore` so `Arc<dyn MemoryStore>` works under tokio/axum without hand-writing two traits. Listed in PRD section "Crate Dependencies (new)" under Phase 1. |
| `blake3` | `1` | `Embedder::model_hash() -> [u8; 32]` uses blake3 to stabilise the "model signature" stored in the `embedding_model` registry. Already slated for Phase 3 auth; pulling it forward costs nothing and avoids a second migration to add a hash column. |
| `async-trait` | `0.1` | Not strictly required with `trait-variant` on MSRV 1.91 (RPITIT is stable), but used for one utility trait (`EmbedderExt`) that carries a default `embed_batch` body. OPTIONAL; see Open Implementation Questions below. |

No changes to `vestige-mcp/Cargo.toml` are required for Phase 1 -- the new trait lives in `vestige-core` and the mcp crate continues to depend on the `SqliteMemoryStore` concrete type (via the `Storage` alias) until Phase 2 introduces backend selection.

## Deliverables

1. `crates/vestige-core/src/storage/memory_store.rs` -- `LocalMemoryStore` + `MemoryStore` traits and supporting types.
2. `crates/vestige-core/src/storage/mod.rs` -- updated exports and module wiring.
3. `crates/vestige-core/src/storage/sqlite.rs` -- `Storage` renamed to `SqliteMemoryStore`, `impl LocalMemoryStore for SqliteMemoryStore` block, enforcement hooks for the model registry, serde of `domains` / `domain_scores` columns.
4. `crates/vestige-core/src/storage/migrations.rs` -- `MIGRATION_V12_UP` adding `embedding_model` table and `domains`, `domain_scores` columns.
5. `crates/vestige-core/src/embedder/mod.rs` -- `Embedder` trait and re-exports.
6. `crates/vestige-core/src/embedder/fastembed.rs` -- `FastembedEmbedder` implementation.
7. `crates/vestige-core/src/embeddings/local.rs` -- retained; `EmbeddingService` kept as the underlying fastembed holder; `FastembedEmbedder` wraps it.
8. `crates/vestige-core/src/lib.rs` -- new `pub mod embedder;` + re-exports for `MemoryStore`, `LocalMemoryStore`, `Embedder`, `FastembedEmbedder`, and the data types.
9. `tests/phase_1/trait_round_trip.rs` -- integration test: round-trip of every trait method through `SqliteMemoryStore`.
10. `tests/phase_1/embedding_model_registry.rs` -- integration test: first-write registers, mismatch refuses, dimension mismatch refuses.
11. `tests/phase_1/domain_column_migration.rs` -- integration test: a v11 DB upgraded to v12 reads `domains=[]` and `domain_scores={}` for all existing rows.
12. `tests/phase_1/cognitive_module_isolation.rs` -- integration test: every cognitive module compiles and executes against an `Arc<dyn MemoryStore>` without touching `SqliteMemoryStore` concretely.
13. `tests/phase_1/send_bound_variant.rs` -- integration test: an `Arc<dyn MemoryStore>` can be moved across `tokio::spawn`.
14. Updated `tests/phase_1/mod.rs` (if the dir already uses a module layout) or individual `[[test]]` entries in `tests/e2e/Cargo.toml` as needed -- see "Test Plan" for the exact layout.

## Detailed Task Breakdown

### D1. Trait + supporting types (`memory_store.rs`)

- **File**: `crates/vestige-core/src/storage/memory_store.rs` (new).
- **Depends on**: `trait-variant` crate added under vestige-core, `chrono`, `serde_json`, `uuid`, `thiserror` (all already in Cargo.toml).
- **Signatures**:

```rust
//! Backend-agnostic memory store trait.
//!
//! This is the single abstraction every cognitive module sits above. It is
//! intentionally flat: one trait, ~25 methods, no sub-traits.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ----------------------------------------------------------------------------
// ERROR
// ----------------------------------------------------------------------------

/// Error returned by every `LocalMemoryStore` / `MemoryStore` method.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("backend error: {0}")]
    Backend(String),

    #[error(
        "embedding model mismatch: store registered {registered_name} (dim {registered_dim}, \
         hash {registered_hash}), embedder is {actual_name} (dim {actual_dim}, hash {actual_hash})"
    )]
    ModelMismatch {
        registered_name: String,
        registered_dim: usize,
        registered_hash: String,
        actual_name: String,
        actual_dim: usize,
        actual_hash: String,
    },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("initialization error: {0}")]
    Init(String),
}

impl From<crate::storage::StorageError> for MemoryStoreError {
    fn from(e: crate::storage::StorageError) -> Self {
        use crate::storage::StorageError as S;
        match e {
            S::NotFound(s) => MemoryStoreError::NotFound(s),
            S::Database(e) => MemoryStoreError::Backend(e.to_string()),
            S::Io(e) => MemoryStoreError::Backend(e.to_string()),
            S::InvalidTimestamp(s) => MemoryStoreError::Backend(format!("invalid timestamp: {s}")),
            S::Init(s) => MemoryStoreError::Init(s),
        }
    }
}

pub type MemoryStoreResult<T> = std::result::Result<T, MemoryStoreError>;

// ----------------------------------------------------------------------------
// DATA TYPES
// ----------------------------------------------------------------------------

/// Backend-agnostic memory record.
///
/// Phase 1 intentionally keeps this type independent of `KnowledgeNode` to
/// avoid dragging 30+ legacy fields through the trait surface. The SQLite
/// backend converts between `MemoryRecord` and `KnowledgeNode` at the
/// boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: Uuid,
    /// Empty = unclassified. Populated in Phase 4.
    pub domains: Vec<String>,
    /// Raw similarity per domain centroid. Empty until Phase 4 runs clustering.
    pub domain_scores: HashMap<String, f64>,
    pub content: String,
    pub node_type: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// FSRS-6 scheduling state, one row per memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingState {
    pub memory_id: Uuid,
    pub stability: f64,
    pub difficulty: f64,
    pub retrievability: f64,
    pub last_review: Option<DateTime<Utc>>,
    pub next_review: Option<DateTime<Utc>>,
    pub reps: u32,
    pub lapses: u32,
}

/// Hybrid search request.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub domains: Option<Vec<String>>,
    pub text: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tags: Option<Vec<String>>,
    pub node_types: Option<Vec<String>>,
    pub limit: usize,
    pub min_retrievability: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub record: MemoryRecord,
    pub score: f64,
    pub fts_score: Option<f64>,
    pub vector_score: Option<f64>,
}

/// Edge in the spreading-activation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub edge_type: String,
    pub weight: f64,
    pub created_at: DateTime<Utc>,
}

/// A topical domain (populated in Phase 4). Phase 1 only needs the type to
/// shape the trait surface; discover/classify are Phase 4 work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub id: String,
    pub label: String,
    pub centroid: Vec<f32>,
    pub top_terms: Vec<String>,
    pub memory_count: usize,
    pub created_at: DateTime<Utc>,
}

/// Result of classifying one vector against all known domains.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub scores: HashMap<String, f64>,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreStats {
    pub total_memories: usize,
    pub memories_with_embeddings: usize,
    pub total_edges: usize,
    pub total_domains: usize,
    pub registered_model_name: Option<String>,
    pub registered_model_dim: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unavailable { reason: String },
}

// ----------------------------------------------------------------------------
// EMBEDDING MODEL SIGNATURE
// ----------------------------------------------------------------------------

/// Snapshot of the embedding model that was used to write vectors into the
/// store. Persisted in the `embedding_model` table; compared on every write
/// before the vector is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSignature {
    pub name: String,
    pub dimension: usize,
    /// Lowercase hex-encoded blake3 hash, 64 chars.
    pub hash: String,
}

// ----------------------------------------------------------------------------
// TRAIT
// ----------------------------------------------------------------------------

/// The single storage abstraction. `trait_variant::make` auto-generates a
/// `MemoryStore` alias with `Send`-bound return futures so `Arc<dyn MemoryStore>`
/// works in tokio/axum contexts.
#[trait_variant::make(MemoryStore: Send)]
pub trait LocalMemoryStore: Sync + 'static {
    // --- Lifecycle ---
    async fn init(&self) -> MemoryStoreResult<()>;
    async fn health_check(&self) -> MemoryStoreResult<HealthStatus>;

    // --- Embedding model registry ---
    async fn registered_model(&self) -> MemoryStoreResult<Option<ModelSignature>>;
    async fn register_model(&self, sig: &ModelSignature) -> MemoryStoreResult<()>;

    // --- CRUD ---
    async fn insert(&self, record: &MemoryRecord) -> MemoryStoreResult<Uuid>;
    async fn get(&self, id: Uuid) -> MemoryStoreResult<Option<MemoryRecord>>;
    async fn update(&self, record: &MemoryRecord) -> MemoryStoreResult<()>;
    async fn delete(&self, id: Uuid) -> MemoryStoreResult<()>;

    // --- Search ---
    async fn search(&self, query: &SearchQuery) -> MemoryStoreResult<Vec<SearchResult>>;
    async fn fts_search(&self, text: &str, limit: usize) -> MemoryStoreResult<Vec<SearchResult>>;
    async fn vector_search(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> MemoryStoreResult<Vec<SearchResult>>;

    // --- FSRS Scheduling ---
    async fn get_scheduling(
        &self,
        memory_id: Uuid,
    ) -> MemoryStoreResult<Option<SchedulingState>>;
    async fn update_scheduling(&self, state: &SchedulingState) -> MemoryStoreResult<()>;
    async fn get_due_memories(
        &self,
        before: DateTime<Utc>,
        limit: usize,
    ) -> MemoryStoreResult<Vec<(MemoryRecord, SchedulingState)>>;

    // --- Graph (spreading activation) ---
    async fn add_edge(&self, edge: &MemoryEdge) -> MemoryStoreResult<()>;
    async fn get_edges(
        &self,
        node_id: Uuid,
        edge_type: Option<&str>,
    ) -> MemoryStoreResult<Vec<MemoryEdge>>;
    async fn remove_edge(&self, source: Uuid, target: Uuid) -> MemoryStoreResult<()>;
    async fn get_neighbors(
        &self,
        node_id: Uuid,
        depth: usize,
    ) -> MemoryStoreResult<Vec<(MemoryRecord, f64)>>;

    // --- Domains (Phase 1: stubs return empty; full impl in Phase 4) ---
    async fn list_domains(&self) -> MemoryStoreResult<Vec<Domain>>;
    async fn get_domain(&self, id: &str) -> MemoryStoreResult<Option<Domain>>;
    async fn upsert_domain(&self, domain: &Domain) -> MemoryStoreResult<()>;
    async fn delete_domain(&self, id: &str) -> MemoryStoreResult<()>;
    /// Phase 1: returns `Ok(vec![])` since no centroids exist. Phase 4 wires
    /// the full soft-assignment pass.
    async fn classify(&self, embedding: &[f32]) -> MemoryStoreResult<Vec<(String, f64)>>;

    // --- Bulk / Maintenance ---
    async fn count(&self) -> MemoryStoreResult<usize>;
    async fn get_stats(&self) -> MemoryStoreResult<StoreStats>;
    async fn vacuum(&self) -> MemoryStoreResult<()>;
}
```

- **Behavior notes**:
  - Every method returns `MemoryStoreResult<T>`; the trait never exposes `rusqlite::Error`.
  - `LocalMemoryStore` requires `Sync + 'static` so `Arc<dyn LocalMemoryStore>` is usable. The auto-generated `MemoryStore` alias adds `Send` bounds on the returned `impl Future`.
  - `register_model` is idempotent: writing the same signature twice is `Ok(())`. Writing a different signature after one is registered returns `MemoryStoreError::ModelMismatch`.
  - `classify` on Phase 1 returns `Ok(vec![])` and MUST NOT error; cognitive modules call it and Phase 4 will flesh it out without changing the signature.
  - `upsert_domain` / `delete_domain` / `list_domains` / `get_domain` operate against a `domains` table that is empty until Phase 4 populates it. Phase 1 still exposes the methods so Phase 2 can implement them against Postgres in one shot.
  - `get_neighbors(node_id, depth)` with `depth == 0` returns just `(node, 1.0)` if the node exists, otherwise `NotFound`. `depth > 0` performs breadth-first expansion over edges, weight = product of edge weights along the shortest path discovered, capped at `max_neighbors = 256` to prevent runaway expansion.

---

### D2. Storage module wiring (`storage/mod.rs`)

- **File**: `crates/vestige-core/src/storage/mod.rs`.
- **Depends on**: D1.
- **Signatures / diff**:

```rust
//! Storage Module
//!
//! Backend-agnostic memory store abstraction plus SQLite reference impl.

mod memory_store;
mod migrations;
mod sqlite;

pub use memory_store::{
    ClassificationResult, Domain, HealthStatus, LocalMemoryStore, MemoryEdge, MemoryRecord,
    MemoryStore, MemoryStoreError, MemoryStoreResult, ModelSignature, SchedulingState,
    SearchQuery, SearchResult, StoreStats,
};
pub use migrations::MIGRATIONS;
pub use sqlite::{
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord, InsightRecord,
    IntentionRecord, Result, SmartIngestResult, SqliteMemoryStore, StateTransitionRecord,
    StorageError,
};

/// Backwards-compatibility alias. Retained until Phase 4 completes so every
/// existing `Arc<Storage>` call site keeps compiling. Scheduled for removal
/// once no downstream source file references it.
pub type Storage = SqliteMemoryStore;
```

- **Behavior notes**:
  - The alias MUST be a `pub type` (not a re-export), because several tool files pattern on `vestige_core::Storage` through `use` statements and we want to keep them compiling verbatim. This has zero runtime cost.
  - `StorageError` stays exported for the 29 existing inherent-method callers; the trait exposes `MemoryStoreError` and provides `From<StorageError>`.

---

### D3. Rename + trait impl in `sqlite.rs`

- **File**: `crates/vestige-core/src/storage/sqlite.rs`.
- **Depends on**: D1, D2, D4 (for schema columns), D5/D6 (to have `Embedder` to accept on `insert`).
- **Signatures (key excerpts)**:

```rust
pub struct SqliteMemoryStore {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
    scheduler: Mutex<FSRSScheduler>,
    #[cfg(feature = "embeddings")]
    embedding_service: EmbeddingService,
    #[cfg(feature = "vector-search")]
    vector_index: Mutex<VectorIndex>,
    #[cfg(feature = "embeddings")]
    query_cache: Mutex<LruCache<String, Vec<f32>>>,
    /// Cached model signature. `None` until the first embedding is written.
    registered_model: std::sync::RwLock<Option<ModelSignature>>,
}

impl SqliteMemoryStore {
    pub fn new(db_path: Option<std::path::PathBuf>) -> MemoryStoreResult<Self> { /* existing body, Result converted */ }

    /// Internal: convert a row into a `MemoryRecord` (new mapping reading
    /// `domains` / `domain_scores` JSON columns).
    fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<MemoryRecord> { /* ... */ }

    /// Internal: given a `MemoryRecord` plus an optional embedding, enforce
    /// the registered model signature and return a `MemoryStoreError` if
    /// the embedder would produce a mismatched vector.
    fn enforce_model(
        &self,
        incoming: Option<&ModelSignature>,
    ) -> MemoryStoreResult<()> { /* ... */ }
}

impl crate::storage::memory_store::LocalMemoryStore for SqliteMemoryStore {
    async fn init(&self) -> MemoryStoreResult<()> { /* no-op; migrations run in `new` */ Ok(()) }

    async fn health_check(&self) -> MemoryStoreResult<HealthStatus> {
        // SELECT 1; check vector index loaded; check embedding_model presence.
    }

    async fn registered_model(&self) -> MemoryStoreResult<Option<ModelSignature>> {
        let cached = self.registered_model.read().map_err(|_| MemoryStoreError::Init("registered_model rwlock poisoned".into()))?.clone();
        if cached.is_some() {
            return Ok(cached);
        }
        // Fall through to DB read...
    }

    async fn register_model(&self, sig: &ModelSignature) -> MemoryStoreResult<()> {
        // INSERT OR IGNORE; if a row exists and differs, return ModelMismatch.
    }

    async fn insert(&self, record: &MemoryRecord) -> MemoryStoreResult<Uuid> {
        if let Some(vec) = &record.embedding {
            // Caller is REQUIRED to have called register_model first (or the
            // store auto-registers on the first embedded write -- see
            // "embedding_model_registry.rs" test).
            let derived = ModelSignature { /* from cache or from record.metadata */ };
            self.enforce_model(Some(&derived))?;
            if vec.len() != derived.dimension {
                return Err(MemoryStoreError::InvalidInput(
                    format!("embedding length {} != registered dimension {}", vec.len(), derived.dimension),
                ));
            }
        }
        // Delegate to a private `insert_record_blocking` helper that is the
        // current `ingest`/`update_node_content` body, rewritten to accept a
        // `MemoryRecord` and to also write `domains` / `domain_scores` JSON.
    }

    // ... remaining ~24 methods follow the same pattern: convert inputs,
    // call the existing synchronous body, convert outputs.
}
```

- **SQL** (covered in full in D4 below).
- **Behavior notes**:
  - The `async fn` bodies are allowed to be synchronous under the hood (rusqlite is blocking). We do NOT wrap in `spawn_blocking` for Phase 1 -- the current `Storage` is already used from synchronous code paths (CLI, MCP stdio handler) and forcing the tokio runtime is a Phase 2 concern when we also add sqlx. The trait simply lifts the synchronous body into an `async fn` so the signatures match the trait. MSRV 1.91 supports async fn in trait via `trait_variant::make`.
  - `insert` preserves the current FSRS initialization logic (stability, difficulty, next_review, etc.) -- the new code path converts `MemoryRecord.metadata` back into `IngestInput`-equivalent fields when needed. All existing inherent methods (`ingest`, `smart_ingest`, `mark_reviewed`, ...) remain on `SqliteMemoryStore` untouched; the trait impl calls into them.
  - `registered_model` cache is an `RwLock<Option<ModelSignature>>`. Invalidated on schema reset. Never mutated after first population until an explicit `--reembed` migration (Phase 2) takes the RwLock exclusively and writes a new row.
  - `enforce_model` returns `Ok(())` if no model is registered yet AND `incoming.is_none()` (no-embedding write). Returns `Ok(())` if no model is registered and `incoming.is_some()` after calling `register_model`. Returns `Err(ModelMismatch)` if registered and they disagree.
  - `domains` / `domain_scores` serialization uses `serde_json::to_string` on write and `serde_json::from_str` on read. Empty vec -> `"[]"`, empty map -> `"{}"`. `NULL` in the DB is treated as the empty value for pre-migration rows.
  - Every existing inherent method is kept verbatim. The trait impl dispatches to them. This is the "no behavior change" guarantee.

---

### D4. Schema migration V12

- **File**: `crates/vestige-core/src/storage/migrations.rs`.
- **Depends on**: D2.
- **SQL**:

```sql
-- Migration V12: embedding model registry + per-memory domain columns.

-- 1. Embedding model registry. Single logical row; the (id = 1) constraint is
--    enforced in code via `register_model` (SQLite CHECK on a single-row
--    table is uglier than a constraint we already enforce in Rust).
CREATE TABLE IF NOT EXISTS embedding_model (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    name         TEXT    NOT NULL,
    dimension    INTEGER NOT NULL,
    hash         TEXT    NOT NULL,       -- lowercase hex blake3
    created_at   TEXT    NOT NULL
);

-- 2. Per-memory domain columns (JSON TEXT; SQLite has no native arrays).
ALTER TABLE knowledge_nodes ADD COLUMN domains       TEXT NOT NULL DEFAULT '[]';
ALTER TABLE knowledge_nodes ADD COLUMN domain_scores TEXT NOT NULL DEFAULT '{}';

-- 3. Index on the domains JSON column to enable `LIKE '%"dev"%'`-style
--    filter in Phase 4. Kept lightweight here; Postgres will use GIN.
CREATE INDEX IF NOT EXISTS idx_nodes_domains        ON knowledge_nodes(domains);
CREATE INDEX IF NOT EXISTS idx_nodes_domain_scores  ON knowledge_nodes(domain_scores);

-- 4. Domains catalogue (empty until Phase 4 populates).
CREATE TABLE IF NOT EXISTS domains (
    id           TEXT    PRIMARY KEY,
    label        TEXT    NOT NULL,
    centroid     BLOB,                    -- f32 vector, raw bytes
    top_terms    TEXT    NOT NULL DEFAULT '[]',
    memory_count INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_domains_created_at ON domains(created_at);

UPDATE schema_version SET version = 12, applied_at = datetime('now');
```

- **Rust changes** to `migrations.rs`:

```rust
pub const MIGRATIONS: &[Migration] = &[
    // ... V1..V11 unchanged ...
    Migration {
        version: 12,
        description: "Phase 1: embedding_model registry, domains/domain_scores columns, domains table",
        up: MIGRATION_V12_UP,
    },
];

const MIGRATION_V12_UP: &str = r#"...SQL above..."#;
```

- **Behavior notes**:
  - Idempotent: `ALTER TABLE ... ADD COLUMN` on SQLite is not idempotent by default, but the `apply_migrations` driver only applies migrations whose version > current. A user who has already applied V12 never sees the SQL again.
  - The `CHECK (id = 1)` on `embedding_model` is the only one-row guardrail -- all inserts go through `register_model` which uses `INSERT OR IGNORE INTO embedding_model (id, ...) VALUES (1, ...)` followed by a `SELECT` to detect mismatch.
  - `centroid BLOB` stores the f32 vector using the same `Embedding::to_bytes()` format used in `node_embeddings`, for consistency.

---

### D5. Embedder trait (`embedder/mod.rs`)

- **File**: `crates/vestige-core/src/embedder/mod.rs` (new).
- **Depends on**: `blake3` crate added to vestige-core.
- **Signatures**:

```rust
//! Text-to-vector encoding trait. Pluggable per-install.

use std::fmt::Debug;

mod fastembed;

pub use fastembed::FastembedEmbedder;

/// Error returned by every `Embedder` method.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum EmbedderError {
    #[error("embedder initialization failed: {0}")]
    Init(String),
    #[error("embedding generation failed: {0}")]
    EmbedFailed(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub type EmbedderResult<T> = std::result::Result<T, EmbedderError>;

/// Pluggable embedder. The storage layer NEVER calls fastembed directly;
/// callers compute vectors via this trait and pass them into `MemoryStore`.
#[trait_variant::make(Embedder: Send)]
pub trait LocalEmbedder: Sync + 'static {
    async fn embed(&self, text: &str) -> EmbedderResult<Vec<f32>>;

    fn model_name(&self) -> &str;

    fn dimension(&self) -> usize;

    /// Stable blake3 hash of (model_name || dimension || optional weights
    /// digest if available). Lowercase hex, 64 chars.
    ///
    /// Used by `MemoryStore::register_model` to detect silent model drift
    /// (e.g. a fastembed minor upgrade that changes vector output).
    fn model_hash(&self) -> String;

    async fn embed_batch(&self, texts: &[&str]) -> EmbedderResult<Vec<Vec<f32>>> {
        // Default: sequential. Backends with native batching override this.
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }

    /// Returns the `ModelSignature` describing this embedder. Convenience
    /// wrapper over the three accessors above.
    fn signature(&self) -> crate::storage::ModelSignature {
        crate::storage::ModelSignature {
            name: self.model_name().to_string(),
            dimension: self.dimension(),
            hash: self.model_hash(),
        }
    }
}
```

- **Behavior notes**:
  - The `embed_batch` default implementation is non-trivial only in that backends with genuine batching override it. The `FastembedEmbedder` overrides to call `EmbeddingService::embed_batch`.
  - `model_hash()` is intentionally a function, not a constant, so backends with configurable weights (a future `OnnxEmbedder` that loads an arbitrary file) can hash the file bytes into the signature.
  - `Embedder` (the `Send` variant) is what cognitive modules bind against when they hold `Arc<dyn Embedder>`. `LocalEmbedder` is available for single-threaded callers (CLI, tests).

---

### D6. FastembedEmbedder impl (`embedder/fastembed.rs`)

- **File**: `crates/vestige-core/src/embedder/fastembed.rs` (new).
- **Depends on**: D5, existing `crate::embeddings::local::EmbeddingService`.
- **Signatures**:

```rust
use super::{EmbedderError, EmbedderResult, LocalEmbedder};
use crate::embeddings::{EMBEDDING_DIMENSIONS, EmbeddingService, matryoshka_truncate};

pub struct FastembedEmbedder {
    inner: EmbeddingService,
    cached_hash: std::sync::OnceLock<String>,
}

impl FastembedEmbedder {
    pub fn new() -> Self {
        Self {
            inner: EmbeddingService::new(),
            cached_hash: std::sync::OnceLock::new(),
        }
    }

    fn compute_hash(name: &str, dim: usize) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(name.as_bytes());
        hasher.update(&(dim as u64).to_le_bytes());
        // fastembed's ONNX bytes are not directly accessible at runtime; we
        // use `(name, dim, static fastembed crate version)` as the
        // signature. If fastembed ever changes its output deterministically
        // between minor versions, bumping the crate version triggers a
        // mismatch -- which is exactly the drift we want to detect.
        hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

impl Default for FastembedEmbedder {
    fn default() -> Self { Self::new() }
}

impl LocalEmbedder for FastembedEmbedder {
    async fn embed(&self, text: &str) -> EmbedderResult<Vec<f32>> {
        let emb = self
            .inner
            .embed(text)
            .map_err(|e| EmbedderError::EmbedFailed(e.to_string()))?;
        Ok(emb.vector)
    }

    fn model_name(&self) -> &str { self.inner.model_name() }

    fn dimension(&self) -> usize { EMBEDDING_DIMENSIONS }

    fn model_hash(&self) -> String {
        self.cached_hash
            .get_or_init(|| Self::compute_hash(self.inner.model_name(), EMBEDDING_DIMENSIONS))
            .clone()
    }

    async fn embed_batch(&self, texts: &[&str]) -> EmbedderResult<Vec<Vec<f32>>> {
        let embs = self
            .inner
            .embed_batch(texts)
            .map_err(|e| EmbedderError::EmbedFailed(e.to_string()))?;
        Ok(embs.into_iter().map(|e| e.vector).collect())
    }
}
```

- **Behavior notes**:
  - `EmbeddingService` is kept as the fastembed singleton holder; `FastembedEmbedder` is a thin trait adapter. Existing callers of `EmbeddingService` continue to work during the transition.
  - `model_hash` is deterministic for a given `(model_name, EMBEDDING_DIMENSIONS, vestige-core version)` triple. This is the drift detector the ADR calls out under "Risks: Embedding model drift".
  - `matryoshka_truncate` is already applied inside `EmbeddingService::embed`, so the vectors returned here are the 256-dim Matryoshka-truncated L2-normalized vectors that the rest of the stack expects.

---

### D7. `lib.rs` re-exports

- **File**: `crates/vestige-core/src/lib.rs`.
- **Depends on**: D1, D2, D5, D6.
- **Diff** (inserted alongside the existing `pub mod storage;` re-exports):

```rust
pub mod embedder;

pub use embedder::{Embedder, EmbedderError, EmbedderResult, FastembedEmbedder, LocalEmbedder};

pub use storage::{
    ClassificationResult, Domain, HealthStatus, LocalMemoryStore, MemoryEdge, MemoryRecord,
    MemoryStore, MemoryStoreError, MemoryStoreResult, ModelSignature, SchedulingState,
    SearchQuery, SearchResult, SqliteMemoryStore, Storage, StoreStats,
    // Existing re-exports retained:
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord, InsightRecord,
    IntentionRecord, Result, SmartIngestResult, StateTransitionRecord, StorageError,
};
```

- **Behavior notes**:
  - `Storage` remains a top-level re-export so `use vestige_core::Storage;` keeps working in `vestige-mcp` without changes. Post-Phase-4 cleanup will grep the downstream crates and replace.

---

### D8. Cognitive module audit

- **Files**: all under `crates/vestige-core/src/neuroscience/*.rs` and `crates/vestige-core/src/advanced/*.rs` -- 21 source files.
- **Depends on**: D1..D7.
- **Work**: perform the following grep-gate BEFORE and AFTER the refactor:

```
Grep pattern: "rusqlite|Connection::|execute\\(|prepare\\(|&Storage|SqliteMemoryStore"
Expected in neuroscience/ and advanced/ BEFORE: only a single comment-only hit in `neuroscience/active_forgetting.rs:54` referencing `Storage::suppress_memory` in a doc comment.
Expected AFTER: zero hits that reference `SqliteMemoryStore` concretely. References through `&dyn LocalMemoryStore` or `&Arc<dyn MemoryStore>` are acceptable.
```

- **Behavior notes**:
  - Current state: the 29 cognitive modules are already pure (they take nodes/vectors/connections as arguments, not a `&Storage`). No refactor is required for their bodies.
  - The only work is the `consolidation/sleep.rs` and `consolidation/phases.rs` path, which in the current codebase accepts `&Storage`. These get rewritten to accept `&dyn LocalMemoryStore` (callable from sync contexts) or `&Arc<dyn MemoryStore>` (callable from async contexts). See file inventory below.
  - Actual rewrites (expected number): 3-5 functions across `consolidation/sleep.rs` and `consolidation/mod.rs`. All trait-object refactors; no logic changes.
  - `cognitive.rs` in `vestige-mcp` uses `storage.get_all_connections()`. Because `SqliteMemoryStore` keeps `get_all_connections` as an inherent method AND implements `MemoryStore::get_edges`, both call styles keep compiling. `cognitive.rs` does not need to change in Phase 1.

---

### D9. Backwards-compatible inherent methods on `SqliteMemoryStore`

- **File**: `crates/vestige-core/src/storage/sqlite.rs`.
- **Depends on**: D3.
- **Behavior notes**:
  - Every one of the 85 existing `pub fn` on `Storage` (e.g. `ingest`, `smart_ingest`, `mark_reviewed`, `hybrid_search_filtered`, `save_intention`, `save_insight`, `save_connection`, `apply_rac1_cascade`, ...) stays as an inherent method on `SqliteMemoryStore`. The Phase 1 refactor ONLY adds the trait impl; it does NOT remove any method, rename any field, or change any SQL.
  - Internal writes that previously embedded `INSERT INTO knowledge_nodes (...)` statements gain two more columns (`domains = '[]'`, `domain_scores = '{}'`) in the INSERT list. These are non-optional columns after migration V12, and their DEFAULT is `'[]'`/`'{}'` respectively, so ALTER behaves correctly for pre-existing rows but INSERT statements need to either list the defaults explicitly or rely on the DB default. Plan: explicitly write `'[]'` and `'{}'` in every `INSERT INTO knowledge_nodes` statement to avoid surprises if a future migration drops the DEFAULT.

---

## Test Plan

### Unit tests (colocated, `#[cfg(test)] mod tests` at end of each source file)

Every public trait method on `LocalMemoryStore` gets at least one unit test, exercised through the `SqliteMemoryStore` impl. The unit test file is `crates/vestige-core/src/storage/sqlite.rs` (inside the existing `mod tests`).

- `vestige_core::storage::sqlite::tests::trait_init_is_idempotent` -- calling `LocalMemoryStore::init` twice returns `Ok(())` both times.
- `vestige_core::storage::sqlite::tests::trait_health_check_reports_healthy_on_fresh_db` -- asserts `HealthStatus::Healthy` on a fresh in-memory DB.
- `vestige_core::storage::sqlite::tests::trait_register_model_first_write_succeeds` -- after registering a signature, `registered_model()` returns it.
- `vestige_core::storage::sqlite::tests::trait_register_model_mismatched_write_refused` -- registering a second, different signature returns `MemoryStoreError::ModelMismatch`.
- `vestige_core::storage::sqlite::tests::trait_register_model_same_signature_idempotent` -- registering the same signature twice returns `Ok(())` both times.
- `vestige_core::storage::sqlite::tests::trait_insert_returns_uuid` -- `insert(record)` returns the UUID from the record.
- `vestige_core::storage::sqlite::tests::trait_insert_refuses_dimension_mismatch` -- inserting a record with a 512-dim vector into a store registered for 256 dims returns `MemoryStoreError::InvalidInput`.
- `vestige_core::storage::sqlite::tests::trait_get_missing_returns_none` -- `get(non_existent_uuid)` returns `Ok(None)`.
- `vestige_core::storage::sqlite::tests::trait_get_after_insert_round_trip` -- insert then get returns a record equal (by content/tags/type) to the input; `domains == []`, `domain_scores == {}`.
- `vestige_core::storage::sqlite::tests::trait_update_modifies_content` -- update with new content reflects in subsequent `get`.
- `vestige_core::storage::sqlite::tests::trait_delete_removes_record` -- `delete` then `get` returns `Ok(None)`.
- `vestige_core::storage::sqlite::tests::trait_search_combines_fts_and_vector` -- with one memory whose content matches by FTS and another by vector, `search` returns both, higher score for the exact content match.
- `vestige_core::storage::sqlite::tests::trait_fts_search_returns_tokens_match` -- verifies FTS path.
- `vestige_core::storage::sqlite::tests::trait_vector_search_returns_cosine_order` -- verifies ordering.
- `vestige_core::storage::sqlite::tests::trait_scheduling_round_trip` -- `update_scheduling` then `get_scheduling` returns equivalent state.
- `vestige_core::storage::sqlite::tests::trait_get_scheduling_missing_returns_none`.
- `vestige_core::storage::sqlite::tests::trait_get_due_memories_returns_in_order` -- inserts 3 records with different `next_review`, asserts older-due listed first.
- `vestige_core::storage::sqlite::tests::trait_add_edge_is_idempotent` -- adding the same edge twice does not duplicate.
- `vestige_core::storage::sqlite::tests::trait_get_edges_filters_by_type`.
- `vestige_core::storage::sqlite::tests::trait_remove_edge_deletes_single`.
- `vestige_core::storage::sqlite::tests::trait_get_neighbors_bfs_depth_zero_returns_self_only`.
- `vestige_core::storage::sqlite::tests::trait_get_neighbors_bfs_depth_two_expands` -- build A->B->C, get_neighbors(A, 2) returns {A, B, C}.
- `vestige_core::storage::sqlite::tests::trait_list_domains_empty_in_phase_1` -- Phase 1 has no clustering, so `list_domains()` returns `[]`.
- `vestige_core::storage::sqlite::tests::trait_upsert_then_get_domain_round_trip`.
- `vestige_core::storage::sqlite::tests::trait_delete_domain_idempotent`.
- `vestige_core::storage::sqlite::tests::trait_classify_with_no_domains_returns_empty` -- verifies Phase 1 stub behavior.
- `vestige_core::storage::sqlite::tests::trait_count_matches_insert_count`.
- `vestige_core::storage::sqlite::tests::trait_get_stats_reports_registered_model`.
- `vestige_core::storage::sqlite::tests::trait_vacuum_succeeds` -- runs and asserts no error.

Every public method on `LocalEmbedder` gets at least one unit test under `crates/vestige-core/src/embedder/fastembed.rs`:

- `vestige_core::embedder::fastembed::tests::embedder_reports_correct_name` -- `model_name()` contains "nomic".
- `vestige_core::embedder::fastembed::tests::embedder_reports_256_dimension`.
- `vestige_core::embedder::fastembed::tests::embedder_hash_is_stable` -- `model_hash()` called twice returns identical string.
- `vestige_core::embedder::fastembed::tests::embedder_hash_includes_crate_version` -- a synthetic test that asserts the hash contains the blake3 of `(name, 256, VERSION)`.
- `vestige_core::embedder::fastembed::tests::embedder_embed_smoke` -- gated on `#[cfg(feature = "embeddings")]`; asserts output length == 256.
- `vestige_core::embedder::fastembed::tests::embedder_embed_batch_matches_sequential` -- gated; assert batch result equals sequential result.
- `vestige_core::embedder::fastembed::tests::embedder_signature_matches_accessors`.

Migration V12 unit tests under `crates/vestige-core/src/storage/migrations.rs`:

- `vestige_core::storage::migrations::tests::v12_adds_embedding_model_table` -- apply V12 then assert `SELECT count(*) FROM sqlite_master WHERE name='embedding_model'` == 1.
- `vestige_core::storage::migrations::tests::v12_adds_domains_columns` -- assert `PRAGMA table_info(knowledge_nodes)` includes `domains` and `domain_scores`.
- `vestige_core::storage::migrations::tests::v12_default_values_empty_json` -- insert a row via raw SQL, read back, assert `domains == '[]'` and `domain_scores == '{}'`.
- `vestige_core::storage::migrations::tests::v12_is_replayable` -- rewind `schema_version` to 11, re-apply migrations, does not error (MUST use `CREATE TABLE IF NOT EXISTS`; `ALTER TABLE ADD COLUMN` will be skipped because the driver only re-runs migrations whose version > current -- already covered by `apply_migrations`).
- `vestige_core::storage::migrations::tests::v12_preserves_existing_rows` -- insert rows under V11 schema, upgrade to V12, assert `domains='[]'` on those rows.

Supporting-type unit tests under `crates/vestige-core/src/storage/memory_store.rs`:

- `vestige_core::storage::memory_store::tests::memory_store_error_from_storage_error` -- converts `StorageError::NotFound` to `MemoryStoreError::NotFound`.
- `vestige_core::storage::memory_store::tests::model_signature_serde_round_trip`.
- `vestige_core::storage::memory_store::tests::memory_record_serde_round_trip`.

### Integration tests (`tests/phase_1/`)

Each file is a standalone `[[test]]` target. The Cargo layout:

- `tests/phase_1/Cargo.toml` with:

```toml
[package]
name = "vestige-phase-1-tests"
version = "0.0.1"
edition = "2024"
publish = false

[dependencies]
vestige-core = { path = "../../crates/vestige-core" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tempfile = "3"
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
serde_json = "1"
rusqlite = { version = "0.38", features = ["bundled"] }
```

And added to the workspace `Cargo.toml` members. Each `.rs` file below is a `#[tokio::test]`-using integration test.

#### `tests/phase_1/trait_round_trip.rs`

- `round_trip::insert_get_update_delete` -- exercises CRUD via the trait. Inserts a record with `domains=[]`, gets it, asserts equality, updates content, deletes, asserts not found.
- `round_trip::scheduling_upsert_and_due_scan` -- upserts FSRS state for three memories with different `next_review`, calls `get_due_memories(Utc::now(), 10)`, asserts only past-due ones appear.
- `round_trip::edge_crud` -- add edge, list edges, remove edge, assert gone.
- `round_trip::search_hybrid_returns_results` -- insert three memories, embed one by content match only, one by semantic only, one by both, search with both `text` and `embedding`, assert all three appear with `fts_score`/`vector_score` correctly populated.
- `round_trip::count_and_stats_track_inserts` -- after 10 inserts, `count()` == 10 and `get_stats().total_memories` == 10.
- `round_trip::vacuum_after_deletes_reclaims` -- insert 50, delete 40, call `vacuum`, assert disk file size decreased (informational; test is lenient if VACUUM was a no-op).
- `round_trip::list_domains_empty_then_upsert_then_delete` -- Phase 1 has no discovery, but manual upsert/delete must work so Phase 2's Postgres impl can share the test.
- `round_trip::classify_with_no_domains_returns_empty` -- calls `classify(embedding)` on a fresh store, asserts `Vec<(String, f64)>` is empty.

#### `tests/phase_1/embedding_model_registry.rs`

- `model_registry::first_embedded_insert_auto_registers` -- fresh store; insert a record with a 256-dim vector using a `FastembedEmbedder`; subsequent `registered_model()` returns a `Some(ModelSignature)` with dim=256.
- `model_registry::second_insert_with_same_signature_succeeds`.
- `model_registry::second_insert_with_different_dimension_refused` -- register a 256-dim signature, try to insert a 512-dim vector, expect `MemoryStoreError::InvalidInput` (because dimension does not match registered).
- `model_registry::second_insert_with_different_model_name_refused` -- register signature A, call `register_model` with signature B (same dim, different name), expect `MemoryStoreError::ModelMismatch`.
- `model_registry::second_insert_with_different_hash_refused` -- register signature A, try to register signature A' with the same name and dim but a different hash, expect `MemoryStoreError::ModelMismatch`.
- `model_registry::no_embedding_insert_allowed_before_registration` -- a plain text memory without an embedding must insert successfully even when `registered_model()` is `None`.
- `model_registry::stats_reports_registered_model_after_first_write`.

#### `tests/phase_1/domain_column_migration.rs`

- `domain_columns::fresh_db_has_v12_schema` -- open a fresh store, query `PRAGMA table_info(knowledge_nodes)`, assert `domains` and `domain_scores` columns are present with the correct defaults.
- `domain_columns::v11_db_upgrades_cleanly` -- programmatically create a DB at V11 by running migrations up to V11 only, insert 5 rows, then invoke the V12 migration, assert all 5 rows now report `domains=='[]'` and `domain_scores=='{}'`.
- `domain_columns::empty_domains_serialize_as_brackets` -- insert a `MemoryRecord { domains: vec![], .. }`, then read the underlying SQLite row via a raw query, assert the stored value is `"[]"`, not `NULL`.
- `domain_columns::populated_domains_round_trip` -- insert a record with `domains=["dev","infra"]` and `domain_scores={"dev":0.82,"infra":0.71}`, read back via the trait, assert equality.
- `domain_columns::domains_table_exists` -- `SELECT name FROM sqlite_master WHERE name='domains'` returns one row.

#### `tests/phase_1/cognitive_module_isolation.rs`

- `cognitive_isolation::all_modules_compile_against_dyn_store` -- a test function that allocates a `let store: Arc<dyn MemoryStore> = Arc::new(SqliteMemoryStore::new(...)?);`, then invokes a representative method from every cognitive module passing in records/vectors/edges it reads through the trait. The point is a compile-time gate: if any module still typed against `SqliteMemoryStore`, this would fail to compile.
- `cognitive_isolation::spreading_activation_traverses_via_trait` -- exercise `ActivationNetwork` seeded from `store.get_edges(...)` results.
- `cognitive_isolation::synaptic_tagging_consumes_records_via_trait` -- build `CapturedMemory` from `store.get(uuid)` and let the tagger compute retroactive importance.
- `cognitive_isolation::hippocampal_index_built_from_store` -- load memories via `store.fts_search`, build `HippocampalIndex`, assert queries against the index work.

#### `tests/phase_1/send_bound_variant.rs`

- `send_bound::arc_dyn_memory_store_moves_across_tokio_tasks` -- wrap `SqliteMemoryStore` in `Arc<dyn MemoryStore>`, spawn 16 tokio tasks each inserting 10 memories, join all tasks, assert final `count() == 160`. This verifies the `#[trait_variant::make(MemoryStore: Send)]` emission actually produces a `Send`-bound future.
- `send_bound::concurrent_readers_one_writer` -- 32 concurrent readers calling `search` while one writer loops inserting; asserts no panics, no deadlocks, eventual consistency on `count`.

#### `tests/phase_1/embedder_trait.rs`

- `embedder::fastembed_implements_embedder_trait` -- `let e: Box<dyn Embedder> = Box::new(FastembedEmbedder::new());` compiles and `e.dimension()` == 256.
- `embedder::signature_matches_memory_store_registry` -- take the signature from `Embedder::signature()`, register it via `MemoryStore::register_model`, assert `registered_model()` returns the same.

### Regression verification

- `cargo build -p vestige-core` -- zero warnings.
- `cargo build -p vestige-mcp` -- zero warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` -- green.
- `cargo test -p vestige-core --lib` -- existing 352 core lib tests remain green.
- `cargo test -p vestige-mcp --lib` -- existing 406 mcp tests remain green.
- `cargo test -p vestige-core --lib storage::migrations::tests` -- explicitly invokes the migration tests added in Phase 1.
- `cargo test -p vestige-core --lib storage::sqlite::tests` -- invokes the trait-method unit tests added in Phase 1.
- `cargo test -p vestige-core --lib embedder::fastembed::tests` -- invokes embedder unit tests.
- `cargo test -p vestige-phase-1-tests --test trait_round_trip` -- Phase 1 integration test file 1.
- `cargo test -p vestige-phase-1-tests --test embedding_model_registry` -- Phase 1 integration test file 2.
- `cargo test -p vestige-phase-1-tests --test domain_column_migration` -- Phase 1 integration test file 3.
- `cargo test -p vestige-phase-1-tests --test cognitive_module_isolation` -- Phase 1 integration test file 4.
- `cargo test -p vestige-phase-1-tests --test send_bound_variant` -- Phase 1 integration test file 5.
- `cargo test -p vestige-phase-1-tests --test embedder_trait` -- Phase 1 integration test file 6.
- `cargo test -p vestige-phase-1-tests` -- convenience: runs all integration test binaries in the Phase 1 crate.
- `cargo test -p vestige-e2e` -- existing e2e harness runs unchanged; no new tests here but existing ones must pass.

## Acceptance Criteria

- [ ] `cargo build -p vestige-core` -- zero warnings.
- [ ] `cargo build -p vestige-mcp` -- zero warnings.
- [ ] `cargo build --workspace --all-targets` -- zero warnings.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` -- exits 0.
- [ ] `cargo test -p vestige-core` -- all 352 existing core tests plus new Phase 1 unit tests pass.
- [ ] `cargo test -p vestige-mcp` -- all 406 existing mcp tests pass, unchanged.
- [ ] `cargo test -p vestige-phase-1-tests` -- all Phase 1 integration tests pass.
- [ ] `cargo test -p vestige-e2e` -- existing e2e journey suite passes unchanged.
- [ ] Cumulative test count >= 758 (the pre-Phase-1 baseline) plus the new unit and integration additions.
- [ ] `git grep -n 'rusqlite::' crates/vestige-core/src/neuroscience/ crates/vestige-core/src/advanced/` -- zero hits (the single pre-existing doc-comment reference in `active_forgetting.rs` is acceptable and does not introduce SQL dependency; code references must be zero).
- [ ] `git grep -n 'SqliteMemoryStore' crates/vestige-core/src/neuroscience/ crates/vestige-core/src/advanced/` -- zero hits.
- [ ] `git grep -n 'fastembed::' crates/vestige-core/src/storage/sqlite.rs` -- zero hits (Storage must never call fastembed directly; embedding goes through the `Embedder` trait held on the caller side).
- [ ] `SqliteMemoryStore::insert` refuses a vector whose dimension disagrees with the registered model (returns `MemoryStoreError::InvalidInput`).
- [ ] `SqliteMemoryStore::register_model` returns `MemoryStoreError::ModelMismatch` when a second, different signature is provided after a first was already registered.
- [ ] After upgrading a V11 database to V12, every pre-existing row has `domains == "[]"` and `domain_scores == "{}"` with no NULLs.
- [ ] `#[trait_variant::make(MemoryStore: Send)]` compiles; `Arc<dyn MemoryStore>` is movable across `tokio::spawn`.
- [ ] Migration V12 is idempotent on replay: `apply_migrations` rewound to V11, re-applied, succeeds without error.
- [ ] `vestige-core::storage::Storage` continues to resolve (via the `pub type` alias) at every current call site in `vestige-mcp`.
- [ ] The `embedding_model` table can only hold a single row (programmatic invariant -- verified by an integration test that attempts a second `INSERT INTO embedding_model (id = 1, ...)` and observes the CHECK-enforced uniqueness).
- [ ] `registered_model()` is cached on first read; no SELECT is issued against `embedding_model` after the first hit within the same process (verified by wrapping the reader in a counting proxy in a dedicated test).

## Rollback Notes

If Phase 1 fails mid-way, rollback granularity is per-deliverable and the DB can be downgraded by SQL.

- **D1 (`memory_store.rs`)**: revert the new file. The trait has zero non-test consumers in Phase 1, so deletion is safe.
- **D2 (`storage/mod.rs`)**: revert to the prior export list. The only forward-facing identifier is the `pub type Storage = SqliteMemoryStore;` alias, which becomes `pub use sqlite::Storage;` again once `SqliteMemoryStore` is renamed back to `Storage`.
- **D3 (`sqlite.rs` rename + trait impl)**: revert the struct rename (`SqliteMemoryStore` -> `Storage`). The trait impl is a separate `impl` block and can be deleted wholesale. Inherent methods are unchanged and do not need to be touched. Net diff on revert: delete one `impl LocalMemoryStore for ...` block plus the two helper functions (`row_to_record`, `enforce_model`).
- **D4 (Migration V12)**: DOWN migration script:

```sql
-- Phase 1 rollback: drop Phase 1 schema additions.
-- WARNING: this deletes any `domains` / `domain_scores` values stored under V12.
-- Execute ONLY when downgrading from V12 to V11 on a database where no Phase 4
-- work has happened yet (otherwise you lose domain classifications).

DROP TABLE IF EXISTS domains;
DROP INDEX IF EXISTS idx_nodes_domains;
DROP INDEX IF EXISTS idx_nodes_domain_scores;

-- SQLite does not support DROP COLUMN before 3.35; the project's bundled
-- rusqlite uses 3.45+ (see `bundled-sqlite` feature). So the DROP COLUMN
-- form below is safe on every target platform.
ALTER TABLE knowledge_nodes DROP COLUMN domains;
ALTER TABLE knowledge_nodes DROP COLUMN domain_scores;

DROP TABLE IF EXISTS embedding_model;

UPDATE schema_version SET version = 11, applied_at = datetime('now');
```

  Operationally: the DOWN script is NOT included in the source migrations list (migrations are forward-only). If a rollback is required, it is applied manually via `sqlite3 vestige.db < rollback_v12.sql`. A backup via `storage.backup_to(...)` MUST be taken before the Phase 1 migration runs in production -- the `Storage::backup_to` method already exists (line 3903) and does not need changes.

- **D5/D6 (`embedder/`)**: delete the module. `EmbeddingService` is untouched, so callers that still use it continue to work. The new `Embedder` trait has no pre-Phase-2 consumers.
- **D7 (`lib.rs`)**: revert the re-export additions. Zero downstream impact since the new symbols have no pre-Phase-2 consumers.
- **D8 (cognitive module audit)**: audit-only, no code changes. Nothing to roll back unless `consolidation/sleep.rs` was changed; if so, revert.
- **Crate-level considerations**:
  - `trait-variant` must remain in `Cargo.toml` until every consumer of the trait alias has been reverted. Safe to leave in `[dependencies]` indefinitely; it has no runtime cost.
  - `blake3` was going to be added in Phase 3 anyway; leaving it in on rollback is harmless.
  - `rusqlite` version stays pinned; no bump required for Phase 1.

## Open Implementation Questions

Implementation-choice-only. Architectural questions are resolved in ADR 0001.

1. **`MemoryRecord` vs `KnowledgeNode` as the trait currency.**
   - Candidate A: `MemoryRecord` (new, lean type matching the PRD) -- chosen.
   - Candidate B: use existing `KnowledgeNode` directly.
   - **Recommendation: A.** `KnowledgeNode` carries 30+ FSRS / dual-strength / sentiment / temporal fields that bind callers to the SQLite columns. `MemoryRecord` is what `PgMemoryStore` and future backends will want. SQLite impl converts between the two at the boundary, which is a ~40-line `impl From<KnowledgeNode> for MemoryRecord` (and back) shim. Pays for itself in Phase 2.

2. **`async fn` in traits vs `Box<dyn Future>` via `async-trait`.**
   - Candidate A: use `trait-variant` (RPITIT-based, MSRV 1.75+, our MSRV is 1.91).
   - Candidate B: use `async-trait` (allocates one Box per call).
   - **Recommendation: A.** `trait-variant` generates both the base `LocalMemoryStore` and the `Send`-bound `MemoryStore` from one definition, matches what the PRD explicitly calls out, and avoids the allocation overhead of boxed futures on every CRUD call.

3. **Blocking SQLite under async signatures: spawn_blocking vs inline.**
   - Candidate A: bodies call the existing sync `self.writer.lock()...` inline inside the `async fn`.
   - Candidate B: bodies wrap in `tokio::task::spawn_blocking`.
   - **Recommendation: A for Phase 1.** The current call sites are a mix of sync (CLI, bin/restore.rs) and async (MCP handlers). Introducing `spawn_blocking` would force a tokio runtime even for CLI use. Inline blocking under `async fn` is a documented pattern that compiles and works; under Phase 2 the Postgres impl uses `sqlx` which is natively async, and we can revisit Sqlite blocking policy at that point. Phase 1 priority is "no behavior change".

4. **Where does `register_model` get called from: storage side auto-register, or caller-side explicit?**
   - Candidate A: caller explicitly calls `store.register_model(embedder.signature())` once after `MemoryStore::init`.
   - Candidate B: first `insert` with a vector auto-registers.
   - **Recommendation: B.** The current code path (`Storage::ingest` -> `generate_embedding_for_node` -> INSERT into `node_embeddings`) has no explicit registration step and we want `--no behavior change`. Auto-register on first embedded write preserves the exact current UX. Callers who care (migration tooling, Phase 2 `--reembed`) can still call `register_model` explicitly; it is a no-op when idempotent.

5. **`model_hash` content: fastembed ONNX bytes vs `(name, dim, crate_version)`.**
   - Candidate A: hash the ONNX file bytes on disk (after model download).
   - Candidate B: hash `(name, dim, vestige-core CARGO_PKG_VERSION)`.
   - **Recommendation: B.** Fastembed caches ONNX files under `FASTEMBED_CACHE_PATH`; reading them from inside `FastembedEmbedder::new()` couples the embedder to fastembed's caching behavior and adds slow startup. Hashing `(name, dim, our crate version)` catches the "silent model drift between vestige versions" case the ADR calls out under Risks. Phase 2 can add a content-hashed `OnnxEmbedder` that loads any file and genuinely hashes it; the trait method signature stays the same.

6. **`LocalMemoryStore` `Sync + 'static` or just `Sync`.**
   - Candidate A: `Sync + 'static`.
   - Candidate B: `Sync`.
   - **Recommendation: A.** `'static` is required for `Arc<dyn LocalMemoryStore>` which is the target call pattern (Axum, MCP server, cognitive engine). Every impl we have in mind -- `SqliteMemoryStore`, `PgMemoryStore` -- holds owned state (connection pool, vector index), so `'static` is free.

7. **Should trait methods appear on the SQLite impl instead of being separate?**
   - Candidate A: keep the current ~85 inherent methods on `SqliteMemoryStore` AND add the `impl LocalMemoryStore` block.
   - Candidate B: move every inherent method into the trait.
   - **Recommendation: A.** Many inherent methods (e.g. `run_rac1_cascade_sweep`, `apply_rac1_cascade`, `save_insight`, `save_connection`, `preview_review`, `get_memory_subgraph`) have SQLite-specific semantics, transactional behavior, and call patterns that do not belong in a backend-agnostic trait. They will stay SQLite-only or be extracted into new traits in a post-Phase-4 cleanup. Phase 1's job is to expose the `~25 methods` contract the ADR specifies, not to retrofit the entire API.

8. **Where do `Domain` bytes (centroid) live?**
   - Candidate A: `BLOB` column on `domains` table.
   - Candidate B: JSON-encoded array of f32 in a `TEXT` column.
   - **Recommendation: A.** Consistent with how `node_embeddings.embedding` already stores vectors (little-endian f32 bytes via `Embedding::to_bytes`). JSON would triple the storage size and slow deserialization. The `Domain::centroid: Vec<f32>` field round-trips through the same codec.

9. **Migration numbering when Phase 2 also wants to add a migration.**
   - Candidate A: Phase 1 takes V12, Phase 2 takes V13.
   - Candidate B: Phase 1 takes V12, Phase 2 re-shapes V12 to include its changes.
   - **Recommendation: A.** Migrations are forward-only and append-only in this project. Phase 2 adds V13 (for `review_events` append-only table, if that lands in Phase 2 -- otherwise it is Phase 5 work).

10. **Integration test crate location: sibling to `tests/e2e/` or inside `crates/vestige-core/tests/`.**
    - Candidate A: new workspace member at `tests/phase_1/` (sibling to `tests/e2e/`).
    - Candidate B: under `crates/vestige-core/tests/` (standard cargo integration-test layout).
    - **Recommendation: A.** Matches the existing pattern of `tests/e2e/`, which is already a workspace member with its own `Cargo.toml`. Keeps the Phase 1 test binary outputs in a predictable location (`target/debug/deps/trait_round_trip-*`). Also avoids the build-graph cycle where `crates/vestige-core/tests/` would re-link everything under `vestige-core` each edit.

### Critical Files for Implementation

- /home/delandtj/prppl/vestige/crates/vestige-core/src/storage/memory_store.rs (new; contains the `LocalMemoryStore` / `MemoryStore` traits plus `MemoryRecord`, `SchedulingState`, `SearchQuery`, `SearchResult`, `MemoryEdge`, `Domain`, `ClassificationResult`, `StoreStats`, `HealthStatus`, `MemoryStoreError`, `ModelSignature`)
- /home/delandtj/prppl/vestige/crates/vestige-core/src/storage/sqlite.rs (rename `Storage` -> `SqliteMemoryStore`, add the `impl LocalMemoryStore` block and the `enforce_model` / `row_to_record` helpers; ~200 line diff on a 4592-line file)
- /home/delandtj/prppl/vestige/crates/vestige-core/src/storage/migrations.rs (append `Migration { version: 12, ... }` + `MIGRATION_V12_UP` constant; ~80 new lines)
- /home/delandtj/prppl/vestige/crates/vestige-core/src/embedder/mod.rs (new; `Embedder` + `LocalEmbedder` traits, `EmbedderError`, default `embed_batch`)
- /home/delandtj/prppl/vestige/crates/vestige-core/src/embedder/fastembed.rs (new; `FastembedEmbedder` implementation adapting the existing `EmbeddingService`)
