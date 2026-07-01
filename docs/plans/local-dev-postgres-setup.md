# Local Dev Postgres Setup (container, hybrid approach)

**Status**: Applied on this machine on 2026-05-27 (rootless podman, Postgres 18.4 + pgvector 0.8.2).
**Related**: docs/plans/0002-phase-2-postgres-backend.md, docs/adr/0002-phase-2-execution.md, docs/adr/0001-pluggable-storage-and-network-access.md

Purpose: capture the minimum, repeatable steps to stand up a long-lived
Postgres 18 + pgvector instance on a local Linux dev box for Phase 2
(`PgMemoryStore`) development, `sqlx prepare`, and manual migration
testing. This is a single-operator dev recipe, not a production runbook.

ADR 0002 picked the **hybrid container** approach over a native install:
the `pgvector/pgvector:pg18` image ships pgvector pre-installed, matches
the image testcontainers will use in the Phase 2 test harness, and avoids
the AUR/build-from-source friction of native pgvector packaging on Arch.

---

## Current state on this machine

- Runtime: rootless `podman` 5.8.2 (Arch). `docker` 29.5.1 also installed but unused.
- Image: `docker.io/pgvector/pgvector:pg18` (PostgreSQL 18.4, pgvector 0.8.2).
- Container: `vestige-pg`, `--restart=always`, port `127.0.0.1:5432:5432`.
- Volume: named podman volume `vestige-pgdata`, mounted at
  `/var/lib/postgresql/data` inside the container; `PGDATA` points at
  `/var/lib/postgresql/data/pgdata` so the volume mount is non-empty at
  init time (Postgres refuses to initdb into a non-empty directory).
- Listens on: `127.0.0.1:5432` only (port mapping is bound to loopback).
- Auth: `scram-sha-256` (image default for both local socket and host).

### Database + role

- Database: `vestige`, UTF8, owner `vestige`, `LC_COLLATE=C.UTF-8`, `LC_CTYPE=C.UTF-8`.
- Role: `vestige` with `LOGIN CREATEDB` (no superuser, no replication).
- Schema `public` re-owned to `vestige` with full default privileges on
  future tables / sequences / functions.
- Extension: `vector` (pgvector 0.8.2) installed in the `vestige`
  database by the superuser at setup time.

Net effect: the `vestige` role can create, alter, drop, and grant freely
inside the `vestige` database -- enough for `sqlx::migrate!`, ad-hoc
schema work, and the full Phase 2 `MemoryStore` surface. It cannot create
extensions; the superuser handled `CREATE EXTENSION vector` already.

### Passwords

Two passwords live in the dev user's home, mode 600:

- `~/.vestige_pg_superpw` -- the `postgres` superuser password inside the
  container. Used for one-shot admin tasks (creating roles, installing
  extensions, password rotation). Day-to-day app traffic does NOT use it.
- `~/.vestige_pg_pw` -- the `vestige` role password. This is the one the
  Phase 2 backend, `sqlx prepare`, and ad-hoc `psql` invocations use.

### Connection

```
postgresql://vestige:<password>@127.0.0.1:5432/vestige
```

Recommended dev shell export (keep this OUT of the repo; use `.env` +
gitignore or a shell rc):

```sh
export DATABASE_URL="postgresql://vestige:$(cat ~/.vestige_pg_pw)@127.0.0.1:5432/vestige"
```

---

## Reproduce from scratch

On a fresh Linux box with `podman` installed and `python3` available:

```sh
# 1. Pull the image
podman pull docker.io/pgvector/pgvector:pg18

# 2. Create a persistent named volume
podman volume create vestige-pgdata

# 3. Generate the superuser password and stash it (mode 600)
SUPER_PW=$(python3 -c 'import secrets,string; a=string.ascii_letters+string.digits; print("".join(secrets.choice(a) for _ in range(32)))')
umask 077
printf '%s' "$SUPER_PW" > ~/.vestige_pg_superpw
chmod 600 ~/.vestige_pg_superpw

# 4. Start the container
podman run -d \
  --name vestige-pg \
  --restart=always \
  -p 127.0.0.1:5432:5432 \
  -e POSTGRES_PASSWORD="$SUPER_PW" \
  -e PGDATA=/var/lib/postgresql/data/pgdata \
  -v vestige-pgdata:/var/lib/postgresql/data \
  docker.io/pgvector/pgvector:pg18

unset SUPER_PW

# 5. Wait for ready
until podman exec vestige-pg pg_isready -U postgres -h 127.0.0.1 >/dev/null 2>&1; do
  sleep 1
done

# 6. Generate the vestige role password and stash it (mode 600)
VESTIGE_PW=$(python3 -c 'import secrets,string; a=string.ascii_letters+string.digits; print("".join(secrets.choice(a) for _ in range(32)))')
umask 077
printf '%s' "$VESTIGE_PW" > ~/.vestige_pg_pw
chmod 600 ~/.vestige_pg_pw

# 7. Create role + database + grants + extension (runs as superuser inside the container)
podman exec -i vestige-pg psql -U postgres -v ON_ERROR_STOP=1 <<SQL
CREATE ROLE vestige WITH LOGIN CREATEDB PASSWORD '${VESTIGE_PW}';
CREATE DATABASE vestige OWNER vestige ENCODING 'UTF8'
  TEMPLATE template0 LC_COLLATE 'C.UTF-8' LC_CTYPE 'C.UTF-8';
GRANT ALL PRIVILEGES ON DATABASE vestige TO vestige;
SQL

podman exec -i vestige-pg psql -U postgres -d vestige -v ON_ERROR_STOP=1 <<'SQL'
GRANT ALL ON SCHEMA public TO vestige;
ALTER SCHEMA public OWNER TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO vestige;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO vestige;
CREATE EXTENSION IF NOT EXISTS vector;
SQL

unset VESTIGE_PW

# 8. Smoke test as the vestige role
PGPASSWORD="$(cat ~/.vestige_pg_pw)" psql -h 127.0.0.1 -U vestige -d vestige \
  -c "SELECT current_user, current_database(), version();" \
  -c "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';" \
  -c "SELECT '[1,2,3]'::vector <-> '[3,2,1]'::vector AS l2_distance;"
```

---

## Boot persistence (rootless podman)

`--restart=always` keeps the container alive across podman daemon
restarts, but rootless podman containers do NOT auto-start on system
boot unless the dev user has lingering enabled:

```sh
sudo loginctl enable-linger "$USER"
```

After that, the `podman-restart.service` user unit handles restart of
`--restart=always` containers when the user session starts at boot:

```sh
systemctl --user enable --now podman-restart.service
```

Skip both if you prefer to start the cluster manually each session with
`podman start vestige-pg`.

---

## Day-to-day operation

```sh
# Status
podman ps --filter name=vestige-pg

# Logs (follow)
podman logs -f vestige-pg

# psql as the app role
PGPASSWORD="$(cat ~/.vestige_pg_pw)" psql -h 127.0.0.1 -U vestige -d vestige

# psql as the superuser (for grants, extensions, role admin)
podman exec -it vestige-pg psql -U postgres

# Stop / start
podman stop vestige-pg
podman start vestige-pg

# Restart in place
podman restart vestige-pg
```

---

## Password rotation

```sh
# Rotate the vestige role password
NEW_PW=$(python3 -c 'import secrets,string; a=string.ascii_letters+string.digits; print("".join(secrets.choice(a) for _ in range(32)))')
umask 077
printf '%s' "$NEW_PW" > ~/.vestige_pg_pw
chmod 600 ~/.vestige_pg_pw
podman exec -i vestige-pg psql -U postgres -v ON_ERROR_STOP=1 \
  -c "ALTER ROLE vestige WITH PASSWORD '${NEW_PW}';"
unset NEW_PW

# Rotate the superuser password (less common)
NEW_SUPER=$(python3 -c 'import secrets,string; a=string.ascii_letters+string.digits; print("".join(secrets.choice(a) for _ in range(32)))')
umask 077
printf '%s' "$NEW_SUPER" > ~/.vestige_pg_superpw
chmod 600 ~/.vestige_pg_superpw
podman exec -i vestige-pg psql -U postgres -v ON_ERROR_STOP=1 \
  -c "ALTER ROLE postgres WITH PASSWORD '${NEW_SUPER}';"
unset NEW_SUPER
```

Then re-export `DATABASE_URL` in any live shells.

---

## Backup and restore (dev-grade)

`pg_dump` writes a plain-text SQL dump to host disk. For dev data this is
enough; production runbook lives in `0002i-runbook.md`.

```sh
# Dump
PGPASSWORD="$(cat ~/.vestige_pg_pw)" pg_dump -h 127.0.0.1 -U vestige -d vestige \
  --format=plain --no-owner > vestige-$(date +%Y%m%d-%H%M%S).sql

# Restore (drops + recreates)
podman exec -i vestige-pg psql -U postgres -v ON_ERROR_STOP=1 \
  -c 'DROP DATABASE IF EXISTS vestige;' \
  -c 'CREATE DATABASE vestige OWNER vestige ENCODING UTF8 TEMPLATE template0;'
PGPASSWORD="$(cat ~/.vestige_pg_pw)" psql -h 127.0.0.1 -U vestige -d vestige < vestige-DUMP.sql
```

The named volume `vestige-pgdata` persists outside the container; the
container can be `podman rm`'d and recreated without losing data, as
long as the volume stays in place.

---

## Teardown

Destroys the cluster and all data in it:

```sh
podman stop vestige-pg
podman rm vestige-pg
podman volume rm vestige-pgdata
podman rmi docker.io/pgvector/pgvector:pg18
rm -f ~/.vestige_pg_pw ~/.vestige_pg_superpw
```

`enable-linger` and the user systemd unit can be undone with
`sudo loginctl disable-linger "$USER"` and
`systemctl --user disable podman-restart.service` if you turned them on.

---

## Notes for Phase 2

- `pgvector` is preinstalled in the image; the `CREATE EXTENSION vector`
  in step 7 above makes it available inside the `vestige` DB. The
  extension must be loaded BEFORE `sqlx::migrate!` runs the Phase 2
  migration that declares typed `Vector` columns, otherwise the
  migration fails.
- Testcontainer-based Phase 2 integration tests use the same
  `pgvector/pgvector:pg18` image and spin up fresh containers per run;
  they are independent of this long-lived cluster. This cluster exists
  for `sqlx prepare`, `cargo run -- migrate --to postgres`, and manual
  poking.
- `sqlx prepare` needs `DATABASE_URL` pointed at this cluster with
  `vestige` migrations already applied. Run from `crates/vestige-core/`.

---

## Out of scope for this doc

- TLS, client-cert auth, non-localhost access. Phase 3 exposes the
  Vestige HTTP API over the network, not Postgres directly.
- PITR, WAL archiving, replication, PgBouncer, tuned `postgresql.conf`.
  Defaults are fine for Phase 2 development.
- Native (non-container) Postgres install. The prior version of this
  doc covered native Arch packaging; superseded by ADR 0002's hybrid
  decision.
- Making this the canonical Vestige backend. By default Vestige still
  uses SQLite; this cluster exists so the `postgres-backend` feature
  can be built and tested locally.
