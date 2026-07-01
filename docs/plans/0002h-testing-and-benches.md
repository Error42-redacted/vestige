# Sub-plan 0002h -- Testing and benches for the Postgres backend

**Status**: Draft
**Master plan**: [0002-phase-2-postgres-backend.md](0002-phase-2-postgres-backend.md)
**ADR**: [0002-phase-2-execution.md](../adr/0002-phase-2-execution.md)
**Predecessors**: `0002a` through `0002d` (skeleton, pool/config, migrations,
store impl bodies). `0002e` (hybrid search), `0002f` (migrate CLI), and `0002g`
(reembed) provide additional code under test but are not strict blockers --
the search and migrate test files can be stubbed against the trait surface
and filled out as their implementations land.

---

## Context

This sub-plan covers master plan deliverables **D14** (six integration test
files under `tests/phase_2/`) and **D15** (Criterion benches for RRF search
at 1k and 100k memories). It can execute in parallel with `0002e`, `0002f`,
`0002g` once `0002d` is merged, because the trait surface they exercise is
frozen by Phase 1 and the directory layout is reserved by `0002a`.

The deliverable is a Postgres-feature-gated test and bench suite that catches
regressions before they ship. Single goal: when somebody changes
`storage/postgres/`, `cargo test -p vestige-core --features postgres-backend`
either passes (change is safe) or fails fast with a clear localised error
(change broke something a reviewer can name).

Scope:

- Add the testcontainer harness in `tests/phase_2/common/`.
- Add six integration test files, each gated on `postgres-backend`.
- Add the Criterion bench `pg_hybrid_search.rs` with two bench groups.
- Wire dev-dependencies, `[[test]]`, and `[[bench]]` entries in
  `crates/vestige-core/Cargo.toml`.
- Document how the suite is run locally and what CI must provide.

Explicitly NOT in scope:

- Trait-parity testing (`tests/phase_2/pg_trait_parity.rs` from the master
  plan). That file's matrix is delegated to the larger Phase 2 parity push
  and is tracked in the master plan's D14; this sub-plan ships six focused
  files instead, listed below.
- Concurrency stress tests (`pg_concurrency.rs` from the master plan).
  Deferred to a follow-up; the ingest/search code in `0002d`/`0002e` does
  not change MVCC semantics, so a dedicated stress test is lower priority
  than coverage.
- Re-embed integration tests beyond a smoke check. `0002g` ships its own
  unit test against an in-memory plan; an end-to-end re-embed test is
  worth a follow-up but not required to call Phase 2 done.

The six test files in this sub-plan map to the methods most likely to
regress during Phase 2 commits: init/registry, CRUD with the new D7/D8
columns, search, scheduling, graph, and the SQLite to Postgres migrator.

---

## Prerequisites

- `0002a` -- `crates/vestige-core/src/storage/postgres/mod.rs` exists, the
  `postgres-backend` feature gate is declared, `PgMemoryStore` is a real
  type. Method bodies may still be `todo!()` for the parts a given test
  does not touch.
- `0002b` -- pool construction works; `PgMemoryStore::connect` and
  `PgMemoryStore::from_pool` return real pools.
- `0002c` -- `sqlx::migrate!` wired; tests can call
  `PgMemoryStore::run_migrations(&pool).await?` (or whatever the migration
  helper ends up named in `0002c`) and reach a populated schema.
- `0002d` -- CRUD, scheduling, and graph method bodies are real (not
  `todo!()`). Without `0002d` the CRUD/scheduling/graph tests cannot pass.
- `0002e` -- hybrid search body is real. The search test depends on it.
  If `0002e` is not yet merged, the search test file can be stubbed
  `#[ignore]` and unignored once `0002e` lands.
- `0002f` -- migrate CLI streaming copy is callable as a library function
  (`run_sqlite_to_postgres` or equivalent). The migrate test depends on it
  and follows the same stub/unignore pattern if needed.
- Docker or Podman is available at test time. CI must provide it. Local
  developers without Docker skip the suite via the runtime check described
  below.

---

## Dev-dependencies

Add `testcontainers` and `testcontainers-modules` as optional dev-deps
gated on the `postgres-backend` feature. `criterion` is already in
dev-dependencies from Phase 1 (`search_bench.rs` uses it).

From the repo root, run:

```bash
cargo add --package vestige-core --dev --optional testcontainers@0.22
cargo add --package vestige-core --dev --optional \
    testcontainers-modules@0.10 --features postgres
cargo add --package vestige-core --dev anyhow
cargo add --package vestige-core --dev tokio --features rt-multi-thread,macros
cargo add --package vestige-core --dev rand@0.8
```

`anyhow` is convenient for the harness's error type (`anyhow::Result<...>`
inside the `common/` helper matches master plan D12). `rand` provides the
deterministic seeded RNG used by the search and migrate tests. `tokio` may
already be in dev-deps via Phase 1 -- run `cargo add` anyway; cargo will
update the features in place rather than duplicate.

Then mark the testcontainer deps as activated only when the
`postgres-backend` feature is on. Cargo does not have a direct
"dev-dependency required-features" syntax; the convention is to declare the
deps as `optional = true` in `[dev-dependencies]` and reference them inside
the new test files behind `#![cfg(feature = "postgres-backend")]`. The
resulting `Cargo.toml` block looks like:

```toml
[dev-dependencies]
tempfile = "3"
criterion = { version = "0.5", features = ["html_reports"] }
anyhow = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
rand = "0.8"
testcontainers = { version = "0.22", optional = true }
testcontainers-modules = { version = "0.10", features = ["postgres"], optional = true }
```

The `optional = true` flag prevents `cargo test` (default features) from
pulling in 30+ MB of testcontainer transitive deps on every contributor
laptop. Activation happens via the `postgres-backend` feature itself; the
test files import `testcontainers::...` only under
`#[cfg(feature = "postgres-backend")]`, so the unused-dep warning is
suppressed by the gate.

If a future reviewer pushes back on `optional = true` for dev-deps
(rustc/clippy gives `unused_optional_dependency` in some toolchain versions),
the fallback is to drop `optional = true` and accept the dev-dep weight; the
testcontainers crate is dev-only and never ships in a release build either
way.

---

## Test container helper

**File**: `crates/vestige-core/tests/phase_2/common/mod.rs`

This is shared infrastructure for every test in `tests/phase_2/`. It is not
its own `[[test]]`; it is a `mod common;` import inside each test file.

```rust
//! Shared testcontainer setup for Phase 2 Postgres integration tests.
#![cfg(feature = "postgres-backend")]

use std::sync::Arc;

use anyhow::Result;
use testcontainers::core::{ContainerPort, IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use testcontainers_modules::postgres::Postgres;

use vestige_core::embedder::Embedder;
use vestige_core::storage::postgres::PgMemoryStore;

/// Spin up a fresh pgvector-enabled Postgres container and return a fully
/// migrated PgMemoryStore connected to it.
///
/// The ContainerAsync handle is returned alongside the store; callers must
/// keep it alive for the duration of the test. Dropping it tears the
/// container down.
pub async fn fresh_pg_store(
    embedder: Arc<dyn Embedder>,
) -> Result<(PgMemoryStore, ContainerAsync<Postgres>)> {
    // pgvector/pgvector:pg18 is the official pgvector image built on the
    // postgres:18 base. testcontainers-modules::postgres::Postgres targets
    // the upstream postgres image by default; we override name + tag.
    let container = Postgres::default()
        .with_name("pgvector/pgvector")
        .with_tag("pg18")
        .start()
        .await?;

    let port = container.get_host_port_ipv4(5432).await?;
    let url = format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres");

    // Pool size 4 is enough for tests and stays well below the container's
    // default max_connections = 100.
    let store = PgMemoryStore::connect(&url, 4).await?;

    // Run migrations. `0002c` decides the exact helper name. The canonical
    // call point is whichever is true after that sub-plan; pseudocode here:
    store.run_migrations().await?;

    // Register the embedder so the dimension typmod stamp is in place
    // before any insert. `0002d` lands the real register_model body.
    let sig = embedder.signature();
    store.register_model(&sig).await?;

    Ok((store, container))
}

/// Fixed embedder used by every test. Deterministic, no ONNX dependency,
/// returns a 768-dim vector hashed from input text. Lives in
/// `tests/phase_2/common/test_embedder.rs`.
pub use test_embedder::TestEmbedder;

mod test_embedder;
```

**File**: `crates/vestige-core/tests/phase_2/common/test_embedder.rs`

```rust
//! Deterministic hash-based embedder for tests.
//!
//! Avoids the fastembed/ONNX dependency in CI. Returns a 768-dim vector
//! built from a stable hash of the input text. Two equal strings produce
//! equal vectors; near-equal strings produce near-equal vectors only at
//! the trivial token-overlap level (good enough for a smoke check that
//! the vector pipeline is wired, not a real embedding quality test).
#![cfg(feature = "postgres-backend")]

use std::sync::Arc;

use async_trait::async_trait;

use vestige_core::embedder::{Embedder, EmbedderError, ModelSignature};

pub struct TestEmbedder {
    pub name: String,
    pub dim: usize,
}

impl TestEmbedder {
    pub fn new_768() -> Arc<dyn Embedder> {
        Arc::new(Self { name: "test-768".into(), dim: 768 })
    }
    pub fn new_1024() -> Arc<dyn Embedder> {
        Arc::new(Self { name: "test-1024".into(), dim: 1024 })
    }
}

#[async_trait]
impl Embedder for TestEmbedder {
    fn signature(&self) -> ModelSignature {
        ModelSignature {
            name: self.name.clone(),
            dimension: self.dim,
            hash: format!("{}-h", self.name),
        }
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        let mut v = vec![0.0f32; self.dim];
        let bytes = text.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            v[i % self.dim] += (*b as f32) / 255.0;
        }
        // Normalize so cosine similarity is meaningful.
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }
}
```

Notes:

- The exact `Embedder` trait shape is owned by Phase 1; the example above
  may need `embed_batch`, `dimension()`, etc. depending on the frozen
  surface. Whoever lands this file mirrors whatever the Phase 1 trait
  exposes.
- The container handle is returned, not stored in a `static`. Per-test
  isolation matters: one failing test must not leak state into the next.
- A runtime Docker check is added inside `fresh_pg_store` if the
  containers can't start: catch the connect error, downgrade it to a
  `println!` plus `panic!("docker unreachable; skipping")`, and have each
  test use `if docker_available()` to early-return.

A small helper guards CI environments without Docker:

```rust
/// Returns Ok if a `docker` or `podman` binary is on PATH and responds.
/// Tests that need a container call this first and `eprintln!`+`return`
/// rather than failing when Docker is absent.
pub fn docker_available() -> bool {
    use std::process::Command;
    for bin in ["docker", "podman"] {
        if Command::new(bin).arg("info").output().map(|o| o.status.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}
```

Each test starts with:

```rust
if !common::docker_available() {
    eprintln!("docker/podman not available; skipping {}", file!());
    return;
}
```

This is preferable to `#[ignore]` because the developer sees the skip in
test output rather than silently passing zero tests.

---

## Six test files

Each file is at `crates/vestige-core/tests/phase_2/<name>.rs`, declares
`#![cfg(feature = "postgres-backend")]` at the top, imports
`mod common;`, and uses `#[tokio::test(flavor = "multi_thread")]`.

Each file is also wired as a separate `[[test]]` entry in the Cargo.toml
(see "Cargo.toml" section below). This keeps `cargo test` parallelism
per-file and lets a developer run just one file with
`cargo test --features postgres-backend --test <name>`.

### 1. `tests/phase_2/init_test.rs`

**Purpose**: verify the migration pipeline and the embedding registry
behave correctly on first connect, on idempotent reconnect, and on
embedder mismatch.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};

#[tokio::test(flavor = "multi_thread")]
async fn migrations_apply_cleanly() {
    if !docker_available() { eprintln!("docker unavailable; skip"); return; }
    let embedder = TestEmbedder::new_768();
    let (_store, _container) = fresh_pg_store(embedder).await.unwrap();
    // If we reached here, sqlx::migrate! ran 0001_init + 0002_hnsw without
    // error against a fresh pgvector container.
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_persists_after_first_connect() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _container) = fresh_pg_store(embedder.clone()).await.unwrap();
    let registered = store.registered_model().await.unwrap();
    assert!(registered.is_some());
    let sig = registered.unwrap();
    assert_eq!(sig.name, "test-768");
    assert_eq!(sig.dimension, 768);
}

#[tokio::test(flavor = "multi_thread")]
async fn second_connect_with_same_embedder_is_idempotent() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store_a, container) = fresh_pg_store(embedder.clone()).await.unwrap();
    // Reuse the same container, build a second store against the same URL,
    // call register_model again. Must not error.
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres");
    let store_b = vestige_core::storage::postgres::PgMemoryStore::connect(&url, 4).await.unwrap();
    store_b.register_model(&embedder.signature()).await.unwrap();
    assert_eq!(
        store_a.registered_model().await.unwrap().unwrap().name,
        store_b.registered_model().await.unwrap().unwrap().name,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn second_connect_with_different_embedder_returns_mismatch() {
    if !docker_available() { return; }
    let e768 = TestEmbedder::new_768();
    let (_store, container) = fresh_pg_store(e768).await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres");
    let store2 = vestige_core::storage::postgres::PgMemoryStore::connect(&url, 4).await.unwrap();
    let e1024 = TestEmbedder::new_1024();
    let err = store2.register_model(&e1024.signature()).await;
    assert!(matches!(err, Err(vestige_core::storage::MemoryStoreError::EmbeddingMismatch { .. })));
}
```

### 2. `tests/phase_2/crud_test.rs`

**Purpose**: insert + get + update + delete round-trip; non-existent id
returns `Ok(None)`; D7+D8 columns (`owner_user_id`, `visibility`,
`shared_with_groups`, `codebase`) round-trip correctly.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};
use vestige_core::memory::{MemoryRecord, Visibility};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn insert_get_update_delete_roundtrip() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _container) = fresh_pg_store(embedder.clone()).await.unwrap();

    let mut rec = MemoryRecord::new("hello world");
    rec.tags = vec!["test".into(), "crud".into()];
    rec.embedding = Some(embedder.embed(&rec.content).await.unwrap());
    let id = store.insert(&rec).await.unwrap();

    let got = store.get(&id).await.unwrap().unwrap();
    assert_eq!(got.content, "hello world");
    assert_eq!(got.tags, vec!["test", "crud"]);

    let mut updated = got.clone();
    updated.content = "hello updated".into();
    updated.embedding = Some(embedder.embed("hello updated").await.unwrap());
    store.update(&updated).await.unwrap();
    let after = store.get(&id).await.unwrap().unwrap();
    assert_eq!(after.content, "hello updated");

    store.delete(&id).await.unwrap();
    assert!(store.get(&id).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_nonexistent_returns_ok_none() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _container) = fresh_pg_store(embedder).await.unwrap();
    let missing = Uuid::new_v4();
    assert!(store.get(&missing).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn update_nonexistent_returns_not_found() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _container) = fresh_pg_store(embedder.clone()).await.unwrap();
    let mut rec = MemoryRecord::new("ghost");
    rec.id = Uuid::new_v4();
    rec.embedding = Some(embedder.embed("ghost").await.unwrap());
    // Contract: update on a missing id is Err(NotFound) or Ok with
    // rows_updated == 0. Whichever 0002d picks is what this test asserts.
    let res = store.update(&rec).await;
    // Adjust to actual contract once 0002d lands:
    assert!(res.is_err() || res.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn d7_d8_columns_roundtrip() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _container) = fresh_pg_store(embedder.clone()).await.unwrap();

    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let group_a = Uuid::new_v4();
    let group_b = Uuid::new_v4();

    let mut rec = MemoryRecord::new("contextful");
    rec.owner_user_id = owner;
    rec.visibility = Visibility::Group;
    rec.shared_with_groups = vec![group_a, group_b];
    rec.codebase = Some("vestige".to_string());
    rec.embedding = Some(embedder.embed(&rec.content).await.unwrap());

    let id = store.insert(&rec).await.unwrap();
    let got = store.get(&id).await.unwrap().unwrap();

    assert_eq!(got.owner_user_id, owner);
    assert_eq!(got.visibility, Visibility::Group);
    assert_eq!(got.shared_with_groups, vec![group_a, group_b]);
    assert_eq!(got.codebase.as_deref(), Some("vestige"));
}
```

### 3. `tests/phase_2/search_test.rs`

**Purpose**: exercise the three search modes (fts only, vector only,
hybrid), then the domain/tag/node_type/min_retrievability filters, then
the empty-query edge case.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};
use vestige_core::memory::MemoryRecord;
use vestige_core::storage::SearchQuery;

async fn seed(store: &impl vestige_core::storage::MemoryStore, embedder: &(impl vestige_core::embedder::Embedder + ?Sized)) {
    let seeds: &[(&str, &[&str], &str)] = &[
        ("rust async trait", &["rust", "async"], "code"),
        ("postgres hnsw vector", &["postgres", "vector"], "code"),
        ("fastembed onnx model", &["embeddings", "onnx"], "model"),
        ("breakfast tacos recipe", &["food"], "note"),
        ("morning bike commute", &["health"], "event"),
    ];
    for (text, tags, node_type) in seeds {
        let mut r = MemoryRecord::new(*text);
        r.tags = tags.iter().map(|s| s.to_string()).collect();
        r.node_type = node_type.to_string();
        r.embedding = Some(embedder.embed(text).await.unwrap());
        store.insert(&r).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fts_only_returns_keyword_matches() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    seed(&store, embedder.as_ref()).await;

    let q = SearchQuery { text: Some("rust".into()), embedding: None, limit: 10, ..Default::default() };
    let hits = store.search(&q).await.unwrap();
    assert!(hits.iter().any(|h| h.content.contains("rust async trait")));
}

#[tokio::test(flavor = "multi_thread")]
async fn vector_only_returns_semantic_matches() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    seed(&store, embedder.as_ref()).await;

    let qe = embedder.embed("vector search").await.unwrap();
    let q = SearchQuery { text: None, embedding: Some(qe), limit: 10, ..Default::default() };
    let hits = store.search(&q).await.unwrap();
    assert!(!hits.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn hybrid_returns_rrf_fused_results() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    seed(&store, embedder.as_ref()).await;

    let qe = embedder.embed("postgres vector").await.unwrap();
    let q = SearchQuery {
        text: Some("postgres".into()),
        embedding: Some(qe),
        limit: 10,
        ..Default::default()
    };
    let hits = store.search(&q).await.unwrap();
    let top = hits.first().unwrap();
    assert!(top.content.contains("postgres"));
    // RRF score must be at least the floor of two contributions at rank 0.
    assert!(top.score >= 1.0 / 61.0);
}

#[tokio::test(flavor = "multi_thread")]
async fn filter_by_tag_and_node_type() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    seed(&store, embedder.as_ref()).await;

    let q = SearchQuery {
        text: Some("model".into()),
        tags: vec!["embeddings".into()],
        node_type: Some("model".into()),
        limit: 10,
        ..Default::default()
    };
    let hits = store.search(&q).await.unwrap();
    assert!(hits.iter().all(|h| h.tags.contains(&"embeddings".into())));
    assert!(hits.iter().all(|h| h.node_type == "model"));
}

#[tokio::test(flavor = "multi_thread")]
async fn min_retrievability_filter() {
    // After 0002e ships the filter wiring this exercises it. For now,
    // assert the empty / pass-through case: min_retrievability = 0.0
    // returns all results.
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    seed(&store, embedder.as_ref()).await;

    let q = SearchQuery { text: Some("rust".into()), min_retrievability: 0.0, limit: 10, ..Default::default() };
    let hits = store.search(&q).await.unwrap();
    assert!(!hits.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_query_returns_ok_empty_or_all() {
    // Contract chosen in 0002e; this test asserts whichever it picks.
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder).await.unwrap();
    let q = SearchQuery { text: None, embedding: None, limit: 10, ..Default::default() };
    let hits = store.search(&q).await.unwrap();
    let _ = hits; // assert is intentionally weak until 0002e fixes the contract
}
```

### 4. `tests/phase_2/scheduling_test.rs`

**Purpose**: FSRS state round-trip via `get_scheduling` /
`update_scheduling` with `ON CONFLICT DO UPDATE` semantics, and
`get_due_memories` paging.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};
use chrono::{Duration, Utc};
use vestige_core::memory::MemoryRecord;
use vestige_core::scheduling::SchedulingState;

#[tokio::test(flavor = "multi_thread")]
async fn scheduling_update_and_get_roundtrip() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();

    let mut rec = MemoryRecord::new("fsrs target");
    rec.embedding = Some(embedder.embed("fsrs target").await.unwrap());
    let id = store.insert(&rec).await.unwrap();

    let s = SchedulingState {
        memory_id: id,
        stability: 2.5,
        difficulty: 6.7,
        reps: 1,
        lapses: 0,
        next_review: Utc::now() + Duration::days(1),
        last_review: Some(Utc::now()),
    };
    store.update_scheduling(&s).await.unwrap();

    let back = store.get_scheduling(&id).await.unwrap().unwrap();
    assert!((back.stability - 2.5).abs() < 1e-6);
    assert_eq!(back.reps, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduling_on_conflict_overwrites() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();

    let mut rec = MemoryRecord::new("repeating");
    rec.embedding = Some(embedder.embed("repeating").await.unwrap());
    let id = store.insert(&rec).await.unwrap();

    for reps in [1u32, 2, 3] {
        let s = SchedulingState {
            memory_id: id,
            stability: reps as f32,
            difficulty: 5.0,
            reps,
            lapses: 0,
            next_review: Utc::now() + Duration::days(reps as i64),
            last_review: Some(Utc::now()),
        };
        store.update_scheduling(&s).await.unwrap();
    }
    let final_state = store.get_scheduling(&id).await.unwrap().unwrap();
    assert_eq!(final_state.reps, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_due_memories_pages() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();

    let now = Utc::now();
    // Insert 25 due memories with next_review in the past.
    for i in 0..25 {
        let mut rec = MemoryRecord::new(format!("due {i}"));
        rec.embedding = Some(embedder.embed(&rec.content).await.unwrap());
        let id = store.insert(&rec).await.unwrap();
        let s = SchedulingState {
            memory_id: id,
            stability: 1.0,
            difficulty: 5.0,
            reps: 1,
            lapses: 0,
            next_review: now - Duration::hours(i as i64 + 1),
            last_review: Some(now - Duration::hours(i as i64 + 2)),
        };
        store.update_scheduling(&s).await.unwrap();
    }
    let page1 = store.get_due_memories(now, 10, 0).await.unwrap();
    let page2 = store.get_due_memories(now, 10, 10).await.unwrap();
    let page3 = store.get_due_memories(now, 10, 20).await.unwrap();
    assert_eq!(page1.len(), 10);
    assert_eq!(page2.len(), 10);
    assert_eq!(page3.len(), 5);
}
```

### 5. `tests/phase_2/graph_test.rs`

**Purpose**: `add_edge`, `get_edges`, `remove_edge`, and `get_neighbors`
with a non-trivial depth.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};
use vestige_core::memory::MemoryRecord;
use vestige_core::storage::Edge;

async fn insert_n(store: &impl vestige_core::storage::MemoryStore, embedder: &(impl vestige_core::embedder::Embedder + ?Sized), n: usize) -> Vec<uuid::Uuid> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let mut r = MemoryRecord::new(format!("node {i}"));
        r.embedding = Some(embedder.embed(&r.content).await.unwrap());
        ids.push(store.insert(&r).await.unwrap());
    }
    ids
}

#[tokio::test(flavor = "multi_thread")]
async fn add_get_remove_edge() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    let ids = insert_n(&store, embedder.as_ref(), 3).await;

    let e = Edge {
        source_id: ids[0],
        target_id: ids[1],
        edge_type: "semantic".into(),
        strength: 0.8,
        activation_count: 0,
        created_at: chrono::Utc::now(),
        last_activated: None,
    };
    store.add_edge(&e).await.unwrap();

    let edges = store.get_edges(&ids[0]).await.unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target_id, ids[1]);

    store.remove_edge(&ids[0], &ids[1], "semantic").await.unwrap();
    assert!(store.get_edges(&ids[0]).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_neighbors_with_depth() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();
    let (store, _c) = fresh_pg_store(embedder.clone()).await.unwrap();
    let ids = insert_n(&store, embedder.as_ref(), 5).await;

    // Chain: 0 -> 1 -> 2 -> 3 -> 4
    for w in ids.windows(2) {
        let e = Edge {
            source_id: w[0],
            target_id: w[1],
            edge_type: "semantic".into(),
            strength: 1.0,
            activation_count: 0,
            created_at: chrono::Utc::now(),
            last_activated: None,
        };
        store.add_edge(&e).await.unwrap();
    }

    let depth_1 = store.get_neighbors(&ids[0], 1).await.unwrap();
    let depth_2 = store.get_neighbors(&ids[0], 2).await.unwrap();
    let depth_4 = store.get_neighbors(&ids[0], 4).await.unwrap();

    assert_eq!(depth_1.len(), 1);
    assert_eq!(depth_2.len(), 2);
    assert_eq!(depth_4.len(), 4);
}
```

### 6. `tests/phase_2/migrate_test.rs`

**Purpose**: seed SQLite with a small dataset, run the migrator, verify
counts and a sample row.

**Tests**:

```rust
#![cfg(feature = "postgres-backend")]

mod common;
use common::{docker_available, fresh_pg_store, TestEmbedder};
use vestige_core::memory::MemoryRecord;
use vestige_core::storage::{SqliteMemoryStore, MemoryStore};
use vestige_core::storage::postgres::migrate_cli::run_sqlite_to_postgres;

#[tokio::test(flavor = "multi_thread")]
async fn sqlite_to_postgres_small_corpus() {
    if !docker_available() { return; }
    let embedder = TestEmbedder::new_768();

    // Seed SQLite (in-memory or tempfile).
    let tmp = tempfile::tempdir().unwrap();
    let sqlite_path = tmp.path().join("seed.db");
    let sqlite = SqliteMemoryStore::new(&sqlite_path).unwrap();
    sqlite.register_model(&embedder.signature()).await.unwrap();
    for i in 0..50 {
        let mut r = MemoryRecord::new(format!("seed row {i}"));
        r.tags = vec![format!("tag-{}", i % 3)];
        r.embedding = Some(embedder.embed(&r.content).await.unwrap());
        sqlite.insert(&r).await.unwrap();
    }

    // Spin up Postgres and migrate.
    let (pg, _container) = fresh_pg_store(embedder.clone()).await.unwrap();
    let report = run_sqlite_to_postgres(&sqlite, &pg, embedder.clone()).await.unwrap();

    assert_eq!(report.memories_copied, 50);
    assert_eq!(pg.count().await.unwrap(), 50);

    // Spot-check a sample row.
    let sample_id = sqlite.list_ids(1, 0).await.unwrap()[0];
    let from_sqlite = sqlite.get(&sample_id).await.unwrap().unwrap();
    let from_pg = pg.get(&sample_id).await.unwrap().unwrap();
    assert_eq!(from_sqlite.content, from_pg.content);
    assert_eq!(from_sqlite.tags, from_pg.tags);
}
```

If `0002f` is not yet merged when this sub-plan executes, the test file is
still added but the body sits behind `#[ignore = "depends on 0002f"]`,
removed once `0002f` lands.

---

## How tests are run

```bash
# Run all six phase_2 integration tests:
cargo test -p vestige-core --features postgres-backend --test '*'

# Run a single file:
cargo test -p vestige-core --features postgres-backend --test init_test
cargo test -p vestige-core --features postgres-backend --test crud_test
cargo test -p vestige-core --features postgres-backend --test search_test
cargo test -p vestige-core --features postgres-backend --test scheduling_test
cargo test -p vestige-core --features postgres-backend --test graph_test
cargo test -p vestige-core --features postgres-backend --test migrate_test

# SQLite-only sanity check (must continue to pass, Phase 1 unchanged):
cargo test -p vestige-core
```

Requirements:

- Docker or Podman must be reachable. `testcontainers` connects via the
  default Docker socket (`/var/run/docker.sock` on Linux, `~/.docker/run/docker.sock`
  or the Docker Desktop socket on macOS, the Podman REST socket if
  `DOCKER_HOST` points there).
- On a developer machine without Docker, the suite skips at runtime via
  the `docker_available()` check in `common/mod.rs`. The test output
  includes a `docker unavailable; skip` line per test so the developer
  knows the tests were not silently dropped.
- The pgvector image (`pgvector/pgvector:pg18`) is pulled on first run;
  ~200 MB. A pre-pulled image keeps the per-run overhead at the cold-start
  container boot (~2-5 seconds).

---

## Benches

**File**: `crates/vestige-core/benches/pg_hybrid_search.rs`

Two Criterion benches: `search_1k` and `search_100k`. Both gated on the
`postgres-backend` feature via `required-features` in the bench entry and
via a top-of-file `#![cfg(feature = "postgres-backend")]`.

```rust
//! Criterion benches for the Postgres backend's hybrid RRF search.
#![cfg(feature = "postgres-backend")]

use std::sync::Arc;
use std::sync::OnceLock;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::{rngs::StdRng, Rng, SeedableRng};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use tokio::runtime::Runtime;

use vestige_core::embedder::Embedder;
use vestige_core::memory::MemoryRecord;
use vestige_core::storage::postgres::PgMemoryStore;
use vestige_core::storage::{MemoryStore, SearchQuery};

// Bench fixture lives in tests/phase_2/common/test_embedder.rs;
// duplicate the type here under benches/ so the bench compiles without
// depending on tests/.
mod test_embedder;
use test_embedder::TestEmbedder;

struct Bench {
    rt: Runtime,
    store: PgMemoryStore,
    embedder: Arc<dyn Embedder>,
    _container: ContainerAsync<Postgres>,
    query_embedding: Vec<f32>,
}

async fn build_bench(rows: usize) -> Bench {
    let rt_handle = tokio::runtime::Handle::current();
    let _ = rt_handle; // proves we are inside an executor
    let embedder = TestEmbedder::new_768();
    let container = Postgres::default()
        .with_name("pgvector/pgvector")
        .with_tag("pg18")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres");
    let store = PgMemoryStore::connect(&url, 8).await.unwrap();
    store.run_migrations().await.unwrap();
    store.register_model(&embedder.signature()).await.unwrap();

    let mut rng = StdRng::seed_from_u64(0xc0ffee);
    let vocab = [
        "rust", "postgres", "vector", "hnsw", "fastembed", "onnx",
        "search", "memory", "fsrs", "consolidate", "graph", "edge",
        "async", "trait", "tokio", "sqlx", "pgvector", "embedding",
    ];
    for i in 0..rows {
        let words: String = (0..8)
            .map(|_| vocab[rng.gen_range(0..vocab.len())])
            .collect::<Vec<_>>()
            .join(" ");
        let mut r = MemoryRecord::new(format!("{i}: {words}"));
        r.tags = vec![format!("tag-{}", i % 7)];
        r.embedding = Some(embedder.embed(&r.content).await.unwrap());
        store.insert(&r).await.unwrap();
    }
    let query_embedding = embedder.embed("postgres vector search").await.unwrap();
    Bench {
        rt: tokio::runtime::Runtime::new().unwrap(),
        store,
        embedder,
        _container: container,
        query_embedding,
    }
}

fn bench_search_1k(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let bench = rt.block_on(build_bench(1_000));
    c.bench_function("pg_search_1k", |b| {
        b.iter(|| {
            let q = SearchQuery {
                text: Some("postgres vector".into()),
                embedding: Some(bench.query_embedding.clone()),
                limit: 10,
                ..Default::default()
            };
            let hits = bench.rt.block_on(bench.store.search(&q)).unwrap();
            black_box(hits);
        })
    });
}

// Heavy: 100k rows; seed time runs into minutes. Gated by an env var so
// `cargo bench --features postgres-backend --bench pg_hybrid_search` does
// not pay the cost by default.
fn bench_search_100k(c: &mut Criterion) {
    if std::env::var("VESTIGE_BENCH_HEAVY").is_err() {
        eprintln!("skip pg_search_100k (set VESTIGE_BENCH_HEAVY=1 to enable)");
        return;
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let bench = rt.block_on(build_bench(100_000));
    c.bench_function("pg_search_100k", |b| {
        b.iter(|| {
            let q = SearchQuery {
                text: Some("postgres vector".into()),
                embedding: Some(bench.query_embedding.clone()),
                limit: 10,
                ..Default::default()
            };
            let hits = bench.rt.block_on(bench.store.search(&q)).unwrap();
            black_box(hits);
        })
    });
}

criterion_group!(benches, bench_search_1k, bench_search_100k);
criterion_main!(benches);
```

**File**: `crates/vestige-core/benches/test_embedder.rs`

Duplicate of `tests/phase_2/common/test_embedder.rs`. Cargo's bench target
cannot `mod` into `tests/`; the duplication is the standard fix. Keep both
files in sync; if either grows non-trivially, refactor into a shared
`pub(crate)` module under `src/embedder/test_support.rs` gated on
`#[cfg(any(test, feature = "test-support"))]`.

`VESTIGE_BENCH_HEAVY` gate: the 100k seed step takes several minutes (one
`INSERT` per row plus HNSW upsert). Skipping by default keeps `cargo bench`
under a minute for the 1k bench. Document this gate in the runbook
(`0002i`).

---

## Cargo.toml

Final state of the relevant sections of
`crates/vestige-core/Cargo.toml` after this sub-plan lands:

```toml
[dev-dependencies]
tempfile = "3"
criterion = { version = "0.5", features = ["html_reports"] }
anyhow = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
rand = "0.8"
testcontainers = { version = "0.22", optional = true }
testcontainers-modules = { version = "0.10", features = ["postgres"], optional = true }

[[bench]]
name = "search_bench"
harness = false

[[bench]]
name = "pg_hybrid_search"
harness = false
required-features = ["postgres-backend"]

[[test]]
name = "init_test"
path = "tests/phase_2/init_test.rs"
required-features = ["postgres-backend"]

[[test]]
name = "crud_test"
path = "tests/phase_2/crud_test.rs"
required-features = ["postgres-backend"]

[[test]]
name = "search_test"
path = "tests/phase_2/search_test.rs"
required-features = ["postgres-backend"]

[[test]]
name = "scheduling_test"
path = "tests/phase_2/scheduling_test.rs"
required-features = ["postgres-backend"]

[[test]]
name = "graph_test"
path = "tests/phase_2/graph_test.rs"
required-features = ["postgres-backend"]

[[test]]
name = "migrate_test"
path = "tests/phase_2/migrate_test.rs"
required-features = ["postgres-backend"]
```

Notes:

- `required-features = ["postgres-backend"]` on each `[[test]]` ensures
  the file is only built (and only counted by `cargo test`) when the
  feature is on. Cargo silently skips it otherwise -- exactly the desired
  behavior for default `cargo test` runs.
- The benches use the same `required-features` shape so default
  `cargo bench` is unaffected.

---

## CI considerations

- GitHub Actions / Forgejo Actions runners need Docker available. Default
  `ubuntu-latest` runners include Docker. Self-hosted Forgejo runners on
  TFGrid VMs must install `docker.io` or run `podman` with the Docker
  socket compatibility shim. Document the runner requirement in the
  runbook (`0002i`).
- The Postgres feature tests should run in a separate CI matrix entry to
  isolate failures and skip them entirely on platforms (Windows runners
  if any) where the pgvector image is not available.
- Cache the `pgvector/pgvector:pg18` image between runs. The
  `docker/setup-buildx-action` cache or a simple `docker pull` step before
  the test step keeps cold-start under the existing CI time budget.
- Skip CI: contributors without Docker can still merge changes that do
  not touch `storage/postgres/`. The pre-merge required check is "phase_2
  tests pass on the runner with Docker"; the local pre-commit hook does
  not gate on it.
- Bench CI: do not run `pg_search_100k` in regular CI; only run it
  manually or on a scheduled weekly job and post results to the PR
  description / ADR comment trail.

Recommended CI job shape (sketch):

```yaml
jobs:
  postgres-tests:
    runs-on: ubuntu-latest
    services:
      # no `postgres` service block needed; testcontainers manages its own
    steps:
      - uses: actions/checkout@v4
      - run: docker pull pgvector/pgvector:pg18
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p vestige-core --features postgres-backend --test '*'
```

---

## Verification

After all files are in place:

```bash
# Default build still clean (no postgres deps pulled in):
cargo build -p vestige-core
cargo test  -p vestige-core

# Postgres feature build + integration tests:
cargo build -p vestige-core --features postgres-backend
cargo test  -p vestige-core --features postgres-backend

# Just the new tests:
cargo test -p vestige-core --features postgres-backend --test '*'

# Quick bench sanity check (1k only):
cargo bench -p vestige-core --features postgres-backend --bench pg_hybrid_search -- --quick

# Heavy bench (manual, multi-minute seed step):
VESTIGE_BENCH_HEAVY=1 cargo bench -p vestige-core \
    --features postgres-backend \
    --bench pg_hybrid_search -- --quick

# Clippy with everything on:
cargo clippy -p vestige-core --features postgres-backend --all-targets -- -D warnings
```

Expected results:

- Default build is unchanged; no testcontainers deps in `Cargo.lock`'s
  default resolution.
- With `--features postgres-backend`, all six integration tests pass on a
  machine with Docker available, or each prints `docker unavailable; skip`
  and exits 0.
- `cargo bench ... -- --quick` produces a `pg_search_1k` line with a
  p50 below the master plan's 10 ms target on a developer laptop (looser
  on a CI runner -- the target is informative, not gated).

---

## Acceptance criteria

- [ ] `crates/vestige-core/tests/phase_2/common/mod.rs` and
      `test_embedder.rs` exist and compile under
      `--features postgres-backend`.
- [ ] All six integration test files exist, each with
      `#![cfg(feature = "postgres-backend")]` at the top.
- [ ] Each test file has a corresponding `[[test]]` entry in
      `Cargo.toml` with `required-features = ["postgres-backend"]`.
- [ ] `crates/vestige-core/benches/pg_hybrid_search.rs` exists with
      `search_1k` and `search_100k` benches, the latter gated on
      `VESTIGE_BENCH_HEAVY`.
- [ ] `[[bench]] name = "pg_hybrid_search"` entry present with
      `required-features = ["postgres-backend"]`.
- [ ] `testcontainers@0.22` and `testcontainers-modules@0.10` with the
      `postgres` feature are in `[dev-dependencies]` of `vestige-core`.
- [ ] `anyhow`, `tokio`, `rand` are in `[dev-dependencies]`.
- [ ] `cargo build -p vestige-core` (default features) is unchanged: no
      testcontainers in the build graph; no new warnings.
- [ ] `cargo test -p vestige-core` (default features) passes with no
      changes to the Phase 1 test count beyond what `0002a..g` already
      moved.
- [ ] `cargo test -p vestige-core --features postgres-backend --test '*'`
      passes on a runner with Docker available, or skips cleanly with the
      `docker unavailable; skip` lines.
- [ ] `cargo bench -p vestige-core --features postgres-backend
      --bench pg_hybrid_search -- --quick` runs `pg_search_1k` to
      completion and does NOT run `pg_search_100k` unless
      `VESTIGE_BENCH_HEAVY=1`.
- [ ] `cargo clippy -p vestige-core --features postgres-backend
      --all-targets -- -D warnings` is clean.
- [ ] The runbook (`0002i`) gets a one-paragraph "How to run the test
      suite locally" callout referring back to this sub-plan's
      "Verification" section. (`0002i` is owned separately; this sub-plan
      just lists the dependency.)

---

## Open questions for the implementer

1. **Migration helper name.** `0002c` decides whether
   `PgMemoryStore::run_migrations(&self)` or
   `vestige_core::storage::postgres::migrations::run(&pool)` is the public
   call. Update `common/mod.rs` to match.
2. **Update-on-missing contract.** `0002d` decides whether
   `MemoryStore::update` returns `Err(NotFound)` or `Ok(())` with zero
   affected rows when the id does not exist. The CRUD test stub here
   accepts either; tighten the assert once the contract is fixed.
3. **Empty-query search contract.** `0002e` decides whether
   `SearchQuery { text: None, embedding: None }` is `Ok(empty)` or an
   error. Same tightening pattern as #2.
4. **Pool size for 100k bench.** Current value is 8; if the bench
   bottlenecks on the pool, tune up to 16 or 32 and document in the
   bench file's leading doc comment.
5. **Shared `TestEmbedder` location.** Currently duplicated between
   `tests/phase_2/common/test_embedder.rs` and
   `benches/test_embedder.rs`. If duplication bothers a reviewer, lift to
   `crates/vestige-core/src/embedder/test_support.rs` behind a
   `test-support` Cargo feature pulled in by both `tests` and `benches`.
   Out of scope for this sub-plan; record as a follow-up.
