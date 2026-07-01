# Sub-plan 0002b -- Pool construction and VestigeConfig

**Status**: Draft
**Master plan**: [0002-phase-2-postgres-backend.md](0002-phase-2-postgres-backend.md)
**ADR**: [0002-phase-2-execution.md](../adr/0002-phase-2-execution.md)
**Predecessor**: [0002a-skeleton-and-feature-gate.md](0002a-skeleton-and-feature-gate.md)

---

## Context

This sub-plan delivers two of the master plan's deliverables now that the
`0002a` skeleton has landed:

- **D3** -- pool construction in
  `crates/vestige-core/src/storage/postgres/pool.rs`. Replaces the `todo!()`
  body of `PgMemoryStore::connect` with a real `PgPool` builder that reads a
  `PostgresConfig`. Registry/migration calls remain `todo!()`; those are
  filled in by sub-plans `0002c` (migrations) and `0002d` (store bodies +
  registry).
- **D7** -- new module `crates/vestige-core/src/config.rs` containing
  `VestigeConfig`, `StorageConfig`, `SqliteConfig`, `PostgresConfig`,
  `EmbeddingsConfig`, plus a `ConfigError` enum and a loader that reads
  `vestige.toml`. The loader is wired into `vestige-mcp` so the running
  server picks SQLite or Postgres at startup based on the config file.

After this sub-plan:

- `cargo build` (default features, no `postgres-backend`) compiles and the
  MCP server still defaults to SQLite when no `vestige.toml` is present.
- `cargo build --features postgres-backend` compiles, with
  `PgMemoryStore::connect` now wiring through `pool.rs` (registry/migration
  still `todo!()` until `0002c` and `0002d`).
- A `vestige.toml` example can be round-tripped through
  `VestigeConfig::load` in a unit test.

This sub-plan deliberately does NOT:

- Add migrations (`0002c`).
- Fill in real CRUD/search bodies on `PgMemoryStore` (`0002d`, `0002e`).
- Add env-var override support (Phase 3 concern, called out in master plan
  D7 behaviour notes).

---

## Dependencies

- `0002a-skeleton-and-feature-gate.md` must be merged. That sub-plan creates
  `crates/vestige-core/src/storage/postgres/mod.rs` with:
  - `PgMemoryStore` struct holding `pool: PgPool`.
  - `PgMemoryStore::connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self>`
    body = `todo!()`.
  - `PgMemoryStore::from_pool(pool: PgPool) -> MemoryStoreResult<Self>`
    body = `todo!()`.
  - The trait impl block with all methods routed to `todo!()`.
  - The `postgres-backend` feature gate on the module declaration in
    `storage/mod.rs`.

This sub-plan extends those bodies and adds two siblings: `pool.rs` and
`registry.rs` (the latter is a stub here, real body in `0002d`).

---

## Audit step (do this first)

Before adding `config.rs`, confirm there is no existing top-level config
loader. Run from the repo root:

```bash
rg -nF 'VestigeConfig' crates/
rg -nF 'toml::from_str' crates/
rg -n '#\[derive.*Deserialize.*\]' crates/vestige-core/src/
```

If a `VestigeConfig` struct already exists from Phase 1, treat the "Config
module" section below as additive: extend the existing struct rather than
creating a new file. The cross-cut additions in that case are:

1. Add the `StorageConfig` enum (gated and ungated branches).
2. Add `SqliteConfig`, `PostgresConfig`.
3. Add the `default_path()` helper if missing.
4. Add `ConfigError` if a different error enum is used today (rename/extend
   instead of duplicating).

As of the audit at the time of this writing, no `VestigeConfig` exists in
`vestige-core`. `directories::ProjectDirs` is already used in
`vestige-core/src/embeddings/local.rs` and in
`vestige-mcp/src/protocol/auth.rs`, so the `directories` crate is already a
workspace dependency -- no new dep there.

---

## Cargo manifest additions

Add `toml` to `vestige-core`. `serde` and `thiserror` are already present
from Phase 1; `directories` is already a transitive dep but we add it
explicitly so `default_path()` is supported.

```bash
cd crates/vestige-core
cargo add toml@0.8
cargo add directories@5
```

No new deps on `vestige-mcp`; it already depends on `vestige-core`.

`sqlx` is already added by `0002a` behind the `postgres-backend` feature
with `runtime-tokio`, `tls-rustls`, `postgres`, `uuid`, `chrono`,
`json`, `macros`, `migrate` features. The pool module only uses what is
already pulled in.

---

## Config module

**File**: `crates/vestige-core/src/config.rs` (new).
**Re-exported** from `crates/vestige-core/src/lib.rs` as `pub mod config;` plus
`pub use config::{VestigeConfig, StorageConfig, SqliteConfig, EmbeddingsConfig, ConfigError};`
and `#[cfg(feature = "postgres-backend")] pub use config::PostgresConfig;`.

Full content:

```rust
//! Vestige top-level configuration.
//!
//! Loaded from `~/.vestige/vestige.toml` by default; the path is overridable
//! via `VestigeConfig::load(Some(&path))`. Parsing uses serde + toml; the
//! `[storage]` section is internally-tagged on a `backend` field so a single
//! enum dispatch picks SQLite or Postgres.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level configuration as parsed from `vestige.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct VestigeConfig {
    pub embeddings: EmbeddingsConfig,
    pub storage: StorageConfig,
    /// Reserved for Phase 3. Empty in Phase 2.
    pub server: ServerConfig,
    /// Reserved for Phase 3. Empty in Phase 2.
    pub auth: AuthConfig,
}

/// Embedding provider selection.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingsConfig {
    /// Provider key. Phase 2 ships `"fastembed"` only.
    pub provider: String,
    /// Model name. For fastembed this is e.g. `"nomic-ai/nomic-embed-text-v1.5"`.
    pub model: String,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            provider: "fastembed".to_string(),
            model: crate::DEFAULT_EMBEDDING_MODEL.to_string(),
        }
    }
}

/// Storage backend selection. Internally tagged on the `backend` field:
///
/// ```toml
/// [storage]
/// backend = "sqlite"
///
/// [storage.sqlite]
/// path = "/home/user/.vestige/vestige.db"
/// ```
///
/// or, when compiled with `--features postgres-backend`:
///
/// ```toml
/// [storage]
/// backend = "postgres"
///
/// [storage.postgres]
/// url = "postgres://vestige:secret@localhost:5432/vestige"
/// max_connections = 10
/// acquire_timeout_secs = 30
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase", deny_unknown_fields)]
pub enum StorageConfig {
    Sqlite(SqliteConfig),
    #[cfg(feature = "postgres-backend")]
    Postgres(PostgresConfig),
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Sqlite(SqliteConfig::default())
    }
}

/// SQLite backend configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqliteConfig {
    /// Path to the `vestige.db` file. If unset, the SqliteMemoryStore
    /// constructor picks its platform default location.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self { path: None }
    }
}

/// Postgres backend configuration. Only present when the `postgres-backend`
/// Cargo feature is enabled.
#[cfg(feature = "postgres-backend")]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    /// `postgres://user:pass@host:port/db` -- forwarded to
    /// `PgConnectOptions::from_str`.
    pub url: String,
    /// Pool size. Default `10`.
    #[serde(default)]
    pub max_connections: Option<u32>,
    /// Acquire timeout in seconds. Default `30`. Set above 30 so
    /// testcontainer-based test fixtures do not race.
    #[serde(default)]
    pub acquire_timeout_secs: Option<u64>,
}

/// Reserved for Phase 3 (bind address, ports, TLS).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {}

/// Reserved for Phase 3 (API keys, claims).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {}

/// Errors raised while locating, reading, or parsing `vestige.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("config dir: could not locate user home")]
    NoHome,
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl VestigeConfig {
    /// Load config from `path` or from `default_path()` when `None`.
    ///
    /// Returns `VestigeConfig::default()` (SQLite + fastembed defaults) when
    /// the file does not exist. Any other I/O or parse failure is surfaced
    /// as a `ConfigError`.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let resolved: PathBuf = match path {
            Some(p) => p.to_path_buf(),
            None => Self::default_path()?,
        };

        match std::fs::read_to_string(&resolved) {
            Ok(text) => {
                let cfg: VestigeConfig = toml::from_str(&text)?;
                cfg.validate()?;
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// `~/.vestige/vestige.toml`. The directory is NOT created here; loading
    /// a missing file falls back to defaults.
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let dirs = directories::ProjectDirs::from("", "vestige", "vestige")
            .ok_or(ConfigError::NoHome)?;
        // ProjectDirs::config_dir() varies per OS. Vestige convention is
        // ~/.vestige/vestige.toml on Linux/macOS regardless of XDG, so we
        // build the path off the home dir explicitly.
        let home = directories::UserDirs::new()
            .ok_or(ConfigError::NoHome)?
            .home_dir()
            .to_path_buf();
        let _ = dirs; // keep the dep wired; future Phase 3 may use it
        Ok(home.join(".vestige").join("vestige.toml"))
    }

    /// Light cross-field validation. Heavy validation (URL parsing,
    /// directory existence) is left to the backend constructors.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.embeddings.provider.is_empty() {
            return Err(ConfigError::Invalid(
                "embeddings.provider must not be empty".into(),
            ));
        }
        if self.embeddings.model.is_empty() {
            return Err(ConfigError::Invalid(
                "embeddings.model must not be empty".into(),
            ));
        }
        match &self.storage {
            StorageConfig::Sqlite(_) => {}
            #[cfg(feature = "postgres-backend")]
            StorageConfig::Postgres(cfg) => {
                if cfg.url.is_empty() {
                    return Err(ConfigError::Invalid(
                        "storage.postgres.url must not be empty".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
```

### Serde behaviour with `postgres-backend` off

`StorageConfig` is generated by serde only for the variants that are
compiled in. When `postgres-backend` is off and the user writes:

```toml
[storage]
backend = "postgres"

[storage.postgres]
url = "..."
```

serde returns a `toml::de::Error` of the form
`unknown variant `postgres`, expected `sqlite``. That error path goes
through `From<toml::de::Error> for ConfigError`, surfacing as
`ConfigError::Toml(..)`. The MCP server prints this once at startup and
exits with a non-zero code; there is no panic.

To make the error friendlier we wrap that specific case in a clearer
message via a thin post-parse check. Add this small helper after parsing
in `load()`:

```rust
// (Inside the Ok(text) arm in load(), wrapping the parse step.)
let cfg: VestigeConfig = match toml::from_str(&text) {
    Ok(c) => c,
    Err(e) => {
        let msg = e.to_string();
        if msg.contains("unknown variant `postgres`") {
            return Err(ConfigError::Invalid(
                "storage.backend = \"postgres\" requires building with --features postgres-backend".into(),
            ));
        }
        return Err(ConfigError::Toml(e));
    }
};
```

This keeps the strict default deny_unknown_fields behaviour while giving the
user a one-line action item.

---

## Pool module

**File**: `crates/vestige-core/src/storage/postgres/pool.rs` (new).

```rust
#![cfg(feature = "postgres-backend")]

//! `PgPool` construction for the Postgres backend.
//!
//! Pool defaults follow ADR 0002 D2 + master plan D3:
//! - max_connections = 10
//! - acquire_timeout = 30s (must exceed testcontainer warmup)
//! - idle_timeout = 600s
//! - max_lifetime = 1800s
//! - test_before_acquire = false (cheap queries; saves a roundtrip)

use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};

use crate::config::PostgresConfig;
use crate::storage::memory_store::{MemoryStoreError, MemoryStoreResult};

const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const IDLE_TIMEOUT_SECS: u64 = 600;
const MAX_LIFETIME_SECS: u64 = 1800;
const STATEMENT_CACHE_CAPACITY: usize = 256;

/// Build a Postgres connection pool from a `PostgresConfig`. Does NOT run
/// migrations or stamp the embedding registry; those are the caller's job
/// (`PgMemoryStore::connect`).
pub async fn build_pool(cfg: &PostgresConfig) -> MemoryStoreResult<PgPool> {
    let opts = PgConnectOptions::from_str(&cfg.url)
        .map_err(MemoryStoreError::from)?
        .application_name("vestige")
        .statement_cache_capacity(STATEMENT_CACHE_CAPACITY)
        .log_statements(tracing::log::LevelFilter::Debug);

    let max_conn = cfg.max_connections.unwrap_or(DEFAULT_MAX_CONNECTIONS);
    let acquire = cfg
        .acquire_timeout_secs
        .unwrap_or(DEFAULT_ACQUIRE_TIMEOUT_SECS);

    let pool = PgPoolOptions::new()
        .max_connections(max_conn)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(acquire))
        .idle_timeout(Some(Duration::from_secs(IDLE_TIMEOUT_SECS)))
        .max_lifetime(Some(Duration::from_secs(MAX_LIFETIME_SECS)))
        .test_before_acquire(false)
        .connect_with(opts)
        .await
        .map_err(MemoryStoreError::from)?;

    Ok(pool)
}
```

### Wiring into `PgMemoryStore::connect`

In `crates/vestige-core/src/storage/postgres/mod.rs`, replace the
`todo!()` body left by `0002a` for `connect` and `from_pool` with:

```rust
// In crates/vestige-core/src/storage/postgres/mod.rs

use sqlx::PgPool;

use crate::config::PostgresConfig;
use crate::storage::memory_store::{MemoryStoreError, MemoryStoreResult};

mod pool;
mod registry; // see "Registry stub" section below

pub struct PgMemoryStore {
    pool: PgPool,
}

impl PgMemoryStore {
    /// Convenience constructor matching `SqliteMemoryStore::new` shape.
    /// Takes a URL + pool size for the common case.
    pub async fn connect(url: &str, max_connections: u32) -> MemoryStoreResult<Self> {
        let cfg = PostgresConfig {
            url: url.to_string(),
            max_connections: Some(max_connections),
            acquire_timeout_secs: None,
        };
        Self::connect_with(&cfg).await
    }

    /// Full-config constructor.
    pub async fn connect_with(cfg: &PostgresConfig) -> MemoryStoreResult<Self> {
        let pool = pool::build_pool(cfg).await?;
        Self::from_pool(pool).await
    }

    /// Construct from an already-built pool (used by tests and the migrate
    /// CLI to share a pool across operations).
    pub async fn from_pool(pool: PgPool) -> MemoryStoreResult<Self> {
        // Migrations are added by 0002c.
        // todo!("run sqlx::migrate! once 0002c lands")
        registry::ensure_registry_stub(&pool).await?;
        Ok(Self { pool })
    }
}
```

`connect_with` is the long-lived API; `connect` becomes a thin shim that
stays compatible with the master-plan-mandated signature.

### Registry stub

**File**: `crates/vestige-core/src/storage/postgres/registry.rs` (new, stub).

```rust
#![cfg(feature = "postgres-backend")]

//! Embedding registry. Real body lands in sub-plan 0002d.

use sqlx::PgPool;

use crate::storage::memory_store::MemoryStoreResult;

/// Placeholder. Real implementation in 0002d reads/writes `embedding_model`
/// and stamps `ALTER TABLE memories ALTER COLUMN embedding TYPE vector($N)`.
pub(crate) async fn ensure_registry_stub(_pool: &PgPool) -> MemoryStoreResult<()> {
    // Intentionally a no-op until 0002c lands the table + 0002d lands the
    // real body. Leaving this as todo!() would crash the MCP server at
    // startup the moment a user switches `backend = "postgres"`, which is
    // not what we want for the build verification step in this sub-plan.
    Ok(())
}
```

The no-op keeps `cargo build --features postgres-backend` not just
compiling but also allowing the MCP server to *boot* against a Postgres
URL pointing at an already-migrated database (the local-dev-postgres-setup
docs cover bringing up such a DB by hand). Real init lands in `0002d`.

---

## Error variants

**File**: `crates/vestige-core/src/storage/memory_store.rs` (edit).

The Phase 1 enum `MemoryStoreError` gains two feature-gated variants. These
were deferred in `0002a` and become required as soon as `pool.rs` calls
`.map_err(MemoryStoreError::from)` on `sqlx::Error`.

```rust
// Within enum MemoryStoreError { ... } in memory_store.rs

#[cfg(feature = "postgres-backend")]
#[error("postgres error: {0}")]
Postgres(#[from] sqlx::Error),

#[cfg(feature = "postgres-backend")]
#[error("postgres migration error: {0}")]
Migrate(#[from] sqlx::migrate::MigrateError),
```

Both use thiserror's `#[from]` attribute so the `?` operator works in
`pool.rs`, the migrate module (`0002c`), and registry code (`0002d`).
Default-features build (no `postgres-backend`) sees neither variant; the
enum stays exhaustive on stable.

If clippy fires on `non_exhaustive` due to the gated variants, add
`#[non_exhaustive]` on the enum. That has no caller-side effect since the
enum is constructed only inside the crate.

---

## vestige-mcp wiring

### Cargo feature passthrough

**File**: `crates/vestige-mcp/Cargo.toml` (edit).

Add a feature that forwards through to `vestige-core`. Default features in
`vestige-mcp` stay unchanged.

```toml
[features]
default = ["embeddings", "vector-search"]
embeddings = ["vestige-core/embeddings"]
vector-search = ["vestige-core/vector-search"]
postgres-backend = ["vestige-core/postgres-backend"]
```

Verify with:

```bash
cargo build -p vestige-mcp --features postgres-backend
```

### Backend dispatch at startup

**File**: `crates/vestige-mcp/src/main.rs` (edit around the existing
`Storage::new(storage_path)` call -- see audit note above; in the current
worktree this is around line 285).

The current code is roughly:

```rust
let storage_path = match prepare_storage_path(config.data_dir) { ... };
let storage = match Storage::new(storage_path) { ... };
```

Replace that with a dispatch driven by `VestigeConfig`:

```rust
use std::sync::Arc;

use vestige_core::config::{StorageConfig, VestigeConfig};
use vestige_core::storage::SqliteMemoryStore;
#[cfg(feature = "postgres-backend")]
use vestige_core::storage::postgres::PgMemoryStore;
use vestige_core::storage::MemoryStore;

// Earlier: still call prepare_storage_path to honour --data-dir override.
let storage_path = match prepare_storage_path(config.data_dir.clone()) { ... };

// New: load vestige.toml (or fall back to defaults).
let vestige_cfg = match VestigeConfig::load(config.config_path.as_deref()) {
    Ok(c) => c,
    Err(e) => {
        eprintln!("vestige: failed to load config: {e}");
        std::process::exit(2);
    }
};

let storage: Arc<dyn MemoryStore> = match &vestige_cfg.storage {
    StorageConfig::Sqlite(sqlite_cfg) => {
        // CLI flag --data-dir wins over the config file path.
        let path = storage_path.clone().or_else(|| sqlite_cfg.path.clone());
        let s = SqliteMemoryStore::new(path).unwrap_or_else(|e| {
            eprintln!("vestige: sqlite init failed: {e}");
            std::process::exit(3);
        });
        Arc::new(s)
    }
    #[cfg(feature = "postgres-backend")]
    StorageConfig::Postgres(pg_cfg) => {
        let s = PgMemoryStore::connect_with(pg_cfg).await.unwrap_or_else(|e| {
            eprintln!("vestige: postgres init failed: {e}");
            std::process::exit(3);
        });
        Arc::new(s)
    }
};
```

The `config_path: Option<PathBuf>` field on the local `Config` (or
clap-derived `Args`) struct must be added if not present; it accepts
`--config <path>`. Default behaviour (no flag) goes through
`VestigeConfig::default_path()`.

If the existing main wires `Storage` through a concrete type rather than
`Arc<dyn MemoryStore>`, the dispatch above lives behind a helper:

```rust
async fn build_store(cfg: &VestigeConfig, cli_path: Option<PathBuf>)
    -> Result<Arc<dyn MemoryStore>, anyhow::Error>
{ ... }
```

and the caller chains `.into()` as needed. Phase 1 already moved
cognitive modules to `Arc<dyn MemoryStore>` so this should be a pure
substitution; if a concrete-type holdout is found, fix it locally in this
sub-plan (separate commit) rather than punting.

---

## vestige.toml example

The canonical example to ship in `docs/` (Phase 2 docs land in `0002i`,
runbook), shown here for reference and used verbatim by the unit test
below.

```toml
# vestige.toml -- top-level configuration
#
# Default location: ~/.vestige/vestige.toml
# Override: vestige-mcp --config /path/to/vestige.toml

[embeddings]
provider = "fastembed"
model    = "nomic-ai/nomic-embed-text-v1.5"

# --- SQLite backend (default) ---
[storage]
backend = "sqlite"

[storage.sqlite]
path = "/home/user/.vestige/vestige.db"

# --- Postgres backend (requires --features postgres-backend) ---
# [storage]
# backend = "postgres"
#
# [storage.postgres]
# url                  = "postgres://vestige:secret@localhost:5432/vestige"
# max_connections      = 10
# acquire_timeout_secs = 30

[server]
# Reserved for Phase 3 (bind address, ports, TLS).

[auth]
# Reserved for Phase 3 (API keys, claims).
```

---

## Verification

Run all of these from the repo root. The first three are the gates that
must pass before this sub-plan is considered done.

### 1. Default build (no Postgres)

```bash
cargo build -p vestige-core
cargo build -p vestige-mcp
cargo test  -p vestige-core --lib
```

Expected: clean build. `VestigeConfig::default()` selects SQLite; the MCP
server boots the same way it did pre-sub-plan.

### 2. Postgres-feature build

```bash
cargo build -p vestige-core --features postgres-backend
cargo build -p vestige-mcp  --features postgres-backend
```

Expected: clean build. `PgMemoryStore::connect_with` resolves to
`pool::build_pool` + `registry::ensure_registry_stub`; no `todo!()` is
reachable on the build path. `connect` and `from_pool` are exported.

### 3. Clippy across both feature sets

```bash
cargo clippy -p vestige-core -- -D warnings
cargo clippy -p vestige-core --features postgres-backend -- -D warnings
cargo clippy -p vestige-mcp  --features postgres-backend -- -D warnings
```

### 4. Unit test: round-trip the example

Add this test to `crates/vestige-core/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_SQLITE: &str = r#"
[embeddings]
provider = "fastembed"
model    = "nomic-ai/nomic-embed-text-v1.5"

[storage]
backend = "sqlite"

[storage.sqlite]
path = "/home/user/.vestige/vestige.db"
"#;

    #[cfg(feature = "postgres-backend")]
    const EXAMPLE_POSTGRES: &str = r#"
[embeddings]
provider = "fastembed"
model    = "nomic-ai/nomic-embed-text-v1.5"

[storage]
backend = "postgres"

[storage.postgres]
url                  = "postgres://vestige:secret@localhost:5432/vestige"
max_connections      = 10
acquire_timeout_secs = 30
"#;

    #[test]
    fn parses_sqlite_example() {
        let cfg: VestigeConfig = toml::from_str(EXAMPLE_SQLITE).expect("parse");
        match cfg.storage {
            StorageConfig::Sqlite(s) => assert!(s.path.is_some()),
            #[cfg(feature = "postgres-backend")]
            StorageConfig::Postgres(_) => panic!("wrong variant"),
        }
        assert_eq!(cfg.embeddings.provider, "fastembed");
    }

    #[cfg(feature = "postgres-backend")]
    #[test]
    fn parses_postgres_example() {
        let cfg: VestigeConfig = toml::from_str(EXAMPLE_POSTGRES).expect("parse");
        match cfg.storage {
            StorageConfig::Postgres(p) => {
                assert_eq!(p.url, "postgres://vestige:secret@localhost:5432/vestige");
                assert_eq!(p.max_connections, Some(10));
                assert_eq!(p.acquire_timeout_secs, Some(30));
            }
            StorageConfig::Sqlite(_) => panic!("wrong variant"),
        }
    }

    #[cfg(not(feature = "postgres-backend"))]
    #[test]
    fn rejects_postgres_when_feature_off() {
        let toml_text = r#"
[storage]
backend = "postgres"

[storage.postgres]
url = "postgres://x/y"
"#;
        let res: Result<VestigeConfig, _> = toml::from_str(toml_text);
        assert!(res.is_err(), "must fail without postgres-backend feature");
    }

    #[test]
    fn defaults_pick_sqlite() {
        let cfg = VestigeConfig::default();
        assert!(matches!(cfg.storage, StorageConfig::Sqlite(_)));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let tmp = std::env::temp_dir().join("vestige-no-such-file.toml");
        let _ = std::fs::remove_file(&tmp);
        let cfg = VestigeConfig::load(Some(&tmp)).expect("missing file is OK");
        assert!(matches!(cfg.storage, StorageConfig::Sqlite(_)));
    }

    #[test]
    fn load_roundtrip_from_disk() {
        let tmp = std::env::temp_dir().join("vestige-roundtrip.toml");
        std::fs::write(&tmp, EXAMPLE_SQLITE).unwrap();
        let cfg = VestigeConfig::load(Some(&tmp)).expect("load");
        assert!(matches!(cfg.storage, StorageConfig::Sqlite(_)));
        let _ = std::fs::remove_file(&tmp);
    }
}
```

Run:

```bash
cargo test -p vestige-core --lib config::
cargo test -p vestige-core --lib config:: --features postgres-backend
```

### 5. Smoke: server boots with default config

```bash
# default build, no vestige.toml on disk
cargo run -p vestige-mcp -- --help
# should print help, no panic
```

---

## Acceptance criteria

- [ ] `cargo build -p vestige-core` (default features) succeeds.
- [ ] `cargo build -p vestige-core --features postgres-backend` succeeds.
- [ ] `cargo build -p vestige-mcp` (default features) succeeds.
- [ ] `cargo build -p vestige-mcp --features postgres-backend` succeeds.
- [ ] `cargo clippy` with and without `postgres-backend` is clean on both
      crates.
- [ ] `crates/vestige-core/src/config.rs` exists, exposes
      `VestigeConfig`, `StorageConfig`, `SqliteConfig`, `EmbeddingsConfig`,
      `ConfigError`, plus `PostgresConfig` when the feature is on.
- [ ] `VestigeConfig::load(None)` returns `Ok(default)` when
      `~/.vestige/vestige.toml` is missing.
- [ ] `VestigeConfig::load(Some(&path))` round-trips both the SQLite and
      Postgres example blocks above.
- [ ] With `postgres-backend` off, parsing `backend = "postgres"` returns
      a clear `ConfigError::Invalid` mentioning the feature flag, NOT a
      panic.
- [ ] `crates/vestige-core/src/storage/postgres/pool.rs` exists,
      implementing `build_pool(&PostgresConfig) -> MemoryStoreResult<PgPool>`
      with the documented defaults.
- [ ] `PgMemoryStore::connect`, `connect_with`, and `from_pool` all wire
      through `pool::build_pool`. None of them is `todo!()`. The registry
      step is a no-op stub documented as filled in by `0002d`.
- [ ] `MemoryStoreError::Postgres(sqlx::Error)` and
      `MemoryStoreError::Migrate(sqlx::migrate::MigrateError)` exist
      behind `#[cfg(feature = "postgres-backend")]` with `#[from]`.
- [ ] `vestige-mcp` has a `postgres-backend` feature that forwards to
      `vestige-core/postgres-backend`.
- [ ] `vestige-mcp/src/main.rs` selects SQLite vs Postgres at startup
      based on `VestigeConfig`. SQLite is the default when no config file
      is present.
- [ ] Unit tests in the "Verification" section pass on both feature sets.

---

## Out of scope (handled by other sub-plans)

- Migrations (`crates/vestige-core/migrations/postgres/*.sql`) -- `0002c`.
- Real `PgMemoryStore` CRUD/search/scheduling/edges bodies -- `0002d`,
  `0002e`.
- `ensure_registry` real body with `ALTER COLUMN TYPE vector(N)` -- `0002d`.
- `vestige migrate --from sqlite --to postgres` CLI -- `0002f`.
- Re-embed flow -- `0002g`.
- Env-var override (`VESTIGE_POSTGRES_URL`, etc.) -- Phase 3.
- RLS, multi-tenant column population -- Phase 3.
