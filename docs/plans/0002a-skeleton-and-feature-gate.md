# Phase 2 Sub-Plan 0002a -- Skeleton and Feature Gate

**Status**: Ready
**Depends on**: Phase 1 amendment (sub-plans `0001a-trait-rewrite.md` and
`0001b-sqlite-split.md`) merged. Specifically:
- `MemoryStore` trait declared with `#[trait_variant::make(MemoryStore: Send)]`,
  generating a non-Send `LocalMemoryStore` companion trait. The
  `pub use MemoryStore as LocalMemoryStore` alias from Phase 1 is gone.
- `crates/vestige-core/src/storage/sqlite.rs` has been split into
  `crates/vestige-core/src/storage/sqlite/` with the same public surface.

This sub-plan covers Phase 2 master-plan deliverables D1 and D2 only:
the `postgres-backend` Cargo feature gate and a compilable `PgMemoryStore`
skeleton whose trait method bodies are `todo!()`. No real Postgres code, no
migrations, no SQL. Later sub-plans (`0002b-pool-and-config.md`,
`0002c-migrations.md`, `0002d-store-impl-bodies.md`, ...) fill the bodies in.

The success criterion is a clean build under both feature-flag configurations,
nothing more.

---

## Context

ADR 0002 D4 commits Phase 2 to a `crates/vestige-core/src/storage/postgres/`
directory from day one. The seven other files in that directory
(`pool.rs`, `migrations.rs`, `registry.rs`, `search.rs`, `migrate_cli.rs`,
`reembed.rs`) belong to subsequent sub-plans. This sub-plan creates only
`crates/vestige-core/src/storage/postgres/mod.rs` so the rest can be added
incrementally without breaking the build.

Per ADR 0002 D2, `PgMemoryStore::connect` mirrors `SqliteMemoryStore::new`:
no `Embedder` argument. The pgvector typmod DDL
(`ALTER TABLE memories ALTER COLUMN embedding TYPE vector($N)`) lives inside
the trait method `register_model`, invoked by the caller after construction.
In this sub-plan `register_model` is a `todo!()` body; `0002c-migrations.md`
and `0002d-store-impl-bodies.md` provide the real implementation.

The trait surface in `crates/vestige-core/src/storage/memory_store.rs` is the
source of truth for method signatures. Do NOT copy signatures from the master
plan -- they are stale in places (for example, master plan 0002 D2 lists
`remove_edge` as three-arg `(source, target, edge_type)`; the live trait has
two args `(source, target)`).

---

## Cargo manifest changes

Two optional crates and one new feature flag. Use `cargo add` per the global
CLAUDE.md preference; do not hand-edit `Cargo.toml`.

```bash
cd crates/vestige-core

cargo add sqlx@0.8 --optional --no-default-features \
    --features runtime-tokio,tls-rustls,postgres,uuid,chrono,json,migrate,macros

cargo add pgvector@0.4 --optional --features sqlx
```

After both commands, open `crates/vestige-core/Cargo.toml` and add the
`postgres-backend` feature line in the `[features]` block. Place it after
the `metal` feature, before `[dependencies]`:

```toml
# Postgres backend (mutually compilable with the SQLite backend; default OFF).
# Compile with: --features postgres-backend
postgres-backend = ["dep:sqlx", "dep:pgvector"]
```

Do NOT add `tokio-stream`, `futures`, `indicatif`, or `toml` in this sub-plan.
The master plan D1 lists them in the `postgres-backend` feature for
convenience, but their consumers (streaming migrate, progress bar, config
parsing) land in later sub-plans. Adding them here pulls dead weight into the
feature gate.

Do NOT add the `vestige-mcp` pass-through feature in this sub-plan either.
The MCP crate gets its `postgres-backend` feature in `0002b-pool-and-config.md`
when `MemoryStoreConfig` lands and the binary needs a knob to pick a backend.

Verify the diff to `crates/vestige-core/Cargo.toml` looks like this and only
this:

```toml
[features]
# ...existing features unchanged...
postgres-backend = ["dep:sqlx", "dep:pgvector"]

[dependencies]
# ...existing deps unchanged...
sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono",
    "json", "migrate", "macros",
], optional = true }
pgvector = { version = "0.4", features = ["sqlx"], optional = true }
```

The exact ordering of the two new lines inside `[dependencies]` is not
significant; `cargo add` places them at the end. Leave the placement that
`cargo add` produces.

---

## Storage module export

Edit `crates/vestige-core/src/storage/mod.rs` to expose the new module behind
the feature flag. Two lines change.

Add to the module-declaration block (after `mod sqlite;`):

```rust
#[cfg(feature = "postgres-backend")]
mod postgres;
```

Add to the re-export block (after the `pub use sqlite::{ ... }` block):

```rust
#[cfg(feature = "postgres-backend")]
pub use postgres::PgMemoryStore;
```

Nothing else in `storage/mod.rs` changes. The `Storage` alias still points at
`SqliteMemoryStore`; the SQLite re-export block is untouched.

---

## Postgres module skeleton

Create `crates/vestige-core/src/storage/postgres/mod.rs` with the full content
below. This is the only new file in this sub-plan.

```rust
#![cfg(feature = "postgres-backend")]
//! Postgres-backed implementation of `MemoryStore`.
//!
//! Skeleton only. Every trait method is `todo!()`. Real bodies land in
//! subsequent Phase 2 sub-plans:
//! - `0002b-pool-and-config.md`: pool construction and config wiring
//! - `0002c-migrations.md`:      sqlx migration files and `init`/`register_model`
//! - `0002d-store-impl-bodies.md`: CRUD, scheduling, edges, domains
//! - `0002e-hybrid-search.md`:   RRF query and search bodies
//!
//! The directory grows companion files (`pool.rs`, `migrations.rs`,
//! `registry.rs`, `search.rs`, `migrate_cli.rs`, `reembed.rs`) in those
//! sub-plans; this skeleton only creates `mod.rs`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::storage::memory_store::{
    Domain, HealthStatus, LocalMemoryStore, MemoryEdge, MemoryRecord, MemoryStoreResult,
    ModelSignature, SchedulingState, SearchQuery, SearchResult, StoreStats,
};

/// Postgres-backed implementation of `MemoryStore`.
///
/// Cheaply cloneable. Methods take `&self`; interior state lives inside the
/// `PgPool` (which already provides `Sync` via `Arc` internally).
#[derive(Clone)]
pub struct PgMemoryStore {
    pool: PgPool,
    /// Embedding vector dimension. Set to 0 in the skeleton; populated by
    /// `register_model` in `0002d-store-impl-bodies.md` once the pgvector
    /// `ALTER COLUMN TYPE vector(N)` DDL lands.
    embedding_dim: i32,
}

impl PgMemoryStore {
    /// Construct a new store from a connection URL.
    ///
    /// Mirrors `SqliteMemoryStore::new`: no `Embedder` argument. The pgvector
    /// `ALTER TABLE memories ALTER COLUMN embedding TYPE vector($N)` DDL lives
    /// inside `register_model`, not here. The caller (cognitive engine
    /// bootstrap, migrate CLI, tests) invokes `register_model` after this
    /// returns, identically to the SQLite path.
    ///
    /// Real body lands in `0002b-pool-and-config.md` (pool construction) and
    /// `0002c-migrations.md` (initial migration run).
    pub async fn connect(_url: &str, _max_connections: u32) -> MemoryStoreResult<Self> {
        todo!("PgMemoryStore::connect lands in 0002b-pool-and-config.md")
    }

    /// Low-level constructor for tests: supply an existing pool, skip migrate.
    ///
    /// Real body lands in `0002b-pool-and-config.md`.
    pub async fn from_pool(_pool: PgPool) -> MemoryStoreResult<Self> {
        todo!("PgMemoryStore::from_pool lands in 0002b-pool-and-config.md")
    }

    /// Accessor used by migrate/reembed CLI.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Currently-registered vector dimension. Returns 0 until `register_model`
    /// has been called (real body: `0002d-store-impl-bodies.md`).
    pub fn embedding_dim(&self) -> i32 {
        self.embedding_dim
    }
}

// trait_variant::make on the trait declaration generates two traits:
//   - `MemoryStore` (Send-bound)
//   - `LocalMemoryStore` (non-Send companion)
// The implementer writes `impl LocalMemoryStore for ...` plainly; the Send
// variant is generated automatically from the non-Send impl.
impl LocalMemoryStore for PgMemoryStore {
    // --- Lifecycle ---
    async fn init(&self) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::init lands in 0002c-migrations.md")
    }

    async fn health_check(&self) -> MemoryStoreResult<HealthStatus> {
        todo!("PgMemoryStore::health_check lands in 0002d-store-impl-bodies.md")
    }

    // --- Embedding model registry ---
    async fn registered_model(&self) -> MemoryStoreResult<Option<ModelSignature>> {
        todo!("PgMemoryStore::registered_model lands in 0002d-store-impl-bodies.md")
    }

    async fn register_model(&self, _sig: &ModelSignature) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::register_model lands in 0002d-store-impl-bodies.md")
    }

    // --- CRUD ---
    async fn insert(&self, _record: &MemoryRecord) -> MemoryStoreResult<Uuid> {
        todo!("PgMemoryStore::insert lands in 0002d-store-impl-bodies.md")
    }

    async fn get(&self, _id: Uuid) -> MemoryStoreResult<Option<MemoryRecord>> {
        todo!("PgMemoryStore::get lands in 0002d-store-impl-bodies.md")
    }

    async fn update(&self, _record: &MemoryRecord) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::update lands in 0002d-store-impl-bodies.md")
    }

    async fn delete(&self, _id: Uuid) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::delete lands in 0002d-store-impl-bodies.md")
    }

    // --- Search ---
    async fn search(&self, _query: &SearchQuery) -> MemoryStoreResult<Vec<SearchResult>> {
        todo!("PgMemoryStore::search lands in 0002e-hybrid-search.md")
    }

    async fn fts_search(
        &self,
        _text: &str,
        _limit: usize,
    ) -> MemoryStoreResult<Vec<SearchResult>> {
        todo!("PgMemoryStore::fts_search lands in 0002e-hybrid-search.md")
    }

    async fn vector_search(
        &self,
        _embedding: &[f32],
        _limit: usize,
    ) -> MemoryStoreResult<Vec<SearchResult>> {
        todo!("PgMemoryStore::vector_search lands in 0002e-hybrid-search.md")
    }

    // --- FSRS Scheduling ---
    async fn get_scheduling(
        &self,
        _memory_id: Uuid,
    ) -> MemoryStoreResult<Option<SchedulingState>> {
        todo!("PgMemoryStore::get_scheduling lands in 0002d-store-impl-bodies.md")
    }

    async fn update_scheduling(&self, _state: &SchedulingState) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::update_scheduling lands in 0002d-store-impl-bodies.md")
    }

    async fn get_due_memories(
        &self,
        _before: DateTime<Utc>,
        _limit: usize,
    ) -> MemoryStoreResult<Vec<(MemoryRecord, SchedulingState)>> {
        todo!("PgMemoryStore::get_due_memories lands in 0002d-store-impl-bodies.md")
    }

    // --- Graph (spreading activation) ---
    async fn add_edge(&self, _edge: &MemoryEdge) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::add_edge lands in 0002d-store-impl-bodies.md")
    }

    async fn get_edges(
        &self,
        _node_id: Uuid,
        _edge_type: Option<&str>,
    ) -> MemoryStoreResult<Vec<MemoryEdge>> {
        todo!("PgMemoryStore::get_edges lands in 0002d-store-impl-bodies.md")
    }

    async fn remove_edge(&self, _source: Uuid, _target: Uuid) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::remove_edge lands in 0002d-store-impl-bodies.md")
    }

    async fn get_neighbors(
        &self,
        _node_id: Uuid,
        _depth: usize,
    ) -> MemoryStoreResult<Vec<(MemoryRecord, f64)>> {
        todo!("PgMemoryStore::get_neighbors lands in 0002d-store-impl-bodies.md")
    }

    // --- Domains (Phase 1: stubs return empty; full impl in Phase 4) ---
    async fn list_domains(&self) -> MemoryStoreResult<Vec<Domain>> {
        todo!("PgMemoryStore::list_domains lands in 0002d-store-impl-bodies.md")
    }

    async fn get_domain(&self, _id: &str) -> MemoryStoreResult<Option<Domain>> {
        todo!("PgMemoryStore::get_domain lands in 0002d-store-impl-bodies.md")
    }

    async fn upsert_domain(&self, _domain: &Domain) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::upsert_domain lands in 0002d-store-impl-bodies.md")
    }

    async fn delete_domain(&self, _id: &str) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::delete_domain lands in 0002d-store-impl-bodies.md")
    }

    async fn classify(&self, _embedding: &[f32]) -> MemoryStoreResult<Vec<(String, f64)>> {
        todo!("PgMemoryStore::classify lands in 0002d-store-impl-bodies.md")
    }

    // --- Bulk / Maintenance ---
    async fn count(&self) -> MemoryStoreResult<usize> {
        todo!("PgMemoryStore::count lands in 0002d-store-impl-bodies.md")
    }

    async fn get_stats(&self) -> MemoryStoreResult<StoreStats> {
        todo!("PgMemoryStore::get_stats lands in 0002d-store-impl-bodies.md")
    }

    async fn vacuum(&self) -> MemoryStoreResult<()> {
        todo!("PgMemoryStore::vacuum lands in 0002d-store-impl-bodies.md")
    }
}
```

Notes on the skeleton:

- The file-level `#![cfg(feature = "postgres-backend")]` means the whole file
  vanishes when the feature is off. The `mod postgres;` line in
  `storage/mod.rs` is itself feature-gated, so this is belt-and-braces; both
  gates are needed because the file-level attribute is what allows the file to
  use `sqlx::PgPool` unconditionally inside it.
- `EmbeddingModelDescriptor` (a separate Postgres-internal type that the
  master plan sketched on the struct) is dropped. The trait surface already
  carries `ModelSignature` for the registry round-trip; the registry storage
  layout is a private concern of `registry.rs`, which is added later. Keep
  `PgMemoryStore` minimal until a real consumer needs the extra type.
- The struct only carries `pool` and `embedding_dim`. The model descriptor
  field from the master plan sketch goes away with `EmbeddingModelDescriptor`.
  If `register_model` later needs to cache the descriptor on the struct, it
  can be added then; the skeleton does not speculate.
- The two trivial accessors (`pool`, `embedding_dim`) get real bodies. Every
  other method is `todo!()` so it returns `!` and trivially coerces to the
  declared return type at the type checker; this is what lets the build pass
  with no error variants and no SQL.

---

## Connect signature

Per ADR 0002 D2:

```rust
pub async fn connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self>;
pub async fn from_pool(pool: PgPool) -> MemoryStoreResult<Self>;
```

No `&dyn Embedder` argument. This deliberately differs from master plan 0002,
which predates the Phase 1 freeze. The pgvector-specific DDL
(`ALTER TABLE memories ALTER COLUMN embedding TYPE vector($N)`) does not run
inside `connect`; it runs inside `register_model(&ModelSignature)`, which the
caller invokes after `connect` returns.

In this sub-plan `register_model` is `todo!()`. The real body lands in
`0002d-store-impl-bodies.md` after `0002c-migrations.md` ships the
`0001_init.up.sql` migration that creates the `memories` table with a
placeholder `embedding vector` column (no typmod), against which
`register_model` later runs the typmod stamp.

---

## Error variant additions: deferred

`MemoryStoreError` does NOT gain `Postgres(sqlx::Error)` or
`Migrate(sqlx::migrate::MigrateError)` in this sub-plan.

The reason is mechanical: `todo!()` evaluates to the never type `!`, which
coerces to any `MemoryStoreResult<T>` regardless of the error variants
available. With every method body a `todo!()`, the skeleton has no expression
that needs to convert a `sqlx::Error` or `sqlx::migrate::MigrateError` into
`MemoryStoreError`. Adding the variants here would mean adding the
`#[cfg(feature = "postgres-backend")]` and `#[from]` plumbing to
`memory_store.rs` with no consumer yet -- dead code at every level except the
enum definition itself.

`0002d-store-impl-bodies.md` introduces both variants in the same commit that
turns the first `todo!()` into a real `sqlx::query!` call. That keeps the
diff to `memory_store.rs` next to the first usage site, which is easier to
review than adding variants ahead of need.

For reference, the variants that will be added in `0002d-store-impl-bodies.md`
look like this:

```rust
#[cfg(feature = "postgres-backend")]
#[error("postgres error: {0}")]
Postgres(#[from] sqlx::Error),

#[cfg(feature = "postgres-backend")]
#[error("postgres migration error: {0}")]
Migrate(#[from] sqlx::migrate::MigrateError),
```

Do not pre-add them here.

---

## Verification

Run these commands from the workspace root. All four must produce a clean
build, zero warnings on the diff-affected files, no test changes.

```bash
# 1. Default features (SQLite backend, postgres-backend OFF). Must build.
cargo build --workspace --all-targets

# 2. Workspace clippy with default features. Must be clean.
cargo clippy --workspace --all-targets -- -D warnings

# 3. Postgres feature enabled. Must build.
cargo build -p vestige-core --features postgres-backend

# 4. Clippy with postgres feature enabled. Must be clean.
cargo clippy -p vestige-core --features postgres-backend --all-targets -- -D warnings
```

Expected outcomes:

- `cargo build --workspace --all-targets` finishes with no compilation of
  `sqlx` or `pgvector` (both are optional, no consumer with default features).
  The `postgres` module is excluded entirely via `#[cfg]`.
- `cargo build -p vestige-core --features postgres-backend` compiles `sqlx`,
  `pgvector`, and `storage/postgres/mod.rs`. The build succeeds because every
  trait method is `todo!()`; nothing actually runs SQL.
- Both `clippy` invocations pass with `-D warnings`. The `todo!()` macro does
  not emit a `dead_code` lint by itself, and the trivial accessors are used by
  later sub-plans (clippy on the postgres feature alone may flag them as
  unused if you run with `--lib` only; the `--all-targets` form keeps tests
  and benches in scope so this does not fire).
- If clippy flags `unused_variables` on the underscore-prefixed parameters in
  the `todo!()` bodies, the underscore prefix is already the standard
  suppression; if a future clippy version disagrees, add
  `#[allow(unused_variables)]` to the impl block, not to each method.

Tests are not modified in this sub-plan. The unit tests in
`memory_store.rs` (`memory_store_error_from_storage_error`,
`model_signature_serde_round_trip`, `memory_record_serde_round_trip`) keep
passing because no type they touch changes.

Do NOT run `cargo test` against the postgres feature -- there is no Postgres
running and no test exercises `PgMemoryStore` yet. The build check is the
contract.

---

## Acceptance criteria

1. `crates/vestige-core/Cargo.toml` declares `sqlx = "0.8"` and
   `pgvector = "0.4"` as optional dependencies with the exact feature sets
   specified above.
2. `crates/vestige-core/Cargo.toml` declares `postgres-backend = ["dep:sqlx",
   "dep:pgvector"]` and nothing else inside that feature.
3. `crates/vestige-mcp/Cargo.toml` is unchanged.
4. `crates/vestige-core/src/storage/mod.rs` adds exactly two
   feature-gated lines: `mod postgres;` and `pub use postgres::PgMemoryStore;`.
   No other change.
5. `crates/vestige-core/src/storage/postgres/mod.rs` exists and contains the
   `PgMemoryStore` struct, `impl PgMemoryStore` block with real `pool` and
   `embedding_dim` accessors and `todo!()` bodies for `connect` and
   `from_pool`, and the full `impl LocalMemoryStore for PgMemoryStore` block
   with `todo!()` for every trait method.
6. The trait impl method signatures match `memory_store.rs` byte-for-byte
   (including `remove_edge(&self, source: Uuid, target: Uuid)` two-arg form,
   not the three-arg form from the master plan).
7. `MemoryStoreError` is unchanged.
8. No other files in the crate are touched. No new files in
   `storage/postgres/` besides `mod.rs`.
9. The four verification commands above all succeed.

---

## Commit sequence

One commit is recommended. The two changes (Cargo manifest + module skeleton)
do not compile in isolation: the manifest change without the skeleton produces
unused-optional-dep warnings, and the skeleton without the manifest change
fails to find `sqlx`. Splitting them adds no review value, since the second
commit is the one that has to compile cleanly.

```bash
git add crates/vestige-core/Cargo.toml \
        crates/vestige-core/Cargo.lock \
        crates/vestige-core/src/storage/mod.rs \
        crates/vestige-core/src/storage/postgres/mod.rs

git commit -m "feat(storage): scaffold postgres-backend feature and PgMemoryStore skeleton

Adds the postgres-backend Cargo feature gating sqlx 0.8 and pgvector 0.4.
Introduces crates/vestige-core/src/storage/postgres/mod.rs with the
PgMemoryStore struct, connect/from_pool/pool/embedding_dim, and a trait impl
whose method bodies are todo!() pending later Phase 2 sub-plans.

Builds clean with default features (SQLite only) and with --features
postgres-backend. No runtime behaviour change.

Refs ADR 0002 D1, D2, D4."
```

If for any reason the manifest change must be reviewed separately (for
example, a security review of the sqlx version pin), split as:

1. `cargo add` for sqlx and pgvector + manual feature line in Cargo.toml.
   Build with default features will pass but `--features postgres-backend`
   will fail (no module to satisfy the feature). This is acceptable for a
   short-lived intermediate commit.
2. `storage/mod.rs` edits + `storage/postgres/mod.rs` creation. Both builds
   pass.

Default to the single-commit form unless asked to split.

---

## Followups

- `0002b-pool-and-config.md` adds `pool.rs`, `PostgresConfig`, and the
  `vestige-mcp` `postgres-backend` pass-through feature.
- `0002c-migrations.md` adds `crates/vestige-core/migrations/postgres/` with
  `0001_init.{up,down}.sql` and `0002_hnsw.{up,down}.sql`, plus
  `postgres/migrations.rs` invoking `sqlx::migrate!`. `init()` body lands here.
- `0002d-store-impl-bodies.md` introduces the two `MemoryStoreError` variants
  and replaces every `todo!()` in CRUD / scheduling / edges / domains /
  registry with real `sqlx::query!` bodies.
- `0002e-hybrid-search.md` fills the three search bodies via the RRF query.
