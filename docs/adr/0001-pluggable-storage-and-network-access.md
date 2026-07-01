# ADR 0001: Pluggable Storage Backend, Network Access, and Emergent Domains

**Status**: Accepted
**Date**: 2026-04-21
**Related**: [docs/prd/001-getting-centralized-vestige.md](../prd/001-getting-centralized-vestige.md)

---

## Context

Vestige v2.x runs as a per-machine local process: stdio MCP transport, SQLite +
FTS5 + USearch HNSW in `~/.vestige/`, fastembed locally for embeddings. This is
ideal for single-machine single-agent use but blocks three real needs:

- **Multi-machine access** -- same memory brain from laptop, desktop, server
- **Multi-agent access** -- multiple AI clients against one store concurrently
- **Future federation** -- syncing memory between decentralized nodes (MOS /
  Threefold grid)

SQLite's single-writer model and lack of a native network protocol make it
unsuitable as a centralized server. PostgreSQL + pgvector collapses our three
storage layers (SQLite, FTS5, USearch) into one engine with MVCC concurrency,
auth, and replication.

Separately, Vestige today has no notion of domain or project scope -- all memories
share one namespace. For a multi-machine brain, users want soft topical
boundaries ("dev", "infra", "home") without manual tenanting. HDBSCAN clustering
on embeddings produces these boundaries from the data itself.

The PRD at `docs/prd/001-getting-centralized-vestige.md` sketches the full design.
This ADR records the architectural decisions and resolves the open questions from
that document.

---

## Decision

Introduce two new trait boundaries, a network transport layer, and a domain
classification module. All four changes ship in parallel phases.

**Trait boundaries:**

1. `MemoryStore` -- single trait covering CRUD, hybrid search, FSRS scheduling,
   graph edges, and domains. One big trait, not four.
2. `Embedder` -- separate trait for text-to-vector encoding. Storage never calls
   fastembed directly. Callers (cognitive engine locally, HTTP server remotely)
   compute embeddings and pass them into the store.

**Backends:**

- `SqliteMemoryStore` -- existing code refactored behind the trait, no behavior
  change.
- `PgMemoryStore` -- new, using sqlx + pgvector + tsvector. Selectable at runtime
  via `vestige.toml`.

**Network:**

- MCP over Streamable HTTP on the existing Axum server.
- API key auth middleware (blake3-hashed, stored in `api_keys` table).
- Dashboard uses the same API keys for login, then signed session cookies for
  subsequent requests.

**Domain classification:**

- HDBSCAN clustering over embeddings to discover domains automatically.
- Soft multi-domain assignment -- raw similarity scores stored per memory, every
  domain above a threshold is assigned.
- Conservative drift handling -- propose splits/merges, never auto-apply.

---

## Architecture Overview

### Component Breakdown

1. **`Embedder` trait** (new module `crates/vestige-core/src/embedder/`)
   - `async fn embed(&self, text: &str) -> Result<Vec<f32>>`
   - `fn model_name(&self) -> &str`
   - `fn dimension(&self) -> usize`
   - Impls: `FastembedEmbedder` (local ONNX, today), future `JinaEmbedder`,
     `OpenAiEmbedder`, etc.
   - Stays pluggable forever -- no lock-in to fastembed or to nomic-embed-text.

2. **`MemoryStore` trait** (new module `crates/vestige-core/src/storage/trait.rs`)
   - One trait, ~25 methods across CRUD, search, FSRS, graph, domain sections.
   - Uses `trait_variant::make` to generate a `Send`-bound variant for
     `Arc<dyn MemoryStore>` in Axum/tokio contexts.
   - The 29 cognitive modules operate exclusively through this trait. No direct
     SQLite or Postgres access from the modules.

3. **`SqliteMemoryStore`** (refactor of existing `crates/vestige-core/src/storage/sqlite.rs`)
   - Existing rusqlite + FTS5 + USearch code, wrapped behind the trait.
   - Add `domains TEXT[]` equivalent (JSON-encoded array column in SQLite).
   - Add `domain_scores` JSON column.
   - No behavioral change for current users.

4. **`PgMemoryStore`** (new `crates/vestige-core/src/storage/postgres.rs`)
   - `sqlx::PgPool` with compile-time checked queries.
   - pgvector HNSW index for vector search, tsvector + GIN for FTS.
   - Native array columns for `domains`, JSONB for `domain_scores` and `metadata`.
   - Hybrid search via RRF (Reciprocal Rank Fusion) in a single SQL query.

5. **Model registry**
   - Per-database table `embedding_model` with `(name, dimension, hash, created_at)`.
   - Both backends refuse writes from an embedder whose signature doesn't match
     the registered row.
   - Model swap = `vestige migrate --reembed --model=<new>`, O(n) cost, explicit.

6. **`DomainClassifier` cognitive module** (new `crates/vestige-core/src/neuroscience/domain_classifier.rs`)
   - Owns the HDBSCAN discovery pass (using the `hdbscan` crate).
   - Computes soft-assignment scores for every memory against every centroid.
   - Stores raw `domain_scores: HashMap<String, f64>` per memory; thresholds into
     the `domains` array using `assign_threshold` (default 0.65).
   - Runs discovery on demand (`vestige domains discover`) or during dream
     consolidation passes.

7. **HTTP MCP transport** (extension of existing Axum server in `crates/vestige-mcp/src/`)
   - New route `POST /mcp` for Streamable HTTP JSON-RPC.
   - New route `GET /mcp` for SSE (for long-running operations).
   - REST API under `/api/v1/` for direct HTTP clients (non-MCP integrations).
   - Auth middleware validates `Authorization: Bearer ...` or `X-API-Key`, plus
     signed session cookies for dashboard.

8. **Key management** (new `crates/vestige-mcp/src/auth/`)
   - `api_keys` table -- blake3-hashed keys, scopes, optional domain filter,
     last-used timestamp.
   - CLI: `vestige keys create|list|revoke`.

9. **FSRS review event log** (future-proofing for federation)
   - New table `review_events` -- append-only `(memory_id, timestamp, rating,
     prior_state, new_state)`.
   - Current `scheduling` table becomes a materialized view over the event log
     (reconstructible from events).
   - Phase 5 federation merges event logs, not derived state. Zero cost today,
     avoids lock-in tomorrow.

### Data Flow

**Local mode (stdio MCP, unchanged UX):**
```
stdio client -> McpServer -> CognitiveEngine -> FastembedEmbedder -> MemoryStore (SQLite)
```

**Server mode (HTTP MCP, new):**
```
Remote client -> Axum HTTP -> auth middleware -> CognitiveEngine
    -> FastembedEmbedder (server-side) -> MemoryStore (Postgres)
```

The cognitive engine is backend-agnostic. The embedder and the store are both
swappable. The 7-stage search pipeline (overfetch -> cross-encoder rerank ->
temporal -> accessibility -> context match -> competition -> spreading activation)
sits *above* the `MemoryStore` trait and works identically against either backend.

### Orthogonality of HDBSCAN and Reranking

HDBSCAN and the cross-encoder reranker solve different problems and both stay:

- **HDBSCAN** discovers domains by clustering embeddings. Runs once per discovery
  pass. Produces centroids. Used to *filter* search candidates, not to rank them.
- **Cross-encoder reranker** (Jina Reranker v1 Turbo) scores query-document pairs
  at search time. Runs on every search. Produces ranked results.

Domain membership is a filter applied before or during overfetch; reranking runs
on whatever candidate set survives the filter.

---

## Alternatives Considered

| Alternative | Pros | Cons | Why Not |
|-------------|------|------|---------|
| Split into 4 traits (`MemoryStore + SchedulingStore + GraphStore + DomainStore`) | Cleaner interface segregation | Every module holds 4 trait objects, coordinates transactions across them | One trait is fine in Rust; extract sub-traits later if a genuine need appears |
| Embedding computed inside the backend | Simpler call sites for callers | Backend becomes aware of embedding models; can't support remote clients without local fastembed | Keep storage pure; separate `Embedder` trait handles pluggability |
| Unconstrained pgvector `vector` (no dimension) | Flexible for model swaps | HNSW still needs fixed dims at index creation; hides a meaningful migration as "silent" | Fixed dimension per install, explicit `--reembed` migration |
| Dashboard separate auth (cookies only, no keys) | Simpler dashboard UX | Two auth systems to maintain | Shared API keys with session cookie layer on top |
| Auto-tuned `assign_threshold` targeting an unclassified ratio | Adapts to corpus | Hard to debug ("why did this memory change domain?"); magical | Static 0.65 default, config-tunable, dashboard shows `domain_scores` for manual retuning |
| Aggressive drift (auto-reassign memories whose scores drifted) | Always up-to-date domains | Breaks user muscle memory; silent reshuffling | Conservative: always propose, user accepts |
| CRDTs for federation state | Mathematically clean merges | Massive complexity, performance cost, overkill | Defer; design FSRS as event log now so any future sync model works |

---

## Consequences

### Positive

- Single memory brain accessible from every machine.
- Multi-agent concurrent access via Postgres MVCC.
- Natural topical scoping emerges from data, not manual tenants.
- Future embedding model swaps are a config + migration, not a rewrite.
- Federation has a clean on-ramp (event log merge) without committing now.
- The `Embedder` / `MemoryStore` split unlocks other storage backends later
  (Redis, Qdrant, Iroh-backed blob store, etc.) with minimal work.

### Negative

- Operating a Postgres instance is more work than managing a SQLite file.
- Users who stay on SQLite gain nothing from this ADR (but lose nothing either).
- Migration (`vestige migrate --from sqlite --to postgres`) is a sensitive
  operation for users with months of memories -- needs strong testing.
- HDBSCAN + re-soft-assignment runs in O(n) over all embeddings. At 100k+
  memories this starts to matter; manageable but not free.

### Risks

- **Trait abstraction leaks**: a cognitive module might need backend-specific
  behavior (e.g., Postgres triggers for tsvector). Mitigation: keep such logic
  inside the backend impl; the trait stays pure.
  Escalation: if a module genuinely cannot express what it needs through the
  trait, the trait grows, not the module bypasses.
- **Embedding model drift**: users on older fastembed versions silently
  producing slightly different vectors after a fastembed upgrade. Mitigation:
  model hash in the registry, refuse mismatched writes, surface a clear error.
- **Auth misconfiguration**: a user binds to `0.0.0.0` without setting
  `auth.enabled = true`. Mitigation: refuse to start with non-localhost bind
  and auth disabled. Hard error, not a warning.
- **Re-clustering feedback loop**: dream consolidation proposes re-clusters,
  which the user accepts, which changes classifications, which affects future
  retrievals, which affect future dreams. Mitigation: cap re-cluster frequency
  (every 5th dream by default), require explicit user acceptance of proposals.
- **Cross-domain spreading activation weight (0.5 default)**: arbitrary choice;
  could be too aggressive or too lax. Mitigation: config-tunable; instrument
  retrieval quality metrics in the dashboard so the user sees impact.

---

## Resolved Decisions (from Q&A)

| # | Question | Resolution |
|---|----------|------------|
| 1 | Trait granularity | Single `MemoryStore` trait |
| 2 | Embedding on insert | Caller provides; separate `Embedder` trait for pluggability |
| 3 | pgvector dimension | Fixed per install, derived from `Embedder::dimension()` at schema init |
| 4 | Federation sync | Defer algorithm; store FSRS reviews as append-only event log now |
| 5 | Dashboard auth | Shared API keys + signed session cookie |
| 6 | HDBSCAN `min_cluster_size` | Default 10; user reruns with `--min-cluster-size N`; no auto-sweep |
| 7 | Domain drift | Conservative -- always propose splits/merges, never auto-apply |
| 8 | Cross-domain spreading activation | Follow with decay factor 0.5 (tunable) |
| 9 | Assignment threshold | Static 0.65 default, config-tunable, raw `domain_scores` stored for introspection |

---

## Implementation Plan

Five phases, each independently shippable.

### Phase 1: Storage trait extraction
- Define `MemoryStore` and `Embedder` traits in `vestige-core`.
- Refactor `SqliteMemoryStore` to implement `MemoryStore`; no behavior change.
- Refactor `FastembedEmbedder` to implement `Embedder`.
- Add `embedding_model` registry table; enforce consistency on write.
- Add `domains TEXT[]`-equivalent and `domain_scores` JSON columns to SQLite
  (empty for all existing rows).
- Convert all 29 cognitive modules to operate via the traits.
- **Acceptance**: existing test suite passes unchanged. Zero warnings.

### Phase 2: PostgreSQL backend
- `PgMemoryStore` with sqlx, pgvector, tsvector.
- sqlx migrations (`crates/vestige-core/migrations/postgres/`).
- Backend selection via `vestige.toml` `[storage]` section.
- `vestige migrate --from sqlite --to postgres` command.
- `vestige migrate --reembed` command for model swaps.
- **Acceptance**: full test suite runs green against Postgres with a testcontainer.

### Phase 3: Network access
- Streamable HTTP MCP route on Axum (`POST /mcp`, `GET /mcp` for SSE).
- REST API under `/api/v1/`.
- API key table + blake3 hashing + `vestige keys create|list|revoke`.
- Auth middleware (Bearer, X-API-Key, session cookie).
- Refuse non-localhost bind without auth enabled.
- **Acceptance**: MCP client over HTTP works from a second machine; dashboard
  login flow works; unauth requests return 401.

### Phase 4: Emergent domain classification
- `DomainClassifier` module using the `hdbscan` crate.
- `vestige domains discover|list|rename|merge` CLI.
- Automatic soft-assignment pipeline (compute `domain_scores` on ingest, threshold
  into `domains`).
- Re-cluster every Nth dream consolidation (default 5); surface proposals in the
  dashboard.
- Context signals (git repo, IDE) as soft priors on classification.
- Cross-domain spreading activation with 0.5 decay.
- **Acceptance**: on a corpus of 500+ mixed memories, discover produces sensible
  clusters; search scoped to a domain returns tightly relevant results.

### Phase 5: Federation (future, explicitly out of scope for this ADR's
acceptance)
- Node discovery (Mycelium / mDNS).
- Memory sync protocol over append-only review events and LWW-per-UUID for
  memory records.
- Explicit follow-up ADR before any code.

---

## Open Questions

None at ADR acceptance time. Follow-up items that are *implementation choices*,
not architectural:

- Precise cross-domain decay weight (start at 0.5, instrument, tune)
- Dashboard histogram of `domain_scores` (UX design detail)
- Whether to gate Postgres behind a Cargo feature flag (`postgres-backend`) or
  always compile it in (lean toward feature flag to keep SQLite-only builds small)
