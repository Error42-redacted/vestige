# ADR 0002: Phase 2 Execution -- Postgres Backend Integration, Phase 1 Amendment

**Status**: Accepted
**Date**: 2026-05-26
**Related**: [docs/adr/0001-pluggable-storage-and-network-access.md](0001-pluggable-storage-and-network-access.md), [docs/plans/0002-phase-2-postgres-backend.md](../plans/0002-phase-2-postgres-backend.md)

---

## Context

ADR 0001 set the architectural direction: introduce `MemoryStore` and `Embedder`
traits, ship a Postgres backend behind a feature flag, and reach a single shared
memory brain across machines. Phase 1 (storage trait extraction) shipped on
`feat/storage-trait-phase1` (790c0c8). The Phase 2 master plan at
`docs/plans/0002-phase-2-postgres-backend.md` was drafted before Phase 1 was
frozen.

Starting Phase 2 surfaces a small set of execution-level decisions that ADR 0001
did not cover and that the master plan now disagrees with the live code on.
These decisions are too big to silently absorb into a per-step plan and too
small to amend ADR 0001. They live here.

Three concrete realities driving this ADR:

1. **Trait shape mismatch.** Master plan 0002 assumed `trait_variant::make`
   produced distinct `MemoryStore` (Send-bound) and `LocalMemoryStore`
   (non-Send) variants, and that errors were `StoreError`. Phase 1 froze on
   `#[async_trait::async_trait]` with `pub use MemoryStore as LocalMemoryStore`
   and an error type called `MemoryStoreError`. The Postgres backend has to
   follow Phase 1, not the master plan -- but we should record that explicitly.
2. **`SqliteMemoryStore` is monolithic.**
   `crates/vestige-core/src/storage/sqlite.rs` is ~8200 lines. Phase 1 appended
   the trait impl block at the bottom of the same file. Adding a similarly
   large `postgres.rs` perpetuates the pattern; this is the natural moment to
   decide whether the SQLite file gets split.
3. **Constructor surface drift.** Master plan 0002 specifies
   `PgMemoryStore::connect(url, max_connections, &dyn Embedder)`. The Phase 1
   `SqliteMemoryStore` constructor takes no embedder -- registry consistency
   runs through `registered_model()` / `register_model()` on the trait,
   invoked by the caller. The two backends should look the same to a caller;
   right now they would not.
4. **Multi-tenancy is a one-way door.** The Postgres schema is the place to
   reserve user/group/visibility columns *now*, even though Phase 3 is the
   phase that wires the auth filter using them. Adding `owner_user_id` and
   GIN indexes to a populated, HNSW-indexed `knowledge_nodes` table later is an
   expensive online migration; reserving NULL-defaulted columns at schema
   creation is ~10 lines of SQL. The same logic applies to per-memory
   context capture (codebase, MCP caller, session) -- promoting `codebase`
   to a first-class column now keeps the door open for context-aware
   sharing rules in Phase 4 without touching `knowledge_nodes`. See D7 and D8.

This ADR is also the umbrella under which Phase 2 sub-plans (`0002a-...`,
`0002b-...`, etc.) sit. The intent is: ADR + sub-plans land as one PR for
review; the implementation lands as a second PR with many commits inside.

---

## Already Decided (carried in by reference)

These are settled by ADR 0001 or by explicit agreement during this session.
Listed here so the discussion frame is clear; not re-litigated below.

- Postgres backend ships behind a `postgres-backend` Cargo feature, default
  OFF. Mutually compilable with SQLite. (ADR 0001.)
- Single big `MemoryStore` trait. `PgMemoryStore` implements the same surface
  as `SqliteMemoryStore`. (ADR 0001.)
- pgvector HNSW + tsvector + GIN + RRF hybrid search in one SQL statement.
  (Master plan 0002, D4-D5.)
- sqlx 0.8 + pgvector 0.4 + compile-time-checked queries + offline `.sqlx/`
  cache committed. (Master plan 0002.)
- Two sqlx migration files: `0001_init` (extensions, tables, non-vector
  indexes) and `0002_hnsw` (HNSW separated for re-embed drop/recreate).
  (Master plan 0002, D4.)
- `vestige migrate --from sqlite --to postgres` and
  `vestige migrate --reembed --model=<new>` CLI subcommands. (ADR 0001 +
  master plan 0002, D8-D10.)
- PR cadence: PR #1 carries this ADR plus all sub-plans; PR #2 carries the
  implementation as many commits.
- Sub-plans use `0002a-`, `0002b-`, ... suffixes off `0002-`.
- `PgMemoryStore::connect` lands as `todo!()` in the skeleton; real body
  comes later.

---

## Decisions

### D1. Sunset async_trait across the Phase 1 traits

Phase 1 froze with `#[async_trait::async_trait]` on both the `MemoryStore`
trait (`storage/memory_store.rs:194`) and the `Embedder` trait
(`embedder/mod.rs:27`), plus their SQLite and Fastembed impl blocks. async_trait
boxes every async fn into `Pin<Box<dyn Future + Send>>` -- one heap allocation
per call inside the hottest code path. We are amending Phase 1 to remove
async_trait entirely and replace it with `trait_variant::make`, so each trait
becomes two real generated variants (`MemoryStore` / `LocalMemoryStore`,
`Embedder` / `LocalEmbedder`) with `Send` bounds on the outer variant.

Scope split across three Phase 1 amendment sub-plans:

- **`0001a-trait-rewrite.md`** -- Rewrite `MemoryStore` only. Touches
  `storage/memory_store.rs` (trait declaration) and `storage/sqlite.rs`
  (impl block attribute). Leaves async_trait in place on the embedder side
  so the diff stays focused.
- **`0001b-sqlite-split.md`** -- Pure code motion. Splits the
  ~8200-line `sqlite.rs` into a `sqlite/` directory. Independent of D1; can
  land in either order relative to `0001a`.
- **`0001c-async-trait-sunset.md`** -- Rewrite `Embedder` the same way, then
  remove `async-trait = "0.1"` from `crates/vestige-core/Cargo.toml`. Final
  amendment commit removes the dependency entirely. After this lands, the
  workspace contains zero references to `async_trait`.

All three sub-plans land on the existing `feat/storage-trait-phase1` branch
(790c0c8 has not been opened upstream yet; amend in place, no force-push to a
public PR).

### D2. PgMemoryStore::connect mirrors SqliteMemoryStore::new

```rust
impl PgMemoryStore {
    pub async fn connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self>;
    pub async fn from_pool(pool: PgPool) -> MemoryStoreResult<Self>;
}
```

No `Embedder` in the constructor. The pgvector-specific
`ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE vector($N)` DDL lives
inside the trait method `register_model(&ModelSignature)`. That method is
called by the caller (cognitive engine bootstrap, migrate CLI, tests) after
construction, exactly as it is for `SqliteMemoryStore`.

`MemoryStoreError` gains two variants behind the feature flag (added during
the Postgres impl, not during the Phase 1 amendment):
```rust
#[cfg(feature = "postgres-backend")]
#[error("postgres error: {0}")]
Postgres(#[from] sqlx::Error),

#[cfg(feature = "postgres-backend")]
#[error("postgres migration error: {0}")]
Migrate(#[from] sqlx::migrate::MigrateError),
```

### D3. Split sqlite.rs into a sqlite/ directory as Phase 1 amendment

Pure code motion, no behavioural change. Target layout:
```
crates/vestige-core/src/storage/sqlite/
  mod.rs           -- SqliteMemoryStore struct, new(), reader/writer locks
  crud.rs          -- insert/get/update/delete
  search.rs        -- fts_search, vector_search, hybrid search
  scheduling.rs    -- FSRS state methods
  graph.rs         -- edges, neighbors
  domain.rs        -- domain CRUD, classify stub
  registry.rs      -- embedding_model table + register_model
  portable_sync.rs -- portable archive backend bridge
  trait_impl.rs    -- impl LocalMemoryStore for SqliteMemoryStore
```

Cognitive-module imports stay on `crate::storage::SqliteMemoryStore` and
related re-exports from `storage/mod.rs`; the split is private to the
module. Each motion commit must keep `cargo test -p vestige-core` green for
bisectability.

This lands in the Phase 1 amendment PR alongside D1 (separate commit, same
branch).

### D4. Postgres backend as a directory from day one

```
crates/vestige-core/src/storage/postgres/
  mod.rs           -- PgMemoryStore struct, connect, from_pool, trait impl
  pool.rs          -- PgPool construction from PostgresConfig
  migrations.rs    -- sqlx::migrate! wrapper
  registry.rs      -- ensure_registry, ALTER COLUMN TYPE vector(N)
  search.rs        -- RRF query + row mapping
  migrate_cli.rs   -- SQLite -> Postgres streaming copy
  reembed.rs       -- O(n) re-encode + HNSW rebuild
```

D1+D2 of the master plan land first as a skeleton in `mod.rs` with `todo!()`
bodies; later sub-plans fill in the other files.

### D5. Sub-plan layout: two phases worth of sub-plans

Phase 1 amendment sub-plans (under `docs/plans/`):
- `0001a-trait-rewrite.md` -- MemoryStore async_trait -> trait_variant, call-site audit
- `0001b-sqlite-split.md` -- sqlite.rs -> sqlite/ directory, commit-by-commit
- `0001c-async-trait-sunset.md` -- Embedder rewrite + drop async-trait dep from Cargo.toml

Phase 2 sub-plans (under `docs/plans/`):
- `0002a-skeleton-and-feature-gate.md` -- master plan D1 + D2 (todo!() bodies)
- `0002b-pool-and-config.md` -- master plan D3 + D7
- `0002c-migrations.md` -- master plan D4
- `0002d-store-impl-bodies.md` -- master plan D2 real bodies + D6 registry
- `0002e-hybrid-search.md` -- master plan D5
- `0002f-migrate-cli.md` -- master plan D8 + D10
- `0002g-reembed.md` -- master plan D9
- `0002h-testing-and-benches.md` -- master plan D14 + D15
- `0002i-runbook.md` -- master plan D16

Each sub-plan is a self-contained brief sized to fit one focused
implementation session (handed to Claude Code as a `/goal` instruction
without requiring the agent to load the master plan).

### D6. SQLite split does not get its own ADR

The split is pure code motion; no public types, behaviour, or paths change.
`0001b-sqlite-split.md` is enough.

### D7. Multi-tenancy schema reservation (L1-L3)

Phase 2 reserves the columns and tables needed for future per-user / per-group
visibility, so Phase 3 (auth) does not require a column-add migration over a
populated, HNSW-indexed `knowledge_nodes` table. Single-user behaviour is unchanged
in both backends.

New tables in `0001_init.up.sql`:

```sql
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
    role      TEXT NOT NULL DEFAULT 'member',   -- 'member' | 'admin'
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, group_id)
);

INSERT INTO users (id, handle, display_name)
  VALUES ('00000000-0000-0000-0000-000000000001', 'local', 'Local User');
```

New columns on `knowledge_nodes`:

```sql
owner_user_id      UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001'
                       REFERENCES users(id),
visibility         TEXT NOT NULL DEFAULT 'private',   -- 'private' | 'group' | 'public'
shared_with_groups UUID[] NOT NULL DEFAULT '{}'

CREATE INDEX idx_knowledge_nodes_owner         ON knowledge_nodes (owner_user_id);
CREATE INDEX idx_knowledge_nodes_shared_groups ON knowledge_nodes USING GIN (shared_with_groups);
```

Phase 3 visibility filter (declared here for reference; implemented in Phase 3):

```sql
WHERE
       (visibility = 'private' AND owner_user_id = $me)
    OR (visibility = 'group'
        AND (owner_user_id = $me OR shared_with_groups && $my_group_ids))
    OR  visibility = 'public'
```

Why tri-state enum and not just `shared_with_groups[] + is_public`: the
explicit `visibility` field documents intent at the row level. A `'private'`
row with a non-empty `shared_with_groups` is detectable inconsistency
(a CHECK constraint can enforce it later) rather than silent data.

SQLite parity: same tables and columns with identical defaults.
`shared_with_groups` is a JSON `'[]'` text encoding (no array type).
Single-user mode never changes any of these values; the trait surface ignores
the visibility filter for SQLite because there is exactly one user.

Sharing automation (matching by domain, tag, repo, MCP caller, ...) is
explicitly **not** in Phase 2. See D8 for context capture, and the Follow-ups
section for the Phase 4 `sharing_rules` design sketch.

RLS policies are not declared in Phase 2. Phase 3 decides whether to add
RLS as defense-in-depth on top of the app-layer filter.

### D8. Context-aware ingest

Every memory carries its ingest context, so future automation (sharing rules,
domain scoping, audit) can match on it without a schema migration. Most of
this is already happening in the Phase 1 ingest pipeline; D8 promotes it to
ADR-level commitment so Phase 2 cannot drop it on the way to Postgres.

Context dimensions and where they live:

- **`codebase`** -- promoted to a first-class indexed column on `knowledge_nodes`.
  High-frequency query path (`SELECT ... WHERE codebase = 'vestige'`) for
  both human exploration and Phase 4 HDBSCAN scoping. Direct B-tree index
  beats JSONB extraction.
  ```sql
  codebase TEXT,  -- nullable; populated from ingest context
  CREATE INDEX idx_knowledge_nodes_codebase ON knowledge_nodes (codebase) WHERE codebase IS NOT NULL;
  ```
  `MemoryRecord` gains `pub codebase: Option<String>`.

- **`mcp_client_id`** -- which MCP caller created this. Persistent identity
  once Phase 3 API keys exist. Lives in `metadata.mcp_client_id` (JSONB).
  Not query-hot enough to deserve a column.

- **`session_id`** -- ephemeral; identifies the calling session for runtime
  override scoping. Lives in `metadata.session_id` (JSONB). Sessions die
  fast; storing them as rows or indexed columns is waste.

- **`file` / `topics`** -- existing optional context already accepted by the
  ingest pipeline. Stay in metadata JSONB.

Phase 2's job for D8 is operational, not architectural: audit the ingest
path from MCP request to row write to ensure none of these fields gets
dropped when crossing the SQLite -> Postgres backend boundary.

---

## PR Cadence

Two work streams, three PRs total:

1. **PR A: Phase 1 amendment**
   - Branch: `feat/storage-trait-phase1` (existing, amended in place)
   - Commits: MemoryStore trait rewrite (0001a) + sqlite split (0001b, multiple
     motion commits) + Embedder rewrite & async-trait dep removal (0001c).
   - Sub-plans `0001a-`, `0001b-`, `0001c-` are committed on this branch.

2. **PR B: ADR 0002 + Phase 2 sub-plans (this document + the 9 sub-plans)**
   - New branch off PR A's tip once that is reviewed.
   - No code; docs only.

3. **PR C: Phase 2 implementation**
   - New branch off PR B's tip.
   - One PR with many commits clustered by sub-plan.

PR B is the "let's discuss execution before writing code" gate. PR C is the
"now we write code" gate. If PR A is itself sizable enough that it needs the
amendments reviewed in stages, the three sub-plans (`0001a`, `0001b`, `0001c`)
can split into separate PRs; that's a tactical call at PR time.

---

## Architecture Overview

Final layout after the Phase 1 amendment (PR A) and Phase 2 implementation
(PR C):

```
crates/vestige-core/src/storage/
  mod.rs              -- re-exports, Storage alias for BC
  memory_store.rs     -- trait_variant-generated MemoryStore + LocalMemoryStore, types, error
  migrations.rs       -- SQLite migration registry (Phase 1, unchanged)
  portable.rs         -- portable archive format (Phase 1, unchanged)
  sqlite/             -- was sqlite.rs (D3, Phase 1 amendment)
    mod.rs            -- SqliteMemoryStore struct, new(), reader/writer locks
    crud.rs           -- insert/get/update/delete
    search.rs         -- fts/vector/hybrid
    scheduling.rs     -- FSRS state
    graph.rs          -- edges, neighbors
    domain.rs         -- domain CRUD, classify stub
    registry.rs       -- embedding_model table + register_model
    portable_sync.rs  -- portable backend bridge
    trait_impl.rs     -- impl LocalMemoryStore for SqliteMemoryStore
  postgres/           -- D4, Phase 2
    mod.rs            -- PgMemoryStore struct, connect, from_pool, trait impl
    pool.rs           -- PgPool construction from config
    migrations.rs     -- sqlx::migrate! wrapper
    registry.rs       -- register_model body, ALTER COLUMN TYPE vector(N)
    search.rs         -- RRF query + row mapping
    migrate_cli.rs    -- SQLite -> Postgres streaming copy
    reembed.rs        -- O(n) re-encode + HNSW rebuild

crates/vestige-core/migrations/
  sqlite/             -- Phase 1, with V15 migration for D7+D8 columns/tables
  postgres/           -- Phase 2
    0001_init.up.sql  -- includes D7 tables + columns, D8 codebase column
    0001_init.down.sql
    0002_hnsw.up.sql
    0002_hnsw.down.sql
```

Tables in the Postgres schema after migration 0001:

| Table | Purpose | Phase that populates |
|-------|---------|----------------------|
| `embedding_model` | One-row registry of name/dim/hash | Phase 2 (first connect) |
| `knowledge_nodes` | Core records + owner/visibility/codebase | Phase 2 ingest; Phase 4 fills `domains` |
| `scheduling` | FSRS state | Phase 2 |
| `edges` | Spreading activation graph | Phase 2 |
| `review_events` | Append-only FSRS review log | Phase 2; Phase 5 federation reads |
| `domains` | Phase 4 cluster centroids | Phase 4 |
| `users` | L1 identities (D7) | Phase 3 |
| `groups` | L3 groups (D7) | Phase 3 |
| `group_memberships` | L3 user-group links (D7) | Phase 3 |

`sharing_rules` (Phase 4) and `api_keys` (Phase 3) are added later by their
own migrations.

---

## Alternatives Considered

| Alternative | Why not |
|-------------|---------|
| Keep async_trait on the Phase 1 trait | One heap allocation per trait call inside the hottest code path in Vestige. Boxing every future also obscures the actual return type, which makes lifetimes and Send-ness harder to reason about. The Phase 1 PR is not opened upstream yet, so amending is free. |
| Take `&dyn Embedder` into `connect` | Couples constructor to embedder; breaks ADR 0001's separation; can't be used by callers that don't have an embedder yet (tests, migrate CLI). |
| Defer SQLite split | Postgres lands alongside an 8K-line peer; the pattern compounds; future readers see "backends are huge here". |
| Single `postgres.rs` | Master plan calls out 7 sub-files; we know it's getting split; doing it twice is waste. |
| Per-deliverable sub-plans (16 docs) | Review fatigue; many sub-plans would be 3-5 lines of Cargo or one migration each. Logical groups cluster naturally with PR commits. |
| One rolling sub-plan with checkboxes | Moving target; doesn't serve as a `/goal` brief for a fresh Claude Code session. |
| Separate ADR for the SQLite split | Pure code motion with no public-surface change; doesn't constrain future decisions. ADRs are for decisions that bind. |
| Punt multi-tenancy schema entirely to Phase 3 | Adding `owner_user_id` and indexes to a populated, HNSW-indexed `knowledge_nodes` table later is an expensive online migration. Reserving NULL-defaulted columns now is ~10 lines of SQL. |
| `shared_with_groups[] + is_public` instead of tri-state visibility enum | More compact but `visibility = 'private'` documents intent at the row level; a CHECK constraint can later enforce array/enum consistency. Two columns conveying one fact is fine when both are referenced often. |
| Add `shared_with_users[]` for direct user-to-user sharing | A "group of one" subsumes it without an extra column and GIN index. Phase 3 CLI can auto-create singleton groups if a user requests direct shares. |
| Bake per-domain or per-tag sharing defaults into Phase 2 schema | Sharing automation needs real usage data before committing to fuzzy (domain centroids) vs crisp (tags) vs context (codebase / MCP caller). Phase 4 designs a generic `sharing_rules` table that matches on any context dimension; deferring costs nothing because rules live in a new table, not new columns. |
| `codebase` stays in JSONB metadata | High-frequency query path (HDBSCAN scoping, codebase-wide searches, future `sharing_rules` match). B-tree on a real column beats GIN on a JSONB key for this access pattern. Cost is one nullable TEXT column. |

---

## Consequences

### Positive
- Phase 1 trait stops boxing futures on every call. Lifetimes and Send-ness
  become inspectable instead of hidden inside an `async_trait` macro expansion.
- `connect` stays backend-agnostic; tests and CLI tools stand up either backend
  without an `Embedder` in scope.
- Cognitive module imports never change paths -- the SQLite split is private
  to `storage/sqlite/`, public re-exports through `storage/mod.rs` unchanged.
- Postgres backend lands already-modular; future SQL changes touch one of
  seven small files, not one of eight thousand lines.
- Phase 2 master plan stays archival; ADR 0002 + sub-plans are the live source
  of truth for execution.
- Multi-tenancy columns reserved now means Phase 3 auth is purely additive --
  no online migration over a populated, HNSW-indexed `knowledge_nodes` table.
- Context-aware ingest (D8) keeps the door open for repo / session /
  MCP-caller-scoped sharing rules in Phase 4 without changing `knowledge_nodes`.

### Negative
- The Phase 1 amendment expands a "finished" branch. It is a real cost: the
  trait rewrite touches every cognitive module that holds a store handle.
- SQLite split is a pure-motion diff. Annoying to review even when safe.
- Three PRs (amendment, ADR+plans, implementation) instead of one or two.
  Discipline tax in exchange for reviewability.
- Multi-tenancy reservation adds three never-queried tables and three
  always-default columns to the SQLite schema. Real but small storage cost in
  single-user mode (a single bootstrap row + empty tables + NULL/empty
  defaults per memory).

### Risks
- **Trait rewrite breaks a cognitive module's Send-ness expectation.**
  Mitigation: `cargo test --workspace` runs after each call-site edit;
  trait_variant-generated `MemoryStore` is the Send variant and matches the
  current `Arc<dyn ...>` usage everywhere except thread-local impls (none
  exist today).
- **SQLite motion commit introduces a silent semantic change.** Mitigation:
  each commit keeps `cargo test -p vestige-core` green; reviewer can bisect.
- **Sub-plan boundaries don't match how implementation wants to commit.**
  Mitigation: sub-plans are advisory; the implementation PR clusters commits
  however it ends up needing to.
- **Reserved columns get used in Phase 3 in a way that mismatches Phase 2
  defaults.** Mitigation: Phase 3 owns the auth filter; Phase 2 defaults
  (`owner_user_id = local`, `visibility = 'private'`) are intentionally the
  "no access for anyone but the owner" worst-case; widening at Phase 3 is
  safe, narrowing would be the dangerous direction.
- **Memory: PR A amendment invalidates the locally-deployed Phase 1 binary's
  ABI.** Not a real risk -- the trait change is purely source-level Rust; the
  on-disk DB schema is unchanged. The rebuilt binary slots in over the
  current one without DB migration.

---

## Resolved Decisions

| # | Question | Resolution |
|---|----------|------------|
| Q1 | Phase 1 trait shape | Rewrite with trait_variant::make. Amend Phase 1 PR. |
| Q2 | PgMemoryStore::connect signature | Mirror SqliteMemoryStore::new; no Embedder. register_model does the pgvector typmod stamp. |
| Q3 | Split sqlite.rs | Yes, as Phase 1 amendment. sqlite.rs -> sqlite/ directory; pure code motion. |
| Q4 | Postgres module layout | Directory from day one. |
| Q5 | Sub-plan granularity | Logical groups, ~9 docs for Phase 2 plus 2 for the Phase 1 amendment. |
| Q6 | ADR for SQLite split | No. Sub-plan `0001b-sqlite-split.md` is sufficient. |
| Q7 | Multi-tenancy schema | Reserve users / groups / group_memberships tables and owner_user_id / visibility / shared_with_groups columns on knowledge_nodes in Phase 2. Single-user defaults; Phase 3 fills in real values. |
| Q8 | Visibility encoding | Tri-state enum `'private' \| 'group' \| 'public'` plus `shared_with_groups[]`. No `shared_with_users[]`; no RLS in Phase 2. |
| Q9 | Sharing automation grain | Per-memory only in Phase 2. Phase 4 ships a generic `sharing_rules` table matching on codebase / tag / node_type / mcp_client_id. |
| Q10 | Context capture on ingest | `codebase` promoted to a first-class indexed column; `mcp_client_id` and `session_id` stay in metadata JSONB. |

---

## Follow-ups

- Phase 1 amendment sub-plans drafted: `0001a-trait-rewrite.md`,
  `0001b-sqlite-split.md`, `0001c-async-trait-sunset.md`. Ready to execute on
  `feat/storage-trait-phase1`.
- Phase 2 sub-plans drafted: `0002a-` through `0002i-` against the accepted
  decisions above. Ready to execute on a new branch off PR A's tip.
- Decide branch placement for this ADR before it gets committed -- it cannot
  live on `feat/storage-trait-phase1` (that branch is now PR A's code-only
  amendment branch). Likely a new branch off PR A's tip for PR B (docs only).
- Validate local Postgres dev cluster before PR C work begins. Recipe at
  `docs/plans/local-dev-postgres-setup.md` is correct but needs to be applied
  on this machine (delandtj-home): cluster is not initdb'd, pgvector is not
  installed. Containerized `pgvector/pgvector:pg18` is a viable alternative
  if pgvector packaging is friction. See open discussion thread.

### Phase 4 sketch: `sharing_rules` and the precedence chain

Recorded here so the Phase 4 author does not have to rediscover the design.
Phase 2 does **not** implement any of this; it only ensures the schema and
ingest context capture make this possible without a `knowledge_nodes` migration.

```sql
-- Phase 4 migration (not Phase 2)
CREATE TABLE sharing_rules (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_user_id    UUID NOT NULL REFERENCES users(id),
  -- Match: any subset; all set fields must match conjunctively
  match_codebase   TEXT,
  match_tag        TEXT,
  match_node_type  TEXT,
  match_api_key_id UUID REFERENCES api_keys(id),   -- MCP caller identity
  -- Policy
  visibility       TEXT NOT NULL,
  shared_with_groups UUID[] NOT NULL DEFAULT '{}',
  -- Conflict resolution
  priority   INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Precedence on ingest, first match wins:

1. Caller-explicit visibility in the MCP request
2. Active session override held by the MCP server (per-session, in-memory,
   not persisted; matched by `session_id`)
3. Highest-priority `sharing_rules` row whose match fields all hold
4. User's `default_visibility` (typically `'private'`)

Per-session overrides do not persist; storing ephemeral session IDs as DB
rows is waste. Per-codebase / per-MCP-caller rules do persist as
`sharing_rules` rows.
