# Phase 4 Plan: Emergent Domain Classification

**Status**: Draft
**Depends on**: Phase 1 (domain columns on memories, `Domain` struct + `DomainStore` methods on `MemoryStore`, `Embedder` trait), Phase 2 (Postgres JSONB + TEXT[] support for domain fields, `embedding_model` registry parity), Phase 3 (Axum HTTP server, REST `/api/v1/` scaffolding, API key auth middleware, signed dashboard session cookies)
**Related**: docs/adr/0001-pluggable-storage-and-network-access.md (Phase 4), docs/prd/001-getting-centralized-vestige.md (Emergent Domain Model)

---

## Scope

### In scope

- `DomainClassifier` cognitive module under `crates/vestige-core/src/neuroscience/domain_classifier.rs`, alongside existing neuroscience modules (spreading_activation, synaptic_tagging, ...).
- HDBSCAN discovery pipeline using the `hdbscan` crate (v0.10): load all embeddings, cluster, extract centroids, extract top-terms via TF-IDF over cluster members, persist via the trait's `DomainStore` methods.
- Soft-assignment pipeline: for each memory, compute `cosine_similarity(memory.embedding, domain.centroid)` for every domain, store raw scores in `domain_scores` JSONB, threshold into `domains[]` using `assign_threshold` (default 0.65).
- Automatic classification on ingest: run through `CognitiveEngine` / `smart_ingest` so new memories get classified against existing centroids immediately; skip when `domain_count == 0` (Phase 0 accumulation).
- Re-cluster hook in dream consolidation: every Nth four-phase dream cycle (N=5 default) triggers a discovery pass and generates proposals (split / merge / none). Proposals land in a new `domain_proposals` table, surface in the dashboard, and are never auto-applied (conservative drift, ADR Q7).
- Context signals: `SignalSource` trait with `GitRepoSignal` (detects `.git` in CWD or `metadata.cwd`) and `IdeHintSignal` (reads `metadata.editor` / `metadata.ide`). Each returns a `boost_map` of `domain_id -> additive delta` (typical +0.05). Injected as a `signal_boost: Option<HashMap<String, f64>>` parameter into `DomainClassifier::classify`.
- Cross-domain spreading activation decay: `ActivationNetwork` traversal multiplies the edge's effective weight by `cross_domain_decay` (default 0.5) when `target.domains` and `source.domains` are disjoint. Strict "no overlap" policy, not graded.
- CLI subcommands (in `crates/vestige-mcp/src/bin/cli.rs`, under a new `Domains` command group): `list`, `discover [--min-cluster-size N] [--force]`, `rename <id> <new_label>`, `merge <a> <b> [--into <id>]`. Human-readable tables on stdout; JSON via `--json`.
- Dashboard UI additions (`apps/dashboard/src/routes/(app)/domains/`): list page, per-domain detail (memories, centroid top_terms, score histogram, proposal review controls).
- REST endpoints under `/api/v1/domains` (introduced by Phase 3 skeleton, implemented in Phase 4): list, discover, rename, merge, proposal list / accept / reject.
- Config additions: `[domains]` section in `vestige.toml` covering `assign_threshold`, `recluster_interval`, `min_cluster_size`, `cross_domain_decay`, `discovery_threshold`, `merge_threshold`, `signal_boost` (per-signal toggle).

### Out of scope

- Phase 5 federation (explicit separate ADR). Domain centroids are installation-local; no sync.
- Learned re-weighting of domain scores (future, only if retrieval-quality metrics show a need).
- Interactive cluster-membership editing in the UI (drag-and-drop reassign) -- future enhancement.
- Multi-user domain namespaces. One domain set per installation; API keys that carry `domain_filter` just restrict access, they do not create namespaces.
- Auto-sweep of `min_cluster_size` / auto-tuned `assign_threshold` (ADR resolution Q6 + Q9: static defaults, user tunes).
- Graded cross-domain decay (`|A intersect B| / max(|A|,|B|)`) -- strict "no overlap" is the Phase 4 rule.

---

## Prerequisites

Artifacts that Phases 1-3 are expected to have landed:

- In `vestige-core`:
  - `Embedder` trait (`crates/vestige-core/src/embedder/`).
  - `MemoryStore` trait (`crates/vestige-core/src/storage/trait.rs` or similar) including `DomainStore` methods: `list_domains`, `get_domain`, `upsert_domain`, `delete_domain`, `classify(&[f32]) -> Vec<(String, f64)>`, plus a bulk accessor such as `all_embeddings()` (already present in sqlite.rs as `get_all_embeddings`) and a `get_all_memories_with_embeddings()` iterator for discovery. The trait must expose a method to batch-update `(domains, domain_scores)` for a memory id.
  - `Domain` struct: `{ id: String, label: String, centroid: Vec<f32>, top_terms: Vec<String>, memory_count: usize, created_at: DateTime<Utc> }`.
  - Columns on memories in both SQLite and Postgres: `domains TEXT[]` (or JSON array on SQLite) and `domain_scores JSONB` (or TEXT JSON on SQLite).
  - The `domains` table in both backends (see PRD schema sketch).
- In `vestige-mcp`:
  - Axum `/api/v1/` router prefix with auth middleware.
  - CLI skeleton (`bin/cli.rs`) using `clap`; Phase 4 adds a `Domains` subcommand tree.
  - REST handlers file structure ready under `crates/vestige-mcp/src/dashboard/handlers.rs` (legacy) and a dedicated REST handler under `/api/v1/`; Phase 4 adds `domains.rs` handler module.
  - SvelteKit dashboard (`apps/dashboard/`) with existing `(app)/memories`, `(app)/timeline`, `(app)/stats`, etc. Phase 4 adds `(app)/domains/`.

New workspace crate additions required (added manually to `Cargo.toml`, since `cargo add` is not run from the plan):

- `hdbscan = "0.10"` in `crates/vestige-core/Cargo.toml` (feature-gated behind `domain-classification`).
- Optional: a lightweight stop-word constant inline; no external stop-word crate -- the neuroscience modules already do tokenization on whitespace + length>3 (see `dreams.rs::content_similarity`). Reuse that style; no `ndarray` needed because `hdbscan` v0.10 accepts `&[Vec<f32>]` directly (verified from PRD snippet).
- No new deps in `vestige-mcp` for Phase 4 -- CLI reuses `clap` / `colored` / `comfy-table` if already present, otherwise a hand-rolled padded print. We pick hand-rolled to avoid adding a table crate; this matches the existing style of `run_stats` in `cli.rs`.

Test fixtures:

- A JSON seed corpus checked into `tests/phase_4/fixtures/seed_500.json` containing >= 500 memories drawn from three plausible clusters. A builder function `tests/phase_4/support/fixtures.rs::build_seed_corpus()` deterministically generates or loads this corpus. Each record has `content`, `tags`, `embedding` (768D bge-base-en-v1.5; use a committed vector or a deterministic mock embedder in tests). For deterministic tests we fake embeddings by hashing content -- acceptable as long as the fake preserves cluster separability (prefix-based: "DEV-...", "INFRA-...", "HOME-..." seeds three Gaussian blobs).
- Reuse `Embedder` mock from Phase 1 tests (`MockEmbedder`) for discovery tests that need real cosine similarity.
- A minimal git-repo fixture created in a tempdir (`tempfile::tempdir` + `std::process::Command::new("git").arg("init")`) for context-signal tests.

---

## Deliverables

1. `DomainClassifier` cognitive module: struct, defaults, `classify`, `classify_with_boost`, `reassign_all`, `discover`.
2. `domain_terms` helper (TF-IDF over cluster members, returning `top_k` terms).
3. `cli domains discover` subcommand.
4. `cli domains list` / `rename` / `merge` subcommands.
5. Auto-classify hook on ingest (wired into the cognitive engine's ingest pipeline before persistence).
6. Re-cluster hook in dream consolidation (`DreamEngine::run` orchestrator gets an optional `DomainReClusterHook`; triggers every Nth dream).
7. Context signal extractor module (`crates/vestige-core/src/neuroscience/context_signals.rs`) with `SignalSource` trait + `GitRepoSignal` + `IdeHintSignal`.
8. Cross-domain spreading activation decay in `ActivationNetwork::activate` (config-driven).
9. `vestige.toml` `[domains]` section + defaults loader.
10. Dashboard UI: SvelteKit routes `(app)/domains/+page.svelte` (list), `(app)/domains/[id]/+page.svelte` (detail), `(app)/domains/proposals/+page.svelte` (review).
11. REST endpoints under `/api/v1/domains` + `/api/v1/domains/proposals`.
12. `domain_proposals` table + migration + `DomainProposal` trait methods on `MemoryStore`.
13. WebSocket event `VestigeEvent::DomainProposalCreated` so the dashboard gets a live notification after a re-cluster fires.

---

## Detailed Task Breakdown

### 1. `DomainClassifier` cognitive module

**File**: `crates/vestige-core/src/neuroscience/domain_classifier.rs`
**Export**: in `crates/vestige-core/src/neuroscience/mod.rs`, add `pub mod domain_classifier;` and re-export `pub use domain_classifier::{DomainClassifier, ClassificationResult, DomainProposal, ProposalKind};`
**Deps**: `hdbscan = "0.10"`, `serde`, `serde_json`, `chrono`, `tracing`, existing `crate::storage::Domain`, `crate::storage::MemoryStore` trait.

Struct and defaults (match PRD exactly):

```rust
pub struct DomainClassifier {
    pub assign_threshold: f64,      // default 0.65
    pub discovery_threshold: usize, // default 150
    pub recluster_interval: usize,  // default 5 (every 5th dream)
    pub min_cluster_size: usize,    // default 10
    pub min_samples: usize,         // default 5 (HDBSCAN)
    pub cross_domain_decay: f64,    // default 0.5
    pub merge_threshold: f64,       // default 0.90 (centroid cosine)
    pub top_terms_k: usize,         // default 10
}

impl Default for DomainClassifier { ... }
```

Result types:

```rust
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub scores: HashMap<String, f64>, // raw per-domain similarities
    pub domains: Vec<String>,         // above assign_threshold
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalKind {
    Split { parent: String, children: Vec<String> },
    Merge { targets: Vec<String>, suggested_label: String },
    NewCluster { top_terms: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct DomainProposal {
    pub id: String,                 // uuid v4
    pub kind: ProposalKind,
    pub rationale: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub status: ProposalStatus,     // Pending | Accepted | Rejected
}
```

Key methods (all pure where possible; all pub):

```rust
impl DomainClassifier {
    pub fn classify(&self, embedding: &[f32], domains: &[Domain]) -> ClassificationResult;

    pub fn classify_with_boost(
        &self,
        embedding: &[f32],
        domains: &[Domain],
        boost: Option<&HashMap<String, f64>>,
    ) -> ClassificationResult;

    pub async fn reassign_all(
        &self,
        store: &dyn MemoryStore,
        domains: &[Domain],
    ) -> Result<usize, StorageError>;

    pub async fn discover(
        &self,
        store: &dyn MemoryStore,
    ) -> Result<Vec<Domain>, StorageError>;

    pub async fn propose_changes(
        &self,
        store: &dyn MemoryStore,
        existing: &[Domain],
        newly_discovered: &[Domain],
    ) -> Result<Vec<DomainProposal>, StorageError>;

    pub async fn apply_proposal(
        &self,
        store: &dyn MemoryStore,
        proposal: &DomainProposal,
    ) -> Result<(), StorageError>;
}
```

Behavior notes:

- `classify` returns empty `{ scores: {}, domains: [] }` iff `domains.is_empty()` (accumulation phase). This matches the PRD snippet verbatim.
- `classify_with_boost` adds the boost delta to each score AFTER cosine, before thresholding. It clamps to `[0.0, 1.0]`. Boost keys not present in `domains` are ignored.
- `reassign_all` streams memories in batches of 500 (iterator on the store) to keep memory bounded; for each memory issues a single `UPDATE memories SET domains = ?, domain_scores = ? WHERE id = ?` call. Returns count of memories whose `domains` vector actually changed.
- `discover` loads all `(id, embedding)` pairs via an `all_embeddings()` method on the store (exists under `#[cfg(all(feature = "embeddings", feature = "vector-search"))]` in `sqlite.rs::get_all_embeddings`; Phase 1 should promote this onto the trait -- if not yet promoted, add the method). Then:
  1. Build `Vec<Vec<f32>>` and index -> id map.
  2. `Hdbscan::default_hyper_params(&embeddings).min_cluster_size(self.min_cluster_size).min_samples(self.min_samples).build()` (exact builder depends on hdbscan 0.10 surface; see Open Question).
  3. `let labels = clusterer.cluster()?;`
  4. `let centers = clusterer.calc_centers(Center::Centroid, &labels)?;`
  5. Group indices by label ignoring -1 (noise). For each cluster compute `top_terms` via `compute_top_terms`.
  6. Preserve stable IDs where possible: match each new cluster centroid to the closest existing domain by cosine; if similarity > 0.85, reuse the existing domain id + label. Otherwise generate a fresh id `cluster_{n}` with a label derived from the first 2 terms.
  7. Upsert all resulting `Domain`s via the store.
- `propose_changes` compares old vs new clusters:
  - **Split**: an old domain that best-matches two or more new domains each with >= `min_cluster_size` members. Rationale: "domain `dev` is now 2 clusters of >=10 memories: `systems` and `networking`".
  - **Merge**: two old domains whose centroids now satisfy `cosine > merge_threshold` get a merge proposal.
  - **NewCluster**: a new cluster that doesn't match any old domain above 0.85 similarity.
- `apply_proposal` runs the split or merge against the store (reassign memberships via `reassign_all`), then marks the proposal `Accepted`. It never runs automatically -- only via the CLI or dashboard.

Helper:

```rust
fn compute_top_terms(documents: &[&str], k: usize) -> Vec<String>;
```

Uses TF-IDF with IDF computed over the entire passed-in corpus (the `documents` slice), tokenization = whitespace split, lowercase, strip non-alphanumeric, drop tokens shorter than 4 chars and a small built-in stop-word list (`the`, `and`, `for`, `that`, `with`, ...). Matches the tokenizer used in `dreams.rs::content_similarity` and `dreams.rs::extract_patterns` so behavior is predictable.

Cosine similarity helper:

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64;
```

Keep the existing crate-level `cosine_similarity` if already present (check `embeddings::` or `search::`); otherwise add a private one. Returns 0.0 on dimension mismatch, panics would be a bug.

### 2. Top-terms computation helper

**File**: same module, private section.

- `fn tokenize(text: &str) -> Vec<String>`: lowercase, split on non-alphanumeric, filter len >= 4, drop stop-words.
- `fn tfidf_top_k(docs: &[&str], k: usize) -> Vec<String>`:
  1. `tf[doc_idx][term] = count / total_terms`.
  2. `df[term] = docs containing term`.
  3. `idf[term] = log((N + 1) / (df[term] + 1)) + 1` (smoothed).
  4. For each term, average `tf` across docs in the cluster; multiply by `idf`; sort desc; return top `k`.

Cluster top-terms are computed over cluster members only, with IDF over the **whole corpus** (all memory contents), not the cluster, so common words get penalized globally. Recompute global IDF once per `discover` call.

### 3. CLI subcommand: `vestige domains discover`

**File**: `crates/vestige-mcp/src/bin/cli.rs`

Add to `enum Commands`:

```rust
/// Emergent domain management
Domains {
    #[command(subcommand)]
    action: DomainAction,
},
```

```rust
#[derive(clap::Subcommand)]
enum DomainAction {
    /// List all discovered domains
    List {
        #[arg(long)] json: bool,
    },
    /// Run HDBSCAN discovery on all embeddings and propose domains
    Discover {
        #[arg(long, default_value_t = 10)] min_cluster_size: usize,
        /// Skip the proposal flow and write new domains directly (first-time use)
        #[arg(long)] force: bool,
        #[arg(long)] json: bool,
    },
    /// Rename a domain (by id)
    Rename {
        id: String,
        new_label: String,
    },
    /// Merge two domains
    Merge {
        a: String,
        b: String,
        #[arg(long)] into: Option<String>, // default: `a`
    },
}
```

Handler plumbing lives in `run_domains(action)` dispatching to `run_domains_list`, `run_domains_discover`, `run_domains_rename`, `run_domains_merge`. Each opens the default `Storage`, constructs a `DomainClassifier::default()`, and invokes the appropriate method.

Output format for `list`:

```
ID              LABEL              MEMORIES    TOP TERMS
dev             Development        87          rust, trait, async, tokio, zinit
infra           Infrastructure     47          bgp, sonic, vlan, frr, peering
home            Home               31          solar, kwh, battery, pool, esphome
(unclassified)                     12
```

Produced via plain `print!` with `%-15s %-18s %-10d %s` style padding. `--json` emits `serde_json::to_string_pretty(&domains)`.

Output format for `discover` with `--force`:

```
HDBSCAN: 500 embeddings, min_cluster_size=10, min_samples=5
Found 3 clusters (ignoring 14 noise points)
  cluster_0 (N=47)  top: bgp, sonic, vlan, frr, peering
  cluster_1 (N=31)  top: solar, kwh, battery, pool, esphome
  cluster_2 (N=22)  top: rust, trait, async, tokio, zinit

Writing 3 domains to the store...
Soft-assigning 500 memories against centroids...
  multi-domain: 43
  single-domain: 412
  unclassified (below threshold 0.65): 45
Done in 7.4s.
```

Output format for `discover` without `--force` (post-Phase-0):

```
HDBSCAN: 623 embeddings, min_cluster_size=10
Comparing to existing 3 domains...

Proposals (pending, accept via dashboard or `vestige domains proposals`):
  [split] dev -> (systems:34, networking:28)    confidence 0.82
  [new]   cluster_5 (books, novels, reading)    confidence 0.71

Run `vestige domains proposals` to review, or open the dashboard.
```

### 4. CLI: `list`, `rename`, `merge`

- `list`: calls `store.list_domains()`, fetches unclassified count via `store.count_memories_without_domains()` (Phase 1 should have provided this; if not, Phase 4 adds it to the trait and both backends).
- `rename`: `store.get_domain(id)` -> mutate `label` -> `store.upsert_domain`. No memory touch.
- `merge`: load both, compute blended centroid (weighted by `memory_count`), merge `top_terms` (union, recompute TF-IDF rank if both sides share the corpus), delete the non-`into` domain, call `reassign_all`. Wrapped in a transaction on Postgres; on SQLite rely on the existing writer-lock pattern.

### 5. Auto-classify on ingest

**File**: `crates/vestige-core/src/cognitive.rs` (or equivalent ingest entry in `vestige-mcp/src/tools/smart_ingest.rs`).

Integration point: just before the record is persisted in the smart-ingest path, after the embedder has produced `embedding` and before `storage.insert(...)`. Trace the current call site -- today `Storage::ingest(IngestInput)` computes embedding inside storage; in Phase 1 the embedder becomes external (ADR decision Q2), so classification can hook right there in the cognitive engine.

Pseudocode:

```rust
let embedding = embedder.embed(&input.content).await?;
let domains = store.list_domains().await?;

let (domains_assigned, domain_scores) = if domains.is_empty() {
    (Vec::new(), HashMap::new())
} else {
    let boost = context_signals.gather_boost(&input.metadata, &domains);
    let result = classifier.classify_with_boost(&embedding, &domains, boost.as_ref());
    (result.domains, result.scores)
};

record.embedding = Some(embedding);
record.domains = domains_assigned;
record.domain_scores = domain_scores;
store.insert(&record).await?;
```

Edge cases:

- Accumulation phase (`domains.is_empty()`): skip classification entirely. Zero overhead.
- Embedding failed / skipped: leave `domains = []`, `domain_scores = {}`. Never fail ingest because of classification.
- Metric: emit `VestigeEvent::MemoryClassified { id, domains, top_score }` on the WebSocket bus so the dashboard sees it live.

### 6. Re-cluster hook in dream consolidation

**File**: `crates/vestige-core/src/advanced/dreams.rs` (long file, 1131-line `dream()` entry on the `MemoryDreamer` impl) plus `crates/vestige-core/src/consolidation/phases.rs` (the `DreamEngine::run` orchestrator).

Design: the `DreamEngine::run(...)` returns `FourPhaseDreamResult`. It does not currently know how many times it has run. Phase 4 introduces a persistent counter on disk (column `dream_cycle_count` on a new singleton `system_state` table, or a simple row in the existing `metadata` / `embedding_model` registry). After the Integration phase finishes, the cognitive engine increments the counter and, if `counter % recluster_interval == 0`, launches discovery asynchronously:

Extension struct in `phases.rs`:

```rust
pub struct DreamReClusterHook<'a> {
    pub classifier: &'a DomainClassifier,
    pub store: &'a dyn MemoryStore,
    pub event_tx: Option<&'a tokio::sync::mpsc::UnboundedSender<VestigeEvent>>,
}

impl<'a> DreamReClusterHook<'a> {
    pub async fn tick(&self, cycle_count: usize) -> Result<Vec<DomainProposal>, StorageError> {
        if cycle_count == 0 || cycle_count % self.classifier.recluster_interval != 0 {
            return Ok(vec![]);
        }
        let existing = self.store.list_domains().await?;
        let rediscovered = self.classifier.discover(self.store).await?;
        let proposals = self
            .classifier
            .propose_changes(self.store, &existing, &rediscovered)
            .await?;
        for p in &proposals {
            self.store.insert_domain_proposal(p).await?;
            if let Some(tx) = self.event_tx {
                let _ = tx.send(VestigeEvent::DomainProposalCreated {
                    id: p.id.clone(),
                    kind: format!("{:?}", p.kind),
                    confidence: p.confidence,
                    timestamp: Utc::now(),
                });
            }
        }
        Ok(proposals)
    }
}
```

Caller wires `tick()` after `DreamEngine::run()` returns, at the ingest/consolidation orchestrator level. The hook never mutates existing domains -- it only writes proposals. The acceptance path is manual (CLI or dashboard).

Counter storage: add method `store.bump_dream_cycle_count() -> Result<usize>` returning the new count. Single-row table:

```sql
CREATE TABLE IF NOT EXISTS system_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- seed: ('dream_cycle_count', '0')
```

### 7. Context signal extractor

**File**: `crates/vestige-core/src/neuroscience/context_signals.rs`

```rust
pub trait SignalSource: Send + Sync {
    /// Returns domain_id -> additive boost (positive or negative, typically in [-0.1, +0.1]).
    fn boost_map(
        &self,
        input_metadata: &serde_json::Value,
        domains: &[Domain],
    ) -> HashMap<String, f64>;

    fn name(&self) -> &'static str;
}

pub struct GitRepoSignal {
    pub boost: f64, // default +0.05
}

pub struct IdeHintSignal {
    pub boost: f64,
}

pub struct ContextSignals {
    sources: Vec<Box<dyn SignalSource>>,
}

impl ContextSignals {
    pub fn gather_boost(
        &self,
        input_metadata: &serde_json::Value,
        domains: &[Domain],
    ) -> Option<HashMap<String, f64>>;
}
```

Signal encoding convention (document in the module header):

- A signal is a **soft prior**. It nudges the post-cosine score by a small additive delta, clamped to `[-0.10, +0.10]` per signal.
- Multiple signals sum, then the final boost per domain is clamped to `[-0.15, +0.15]` so signals cannot by themselves push a memory into or out of a domain; the embedding similarity dominates.
- Signals target domains by heuristic: `GitRepoSignal` boosts any domain whose `top_terms` overlaps `{"rust","async","trait","function","class","def","git","commit","fn","code"}`. `IdeHintSignal` does the same for `{"file","line","editor","vscode","neovim","rust-analyzer","lsp"}`.
- All signal boosts are logged via `tracing::debug!` so users can audit why a memory picked up a domain.

`GitRepoSignal::boost_map` implementation:

```rust
fn boost_map(&self, meta: &Value, domains: &[Domain]) -> HashMap<String, f64> {
    let is_git = meta.get("cwd")
        .and_then(|v| v.as_str())
        .map(|cwd| std::path::Path::new(cwd).join(".git").exists())
        .unwrap_or(false)
        || meta.get("git_repo").is_some();
    if !is_git { return HashMap::new(); }
    let mut out = HashMap::new();
    for d in domains {
        let code_hits = d.top_terms.iter()
            .filter(|t| CODE_TERMS.contains(t.as_str()))
            .count();
        if code_hits > 0 { out.insert(d.id.clone(), self.boost); }
    }
    out
}
```

Config knob in `[domains.signals]`: `git = true`, `ide = true`, `git_boost = 0.05`, `ide_boost = 0.05`.

### 8. Cross-domain spreading activation decay

**File**: `crates/vestige-core/src/neuroscience/spreading_activation.rs`

Modify `ActivationConfig`:

```rust
pub struct ActivationConfig {
    pub decay_factor: f64,
    pub max_hops: u32,
    pub min_threshold: f64,
    pub allow_cycles: bool,
    pub cross_domain_decay: f64, // NEW, default 0.5
}
```

Domain metadata on nodes: the current `ActivationNode` has `id`, `activation`, `last_activated`, `edges: Vec<String>`. Phase 4 adds `pub domains: Vec<String>`. Populated when nodes get added (propagated from the memory's `domains` field). The network is rebuilt on each search from the store; if the in-memory network is persisted (check `ActivationNetwork` lifetime in `CognitiveEngine`), the population happens in the engine at boot and on insert.

Traversal change, in `ActivationNetwork::activate` loop, replacing the single line `let propagated = current_activation * edge.strength * self.config.decay_factor;`:

```rust
let cross_penalty = {
    let src_doms = self.nodes.get(&current_id).map(|n| &n.domains);
    let tgt_doms = self.nodes.get(&target_id).map(|n| &n.domains);
    match (src_doms, tgt_doms) {
        (Some(s), Some(t)) if !s.is_empty() && !t.is_empty() => {
            let overlap = s.iter().any(|d| t.contains(d));
            if overlap { 1.0 } else { self.config.cross_domain_decay }
        }
        _ => 1.0, // unclassified on either side: no penalty
    }
};
let propagated = current_activation * edge.strength * self.config.decay_factor * cross_penalty;
```

Rationale for "unclassified -> no penalty": unclassified memories are Phase-0 or low-confidence corpus members; penalizing them would block useful cross-pollination during the accumulation ramp.

API to update a node's domains after reclassification:

```rust
pub fn set_node_domains(&mut self, id: &str, domains: Vec<String>);
```

Called by the reassignment pipeline after `reassign_all`.

### 9. `vestige.toml` `[domains]` section

**File**: wherever `vestige.toml` is loaded (search for `[storage]` / `[server]` loaders). Add:

```toml
[domains]
assign_threshold = 0.65
discovery_threshold = 150
recluster_interval = 5
min_cluster_size = 10
min_samples = 5
cross_domain_decay = 0.5
merge_threshold = 0.90
top_terms_k = 10

[domains.signals]
git = true
ide = true
git_boost = 0.05
ide_boost = 0.05
```

Rust-side: `DomainsConfig { ... }` struct with `serde(default)` so `vestige.toml` without a `[domains]` section falls back to hard-coded defaults. `DomainClassifier::from_config(cfg: &DomainsConfig) -> Self`.

### 10. Dashboard UI additions

**SvelteKit routes** (`apps/dashboard/src/routes/(app)/domains/`):

- `+page.svelte` (list): fetches `GET /api/v1/domains` and `GET /api/v1/domains/unclassified-count`. Renders a table: `label`, `memories`, `top_terms` chips, `created_at`. Each row links to `/domains/[id]`. A "Discover" button posts `POST /api/v1/domains/discover`.
- `[id]/+page.svelte` (detail): fetches `GET /api/v1/domains/:id`, `GET /api/v1/domains/:id/memories?limit=100`, `GET /api/v1/domains/:id/score-histogram`. Renders:
  - Header: label (editable, triggers `PUT /api/v1/domains/:id`), top-terms chips, memory count, created_at.
  - Histogram: a vertical bar chart of `domain_scores[:id]` buckets 0-0.1, 0.1-0.2, ..., 0.9-1.0 across all memories. Data source: server precomputes buckets so the client does not need to fetch all scores.
  - Memory list: paginated, each row shows the raw score for this domain.
- `proposals/+page.svelte`: fetches `GET /api/v1/domains/proposals?status=pending`. Each pending proposal card shows `kind`, `rationale`, `confidence`, `created_at`, buttons "Accept" (posts `POST /api/v1/domains/proposals/:id/accept`) and "Reject" (`POST .../reject`). Live updates via the existing WebSocket channel (`/ws`) reacting to `DomainProposalCreated` events.

Styling reuses the existing Tailwind + shadcn-svelte conventions in `apps/dashboard/src/lib/components/`.

Existing `(app)/stats` and `(app)/feed` pages get a small "Domains" summary panel that links to `/domains`.

### 11. REST endpoints

**File**: `crates/vestige-mcp/src/protocol/http.rs` or a new `crates/vestige-mcp/src/api/domains.rs` module, wired into the `/api/v1/` router.

| Method | Path | Handler |
|--------|------|---------|
| GET | `/api/v1/domains` | `list_domains` -- returns `[Domain...]` + unclassified count |
| POST | `/api/v1/domains/discover` | `trigger_discover` -- body `{ min_cluster_size?: usize, force?: bool }`, returns proposals or applied domains |
| GET | `/api/v1/domains/:id` | `get_domain` |
| PUT | `/api/v1/domains/:id` | `update_domain` -- rename |
| DELETE | `/api/v1/domains/:id` | `delete_domain` -- with `?merge_into=other_id` |
| GET | `/api/v1/domains/:id/memories` | paginated memories in this domain |
| GET | `/api/v1/domains/:id/score-histogram` | precomputed buckets |
| GET | `/api/v1/domains/proposals` | `list_proposals?status=pending` |
| POST | `/api/v1/domains/proposals/:id/accept` | `accept_proposal` |
| POST | `/api/v1/domains/proposals/:id/reject` | `reject_proposal` |

All handlers go through the Phase 3 auth middleware (Bearer / X-API-Key / session cookie). Responses are JSON; error paths use `StatusCode::*` with a small `{"error": "..."}` body.

### 12. `domain_proposals` table + trait methods

Postgres migration (`crates/vestige-core/migrations/postgres/00XX_domain_proposals.sql`):

```sql
CREATE TABLE domain_proposals (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind         TEXT NOT NULL,      -- 'split' | 'merge' | 'new_cluster'
    payload      JSONB NOT NULL,     -- serialized ProposalKind body
    rationale    TEXT NOT NULL,
    confidence   DOUBLE PRECISION NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending', -- pending|accepted|rejected
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at  TIMESTAMPTZ
);
CREATE INDEX idx_domain_proposals_status ON domain_proposals (status, created_at DESC);
```

SQLite migration: same table, `UUID` -> `TEXT`, `JSONB` -> `TEXT` with JSON-encoded bodies, `TIMESTAMPTZ` -> `TEXT` ISO-8601.

`MemoryStore` trait additions:

```rust
async fn insert_domain_proposal(&self, p: &DomainProposal) -> Result<()>;
async fn list_domain_proposals(&self, status: Option<&str>) -> Result<Vec<DomainProposal>>;
async fn get_domain_proposal(&self, id: &str) -> Result<Option<DomainProposal>>;
async fn set_proposal_status(&self, id: &str, status: &str) -> Result<()>;
```

### 13. WebSocket event for proposals

**File**: `crates/vestige-mcp/src/dashboard/events.rs`

Add variant:

```rust
pub enum VestigeEvent {
    // ... existing ...
    DomainProposalCreated {
        id: String,
        kind: String,
        confidence: f64,
        timestamp: DateTime<Utc>,
    },
    MemoryClassified {
        id: String,
        domains: Vec<String>,
        top_score: f64,
        timestamp: DateTime<Utc>,
    },
}
```

The SvelteKit dashboard's WS client reacts to both events: classified events refresh any open domain-detail page; proposal events push a toast and a badge on the navbar.

---

## Test Plan

Test root: `tests/phase_4/` (a new member of the workspace; mirror the `tests/e2e` layout).

`tests/phase_4/Cargo.toml`:

```toml
[package]
name = "vestige-phase4-tests"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
vestige-core = { path = "../../crates/vestige-core", features = ["embeddings", "vector-search", "domain-classification"] }
vestige-mcp  = { path = "../../crates/vestige-mcp" }
tokio = { workspace = true }
anyhow = "1"
tempfile = "3"
serde_json = { workspace = true }
uuid = { workspace = true }
```

### Unit tests (colocated in `domain_classifier.rs::tests`, `context_signals.rs::tests`, `spreading_activation.rs::tests`)

Each public function must have at least one test:

- `classify_empty_domains_returns_empty`: `classify(&[0.0; 768], &[])` returns `ClassificationResult { scores: {}, domains: [] }`.
- `classify_single_domain_scores`: one `Domain` with a known centroid; input embedding equal to centroid; expect score 1.0 and `domains == [id]`.
- `classify_multi_domain_overlap`: two domains A, B; input halfway between centroids; expect both scores >= `assign_threshold`; expect `domains == [A, B]` (order not guaranteed).
- `classify_below_threshold_returns_empty_domains_but_scores_filled`: input orthogonal to all centroids; expect `scores` populated, `domains` empty.
- `classify_with_boost_adds_delta`: same input as above, with `boost = {A: 0.4}`; expect A now above threshold, B unchanged.
- `classify_boost_clamps_to_unit`: `boost = {A: 5.0}`; resulting `scores[A]` must be <= 1.0.
- `tfidf_top_k_returns_distinct_terms`: given three fake docs, `top_k=3` returns three non-duplicate strings, in descending TF-IDF order.
- `tfidf_top_k_drops_stopwords`: `["the and for"]` + real content -> stop-words absent.
- `compute_top_terms_handles_empty_cluster`: returns `vec![]` (no panic).
- `signal_git_present_vs_absent`: `GitRepoSignal` given metadata with `.git` in cwd returns non-empty map; without it returns empty.
- `signal_ide_present_vs_absent`: `IdeHintSignal` ditto for `metadata.editor == "vscode"`.
- `signal_combined_clamped`: two signals both firing each at +0.10 -> combined map values <= +0.15.
- `cross_domain_decay_full_weight_on_overlap`: graph with node A in domain `dev`, node B in domain `dev`, edge A->B strength 1.0; after `activate`, B's activation equals the standard `initial * strength * decay_factor` (no extra penalty).
- `cross_domain_decay_half_weight_no_overlap`: A in `dev`, B in `infra`, same edge -> B's activation is 0.5x that of the overlap case.
- `cross_domain_decay_unclassified_no_penalty`: A classified, B unclassified -> full weight.
- `propose_changes_detects_split`: existing domain `dev`; new discovery returns two clusters whose centroids both sit close to old `dev` centroid, each >= min_cluster_size members -> proposal of kind `Split { parent: "dev", children: [a, b] }`.
- `propose_changes_detects_merge`: two existing domains whose new centroids now have cosine > `merge_threshold` -> proposal of kind `Merge`.
- `propose_changes_detects_new_cluster`: a new cluster with no match >= 0.85 to any existing -> `NewCluster`.
- `apply_proposal_split_updates_memberships`: after accept, memories previously in `dev` get reassigned (some to child a, some to child b) via `reassign_all`.

### Integration tests (`tests/phase_4/tests/`)

One file per behavior listed in the Phase 4 acceptance sheet.

- `discover_seed_corpus.rs` -- loads the 500-memory fixture, runs `classifier.discover(&store).await`, asserts at least 3 clusters, asserts per-cluster intra-similarity mean > 0.6, asserts discovery wall time < 10s in release. Also asserts `top_terms` for each cluster contains at least one expected keyword per cluster (dev: contains any of `rust/trait/async`; infra: `bgp/vlan/network`; home: `solar/battery/pool`).
- `soft_assign_multi_domain.rs` -- inserts a memory "deploy zinit containers over BGP network"; after classify, `domains` contains both `dev` and `infra` (from a known centroid setup).
- `auto_classify_on_ingest.rs` -- with three existing domains, a fresh `smart_ingest` of a dev-ish sentence ends up with `domains == ["dev"]` and non-empty `domain_scores`.
- `reembed_triggers_recluster.rs` -- after `vestige migrate --reembed`, centroids must be recomputed; verify `list_domains()` returns fresh `centroid` values (different from pre-reembed).
- `dream_consolidation_recluster_hook.rs` -- run 5 dream cycles with heavy synthetic memory insertion; after the 5th, assert `list_domain_proposals("pending")` has at least one proposal.
- `proposal_accept_applies_changes.rs` -- accept a split proposal via `apply_proposal`; verify that memories in `dev` are now distributed across the new children and that the old `dev` domain is removed.
- `proposal_reject_leaves_state.rs` -- reject a proposal; verify all domains and memberships unchanged.
- `drift_is_proposal_only.rs` -- over 5 dream cycles with new inserts, never call accept; verify every memory's `domains` field equals its initial post-discovery value. No auto-apply.
- `cross_domain_activation_decay.rs` -- build a `ActivationNetwork` with two memories linked by a strength-1.0 edge, one in `dev`, one in `infra`; activate `dev` memory with 1.0; assert `infra` memory's activation == `0.5 * decay_factor` (0.35 with default decay_factor 0.7). Then set both to `dev` and reassert activation == `0.7`.
- `cli_domains_discover.rs` -- spawn `cargo run -- domains discover --force --json`, parse stdout, assert at least 3 clusters and valid JSON shape.
- `cli_domains_rename_merge.rs` -- happy-path rename then merge, with stdout assertions.
- `context_signal_git_repo.rs` -- ingest the same sentence from inside a tempdir with `.git` vs outside; assert the git-run produces slightly higher `domain_scores` for the code-related domain (diff >= 0.04, matches `git_boost = 0.05`).
- `threshold_tunable.rs` -- same memory, two runs with `assign_threshold = 0.40` vs `0.85`; the low-threshold run assigns more domains than the high-threshold run for the same content.
- `signal_boost_clamped.rs` -- artificially configure `git_boost = 5.0` and assert the resulting per-domain score is still <= 1.0.
- `discover_preserves_stable_ids.rs` -- run discover twice with no new memories; the second run's domain ids match the first's (via centroid-similarity stable-ID matching above 0.85).

### Dashboard UI tests (`tests/phase_4/ui/`)

Use curl-driven smoke tests (avoids adding Playwright as a new hard dep; Playwright already exists at `apps/dashboard/playwright.config.ts` and can be extended later).

- `domains_list_renders.sh` -- `curl -H "X-API-Key: $KEY" http://localhost:3927/api/v1/domains` returns 200 + JSON array with expected keys.
- `domain_detail_histogram.sh` -- `curl .../api/v1/domains/dev/score-histogram` returns 10 buckets.
- `proposal_review_flow.sh` -- create a pending proposal via SQL insert; `curl POST .../api/v1/domains/proposals/<id>/accept`; `curl GET .../proposals?status=accepted` shows it.
- `unauth_domain_list_rejected.sh` -- no auth header -> 401.

### Benchmarks (`tests/phase_4/benches/`)

Criterion benches:

- `bench_discover_10k.rs` -- synthetic 10k x 768D embeddings drawn from 5 blobs; assert `discover` wall p95 < 30s on a warm release build.
- `bench_auto_classify_single.rs` -- 20 domains in memory, classify one 768D vector; assert p99 < 5ms.
- `bench_reassign_all.rs` -- 10k memories, 5 domains; assert full `reassign_all` wall time < 90s (100 rows/ms baseline).

---

## Acceptance Criteria

- [ ] `cargo build -p vestige-core --features domain-classification` zero warnings.
- [ ] `cargo build -p vestige-mcp` zero warnings.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- [ ] `cargo test -p vestige-phase4-tests` -- all tests in `tests/phase_4/` pass.
- [ ] On a 500+ memory seed corpus covering three natural clusters (dev / infra / home), `vestige domains discover --force` produces sensible top-terms matching the expected keyword sets and labels are stable on a second run.
- [ ] `vestige search` with domain filter `["dev"]` excludes any memory whose `domains` array does not include `dev`.
- [ ] After 5 dream cycles with ongoing inserts, no existing memory's `domains` has silently changed; proposals exist in `domain_proposals` table; accepting a proposal reassigns as described.
- [ ] Cross-domain spreading activation: a query in `dev` that crosses a single edge into an `infra`-only memory still returns the memory but with activation `cross_domain_decay * in-domain_activation`.
- [ ] `vestige domains discover --min-cluster-size 20` produces strictly fewer or equal clusters than the default, and with larger per-cluster membership.
- [ ] Dashboard `/dashboard/domains` route renders all domains within 2 seconds on the seed corpus.
- [ ] Proposal UI flow (open pending, accept, confirmed in store) works end-to-end.
- [ ] Benchmarks meet targets (discover 10k p95 < 30s, auto-classify p99 < 5ms).

---

## Rollback Notes

- **Feature gate**: add `domain-classification` to `crates/vestige-core/Cargo.toml`'s `[features]`. When disabled, the `DomainClassifier` module is not compiled, the classification call in the ingest path is a no-op (`#[cfg]`-guarded), and cross-domain decay collapses to `1.0`. The CLI `domains` subcommand emits "domain classification is disabled in this build".
- **Revert strategy**: drop the two new tables `domains` (if created in Phase 1 is retained) or `domain_proposals` (Phase 4). A DOWN migration clears `memories.domains` and `memories.domain_scores`. Existing memories simply lose their domain assignments; all search and retrieval paths work unchanged because `domains = []` is the documented "unclassified" state.
- **Idempotency**: rerunning `discover` is always safe. Cluster numeric IDs may differ between runs, but the stable-ID match by centroid similarity preserves user-assigned labels. Do not persist cluster ids in client-side bookmarks; link via the user-assigned label.
- **Data-loss risk**: `apply_proposal` is a destructive operation (it deletes the old parent domain in a split or merges two). The dashboard's accept button double-confirms with a modal that shows the number of affected memories.

---

## Open Implementation Questions

Each question + candidates + RECOMMENDATION.

### OQ1. Top-terms extraction: TF-IDF vs BM25 vs frequency?
- TF-IDF with smoothed IDF -- standard, cheap, good-enough.
- BM25 -- better for long-document discrimination, overkill for short memory contents.
- Raw frequency -- noisy; stop-words dominate.
**RECOMMENDATION**: TF-IDF with global IDF over the entire memory corpus (not just cluster members), recomputed once per `discover` call. Same tokenizer as the `dreams.rs::content_similarity` Jaccard for consistency.

### OQ2. Proposal persistence: DB table vs in-memory with dashboard notification?
- DB table (`domain_proposals`) -- durable, surfaces across restarts, enables audit.
- In-memory only -- simpler, but loses proposals on server restart.
**RECOMMENDATION**: DB table. Proposals are rare (every 5th dream) and valuable user-facing artifacts; durability is mandatory.

### OQ3. `hdbscan` crate: f32 vs f64 input, exact API surface?
- v0.10 historically takes `&[Vec<f64>]`; embeddings are `Vec<f32>`.
- Cost of converting f32 -> f64 at discovery time: `10k * 768 = 7.68M` f64 doubles ~ 60MB transient, acceptable.
**RECOMMENDATION**: verify v0.10's type signature at implementation time; if it requires f64, perform the conversion in `discover()` behind a single allocation. Document in module header. If the crate API diverged from the PRD snippet, fall back to the manual builder style (`HdbscanHyperParams::builder().min_cluster_size(n).min_samples(s).build()`).

### OQ4. Stable domain IDs across discover re-runs?
- Option A: numeric IDs from HDBSCAN labels -- unstable, re-runs shuffle them.
- Option B: hash(top_terms) -- stable if top-terms stable, but top-terms drift.
- Option C (recommended): after computing new centroids, match each to the closest existing domain by centroid cosine; if similarity > 0.85, reuse the existing domain's `id` and `label`. Otherwise mint a fresh `id = "cluster_<uuid>"`.
**RECOMMENDATION**: Option C. Preserves user-assigned labels across drift. Threshold 0.85 is config-tunable via `stable_id_threshold` if needed later.

### OQ5. Context signal injection site: ingest handler vs embedder vs classifier?
- Embedder -- would alter embedding; signals are not about embedding quality.
- Ingest handler -- signals known there, but then `DomainClassifier` cannot be tested in isolation.
- Classifier as a `classify_with_boost(boost: Option<&HashMap>)` parameter -- pure, testable, composable.
**RECOMMENDATION**: classifier parameter. The cognitive engine constructs the boost map via `ContextSignals::gather_boost(&metadata, &domains)` and hands it to the classifier. Keeps the classifier stateless w.r.t. signals.

### OQ6. Re-cluster proposal cadence: event-based (every Nth dream) vs time-based (weekly)?
- ADR resolution Q7: every Nth dream (N=5 default).
- Alternative: once per week regardless of dream cadence.
**RECOMMENDATION**: stick with every Nth dream. Users who dream rarely re-cluster rarely -- that matches the philosophy ("memory work triggers memory bookkeeping"). Note the alternative as future consideration; if users complain about never seeing proposals, add a time-based fallback.

### OQ7. Minimum corpus size for first discover?
- PRD default: 150.
- Too low -> noisy initial clusters, proposals every dream.
- Too high -> user waits forever for domains to appear.
**RECOMMENDATION**: 150 as the default discovery gate; HDBSCAN's `min_cluster_size=10` will produce 0 clusters for < 100 memories, so the system gracefully produces no domains until the corpus is large enough. Test with `N=80, 150, 500` in `threshold_tunable.rs` to confirm sensible behavior.

### OQ8. Cross-domain decay: strict no-overlap vs graded?
- Strict: `1.0` if any overlap, `cross_domain_decay` otherwise.
- Graded: `max(cross_domain_decay, |A intersect B| / max(|A|, |B|))`.
**RECOMMENDATION**: strict for Phase 4. Easier to reason about, easier to tune, easier to test. Graded is a marked future enhancement; file an issue if retrieval-quality metrics justify it.

### OQ9. Classifier invocation from remote HTTP clients?
- In server mode, an agent posts `smart_ingest` -> server embeds -> server classifies.
- All the work stays server-side; MCP clients never do classification.
**RECOMMENDATION**: confirmed server-side-only. Document in the MCP tool schema that `smart_ingest` now returns `domains` and `domain_scores` in its response so clients can display the classification to the user.

### OQ10. Where to store the dream-cycle counter?
- In-memory on `CognitiveEngine` -- lost on restart, miscounts cadence.
- New `system_state` singleton table.
**RECOMMENDATION**: `system_state` table. Survives restarts. Also useful for future metrics (total memories ever, total dreams ever).

### OQ11. Scope of `reassign_all` after a proposal accept vs a normal discover?
- On discover --force (first-time), run `reassign_all` against all memories.
- On proposal accept (split / merge), run `reassign_all` only on affected memories (parent's members for split; both parents' members for merge) to avoid touching unrelated records.
**RECOMMENDATION**: scoped reassignment where possible; fall back to full `reassign_all` only on `discover --force` or when the set of domains has fundamentally changed. Reduces write amplification on large corpora.

### OQ12. Proposal freshness?
- Multiple re-clusters could stack up pending proposals.
**RECOMMENDATION**: before inserting a new proposal, check for existing pending proposals with the same `kind + targets`; if present, bump `created_at` and `confidence` instead of creating a duplicate. Add a `confidence_history` array in the `payload` JSONB for audit.

---

## Implementation Sequencing (suggested order)

1. Land the `DomainClassifier` struct, `classify` / `classify_with_boost`, unit tests. (Day 1)
2. Add `compute_top_terms` + TF-IDF helper, tests. (Day 1)
3. Wire `discover` end-to-end against SQLite; `discover_seed_corpus` integration test. (Day 2)
4. Add `domain_proposals` table migrations + trait methods; both backends. (Day 2)
5. Implement `propose_changes` + `apply_proposal`; proposal unit tests. (Day 3)
6. Context signals module + tests. (Day 3)
7. Hook classifier into ingest path; `auto_classify_on_ingest` integration test. (Day 4)
8. Cross-domain decay in spreading activation; unit + integration tests. (Day 4)
9. Dream re-cluster hook + `system_state` counter; integration tests for drift-only behavior. (Day 5)
10. CLI subcommands. (Day 6)
11. REST endpoints. (Day 6)
12. SvelteKit dashboard routes + WebSocket event wiring. (Day 7-8)
13. Benchmarks + acceptance sweep on the 500-memory seed. (Day 9)

---

## File Map (everything Phase 4 touches or creates)

Creates:

- `crates/vestige-core/src/neuroscience/domain_classifier.rs`
- `crates/vestige-core/src/neuroscience/context_signals.rs`
- `crates/vestige-core/migrations/postgres/00XX_domain_proposals.sql`
- `crates/vestige-core/migrations/sqlite/00XX_domain_proposals.sql` (or inline in `storage/migrations.rs`)
- `crates/vestige-mcp/src/api/domains.rs` (REST handlers)
- `apps/dashboard/src/routes/(app)/domains/+page.svelte`
- `apps/dashboard/src/routes/(app)/domains/[id]/+page.svelte`
- `apps/dashboard/src/routes/(app)/domains/proposals/+page.svelte`
- `apps/dashboard/src/lib/api/domains.ts`
- `tests/phase_4/Cargo.toml`
- `tests/phase_4/tests/*.rs` (per the Integration test list)
- `tests/phase_4/fixtures/seed_500.json`
- `tests/phase_4/support/fixtures.rs`

Modifies:

- `crates/vestige-core/Cargo.toml` -- add `hdbscan = "0.10"` under a new `domain-classification` feature.
- `crates/vestige-core/src/neuroscience/mod.rs` -- register new modules, re-exports.
- `crates/vestige-core/src/neuroscience/spreading_activation.rs` -- `cross_domain_decay` field in `ActivationConfig`, `domains` field on `ActivationNode`, decay math in `activate`.
- `crates/vestige-core/src/consolidation/phases.rs` -- `DreamReClusterHook`.
- `crates/vestige-core/src/advanced/dreams.rs` -- accept a hook callback from the orchestrator (if the orchestration is done at this level).
- `crates/vestige-core/src/storage/trait.rs` -- add proposal + system_state methods.
- `crates/vestige-core/src/storage/sqlite.rs` -- implement proposal + system_state methods + `all_embeddings_with_meta` if not already on the trait.
- `crates/vestige-core/src/storage/postgres.rs` (Phase 2) -- same.
- `crates/vestige-core/src/lib.rs` -- re-exports.
- `crates/vestige-core/src/cognitive.rs` (or equivalent ingest orchestrator) -- auto-classify injection.
- `crates/vestige-mcp/src/bin/cli.rs` -- `Domains` subcommand + dispatch.
- `crates/vestige-mcp/src/dashboard/mod.rs` -- wire new REST routes.
- `crates/vestige-mcp/src/dashboard/events.rs` -- new event variants.
- `crates/vestige-mcp/src/dashboard/handlers.rs` -- if legacy dashboard gets a domains panel (optional).
- `vestige.toml` config loader -- `[domains]` section + struct + defaults.
- Root `Cargo.toml` workspace members -- add `tests/phase_4`.

---

## Risks

- **HDBSCAN determinism**: HDBSCAN is deterministic given input order; sorting embeddings by memory id before feeding the clusterer guarantees reproducibility across runs -- do this in `discover()` and document it.
- **Embedding dimension drift**: Phase 1's `embedding_model` registry blocks writes from mismatched embedders. If `discover()` ever sees two dimensions, it bails with a clear error and points at `vestige migrate --reembed`.
- **Classification latency on ingest**: for users with thousands of domains (unlikely but possible), `classify` is O(n_domains * dim). 20 domains * 768 f32 = 15k flops per classification, trivial. Still, expose a `classify_budget_ms` config knob for paranoia.
- **Re-cluster proposal storms**: if the corpus is borderline-stable, small changes can produce conflicting proposals on consecutive dreams. Mitigation: OQ12 (dedup by target set, bump confidence instead of stacking).
- **Dashboard feature gap**: if the SvelteKit app lands with the domains route but the REST endpoints are not yet deployed, the route 404s. Mitigation: ship the REST endpoints in the same release; a feature flag on the client toggles the nav entry.

---

## Non-Goals Reminder

- No Phase 5 federation concerns in this plan.
- No cross-installation domain sync.
- No automatic accept of proposals, ever.
- No graded cross-domain decay; strict only.
- No ML-based domain label suggestion (top-terms are enough for v1).
- No editing individual memory memberships from the UI in this phase.
