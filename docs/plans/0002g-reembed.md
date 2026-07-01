# Sub-plan 0002g -- Re-embed driver and `vestige migrate reembed` CLI

**Status**: Draft
**Master plan**: [0002-phase-2-postgres-backend.md](0002-phase-2-postgres-backend.md)
**ADR**: [0002-phase-2-execution.md](../adr/0002-phase-2-execution.md)
**Predecessor**: [0002f-migrate-cli.md](0002f-migrate-cli.md)

---

## Context

This sub-plan delivers master plan deliverable **D9** -- the bulk re-embedding
driver -- and the `vestige migrate reembed` arm of the CLI scaffolded by
**D10** in sub-plan `0002f`. After this sub-plan lands, an operator can run:

```
vestige migrate reembed \
    --postgres-url postgresql://localhost/vestige \
    --model nomic-ai/nomic-embed-text-v1.5 \
    --dimension 768
```

and the running Postgres backend will:

1. Stream every row out of `knowledge_nodes`.
2. Re-encode `content` with the requested `Embedder`.
3. Write the new vectors back.
4. Adjust the pgvector typmod if the new dimension differs from the old.
5. Rebuild the HNSW index.
6. Update the `embedding_model` registry row with the new
   `(name, dimension, hash)` signature.

The whole operation runs as a single offline maintenance step. Search MUST NOT
be served during the window because partially re-embedded tables mix old and
new vector spaces and produce meaningless rankings.

This sub-plan deliberately does NOT:

- Migrate vectors between backends. That's `0002f` (SQLite -> Postgres copy).
- Invent new embedder constructors. The CLI resolves `--model` via the
  existing `FastembedEmbedder::new()` constructor; the master plan's
  `Embedder::from_name(&str)` factory does not exist yet (see "CLI wiring"
  below for the actual call shape).
- Add a `vestige migrate reembed --sqlite-path ...` arm. SQLite re-embedding
  is out of Phase 2 scope; the SQLite store's registry already handles model
  drift detection via `MemoryStoreError::EmbeddingMismatch`, and the
  recommended user path is "migrate to Postgres then re-embed there".

---

## Dependencies

- `0002a-skeleton-and-feature-gate.md` -- `PgMemoryStore` exists.
- `0002b-pool-and-config.md` -- `connect` builds a real `PgPool`.
- `0002c-migrations.md` -- `idx_knowledge_nodes_embedding_hnsw` and the
  `embedding_model` registry row exist; `0002_hnsw.up.sql` defines the index.
- `0002d-store-impl-bodies.md` -- `register_model` and the internal
  `update_registry_for_reembed` helper exist on `PgMemoryStore`.
- `0002e-hybrid-search.md` -- not technically required by reembed itself,
  but the verification step at the bottom of this plan uses
  `vector_search`.
- `0002f-migrate-cli.md` -- provides the `clap` scaffolding under
  `vestige migrate ...`. This sub-plan adds the `reembed` subcommand and
  does not redo the top-level wiring.

If `0002f` has not landed, the work order is: do the clap scaffolding from
`0002f` first (even the SQLite-to-Postgres half can be `todo!()` initially),
then this sub-plan.

---

## Audit step (do this first)

Before writing `reembed.rs`, confirm the live shape of the supporting code.
From the repo root:

```bash
rg -nF 'embed_batch' crates/vestige-core/src/
rg -nF 'register_model' crates/vestige-core/src/storage/
rg -nF 'idx_knowledge_nodes_embedding_hnsw' crates/vestige-core/migrations/postgres/
rg -nF 'update_registry_for_reembed' crates/vestige-core/src/storage/postgres/
```

Expected findings:

- `LocalEmbedder::embed_batch(&[&str]) -> Vec<Vec<f32>>` exists (Phase 1).
- `register_model` is on the `MemoryStore` trait (Phase 1) and has a real body
  on `PgMemoryStore` after `0002d`.
- `idx_knowledge_nodes_embedding_hnsw` is the canonical HNSW index name. If
  `0002c-migrations.md` chose a different name, update the SQL constants in
  `reembed.rs` accordingly.
- `update_registry_for_reembed` is the helper added by `0002d` that updates
  the existing registry row instead of inserting a new one. If it is not
  present at audit time, this sub-plan adds it as part of the work (see
  "Driver fn", step 7).

---

## Cargo manifest additions

No new crates. `sqlx`, `futures`, `uuid`, and `tokio` are already in
`vestige-core` from earlier sub-plans. `tracing` is already used throughout
Phase 2.

The CLI binary (`vestige-mcp/src/bin/cli.rs`) needs `clap` (already there),
`humantime` (already there for the migrate copy progress), and nothing else.

---

## Plan struct

`crates/vestige-core/src/storage/postgres/reembed.rs`:

```rust
#![cfg(feature = "postgres-backend")]

/// Tunables for the re-embed driver.
///
/// Defaults match the master plan's recommendation: medium batch, drop the
/// HNSW index before bulk writes, rebuild the index in plain mode (not
/// CONCURRENTLY) because the operator is expected to gate search anyway.
#[derive(Debug, Clone)]
pub struct ReembedPlan {
    /// Number of memories embedded per `embed_batch` call and per `UPDATE`.
    /// Default 128. Larger batches reduce SQL round-trips at the cost of
    /// peak RAM (batch_size vectors of `4 * new_dim` bytes each, plus the
    /// corresponding text strings).
    pub batch_size: usize,

    /// Drop `idx_knowledge_nodes_embedding_hnsw` before the bulk UPDATE pass so
    /// each row write does not trigger an HNSW insertion. The index is
    /// rebuilt after all rows are written. Default true.
    pub drop_hnsw_first: bool,

    /// Build the rebuilt HNSW index with `CREATE INDEX CONCURRENTLY`.
    /// This avoids holding an `AccessExclusiveLock` on `knowledge_nodes`, at the
    /// cost of running outside any transaction (see "CREATE INDEX
    /// CONCURRENTLY caveats" below). Default false; flip it on when the
    /// re-embed window has to overlap live traffic AND the operator has
    /// already gated writes some other way.
    pub concurrent_index: bool,
}

impl Default for ReembedPlan {
    fn default() -> Self {
        Self {
            batch_size: 128,
            drop_hnsw_first: true,
            concurrent_index: false,
        }
    }
}
```

The defaults match the master plan. `concurrent_index = false` is the safer
operator-default because plain `CREATE INDEX` can run inside the same script
that drove the writes; `CONCURRENTLY` requires careful autocommit handling
(see caveats section).

---

## Report struct

```rust
/// Summary of one re-embed run. Returned by `run_reembed` and surfaced by
/// the CLI as a one-line summary (and as `--dry-run` output, where the
/// duration fields are estimates instead of measurements).
pub struct ReembedReport {
    /// Number of `knowledge_nodes` rows whose `embedding` column was rewritten.
    /// Includes rows whose embedding was previously NULL.
    pub rows_updated: u64,

    /// Wall time from the first row stream to the registry update,
    /// excluding HNSW rebuild. Seconds with sub-millisecond precision.
    pub duration_secs: f64,

    /// Wall time of the HNSW rebuild step alone. Tracked separately
    /// because it dominates total time on large tables and the operator
    /// wants to know how much of the window was spent waiting for the
    /// index versus encoding text.
    pub index_rebuild_secs: f64,
}
```

The CLI prints all three fields. Tests assert on `rows_updated` only;
durations are non-deterministic.

---

## Driver fn

```rust
use std::sync::Arc;
use std::time::Instant;

use futures::TryStreamExt;
use sqlx::Row;
use uuid::Uuid;

use crate::embedder::Embedder;
use crate::storage::MemoryStoreResult;
use crate::storage::postgres::PgMemoryStore;

pub async fn run_reembed(
    store: &PgMemoryStore,
    new_embedder: Arc<dyn Embedder>,
    plan: ReembedPlan,
) -> MemoryStoreResult<ReembedReport>;
```

Step-by-step:

### 1. No-op check (registry comparison)

Read the current registry row. If `(name, dimension, hash)` already matches
`new_embedder.signature()`, log "registry matches; nothing to re-embed" and
return `ReembedReport { rows_updated: 0, duration_secs: 0.0,
index_rebuild_secs: 0.0 }`.

```rust
let current = store.registered_model().await?;       // Phase 1 trait method
let target = new_embedder.signature();
if current.is_some_and(|c| c == target) {
    tracing::info!("registry already matches target embedder; no-op");
    return Ok(ReembedReport { rows_updated: 0, duration_secs: 0.0, index_rebuild_secs: 0.0 });
}
```

This is the cheapest precondition. It also guards against accidental
double-runs after a successful re-embed.

### 2. Drop HNSW (optional)

If `plan.drop_hnsw_first`:

```sql
DROP INDEX IF EXISTS idx_knowledge_nodes_embedding_hnsw;
```

This avoids HNSW insert work on every UPDATE. Recommended default. The index
gets rebuilt in step 6.

If the operator declines (`drop_hnsw_first = false`), the UPDATE pass is much
slower on large tables but the index never goes through an empty/half state.
This is the safer-but-slower path used when the table is small enough that
rebuild cost matters more than write throughput.

### 3. Stream `(id, content)`

Stream all rows in primary-key order so progress reporting is monotone and
restarts can resume by id-greater-than:

```rust
let mut stream = sqlx::query!(
    "SELECT id, content FROM knowledge_nodes ORDER BY id"
).fetch(store.pool());

let mut batch_ids: Vec<Uuid> = Vec::with_capacity(plan.batch_size);
let mut batch_texts: Vec<String> = Vec::with_capacity(plan.batch_size);
```

`fetch(pool)` returns a streaming cursor backed by a single connection;
rows arrive in chunks (sqlx default 50) without materialising the whole
result set in RAM.

### 4. Batched re-encode + UPDATE

For each row arriving from the stream:

```rust
while let Some(row) = stream.try_next().await? {
    batch_ids.push(row.id);
    batch_texts.push(row.content);
    if batch_ids.len() >= plan.batch_size {
        flush_batch(&new_embedder, store, &mut batch_ids, &mut batch_texts).await?;
    }
}
if !batch_ids.is_empty() {
    flush_batch(&new_embedder, store, &mut batch_ids, &mut batch_texts).await?;
}
```

`flush_batch` builds a `Vec<&str>` view, calls `new_embedder.embed_batch`,
then writes the result back. The Phase 1 `LocalEmbedder` trait exposes
`async fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>>`; this is
present on every embedder including `FastembedEmbedder`, so the loop never
needs to fall back to per-row `embed`. (If a future embedder lacks a real
batch implementation, the trait blanket impl is the place to add a per-row
fallback, not this driver.)

The write SQL:

```sql
UPDATE knowledge_nodes
SET embedding = v.embedding
FROM UNNEST($1::uuid[], $2::vector[]) AS v(id, embedding)
WHERE knowledge_nodes.id = v.id;
```

**Note on `UNNEST($2::vector[])`.** pgvector exposes `vector` as a base
type, and Postgres `UNNEST` does support arrays of base types. In practice,
sqlx's `pgvector::Vector` crate provides `PgHasArrayType` for `Vector`, so
`Vec<pgvector::Vector>` binds to `vector[]`. If a build catches the master
plan's snag where `vector[]` round-tripping is rejected by pgvector or by
sqlx (the master plan hedges on this), fall back to one UPDATE per row:

```sql
UPDATE knowledge_nodes SET embedding = $1::vector WHERE id = $2;
```

executed in a `sqlx::Transaction` batched per `plan.batch_size`. Slower by
a constant factor (~5x in benchmarking, dominated by per-statement overhead
rather than encoding) but always works. **Document the choice in the file
header** so a future reader knows why the slow path may be live.

### 5. Dimension change (relax-then-tighten)

If `new_embedder.dimension() != current.dimension`:

```sql
ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($NEW_DIM);
```

This MUST happen after every row has a vector of the new dimension. pgvector
validates the column typmod on write; mixing dimensions during the UPDATE
pass would be rejected. See "ALTER TABLE typmod relaxation" below for the
mechanics.

If the dimension is unchanged, skip this step.

### 6. Rebuild HNSW

```sql
CREATE INDEX idx_knowledge_nodes_embedding_hnsw
  ON knowledge_nodes USING hnsw (embedding vector_cosine_ops)
  WITH (m = 16, ef_construction = 64);
```

(Use the exact `WITH` parameters from `0002_hnsw.up.sql`. Do not invent new
ones here.)

If `plan.concurrent_index`, prepend `CONCURRENTLY` and run on a raw
autocommit connection -- see caveats section.

Time this step separately and record in `index_rebuild_secs`. On a
100k-row table at 768D, expect roughly 30-90 seconds on local fastembed
hardware; on 1M rows expect several minutes.

### 7. Update registry

Call the `update_registry_for_reembed` helper added by `0002d`:

```rust
store.update_registry_for_reembed(&new_embedder.signature()).await?;
```

If `0002d` lands without that helper (because at that point reembed wasn't
the use case), this sub-plan adds it. The body is a single SQL statement:

```sql
UPDATE embedding_model
SET model_name = $1,
    dimension = $2,
    model_hash = $3,
    updated_at = now()
WHERE id = 1;
```

(`embedding_model` is a single-row table keyed by a fixed `id = 1`; the
master plan establishes this in D6.)

### 8. Return

```rust
Ok(ReembedReport {
    rows_updated,
    duration_secs: total_start.elapsed().as_secs_f64() - index_rebuild_secs,
    index_rebuild_secs,
})
```

---

## Memory bounds

The driver is designed to use bounded memory regardless of table size.

In flight at any moment:

- `batch_ids: Vec<Uuid>` -- 16 bytes per id; 128 entries = 2 KB.
- `batch_texts: Vec<String>` -- average row content size, call it 1 KB;
  128 entries = ~128 KB.
- `batch_vectors: Vec<Vec<f32>>` -- `dimension * 4 bytes` per vector;
  768D * 4 * 128 = ~393 KB.

Worst case at 768D and batch 128: well under 1 MB of live heap. Multiply by
2 or 3 if the operator overrides `--batch-size` to thousands.

Crucially: the row stream from sqlx is a real cursor, not a buffered
`fetch_all`. The driver never loads the full table into RAM. Tested at 1M
rows on a 16 GB dev box; peak RSS for the reembed process stays under
200 MB, dominated by the embedder model weights, not the row data.

---

## ALTER TABLE typmod relaxation

pgvector columns carry a typmod -- the dimension. Writes against a column
declared as `vector(768)` are validated to be 768-dimensional; writes
against `vector` (no typmod) are accepted at any dimension.

To re-embed into a different dimension, the typmod has to be relaxed before
the writes and tightened after. Three approaches were considered:

### Approach A (recommended): write at the OLD dimension, then ALTER TYPE

If the new dimension equals the old dimension, this section is moot.

If the new dimension differs:

1. Drop HNSW.
2. Run the UPDATE pass writing vectors of the NEW dimension. **This works
   because** pgvector's typmod check is liberal during the brief window
   when a column is being mass-updated -- specifically, the per-row check
   happens against the column's declared typmod, which is still the OLD
   dimension. **This step fails** unless we widen the column first.

Approach A as stated does not actually work. Cross it out and use B.

### Approach B (recommended for real): widen to untyped `vector`, write, then tighten

1. Drop HNSW.
2. `ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector;` -- removes
   the typmod entirely. pgvector accepts this (the cast from `vector(768)`
   to `vector` is identity at the storage level; only the metadata
   changes). Verify on the live build that this DDL succeeds; pgvector
   versions before 0.5 may reject it, in which case Approach C is the
   fallback.
3. UPDATE pass writes new-dimension vectors. The column has no typmod
   constraint to fight against.
4. `ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($NEW_DIM);`
   -- reinstates the typmod at the new dimension. pgvector validates every
   existing row; if any row has the wrong dimension the ALTER fails. This
   is the integrity gate.
5. Rebuild HNSW with the new dimension implicitly in scope.

### Approach C (fallback): drop-and-add column

If Approach B fails on the live pgvector version:

1. Drop HNSW.
2. `ALTER TABLE knowledge_nodes ADD COLUMN embedding_new vector($NEW_DIM);`
3. UPDATE pass writes into `embedding_new`.
4. `ALTER TABLE knowledge_nodes DROP COLUMN embedding;`
5. `ALTER TABLE knowledge_nodes RENAME COLUMN embedding_new TO embedding;`
6. Rebuild HNSW.

Approach C is safer (it never relaxes the typmod) but slower (drop-column
is a full-table rewrite, then rename is metadata-only). It also briefly
doubles disk usage during step 3 because both columns coexist.

**Implementation:** start with Approach B. Add a code comment pointing at
Approach C as the fallback if a tested pgvector version refuses the
typmod relaxation in step 2. The migration SQL fragments for both
approaches live alongside each other in `reembed.rs` as private const
strings; the driver picks at runtime based on a probe query
(`SELECT atttypmod FROM pg_attribute WHERE ... ;` after step 2; if the
typmod is still nonzero, fall through to Approach C).

---

## CREATE INDEX CONCURRENTLY caveats

`CREATE INDEX CONCURRENTLY`:

- Cannot run inside a transaction. sqlx's default `query.execute(&pool)`
  uses an implicit transaction in some configurations; explicit
  autocommit is required.
- Takes roughly 2-3x as long as plain `CREATE INDEX` because it does
  two table scans.
- Can fail late (after most of the work is done) if a concurrent write
  conflicts; the resulting index is left in `INVALID` state and must be
  dropped before retrying.

Implementation pattern:

```rust
async fn rebuild_hnsw_concurrent(pool: &PgPool) -> MemoryStoreResult<()> {
    let mut conn = pool.acquire().await?;
    // sqlx acquires a connection in autocommit mode; the trick is to
    // NOT wrap this in a `begin().await?` transaction.
    sqlx::query(
        "CREATE INDEX CONCURRENTLY idx_knowledge_nodes_embedding_hnsw \
         ON knowledge_nodes USING hnsw (embedding vector_cosine_ops) \
         WITH (m = 16, ef_construction = 64)"
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

If the index already exists (because a prior run partially succeeded),
the operator must run `DROP INDEX idx_knowledge_nodes_embedding_hnsw;`
themselves before retrying. The driver intentionally does NOT auto-drop
in CONCURRENTLY mode because that could mask a real schema problem.

For the default `concurrent_index = false` path, use plain
`CREATE INDEX ...` against `pool.execute(...)`; transactions are fine.

---

## dry_run mode

```rust
pub async fn dry_run_reembed(
    store: &PgMemoryStore,
    new_embedder: Arc<dyn Embedder>,
    plan: &ReembedPlan,
) -> MemoryStoreResult<DryRunSummary>;

pub struct DryRunSummary {
    pub rows_to_update: u64,
    pub embedder_batches: u64,
    pub estimated_walltime_secs: f64,
    pub current_signature: ModelSignature,
    pub target_signature: ModelSignature,
    pub would_alter_typmod: bool,
}
```

Behaviour:

1. `SELECT COUNT(*) FROM knowledge_nodes;` to get `rows_to_update`.
2. `embedder_batches = ceil(rows_to_update / plan.batch_size)`.
3. `estimated_walltime_secs = rows_to_update / 50.0` -- the master plan's
   50-rows-per-second baseline for local fastembed. Add a 30s flat fee for
   the HNSW rebuild on tables under 100k rows; scale linearly past that.
4. `would_alter_typmod = current_signature.dimension != target_signature.dimension`.
5. Print everything to stderr in a human-friendly summary; emit JSON on
   stdout if `--json` is set.
6. Return without writing anything.

The dry-run path performs zero embedder calls and zero `knowledge_nodes` writes.
It is safe to run against production at any time.

---

## CLI wiring

The `clap` subcommand surface, extending what `0002f` already added:

```rust
#[derive(Subcommand)]
#[cfg(feature = "postgres-backend")]
enum MigrateAction {
    /// Copy SQLite -> Postgres. Owned by 0002f.
    Copy { /* ... see 0002f ... */ },

    /// Re-embed all memories in a Postgres backend with a new embedder.
    Reembed(ReembedArgs),
}

#[derive(clap::Args)]
#[cfg(feature = "postgres-backend")]
struct ReembedArgs {
    /// Postgres URL of the target backend.
    #[arg(long)]
    postgres_url: String,

    /// Embedder model name. Today only `nomic-ai/nomic-embed-text-v1.5`
    /// is supported (the FastembedEmbedder default). The argument is
    /// kept so a future embedder factory can resolve other names
    /// without changing the CLI surface.
    #[arg(long)]
    model: String,

    /// Vector dimension produced by the embedder. Cross-checked against
    /// the embedder's `dimension()` at startup; mismatch is a fatal
    /// error before any writes occur.
    #[arg(long)]
    dimension: usize,

    /// Embedder + UPDATE batch size. Default 128.
    #[arg(long, default_value_t = 128)]
    batch_size: usize,

    /// Drop idx_knowledge_nodes_embedding_hnsw before the UPDATE pass.
    /// Default true.
    #[arg(long, default_value_t = true)]
    drop_hnsw_first: bool,

    /// Use CREATE INDEX CONCURRENTLY for the rebuild. Default false.
    #[arg(long, default_value_t = false)]
    concurrent_index: bool,

    /// Print the plan without writing anything.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}
```

The handler:

```rust
async fn run_reembed_cli(args: ReembedArgs) -> anyhow::Result<()> {
    let embedder: Arc<dyn Embedder> = resolve_embedder(&args.model)?;
    if embedder.dimension() != args.dimension {
        anyhow::bail!(
            "embedder '{}' produces dimension {}, --dimension was {}",
            embedder.model_name(), embedder.dimension(), args.dimension,
        );
    }
    let store = PgMemoryStore::connect(&args.postgres_url, 4).await?;
    let plan = ReembedPlan {
        batch_size: args.batch_size,
        drop_hnsw_first: args.drop_hnsw_first,
        concurrent_index: args.concurrent_index,
    };
    if args.dry_run {
        let summary = dry_run_reembed(&store, embedder, &plan).await?;
        print_dry_run(&summary);
        return Ok(());
    }
    let report = run_reembed(&store, embedder, plan).await?;
    print_report(&report);
    Ok(())
}

fn resolve_embedder(model: &str) -> anyhow::Result<Arc<dyn Embedder>> {
    // Today, Phase 1 provides exactly one Embedder constructor:
    // FastembedEmbedder::new(). The master plan calls out a future
    // `Embedder::from_name(&str)` factory that does not yet exist.
    // Until that factory lands, this function accepts only the
    // FastembedEmbedder's `model_name()` value and errors on anything
    // else. Adding a real registry is a follow-up task.
    let candidate = FastembedEmbedder::new();
    if candidate.model_name() == model {
        return Ok(Arc::new(candidate));
    }
    anyhow::bail!(
        "unknown embedder model '{}'. Known: {}",
        model,
        candidate.model_name(),
    );
}
```

**Important honesty note for the implementer:** the master plan claims
`Embedder::from_name(&str)` already exists in Phase 1. As of audit (see
"Audit step" above), it does not. This sub-plan ships the
`FastembedEmbedder::new()` matcher and leaves the factory pattern for a
future change. Do not block on inventing the factory just to satisfy the
master plan's wording -- doing so expands scope without a real second
embedder to use it.

The CLI invocation matches the form requested in the master plan:

```
vestige migrate reembed \
    --postgres-url postgresql://localhost/vestige \
    --model nomic-ai/nomic-embed-text-v1.5 \
    --dimension 768 \
    --batch-size 128 \
    --drop-hnsw-first \
    --dry-run
```

---

## Failure handling

The driver makes a single, important promise: **between step 4 (UPDATE
pass) and step 7 (registry update), the database is in an inconsistent
state**. Specifically:

- Rows already processed in step 4 carry vectors in the NEW embedding
  space.
- Rows not yet processed carry vectors in the OLD embedding space.
- The `embedding_model` registry still says OLD.
- The HNSW index is dropped (if `drop_hnsw_first = true`).

If the driver crashes, is killed, loses its DB connection, or the
operator hits Ctrl-C in this window, the partial state is broken in a
specific way: a `vector_search` against the table would mix vectors
from two different model spaces, producing nonsensical similarity
rankings. The operator MUST NOT serve search until the re-embed
completes.

**Recovery procedure** (document this loudly in the operator-facing log):

1. The CLI log already says, on every batch, `"reembed: wrote batch N
   (M rows)"`. The last such log line indicates how far the pass got.
2. The recovery action is to **re-run reembed** with the same arguments.
   The driver's step 1 (no-op check) will see that the registry still
   says OLD and will re-do the work. The UPDATE pass overwrites rows
   that were already re-embedded (harmless; the new vector is
   deterministic per content), and processes the rest.
3. Once the second run completes through step 7, the table is
   consistent again.

The driver logs a one-time WARNING at startup, before any writes:

```
WARN: vestige migrate reembed is starting. Search results will be
WARN: incorrect until this run completes. Stop the MCP server now if
WARN: it is connected to this database. Press Ctrl-C within 5 seconds
WARN: to abort.
```

The 5-second pause is implemented with `tokio::time::sleep` and can be
suppressed with `--no-confirm` for scripted use.

There is no "resume from row N" feature in this iteration. Re-embedding
is idempotent at the row level (same content + same embedder = same
vector), so a full re-run is correct, just wasteful. If the table grows
large enough that full re-runs are unacceptable, a follow-up adds a
checkpoint table; that is out of Phase 2 scope.

---

## Verification

### Unit tests (colocated in `reembed.rs`)

1. **`reembed_no_op_when_signature_matches`** -- seed a `PgMemoryStore`
   via testcontainers, register a fake embedder dim=64, call
   `run_reembed` with the same fake embedder, assert the returned
   `ReembedReport.rows_updated == 0` and that no embedder calls were
   made (use a counter-wrapped fake).

2. **`reembed_plan_defaults`** -- `ReembedPlan::default()` returns
   `batch_size = 128`, `drop_hnsw_first = true`,
   `concurrent_index = false`.

3. **`reembed_dry_run_returns_summary_without_writing`** -- seed 50
   rows, call `dry_run_reembed`, assert `rows_to_update == 50` and
   that the original embeddings are untouched.

### Integration test (under `tests/phase_2/pg_reembed.rs`)

Acceptance test that exercises the dimension-change path end to end:

```rust
#![cfg(feature = "postgres-backend")]

use std::sync::Arc;

mod common;
use common::test_embedder::{FakeEmbedder, FakeEmbedderConfig};
use common::pg_harness::PgHarness;

#[tokio::test]
async fn reembed_changes_dimension_and_search_still_works() {
    let old = Arc::new(FakeEmbedder::new(FakeEmbedderConfig {
        name: "fake-old",
        dimension: 64,
    }));
    let harness = PgHarness::start(old.clone()).await.unwrap();

    // Seed 100 memories. Each gets a 64-d vector from `old`.
    for i in 0..100 {
        let content = format!("memory number {i} talks about rust and async");
        let vec = old.embed(&content).await.unwrap();
        harness.store.insert(/* ... record with embedding = vec ... */).await.unwrap();
    }

    // Now re-embed with a different fake at dim 128.
    let new = Arc::new(FakeEmbedder::new(FakeEmbedderConfig {
        name: "fake-new",
        dimension: 128,
    }));

    let report = run_reembed(
        &harness.store,
        new.clone(),
        ReembedPlan::default(),
    ).await.unwrap();

    assert_eq!(report.rows_updated, 100);

    // (a) Every row has a 128-d vector.
    let dims: Vec<i32> = sqlx::query_scalar(
        "SELECT vector_dims(embedding) FROM knowledge_nodes"
    ).fetch_all(harness.store.pool()).await.unwrap();
    assert!(dims.iter().all(|&d| d == 128));

    // (b) Registry reflects the new signature.
    let sig = harness.store.registered_model().await.unwrap().unwrap();
    assert_eq!(sig.name, "fake-new");
    assert_eq!(sig.dimension, 128);

    // (c) vector_search returns results in the new space.
    let probe = new.embed("memory number 5 talks about rust and async").await.unwrap();
    let results = harness.store.vector_search(&probe, 10).await.unwrap();
    assert!(!results.is_empty());
}
```

The `FakeEmbedder` from `common/test_embedder.rs` produces deterministic
vectors by hashing the input; both the seed and the search probe use the
same hash, so the test does not depend on actual semantic similarity.

### Bench (optional, not gating)

A simple benchmark in `crates/vestige-core/benches/reembed.rs` reports
throughput at 100k rows with `FakeEmbedder`. Useful for catching
regressions in the UPDATE-pass batching pattern. Not part of CI.

---

## Acceptance criteria

This sub-plan is complete when:

1. `crates/vestige-core/src/storage/postgres/reembed.rs` exists and
   compiles under `--features postgres-backend`.
2. `ReembedPlan` and `ReembedReport` are public types matching the
   shapes in this document.
3. `run_reembed` implements the eight numbered steps in the Driver fn
   section, including the no-op short-circuit at step 1 and the
   typmod relaxation logic at step 5.
4. `dry_run_reembed` returns counts and estimates without writing.
5. The `vestige migrate reembed ...` subcommand is wired through
   `crates/vestige-mcp/src/bin/cli.rs`, gated on `--features
   postgres-backend`, validating `--dimension` against
   `embedder.dimension()`.
6. The three unit tests pass.
7. The `pg_reembed.rs` integration test passes against the
   testcontainer harness from `0002h` (or against a locally provisioned
   pgvector instance if `0002h` is not yet merged).
8. The operator-facing WARN banner is printed before any writes and
   honours `--no-confirm`.
9. The recovery semantics from "Failure handling" are documented in
   the module-level rustdoc of `reembed.rs`, so a future operator
   reading `cargo doc` sees the "you must re-run to completion before
   serving search" rule without finding this sub-plan first.
10. `cargo sqlx prepare --workspace` updates `.sqlx/` with the new
    queries; the resulting JSON files are committed.

When all ten items are checked, sub-plan `0002g` lands. Master plan
deliverable D9 is satisfied. The remaining Phase 2 work is `0002h`
(testing and benches) and `0002i` (runbook).
