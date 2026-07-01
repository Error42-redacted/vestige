# Phase 2 Sub-Plan 0002d -- Store Implementation Bodies

**Status**: Ready
**Depends on**:
- `0002a-skeleton-and-feature-gate.md` -- `PgMemoryStore` struct + trait impl block exist with `todo!()` bodies.
- `0002b-pool-and-config.md` -- `PgPool` is constructable, `MemoryStoreError::Postgres` and `MemoryStoreError::Migrate` variants exist behind the `postgres-backend` feature.
- `0002c-migrations.md` -- the two sqlx migrations (`0001_init`, `0002_hnsw`) exist, the schema is applied on `connect`, the `knowledge_nodes` / `scheduling` / `edges` / `domains` / `embedding_model` / `users` / `groups` / `group_memberships` / `review_events` tables exist with the D7+D8 columns.

This sub-plan replaces every `todo!()` in
`crates/vestige-core/src/storage/postgres/mod.rs` with a real sqlx-backed
body, and adds `crates/vestige-core/src/storage/postgres/registry.rs` with
the `ensure_registry` / `register_model` typmod-stamping logic.

The hybrid `search()` method is the meatiest single body in the backend
(RRF in one SQL statement) and lives in its own sub-plan
(`0002e-hybrid-search.md`). The bodies for the trivial single-branch
variants `fts_search` and `vector_search` are still inside this sub-plan
because they share row-mapping infrastructure with the CRUD bodies.

Out of scope for this sub-plan:
- The full hybrid `search()` -- see `0002e-hybrid-search.md`.
- SQLite -> Postgres migrate CLI -- see `0002f-migrate-cli.md`.
- Re-embed flow -- see `0002g-reembed.md`.
- Phase 3 visibility filter -- explicitly NOT wired in Phase 2; see the
  "Visibility filter posture" section below.

---

## Context

The Phase 1 `MemoryStore` trait surface is defined in
`crates/vestige-core/src/storage/memory_store.rs` and is the source of
truth for method signatures. ADR 0002 D7 added owner / visibility /
shared_with_groups columns to the `knowledge_nodes` table; ADR 0002 D8 promoted
`codebase` to a first-class column. The sqlx bodies in this sub-plan must
write to and read from those columns, but per ADR 0002 D7 they must NOT
filter on them in Phase 2 -- the visibility filter is a Phase 3
deliverable that takes an `AuthContext` parameter.

The semantics of every body must match the SQLite backend's current
behaviour. Where Postgres has native types (`UUID`, `JSONB`, `vector`,
`TEXT[]`, `TIMESTAMPTZ`) we use them directly; the SQLite backend's
RFC3339-string-and-JSON-blob encoding is an artefact of SQLite typing,
not the trait contract.

Compile-time SQL validation uses sqlx's `query!` / `query_as!` macros.
The first time these macros run against a real database in CI they
populate `.sqlx/` query metadata; the metadata file is committed so
offline builds (CI without a live Postgres) succeed.

---

## MemoryRecord type changes

ADR 0002 D7 and D8 added four new columns to the `knowledge_nodes` table.
The `MemoryRecord` struct in
`crates/vestige-core/src/storage/memory_store.rs` must grow matching
fields so the trait surface can carry the data through both backends.
This is an additive change to the public type.

Add to `MemoryRecord` (after the existing `metadata` field):

```rust
/// Owner of this memory. Defaults to the local bootstrap user
/// (`00000000-0000-0000-0000-000000000001`) in single-user mode.
pub owner_user_id: Uuid,

/// Tri-state visibility. ADR 0002 D7.
pub visibility: Visibility,

/// Group IDs this memory is shared with when `visibility == Group`.
/// Empty for `Private` and `Public`.
pub shared_with_groups: Vec<Uuid>,

/// First-class codebase tag. ADR 0002 D8. None if the ingest pipeline
/// could not infer one.
pub codebase: Option<String>,
```

Add a new enum next to `MemoryRecord`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Group,
    Public,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Group => "group",
            Self::Public => "public",
        }
    }

    pub fn from_str(s: &str) -> MemoryStoreResult<Self> {
        match s {
            "private" => Ok(Self::Private),
            "group"   => Ok(Self::Group),
            "public"  => Ok(Self::Public),
            other     => Err(MemoryStoreError::Backend(
                format!("unknown visibility value: {other}"),
            )),
        }
    }
}

impl Default for Visibility {
    fn default() -> Self { Self::Private }
}
```

`MemoryRecord` already derives `Serialize` and `Deserialize`; the new
fields ride along automatically. Two callers must change as part of this
sub-plan:

1. **SQLite backend (V15 migration ships in `0001b-sqlite-split.md` or
   the same Phase 1 amendment branch)**: the SQLite backend reads the
   four new columns out of `knowledge_nodes` (V15 added them) and
   populates the new fields in `Self::node_to_record`. Bootstrap user
   ID is the same constant on both backends. Existing call sites that
   construct `MemoryRecord` literally (in tests, in cognitive modules)
   may default-init the four new fields:

   ```rust
   MemoryRecord {
       // ... existing fields ...
       owner_user_id: LOCAL_USER_ID,
       visibility: Visibility::default(),
       shared_with_groups: Vec::new(),
       codebase: None,
       metadata: serde_json::json!({}),
   }
   ```

   A single `pub const LOCAL_USER_ID: Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000001");`
   in `storage::memory_store` provides the bootstrap constant.

2. **Cognitive modules that build `MemoryRecord` from the ingest
   pipeline**: the ingest path already captures `codebase` in metadata
   (see ADR 0002 D8). Lift it from `metadata.codebase` to the new
   `codebase` field at the boundary where `MemoryRecord` is built. The
   `metadata.codebase` JSON key is removed in the same commit; the
   column is now the only source of truth.

The change is purely additive to the trait surface -- no method
signatures change. Backwards compatibility for stored data (in the
SQLite case) comes from V15 defaulting the new columns to `'private'`
and the bootstrap user. The Postgres schema applies the same defaults
in `0001_init.up.sql`.

---

## Registry module

New file: `crates/vestige-core/src/storage/postgres/registry.rs`.

```rust
#![cfg(feature = "postgres-backend")]

//! Embedding-model registry for the Postgres backend.
//!
//! The `embedding_model` table stores exactly one row (id = 1) describing
//! the model whose vectors live in `knowledge_nodes.embedding`. Phase 2 enforces
//! that the active embedder matches the registered model on every write;
//! re-embed (`0002g-reembed.md`) is the only flow allowed to change the
//! row.
//!
//! The pgvector column `knowledge_nodes.embedding` is created in
//! `0001_init.up.sql` with a placeholder type (`vector`) -- no typmod.
//! On first connect we stamp the real dimension via
//! `ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($N)` so the
//! HNSW index (created in `0002_hnsw.up.sql`) sees a sized type.

use sqlx::PgPool;

use crate::storage::memory_store::{
    MemoryStoreError, MemoryStoreResult, ModelSignature,
};

/// Look up the registered signature, if any. Returns `Ok(None)` on a
/// fresh database.
pub(crate) async fn fetch_registry(
    pool: &PgPool,
) -> MemoryStoreResult<Option<ModelSignature>> {
    let row = sqlx::query!(
        r#"
        SELECT name, dimension, hash
        FROM embedding_model
        WHERE id = 1
        "#
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ModelSignature {
        name: r.name,
        dimension: r.dimension as usize,
        hash: r.hash,
    }))
}

/// First-ever call inserts the row and stamps the typmod on
/// `knowledge_nodes.embedding`. Subsequent calls compare against the stored
/// row and return `ModelMismatch` if any field differs.
pub(crate) async fn ensure_registry(
    pool: &PgPool,
    sig: &ModelSignature,
) -> MemoryStoreResult<()> {
    let existing = fetch_registry(pool).await?;

    match existing {
        None => {
            sqlx::query!(
                r#"
                INSERT INTO embedding_model (id, name, dimension, hash)
                VALUES (1, $1, $2, $3)
                "#,
                sig.name,
                sig.dimension as i32,
                sig.hash,
            )
            .execute(pool)
            .await?;

            stamp_vector_typmod(pool, sig.dimension).await?;
            Ok(())
        }
        Some(reg) if reg == *sig => Ok(()),
        Some(reg) => Err(MemoryStoreError::ModelMismatch {
            registered_name: reg.name,
            registered_dim:  reg.dimension,
            registered_hash: reg.hash,
            actual_name: sig.name.clone(),
            actual_dim:  sig.dimension,
            actual_hash: sig.hash.clone(),
        }),
    }
}

/// Called only by the re-embed flow (`0002g-reembed.md`) after a full
/// re-encode has rewritten every row. Updates the registry row and
/// re-stamps the typmod for the new dimension.
pub(crate) async fn update_registry_for_reembed(
    pool: &PgPool,
    sig: &ModelSignature,
) -> MemoryStoreResult<()> {
    sqlx::query!(
        r#"
        UPDATE embedding_model
        SET name = $1, dimension = $2, hash = $3, created_at = now()
        WHERE id = 1
        "#,
        sig.name,
        sig.dimension as i32,
        sig.hash,
    )
    .execute(pool)
    .await?;

    stamp_vector_typmod(pool, sig.dimension).await?;
    Ok(())
}

async fn stamp_vector_typmod(pool: &PgPool, dim: usize) -> MemoryStoreResult<()> {
    // pgvector's typmod is part of the column type, not a bound parameter.
    // `format!` is safe here because `dim` is a `usize` cast to a decimal
    // literal; there is no path for user-controlled SQL to reach this
    // string.
    let ddl = format!(
        "ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector({dim})"
    );
    sqlx::query(&ddl).execute(pool).await?;
    Ok(())
}
```

Wire the new module into `crates/vestige-core/src/storage/postgres/mod.rs`:

```rust
pub(crate) mod registry;
```

The `fetch_registry` / `ensure_registry` functions are reached from the
trait methods `registered_model` and `register_model` (see method bodies
below). `update_registry_for_reembed` is reached only from
`postgres::reembed`, which is filled in by `0002g-reembed.md`.

---

## Method-by-method bodies

Every body below replaces a `todo!()` in
`crates/vestige-core/src/storage/postgres/mod.rs`. Method order matches
the trait declaration in `memory_store.rs`.

Common imports at the top of `mod.rs`:

```rust
use chrono::{DateTime, Utc};
use pgvector::Vector;
use uuid::Uuid;

use crate::storage::memory_store::{
    Domain, HealthStatus, LocalMemoryStore, MemoryEdge, MemoryRecord,
    MemoryStoreError, MemoryStoreResult, ModelSignature, SchedulingState,
    SearchQuery, SearchResult, StoreStats, Visibility,
};
```

Recurring row-to-record helper (private to `mod.rs`):

```rust
fn row_to_record(
    id: Uuid,
    content: String,
    node_type: String,
    tags: Vec<String>,
    domains: Vec<String>,
    domain_scores: serde_json::Value,
    codebase: Option<String>,
    owner_user_id: Uuid,
    visibility: String,
    shared_with_groups: Vec<Uuid>,
    embedding: Option<Vector>,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> MemoryStoreResult<MemoryRecord> {
    let domain_scores: std::collections::HashMap<String, f64> =
        serde_json::from_value(domain_scores).unwrap_or_default();
    let embedding = embedding.map(|v| v.to_vec());
    Ok(MemoryRecord {
        id,
        domains,
        domain_scores,
        content,
        node_type,
        tags,
        embedding,
        created_at,
        updated_at,
        metadata,
        owner_user_id,
        visibility: Visibility::from_str(&visibility)?,
        shared_with_groups,
        codebase,
    })
}
```

### Lifecycle

#### `init`

```rust
async fn init(&self) -> MemoryStoreResult<()>
```

Migrations already ran in `connect`; this is a no-op identical to
SQLite's behaviour.

```rust
async fn init(&self) -> MemoryStoreResult<()> {
    Ok(())
}
```

#### `health_check`

```rust
async fn health_check(&self) -> MemoryStoreResult<HealthStatus>
```

Issue a trivial `SELECT 1`. Pool acquisition errors degrade to
`HealthStatus::Degraded`; any other error path returns `Unavailable`.

```rust
async fn health_check(&self) -> MemoryStoreResult<HealthStatus> {
    match sqlx::query_scalar!("SELECT 1::int")
        .fetch_one(&self.pool)
        .await
    {
        Ok(_) => Ok(HealthStatus::Healthy),
        Err(sqlx::Error::PoolTimedOut) => Ok(HealthStatus::Degraded {
            reason: "pool exhausted".to_string(),
        }),
        Err(e) => Ok(HealthStatus::Unavailable {
            reason: e.to_string(),
        }),
    }
}
```

### Embedding-model registry

#### `registered_model`

```rust
async fn registered_model(&self) -> MemoryStoreResult<Option<ModelSignature>>
```

Thin pass-through to `registry::fetch_registry`. The Postgres backend
does not cache the row in-memory the way the SQLite backend does --
sqlx's prepared-statement cache already keeps the SELECT cheap, and
`registered_model` is not on the hot path.

```rust
async fn registered_model(&self) -> MemoryStoreResult<Option<ModelSignature>> {
    crate::storage::postgres::registry::fetch_registry(&self.pool).await
}
```

#### `register_model`

```rust
async fn register_model(&self, sig: &ModelSignature) -> MemoryStoreResult<()>
```

Delegate to `registry::ensure_registry`, which handles the
"insert + stamp typmod" first-run path and the "compare" subsequent path.

```rust
async fn register_model(&self, sig: &ModelSignature) -> MemoryStoreResult<()> {
    crate::storage::postgres::registry::ensure_registry(&self.pool, sig).await
}
```

### CRUD

#### `insert`

```rust
async fn insert(&self, record: &MemoryRecord) -> MemoryStoreResult<Uuid>
```

Single `INSERT` into `knowledge_nodes` with all D7+D8 columns. Bind embedding
as `Option<pgvector::Vector>` -- pgvector's sqlx integration handles the
typmod check at execution time, so a length mismatch surfaces as
`MemoryStoreError::Postgres`. The caller-supplied UUID is preserved
(same contract as SQLite). Initial scheduling state is inserted in the
same transaction so a memory is never queryable without a scheduling
row.

```rust
async fn insert(&self, record: &MemoryRecord) -> MemoryStoreResult<Uuid> {
    let embedding: Option<Vector> = record
        .embedding
        .as_ref()
        .map(|v| Vector::from(v.clone()));
    let domain_scores = serde_json::to_value(&record.domain_scores)
        .unwrap_or_else(|_| serde_json::json!({}));

    let mut tx = self.pool.begin().await?;

    sqlx::query!(
        r#"
        INSERT INTO knowledge_nodes (
            id,
            owner_user_id,
            visibility,
            shared_with_groups,
            codebase,
            content,
            node_type,
            tags,
            domains,
            domain_scores,
            embedding,
            metadata,
            created_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb,
            $11, $12::jsonb, $13, $14
        )
        "#,
        record.id,
        record.owner_user_id,
        record.visibility.as_str(),
        &record.shared_with_groups as &[Uuid],
        record.codebase.as_deref(),
        record.content,
        record.node_type,
        &record.tags as &[String],
        &record.domains as &[String],
        domain_scores,
        embedding as Option<Vector>,
        record.metadata,
        record.created_at,
        record.updated_at,
    )
    .execute(&mut *tx)
    .await?;

    // Seed scheduling state. Mirrors SQLite defaults from `knowledge_nodes`
    // (stability=1.0, difficulty=0.3, retrievability=1.0, reps=0, lapses=0,
    // next_review = created_at + 1 day).
    sqlx::query!(
        r#"
        INSERT INTO scheduling (
            memory_id, stability, difficulty, retrievability,
            last_review, next_review, reps, lapses
        )
        VALUES ($1, 1.0, 0.3, 1.0, NULL, $2, 0, 0)
        "#,
        record.id,
        record.created_at + chrono::Duration::days(1),
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(record.id)
}
```

Tricky bits:
- `&record.tags as &[String]` -- sqlx requires an explicit slice cast
  to bind a `Vec<String>` as `text[]`.
- `&record.shared_with_groups as &[Uuid]` -- same pattern for `uuid[]`.
- `embedding as Option<Vector>` -- type annotation is mandatory in the
  macro because the inference path bottoms out at a generic; pgvector's
  `Encode` impl resolves only with a known concrete type.
- The `$10::jsonb` and `$12::jsonb` casts force sqlx to encode through
  the JSONB path even if the parameter type-resolves to `JSON`. This
  matters because the migrations created the columns as `JSONB`, and
  sqlx 0.8 does not always pick JSONB without the cast.

#### `get`

```rust
async fn get(&self, id: Uuid) -> MemoryStoreResult<Option<MemoryRecord>>
```

`SELECT *` filtered by primary key. Row mapping goes through
`row_to_record`.

```rust
async fn get(&self, id: Uuid) -> MemoryStoreResult<Option<MemoryRecord>> {
    let row = sqlx::query!(
        r#"
        SELECT
            id            AS "id!: Uuid",
            owner_user_id AS "owner_user_id!: Uuid",
            visibility,
            shared_with_groups AS "shared_with_groups!: Vec<Uuid>",
            codebase,
            content,
            node_type,
            tags          AS "tags!: Vec<String>",
            domains       AS "domains!: Vec<String>",
            domain_scores AS "domain_scores!: serde_json::Value",
            embedding     AS "embedding: Vector",
            metadata      AS "metadata!: serde_json::Value",
            created_at    AS "created_at!: DateTime<Utc>",
            updated_at    AS "updated_at!: DateTime<Utc>"
        FROM knowledge_nodes
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&self.pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    Ok(Some(row_to_record(
        r.id, r.content, r.node_type, r.tags, r.domains,
        r.domain_scores, r.codebase, r.owner_user_id, r.visibility,
        r.shared_with_groups, r.embedding, r.metadata,
        r.created_at, r.updated_at,
    )?))
}
```

The `AS "name!: Type"` annotations tell sqlx the exact Rust type for
each column, which is required for `Vec<Uuid>` (from `uuid[]`) and
`Vector` (from `vector`). The `!` means "trust me, this column is NOT
NULL"; sqlx skips its `Option<T>` wrapping for those columns. The
`embedding` column is nullable, so it gets `Option<Vector>` (no `!`).

#### `update`

```rust
async fn update(&self, record: &MemoryRecord) -> MemoryStoreResult<()>
```

Update everything the caller might have changed. `updated_at` is set
server-side via `now()` so clock drift between hosts does not leak into
the timeline. (If the caller wants to forge `updated_at` -- e.g. the
migrate CLI replaying SQLite timestamps -- it goes through `insert`, not
`update`.) The schema's `BEFORE UPDATE` trigger could replace this; we
write `updated_at = now()` explicitly to be backend-agnostic.

```rust
async fn update(&self, record: &MemoryRecord) -> MemoryStoreResult<()> {
    let embedding: Option<Vector> = record
        .embedding
        .as_ref()
        .map(|v| Vector::from(v.clone()));
    let domain_scores = serde_json::to_value(&record.domain_scores)
        .unwrap_or_else(|_| serde_json::json!({}));

    let rows = sqlx::query!(
        r#"
        UPDATE knowledge_nodes SET
            owner_user_id      = $2,
            visibility         = $3,
            shared_with_groups = $4,
            codebase           = $5,
            content            = $6,
            node_type          = $7,
            tags               = $8,
            domains            = $9,
            domain_scores      = $10::jsonb,
            embedding          = $11,
            metadata           = $12::jsonb,
            updated_at         = now()
        WHERE id = $1
        "#,
        record.id,
        record.owner_user_id,
        record.visibility.as_str(),
        &record.shared_with_groups as &[Uuid],
        record.codebase.as_deref(),
        record.content,
        record.node_type,
        &record.tags as &[String],
        &record.domains as &[String],
        domain_scores,
        embedding as Option<Vector>,
        record.metadata,
    )
    .execute(&self.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(MemoryStoreError::NotFound(record.id.to_string()));
    }
    Ok(())
}
```

#### `delete`

```rust
async fn delete(&self, id: Uuid) -> MemoryStoreResult<()>
```

Single `DELETE` by id. `scheduling`, `edges`, and `review_events` all
have `ON DELETE CASCADE` on their `memory_id` foreign key, so this one
statement clears every dependent row.

```rust
async fn delete(&self, id: Uuid) -> MemoryStoreResult<()> {
    let rows = sqlx::query!(
        "DELETE FROM knowledge_nodes WHERE id = $1",
        id,
    )
    .execute(&self.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(MemoryStoreError::NotFound(id.to_string()));
    }
    Ok(())
}
```

### Search (single-branch variants)

The full hybrid `search` is implemented in `0002e-hybrid-search.md`.
The two single-branch variants below ship in this sub-plan.

#### `fts_search`

```rust
async fn fts_search(&self, text: &str, limit: usize) -> MemoryStoreResult<Vec<SearchResult>>
```

PostgreSQL full-text search using the precomputed `search_vec` tsvector
column and `websearch_to_tsquery` (handles bare words, phrases, and
boolean operators). Ranking with `ts_rank_cd` (cover-density) so longer
matches outrank shorter ones; the SQLite backend uses BM25 from FTS5 but
the trait contract only requires "higher is better".

```rust
async fn fts_search(
    &self,
    text: &str,
    limit: usize,
) -> MemoryStoreResult<Vec<SearchResult>> {
    let limit = limit.min(1000) as i64;
    let rows = sqlx::query!(
        r#"
        SELECT
            m.id            AS "id!: Uuid",
            m.owner_user_id AS "owner_user_id!: Uuid",
            m.visibility,
            m.shared_with_groups AS "shared_with_groups!: Vec<Uuid>",
            m.codebase,
            m.content,
            m.node_type,
            m.tags          AS "tags!: Vec<String>",
            m.domains       AS "domains!: Vec<String>",
            m.domain_scores AS "domain_scores!: serde_json::Value",
            m.embedding     AS "embedding: Vector",
            m.metadata      AS "metadata!: serde_json::Value",
            m.created_at    AS "created_at!: DateTime<Utc>",
            m.updated_at    AS "updated_at!: DateTime<Utc>",
            ts_rank_cd(m.search_vec, websearch_to_tsquery('english', $1))
                AS "score!: f64"
        FROM knowledge_nodes m
        WHERE m.search_vec @@ websearch_to_tsquery('english', $1)
        ORDER BY score DESC
        LIMIT $2
        "#,
        text,
        limit,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let rec = row_to_record(
            r.id, r.content, r.node_type, r.tags, r.domains,
            r.domain_scores, r.codebase, r.owner_user_id, r.visibility,
            r.shared_with_groups, r.embedding, r.metadata,
            r.created_at, r.updated_at,
        )?;
        out.push(SearchResult {
            record: rec,
            score: r.score,
            fts_score: Some(r.score),
            vector_score: None,
        });
    }
    Ok(out)
}
```

The `'english'` text-search configuration matches the GIN index built in
`0001_init.up.sql`. If a future migration parameterises the config, both
the index and this query change together.

#### `vector_search`

```rust
async fn vector_search(&self, embedding: &[f32], limit: usize) -> MemoryStoreResult<Vec<SearchResult>>
```

pgvector cosine distance. The HNSW index on `embedding` (built in
`0002_hnsw.up.sql` with `vector_cosine_ops`) makes the `<=>` operator
index-accelerated. We convert the returned distance (0 = identical, 2 =
opposite for cosine on normalized vectors) to a similarity in `[0, 1]`
via `1 - distance`; this matches the SQLite backend's convention.

```rust
async fn vector_search(
    &self,
    embedding: &[f32],
    limit: usize,
) -> MemoryStoreResult<Vec<SearchResult>> {
    let query_vec = Vector::from(embedding.to_vec());
    let limit = limit.min(1000) as i64;

    let rows = sqlx::query!(
        r#"
        SELECT
            m.id            AS "id!: Uuid",
            m.owner_user_id AS "owner_user_id!: Uuid",
            m.visibility,
            m.shared_with_groups AS "shared_with_groups!: Vec<Uuid>",
            m.codebase,
            m.content,
            m.node_type,
            m.tags          AS "tags!: Vec<String>",
            m.domains       AS "domains!: Vec<String>",
            m.domain_scores AS "domain_scores!: serde_json::Value",
            m.embedding     AS "embedding: Vector",
            m.metadata      AS "metadata!: serde_json::Value",
            m.created_at    AS "created_at!: DateTime<Utc>",
            m.updated_at    AS "updated_at!: DateTime<Utc>",
            (1.0 - (m.embedding <=> $1)) AS "score!: f64"
        FROM knowledge_nodes m
        WHERE m.embedding IS NOT NULL
        ORDER BY m.embedding <=> $1
        LIMIT $2
        "#,
        query_vec as Vector,
        limit,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let rec = row_to_record(
            r.id, r.content, r.node_type, r.tags, r.domains,
            r.domain_scores, r.codebase, r.owner_user_id, r.visibility,
            r.shared_with_groups, r.embedding, r.metadata,
            r.created_at, r.updated_at,
        )?;
        out.push(SearchResult {
            record: rec,
            score: r.score,
            fts_score: None,
            vector_score: Some(r.score),
        });
    }
    Ok(out)
}
```

The `query_vec as Vector` cast is the same type-annotation trick as
`insert` -- sqlx needs the concrete pgvector type to wire up encoding.
The `ORDER BY m.embedding <=> $1` (no `score`) is intentional: it lets
the HNSW index serve the query directly. Sorting by the computed
`score` column would force a sequential scan because the index orders
by distance, not similarity.

### Scheduling

The Postgres `scheduling` table is a separate row keyed on `memory_id`,
not embedded in `knowledge_nodes` (unlike SQLite where FSRS columns live on
`knowledge_nodes`). The bodies abstract that difference at the SQL
boundary; callers see the same `SchedulingState` value.

#### `get_scheduling`

```rust
async fn get_scheduling(&self, memory_id: Uuid) -> MemoryStoreResult<Option<SchedulingState>>
```

```rust
async fn get_scheduling(
    &self,
    memory_id: Uuid,
) -> MemoryStoreResult<Option<SchedulingState>> {
    let row = sqlx::query!(
        r#"
        SELECT
            memory_id       AS "memory_id!: Uuid",
            stability       AS "stability!: f64",
            difficulty      AS "difficulty!: f64",
            retrievability  AS "retrievability!: f64",
            last_review     AS "last_review: DateTime<Utc>",
            next_review     AS "next_review: DateTime<Utc>",
            reps            AS "reps!: i32",
            lapses          AS "lapses!: i32"
        FROM scheduling
        WHERE memory_id = $1
        "#,
        memory_id,
    )
    .fetch_optional(&self.pool)
    .await?;

    Ok(row.map(|r| SchedulingState {
        memory_id: r.memory_id,
        stability: r.stability,
        difficulty: r.difficulty,
        retrievability: r.retrievability,
        last_review: r.last_review,
        next_review: r.next_review,
        reps: r.reps as u32,
        lapses: r.lapses as u32,
    }))
}
```

#### `update_scheduling`

```rust
async fn update_scheduling(&self, state: &SchedulingState) -> MemoryStoreResult<()>
```

Upsert -- the `INSERT ... ON CONFLICT DO UPDATE` form -- so cognitive
modules that update scheduling for a freshly-inserted memory don't have
to race with the seed row from `insert`.

```rust
async fn update_scheduling(
    &self,
    state: &SchedulingState,
) -> MemoryStoreResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO scheduling (
            memory_id, stability, difficulty, retrievability,
            last_review, next_review, reps, lapses
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (memory_id) DO UPDATE SET
            stability      = EXCLUDED.stability,
            difficulty     = EXCLUDED.difficulty,
            retrievability = EXCLUDED.retrievability,
            last_review    = EXCLUDED.last_review,
            next_review    = EXCLUDED.next_review,
            reps           = EXCLUDED.reps,
            lapses         = EXCLUDED.lapses
        "#,
        state.memory_id,
        state.stability,
        state.difficulty,
        state.retrievability,
        state.last_review,
        state.next_review,
        state.reps as i32,
        state.lapses as i32,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

#### `get_due_memories`

```rust
async fn get_due_memories(
    &self,
    before: DateTime<Utc>,
    limit: usize,
) -> MemoryStoreResult<Vec<(MemoryRecord, SchedulingState)>>
```

Join `knowledge_nodes` and `scheduling`, filter on `next_review <= before`.
Single query returns both halves of the tuple.

```rust
async fn get_due_memories(
    &self,
    before: DateTime<Utc>,
    limit: usize,
) -> MemoryStoreResult<Vec<(MemoryRecord, SchedulingState)>> {
    let limit = limit.min(10_000) as i64;
    let rows = sqlx::query!(
        r#"
        SELECT
            m.id            AS "id!: Uuid",
            m.owner_user_id AS "owner_user_id!: Uuid",
            m.visibility,
            m.shared_with_groups AS "shared_with_groups!: Vec<Uuid>",
            m.codebase,
            m.content,
            m.node_type,
            m.tags          AS "tags!: Vec<String>",
            m.domains       AS "domains!: Vec<String>",
            m.domain_scores AS "domain_scores!: serde_json::Value",
            m.embedding     AS "embedding: Vector",
            m.metadata      AS "metadata!: serde_json::Value",
            m.created_at    AS "created_at!: DateTime<Utc>",
            m.updated_at    AS "updated_at!: DateTime<Utc>",
            s.stability     AS "stability!: f64",
            s.difficulty    AS "difficulty!: f64",
            s.retrievability AS "retrievability!: f64",
            s.last_review   AS "last_review: DateTime<Utc>",
            s.next_review   AS "next_review: DateTime<Utc>",
            s.reps          AS "reps!: i32",
            s.lapses        AS "lapses!: i32"
        FROM knowledge_nodes m
        JOIN scheduling s ON s.memory_id = m.id
        WHERE s.next_review IS NOT NULL AND s.next_review <= $1
        ORDER BY s.next_review ASC
        LIMIT $2
        "#,
        before,
        limit,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let rec = row_to_record(
            r.id, r.content, r.node_type, r.tags, r.domains,
            r.domain_scores, r.codebase, r.owner_user_id, r.visibility,
            r.shared_with_groups, r.embedding, r.metadata,
            r.created_at, r.updated_at,
        )?;
        let state = SchedulingState {
            memory_id: rec.id,
            stability: r.stability,
            difficulty: r.difficulty,
            retrievability: r.retrievability,
            last_review: r.last_review,
            next_review: r.next_review,
            reps: r.reps as u32,
            lapses: r.lapses as u32,
        };
        out.push((rec, state));
    }
    Ok(out)
}
```

### Graph (edges)

#### `add_edge`

```rust
async fn add_edge(&self, edge: &MemoryEdge) -> MemoryStoreResult<()>
```

`INSERT ... ON CONFLICT` -- updating the weight if an edge already
exists (matches SQLite's `save_connection` semantics).

```rust
async fn add_edge(&self, edge: &MemoryEdge) -> MemoryStoreResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO edges (
            source_id, target_id, edge_type, weight, created_at
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (source_id, target_id, edge_type) DO UPDATE SET
            weight = EXCLUDED.weight
        "#,
        edge.source_id,
        edge.target_id,
        edge.edge_type,
        edge.weight,
        edge.created_at,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

#### `get_edges`

```rust
async fn get_edges(
    &self,
    node_id: Uuid,
    edge_type: Option<&str>,
) -> MemoryStoreResult<Vec<MemoryEdge>>
```

Return every edge incident to `node_id` in either direction, optionally
filtered by `edge_type`. The optional filter binds as nullable; `$2 IS
NULL OR edge_type = $2` keeps the prepared statement reusable.

```rust
async fn get_edges(
    &self,
    node_id: Uuid,
    edge_type: Option<&str>,
) -> MemoryStoreResult<Vec<MemoryEdge>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            source_id  AS "source_id!: Uuid",
            target_id  AS "target_id!: Uuid",
            edge_type,
            weight     AS "weight!: f64",
            created_at AS "created_at!: DateTime<Utc>"
        FROM edges
        WHERE (source_id = $1 OR target_id = $1)
          AND ($2::text IS NULL OR edge_type = $2)
        "#,
        node_id,
        edge_type,
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| MemoryEdge {
            source_id: r.source_id,
            target_id: r.target_id,
            edge_type: r.edge_type,
            weight: r.weight,
            created_at: r.created_at,
        })
        .collect())
}
```

#### `remove_edge`

```rust
async fn remove_edge(&self, source: Uuid, target: Uuid) -> MemoryStoreResult<()>
```

Note: the live trait signature is two args (`source`, `target`). The
master plan's stale three-arg signature including `edge_type` is not
implemented -- the live trait surface wins. Deletes every edge between
the pair regardless of `edge_type`.

```rust
async fn remove_edge(
    &self,
    source: Uuid,
    target: Uuid,
) -> MemoryStoreResult<()> {
    sqlx::query!(
        "DELETE FROM edges WHERE source_id = $1 AND target_id = $2",
        source,
        target,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

#### `get_neighbors`

```rust
async fn get_neighbors(
    &self,
    node_id: Uuid,
    depth: usize,
) -> MemoryStoreResult<Vec<(MemoryRecord, f64)>>
```

Recursive CTE walks the edge graph outward from `node_id` for up to
`depth` hops. Weights compound multiplicatively along the path (same as
SQLite BFS). Cap the visited set at 256 rows to match SQLite. Direction
is treated as undirected by unioning both halves of each edge inside
the CTE.

```rust
async fn get_neighbors(
    &self,
    node_id: Uuid,
    depth: usize,
) -> MemoryStoreResult<Vec<(MemoryRecord, f64)>> {
    if depth == 0 {
        let Some(rec) = self.get(node_id).await? else {
            return Err(MemoryStoreError::NotFound(node_id.to_string()));
        };
        return Ok(vec![(rec, 1.0)]);
    }

    let depth_i = depth.min(16) as i32;
    let rows = sqlx::query!(
        r#"
        WITH RECURSIVE walk(node_id, weight, hops) AS (
            SELECT $1::uuid, 1.0::float8, 0
          UNION ALL
            SELECT
                CASE WHEN e.source_id = w.node_id THEN e.target_id
                     ELSE e.source_id END,
                w.weight * e.weight,
                w.hops + 1
            FROM walk w
            JOIN edges e
              ON e.source_id = w.node_id OR e.target_id = w.node_id
            WHERE w.hops < $2
        ),
        best AS (
            SELECT node_id, MAX(weight) AS weight
            FROM walk
            GROUP BY node_id
            LIMIT 256
        )
        SELECT
            m.id            AS "id!: Uuid",
            m.owner_user_id AS "owner_user_id!: Uuid",
            m.visibility,
            m.shared_with_groups AS "shared_with_groups!: Vec<Uuid>",
            m.codebase,
            m.content,
            m.node_type,
            m.tags          AS "tags!: Vec<String>",
            m.domains       AS "domains!: Vec<String>",
            m.domain_scores AS "domain_scores!: serde_json::Value",
            m.embedding     AS "embedding: Vector",
            m.metadata      AS "metadata!: serde_json::Value",
            m.created_at    AS "created_at!: DateTime<Utc>",
            m.updated_at    AS "updated_at!: DateTime<Utc>",
            b.weight        AS "weight!: f64"
        FROM best b
        JOIN knowledge_nodes m ON m.id = b.node_id
        "#,
        node_id,
        depth_i,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let rec = row_to_record(
            r.id, r.content, r.node_type, r.tags, r.domains,
            r.domain_scores, r.codebase, r.owner_user_id, r.visibility,
            r.shared_with_groups, r.embedding, r.metadata,
            r.created_at, r.updated_at,
        )?;
        out.push((rec, r.weight));
    }
    Ok(out)
}
```

The CTE can visit a node multiple times via different paths; the `best`
sub-CTE picks the highest weight per node. The `LIMIT 256` matches the
SQLite BFS cap. Postgres' recursive CTE is breadth-first by hop count
because of the `WHERE w.hops < $2` predicate.

### Domains (Phase 4 populates; Phase 2 ships CRUD)

The `domains` table is empty in Phase 2; these methods exist so the
trait surface is complete but they do not get exercised until Phase 4
HDBSCAN clustering runs.

#### `list_domains`

```rust
async fn list_domains(&self) -> MemoryStoreResult<Vec<Domain>>
```

```rust
async fn list_domains(&self) -> MemoryStoreResult<Vec<Domain>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            label,
            centroid    AS "centroid: Vector",
            top_terms   AS "top_terms!: Vec<String>",
            memory_count AS "memory_count!: i64",
            created_at  AS "created_at!: DateTime<Utc>"
        FROM domains
        ORDER BY created_at ASC
        "#
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Domain {
            id: r.id,
            label: r.label,
            centroid: r.centroid.map(|v| v.to_vec()).unwrap_or_default(),
            top_terms: r.top_terms,
            memory_count: r.memory_count as usize,
            created_at: r.created_at,
        })
        .collect())
}
```

#### `get_domain`

```rust
async fn get_domain(&self, id: &str) -> MemoryStoreResult<Option<Domain>>
```

```rust
async fn get_domain(&self, id: &str) -> MemoryStoreResult<Option<Domain>> {
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            label,
            centroid    AS "centroid: Vector",
            top_terms   AS "top_terms!: Vec<String>",
            memory_count AS "memory_count!: i64",
            created_at  AS "created_at!: DateTime<Utc>"
        FROM domains
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&self.pool)
    .await?;

    Ok(row.map(|r| Domain {
        id: r.id,
        label: r.label,
        centroid: r.centroid.map(|v| v.to_vec()).unwrap_or_default(),
        top_terms: r.top_terms,
        memory_count: r.memory_count as usize,
        created_at: r.created_at,
    }))
}
```

#### `upsert_domain`

```rust
async fn upsert_domain(&self, domain: &Domain) -> MemoryStoreResult<()>
```

```rust
async fn upsert_domain(&self, domain: &Domain) -> MemoryStoreResult<()> {
    let centroid = if domain.centroid.is_empty() {
        None
    } else {
        Some(Vector::from(domain.centroid.clone()))
    };

    sqlx::query!(
        r#"
        INSERT INTO domains (
            id, label, centroid, top_terms, memory_count, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            label        = EXCLUDED.label,
            centroid     = EXCLUDED.centroid,
            top_terms    = EXCLUDED.top_terms,
            memory_count = EXCLUDED.memory_count
        "#,
        domain.id,
        domain.label,
        centroid as Option<Vector>,
        &domain.top_terms as &[String],
        domain.memory_count as i64,
        domain.created_at,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

#### `delete_domain`

```rust
async fn delete_domain(&self, id: &str) -> MemoryStoreResult<()>
```

```rust
async fn delete_domain(&self, id: &str) -> MemoryStoreResult<()> {
    sqlx::query!(
        "DELETE FROM domains WHERE id = $1",
        id,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

#### `classify`

```rust
async fn classify(&self, embedding: &[f32]) -> MemoryStoreResult<Vec<(String, f64)>>
```

The Postgres backend can ship this as a single SQL query against the
empty `domains` table -- it correctly returns an empty vector in Phase
2 and starts returning real scores in Phase 4 without any code change.

```rust
async fn classify(
    &self,
    embedding: &[f32],
) -> MemoryStoreResult<Vec<(String, f64)>> {
    let query_vec = Vector::from(embedding.to_vec());
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            (1.0 - (centroid <=> $1)) AS "score!: f64"
        FROM domains
        WHERE centroid IS NOT NULL
        ORDER BY score DESC
        "#,
        query_vec as Vector,
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows.into_iter().map(|r| (r.id, r.score)).collect())
}
```

### Bulk / maintenance

#### `count`

```rust
async fn count(&self) -> MemoryStoreResult<usize>
```

```rust
async fn count(&self) -> MemoryStoreResult<usize> {
    let n: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM knowledge_nodes")
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);
    Ok(n as usize)
}
```

#### `get_stats`

```rust
async fn get_stats(&self) -> MemoryStoreResult<StoreStats>
```

Aggregate counts across `knowledge_nodes`, `edges`, `domains`. Read the
registry inline.

```rust
async fn get_stats(&self) -> MemoryStoreResult<StoreStats> {
    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT COUNT(*) FROM knowledge_nodes)
                AS "total_memories!: i64",
            (SELECT COUNT(*) FROM knowledge_nodes WHERE embedding IS NOT NULL)
                AS "memories_with_embeddings!: i64",
            (SELECT COUNT(*) FROM edges)
                AS "total_edges!: i64",
            (SELECT COUNT(*) FROM domains)
                AS "total_domains!: i64",
            (SELECT name FROM embedding_model WHERE id = 1)
                AS "registered_model_name: String",
            (SELECT dimension FROM embedding_model WHERE id = 1)
                AS "registered_model_dim: i32"
        "#
    )
    .fetch_one(&self.pool)
    .await?;

    Ok(StoreStats {
        total_memories: row.total_memories as usize,
        memories_with_embeddings: row.memories_with_embeddings as usize,
        total_edges: row.total_edges as usize,
        total_domains: row.total_domains as usize,
        registered_model_name: row.registered_model_name,
        registered_model_dim: row.registered_model_dim.map(|d| d as usize),
    })
}
```

#### `vacuum`

```rust
async fn vacuum(&self) -> MemoryStoreResult<()>
```

`VACUUM` cannot run inside a transaction. sqlx wraps each `query!`
invocation in an implicit transaction when it grabs a pooled
connection, but it does not -- the pool hands out a raw connection
that runs statements in autocommit mode by default. The safe path is
to acquire a connection explicitly and `execute` each statement
separately so neither participates in a transaction.

```rust
async fn vacuum(&self) -> MemoryStoreResult<()> {
    let mut conn = self.pool.acquire().await?;
    sqlx::query("VACUUM ANALYZE knowledge_nodes")
        .execute(conn.as_mut())
        .await?;
    sqlx::query("VACUUM ANALYZE scheduling")
        .execute(conn.as_mut())
        .await?;
    sqlx::query("VACUUM ANALYZE edges")
        .execute(conn.as_mut())
        .await?;
    Ok(())
}
```

`conn.as_mut()` yields a `&mut PgConnection`, which sqlx accepts as an
executor. Using `&self.pool` here would let sqlx pick a fresh
connection per statement (still fine, but two extra acquisitions). Note
we do NOT vacuum `domains`, `edges`-related lookup tables (`users` /
`groups` etc.) -- they are either empty in Phase 2 or low-churn and the
nightly autovacuum suffices.

---

## Visibility filter posture

ADR 0002 D7 declares the future Phase 3 visibility filter (reproduced
here for clarity):

```sql
WHERE
       (visibility = 'private' AND owner_user_id = $me)
    OR (visibility = 'group'
        AND (owner_user_id = $me OR shared_with_groups && $my_group_ids))
    OR  visibility = 'public'
```

**Phase 2 does NOT apply this filter.** Every body above reads and
writes the rows it touches regardless of `owner_user_id` or
`visibility` because there is exactly one user in Phase 2 mode (the
bootstrap user from `0001_init.up.sql`). The reviewer should NOT expect
`WHERE owner_user_id = $...` clauses in Phase 2 method bodies.

Phase 3 introduces an `AuthContext` parameter on the trait methods and
threads it into each WHERE clause. That migration is purely additive
(adds a parameter, adds a clause); it does not need a schema migration
because the columns and indexes are already in place.

The four new `MemoryRecord` fields ARE populated correctly in Phase 2
(insert writes them, get/search read them) so that exported archives
and replicated rows round-trip the visibility intent the moment Phase
3 enables filtering.

---

## Offline sqlx cache

`sqlx::query!` and `sqlx::query_as!` validate every SQL string at
compile time by contacting a live database. To keep CI builds from
needing a Postgres on the build host, sqlx supports an offline cache
in `<crate-root>/.sqlx/` containing one JSON file per validated query.

This sub-plan is where `.sqlx/` is first populated and committed.

Workflow:

1. Ensure a local Postgres is running with the same schema CI will see:

   ```bash
   cd crates/vestige-core
   export DATABASE_URL="postgres://vestige:vestige@127.0.0.1:5432/vestige_dev"
   sqlx database create
   sqlx migrate run --source migrations/postgres
   ```

2. Generate the offline cache:

   ```bash
   cargo sqlx prepare --workspace -- --features postgres-backend
   ```

   This walks every `sqlx::query!` invocation under the active feature
   flags and writes `crates/vestige-core/.sqlx/query-<hash>.json`. The
   `--workspace` flag is needed because `vestige-mcp` enables the
   `postgres-backend` feature transitively in `0002b-pool-and-config.md`.

3. Stage and commit the cache directory:

   ```bash
   git add crates/vestige-core/.sqlx/
   git commit -m "store: populate sqlx offline cache for postgres backend"
   ```

4. Add to repo `.gitignore` adjustments (only if entries already deny
   `target/` or similar globs): leave `.sqlx/` excluded from any
   ignore globs. Specifically the workspace root `.gitignore` does NOT
   contain a `.sqlx` line; if a future PR adds one, this sub-plan's
   commit reverts it.

5. CI runs `SQLX_OFFLINE=true cargo check --features postgres-backend`.
   sqlx falls back to the JSON cache when `SQLX_OFFLINE=true` is set,
   so CI does not need network access to a Postgres.

Every time a `query!` invocation changes -- add a column, change a
WHERE clause, rename a binding -- re-run `cargo sqlx prepare` and
commit the updated `.sqlx/` files. The agent implementing this sub-plan
runs `cargo sqlx prepare` as the last step before opening the PR.

---

## Verification

Three layers of verification before merging this sub-plan.

### 1. Compile and lint

```bash
cargo check  --workspace --features postgres-backend
cargo build  --workspace --features postgres-backend
cargo clippy --workspace --features postgres-backend -- -D warnings

# SQLite-only build still works (mutual compilability per CLAUDE.md):
cargo check --workspace --no-default-features --features embeddings,vector-search
```

### 2. Offline cache builds

```bash
SQLX_OFFLINE=true cargo check --workspace --features postgres-backend
```

This is what CI will run. If it fails, `cargo sqlx prepare` was not
re-run after the last query change.

### 3. Integration round-trip test (testcontainers)

New test file:
`crates/vestige-core/tests/postgres_round_trip.rs`. Skipped unless the
`postgres-backend` feature is active and Docker / Podman is available.

```rust
#![cfg(feature = "postgres-backend")]

use chrono::Utc;
use testcontainers::{clients, GenericImage};
use uuid::Uuid;
use vestige_core::storage::memory_store::{
    LocalMemoryStore, MemoryEdge, MemoryRecord, SchedulingState, Visibility,
    LOCAL_USER_ID,
};
use vestige_core::storage::postgres::PgMemoryStore;

#[tokio::test]
async fn round_trip_crud_search_scheduling_edges() {
    let docker = clients::Cli::default();
    let image = GenericImage::new("pgvector/pgvector", "pg18")
        .with_env_var("POSTGRES_PASSWORD", "test")
        .with_env_var("POSTGRES_DB", "vestige_test")
        .with_exposed_port(5432);
    let container = docker.run(image);
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:test@127.0.0.1:{port}/vestige_test");

    let store = PgMemoryStore::connect(&url, 5).await.expect("connect");

    // Register the model (typmod stamp).
    store.register_model(&fixture_signature(384)).await.expect("register");

    // insert -> get -> update -> delete.
    let mut rec = fixture_record();
    let id = store.insert(&rec).await.expect("insert");
    let fetched = store.get(id).await.expect("get").expect("present");
    assert_eq!(fetched.content, rec.content);
    assert_eq!(fetched.owner_user_id, LOCAL_USER_ID);
    assert_eq!(fetched.visibility, Visibility::Private);

    rec.content = "edited".to_string();
    store.update(&rec).await.expect("update");
    assert_eq!(store.get(id).await.unwrap().unwrap().content, "edited");

    // fts_search.
    let hits = store.fts_search("edited", 10).await.expect("fts");
    assert!(hits.iter().any(|h| h.record.id == id));

    // vector_search.
    let emb = rec.embedding.clone().unwrap();
    let vhits = store.vector_search(&emb, 10).await.expect("vector");
    assert!(vhits.iter().any(|h| h.record.id == id));

    // scheduling round-trip.
    let sched = store.get_scheduling(id).await.unwrap().expect("seeded");
    let new_state = SchedulingState {
        memory_id: id,
        stability: 5.5,
        difficulty: 0.2,
        retrievability: 0.95,
        last_review: Some(Utc::now()),
        next_review: Some(Utc::now() + chrono::Duration::days(3)),
        reps: sched.reps + 1,
        lapses: sched.lapses,
    };
    store.update_scheduling(&new_state).await.expect("update sched");
    let after = store.get_scheduling(id).await.unwrap().unwrap();
    assert_eq!(after.reps, new_state.reps);

    // edges.
    let other = fixture_record();
    let other_id = store.insert(&other).await.unwrap();
    let edge = MemoryEdge {
        source_id: id,
        target_id: other_id,
        edge_type: "semantic".to_string(),
        weight: 0.8,
        created_at: Utc::now(),
    };
    store.add_edge(&edge).await.expect("add_edge");
    let edges = store.get_edges(id, None).await.unwrap();
    assert_eq!(edges.len(), 1);
    let neighbors = store.get_neighbors(id, 1).await.unwrap();
    assert!(neighbors.iter().any(|(r, _)| r.id == other_id));
    store.remove_edge(id, other_id).await.expect("remove_edge");
    assert!(store.get_edges(id, None).await.unwrap().is_empty());

    // delete -> cascade.
    store.delete(id).await.expect("delete");
    assert!(store.get(id).await.unwrap().is_none());
    assert!(store.get_scheduling(id).await.unwrap().is_none());
}

fn fixture_record() -> MemoryRecord {
    MemoryRecord {
        id: Uuid::new_v4(),
        domains: vec![],
        domain_scores: Default::default(),
        content: "the quick brown fox jumps over the lazy dog".into(),
        node_type: "fact".into(),
        tags: vec!["test".into()],
        embedding: Some(vec![0.1_f32; 384]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        metadata: serde_json::json!({}),
        owner_user_id: LOCAL_USER_ID,
        visibility: Visibility::Private,
        shared_with_groups: vec![],
        codebase: Some("vestige".to_string()),
    }
}

fn fixture_signature(dim: usize) -> vestige_core::storage::memory_store::ModelSignature {
    vestige_core::storage::memory_store::ModelSignature {
        name: "test/model".to_string(),
        dimension: dim,
        hash: "0".repeat(64),
    }
}
```

Add `testcontainers = "0.20"` to `[dev-dependencies]` under
`#[cfg(feature = "postgres-backend")]` gating. The test is the slowest
in the suite (spawns a Docker container, ~5 s startup); annotate with
`#[ignore]` if CI runtime budget requires opt-in execution.

### 4. Manual smoke (optional but recommended)

```bash
# Tear down any prior database.
make postgres-down ; make postgres-up
DATABASE_URL=$(make postgres-url) cargo test \
    -p vestige-core --features postgres-backend -- --include-ignored
```

The `postgres-up` / `postgres-down` / `postgres-url` make targets are
defined in `docs/plans/local-dev-postgres-setup.md`.

---

## Acceptance criteria

This sub-plan is complete when ALL of the following hold:

1. `cargo build --workspace --features postgres-backend` succeeds with
   zero warnings.
2. `cargo clippy --workspace --features postgres-backend -- -D warnings`
   succeeds.
3. `cargo build --workspace --no-default-features --features embeddings,vector-search`
   still succeeds (the SQLite-only build is not regressed).
4. `SQLX_OFFLINE=true cargo check --workspace --features postgres-backend`
   succeeds. `crates/vestige-core/.sqlx/` exists and contains one JSON
   file per `sqlx::query!` / `sqlx::query_as!` invocation in the
   Postgres backend.
5. Zero `todo!()` macros remain in
   `crates/vestige-core/src/storage/postgres/mod.rs`. The only
   exception is the body of the trait method `search` -- that method
   stays `todo!()` until `0002e-hybrid-search.md` lands.
6. `crates/vestige-core/src/storage/postgres/registry.rs` exists with
   the three functions described above
   (`fetch_registry`, `ensure_registry`, `update_registry_for_reembed`).
7. `MemoryRecord` carries the four new fields
   (`owner_user_id`, `visibility`, `shared_with_groups`, `codebase`)
   and the `Visibility` enum is exported alongside it. The SQLite
   backend reads and writes the same four fields.
8. The `tests/postgres_round_trip.rs` integration test passes against
   a `pgvector/pgvector:pg18` container (insert / get / update / delete
   / fts_search / vector_search / get_scheduling / update_scheduling
   / add_edge / get_edges / remove_edge / get_neighbors / cascade
   delete).
9. No visibility filter clause is present in any Phase 2 method body.
   `WHERE owner_user_id = ...`, `WHERE visibility = ...`, and
   `shared_with_groups && ...` do not appear anywhere in
   `crates/vestige-core/src/storage/postgres/`.
10. `cargo sqlx prepare` was the last command run before commit; the
    diff includes `.sqlx/` changes if any query changed.
