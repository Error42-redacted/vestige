# Phase 2 Sub-Plan 0002i -- Postgres Ops Runbook

**Status**: Ready
**Depends on**: Phase 2 sub-plans 0002a through 0002h merged (or at least
their interfaces stable). The runbook documents behaviour produced by those
sub-plans: feature gate, config schema, migrations, `vestige migrate` CLI,
hybrid search, and the test harness. Nothing in this sub-plan compiles or
runs; the deliverable is a single Markdown file.

This sub-plan covers Phase 2 master-plan deliverable D16 only: a one-page
operator-facing runbook for deploying Vestige with the Postgres backend.

---

## Context

Why a runbook. The ADR (0002) and the master plan (0002) are written for
implementors. They settle execution-level decisions and itemise deliverables.
They are not deployable instructions. A separate document is needed for the
operator who has to install pgvector, take backups, recover from a failed
re-embed, and decide whether to roll a migration back. The runbook is that
document.

Who reads it. Ops people, not developers. Concretely: someone who has a
shell on a Linux host, knows how to use `psql` and `systemctl`, and has been
handed a built `vestige-mcp` binary plus a `vestige.toml`. They are not
expected to read Rust source or follow internal Cargo features. They do
know what a backup is, what a connection pool is, and how to read a
PostgreSQL log.

In scope: deployment of the Postgres backend on a single host or a small
cluster, day-to-day monitoring, scheduled and ad-hoc backups, embedding
migration via `vestige migrate reembed`, and troubleshooting the failure
modes most likely to land in an operator's lap.

Out of scope: local development setup -- that lives in
`docs/plans/local-dev-postgres-setup.md` and the runbook links to it for
developer onboarding only. Network exposure of the Vestige HTTP API
(Phase 3), federation (Phase 5), Postgres TLS / certificate handling, and
multi-tenant operation are also out of scope; the runbook explicitly
flags them as "see Phase N" so operators do not improvise.

This sub-plan is the plan for producing the runbook. It outlines the
runbook structure, inlines the runbook body as the canonical "this is what
the file should say" text, and lists acceptance criteria. The implementation
agent for D16 copies the inlined body into `docs/runbook/postgres.md`,
creating `docs/runbook/` if it does not already exist. No other files in the
repository are modified.

---

## Deliverable

The artifact produced by executing this sub-plan is exactly one new file:

```
docs/runbook/postgres.md
```

It is NOT under `docs/plans/`. Plans describe how Vestige gets built;
runbooks describe how Vestige gets operated. The two directories are
deliberately separated.

Side effect: create the directory `docs/runbook/` if it does not exist.
Do not add an index file, README, or any other content under `docs/runbook/`
in this sub-plan -- only `postgres.md`.

This sub-plan document (`docs/plans/0002i-runbook.md`) is itself NOT a
deliverable in the operator sense. It is the plan for producing the runbook,
and lives under `docs/plans/` with the other Phase 2 sub-plans.

---

## Runbook structure

The runbook is organised as a flat list of ten sections, in order. Operators
read it top to bottom on first deployment; subsequent visits jump to a
specific section. Section numbering matches the inlined body below.

1. **Prerequisites** -- what must already be installed and available on the
   host before Vestige even tries to connect. PostgreSQL 16 or newer
   (18 on Arch is fine), pgvector >= 0.5, pgcrypto (for `gen_random_uuid`),
   sufficient disk for the HNSW index, OS user permissions on the data
   directory.

2. **Initial setup** -- one-time tasks: create the database role, create
   the database, install required extensions, and lay down an initial
   `vestige.toml`. Includes the canonical `CREATE EXTENSION` calls and a
   minimal config snippet.

3. **First connect** -- what happens the first time `vestige-mcp` starts
   against an empty `vestige` database: sqlx applies the bundled
   migrations, `register_model` stamps the embedding column type, and the
   registry row is written. How an operator verifies each step succeeded
   using `psql`.

4. **Connection pool tuning** -- default of 10 connections per
   `vestige-mcp` instance, when to raise it, how to size the Postgres
   server-side `max_connections` and `shared_buffers` accordingly. Cross-
   reference to `vestige.toml` and to ADR 0002 D2 / open question Q5.

5. **Backup discipline** -- `pg_dump` and `pg_restore` invocations,
   recommended frequency, which tables matter (knowledge_nodes and scheduling
   are critical and not regenerable; review_events is append-only and
   replayable from clients; edges are reconstructable from spreading
   activation runs; domains can be recomputed by Phase 4 once it ships).
   Also covers backup verification (restore-to-tmp drill).

6. **Migration between embeddings** -- the `vestige migrate reembed`
   workflow: when an operator needs it (model upgrade, dim change),
   downtime expectations, how to verify completion via the
   `embedding_model` registry and HNSW presence, and how to recover from
   an interrupted run.

7. **Re-clustering domains** -- a brief forward reference. Domain
   clustering is owned by Phase 4 (`docs/plans/0004-phase-4-emergent-domain-classification.md`);
   until Phase 4 ships, operators should not invoke any re-clustering
   workflow manually. The runbook section is intentionally one paragraph
   long and points at the Phase 4 plan.

8. **Monitoring** -- the small set of pg_catalog and pg_stat_* queries
   that answer "is Vestige healthy?": `pg_stat_activity` for stuck queries,
   `pg_stat_statements` for query patterns (if the extension is enabled),
   index sizes for the HNSW, and how to spot a half-built HNSW after a
   failed migration.

9. **Troubleshooting** -- a table of common errors with the symptom and
   the fix. Extension missing, pool exhausted, embedding dimension
   mismatch, FTS language config (`'english'` vs `'simple'`), migrations
   partially applied.

10. **Rollback caveats** -- every `*.up.sql` has a `*.down.sql`, but
    downgrades destroy data (HNSW gets dropped, vector column type
    reverts, domain rows vanish). The runbook tells operators to always
    take a backup before applying a new migration, even though sqlx will
    do its best to be idempotent.

---

## Runbook body

The full text below is what should be copied verbatim into
`docs/runbook/postgres.md`. ASCII only. Code blocks use fenced syntax with
language hints. Operator-facing prose; second person ("you") for
instructions. Where a command requires sudo, the prompt shows it explicitly.

```markdown
# Vestige Postgres Backend -- Operator Runbook

This runbook covers deploying, operating, monitoring, and recovering a
Vestige installation that uses the Postgres backend. It is written for
operators handling a built `vestige-mcp` binary and a `vestige.toml`.

For local development setup, see
`docs/plans/local-dev-postgres-setup.md`. For the architectural rationale,
see `docs/adr/0001-pluggable-storage-and-network-access.md` and
`docs/adr/0002-phase-2-execution.md`. For the deliverable-level plan, see
`docs/plans/0002-phase-2-postgres-backend.md`.

---

## 1. Prerequisites

Before Vestige can connect:

- PostgreSQL server, version 16 or newer. Arch ships 18.x; Debian stable
  ships 16.x; both work.
- `pgvector` extension, version 0.5 or newer. Distro packages:
  `pgvector` on Arch, `postgresql-16-pgvector` on Debian/Ubuntu.
- `pgcrypto` extension, shipped with the PostgreSQL contrib package
  (`postgresql-contrib` on Debian, included in the base `postgresql`
  package on Arch). Vestige uses `gen_random_uuid()` from pgcrypto for
  primary keys.
- Disk space: budget roughly 4x the size of your `knowledge_nodes.embedding`
  column for the HNSW index. With 768-dim float32 vectors at 100k
  memories, that is about 1.2 GB for the embeddings plus 4-5 GB for the
  HNSW index. Plan accordingly.
- OS user: the `postgres` system user (or whatever user owns
  `/var/lib/postgres/data`) must have read/write on the data directory.
  Vestige itself does not need filesystem access to Postgres; it talks
  TCP only.
- Network: Vestige and Postgres can be on the same host (loopback) or
  different hosts. If different hosts, allow the Vestige host's IP in
  `pg_hba.conf` and on any firewall.

---

## 2. Initial setup

These steps run once per Postgres cluster.

### 2.1 Install extensions

As the `postgres` superuser:

```sh
sudo -u postgres psql -d vestige <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
SQL
```

Verify:

```sh
sudo -u postgres psql -d vestige -c \
  "SELECT extname, extversion FROM pg_extension WHERE extname IN ('vector','pgcrypto');"
```

You should see two rows. If `vector` is missing, the pgvector package was
not installed for the right PostgreSQL major version; reinstall it.

### 2.2 Create the role and database

The `vestige` role owns its own database; it does NOT need superuser.
Extensions must be installed by `postgres`, not by `vestige`.

```sh
sudo -u postgres psql -v ON_ERROR_STOP=1 <<'SQL'
CREATE ROLE vestige WITH LOGIN CREATEDB PASSWORD 'CHANGE_ME';
CREATE DATABASE vestige OWNER vestige ENCODING 'UTF8';
GRANT ALL PRIVILEGES ON DATABASE vestige TO vestige;
SQL

sudo -u postgres psql -d vestige -v ON_ERROR_STOP=1 <<'SQL'
GRANT ALL ON SCHEMA public TO vestige;
ALTER SCHEMA public OWNER TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO vestige;
SQL
```

Replace `CHANGE_ME` with a strong password and store it where Vestige can
read it (typically `~/.vestige_pg_pw`, mode 600, owned by the user running
`vestige-mcp`).

### 2.3 Minimal `vestige.toml`

```toml
[storage]
backend = "postgres"

[storage.postgres]
url = "postgresql://vestige:CHANGE_ME@127.0.0.1:5432/vestige"
max_connections = 10
```

The `url` field accepts a `${VAR}` placeholder; in practice operators
either inline the password or export `DATABASE_URL` and reference
`url = "${DATABASE_URL}"`. See `docs/CONFIGURATION.md` for the full
schema once Phase 3 lands.

---

## 3. First connect

When `vestige-mcp` starts against an empty `vestige` database, it:

1. Builds a `PgPool` of `max_connections` (default 10) connections.
2. Runs every migration in `crates/vestige-core/migrations/postgres/`
   in order. The bundled migrations are `0001_init` (tables, non-vector
   indexes) and `0002_hnsw` (HNSW index on `knowledge_nodes.embedding`).
3. Calls `register_model` once it knows the active embedder's dimension.
   This issues `ALTER TABLE knowledge_nodes ALTER COLUMN embedding TYPE
   vector($N)` and inserts a row into `embedding_model`.
4. Begins accepting MCP requests.

To verify after the first start:

```sh
sudo -u postgres psql -d vestige <<'SQL'
-- All expected tables present.
\dt
-- embedding_model has exactly one row.
SELECT name, dimension, hash FROM embedding_model;
-- The HNSW index exists.
SELECT indexname FROM pg_indexes
  WHERE tablename = 'knowledge_nodes' AND indexname LIKE '%hnsw%';
SQL
```

Expected: `knowledge_nodes`, `scheduling`, `edges`, `domains`, `review_events`,
`embedding_model`, `users`, `groups`, `group_memberships`; one row in
`embedding_model`; one `idx_knowledge_nodes_embedding_hnsw` index.

If a migration fails mid-way, the partial state lands in
`_sqlx_migrations`. See section 9 for recovery.

---

## 4. Connection pool tuning

Defaults:

- Vestige client pool: `max_connections = 10` per `vestige-mcp` instance.
- Postgres server: `max_connections = 100` (default).

Math: one MCP client with the default pool uses up to 10 server slots.
Five concurrent MCP clients use up to 50 slots. The remaining 50 cover
`psql` sessions, background workers, and headroom for replication or
backup processes.

When to raise:

- More than three MCP clients connecting to one Postgres instance.
- Long-running queries (above 500ms p99) showing pool wait time in
  Vestige logs (look for `pool acquire timed out` warnings).
- A noticeable number of concurrent dream/consolidation runs.

How to raise:

```toml
[storage.postgres]
max_connections = 20   # client side, per vestige-mcp instance
```

And on the Postgres server, edit `postgresql.conf`:

```conf
max_connections = 200
shared_buffers = 2GB     # roughly 25 percent of RAM, never above 8GB
```

Then restart Postgres (`sudo systemctl restart postgresql`). Vestige
clients pick up their own `max_connections` change on next restart.

Do not raise pool sizes blindly. Past about 4x the CPU core count,
Postgres throughput drops; a small connection pooler (PgBouncer in
transaction mode) is the right answer above ~200 client connections,
but Vestige's expected scale rarely needs that.

---

## 5. Backup discipline

### 5.1 Which tables matter

| Table | Backup priority | Regenerable? |
|-------|-----------------|--------------|
| `knowledge_nodes` | Critical | No |
| `scheduling` | Critical | No (FSRS state) |
| `embedding_model` | Critical | No (one row, but stamps the column type) |
| `users`, `groups`, `group_memberships` | Critical | No (Phase 3 will populate) |
| `review_events` | Important | Replayable by clients but tedious |
| `edges` | Optional | Yes (recomputed by spreading activation) |
| `domains` | Optional | Yes (Phase 4 recomputes by clustering) |

For a typical single-operator install, dumping the whole database is
fastest and simplest. Skip the optional tables only if dump size becomes
a bandwidth problem.

### 5.2 Full logical backup

```sh
pg_dump --host=127.0.0.1 --username=vestige --format=custom \
        --file=vestige-$(date -u +%Y%m%dT%H%M%SZ).dump \
        vestige
```

The custom format compresses by default and works with parallel restore.
File size for 10k memories: roughly 80 MB.

Frequency recommendations:

- Daily for any installation with active ingest.
- Before every `vestige migrate reembed` run (see section 6).
- Before every Postgres major-version upgrade.
- Retain at least 7 daily, 4 weekly, 3 monthly dumps. Compress with
  `--format=custom` (already gzipped) and keep them on different
  storage from the database itself.

### 5.3 Restore

To a fresh database:

```sh
sudo -u postgres createdb -O vestige vestige_restore
pg_restore --host=127.0.0.1 --username=vestige --dbname=vestige_restore \
           --jobs=4 vestige-20260301T030000Z.dump
```

To replace the live database (destructive; only after taking a fresh
dump):

```sh
sudo systemctl stop vestige-mcp     # or however the service is run
sudo -u postgres dropdb vestige
sudo -u postgres createdb -O vestige vestige
pg_restore --host=127.0.0.1 --username=vestige --dbname=vestige \
           --jobs=4 vestige-20260301T030000Z.dump
sudo systemctl start vestige-mcp
```

### 5.4 Restore drill

Run a restore-to-throwaway-database every month and run `vestige search`
or a manual `psql` count against it. A backup you have not restored is a
backup you do not have.

```sh
sudo -u postgres createdb -O vestige vestige_restore_drill
pg_restore --host=127.0.0.1 --username=vestige --dbname=vestige_restore_drill \
           --jobs=4 vestige-latest.dump
PGPASSWORD="$(cat ~/.vestige_pg_pw)" psql -h 127.0.0.1 -U vestige \
  -d vestige_restore_drill \
  -c 'SELECT count(*) FROM knowledge_nodes;'
sudo -u postgres dropdb vestige_restore_drill
```

---

## 6. Migration between embeddings

Use `vestige migrate reembed` when:

- Upgrading to a new embedding model that produces a different dimension
  (for example, swapping from `nomic-embed-text-v1.5` 768D to a 1024D
  model).
- Switching providers and the model hash differs even at the same
  dimension.

What it does:

1. Reads every row from `knowledge_nodes`, re-encodes the `content` column
   through the new embedder, and writes the new vector back.
2. Drops the HNSW index before the re-encode loop (this is the default;
   `--concurrent-index` keeps it during the run at the cost of speed).
3. Updates the `embedding_model` row with the new name, dimension, and
   hash.
4. Rebuilds the HNSW index with the new vectors.

### 6.1 Before starting

- Take a fresh backup (section 5.2). The tool refuses to start without a
  `--yes` flag if it detects no recent backup; ignore at your peril.
- Stop ingest. Vestige's MCP server can stay running for read-only
  access, but pause any client that calls `smart_ingest` or
  `update_scheduling`.
- Have the new embedder model available locally. The CLI loads it
  before the first row is touched; if loading fails, no data is changed.

### 6.2 Running

```sh
vestige migrate reembed --model=<new-model-name> --yes
```

Add `--concurrent-index` if you cannot accept the brief window during
HNSW rebuild where queries do not use the index (sequential scan
fallback works but is slow).

The tool prints a progress bar via `indicatif`. Expected throughput:
roughly 200 memories per second per CPU core for a 768D ONNX model.
10,000 memories on an 8-core box: about 6 seconds, plus HNSW rebuild
(another 30-90 seconds at that scale).

### 6.3 Verifying completion

```sh
sudo -u postgres psql -d vestige <<'SQL'
-- Registry reflects the new model.
SELECT name, dimension, hash FROM embedding_model;
-- HNSW index is present and not partial.
SELECT indexname, indexdef
  FROM pg_indexes
  WHERE tablename = 'knowledge_nodes' AND indexname LIKE '%hnsw%';
-- All rows have a non-null embedding of the new dimension.
SELECT count(*) FILTER (WHERE embedding IS NULL) AS missing,
       count(*)                                  AS total
  FROM knowledge_nodes;
SQL
```

Expected: registry shows the new model name and dimension, one HNSW
index, zero missing embeddings.

### 6.4 Recovering from an interrupted run

`vestige migrate reembed` is restartable. On interruption:

- The `embedding_model` row may or may not have been updated. Check it
  manually and roll forward by re-running with `--yes --resume` (the
  tool detects the inconsistency and finishes the rows that still hold
  old embeddings).
- The HNSW index may be missing. Re-running the command rebuilds it as
  its last step.
- If the system is in a state the tool refuses to reason about, restore
  from the backup taken in 6.1.

---

## 7. Re-clustering domains

Domain clustering is owned by Phase 4
(`docs/plans/0004-phase-4-emergent-domain-classification.md`). Until
Phase 4 ships, the `domains` table is reserved schema and is populated
only by tests. Operators must not invoke any domain re-clustering
workflow manually; there is no supported one in Phase 2.

When Phase 4 lands, this section is replaced with the real procedure.

---

## 8. Monitoring

### 8.1 Quick health check

```sh
PGPASSWORD="$(cat ~/.vestige_pg_pw)" psql -h 127.0.0.1 -U vestige -d vestige <<'SQL'
SELECT count(*) AS memory_count FROM knowledge_nodes;
SELECT name, dimension FROM embedding_model;
SELECT pg_size_pretty(pg_database_size('vestige')) AS db_size;
SQL
```

### 8.2 In-flight queries

```sql
SELECT pid, now() - query_start AS runtime, state, query
  FROM pg_stat_activity
  WHERE datname = 'vestige' AND state <> 'idle'
  ORDER BY runtime DESC NULLS LAST;
```

Anything over 5 seconds with `state = 'active'` deserves a look. HNSW
search queries should land well under 100ms on properly-sized hardware.

### 8.3 Query pattern analysis

If `pg_stat_statements` is loaded (`shared_preload_libraries =
'pg_stat_statements'` in `postgresql.conf`):

```sql
SELECT calls, mean_exec_time, query
  FROM pg_stat_statements
  WHERE query ILIKE '%knowledge_nodes%'
  ORDER BY mean_exec_time DESC
  LIMIT 20;
```

Look for hybrid-search queries that have drifted above 100ms p50. The
usual culprit is a missing or half-built HNSW index.

### 8.4 Index health

```sql
SELECT indexname, pg_size_pretty(pg_relation_size(indexrelid)) AS size,
       idx_scan, idx_tup_read
  FROM pg_indexes
  JOIN pg_stat_user_indexes USING (indexrelid)
  WHERE schemaname = 'public' AND relname = 'knowledge_nodes';
```

A HNSW index with `idx_scan = 0` after several hours of traffic usually
means the planner is preferring sequential scan -- either the table is
too small to bother with the index (fine) or the index is corrupt and
needs rebuilding (`REINDEX INDEX idx_knowledge_nodes_embedding_hnsw;`).

### 8.5 Spotting half-built HNSW

After a failed migration or a crashed `reembed`:

```sql
SELECT indexname, indisvalid, indisready
  FROM pg_indexes
  JOIN pg_index ON indexrelid = (schemaname || '.' || indexname)::regclass
  WHERE tablename = 'knowledge_nodes';
```

Any row with `indisvalid = false` is broken. Drop and recreate:

```sql
DROP INDEX IF EXISTS idx_knowledge_nodes_embedding_hnsw;
CREATE INDEX idx_knowledge_nodes_embedding_hnsw
  ON knowledge_nodes USING hnsw (embedding vector_cosine_ops);
```

---

## 9. Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `ERROR: extension "vector" is not available` on start | pgvector not installed for this Postgres major version | Install the distro package matching `pg_config --version`, then `CREATE EXTENSION vector;` as superuser |
| `pool timed out while waiting for an open connection` in Vestige logs | Pool too small or stuck queries holding connections | Raise `max_connections` in `vestige.toml`; investigate `pg_stat_activity` for queries above 5s |
| `vector dimensions do not match` on insert | `embedding_model` was stamped at one dimension and a different embedder is now running | Re-run `vestige migrate reembed --model=<correct>` or fix the embedder configuration |
| Hybrid search returns the same row twice | Stale `.sqlx/` query cache from before D5 landed | Run `cargo sqlx prepare` in `crates/vestige-core/`, rebuild the binary |
| `text search configuration "english" does not exist` | Postgres locale build does not include the english dictionary (rare on Alpine) | Install the language-pack or override the FTS language in `vestige.toml` (see `[storage.postgres.fts]` once Phase 2 D5 lands) |
| `relation "_sqlx_migrations" exists, but migration X is in "applied" with no checksum` | Previous run died between `BEGIN` and `COMMIT` | Stop Vestige, restore from backup, restart |
| HNSW index very large compared to data | `m` and `ef_construction` defaults too high for the corpus | Acceptable for now; tuning lands as part of Phase 4 |
| `permission denied for schema public` on a new install | `vestige` role does not own `public` | Re-run the grants block in section 2.2 as `postgres` |

If a problem is not in this table, capture: PostgreSQL log
(`/var/log/postgres/`, journalctl `-u postgresql`), Vestige log
(`RUST_LOG=debug,sqlx=info` for a fresh run), the migration state
(`SELECT * FROM _sqlx_migrations ORDER BY version;`), and file a bug.

---

## 10. Rollback caveats

Every migration in `crates/vestige-core/migrations/postgres/` has a
matching `*.down.sql`. `sqlx migrate revert` walks them in reverse order.

This is not the same as risk-free. The `0002_hnsw.down.sql` drops the
HNSW index (rebuildable, expensive). The `0001_init.down.sql` drops
every table -- including `knowledge_nodes`, including data. Down migrations
exist for development, not for casual production use.

Before applying any new migration:

1. Take a backup (section 5.2).
2. Run the migration on a restored copy first if you can afford the time.
3. Read the new migration's `*.up.sql` and `*.down.sql` to understand
   what changes.

To revert one migration manually:

```sh
sqlx migrate revert \
  --database-url "postgresql://vestige:...@127.0.0.1:5432/vestige" \
  --source crates/vestige-core/migrations/postgres
```

Note that Vestige's binary does not run `sqlx migrate revert`
automatically. Reverts are always an explicit operator decision.

If a revert fails partway through, treat the database as inconsistent:
restore from the backup taken in step 1.
```

---

## Cross-references

- `docs/adr/0001-pluggable-storage-and-network-access.md` -- ADR that
  established the pluggable backend.
- `docs/adr/0002-phase-2-execution.md` -- ADR settling Phase 2 execution
  decisions; section "Architecture Overview" lists every table the
  runbook references.
- `docs/plans/0002-phase-2-postgres-backend.md` -- master plan; D16
  (deliverables list) and the Open Implementation Questions section
  (especially Q4 HNSW rebuild and Q5 pool sizing) inform the runbook's
  recommendations.
- `docs/plans/local-dev-postgres-setup.md` -- developer-facing recipe
  for a one-machine Arch / CachyOS dev cluster. The runbook links to it
  as the "for development, see" pointer.
- `docs/CONFIGURATION.md` -- existing config doc; section 4 of the
  runbook ("Connection pool tuning") cross-references it for the
  authoritative `vestige.toml` schema.

---

## Verification

A reviewer is given:

- A fresh Linux VM (Debian 12 or Arch current; both must work) with
  network access and no Postgres installed.
- A built `vestige-mcp` binary for that platform.
- The runbook (`docs/runbook/postgres.md`).

The reviewer follows the runbook top to bottom and reaches a state in
which Vestige answers MCP requests against the Postgres backend.
Checkpoints, in order:

1. After section 1 (Prerequisites): `pg_config --version` returns 16 or
   newer; `pkg-config --modversion libpq` resolves; the `pgvector`
   distro package is installed.
2. After section 2.1 (Extensions): two rows in
   `SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pgcrypto');`.
3. After section 2.2 (Role + DB): `psql -U vestige -h 127.0.0.1 -d vestige -c '\conninfo'`
   succeeds.
4. After section 2.3 (Config): `vestige.toml` parses (test by
   `vestige config print` once that subcommand lands, otherwise
   `vestige-mcp --check-config`).
5. After section 3 (First connect): the eight expected tables are
   present; `embedding_model` has exactly one row; the HNSW index
   exists; `vestige-mcp` log shows "Postgres backend ready".
6. After section 5.2 (Backup): the dump file exists and `pg_restore -l`
   on it lists the expected tables.
7. After section 5.4 (Restore drill): the drill database holds the same
   row count as the source.

If any checkpoint fails, the runbook section that produced the failure
is the one that needs revision. Capture the exact command, exit code,
and log line; revise the runbook in a follow-up PR.

A second reviewer reads the runbook without executing it and checks for:

- ASCII only; no em dashes, no curly quotes, no Unicode arrows, no
  ellipses, no bullets (`*`/`-` ASCII only).
- Every section number from 1 to 10 present and in order.
- Every cross-reference resolves to an existing file or to a Phase
  number explicitly marked as "future".
- No code block longer than 30 lines; if longer, it should be split or
  referenced from another file.

---

## Acceptance criteria

- [ ] `docs/runbook/` directory exists.
- [ ] `docs/runbook/postgres.md` exists and matches the inlined body
      above byte-for-byte after stripping the outer code fence used in
      this sub-plan to embed it.
- [ ] All ten sections from the "Runbook structure" outline are present
      under their stated headings.
- [ ] No file other than `docs/runbook/postgres.md` is created or
      modified by executing this sub-plan.
- [ ] ASCII only: no em dashes, no curly quotes, no Unicode arrows,
      no ellipses, no Unicode bullets (`grep -P '[^\x00-\x7F]'
      docs/runbook/postgres.md` returns no matches).
- [ ] Every cross-reference in the runbook points at a file that exists
      in the repository at the time of merge, OR is explicitly framed
      as "future Phase N" with a pointer to the relevant plan document.
- [ ] Every command block is copy-pastable: no `<placeholder>` syntax
      that does not also have an inline note describing what to
      substitute.
- [ ] A second pair of eyes confirms the verification checkpoints in the
      preceding section are reproducible.
- [ ] The runbook is no longer than the inlined body in this sub-plan;
      operators reach the end without losing patience.
