# Phase 2 Sub-Plan 0002f -- SQLite-to-Postgres Migrate CLI

**Status**: Ready
**Depends on**:
- `0002a-skeleton-and-feature-gate.md` (the `postgres-backend` Cargo feature
  and the `crates/vestige-core/src/storage/postgres/` module skeleton).
- `0002b-pool-and-config.md` (`PgPool` construction and `PostgresConfig`).
- `0002c-migrations.md` (the `postgres/migrations/0001_init.up.sql` schema,
  including the D7 tenancy columns/tables and the D8 `codebase` column).
- `0002d-store-impl-bodies.md` (real bodies for `PgMemoryStore` trait methods:
  `insert`, `register_model`, `add_edge`, `update_scheduling`, etc.; and the
  matching source-side reader bodies on `SqliteMemoryStore`, in particular a
  windowed-stream API ordered by `(created_at, id)`).

This sub-plan covers Phase 2 master-plan deliverables D8 (the streaming copy
in `crates/vestige-core/src/storage/postgres/migrate_cli.rs`) and D10 (the
`vestige migrate copy ...` clap subcommand in
`crates/vestige-mcp/src/bin/cli.rs`). Sub-plan `0002g-reembed.md` covers the
`vestige migrate reembed ...` subcommand body; this sub-plan only declares the
`Reembed` clap variant alongside `Copy` so the subcommand layout is final.

The success criterion is:

```
vestige migrate copy --from sqlite --to postgres \
    --sqlite-path ~/.vestige/vestige.db \
    --postgres-url postgresql://localhost/vestige
```

streams every row from a Phase 1 SQLite database into a fresh (or partially
populated) Phase 2 Postgres database. Re-running the same command is a no-op
on already-present rows. A `--dry-run` flag prints per-table counts without
writing anything.

---

## Context

ADR 0002 D2 settled that `PgMemoryStore::connect` mirrors
`SqliteMemoryStore::new`: no `Embedder` in the constructor; the model
signature is stamped by a separate call to `register_model`. The migrator
inherits this symmetry. It opens both backends, validates that the source's
`embedding_model` registry agrees with the destination's (or with the
embedder the user supplied for the destination), and then streams rows.

ADR 0002 D7 reserved multi-tenancy columns on `knowledge_nodes` (`owner_user_id`,
`visibility`, `shared_with_groups`) and three tables (`users`, `groups`,
`group_memberships`). Phase 2 single-user defaults are the bootstrap row
`'00000000-0000-0000-0000-000000000001'` (`'local'`), `visibility = 'private'`,
empty `shared_with_groups`. The migrator preserves whatever values the source
SQLite holds; it does NOT rewrite owner_user_id from real values to the
bootstrap user. If a Phase 3-aware source has real user rows, those are
copied first (step 5 below) and the foreign key in `knowledge_nodes.owner_user_id`
resolves to the same UUID on the destination.

ADR 0002 D8 promoted `codebase` to a first-class `TEXT` column. The migrator
reads it as a column on the source side (the Phase 1 amendment's V15 SQLite
migration ensures the column exists; for pre-V15 SQLite the migrator must
detect and fall back to extracting from `metadata->>'codebase'`, see "Source
schema variants" below).

The Phase 1 `SqliteMemoryStore` is the source backend. `0002d-store-impl-bodies.md`
extends it (and the trait) with a windowed reader ordered by `(created_at, id)`
so the migrator can stream rows in deterministic batches without holding the
full result set in RAM. The migrator assumes that reader exists and produces
`MemoryRecord` instances with all D7+D8 columns populated.

---

## File layout

```
crates/vestige-core/src/storage/postgres/migrate_cli.rs   -- D8 body
crates/vestige-mcp/src/bin/cli.rs                          -- D10 clap wiring
tests/phase_2/migrate_test.rs                              -- integration test
```

The migrator lives behind `#[cfg(feature = "postgres-backend")]`. The
`Migrate` clap variant in the CLI is similarly gated. Without the feature,
`vestige` builds and runs exactly as in Phase 1 -- the `migrate` subcommand
simply does not exist.

---

## Plan struct

```rust
#![cfg(feature = "postgres-backend")]

use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use crate::embedder::Embedder;
use crate::storage::memory_store::MemoryStoreError;

#[derive(Debug, Clone)]
pub struct SqliteToPostgresPlan {
    /// Filesystem path to the source SQLite database. Opened read-only.
    pub sqlite_path: PathBuf,

    /// libpq-style URL for the destination Postgres database.
    pub postgres_url: String,

    /// sqlx pool size for the destination. Default 4. The migrator is
    /// single-writer per table for ordering reasons; extra connections are
    /// only used for the embedding-model registry probe and for the dry-run
    /// COUNT queries that run in parallel with the row scan.
    pub max_connections: u32,

    /// Number of rows per Postgres transaction. Default 500. Larger batches
    /// reduce commit overhead but increase the amount of work a crash
    /// re-runs.
    pub batch_size: usize,

    /// If true, count rows per table and emit a report without writing
    /// anything to Postgres.
    pub dry_run: bool,
}

impl Default for SqliteToPostgresPlan {
    fn default() -> Self {
        Self {
            sqlite_path: PathBuf::new(),
            postgres_url: String::new(),
            max_connections: 4,
            batch_size: 500,
            dry_run: false,
        }
    }
}
```

The struct is public so a future programmatic driver (Rhai script, hero
service, in-process test harness) can call `run_sqlite_to_postgres` without
touching clap.

---

## Report struct

```rust
#[derive(Debug, Default)]
pub struct MigrationReport {
    pub memories_copied: u64,
    pub scheduling_rows: u64,
    pub edges_copied: u64,
    pub review_events_copied: u64,
    pub domains_copied: u64,
    pub users_copied: u64,
    pub groups_copied: u64,
    pub group_memberships_copied: u64,

    /// Per-row failures that did not abort the migrator. Each entry pairs
    /// the source row id (where derivable) with the error that caused it to
    /// be skipped. Rows whose UUID cannot be parsed are reported with
    /// `Uuid::nil()` and a descriptive `MemoryStoreError::InvalidInput`.
    pub errors: Vec<(Uuid, MemoryStoreError)>,
}
```

`errors.is_empty()` is the "clean migration" check. The CLI prints
`errors.len()` at the end and exits non-zero if it is positive.

Counts are the number of rows the migrator either inserted or skipped due to
ON CONFLICT. They reflect what the source presented, not what the destination
ended up with -- that distinction matters for re-runs: a re-run of a finished
migration reports the same counts but writes zero new rows.

---

## Driver fn

```rust
pub async fn run_sqlite_to_postgres(
    plan: SqliteToPostgresPlan,
    embedder: Arc<dyn Embedder>,
) -> MemoryStoreResult<MigrationReport>;
```

Algorithm, step by step:

### Step 1. Open source SQLite read-only

Build a SQLite URL with `?mode=ro` so the migrator cannot mutate the source
even by accident:

```rust
let src_url = format!(
    "sqlite://{}?mode=ro",
    plan.sqlite_path.display(),
);
let src = SqliteMemoryStore::open_url(&src_url).await?;
```

`SqliteMemoryStore::open_url` is added by `0002d-store-impl-bodies.md` as a
small wrapper over the existing `new` that accepts a fully-formed URL. If the
file does not exist, `MemoryStoreError::Init` propagates.

The source store still runs its own startup-time migrations in `?mode=ro`?
No -- read-only mode rejects writes. The migrator therefore opens the source
twice if the live source DB is older than V15: once writable to bring its
schema forward to V15 (so the D7+D8 columns are present), then re-opens
read-only. Detection: query `user_version` on the source DB before opening
the read-only handle. If it is below V15 and `--allow-source-upgrade` is set,
open writable, run `SqliteMemoryStore::new` (which runs migrations), close,
and re-open read-only. If `--allow-source-upgrade` is not set, fail with a
clear error pointing at the flag. Default: not set; the user must opt in to
modifying their source.

### Step 2. Embedding model registry compatibility check

Read both registries:

```rust
let src_sig = src.registered_model().await?;
let actual = embedder.model_signature();    // ModelSignature
```

If `src_sig` is `Some` and disagrees with `actual` (any of `name`,
`dimension`, `hash`), return:

```rust
MemoryStoreError::ModelMismatch {
    registered_name: src_sig.name,
    registered_dim: src_sig.dimension,
    registered_hash: src_sig.hash,
    actual_name: actual.name,
    actual_dim: actual.dimension,
    actual_hash: actual.hash,
}
```

The CLI translates this into a message that mentions `0002g`'s `--reembed`
command as the recovery path. Do NOT silently re-encode here; that is a
separate concern with its own flag set, performance profile, and HNSW
rebuild.

If `src_sig` is `None` (source never had an embedding model -- empty DB or
pre-Phase-1), use the actual embedder's signature for the destination
registry. Memory rows whose `embedding` column is NULL stay NULL on the
destination side.

### Step 3. Open destination Postgres

```rust
let dst = PgMemoryStore::connect(&plan.postgres_url, plan.max_connections).await?;
```

`PgMemoryStore::connect` (per `0002d-store-impl-bodies.md`) runs the
`sqlx::migrate!` macro internally, which idempotently applies `0001_init`
and `0002_hnsw`. Re-running the migrator against an already-initialised
destination is fine.

Stamp the registry on the destination:

```rust
let sig = src_sig.unwrap_or_else(|| embedder.model_signature());
dst.register_model(&sig).await?;
```

`register_model` is idempotent in the Postgres backend: it upserts the single
registry row, and (per ADR 0002 D2) it runs the `ALTER TABLE knowledge_nodes
ALTER COLUMN embedding TYPE vector($N)` typmod stamp inside its body. The
ALTER is itself idempotent: pgvector accepts the same typmod twice as a no-op.

### Step 4. Verify schema

Not really a separate step -- `PgMemoryStore::connect` already calls
`sqlx::migrate!` and the `register_model` call already stamps the typmod.
Listed here for documentation: this is the point at which the destination is
known to be schema-correct for the source's embedding dimension.

### Step 5. Copy `users`, `groups`, `group_memberships` first

These tables exist for both pre-Phase-3 and Phase-3-aware sources because
ADR 0002 D7 reserved them in V15 of the SQLite schema. Phase 2 single-user
deployments have exactly one user row (`local`) and zero groups, but the
migrator does not assume that: it copies whatever is present.

The bootstrap user `00000000-0000-0000-0000-000000000001` is inserted by
`0001_init.up.sql` on the destination. The source's bootstrap row collides
with the destination's; `ON CONFLICT (id) DO NOTHING` resolves the collision
silently.

Pseudocode:

```rust
let mut tx = dst.pool().begin().await?;
let mut report = MigrationReport::default();

for batch in src.stream_users(plan.batch_size).await? {
    for u in batch? {
        sqlx::query!(
            "INSERT INTO users (id, handle, display_name, created_at, metadata) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (id) DO NOTHING",
            u.id, u.handle, u.display_name, u.created_at, u.metadata,
        ).execute(&mut *tx).await?;
        report.users_copied += 1;
    }
}
tx.commit().await?;
```

Repeat the same shape for `groups` and `group_memberships`. The membership
table has a composite primary key `(user_id, group_id)`:

```rust
"INSERT INTO group_memberships (user_id, group_id, role, joined_at) \
 VALUES ($1, $2, $3, $4) \
 ON CONFLICT (user_id, group_id) DO NOTHING",
```

The `stream_users` / `stream_groups` / `stream_memberships` reader methods on
`SqliteMemoryStore` are introduced by `0002d-store-impl-bodies.md`. They
return `BoxStream<MemoryStoreResult<Vec<...>>>` to keep the migrator
backend-agnostic.

If the source SQLite predates V15 -- the V15 migration is the one that
introduces these tables -- they simply do not exist. The reader detects
their absence at open time and returns an empty stream. See "Source schema
variants" below.

### Step 6. Copy `knowledge_nodes` in batches

Stream the source ordered by `(created_at, id)`. The cursor key is the
last-seen `(created_at, id)` pair; the reader uses keyset pagination so
restarts pick up where the previous run left off:

```sql
SELECT ...
FROM knowledge_nodes
WHERE (created_at, id) > ($cursor_ts, $cursor_id)
ORDER BY created_at, id
LIMIT $batch_size
```

For each batch:

```rust
let mut tx = dst.pool().begin().await?;
for record in batch {
    // D7 + D8 columns are all on MemoryRecord by Phase 2.
    let groups: Vec<Uuid> = record.shared_with_groups.clone();

    let result = sqlx::query!(
        "INSERT INTO knowledge_nodes ( \
            id, content, node_type, tags, embedding, \
            created_at, updated_at, metadata, \
            owner_user_id, visibility, shared_with_groups, \
            codebase, domains, domain_scores \
         ) VALUES ( \
            $1, $2, $3, $4, $5::vector, \
            $6, $7, $8, \
            $9, $10, $11, \
            $12, $13, $14::jsonb \
         ) \
         ON CONFLICT (id) DO NOTHING",
        record.id,
        record.content,
        record.node_type,
        &record.tags,
        record.embedding.as_deref().map(pgvector::Vector::from),
        record.created_at,
        record.updated_at,
        record.metadata,
        record.owner_user_id,
        record.visibility,
        &groups,
        record.codebase,
        &record.domains,
        serde_json::to_value(&record.domain_scores)
            .unwrap_or(serde_json::Value::Object(Default::default())),
    )
    .execute(&mut *tx)
    .await;

    match result {
        Ok(_) => report.memories_copied += 1,
        Err(e) => report
            .errors
            .push((record.id, MemoryStoreError::from(e))),
    }
}
tx.commit().await?;
```

Notes:

- `embedding` is `Option<Vec<f32>>` on `MemoryRecord`. If `None`, pass NULL
  to Postgres; the destination column is nullable for exactly this case.
- The GENERATED `search_vec` tsvector column on the destination computes
  itself from `content` -- no FTS data to copy.
- Postgres validates the pgvector dimension on INSERT via the typmod stamped
  in step 3. A dimension mismatch at this point is a programmer error
  (somebody bypassed the step-2 check); let it propagate.

Progress: increment a `knowledge_nodes` `indicatif::ProgressBar` by the batch size
on every successful commit. Log INFO every 1000 rows via `tracing`:

```rust
if report.memories_copied % 1000 == 0 {
    tracing::info!(
        memories_copied = report.memories_copied,
        "migrate: knowledge_nodes batch committed",
    );
}
```

### Step 7. Copy `scheduling`

One row per memory. Read with the same windowed-stream API (keyed by
`memory_id`, which is already a UUID with a stable sort order):

```rust
"INSERT INTO scheduling ( \
    memory_id, stability, difficulty, retrievability, \
    last_review, next_review, reps, lapses \
 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
 ON CONFLICT (memory_id) DO NOTHING",
```

The conflict here is the foreign-key target's primary key, which is what
makes the upsert safe on restart. Increment `report.scheduling_rows`.

### Step 8. Copy `edges`

```rust
"INSERT INTO edges ( \
    source_id, target_id, edge_type, weight, created_at \
 ) VALUES ($1, $2, $3, $4, $5) \
 ON CONFLICT (source_id, target_id) DO NOTHING",
```

The `edges` table's PK is `(source_id, target_id)` (the Phase 2 schema does
not distinguish edge types in the key -- a memory pair has exactly one edge
with one type). Increment `report.edges_copied`.

### Step 9. Copy `review_events`

```rust
"INSERT INTO review_events ( \
    id, memory_id, occurred_at, retrievability_before, retrievability_after, \
    rating, kind, metadata \
 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
 ON CONFLICT (id) DO NOTHING",
```

`review_events` is an append-only log. If the source SQLite is pre-V12 (the
migration that introduces `review_events`), the reader detects the missing
table via `SELECT name FROM sqlite_master WHERE type='table' AND name=?`
returning empty and yields an empty stream. The migrator increments
`report.review_events_copied` only when rows actually arrive.

### Step 10. Copy `domains`

Phase 4 table. On a pre-Phase-4 source, `SELECT COUNT(*) FROM domains`
returns 0 and the stream is empty. The migrator does not skip the table;
it iterates and finds nothing. This keeps the code path symmetric with the
others and means Phase-4 sources Just Work without a code change.

```rust
"INSERT INTO domains ( \
    id, label, centroid, top_terms, memory_count, created_at \
 ) VALUES ($1, $2, $3::vector, $4, $5, $6) \
 ON CONFLICT (id) DO NOTHING",
```

Increment `report.domains_copied`.

### Step 11. Progress bars

`indicatif::MultiProgress` with one `ProgressBar` per table. Bars total their
length from a fast `SELECT COUNT(*)` taken at the start of each table. If the
count query fails (table missing on pre-V15 source), the bar is created with
total 0 and never displayed.

Per-bar style:

```rust
let style = ProgressStyle::with_template(
    "{prefix:>14} [{bar:40.cyan/blue}] {pos}/{len} ({per_sec}, eta {eta})",
)
.unwrap()
.progress_chars("##-");
```

Prefix names: `knowledge_nodes`, `scheduling`, `edges`, `review_events`, `domains`,
`users`, `groups`, `memberships`.

### Step 12. Dry-run path

If `plan.dry_run` is true, skip steps 3, 5-10 (no writes) and instead run
`SELECT COUNT(*) FROM <each table>` on the source. Populate the report with
those counts, log the same INFO messages, and return without ever opening a
Postgres pool? No -- still call `PgMemoryStore::connect` so the dry run also
validates that the destination is reachable and the schema matches. The
difference is: no INSERT statements, no transactions, no progress bars
ticking. Print the report at the end and exit.

---

## Idempotency

Re-running `vestige migrate copy ...` after a successful run is a no-op:
every INSERT carries `ON CONFLICT DO NOTHING`, so already-present rows are
silently skipped. The report counts grow by zero; the destination is
unchanged.

Re-running after a crash mid-batch is safe in the same way. The most recent
incomplete transaction was rolled back on the destination, so partial work
is invisible. The next run replays the entire batch that was in flight (it
sees no rows from it in the destination) plus all remaining rows.

If a single row is corrupted on the source (e.g., a UUID column with a
non-UUID string, malformed `metadata` JSON, etc.), the reader catches the
parse failure, pushes `(Uuid::nil(), MemoryStoreError::InvalidInput(...))`
to `report.errors`, and continues. The migrator never aborts on a single bad
row. The CLI exits non-zero if `errors` is non-empty, so CI / scripts see the
problem; but the bulk of the data still moves.

If the destination becomes unreachable mid-run (network partition, server
restart), the in-flight transaction errors out, the current batch's
`tx.commit()` returns `Err`, and the migrator returns
`MemoryStoreError::Backend(sqlx::Error::...)`. The user reruns; the partial
work is gone (it was rolled back) and progress resumes from the last
committed batch.

---

## Embedding model match check

Read both registries up front (step 2). The check is exact: name AND
dimension AND hash must match. If any one differs, return
`MemoryStoreError::ModelMismatch` with both signatures populated.

The CLI catches that variant specifically and prints:

```
error: embedding model mismatch between source and destination

  source registered:  nomic-ai/nomic-embed-text-v1.5 (dim 768, hash abcd...)
  embedder presented: BAAI/bge-large-en-v1.5         (dim 1024, hash 1234...)

Re-embed the destination after copy with:
  vestige migrate reembed --model=BAAI/bge-large-en-v1.5

or rerun this command with the original embedder so the dimensions match.
```

The migrator does NOT call into the embedder during copy. Vectors flow from
SQLite BLOB to Postgres `vector` unchanged. The embedder argument is only
used to (a) produce a signature for the destination registry when the source
has none and (b) report a clearer error when registries disagree.

Re-embedding lives in `0002g-reembed.md`. That sub-plan's body assumes the
destination is already populated, so the user's workflow is:

1. `vestige migrate copy ...` (this sub-plan; may fail with `ModelMismatch`)
2. `vestige migrate copy --reembed-after ...` -- not added in Phase 2; the
   user runs the two commands in sequence
3. `vestige migrate reembed --model=...` (next sub-plan)

A future Phase 3 ergonomic improvement could fuse copy-then-reembed behind a
single flag. Not in Phase 2 scope.

---

## CLI wiring

Edit `crates/vestige-mcp/src/bin/cli.rs`. Add a feature-gated `Migrate`
variant to the existing `Commands` enum. The full additions:

```rust
use std::path::PathBuf;

#[derive(Subcommand)]
enum Commands {
    // existing variants: Stats, Health, Consolidate, Update, Sandwich,
    // Restore, Backup, Export, PortableExport, PortableImport, Sync,
    // Gc, Dashboard, Ingest, Serve ...

    /// Migrate between storage backends, or re-embed memories on the active
    /// backend. Available when compiled with --features postgres-backend.
    #[cfg(feature = "postgres-backend")]
    Migrate(MigrateArgs),
}

#[derive(clap::Args)]
#[cfg(feature = "postgres-backend")]
struct MigrateArgs {
    #[command(subcommand)]
    action: MigrateAction,
}

#[derive(Subcommand)]
#[cfg(feature = "postgres-backend")]
enum MigrateAction {
    /// Copy all memories, scheduling state, edges, and review events from a
    /// SQLite database to a Postgres database. Idempotent.
    Copy {
        /// Source backend name. Currently only "sqlite" is accepted.
        #[arg(long)]
        from: String,

        /// Destination backend name. Currently only "postgres" is accepted.
        #[arg(long)]
        to: String,

        /// Path to the source SQLite database file.
        #[arg(long)]
        sqlite_path: PathBuf,

        /// libpq-style URL for the destination Postgres database.
        #[arg(long)]
        postgres_url: String,

        /// Rows per Postgres transaction.
        #[arg(long, default_value_t = 500)]
        batch_size: usize,

        /// sqlx pool size for the destination.
        #[arg(long, default_value_t = 4)]
        max_connections: u32,

        /// Permit the migrator to bring the source SQLite forward to V15
        /// (the schema version that introduces the D7+D8 columns) by
        /// re-opening it writable, running migrations, and closing it.
        /// Without this flag, a pre-V15 source fails fast.
        #[arg(long)]
        allow_source_upgrade: bool,

        /// Count rows per table and print a report without writing anything
        /// to Postgres.
        #[arg(long)]
        dry_run: bool,
    },

    /// Re-embed all memories on the active Postgres backend with a new
    /// embedder. See sub-plan 0002g for the body.
    Reembed {
        /// Embedder name (e.g., "BAAI/bge-large-en-v1.5"). Resolved via
        /// the Phase 1 embedder factory.
        #[arg(long)]
        model: String,

        /// libpq-style URL for the Postgres database to re-embed in.
        #[arg(long)]
        postgres_url: String,

        /// Rows per embedder batch.
        #[arg(long, default_value_t = 128)]
        batch_size: usize,

        /// Drop the HNSW index before re-embedding (recommended; rebuild is
        /// faster than incremental updates).
        #[arg(long, default_value_t = true)]
        drop_hnsw_first: bool,

        /// Rebuild HNSW with CREATE INDEX CONCURRENTLY. Slower but does not
        /// hold AccessExclusiveLock.
        #[arg(long)]
        concurrent_index: bool,

        /// sqlx pool size for the destination.
        #[arg(long, default_value_t = 4)]
        max_connections: u32,

        /// Plan the work and print estimates without making changes.
        #[arg(long)]
        dry_run: bool,
    },
}
```

Argument validation for `Copy`:

```rust
fn validate_copy_backends(from: &str, to: &str) -> anyhow::Result<()> {
    match (from, to) {
        ("sqlite", "postgres") => Ok(()),
        (other_from, "postgres") => anyhow::bail!(
            "unsupported source backend: {}. Only 'sqlite' is accepted as --from in Phase 2.",
            other_from,
        ),
        ("sqlite", other_to) => anyhow::bail!(
            "unsupported destination backend: {}. Only 'postgres' is accepted as --to in Phase 2.",
            other_to,
        ),
        (other_from, other_to) => anyhow::bail!(
            "unsupported migration direction: {} -> {}. Only 'sqlite' -> 'postgres' is accepted in Phase 2.",
            other_from, other_to,
        ),
    }
}
```

Wire the new variant in the `main` match:

```rust
match cli.command {
    // ... existing arms ...

    #[cfg(feature = "postgres-backend")]
    Commands::Migrate(args) => match args.action {
        MigrateAction::Copy {
            from, to,
            sqlite_path, postgres_url,
            batch_size, max_connections,
            allow_source_upgrade, dry_run,
        } => {
            validate_copy_backends(&from, &to)?;
            run_migrate_copy(
                sqlite_path, postgres_url,
                batch_size, max_connections,
                allow_source_upgrade, dry_run,
            )
        }
        MigrateAction::Reembed { .. } => {
            // Body implemented in sub-plan 0002g.
            run_migrate_reembed(/* ... */)
        }
    },
}
```

`run_migrate_copy` is a thin wrapper that:

1. Builds a `SqliteToPostgresPlan` from the clap args.
2. Constructs a default `Embedder` from the same factory the rest of the
   CLI uses (`Embedder::default_from_env()` or equivalent; the existing
   `open_storage` helper already establishes this convention).
3. Starts a tokio runtime if one is not already running. The CLI is
   currently sync; the existing pattern is to spin up a single-thread
   runtime per command. Reuse that.
4. Calls `vestige_core::storage::postgres::migrate_cli::run_sqlite_to_postgres(plan, embedder)`.
5. Prints the report and exits with the appropriate status code.

Pseudocode:

```rust
fn run_migrate_copy(
    sqlite_path: PathBuf,
    postgres_url: String,
    batch_size: usize,
    max_connections: u32,
    allow_source_upgrade: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    use vestige_core::storage::postgres::migrate_cli::{
        run_sqlite_to_postgres, SqliteToPostgresPlan,
    };

    let plan = SqliteToPostgresPlan {
        sqlite_path,
        postgres_url,
        max_connections,
        batch_size,
        dry_run,
    };

    let embedder = build_default_embedder()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let report = runtime.block_on(run_sqlite_to_postgres(plan, embedder))
        .context("migrate copy failed")?;

    print_migration_report(&report);

    if report.errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("migrate copy completed with {} row errors", report.errors.len())
    }
}
```

`print_migration_report` writes a colored summary block matching the style
of `run_stats` and `run_health`: section header, then one labeled row per
counter, then an "Errors" subsection (only when non-empty) listing
`(uuid, error)` pairs truncated to the first 20 entries with a "+N more"
footer.

---

## Source-row mapping

The Phase 1 `MemoryRecord` (after the Phase 2 amendment in `0002d`) has
these D7+D8 fields:

```rust
pub struct MemoryRecord {
    pub id: Uuid,
    pub content: String,
    pub node_type: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
    pub domains: Vec<String>,
    pub domain_scores: HashMap<String, f64>,

    // D7
    pub owner_user_id: Uuid,
    pub visibility: String,        // 'private' | 'group' | 'public'
    pub shared_with_groups: Vec<Uuid>,

    // D8
    pub codebase: Option<String>,
}
```

The SQLite backend stores most of these directly, but `shared_with_groups`
is JSON-encoded into a `TEXT` column because SQLite has no array type. The
Phase 1 amendment's V15 column definition is:

```sql
shared_with_groups TEXT NOT NULL DEFAULT '[]'
```

The `SqliteMemoryStore` reader parses this with `serde_json::from_str::<Vec<Uuid>>`.
On parse failure (malformed JSON, non-UUID strings), the migrator behavior
is:

```rust
let groups: Vec<Uuid> = match serde_json::from_str(&raw_groups) {
    Ok(v) => v,
    Err(e) => {
        report.errors.push((
            row.id,
            MemoryStoreError::InvalidInput(format!(
                "shared_with_groups JSON parse failed: {e}",
            )),
        ));
        Vec::new()
    }
};
```

A row with malformed `shared_with_groups` is still copied; the destination
gets an empty group array. This keeps the migrator on the side of "best
effort, never lose memories".

The `visibility` column is `TEXT NOT NULL DEFAULT 'private'` on both sides.
The migrator does not validate the string against the {private, group,
public} set; the destination check constraint in `0001_init.up.sql` enforces
that:

```sql
CHECK (visibility IN ('private', 'group', 'public'))
```

A bad value on the source becomes a Postgres CHECK violation on insert,
which is caught and pushed to `errors`.

`owner_user_id` is `UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001'`
on both sides. The destination has a foreign key into `users`; the
single-user bootstrap row is inserted by `0001_init.up.sql`. Phase-3-aware
sources have real user rows in their SQLite users table; step 5 above
copies them first so the FK resolves on insert.

`codebase` is nullable `TEXT` on both sides. Direct copy, no special
handling.

`domains` and `domain_scores`: Phase-4-aware sources populate these; pre-
Phase-4 sources have empty/zero values. Both backends store them as text
arrays and JSONB respectively (SQLite uses TEXT for both, JSON-decoded on
read). Direct copy.

`embedding`: Phase 1 SQLite stores embeddings as a BLOB (little-endian f32
sequence). The Phase 1 reader decodes to `Vec<f32>`. The migrator hands the
`Vec<f32>` directly to `pgvector::Vector::from`, which converts to the
postgres wire format. No precision loss.

`metadata`: SQLite TEXT containing JSON. The reader parses to
`serde_json::Value`. The migrator passes it through to a JSONB column.
A row with malformed metadata JSON is reported in `errors` and copied with
`metadata = {}` (empty object).

### Source schema variants

The migrator must work against several historical SQLite schema versions:

| Version | What is missing | Migrator behavior |
|---------|-----------------|-------------------|
| V11 | no `review_events` table | review_events stream is empty, count = 0 |
| V12-V14 | has review_events; no D7+D8 columns | step 5 streams are empty; D7+D8 read from metadata fallback (see below) |
| V15 | all D7+D8 columns and tables | direct read |

For pre-V15 sources without `--allow-source-upgrade`, the migrator fails
with a clear message naming the flag. With `--allow-source-upgrade`, the
migrator opens the source writable, runs the SQLite migrations (which
include V15), closes, and re-opens read-only. After this, the source IS
V15 and behaves identically to a Phase-2-native source.

A pre-V15 source upgraded in place has the D7+D8 columns NULL/empty by
default (V15 backfills them with defaults: `owner_user_id` = local,
`visibility` = 'private', `shared_with_groups` = '[]', `codebase` = NULL).
The migrator copies those defaults to the destination unchanged.

---

## Tracing / logs

Emit INFO logs at three points:

1. Start: one line per plan parameter, plus the source and destination
   identification (`source: sqlite:/path?mode=ro, destination: postgres://...`).
2. Mid-flight: every 1000 rows on the `knowledge_nodes` table only. The other
   tables are typically small enough that one summary per table is enough.
3. End: print the full `MigrationReport` at INFO level, plus duration.

```rust
let started = Instant::now();
tracing::info!(
    sqlite_path = %plan.sqlite_path.display(),
    postgres_url = %obfuscate_password(&plan.postgres_url),
    batch_size = plan.batch_size,
    dry_run = plan.dry_run,
    "migrate: starting sqlite -> postgres copy",
);

// ... per-table sections ...

tracing::info!(
    memories = report.memories_copied,
    scheduling = report.scheduling_rows,
    edges = report.edges_copied,
    review_events = report.review_events_copied,
    domains = report.domains_copied,
    users = report.users_copied,
    groups = report.groups_copied,
    memberships = report.group_memberships_copied,
    errors = report.errors.len(),
    duration_ms = started.elapsed().as_millis() as u64,
    "migrate: complete",
);
```

`obfuscate_password` masks the password segment of the libpq URL so logs are
safe to share. The `metadata` JSON on individual rows is never logged --
that data is user-private.

Per-row errors are logged at WARN with the row id and the error string. The
counts in the final INFO line tell the user how many to expect.

---

## Verification

Integration test under `tests/phase_2/migrate_test.rs`. Add it next to
`tests/phase_2/common/mod.rs` (the testcontainer harness from `0002h`).

The test:

1. Creates an in-memory `SqliteMemoryStore` at a tempfile path. Runs
   migrations to V15.
2. Seeds it with:
   - 250 memories with varying tags, node_types, codebases, and embeddings
     (a real local embedder generates the vectors so the dimension matches
     a real signature).
   - 250 scheduling rows (one per memory).
   - 50 edges between random memory pairs.
   - 50 review events.
   - Optional: 3 user rows + 2 groups + 4 memberships to exercise the D7
     path.
   - Optional: 5 domain rows to exercise the Phase 4 path.
3. Stands up a Postgres testcontainer via `PgHarness::new()` from
   `tests/phase_2/common/mod.rs`.
4. Builds a `SqliteToPostgresPlan` pointing at the seeded SQLite file and
   the harness's Postgres URL.
5. Calls `run_sqlite_to_postgres(plan, embedder).await`.
6. Asserts:
   - `report.memories_copied == 250`
   - `report.scheduling_rows == 250`
   - `report.edges_copied == 50`
   - `report.review_events_copied == 50`
   - `report.users_copied == 4` (3 plus bootstrap)
   - `report.groups_copied == 2`
   - `report.group_memberships_copied == 4`
   - `report.domains_copied == 5`
   - `report.errors.is_empty()`
7. Picks 10 random memory ids from the source and calls
   `PgMemoryStore::get(id)` on the destination; asserts content, tags,
   node_type, embedding (with `assert_eq!` on the `Vec<f32>` -- exact
   equality, not approximate), owner_user_id, visibility, shared_with_groups,
   and codebase all match the source.
8. Re-runs the migrator with the same plan. Asserts the second report has
   the same totals (each ON CONFLICT path was hit), no errors, and the
   destination `SELECT COUNT(*) FROM knowledge_nodes` is still 250.
9. Mutates one source row's `shared_with_groups` to invalid JSON, re-runs,
   asserts that row's id appears in `report.errors` and the destination
   row's `shared_with_groups` is `{}` (empty).
10. Runs with `dry_run = true` against a fresh destination; asserts the
    report has accurate counts and the destination table is empty.

Additional cases (each its own `#[tokio::test]`):

- `migrate_pre_v15_source_without_upgrade_fails`: seed a V14 SQLite, call
  without `allow_source_upgrade`, assert `Err(MemoryStoreError::Init)` or
  similar with a message naming the flag.
- `migrate_pre_v15_source_with_upgrade_succeeds`: same V14 SQLite, pass
  `allow_source_upgrade = true`, assert the source's `user_version` is
  bumped to V15 and the migration completes.
- `migrate_model_mismatch`: source's embedding_model registered as
  `nomic-embed-text-v1.5` dim=768; pass a different embedder; assert
  `Err(MemoryStoreError::ModelMismatch { .. })` with both signatures
  populated.

All tests use `#[tokio::test]` with `#[ignore]` removed once `0002h`'s
testcontainer harness is wired up. CI runs them in the
`postgres-backend` feature matrix only.

---

## Acceptance criteria

The sub-plan is complete when:

1. `cargo build --features postgres-backend -p vestige-core` succeeds.
2. `cargo build --features postgres-backend -p vestige-mcp` succeeds.
3. `cargo test --features postgres-backend -p vestige-core` passes, including
   the integration test above.
4. `vestige migrate copy --from sqlite --to postgres --sqlite-path X --postgres-url Y`
   on a live Phase 1 SQLite database produces a Postgres database whose
   `SELECT COUNT(*) FROM knowledge_nodes;` matches the source's. Manual smoke test
   against the user's own `~/.vestige/vestige.db` is the gold-standard check.
5. Re-running the same command produces zero new rows and zero errors.
6. `vestige migrate copy --from sqlite --to postgres ... --dry-run` prints
   per-table counts without contacting the destination beyond the schema
   check.
7. `vestige migrate copy --from <other> --to postgres ...` rejects with a
   clear message naming the supported pairs.
8. `vestige migrate copy ...` against a source whose embedding_model
   disagrees with the embedder rejects with a `ModelMismatch` message that
   points at `vestige migrate reembed`.
9. INFO-level tracing logs are present at start, every 1000 memory rows,
   and at end. Passwords in URLs are not logged in cleartext.
10. The `Reembed` clap variant compiles with `todo!()` or a stub body and
    is filled in by `0002g-reembed.md`.
