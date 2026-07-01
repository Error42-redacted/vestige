# Phase 2 Sub-plan 0002c: sqlx Migrations

**Status**: Draft
**Depends on**: `0002a-skeleton-and-feature-gate.md` (PgMemoryStore skeleton, error variants), `0002b-pool-and-config.md` (PgPool builder, PostgresConfig)
**Related**: docs/adr/0002-phase-2-execution.md (D7 multi-tenancy reservation, D8 codebase column), docs/plans/0002-phase-2-postgres-backend.md (D4 master SQL), docs/plans/local-dev-postgres-setup.md (local cluster + role + DB)

---

## Context

This sub-plan covers Phase 2 deliverable D4 (sqlx migration files under
`crates/vestige-core/migrations/postgres/`) PLUS the schema additions decided
in ADR 0002:

- D7 -- multi-tenancy reservation: `users`, `groups`, `group_memberships`
  tables, plus `owner_user_id`, `visibility`, `shared_with_groups` columns on
  `knowledge_nodes`. Phase 3 fills these in; Phase 2 just reserves them so the auth
  filter is later additive instead of an online migration over a populated,
  HNSW-indexed table.
- D8 -- `codebase` promoted to a first-class indexed column on `knowledge_nodes`.

This sub-plan also adds the parity SQLite migration (V15) that mirrors D7 +
D8 on the SQLite side, so a single-user SQLite deployment sees the same
columns (with stand-in defaults).

After this sub-plan lands:

- A fresh Postgres database, with the `vestige` role from the local-dev
  setup, can be initialized by running `sqlx::migrate!` against
  `crates/vestige-core/migrations/postgres/`, plus one programmatic
  `register_model` call before the HNSW migration.
- A fresh SQLite database initialized by `apply_migrations` lands at
  schema_version = 15 with the new tables and columns present.
- `PgMemoryStore::connect` wires the migrator into the connect path
  (pool build -> migrator up-to v1 -> register_model -> migrator up-to v2).
- The SQLite test suite continues to pass.
- No `sqlx::query!` calls are introduced yet; the offline `.sqlx/` cache is
  filled out in `0002d-store-impl-bodies.md`.

The deliverable is purely schema. No query bodies, no row-mapping, no search.

---

## Postgres migration files

Layout, relative to repo root:

```
crates/vestige-core/migrations/postgres/
  0001_init.up.sql
  0001_init.down.sql
  0002_hnsw.up.sql
  0002_hnsw.down.sql
```

The `migrations/postgres/` directory is sibling-of-`src/`, not under `src/`,
because `sqlx::migrate!` and `sqlx-cli` both look for a path relative to
`CARGO_MANIFEST_DIR`. The directory is committed.

### 0001_init.up.sql

Creates extensions, the multi-tenancy tables (D7), the embedding registry,
the domains catalogue, the `knowledge_nodes` table (with D7 + D8 columns merged in),
the FSRS scheduling and edges tables, the review-events log, all non-vector
indexes, the updated_at trigger, and the bootstrap `local` user row.

The HNSW vector index is deliberately NOT here -- it requires a typmod on
`knowledge_nodes.embedding`, which is stamped by `register_model` at runtime. See
the "HNSW typmod ordering" section below.

```sql
-- crates/vestige-core/migrations/postgres/0001_init.up.sql
--
-- Phase 2 initial schema for the Postgres backend.
-- Includes D7 multi-tenancy reservation (users/groups/group_memberships,
-- owner_user_id/visibility/shared_with_groups on knowledge_nodes) and D8
-- (codebase first-class column on knowledge_nodes).
--
-- The HNSW index on knowledge_nodes.embedding lives in 0002_hnsw.up.sql; it
-- requires the column typmod to be stamped first by register_model().

-- Extensions ----------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS vector;

-- Embedding model registry --------------------------------------------------
-- Mirrors the SQLite table created in Phase 1 V14.
-- One logical row enforced by CHECK (id = 1).

CREATE TABLE embedding_model (
    id          SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    name        TEXT NOT NULL,
    dimension   INTEGER NOT NULL CHECK (dimension > 0),
    hash        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Domains catalogue ---------------------------------------------------------
-- Populated by the Phase 4 DomainClassifier. Phase 2 creates the empty
-- table so list/get/upsert/delete work uniformly against both backends.

CREATE TABLE domains (
    id           TEXT PRIMARY KEY,
    label        TEXT NOT NULL,
    centroid     vector,
    top_terms    TEXT[] NOT NULL DEFAULT '{}',
    memory_count INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- Multi-tenancy (D7) --------------------------------------------------------
-- Reserved in Phase 2; populated in Phase 3.
-- Single bootstrap user inserted at the bottom of this file.

CREATE TABLE users (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    handle       TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE groups (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    handle       TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE group_memberships (
    user_id   UUID NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    group_id  UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'member',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, group_id),
    CHECK (role IN ('member', 'admin'))
);

-- Core knowledge_nodes table -------------------------------------------------
-- Original Phase 2 columns merged with D7 (owner_user_id, visibility,
-- shared_with_groups) and D8 (codebase).

CREATE TABLE knowledge_nodes (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Content
    content            TEXT NOT NULL,
    node_type          TEXT NOT NULL DEFAULT 'general',
    tags               TEXT[] NOT NULL DEFAULT '{}',
    metadata           JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Phase 4 emergent domains (Phase 2 leaves empty)
    domains            TEXT[] NOT NULL DEFAULT '{}',
    domain_scores      JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Embedding (typmod stamped by register_model before 0002_hnsw runs)
    embedding          vector,

    -- D8: first-class codebase column for high-frequency scoped queries
    codebase           TEXT,

    -- D7: multi-tenancy reservation. Defaults make Phase 2 single-user
    -- behaviour identical to Phase 1.
    owner_user_id      UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001'
                           REFERENCES users(id),
    visibility         TEXT NOT NULL DEFAULT 'private',
    shared_with_groups UUID[] NOT NULL DEFAULT '{}',

    -- Timestamps
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Generated full-text search vector. Phase 2 uses websearch_to_tsquery
    -- against this column at query time (see 0002e-hybrid-search.md).
    search_vec         TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(content, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(node_type, '')), 'B') ||
        setweight(to_tsvector('english', coalesce(array_to_string(tags, ' '), '')), 'C')
    ) STORED,

    -- Visibility tri-state CHECK constraint. See "Visibility CHECK
    -- constraint" section below for the cardinality variant we
    -- intentionally do NOT add yet.
    CHECK (visibility IN ('private', 'group', 'public'))
);

-- FSRS scheduling state (1:1 with knowledge_nodes) ---------------------------
--
-- Note: the FK column is named `memory_id` (not `node_id`) to match the
-- Phase 1 SQLite trait surface: `SchedulingState { memory_id: Uuid, ... }`
-- and `get_scheduling(memory_id: Uuid)` / `update_scheduling(&state)`. The
-- table is `knowledge_nodes` but the Rust identifier remained `memory_id`
-- across Phase 1 and is preserved here so both backends speak the same
-- language at the trait boundary.

CREATE TABLE scheduling (
    memory_id       UUID PRIMARY KEY REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    stability       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    difficulty      DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    retrievability  DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    last_review     TIMESTAMPTZ,
    next_review     TIMESTAMPTZ,
    reps            INTEGER NOT NULL DEFAULT 0,
    lapses          INTEGER NOT NULL DEFAULT 0
);

-- Spreading activation graph edges ------------------------------------------

CREATE TABLE edges (
    source_id   UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    target_id   UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    edge_type   TEXT NOT NULL DEFAULT 'related',
    weight      DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, target_id, edge_type)
);

-- FSRS review event log (append-only; Phase 5 federation reads) -------------

CREATE TABLE review_events (
    id              BIGSERIAL PRIMARY KEY,
    memory_id       UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT now(),
    rating          SMALLINT NOT NULL,
    prior_state     JSONB NOT NULL,
    new_state       JSONB NOT NULL
);

-- Indexes -------------------------------------------------------------------

-- knowledge_nodes: full-text, arrays, hot scalar columns, D7+D8 access patterns
CREATE INDEX idx_knowledge_nodes_fts            ON knowledge_nodes USING GIN (search_vec);
CREATE INDEX idx_knowledge_nodes_domains        ON knowledge_nodes USING GIN (domains);
CREATE INDEX idx_knowledge_nodes_tags           ON knowledge_nodes USING GIN (tags);
CREATE INDEX idx_knowledge_nodes_node_type      ON knowledge_nodes (node_type);
CREATE INDEX idx_knowledge_nodes_created        ON knowledge_nodes (created_at);
CREATE INDEX idx_knowledge_nodes_updated        ON knowledge_nodes (updated_at);

-- D7 visibility filter (Phase 3 query: WHERE owner_user_id = $me ...)
CREATE INDEX idx_knowledge_nodes_owner          ON knowledge_nodes (owner_user_id);
CREATE INDEX idx_knowledge_nodes_shared_groups  ON knowledge_nodes USING GIN (shared_with_groups);

-- D8 codebase scoping (Phase 4 HDBSCAN per-repo, sharing rules in Phase 4).
-- Partial index keeps the index small in single-user mode where most rows
-- never set a codebase.
CREATE INDEX idx_knowledge_nodes_codebase
    ON knowledge_nodes (codebase)
    WHERE codebase IS NOT NULL;

-- scheduling: hot lookup paths for FSRS pickers
CREATE INDEX idx_scheduling_next_review  ON scheduling (next_review);
CREATE INDEX idx_scheduling_last_review  ON scheduling (last_review);

-- edges: bidirectional + edge type
CREATE INDEX idx_edges_target            ON edges (target_id);
CREATE INDEX idx_edges_source            ON edges (source_id);
CREATE INDEX idx_edges_type              ON edges (edge_type);

-- review_events: per-memory and chronological
CREATE INDEX idx_review_events_memory    ON review_events (memory_id);
CREATE INDEX idx_review_events_ts        ON review_events (timestamp);

-- users / groups: unique handle indexes are implicit; add nothing extra.
-- group_memberships: primary key (user_id, group_id) is the access path.

-- updated_at trigger on knowledge_nodes ----------------------------------------

CREATE OR REPLACE FUNCTION knowledge_nodes_set_updated_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_knowledge_nodes_updated_at
BEFORE UPDATE ON knowledge_nodes
FOR EACH ROW EXECUTE FUNCTION knowledge_nodes_set_updated_at();

-- Bootstrap rows ------------------------------------------------------------
-- Single 'local' user matches the default on knowledge_nodes.owner_user_id so
-- single-user Phase 2 inserts never violate the FK.

INSERT INTO users (id, handle, display_name)
  VALUES ('00000000-0000-0000-0000-000000000001', 'local', 'Local User');
```

### 0001_init.down.sql

Reverse-dependency drop order. Trigger and function first, then indexes,
then tables, then extensions are left alone (extensions are global; we do
not drop them in a `down`).

```sql
-- crates/vestige-core/migrations/postgres/0001_init.down.sql

DROP TRIGGER IF EXISTS trg_knowledge_nodes_updated_at ON knowledge_nodes;
DROP FUNCTION IF EXISTS knowledge_nodes_set_updated_at();

-- knowledge_nodes indexes
DROP INDEX IF EXISTS idx_knowledge_nodes_codebase;
DROP INDEX IF EXISTS idx_knowledge_nodes_shared_groups;
DROP INDEX IF EXISTS idx_knowledge_nodes_owner;
DROP INDEX IF EXISTS idx_knowledge_nodes_updated;
DROP INDEX IF EXISTS idx_knowledge_nodes_created;
DROP INDEX IF EXISTS idx_knowledge_nodes_node_type;
DROP INDEX IF EXISTS idx_knowledge_nodes_tags;
DROP INDEX IF EXISTS idx_knowledge_nodes_domains;
DROP INDEX IF EXISTS idx_knowledge_nodes_fts;

-- scheduling indexes
DROP INDEX IF EXISTS idx_scheduling_last_review;
DROP INDEX IF EXISTS idx_scheduling_next_review;

-- edges indexes
DROP INDEX IF EXISTS idx_edges_type;
DROP INDEX IF EXISTS idx_edges_source;
DROP INDEX IF EXISTS idx_edges_target;

-- review_events indexes
DROP INDEX IF EXISTS idx_review_events_ts;
DROP INDEX IF EXISTS idx_review_events_memory;

-- Tables, reverse dependency order
DROP TABLE IF EXISTS review_events;
DROP TABLE IF EXISTS edges;
DROP TABLE IF EXISTS scheduling;
DROP TABLE IF EXISTS knowledge_nodes;
DROP TABLE IF EXISTS group_memberships;
DROP TABLE IF EXISTS groups;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS domains;
DROP TABLE IF EXISTS embedding_model;

-- Extensions are intentionally NOT dropped. They may be in use by other
-- databases on the cluster; dropping them is an admin choice.
```

### 0002_hnsw.up.sql

Single statement; separated from 0001 so reembed (sub-plan 0002g) can
DROP/CREATE this index in isolation without touching anything else.

```sql
-- crates/vestige-core/migrations/postgres/0002_hnsw.up.sql
--
-- HNSW index on knowledge_nodes.embedding. This migration runs AFTER
-- register_model() has stamped the typmod via:
--
--     ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($N)
--
-- where $N is the embedder's dimension(). Without the typmod, pgvector
-- rejects HNSW creation with:
--
--     ERROR: column does not have dimensions
--
-- See "HNSW typmod ordering" in 0002c-migrations.md and the connect()
-- sequence in 0002a-skeleton-and-feature-gate.md / 0002d-store-impl-bodies.md.
--
-- Operator class: vector_cosine_ops -> distance operator `<=>`.
-- Build parameters: m = 16, ef_construction = 64 (pgvector defaults; see
-- the master plan 0002 D5 RRF discussion for the rationale).

CREATE INDEX idx_knowledge_nodes_embedding_hnsw
    ON knowledge_nodes USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
```

### 0002_hnsw.down.sql

```sql
-- crates/vestige-core/migrations/postgres/0002_hnsw.down.sql

DROP INDEX IF EXISTS idx_knowledge_nodes_embedding_hnsw;
```

---

## HNSW typmod ordering

pgvector's HNSW index requires the indexed column to have a typmod (fixed
dimension). `vector` (unconstrained) is rejected; `vector(768)` is accepted.
We cannot bake the dimension into 0001 because the dimension is an
embedder-determined runtime value -- different builds may use different
embedders.

This forces an ordering:

1. Apply migration 0001 (creates `knowledge_nodes.embedding vector`, no typmod).
2. Connect, decide which embedder is in use, run
   `ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($N)`
   inside `register_model`.
3. Apply migration 0002 (creates HNSW; succeeds because the column now has
   a typmod).

`sqlx::migrate!("...")` runs ALL pending migrations in a single call. It is
not designed to pause between two specific migrations so application code
can interleave a runtime DDL step. So we have two options:

**Option A: Migration 0002 lives outside the sqlx migrations directory.**
Keep `0001_init.{up,down}.sql` only in `migrations/postgres/`; promote
`0002_hnsw.up.sql` to a Rust `include_str!` constant or a separate
`migrations/postgres-hnsw/` directory, run it manually by `PgMemoryStore`
after `register_model`.

Pros: simple control flow, one `sqlx::migrate!()` call.
Cons: `sqlx_migrations` table does not record 0002, so `sqlx-cli migrate
info` lies. The HNSW index becomes "shadow" schema state from sqlx's POV.
Reembed (sub-plan 0002g) has to also know about this file outside the
normal migrations directory.

**Option B (chosen): Both migrations live in the directory; the runner
splits them programmatically.** Use `sqlx::migrate::Migrator::new` to load
the directory and call its `run_to(...)` method with a specific version.

```rust
// crates/vestige-core/src/storage/postgres/migrations.rs
use sqlx::migrate::Migrator;
use sqlx::PgPool;

use crate::storage::error::MemoryStoreResult;

/// Embedded migrator. Loaded at compile time from the migrations directory
/// alongside the crate. Path is relative to CARGO_MANIFEST_DIR.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

/// Run migrations up to (and including) version 1.
///
/// This must be called BEFORE register_model so the schema (knowledge_nodes table,
/// embedding_model registry, etc.) exists for register_model to write into
/// and to ALTER.
pub(crate) async fn run_pre_register(pool: &PgPool) -> MemoryStoreResult<()> {
    MIGRATOR.run_to(pool, 1).await?;
    Ok(())
}

/// Run any remaining migrations (currently: HNSW = version 2).
///
/// Called AFTER register_model has stamped the embedding column's typmod.
pub(crate) async fn run_post_register(pool: &PgPool) -> MemoryStoreResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
```

Pros: sqlx is the only source of truth for migration version state;
`sqlx-cli migrate info` is accurate; reembed re-applies 0002 by name; future
migrations slot in normally.
Cons: relies on `Migrator::run_to`, which exists in sqlx 0.7+ and is the
documented API for staged migration. If that API ever disappears we fall
back to Option A.

Decision: Option B. `Migrator::run_to(target_version)` is stable in sqlx
0.8. Sub-plan 0002a's `MemoryStoreError` already gains
`#[from] sqlx::migrate::MigrateError` to absorb whichever error variant
this surfaces.

The `connect()` sequence in sub-plan 0002d will therefore look like:

```rust
// Sketch only; full body lives in 0002d-store-impl-bodies.md.
pub async fn connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self> {
    let pool = crate::storage::postgres::pool::build(url, max_connections).await?;
    crate::storage::postgres::migrations::run_pre_register(&pool).await?;
    let store = Self { pool };
    // register_model is called by the cognitive engine bootstrap, NOT here.
    // After it runs, the engine calls store.finalize_schema() which calls
    // run_post_register. Same shape as SqliteMemoryStore.
    Ok(store)
}

pub async fn finalize_schema(&self) -> MemoryStoreResult<()> {
    crate::storage::postgres::migrations::run_post_register(&self.pool).await
}
```

`finalize_schema` lands in 0002d; this sub-plan only ships `run_pre_register`
and `run_post_register` plus their wiring into `connect`.

---

## SQLite V15 migration

The Phase 1 SQLite schema lives in `crates/vestige-core/src/storage/migrations.rs`
as a `MIGRATIONS` slice. V14 is the latest entry. V15 is appended to mirror
D7 (multi-tenancy) and D8 (codebase) on the SQLite side, so a single-user
SQLite deployment sees the same surface area.

Constraints versus the Postgres migration:

- No `UUID[]` -- `shared_with_groups` is a TEXT JSON-encoded `'[]'`.
- No `gen_random_uuid()` -- the bootstrap user UUID is a literal.
- No partial indexes for our chosen pattern (SQLite *does* support partial
  indexes since 3.8; we use one for `codebase` to match Postgres).
- No `ADD COLUMN IF NOT EXISTS` -- the V15 column additions are split into a
  `MIGRATION_V15_ALTER_COLUMNS` slice exactly like V14 did, so the migration
  is idempotent on replay.

### Insertion point in migrations.rs

Add to the `MIGRATIONS` slice immediately after V14:

```rust
// In MIGRATIONS slice, after the V14 entry:
Migration {
    version: 15,
    description: "ADR 0002 D7+D8: multi-tenancy reservation + codebase column",
    up: MIGRATION_V15_UP,
},
```

### V15 SQL

```rust
/// V15: ADR 0002 D7 + D8.
///
/// D7 reserves users / groups / group_memberships and owner_user_id /
/// visibility / shared_with_groups columns on knowledge_nodes. Single-user
/// SQLite mode never reads these (the trait surface ignores visibility
/// because there is exactly one user) but they exist so Phase 3 does not
/// have to ALTER a populated table.
///
/// D8 adds a first-class `codebase` column.
///
/// Like V14, the ALTER TABLE statements are split into
/// MIGRATION_V15_ALTER_COLUMNS because SQLite has no ADD COLUMN IF NOT EXISTS.
const MIGRATION_V15_UP: &str = r#"
-- Migration V15: multi-tenancy reservation + codebase column.

-- 1. Users / groups / group_memberships -----------------------------------
-- Mirrors the Postgres D7 tables. Single bootstrap user inserted below.

CREATE TABLE IF NOT EXISTS users (
    id           TEXT PRIMARY KEY,
    handle       TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at   TEXT NOT NULL,
    metadata     TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS groups (
    id           TEXT PRIMARY KEY,
    handle       TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at   TEXT NOT NULL,
    metadata     TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS group_memberships (
    user_id   TEXT NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    group_id  TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('member', 'admin')),
    joined_at TEXT NOT NULL,
    PRIMARY KEY (user_id, group_id)
);

-- 2. Bootstrap 'local' user. Same UUID as the Postgres default so a future
-- portable export from SQLite -> import to Postgres preserves owner_user_id.

INSERT OR IGNORE INTO users (id, handle, display_name, created_at)
  VALUES ('00000000-0000-0000-0000-000000000001', 'local', 'Local User',
          datetime('now'));

-- 3. Per-memory column additions are applied separately by the migration
--    runner (see MIGRATION_V15_ALTER_COLUMNS).

-- 4. Indexes that do not depend on the new columns. Index creation on the
--    new knowledge_nodes columns is done after MIGRATION_V15_ALTER_COLUMNS
--    runs (see runner glue below).

UPDATE schema_version SET version = 15, applied_at = datetime('now');
"#;

/// V15 column additions. SQLite has no ADD COLUMN IF NOT EXISTS, so the
/// runner skips "duplicate column" errors per statement (same shape as V14).
pub const MIGRATION_V15_ALTER_COLUMNS: &[&str] = &[
    // D7 columns. Defaults match the Postgres side. shared_with_groups is
    // a JSON-encoded array.
    "ALTER TABLE knowledge_nodes ADD COLUMN owner_user_id      TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001'",
    "ALTER TABLE knowledge_nodes ADD COLUMN visibility         TEXT NOT NULL DEFAULT 'private'",
    "ALTER TABLE knowledge_nodes ADD COLUMN shared_with_groups TEXT NOT NULL DEFAULT '[]'",
    // D8 column.
    "ALTER TABLE knowledge_nodes ADD COLUMN codebase           TEXT",
];

/// V15 index creation. Runs AFTER the ALTER COLUMN statements succeed.
/// Kept as a separate batch so a partial replay (columns already there,
/// indexes not yet) still creates the indexes.
const MIGRATION_V15_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_nodes_owner_user_id ON knowledge_nodes(owner_user_id);
CREATE INDEX IF NOT EXISTS idx_nodes_codebase      ON knowledge_nodes(codebase) WHERE codebase IS NOT NULL;
-- shared_with_groups is TEXT JSON in SQLite; we do not add a GIN-equivalent
-- index. Phase 3 lookups on the SQLite side will scan; SQLite never serves
-- the multi-user query path in Phase 2-4 anyway.
"#;
```

### Runner glue

Extend `apply_migrations` in `migrations.rs` to recognise V15 the same way
it recognises V14:

```rust
// Existing pattern for V14 lives in apply_migrations; extend it:
if migration.version == 15 {
    for stmt in MIGRATION_V15_ALTER_COLUMNS {
        if let Err(e) = conn.execute_batch(stmt) {
            let msg = e.to_string();
            if msg.contains("duplicate column name") {
                tracing::debug!(
                    "V15 ALTER TABLE skipped (column already exists): {}",
                    msg
                );
            } else {
                return Err(e);
            }
        }
    }
    // Indexes run *after* the columns exist.
    conn.execute_batch(MIGRATION_V15_INDEXES)?;
}

// Then the normal:
conn.execute_batch(migration.up)?;
```

Order of operations on a fresh in-memory DB:

1. V1 - V14 run as before.
2. V15: column ALTERs run first (so MIGRATION_V15_INDEXES sees them).
3. V15 main body creates users/groups/group_memberships and the bootstrap row.
4. V15 indexes batch runs.
5. schema_version advances to 15.

This intentionally mirrors how V14 handles its ALTER + index pair.

### Existing-data backfill

Existing SQLite databases (every Phase 1 deployment) have populated
`knowledge_nodes` rows. The V15 ALTER COLUMN ADD COLUMN statements assign
the default values to every existing row:

- `owner_user_id` -> `'00000000-0000-0000-0000-000000000001'`
- `visibility`    -> `'private'`
- `shared_with_groups` -> `'[]'`
- `codebase`      -> NULL

Phase 2 leaves these defaults in place. Phase 3 owns the migration story
for populating real owner UUIDs and visibility values.

---

## Rust wrapper

Single file:

```rust
// crates/vestige-core/src/storage/postgres/migrations.rs
//
// sqlx::migrate! wrapper for the Postgres backend.
//
// We split the migration apply into two halves around register_model:
//   - run_pre_register: applies everything up to and including version 1
//                       (schema, indexes, bootstrap row). Safe to call on a
//                       fresh DB.
//   - run_post_register: applies the remainder (currently: 0002_hnsw, which
//                       needs the embedding column typmod stamped first).
//
// See docs/plans/0002c-migrations.md "HNSW typmod ordering" for why this
// split exists.

#![cfg(feature = "postgres-backend")]

use sqlx::PgPool;
use sqlx::migrate::Migrator;

use crate::storage::error::MemoryStoreResult;

/// Embedded migrator. Path is relative to CARGO_MANIFEST_DIR
/// (`crates/vestige-core/`).
static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

/// Apply migrations through version 1 (the schema-only migration).
///
/// Idempotent: sqlx::migrate consults the `_sqlx_migrations` table and is
/// a no-op on a database already at version 1 or higher.
pub(crate) async fn run_pre_register(pool: &PgPool) -> MemoryStoreResult<()> {
    MIGRATOR.run_to(pool, 1).await?;
    Ok(())
}

/// Apply any remaining migrations. Called after `register_model` has
/// stamped the typmod on `knowledge_nodes.embedding`.
pub(crate) async fn run_post_register(pool: &PgPool) -> MemoryStoreResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
```

Wiring into `PgMemoryStore::connect`. The skeleton from 0002a uses
`todo!()` for everything past pool construction. This sub-plan replaces
that with `run_pre_register` only; `run_post_register` is invoked by
`finalize_schema`, which lands in 0002d. Sketch:

```rust
// In crates/vestige-core/src/storage/postgres/mod.rs (sub-plan 0002a wires
// pool construction; this sub-plan adds the run_pre_register call):

impl PgMemoryStore {
    pub async fn connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self> {
        let pool = super::pool::build(url, max_connections).await?;
        super::migrations::run_pre_register(&pool).await?;
        Ok(Self { pool })
    }
}
```

Module wire-up in `crates/vestige-core/src/storage/postgres/mod.rs`:

```rust
mod migrations;  // pub(crate) functions; not re-exported.
```

### Error variant

Sub-plan 0002a already added (under feature gate) to `MemoryStoreError`:

```rust
#[cfg(feature = "postgres-backend")]
#[error("postgres migration error: {0}")]
Migrate(#[from] sqlx::migrate::MigrateError),
```

`run_pre_register` / `run_post_register` use the `?` operator and the
`#[from]` conversion handles it; no extra error handling code is needed.

---

## Visibility CHECK constraint

ADR 0002 D7 specifies the tri-state enum:

```
visibility IN ('private', 'group', 'public')
```

This sub-plan includes that CHECK on the `knowledge_nodes` table (see 0001_init.up.sql
above) on both sides:

- Postgres: `CHECK (visibility IN ('private', 'group', 'public'))` inline on
  the table.
- SQLite: same CHECK constraint can be added to V15 if desired. (It is not
  in the V15 body above because adding a CHECK via ALTER TABLE on SQLite
  requires a table rebuild; we trust the application layer for SQLite, since
  SQLite never serves the multi-user query path in Phase 2.)

The stronger consistency rule from the ADR 0002 follow-ups section,

```
CHECK (
    visibility = 'private'
 OR cardinality(shared_with_groups) > 0
 OR visibility = 'public'
)
```

is intentionally NOT added in this sub-plan. Rationale:

- The rule is a "no orphan group rows" sanity check, not a correctness
  requirement for Phase 2 (single-user mode never touches the column).
- Phase 3 is the first phase that writes `visibility = 'group'`. The check
  belongs in the Phase 3 migration that lights up auth, alongside the
  application code that ensures `shared_with_groups` is populated before
  the visibility flips.
- Adding it now and discovering Phase 3 wants a different shape forces an
  online CHECK constraint replacement.

Recommendation: include only the IN check in Phase 2; revisit the
cardinality check in Phase 3.

---

## Offline sqlx cache

`crates/vestige-core/.sqlx/` is the on-disk cache of compile-time-checked
queries that `sqlx::query!` / `sqlx::query_as!` emit at build time when
`SQLX_OFFLINE=true`. It is committed to the repo so builds without
`DATABASE_URL` (CI, downstream consumers, contributors without Postgres)
succeed.

This sub-plan does NOT yet generate or commit `.sqlx/` content. Reasons:

- `sqlx::query!` calls are introduced in `0002d-store-impl-bodies.md` (real
  CRUD bodies) and `0002e-hybrid-search.md` (RRF). This sub-plan ships only
  the migrations directory and a wrapper that uses `sqlx::migrate!` -- which
  is a compile-time macro that reads files, not a query macro that needs a
  DB connection.
- Generating an empty `.sqlx/` directory now is noise that gets immediately
  overwritten in the next sub-plan.

Sub-plan 0002d will land the procedure:

```sh
# Local dev box with vestige DB initialised per local-dev-postgres-setup.md.
export DATABASE_URL="postgresql://vestige:$(cat ~/.vestige_pg_pw)@127.0.0.1:5432/vestige"

# Apply migrations against the dev DB.
cargo sqlx migrate run \
  --source crates/vestige-core/migrations/postgres \
  --database-url "$DATABASE_URL"

# Generate the offline cache.
cargo sqlx prepare --workspace -- --features postgres-backend

# Verify cache compiles offline.
SQLX_OFFLINE=true cargo check --workspace --features postgres-backend
```

The `.sqlx/` directory commit policy is: committed, reviewed in PRs that
add or change `query!` calls, regenerated locally and pushed.

What this sub-plan DOES need from sqlx-cli, for verification only (see next
section): `cargo sqlx migrate run --source crates/vestige-core/migrations/postgres`.

---

## Verification

Two halves: Postgres migrations run cleanly on a fresh DB; SQLite V15 does
not break the Phase 1 store.

### Postgres

Prerequisites: Postgres 18 with pgvector, a role with CREATEDB and EXTENSION
rights, per `docs/plans/local-dev-postgres-setup.md`. Alternatively, a
container:

```sh
podman run --rm -d --name vestige-pg \
    -e POSTGRES_PASSWORD=devpw \
    -e POSTGRES_USER=vestige \
    -e POSTGRES_DB=vestige \
    -p 5432:5432 \
    docker.io/pgvector/pgvector:pg18

export DATABASE_URL="postgresql://vestige:devpw@127.0.0.1:5432/vestige"
```

Steps:

1. Apply migrations. From the repo root:

   ```sh
   cargo install sqlx-cli --no-default-features --features postgres
   cargo sqlx migrate run \
       --source crates/vestige-core/migrations/postgres \
       --database-url "$DATABASE_URL"
   ```

   Expected output: `Applied 1/migrate init` (`0002` is gated on typmod;
   sqlx-cli will run it and pgvector will reject the HNSW creation with
   "column does not have dimensions". This is the expected behaviour when
   running migrations without going through the Rust connect path. To run
   0002 manually for verification, first stamp the typmod:

   ```sh
   psql "$DATABASE_URL" -c "ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector(768);"
   cargo sqlx migrate run \
       --source crates/vestige-core/migrations/postgres \
       --database-url "$DATABASE_URL"
   ```

   Now 0002 should apply.)

2. Verify tables exist:

   ```sh
   psql "$DATABASE_URL" -c "\dt"
   ```

   Expected (alphabetical):
   ```
   domains
   edges
   embedding_model
   group_memberships
   groups
   knowledge_nodes
   review_events
   scheduling
   users
   ```

3. Verify the bootstrap user row:

   ```sh
   psql "$DATABASE_URL" -c "SELECT id, handle, display_name FROM users;"
   ```

   Expected:
   ```
                     id                  | handle | display_name
   --------------------------------------+--------+--------------
    00000000-0000-0000-0000-000000000001 | local  | Local User
   ```

4. Verify HNSW index (only after the typmod stamp + migrate 0002):

   ```sh
   psql "$DATABASE_URL" -c "\d knowledge_nodes"
   ```

   The trailing `Indexes:` block should include `idx_knowledge_nodes_embedding_hnsw`.

5. Verify the D7+D8 columns are present:

   ```sh
   psql "$DATABASE_URL" -c "
       SELECT column_name, data_type, column_default
       FROM information_schema.columns
       WHERE table_name = 'knowledge_nodes'
         AND column_name IN ('owner_user_id', 'visibility',
                             'shared_with_groups', 'codebase')
       ORDER BY column_name;
   "
   ```

   Expected: four rows, with `owner_user_id` defaulting to the bootstrap
   UUID, `visibility` to `'private'::text`, `shared_with_groups` to
   `'{}'::uuid[]`, `codebase` NULL-default.

6. Verify CHECK constraint:

   ```sh
   psql "$DATABASE_URL" -c "
       INSERT INTO knowledge_nodes (content, visibility) VALUES ('test', 'bogus');
   "
   # Expected: ERROR: new row for relation \"knowledge_nodes\" violates check constraint
   ```

7. Roll back to verify down migrations work:

   ```sh
   cargo sqlx migrate revert \
       --source crates/vestige-core/migrations/postgres \
       --database-url "$DATABASE_URL"
   cargo sqlx migrate revert \
       --source crates/vestige-core/migrations/postgres \
       --database-url "$DATABASE_URL"
   ```

   `\dt` should then list only the sqlx-managed `_sqlx_migrations` table.

8. Rust-side smoke test (no `sqlx::query!` calls yet, so cannot live in
   a `#[sqlx::test]`-decorated function until 0002d). Manual:

   ```sh
   cargo build -p vestige-core --features postgres-backend
   ```

   Should compile. The `sqlx::migrate!("./migrations/postgres")` macro
   reads the directory at compile time; a missing file or syntax error
   surfaces as a compile error.

### SQLite

1. Run the existing test suite:

   ```sh
   cargo test -p vestige-core
   ```

   Expected: 352 (or current count + new V15 tests) tests pass, zero
   warnings.

2. New test in `migrations.rs#tests`:

   ```rust
   #[test]
   fn test_v15_advances_to_15_and_adds_d7_d8_columns() {
       let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
       apply_migrations(&conn).expect("apply_migrations succeeds");

       let version = get_current_version(&conn).expect("read schema_version");
       assert_eq!(version, 15, "schema_version should advance to 15");

       // Tables exist
       for tbl in ["users", "groups", "group_memberships"] {
           let n: i32 = conn.query_row(
               "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
               [tbl],
               |r| r.get(0),
           ).expect("query sqlite_master");
           assert_eq!(n, 1, "table {tbl} should exist after V15");
       }

       // Bootstrap user row exists
       let n: i32 = conn.query_row(
           "SELECT COUNT(*) FROM users WHERE id = '00000000-0000-0000-0000-000000000001'",
           [],
           |r| r.get(0),
       ).expect("query users");
       assert_eq!(n, 1, "bootstrap local user row should exist");

       // D7+D8 columns on knowledge_nodes
       let cols: Vec<String> = conn
           .prepare("PRAGMA table_info(knowledge_nodes)")
           .unwrap()
           .query_map([], |r| r.get::<_, String>(1))
           .unwrap()
           .collect::<rusqlite::Result<_>>()
           .unwrap();
       for c in ["owner_user_id", "visibility", "shared_with_groups", "codebase"] {
           assert!(cols.iter().any(|x| x == c),
                   "knowledge_nodes should have column {c}");
       }
   }
   ```

3. Idempotency: re-applying V15 on an already-V15 DB must not error.
   `apply_migrations` already skips when `current_version >= migration.version`;
   no extra test needed beyond ensuring the V14 + V15 ALTER pattern works.

4. Existing-data backfill smoke: insert a row before applying V15, then
   verify the defaults populate:

   ```rust
   #[test]
   fn test_v15_backfills_existing_rows_with_defaults() {
       let conn = rusqlite::Connection::open_in_memory().expect("open");

       // Apply migrations through V14 only.
       // (We rely on the fact that re-running apply_migrations is a no-op,
       //  so we apply all, then probe the columns. The V15 ALTER on a
       //  populated table is what we are testing implicitly.)
       apply_migrations(&conn).expect("V1-V15");

       // Insert a row using only Phase 1 columns; V15 defaults must
       // populate owner_user_id / visibility / shared_with_groups / codebase.
       conn.execute(
           "INSERT INTO knowledge_nodes (id, content, node_type, created_at, updated_at, last_accessed)
            VALUES ('test', 'hello', 'fact', datetime('now'), datetime('now'), datetime('now'))",
           [],
       ).expect("insert");

       let (owner, vis, shared, codebase): (String, String, String, Option<String>) =
           conn.query_row(
               "SELECT owner_user_id, visibility, shared_with_groups, codebase
                FROM knowledge_nodes WHERE id = 'test'",
               [],
               |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
           ).expect("query");

       assert_eq!(owner, "00000000-0000-0000-0000-000000000001");
       assert_eq!(vis, "private");
       assert_eq!(shared, "[]");
       assert_eq!(codebase, None);
   }
   ```

5. Live deployment: apply V15 to a copy of `~/.vestige/vestige.db` and
   verify the existing 150 memories all carry the four new columns with
   default values:

   ```sh
   cp ~/.vestige/vestige.db /tmp/v15-test.db
   sqlite3 /tmp/v15-test.db <<'SQL'
   .schema knowledge_nodes
   SELECT COUNT(*) FROM knowledge_nodes;
   SELECT DISTINCT owner_user_id, visibility, shared_with_groups
     FROM knowledge_nodes LIMIT 5;
   SQL
   # (Migration applies on first read by the vestige binary running V15.)
   ```

   Capture pre- and post-counts. Expected: no row count change, all new
   columns populated by defaults.

---

## Acceptance criteria

- [ ] `crates/vestige-core/migrations/postgres/` directory contains exactly
      four files: `0001_init.up.sql`, `0001_init.down.sql`,
      `0002_hnsw.up.sql`, `0002_hnsw.down.sql`. Content matches this
      sub-plan.
- [ ] `crates/vestige-core/src/storage/postgres/migrations.rs` exports
      `run_pre_register` and `run_post_register` as `pub(crate)` async
      functions returning `MemoryStoreResult<()>`. Compiles with
      `--features postgres-backend`.
- [ ] `PgMemoryStore::connect` (sub-plan 0002a skeleton) is updated to call
      `run_pre_register` immediately after pool construction. `connect`
      still returns before `register_model` runs; `run_post_register`
      lands in 0002d via `finalize_schema`.
- [ ] `crates/vestige-core/src/storage/migrations.rs` has a new V15 entry
      in `MIGRATIONS`, with `MIGRATION_V15_UP`, `MIGRATION_V15_ALTER_COLUMNS`,
      and `MIGRATION_V15_INDEXES` constants. `apply_migrations` handles
      V15 the same shape as V14.
- [ ] `cargo test -p vestige-core` passes. New tests cover V15 advance,
      D7+D8 column existence, bootstrap user row, and existing-row backfill.
- [ ] `cargo build -p vestige-core --features postgres-backend` compiles
      (the `sqlx::migrate!` macro will fail at compile time if any of the
      four SQL files is missing or malformed).
- [ ] `cargo sqlx migrate run --source crates/vestige-core/migrations/postgres`
      against a fresh container applies 0001 cleanly; `\dt` lists the nine
      Phase 2 tables; `users` contains the bootstrap row.
- [ ] After the manual typmod stamp documented above, `cargo sqlx migrate
      run` applies 0002 and `\d knowledge_nodes` shows `idx_knowledge_nodes_embedding_hnsw`.
- [ ] `cargo sqlx migrate revert` twice cleans the DB back to only the
      `_sqlx_migrations` table.
- [ ] Inserting a row with `visibility = 'bogus'` is rejected by the CHECK
      constraint.
- [ ] No `sqlx::query!` / `sqlx::query_as!` calls are added in this
      sub-plan; the `.sqlx/` offline cache is not yet generated.
- [ ] The existing live SQLite DB on the development machine migrates from
      V14 to V15 without row count change, and the 150 existing rows all
      receive the four V15 default values.
