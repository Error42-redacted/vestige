# Phase 2 Sub-Plan 0002e -- Hybrid RRF Search

**Status**: Ready
**Depends on**:
- `0002a-skeleton-and-feature-gate.md` (the `postgres-backend` feature flag
  exists and `PgMemoryStore` compiles with `todo!()` bodies).
- `0002b-pool-and-config.md` (a working `PgPool` reaches the backend).
- `0002c-migrations.md` (migration `0001_init` has created the `knowledge_nodes`
  table with the D7 columns -- `owner_user_id`, `visibility`,
  `shared_with_groups` -- and the D8 column `codebase`; migration `0002_hnsw`
  has built the HNSW index).
- `0002d-store-impl-bodies.md` (real CRUD bodies exist so the integration
  tests below can seed data through the trait surface rather than raw SQL).

This sub-plan covers master plan 0002 deliverable D5: the hybrid RRF search
query implementation in `crates/vestige-core/src/storage/postgres/search.rs`,
plus the `search`, `fts_search`, and `vector_search` method bodies in
`crates/vestige-core/src/storage/postgres/mod.rs` that delegate into it.

---

## Context

This is one of the more performance-sensitive sub-plans in Phase 2. Every
search call from the cognitive engine -- the 7-stage retrieval pipeline,
`session_context`, `predict`, `deep_reference`, the dashboard -- bottoms out
in `MemoryStore::search`. The Postgres backend has to keep up with the
existing SQLite hybrid path, which combines BM25 over FTS5 with USearch HNSW
in two separate round trips and fuses the rankings in Rust.

The shape of the win on Postgres is that both branches and the fusion run
inside one statement. The planner sees both CTEs together, the round trip is
single, and the rerank stage runs over a cleanly overfetched candidate set.

Latency targets live in `0002h-testing-and-benches.md`. This sub-plan is
responsible for producing a correct, schema-stable query that the benches
can drive against. Do not optimise here; correctness and structure first.

Master plan 0002 D5 (around lines 522-628 of
`docs/plans/0002-phase-2-postgres-backend.md`) sketches the SQL. That
sketch is the starting point, not the finished product. The schema after
the D7 and D8 amendments has more columns than the sketch enumerates, and
the SQLite `search` method (around line 6503 of
`crates/vestige-core/src/storage/sqlite.rs` in the Phase 1 worktree)
documents the semantics this implementation must stay compatible with:

- Empty `query.limit` defaults to 10.
- `query.text == Some("")` is treated as no text query (degrade to vector).
- `query.embedding == None` is treated as no vector query (degrade to FTS).
- Both empty returns `Ok(vec![])`; not an error.
- The `MemoryRecord` in each `SearchResult` must be populated with all
  fields the trait promises, including `domains` and `domain_scores` (Phase
  4 will fill these in; Phase 2 returns the stored values, which may be
  empty arrays / empty objects).

---

## Constants

```rust
/// Reciprocal Rank Fusion smoothing constant from Cormack, Clarke and Buettcher
/// 2009 ("Reciprocal Rank Fusion outperforms Condorcet and individual rank
/// learning methods"). 60 is the canonical default and is robust across most
/// fusion regimes. Do not tune this without a paper-citation-grade reason.
const RRF_K: i32 = 60;

/// Each branch (FTS, vector) is allowed to return OVERFETCH_MULT x final_limit
/// rows before fusion. Three matches the Phase 1 SQLite overfetch and gives
/// the fusion enough candidates to recover from any single branch's bad
/// recall on a given query.
const OVERFETCH_MULT: i64 = 3;
```

These live at module scope in
`crates/vestige-core/src/storage/postgres/search.rs`. They are `pub(crate)`
only if `0002h-testing-and-benches.md` needs to reference them from the
integration tests; otherwise private.

---

## Public API

```rust
#![cfg(feature = "postgres-backend")]

use pgvector::Vector;
use sqlx::PgPool;

use crate::storage::memory_store::{
    MemoryStoreResult, SearchQuery, SearchResult,
};

/// Hybrid RRF search over Postgres FTS and pgvector cosine distance.
///
/// Branch behavior:
/// - empty text + null embedding   -> Ok(vec![])
/// - empty text + Some(embedding)  -> pure vector search (FTS CTE returns
///                                    zero rows; fusion equals the vector
///                                    branch)
/// - Some(text) + null embedding   -> pure FTS search
/// - Some(text) + Some(embedding)  -> full RRF fusion
///
/// `query.limit == 0` is treated as 10 (matches the SQLite default).
pub(crate) async fn rrf_search(
    pool: &PgPool,
    query: &SearchQuery,
) -> MemoryStoreResult<Vec<SearchResult>>;

/// FTS-only convenience search. Equivalent to calling `rrf_search` with
/// `query.embedding = None`, but uses a dedicated single-branch query that
/// avoids the FULL OUTER JOIN and the params CTE; faster by one planner pass
/// per call.
pub(crate) async fn fts_only(
    pool: &PgPool,
    text: &str,
    limit: usize,
) -> MemoryStoreResult<Vec<SearchResult>>;

/// Vector-only convenience search. Dedicated single-branch query for the same
/// latency reason as `fts_only`.
pub(crate) async fn vector_only(
    pool: &PgPool,
    embedding: &[f32],
    limit: usize,
) -> MemoryStoreResult<Vec<SearchResult>>;
```

### Parameter handling

In `rrf_search`:

```rust
let final_limit: i32 = if query.limit == 0 { 10 } else { query.limit as i32 };
let overfetch: i32 = (final_limit as i64 * OVERFETCH_MULT)
    .min(i32::MAX as i64) as i32;

let q_text: &str = query.text.as_deref().unwrap_or("");
let q_vec: Option<Vector> = query.embedding.as_ref()
    .map(|v| Vector::from(v.clone()));

let dom_filter: Option<&[String]> = query.domains.as_deref();
let nt_filter:  Option<&[String]> = query.node_types.as_deref();
let tag_filter: Option<&[String]> = query.tags.as_deref();

let min_retr: Option<f64> = query.min_retrievability;
```

Both branches empty -- `q_text` is empty and `q_vec` is `None` -- returns
`Ok(vec![])` without hitting the database. The SQLite backend has the same
behavior and tests rely on it.

```rust
if q_text.is_empty() && q_vec.is_none() {
    return Ok(Vec::new());
}
```

### `search` method body in `postgres/mod.rs`

```rust
#[async_trait::async_trait]   // or trait_variant after the Phase 1 amendment
impl MemoryStore for PgMemoryStore {
    async fn search(&self, query: &SearchQuery)
        -> MemoryStoreResult<Vec<SearchResult>>
    {
        crate::storage::postgres::search::rrf_search(&self.pool, query).await
    }

    async fn fts_search(&self, text: &str, limit: usize)
        -> MemoryStoreResult<Vec<SearchResult>>
    {
        crate::storage::postgres::search::fts_only(&self.pool, text, limit)
            .await
    }

    async fn vector_search(&self, embedding: &[f32], limit: usize)
        -> MemoryStoreResult<Vec<SearchResult>>
    {
        crate::storage::postgres::search::vector_only(&self.pool, embedding, limit)
            .await
    }
}
```

Everything below specifies the inside of those three free functions.

---

## SQL: the hybrid RRF query

The query is built as one `&'static str` (or `OnceCell<String>`; see "Use
of sqlx::query!" below) and reused. Bound parameters are kept to seven
through a `params` CTE that the rest of the query references by name --
this keeps the SQL readable and stops the bound-parameter count growing
with each filter clause.

Bound parameters:

- `$1`: text query (TEXT, may be empty)
- `$2`: embedding (pgvector::Vector, may be NULL)
- `$3`: overfetch limit per branch (INT)
- `$4`: final limit (INT)
- `$5`: domain filter (TEXT[] or NULL)
- `$6`: node_type filter (TEXT[] or NULL)
- `$7`: tag filter (TEXT[] or NULL)

If `min_retrievability.is_some()` the outer SELECT adds a JOIN on
`scheduling` and a WHERE clause; that path uses a different prepared
statement (see "min_retrievability filter" below) so the simple-path query
stays free of the join.

```sql
WITH params AS (
    SELECT
        $1::text    AS q_text,
        $2::vector  AS q_vec,
        $3::int     AS overfetch,
        $4::int     AS final_limit,
        $5::text[]  AS dom_filter,
        $6::text[]  AS nt_filter,
        $7::text[]  AS tag_filter
),
fts AS (
    SELECT
        m.id,
        ts_rank_cd(
            m.search_vec,
            websearch_to_tsquery('english', p.q_text)
        ) AS score,
        ROW_NUMBER() OVER (
            ORDER BY ts_rank_cd(
                m.search_vec,
                websearch_to_tsquery('english', p.q_text)
            ) DESC
        ) AS rank
    FROM knowledge_nodes m
    CROSS JOIN params p
    WHERE p.q_text <> ''
      AND m.search_vec @@ websearch_to_tsquery('english', p.q_text)
      AND (p.dom_filter IS NULL OR m.domains   && p.dom_filter)
      AND (p.nt_filter  IS NULL OR m.node_type =  ANY(p.nt_filter))
      AND (p.tag_filter IS NULL OR m.tags      && p.tag_filter)
    ORDER BY score DESC
    LIMIT (SELECT overfetch FROM params)
),
vec AS (
    SELECT
        m.id,
        1.0 - (m.embedding <=> p.q_vec) AS score,
        ROW_NUMBER() OVER (
            ORDER BY m.embedding <=> p.q_vec
        ) AS rank
    FROM knowledge_nodes m
    CROSS JOIN params p
    WHERE m.embedding IS NOT NULL
      AND p.q_vec IS NOT NULL
      AND (p.dom_filter IS NULL OR m.domains   && p.dom_filter)
      AND (p.nt_filter  IS NULL OR m.node_type =  ANY(p.nt_filter))
      AND (p.tag_filter IS NULL OR m.tags      && p.tag_filter)
    ORDER BY m.embedding <=> p.q_vec
    LIMIT (SELECT overfetch FROM params)
),
fused AS (
    SELECT
        COALESCE(f.id, v.id) AS id,
          COALESCE(1.0 / (60 + f.rank), 0.0)
        + COALESCE(1.0 / (60 + v.rank), 0.0) AS rrf_score,
        f.score AS fts_score,
        v.score AS vector_score
    FROM fts f
    FULL OUTER JOIN vec v ON f.id = v.id
)
SELECT
    m.id                  AS "id!: uuid::Uuid",
    m.owner_user_id       AS "owner_user_id!: uuid::Uuid",
    m.visibility          AS "visibility!: String",
    m.shared_with_groups  AS "shared_with_groups!: Vec<uuid::Uuid>",
    m.codebase            AS "codebase: String",
    m.domains             AS "domains!: Vec<String>",
    m.domain_scores       AS "domain_scores!: serde_json::Value",
    m.content             AS "content!: String",
    m.node_type           AS "node_type!: String",
    m.tags                AS "tags!: Vec<String>",
    m.embedding           AS "embedding: pgvector::Vector",
    m.metadata            AS "metadata!: serde_json::Value",
    m.created_at          AS "created_at!: chrono::DateTime<chrono::Utc>",
    m.updated_at          AS "updated_at!: chrono::DateTime<chrono::Utc>",
    fused.rrf_score       AS "rrf_score!: f64",
    fused.fts_score       AS "fts_score: f64",
    fused.vector_score    AS "vector_score: f64"
FROM fused
JOIN knowledge_nodes m ON m.id = fused.id
ORDER BY fused.rrf_score DESC
LIMIT (SELECT final_limit FROM params);
```

Notes on the SELECT column list. The D7 columns (`owner_user_id`,
`visibility`, `shared_with_groups`) and the D8 column (`codebase`) are
selected even though Phase 2 does not filter on them yet, so:

1. The `MemoryRecord` returned to the trait can be populated with the
   stored values from day one. Phase 3 will start writing real
   `owner_user_id` / `visibility` values; Phase 2 always writes the
   single-user defaults (`'00000000-...-0001'`, `'private'`, `'{}'`). The
   `MemoryRecord` returned in Phase 2 simply carries those defaults.
2. The schema-drift integration tests (see "Verification") catch the case
   where someone adds a NOT NULL column to `knowledge_nodes` without updating
   this query.

Notes on the body:

- `CROSS JOIN params p` is used instead of the master-plan sketch's
  `FROM knowledge_nodes m, params p`. Same plan, clearer intent.
- The `ORDER BY ... LIMIT` inside each branch CTE is there so the planner
  can stop early once it has `overfetch` rows; without it the LIMIT is
  applied after a full sort over all matches.
- `1.0 - (m.embedding <=> p.q_vec)` converts pgvector's cosine *distance*
  to cosine *similarity* in [0, 1] for the `vector_score` output. RRF
  itself does not need the similarity -- it uses ranks -- but the trait
  surface exposes `vector_score: Option<f64>` for caller introspection.
- `RRF_K = 60` is inlined as `60` in the SQL string. A `const` formatter
  feels tidier but `60` is a literature constant; spell it out and leave a
  comment in the Rust source: `// 60 == RRF_K (Cormack 2009)`.
- `FULL OUTER JOIN` is required: a row that the FTS branch finds and the
  vector branch does not must still appear in `fused`, and vice versa.
- `COALESCE(..., 0.0)` on each `1.0 / (60 + rank)` term handles the
  no-match-from-this-branch case. The fusion score for a row that only the
  FTS branch ranks is `1/(60 + f.rank)` exactly.
- `m.search_vec` is the generated `tsvector` column created in migration
  `0001_init` (see D4 of the master plan).

---

## Result row mapping

`sqlx::query_as::<_, SearchRow>` reads each row into a private struct that
owns the column types exactly as they come back from Postgres. The struct
is converted into a `SearchResult` after fetch.

```rust
#[derive(sqlx::FromRow)]
struct SearchRow {
    id: uuid::Uuid,
    owner_user_id: uuid::Uuid,
    visibility: String,
    shared_with_groups: Vec<uuid::Uuid>,
    codebase: Option<String>,
    domains: Vec<String>,
    domain_scores: serde_json::Value,
    content: String,
    node_type: String,
    tags: Vec<String>,
    embedding: Option<pgvector::Vector>,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    rrf_score: f64,
    fts_score: Option<f64>,
    vector_score: Option<f64>,
}

impl SearchRow {
    fn into_result(self) -> SearchResult {
        use crate::storage::memory_store::MemoryRecord;
        use std::collections::HashMap;

        // domain_scores is JSONB; the column always exists, but may be the
        // empty object {} when Phase 4 has not classified this memory yet.
        let domain_scores: HashMap<String, f64> =
            serde_json::from_value(self.domain_scores).unwrap_or_default();

        let record = MemoryRecord {
            id: self.id,
            domains: self.domains,
            domain_scores,
            content: self.content,
            node_type: self.node_type,
            tags: self.tags,
            // pgvector::Vector -> Vec<f32>
            embedding: self.embedding.map(|v| v.to_vec()),
            created_at: self.created_at,
            updated_at: self.updated_at,
            metadata: self.metadata,
            // owner_user_id / visibility / shared_with_groups / codebase
            // do not appear on MemoryRecord yet. Phase 3 will decide whether
            // to extend MemoryRecord or surface them via a side channel.
            // For Phase 2 they are read but discarded here.
        };

        SearchResult {
            record,
            score: self.rrf_score,
            fts_score: self.fts_score,
            vector_score: self.vector_score,
        }
    }
}
```

Type mapping summary:

| SQL type          | Rust type                            | Notes                                          |
|-------------------|--------------------------------------|------------------------------------------------|
| UUID              | `uuid::Uuid`                         | requires sqlx `uuid` feature                   |
| TEXT              | `String`                             |                                                |
| TEXT NULL         | `Option<String>`                     | used for `codebase`                            |
| TEXT[]            | `Vec<String>`                        | for `tags`, `domains`                          |
| UUID[]            | `Vec<uuid::Uuid>`                    | for `shared_with_groups`                       |
| JSONB             | `serde_json::Value`                  | for `metadata`, `domain_scores`                |
| TIMESTAMPTZ       | `chrono::DateTime<chrono::Utc>`      | requires sqlx `chrono` feature                 |
| VECTOR(N) NULL    | `Option<pgvector::Vector>`           | `.map(|v| v.to_vec())` to `Option<Vec<f32>>`   |
| FLOAT8            | `f64`                                |                                                |
| FLOAT8 NULL       | `Option<f64>`                        | for `fts_score`, `vector_score`                |

If `MemoryRecord` is extended in Phase 3 to carry `owner_user_id`,
`visibility`, `shared_with_groups`, and `codebase`, the conversion above
gets four more fields. Phase 2 reads them so the integration tests can
assert on them via SQL, but the trait surface does not expose them yet.

---

## `fts_only` and `vector_only` -- dedicated single-branch queries

The master plan offers two options for the convenience methods: reuse
`rrf_search` with one branch nulled, or write dedicated queries. The
dedicated queries win:

- One CTE instead of three. Planner picks the obvious plan.
- No FULL OUTER JOIN.
- No `params` indirection -- bound parameters used directly.
- The output `score` is the branch's native score (BM25-ish `ts_rank_cd` /
  cosine similarity), not an RRF fusion score over one branch. Callers of
  `fts_search` and `vector_search` get an intuitive score back.

### `fts_only`

Bound parameters:

- `$1`: text query (TEXT, must be non-empty; the caller guards `text.is_empty()`)
- `$2`: limit (INT)

```sql
SELECT
    m.id                  AS "id!: uuid::Uuid",
    m.owner_user_id       AS "owner_user_id!: uuid::Uuid",
    m.visibility          AS "visibility!: String",
    m.shared_with_groups  AS "shared_with_groups!: Vec<uuid::Uuid>",
    m.codebase            AS "codebase: String",
    m.domains             AS "domains!: Vec<String>",
    m.domain_scores       AS "domain_scores!: serde_json::Value",
    m.content             AS "content!: String",
    m.node_type           AS "node_type!: String",
    m.tags                AS "tags!: Vec<String>",
    m.embedding           AS "embedding: pgvector::Vector",
    m.metadata            AS "metadata!: serde_json::Value",
    m.created_at          AS "created_at!: chrono::DateTime<chrono::Utc>",
    m.updated_at          AS "updated_at!: chrono::DateTime<chrono::Utc>",
    ts_rank_cd(m.search_vec, websearch_to_tsquery('english', $1))
                          AS "fts_score!: f64"
FROM knowledge_nodes m
WHERE m.search_vec @@ websearch_to_tsquery('english', $1)
ORDER BY ts_rank_cd(m.search_vec, websearch_to_tsquery('english', $1)) DESC
LIMIT $2;
```

The Rust caller maps each row to a `SearchResult` with:

```rust
SearchResult {
    record,
    score: fts_score,
    fts_score: Some(fts_score),
    vector_score: None,
}
```

If `text.is_empty()` the caller returns `Ok(Vec::new())` before hitting
the database. `websearch_to_tsquery('english', '')` returns an empty
tsquery that matches nothing; the round-trip is wasted work otherwise.

### `vector_only`

Bound parameters:

- `$1`: embedding (pgvector::Vector)
- `$2`: limit (INT)

```sql
SELECT
    m.id                  AS "id!: uuid::Uuid",
    m.owner_user_id       AS "owner_user_id!: uuid::Uuid",
    m.visibility          AS "visibility!: String",
    m.shared_with_groups  AS "shared_with_groups!: Vec<uuid::Uuid>",
    m.codebase            AS "codebase: String",
    m.domains             AS "domains!: Vec<String>",
    m.domain_scores       AS "domain_scores!: serde_json::Value",
    m.content             AS "content!: String",
    m.node_type           AS "node_type!: String",
    m.tags                AS "tags!: Vec<String>",
    m.embedding           AS "embedding: pgvector::Vector",
    m.metadata            AS "metadata!: serde_json::Value",
    m.created_at          AS "created_at!: chrono::DateTime<chrono::Utc>",
    m.updated_at          AS "updated_at!: chrono::DateTime<chrono::Utc>",
    1.0 - (m.embedding <=> $1) AS "vector_score!: f64"
FROM knowledge_nodes m
WHERE m.embedding IS NOT NULL
ORDER BY m.embedding <=> $1
LIMIT $2;
```

The Rust caller maps each row to:

```rust
SearchResult {
    record,
    score: vector_score,
    fts_score: None,
    vector_score: Some(vector_score),
}
```

Both convenience methods ignore `SearchQuery.domains` / `tags` /
`node_types` / `min_retrievability` -- they take `&str` and `&[f32]`
respectively, not a `SearchQuery`. Callers that want filters on a
single-branch search should call `search` with the other branch input
left at its degrade-to-zero default.

---

## `min_retrievability` filter

`SearchQuery::min_retrievability: Option<f64>` is applied as a final
filter after fusion by joining on the `scheduling` table:

```sql
-- with-min-retrievability variant: identical CTEs to the base query, only
-- the final SELECT changes.
SELECT
    ... (same column list as the base query) ...
FROM fused
JOIN knowledge_nodes m ON m.id = fused.id
JOIN scheduling s ON s.memory_id = m.id
WHERE s.retrievability >= $8
ORDER BY fused.rrf_score DESC
LIMIT (SELECT final_limit FROM params);
```

This is a separate prepared statement -- the eight-parameter variant --
held alongside the seven-parameter base. The Rust dispatch:

```rust
if let Some(min_r) = query.min_retrievability {
    sqlx::query_as::<_, SearchRow>(QUERY_WITH_MIN_R)
        .bind(q_text)
        .bind(q_vec)
        .bind(overfetch)
        .bind(final_limit)
        .bind(dom_filter)
        .bind(nt_filter)
        .bind(tag_filter)
        .bind(min_r)
        .fetch_all(pool).await?
} else {
    sqlx::query_as::<_, SearchRow>(QUERY_BASE)
        .bind(q_text)
        .bind(q_vec)
        .bind(overfetch)
        .bind(final_limit)
        .bind(dom_filter)
        .bind(nt_filter)
        .bind(tag_filter)
        .fetch_all(pool).await?
}
```

Why not unconditionally join: the `scheduling` join is expensive enough on
a large `knowledge_nodes` table that adding it to every search call regresses the
common path. `min_retrievability` is set by the cognitive engine's
accessibility filter and is `None` in most direct callers.

The same two-variant pattern repeats for `fts_only` and `vector_only`; in
practice callers of those methods rarely set `min_retrievability` (it is
not part of their argument list), so only the base variant is needed
unless the trait surface grows.

---

## Domain / tag / node_type filters

Each filter is expressed as a NULL-conditional clause inside both branch
CTEs, written using PostgreSQL array operators:

```sql
AND (p.dom_filter IS NULL OR m.domains   && p.dom_filter)
AND (p.nt_filter  IS NULL OR m.node_type =  ANY(p.nt_filter))
AND (p.tag_filter IS NULL OR m.tags      && p.tag_filter)
```

- `&&` is the PostgreSQL "arrays overlap" operator. Matches if any
  element in `m.domains` is in the filter array. Index-friendly with a
  GIN index on `m.domains` (created in `0001_init`).
- `= ANY(...)` matches `m.node_type` (a scalar) against any element of
  the filter array. Index-friendly with a B-tree on `m.node_type`.
- `&&` is used again on `m.tags` (a `TEXT[]`).

The NULL-conditional form is critical: when the filter parameter is
`NULL`, the clause short-circuits to `TRUE` and contributes nothing to
the WHERE. This keeps a single query reusable across "no filter" and
"filter set" cases without rewriting SQL.

When the Rust caller passes `None` for a filter, sqlx binds it as `NULL`
of the column type (`text[]`). The cast `$5::text[]` in the `params` CTE
is what tells sqlx the binding type.

The master plan's draft has each filter clause duplicated across both
branch CTEs. That duplication is correct -- the planner cannot push a
WHERE clause across a FULL OUTER JOIN into both sides automatically; we
do it manually.

---

## Empty-string text query handling

The base query guards the FTS branch with `WHERE p.q_text <> ''`.

`websearch_to_tsquery('english', '')` returns an empty tsquery. An empty
tsquery has no lexemes and matches no document; the `@@` operator returns
false for every row. Without the guard, the FTS branch would still run --
sequential scan, tokenisation per row, comparison -- and return zero
rows. The guard short-circuits at planning time.

The guard does not affect the FULL OUTER JOIN: when the FTS branch
returns zero rows, the join degenerates to "every row that the vector
branch returned, with `f.id IS NULL` and `f.rank IS NULL`". The
`COALESCE(1.0 / (60 + f.rank), 0.0)` then evaluates to `0.0`, and the
fused score reduces to the vector branch's RRF term alone. This is the
"pure vector search" degrade path.

Symmetrically, the vector branch guards itself with
`WHERE m.embedding IS NOT NULL AND p.q_vec IS NOT NULL`, which gives the
"pure FTS search" degrade path when the caller passes no embedding.

The both-empty case (`q_text == ''` and `q_vec IS NULL`) is intercepted
in Rust before the query runs and returns `Ok(vec![])`. Returning empty
rather than error matches the SQLite behavior and is what the Phase 1
ingest pipeline relies on for "no signal, no results" fallback.

---

## Use of `sqlx::query!` versus `sqlx::query_as`

`sqlx::query!` and `sqlx::query_as!` are compile-time-checked: the SQL is
sent to a live Postgres at build time, the result schema is validated, and
the generated Rust struct fields are derived. That checking is the
default for every other query in `0002d-store-impl-bodies.md`.

For the RRF query, the macro path is impractical for two reasons:

1. **Two structurally different queries** -- the base (seven parameters,
   no `scheduling` join) and the `min_retrievability` variant (eight
   parameters, with the join). The macro would force two macro
   invocations, each producing its own anonymous result struct, and the
   result types would not unify. Manual `From` impls would be needed in
   both directions.
2. **The dedicated `fts_only` and `vector_only` queries have a different
   output column set** (`fts_score!` instead of `rrf_score! + fts_score? +
   vector_score?`). Three macro invocations, three structs, three
   conversion helpers.

The chosen pattern is `sqlx::query_as::<_, SearchRow>(SQL_CONST)` with a
single `SearchRow` struct that owns the column types and a single
`SearchRow::into_result` helper. The SQL is held in module-scope `&'static
str` constants:

```rust
const QUERY_BASE:        &str = include_str!("search.rrf.sql");
const QUERY_WITH_MIN_R:  &str = include_str!("search.rrf.min_retr.sql");
const QUERY_FTS_ONLY:    &str = include_str!("search.fts.sql");
const QUERY_VECTOR_ONLY: &str = include_str!("search.vector.sql");
```

`include_str!` keeps the SQL out of the Rust source. The four `.sql`
files live next to `search.rs` in
`crates/vestige-core/src/storage/postgres/`.

The cost: schema drift (someone renames `m.codebase` to `m.repo_name`)
will not break the build. The integration tests in "Verification" below
are the safety net. This is a deliberate trade -- it is the one sub-plan
in Phase 2 where runtime flexibility beats compile-time checking.

If a future contributor wants compile-time checking back for the simple
case, the right move is to introduce a `#[cfg(test)]`-only macro-checked
variant of `QUERY_BASE` and assert at test build time that the macro
agrees with the string. That belongs in `0002h-testing-and-benches.md` if
anywhere.

---

## Verification

Integration tests live in
`crates/vestige-core/tests/postgres_search.rs`, gated by
`#[cfg(feature = "postgres-backend")]` and `#[ignore]` by default (the
test runner CI workflow in `0002h-testing-and-benches.md` runs them with
`--ignored` against a live Postgres).

Common harness for every test:

1. Spin up Postgres via `sqlx::PgPool::connect` against the test URL.
2. Run `sqlx::migrate!("./migrations/postgres").run(&pool)` to bring the
   schema up.
3. Register a deterministic test embedder via `register_model` so
   `embedding` columns can be written.
4. Seed 50 mixed memories through `MemoryStore::insert` -- mixed
   `node_type` (`fact`, `concept`, `event`, `decision`), mixed `tags`
   (`rust`, `postgres`, `search`, `dream`, `bug-fix`), mixed `codebase`,
   embeddings drawn from three small clusters so vector recall has
   structure to find.

Test cases:

**T1. Full RRF returns the seeded target.**
Insert a known memory with `content = "FSRS-6 spaced repetition cadence"`
and an embedding from cluster A. Query with
`text = Some("FSRS spaced repetition")` and an embedding near cluster A.
Assert the target memory is in the top 3 of the returned `SearchResult`s
and that both `fts_score` and `vector_score` are `Some` for it.

**T2. Pure FTS degrade.**
Same target as T1. Query with `text = Some("FSRS spaced repetition")` and
`embedding = None`. Assert the target appears, all results have
`vector_score == None`, `fts_score == Some(_)`, and `score` equals the
fused RRF score (which collapses to one branch's `1.0/(60 + rank)`).

**T3. Pure vector degrade.**
Same target as T1. Query with `text = Some("")` and
`embedding = Some(cluster_A_vector)`. Assert the target appears, all
results have `fts_score == None`, `vector_score == Some(_)`.

**T4. Both empty returns `Ok(vec![])`.**
Query with `text = Some("")` and `embedding = None`. Assert exactly an
empty result vector and that no SQL was executed (assert via a
`sqlx::PgPool` query-count handle if convenient; otherwise document that
the short-circuit lives in Rust).

**T5. `domains` filter.**
Insert one memory with `domains = vec!["domain-x"]` and 49 others without
it. Query with `domains = Some(vec!["domain-x"])` and a matching text.
Assert exactly one result is returned and it is the seeded memory.

**T6. `tags` filter.**
Same pattern as T5 with `tags = Some(vec!["bug-fix"])`.

**T7. `node_types` filter.**
Same pattern as T5 with `node_types = Some(vec!["decision"])`.

**T8. `min_retrievability` filter.**
Seed two memories with the same content + embedding. Write
`scheduling` rows so that one has `retrievability = 0.9` and the other
`0.1`. Query with `min_retrievability = Some(0.5)`. Assert exactly the
high-retrievability memory is returned.

**T9. `query.limit == 0` defaults to 10.**
Seed 30 matching memories. Query with `limit = 0`. Assert the result
contains exactly 10 entries.

**T10. `fts_only` and `vector_only` parity.**
For the same target memory, call `fts_only` and `vector_only` directly
and compare against `search` with the corresponding branch zeroed. The
top-1 result must match by id; the scores need only be of the same sign
and magnitude (not bit-identical, because RRF fusion changes the
absolute score).

**T11. Schema-drift canary.**
Run the base query against an empty `knowledge_nodes` table and `fetch_all`
into `Vec<SearchRow>`. Any added NOT NULL column on `knowledge_nodes` that is
not in the SELECT will fail the test at the `try_get` boundary with a
clear error. This is the test that compensates for not using
`sqlx::query!`.

**T12. Hostile inputs.**
Query with `text = Some("'; DROP TABLE knowledge_nodes; --")` and a normal
embedding. Assert no panic, results returned cleanly, `knowledge_nodes` table
still present. This is symbolic; `websearch_to_tsquery` is parameter-
bound and SQL injection is not actually possible, but the test is cheap
and the assertion is real.

---

## Acceptance criteria

A reviewer of the implementation PR should be able to confirm:

1. `crates/vestige-core/src/storage/postgres/search.rs` exists and is
   compiled only when `feature = "postgres-backend"` is on.
2. The four `.sql` files (`search.rrf.sql`,
   `search.rrf.min_retr.sql`, `search.fts.sql`, `search.vector.sql`)
   exist in the same directory and are `include_str!`-ed into module-
   scope `&'static str` constants.
3. `RRF_K = 60` and `OVERFETCH_MULT = 3` are defined as constants at
   module scope with the Cormack 2009 citation in a comment.
4. The seven-parameter base query is one statement and uses a `params`
   CTE; the eight-parameter `min_retrievability` variant adds exactly
   one JOIN and one WHERE clause on top of the base.
5. Empty text degrades to pure vector; null embedding degrades to pure
   FTS; both empty short-circuits to `Ok(vec![])` in Rust before the
   query runs.
6. The SELECT column list in every query includes `owner_user_id`,
   `visibility`, `shared_with_groups`, and `codebase` even though Phase 2
   does not filter on them.
7. `SearchRow::into_result` populates a `MemoryRecord` with every field
   the trait requires, including `domains` and `domain_scores` decoded
   from JSONB.
8. `PgMemoryStore::search`, `PgMemoryStore::fts_search`, and
   `PgMemoryStore::vector_search` each delegate to the corresponding
   free function with one line of body.
9. All twelve integration tests (`T1` through `T12`) pass against a live
   Postgres with the `0001_init` + `0002_hnsw` migrations applied.
10. `cargo build -p vestige-core` succeeds with
    `--features postgres-backend` and with the feature off.
11. `cargo clippy -p vestige-core --features postgres-backend -- -D warnings`
    is clean.

When all eleven are true, this sub-plan is done and
`0002f-migrate-cli.md` is unblocked.
