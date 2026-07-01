# Phase 3 Plan: Network Access and Authentication

**Status**: Draft
**Depends on**: Phase 1 (MemoryStore trait), Phase 2 (PgMemoryStore, backend config)
**Related**: docs/adr/0001-pluggable-storage-and-network-access.md (Phase 3)

---

## Scope

### In scope

- HTTP MCP Streamable endpoint at `POST /mcp` (JSON-RPC body, keep existing
  session semantics) and `GET /mcp` (Server-Sent Events for long-running
  operations: dream, consolidate, discover, reassign).
- REST API under `/api/v1/` for direct HTTP clients that do not speak MCP
  (memories CRUD, search, consolidate trigger, stats, domains
  list/rename/merge/discover).
- `api_keys` table + enforcement (blake3-hashed, scopes `read`/`write`, optional
  `domain_filter` TEXT[], `last_used` timestamp, `active` flag, revocation).
- Auth middleware with three resolution paths in priority order:
  `Authorization: Bearer <key>` then `X-API-Key: <key>` then signed session
  cookie. All three resolve to the same `ApiKeyIdentity`.
- Signed session cookie: `vestige_session`, SameSite=Strict, HttpOnly,
  Secure-when-TLS, Path=/, Max-Age 8 hours. Signed with HMAC-SHA256 using a
  key derived from `VESTIGE_SESSION_SECRET` (env) or generated + persisted to
  `<data_dir>/session_secret` on first boot.
- `vestige keys create|list|revoke` CLI subcommand (plus `keys rotate` as a
  convenience alias of `revoke` + `create`).
- Startup-time refusal to bind non-loopback with `auth.enabled = false` (hard
  error, non-zero exit, stderr message, no fallback).
- Dashboard login flow: `POST /dashboard/login` with `{"api_key":"vst_..."}`
  JSON body, `X-API-Key` header, or form body; sets signed cookie; returns 200
  JSON `{"ok":true}` for XHR or 303 to `/` if form. Logout at
  `POST /dashboard/logout` clears cookie.
- Per-key `domain_filter` enforced inside the auth layer: if the key has
  `domain_filter = ["dev","infra"]`, every handler that searches or lists sees
  the filter pre-applied via a request extension. Optional
  `X-Vestige-Domain: home` header may narrow further but may never escape the
  key's filter.
- `[server]` and `[auth]` sections in `vestige.toml`, plus backward-compatible
  env var bridges.
- `VESTIGE_AUTH_TOKEN` continues to work for one minor release as a synthetic
  single-key fallback, but logs a deprecation warning.
- Per-request request IDs and structured tracing; `last_used` write-back on
  successful auth.

### Out of scope

- Phase 4 HDBSCAN domain classifier itself. The REST surface exposes domain
  endpoints but they may stub to empty results until Phase 4 lands.
- Real TLS termination. Assumed handled by a reverse proxy (nginx, Caddy,
  Mycelium). An optional `tls_cert` / `tls_key` pair is documented but its
  implementation may be deferred behind a `tls` Cargo feature.
- OAuth / OIDC / SSO. Future work.
- Rate limiting per key (documented in Open Questions, not implemented here).
- WebAuthn / passkey dashboard login. Future work.
- Fine-grained RBAC beyond `read` / `write` scopes.

## Prerequisites

Phase 1 artifacts:

- `vestige_core::storage::MemoryStore` trait (with `Send` variant via
  `trait_variant::make`).
- `Embedder` trait.
- `SqliteMemoryStore` implementing `MemoryStore`.

Phase 2 artifacts:

- `PgMemoryStore` implementing `MemoryStore`.
- `crates/vestige-core/migrations/postgres/` sqlx migrations; `api_keys` table
  schema present but enforcement path is Phase 3's job.
- Runtime backend selection via `vestige.toml` `[storage]` section returning
  an `Arc<dyn MemoryStore>`.

Assumed already available in workspace:

- `axum = 0.8` (currently pinned in `crates/vestige-mcp/Cargo.toml`).
- `tower = 0.5`, `tower-http = 0.6` (`cors`, `set-header` features already on).
- `tokio`, `serde`, `serde_json`, `uuid`, `chrono`, `tracing`,
  `tracing-subscriber`, `thiserror`, `anyhow`, `subtle`, `clap`, `directories`.

New crates required (add via `cargo add -p vestige-mcp`):

- `blake3 = "1"` -- key hashing.
- `rand = "0.9"` with `std_rng` (for key bytes; prefer `rand::rngs::OsRng`).
- `axum-extra = { version = "0.10", features = ["cookie-signed", "typed-header"] }`
  -- `SignedCookieJar`, `Cookie`, `Key`.
- `hmac = "0.12"` + `sha2 = "0.10"` -- HMAC-SHA256 for the session secret
  derivation (not required if `axum-extra`'s `SignedCookieJar` is used, but
  retained for the pure-token-signing path). RECOMMENDATION: rely solely on
  `axum-extra::extract::cookie::{Key, SignedCookieJar}`.
- `tower-http` features bump: add `trace` and `request-id`.
- `async-stream = "0.3"` -- emitting SSE events from async closures.
- `futures-util` already present -- for `Stream` adapters.
- `base64 = "0.22"` -- emitting / parsing the random bytes in the `vst_...`
  prefix. Use the `URL_SAFE_NO_PAD` alphabet.
- `zeroize = "1"` (optional, recommended) -- scrub the plaintext key in RAM
  after hashing.

`cargo add` commands (do not execute here, leave to implementation):

    cargo add -p vestige-mcp blake3 rand base64 zeroize async-stream
    cargo add -p vestige-mcp axum-extra --features cookie-signed,typed-header
    cargo add -p vestige-mcp tower-http --features trace,request-id,cors,set-header

JSON-RPC library: the project uses a hand-rolled `JsonRpcRequest` /
`JsonRpcResponse` pair in `crates/vestige-mcp/src/protocol/types.rs`. Keep it
in Phase 3 (no jsonrpsee migration). Streamable HTTP remains implemented as
`POST /mcp` + session header + `GET /mcp` SSE. See Open Questions for rationale.

## Deliverables

1. `crates/vestige-mcp/src/auth/` module (new). Houses key generation, key
   verification, identity resolution, scopes, domain-filter extractor, session
   key type, and error types.

2. `crates/vestige-mcp/src/auth/keys.rs` -- key format, generation,
   blake3 hashing, store-facing trait methods for list / create / revoke /
   verify.

3. `crates/vestige-mcp/src/auth/middleware.rs` -- axum `from_fn` middleware
   that populates `Extension<Identity>` on the request, rejects unauthenticated
   requests with 401, insufficient scope with 403.

4. `crates/vestige-mcp/src/auth/session.rs` -- `SignedCookieJar` integration,
   `session_key()` loader (env or persisted file), `issue_session()` and
   `revoke_session()` helpers.

5. `crates/vestige-mcp/src/http/` module split out of `protocol/http.rs`:
   - `http/mcp.rs` -- MCP JSON-RPC endpoint (adapted from the current
     `post_mcp` / `delete_mcp`, with auth middleware now gating).
   - `http/mcp_sse.rs` -- SSE handler for `GET /mcp` long-running ops.
   - `http/rest.rs` -- `/api/v1/*` handlers.
   - `http/mod.rs` -- `build_router()`, `start_server()`, bind-safety check,
     layer stack assembly.

6. `crates/vestige-mcp/src/http/errors.rs` -- uniform `ApiError` enum and
   `IntoResponse` implementation. Maps to RFC 7807 problem+json for REST and
   plain JSON for `/mcp`.

7. Dashboard patch: `crates/vestige-mcp/src/dashboard/mod.rs` -- add the auth
   middleware to the dashboard router, add `/dashboard/login` + `/dashboard/logout`
   endpoints, keep `/api/health` unauthenticated.

8. `crates/vestige-mcp/src/bin/cli.rs` -- new `Keys` subcommand group (`create`,
   `list`, `revoke`, `rotate`).

9. `crates/vestige-mcp/src/config.rs` (new file) -- typed `ServerConfig`,
   `AuthConfig`, `StorageConfig` loader from `vestige.toml`, merging env var
   overrides, validating the non-loopback + auth-disabled combination.

10. SQL migration `crates/vestige-core/migrations/postgres/0300_api_keys_enforcement.sql`
    and SQLite equivalent `crates/vestige-core/migrations/sqlite/0300_api_keys.sql`:
    - `api_keys` table (if not already created in Phase 2), with `key_hash`
      UNIQUE, `label` NOT NULL, `scopes` TEXT[] default `{read,write}`,
      `domain_filter` TEXT[] default `{}`, `created_at`, `last_used`,
      `active BOOLEAN DEFAULT true`.
    - Index on `key_hash` (unique already), and on `active WHERE active`.

11. `MemoryStore` trait extension (Phase 2 may already cover this; if not,
    finalize in Phase 3): `list_api_keys`, `create_api_key`,
    `revoke_api_key`, `find_api_key_by_hash`, `touch_api_key_last_used`.

12. Docs updates:
    - `docs/env-vars.md` (new) -- one sheet for all runtime env vars.
    - `README.md` server-mode section.
    - `docs/adr/0001-*.md` -- mark Phase 3 as Implemented when merged.

## Detailed Task Breakdown

### D1. Auth module skeleton

Files:

- `crates/vestige-mcp/src/auth/mod.rs`
- `crates/vestige-mcp/src/auth/keys.rs`
- `crates/vestige-mcp/src/auth/session.rs`
- `crates/vestige-mcp/src/auth/middleware.rs`
- `crates/vestige-mcp/src/auth/errors.rs`

`auth/mod.rs`:

    pub mod errors;
    pub mod keys;
    pub mod middleware;
    pub mod session;

    pub use errors::AuthError;
    pub use keys::{ApiKey, ApiKeyPlaintext, ApiKeyRecord, Scope};
    pub use middleware::{Identity, auth_layer};
    pub use session::{SessionConfig, session_key};

`auth/errors.rs`:

    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use serde::Serialize;
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum AuthError {
        #[error("missing credentials")]
        MissingCredentials,
        #[error("invalid credentials")]
        InvalidCredentials,
        #[error("key revoked")]
        Revoked,
        #[error("insufficient scope: required {required}")]
        InsufficientScope { required: &'static str },
        #[error("domain not permitted for this key: {domain}")]
        DomainNotAllowed { domain: String },
        #[error("internal auth error")]
        Internal,
    }

    #[derive(Serialize)]
    struct Problem<'a> {
        #[serde(rename = "type")]
        kind: &'a str,
        title: &'a str,
        status: u16,
        detail: &'a str,
    }

    impl IntoResponse for AuthError {
        fn into_response(self) -> Response {
            let (status, title) = match self {
                AuthError::MissingCredentials => (StatusCode::UNAUTHORIZED, "unauthorized"),
                AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "unauthorized"),
                AuthError::Revoked => (StatusCode::UNAUTHORIZED, "unauthorized"),
                AuthError::InsufficientScope { .. } => (StatusCode::FORBIDDEN, "forbidden"),
                AuthError::DomainNotAllowed { .. } => (StatusCode::FORBIDDEN, "forbidden"),
                AuthError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
            };
            let detail = self.to_string();
            let body = axum::Json(Problem {
                kind: "about:blank",
                title,
                status: status.as_u16(),
                detail: &detail,
            });
            let mut r = (status, body).into_response();
            r.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/problem+json"),
            );
            r
        }
    }

### D2. Key format and generation

File: `crates/vestige-mcp/src/auth/keys.rs`

- Key on wire: `vst_<22-byte base64url-no-pad>`. 22 bytes = 176 bits entropy.
  Encoded length ~30 chars. Full string ~34 chars including the `vst_` prefix.
- Hash stored in DB: `blake3(key_plaintext)` hex lowercase (32 bytes -> 64
  hex chars).
- Hash prefix on list: first 12 hex characters, e.g. `key_hash[..12]` for
  human display.

Signatures:

    use blake3::Hasher;
    use rand::rngs::OsRng;
    use rand::TryRngCore;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use zeroize::Zeroize;

    const KEY_PREFIX: &str = "vst_";
    const KEY_RANDOM_BYTES: usize = 22;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Scope {
        Read,
        Write,
    }

    impl Scope {
        pub fn as_str(&self) -> &'static str {
            match self {
                Scope::Read => "read",
                Scope::Write => "write",
            }
        }
        pub fn from_str(s: &str) -> Option<Self> {
            match s {
                "read" => Some(Scope::Read),
                "write" => Some(Scope::Write),
                _ => None,
            }
        }
    }

    /// The plaintext key. Shown to the user exactly once.
    /// Zeroed on drop.
    pub struct ApiKeyPlaintext(String);

    impl ApiKeyPlaintext {
        pub fn as_str(&self) -> &str { &self.0 }
        pub fn into_inner(mut self) -> String {
            std::mem::take(&mut self.0)
        }
    }

    impl Drop for ApiKeyPlaintext {
        fn drop(&mut self) { self.0.zeroize(); }
    }

    #[derive(Clone, Debug)]
    pub struct ApiKeyRecord {
        pub id: uuid::Uuid,
        pub key_hash: String,          // hex-encoded blake3(plaintext)
        pub label: String,
        pub scopes: Vec<Scope>,
        pub domain_filter: Vec<String>,
        pub created_at: chrono::DateTime<chrono::Utc>,
        pub last_used: Option<chrono::DateTime<chrono::Utc>>,
        pub active: bool,
    }

    pub fn generate_key() -> ApiKeyPlaintext {
        let mut bytes = [0u8; KEY_RANDOM_BYTES];
        OsRng.try_fill_bytes(&mut bytes).expect("OsRng");
        let encoded = URL_SAFE_NO_PAD.encode(&bytes);
        bytes.zeroize();
        ApiKeyPlaintext(format!("{}{}", KEY_PREFIX, encoded))
    }

    pub fn hash_key(plaintext: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(plaintext.as_bytes());
        hasher.finalize().to_hex().to_string()
    }

    pub fn verify_key(plaintext: &str, stored_hash_hex: &str) -> bool {
        use subtle::ConstantTimeEq;
        let computed = hash_key(plaintext);
        computed.as_bytes().ct_eq(stored_hash_hex.as_bytes()).unwrap_u8() == 1
    }

Helpers on a thin repository trait that both backends implement through
`MemoryStore` (Phase 2 already adds the required columns; Phase 3 wires the
methods):

    #[async_trait::async_trait]
    pub trait ApiKeyStore: Send + Sync + 'static {
        async fn create_api_key(&self, rec: &ApiKeyRecord) -> anyhow::Result<()>;
        async fn find_api_key_by_hash(&self, hash: &str) -> anyhow::Result<Option<ApiKeyRecord>>;
        async fn list_api_keys(&self) -> anyhow::Result<Vec<ApiKeyRecord>>;
        async fn revoke_api_key(&self, id: uuid::Uuid) -> anyhow::Result<bool>;
        async fn touch_api_key_last_used(&self, id: uuid::Uuid) -> anyhow::Result<()>;
    }

(If Phase 2 already bolted these onto `MemoryStore`, `ApiKeyStore` is simply a
re-export of the relevant subset.)

### D3. Session cookie

File: `crates/vestige-mcp/src/auth/session.rs`

- Cookie name: `vestige_session`.
- Cookie attributes: `HttpOnly`, `SameSite=Strict`, `Path=/`, `Max-Age=28800`
  (8h), `Secure` when the server is running behind TLS (detected from
  `config.server.tls_cert.is_some()` or the `X-Forwarded-Proto` trusted header;
  default: set `Secure` whenever `config.server.bind` is non-loopback).
- Payload: serialized `SessionClaims { key_id: Uuid, issued_at: i64,
  expires_at: i64 }` encoded as `serde_json` then base64url. The signing is
  handled by `axum-extra::extract::cookie::SignedCookieJar` (HMAC via a 64-byte
  `Key`). Any tampering or truncation is rejected by the jar automatically.
- Key material: 64 random bytes, stored at `<data_dir>/session_secret` (mode
  0600) or overridden by `VESTIGE_SESSION_SECRET` (base64url-encoded 64 bytes,
  reject if shorter).

Signatures:

    use axum_extra::extract::cookie::{Cookie, Key, SameSite, SignedCookieJar};
    use chrono::{Duration, Utc};
    use serde::{Deserialize, Serialize};

    const COOKIE_NAME: &str = "vestige_session";
    const DEFAULT_TTL: Duration = Duration::hours(8);

    #[derive(Clone, Serialize, Deserialize)]
    pub struct SessionClaims {
        pub key_id: uuid::Uuid,
        pub iat: i64,
        pub exp: i64,
    }

    pub fn session_key(data_dir: &std::path::Path) -> anyhow::Result<Key> {
        // 1) env override
        if let Ok(env_val) = std::env::var("VESTIGE_SESSION_SECRET") {
            let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(env_val.trim())?;
            anyhow::ensure!(raw.len() >= 64, "VESTIGE_SESSION_SECRET must be >= 64 bytes");
            return Ok(Key::from(&raw));
        }
        // 2) persisted file
        let path = data_dir.join("session_secret");
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            return Ok(Key::from(&bytes));
        }
        // 3) generate
        use rand::TryRngCore;
        let mut bytes = [0u8; 64];
        rand::rngs::OsRng.try_fill_bytes(&mut bytes)?;
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::create_dir_all(data_dir).ok();
            let mut f = std::fs::OpenOptions::new()
                .create_new(true).write(true).mode(0o600).open(&path)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        #[cfg(not(unix))]
        std::fs::write(&path, &bytes)?;
        Ok(Key::from(&bytes))
    }

    pub fn issue_session(
        jar: SignedCookieJar,
        key_id: uuid::Uuid,
        secure: bool,
    ) -> SignedCookieJar {
        let now = Utc::now();
        let claims = SessionClaims {
            key_id,
            iat: now.timestamp(),
            exp: (now + DEFAULT_TTL).timestamp(),
        };
        let value = serde_json::to_string(&claims).expect("serialize claims");
        let mut cookie = Cookie::new(COOKIE_NAME, value);
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Strict);
        cookie.set_path("/");
        cookie.set_max_age(cookie::time::Duration::seconds(DEFAULT_TTL.num_seconds()));
        cookie.set_secure(secure);
        jar.add(cookie)
    }

    pub fn revoke_session(jar: SignedCookieJar) -> SignedCookieJar {
        jar.remove(Cookie::from(COOKIE_NAME))
    }

    pub fn claims_from(jar: &SignedCookieJar) -> Option<SessionClaims> {
        let c = jar.get(COOKIE_NAME)?;
        let claims: SessionClaims = serde_json::from_str(c.value()).ok()?;
        if claims.exp < Utc::now().timestamp() { return None; }
        Some(claims)
    }

### D4. Auth middleware

File: `crates/vestige-mcp/src/auth/middleware.rs`

Identity carried through the request:

    #[derive(Clone, Debug)]
    pub struct Identity {
        pub key_id: uuid::Uuid,
        pub label: String,
        pub scopes: Vec<Scope>,
        pub domain_filter: Vec<String>,
        pub via: AuthVia,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum AuthVia {
        Bearer,
        ApiKeyHeader,
        SessionCookie,
    }

Middleware (axum 0.8):

    use axum::extract::{Request, State};
    use axum::http::{header, StatusCode};
    use axum::middleware::Next;
    use axum::response::{IntoResponse, Response};
    use axum_extra::extract::cookie::SignedCookieJar;
    use std::sync::Arc;

    pub async fn auth_layer(
        State(state): State<Arc<AppCtx>>,
        jar: SignedCookieJar,
        mut request: Request,
        next: Next,
    ) -> Response {
        // Allowlist endpoints that never require auth:
        let path = request.uri().path();
        if path == "/api/health" || path == "/api/v1/health" ||
           path == "/dashboard/login" {
            return next.run(request).await;
        }

        let via_and_key = extract_credentials(request.headers(), &jar);
        let outcome = match via_and_key {
            Some((AuthVia::Bearer, key)) | Some((AuthVia::ApiKeyHeader, key)) => {
                resolve_by_plaintext(&state, &key).await.map(|id| (id, via_and_key.unwrap().0))
            }
            Some((AuthVia::SessionCookie, key_id_str)) => {
                let id = uuid::Uuid::parse_str(&key_id_str).map_err(|_| AuthError::InvalidCredentials)?;
                resolve_by_key_id(&state, id).await.map(|id| (id, AuthVia::SessionCookie))
            }
            None => Err(AuthError::MissingCredentials),
        };

        let identity = match outcome {
            Ok((id, via)) => Identity { via, ..id },
            Err(e) => return e.into_response(),
        };

        // touch last_used asynchronously; do not block request path
        let st2 = state.clone();
        let kid = identity.key_id;
        tokio::spawn(async move { let _ = st2.store.touch_api_key_last_used(kid).await; });

        request.extensions_mut().insert(identity);
        next.run(request).await
    }

Credential extraction (priority: Bearer > X-API-Key > cookie):

    fn extract_credentials(
        headers: &axum::http::HeaderMap,
        jar: &SignedCookieJar,
    ) -> Option<(AuthVia, String)> {
        if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
            if let Some(rest) = v.strip_prefix("Bearer ") {
                return Some((AuthVia::Bearer, rest.trim().to_string()));
            }
        }
        if let Some(v) = headers.get("x-api-key").and_then(|h| h.to_str().ok()) {
            return Some((AuthVia::ApiKeyHeader, v.trim().to_string()));
        }
        if let Some(claims) = crate::auth::session::claims_from(jar) {
            return Some((AuthVia::SessionCookie, claims.key_id.to_string()));
        }
        None
    }

Resolution helpers:

    async fn resolve_by_plaintext(st: &AppCtx, key: &str) -> Result<Identity, AuthError> {
        let hash = crate::auth::keys::hash_key(key);
        let rec = st.store.find_api_key_by_hash(&hash).await
            .map_err(|_| AuthError::Internal)?
            .ok_or(AuthError::InvalidCredentials)?;
        if !rec.active { return Err(AuthError::Revoked); }
        Ok(Identity {
            key_id: rec.id, label: rec.label, scopes: rec.scopes,
            domain_filter: rec.domain_filter, via: AuthVia::Bearer,
        })
    }

    async fn resolve_by_key_id(st: &AppCtx, id: uuid::Uuid) -> Result<Identity, AuthError> {
        let rec = st.store.find_api_key_by_id(id).await
            .map_err(|_| AuthError::Internal)?
            .ok_or(AuthError::InvalidCredentials)?;
        if !rec.active { return Err(AuthError::Revoked); }
        Ok(Identity {
            key_id: rec.id, label: rec.label, scopes: rec.scopes,
            domain_filter: rec.domain_filter, via: AuthVia::SessionCookie,
        })
    }

Scope guard extractor (per-handler opt-in):

    pub struct RequireScope<const WRITE: bool>;
    impl<S, const WRITE: bool> axum::extract::FromRequestParts<S> for RequireScope<WRITE>
    where S: Send + Sync,
    {
        type Rejection = AuthError;
        async fn from_request_parts(
            parts: &mut axum::http::request::Parts, _state: &S,
        ) -> Result<Self, Self::Rejection> {
            let id = parts.extensions.get::<Identity>().ok_or(AuthError::MissingCredentials)?;
            let need = if WRITE { Scope::Write } else { Scope::Read };
            if !id.scopes.contains(&need) {
                return Err(AuthError::InsufficientScope {
                    required: if WRITE { "write" } else { "read" },
                });
            }
            Ok(RequireScope)
        }
    }

Domain scoping:

    /// Returns the effective domain filter for the request:
    /// - Intersect the key's domain_filter with any X-Vestige-Domain header.
    /// - Empty key filter means "all domains", so the header is authoritative.
    /// - A header that names a domain outside the key filter returns
    ///   `Err(DomainNotAllowed)`.
    pub fn effective_domain_filter(
        id: &Identity, header: Option<&str>,
    ) -> Result<Option<Vec<String>>, AuthError> {
        let header_dom = header.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        match (id.domain_filter.as_slice(), header_dom) {
            ([], None) => Ok(None),
            ([], Some(h)) => Ok(Some(vec![h])),
            (filter, None) => Ok(Some(filter.to_vec())),
            (filter, Some(h)) => {
                if filter.iter().any(|d| d == &h) {
                    Ok(Some(vec![h]))
                } else {
                    Err(AuthError::DomainNotAllowed { domain: h })
                }
            }
        }
    }

### D5. Layer ordering

Router assembly in `http/mod.rs::build_router`:

    let trace = tower_http::trace::TraceLayer::new_for_http();
    let request_id = tower_http::request_id::SetRequestIdLayer::x_request_id(
        tower_http::request_id::MakeRequestUuid);
    let propagate_id = tower_http::request_id::PropagateRequestIdLayer::x_request_id();

    let cors = CorsLayer::new()
        .allow_origin(cfg.server.allowed_origins())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION,
                        HeaderName::from_static("x-api-key"),
                        HeaderName::from_static("x-vestige-domain"),
                        HeaderName::from_static("mcp-session-id")])
        .allow_credentials(true);

    let app = Router::new()
        // Unauth routes first (not subjected to auth_layer by path allowlist)
        .route("/api/health", get(health))
        .route("/dashboard/login", post(login))
        .route("/dashboard/logout", post(logout))
        // MCP + REST + dashboard
        .route("/mcp", post(http::mcp::post_mcp).get(http::mcp_sse::get_mcp_sse)
                          .delete(http::mcp::delete_mcp))
        .nest("/api/v1", http::rest::router())
        .merge(dashboard::router())
        // Auth middleware applied via from_fn_with_state (allowlist inside)
        .layer(axum::middleware::from_fn_with_state(ctx.clone(), auth_layer))
        // Outermost: tracing, request-id, cors, body limit, concurrency
        .layer(
            ServiceBuilder::new()
                .layer(trace)
                .layer(request_id)
                .layer(propagate_id)
                .layer(cors)
                .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
                .layer(ConcurrencyLimitLayer::new(CONCURRENCY_LIMIT))
        )
        .with_state(ctx);

Axum applies `layer()` calls outermost-first in the order they are declared.
The result here: request -> trace -> request-id -> CORS -> body-limit ->
concurrency -> auth -> handler. Auth must wrap the handlers but be inside
tracing so its spans can log auth outcomes.

### D6. MCP endpoints

File: `crates/vestige-mcp/src/http/mcp.rs`

`POST /mcp` -- keep the session-based structure already in `protocol/http.rs`
but use the `Identity` injected by the auth layer instead of a shared
`auth_token`:

    pub async fn post_mcp(
        State(ctx): State<Arc<AppCtx>>,
        Extension(id): Extension<Identity>,
        headers: HeaderMap,
        Json(request): Json<JsonRpcRequest>,
    ) -> Response { ... }

Auth happens in the layer, so this handler cannot be reached without a valid
`Identity`. Scope check: all MCP writes (tools that mutate) require
`RequireScope<true>`. Use an enum of MCP methods or a method -> required-scope
map. `tools/list`, `resources/list`, `initialize`, `ping` are read-only.
`tools/call` is conservatively classified as write; the per-tool dispatch
inside `McpServer::handle_tools_call` may further reject writes when the tool
name is read-only and the key lacks write.

`DELETE /mcp` -- unchanged semantics, drops the session.

`GET /mcp` -- SSE. Implementation in `http/mcp_sse.rs`:

    use axum::response::sse::{Event, KeepAlive, Sse};
    use axum::extract::Query;
    use futures_util::stream::Stream;
    use async_stream::stream;
    use std::time::Duration;

    #[derive(serde::Deserialize)]
    pub struct SseParams {
        pub op: String,               // "dream" | "consolidate" | "discover" | "reassign"
        pub session: Option<String>,  // optional operation correlation id
    }

    pub async fn get_mcp_sse(
        State(ctx): State<Arc<AppCtx>>,
        Extension(_id): Extension<Identity>,
        Query(params): Query<SseParams>,
    ) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, AuthError> {
        let op = params.op.clone();
        let ctx2 = ctx.clone();
        let s = stream! {
            yield Ok(Event::default().event("start").data(format!("{{\"op\":\"{}\"}}", op)));
            match op.as_str() {
                "dream" => {
                    let mut rx = ctx2.cognitive.lock().await.begin_dream_stream().await;
                    while let Some(ev) = rx.recv().await {
                        yield Ok(Event::default().event("progress").json_data(ev)?);
                    }
                    yield Ok(Event::default().event("done").data("{}"));
                }
                "consolidate" => { /* same pattern over Storage::run_consolidation_stream */ }
                "discover" => { /* Phase 4 */ }
                "reassign" => { /* Phase 4 */ }
                other => {
                    yield Ok(Event::default().event("error")
                        .data(format!("{{\"message\":\"unknown op {}\"}}", other)));
                }
            }
        };
        Ok(Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
    }

SSE event shape (stable contract, document in `docs/http-api.md`):

    event: start
    data: {"op":"dream"}

    event: progress
    data: {"stage":"replay","processed":12,"total":50}

    event: progress
    data: {"stage":"cross_reference","processed":25,"total":50}

    event: done
    data: {"nodes_processed":50,"duration_ms":14320}

The `keep-alive` hint is 15s to survive most proxy timeouts.

### D7. REST API

File: `crates/vestige-mcp/src/http/rest.rs`

Routes:

    pub fn router() -> Router<Arc<AppCtx>> {
        Router::new()
            .route("/health", get(health))
            .route("/memories", post(create_memory).get(list_memories))
            .route("/memories/{id}", get(get_memory).put(update_memory).delete(delete_memory))
            .route("/memories/{id}/promote", post(promote_memory))
            .route("/memories/{id}/demote", post(demote_memory))
            .route("/search", post(search_memories))
            .route("/consolidate", post(trigger_consolidation))
            .route("/stats", get(get_stats))
            .route("/domains", get(list_domains))
            .route("/domains/discover", post(trigger_discovery))
            .route("/domains/{id}", put(rename_domain).delete(delete_domain))
            .route("/domains/{id}/merge", post(merge_domain))
            .route("/keys", post(create_key).get(list_keys))
            .route("/keys/{id}", delete(revoke_key))
    }

Representative signatures:

    #[derive(serde::Deserialize)]
    pub struct CreateMemoryReq {
        pub content: String,
        pub node_type: Option<String>,
        pub tags: Option<Vec<String>>,
        pub source: Option<String>,
        pub metadata: Option<serde_json::Value>,
    }

    #[derive(serde::Serialize)]
    pub struct MemoryView { /* flat projection of MemoryRecord */ }

    pub async fn create_memory(
        State(ctx): State<Arc<AppCtx>>,
        Extension(id): Extension<Identity>,
        _: RequireScope<true>,
        Json(req): Json<CreateMemoryReq>,
    ) -> Result<(StatusCode, Json<MemoryView>), ApiError> {
        let effective = effective_domain_filter(&id, None)?;
        let rec = ctx.store.insert_from_rest(req, effective).await?;
        Ok((StatusCode::CREATED, Json(MemoryView::from(rec))))
    }

    pub async fn search_memories(
        State(ctx): State<Arc<AppCtx>>,
        Extension(id): Extension<Identity>,
        _: RequireScope<false>,
        headers: HeaderMap,
        Json(req): Json<SearchReq>,
    ) -> Result<Json<SearchResp>, ApiError> {
        let dom_header = headers.get("x-vestige-domain").and_then(|h| h.to_str().ok());
        let effective = effective_domain_filter(&id, dom_header)?;
        let q = SearchQuery { domains: effective, ..req.into() };
        let res = ctx.store.search(&q).await?;
        Ok(Json(SearchResp::from(res)))
    }

`trigger_consolidation` returns 202 Accepted + a JSON body with a `session_id`
the client may pass to `GET /mcp?op=consolidate&session=...` to stream
progress.

### D8. Error mapping

File: `crates/vestige-mcp/src/http/errors.rs`

    #[derive(Debug, thiserror::Error)]
    pub enum ApiError {
        #[error(transparent)] Auth(#[from] AuthError),
        #[error("bad request: {0}")] BadRequest(String),
        #[error("not found")] NotFound,
        #[error("conflict: {0}")] Conflict(String),
        #[error(transparent)] Store(#[from] anyhow::Error),
    }

    impl IntoResponse for ApiError {
        fn into_response(self) -> Response {
            match self {
                ApiError::Auth(a) => a.into_response(),
                ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, problem(400, "bad_request", &m)).into_response(),
                ApiError::NotFound => (StatusCode::NOT_FOUND, problem(404, "not_found", "")).into_response(),
                ApiError::Conflict(m) => (StatusCode::CONFLICT, problem(409, "conflict", &m)).into_response(),
                ApiError::Store(e) => {
                    tracing::error!(err = %e, "store error");
                    (StatusCode::INTERNAL_SERVER_ERROR, problem(500, "internal", "internal error")).into_response()
                }
            }
        }
    }

All MCP JSON-RPC error mapping is unchanged (done in `McpServer`); only
transport-level errors (401/403) leave that path.

### D9. Config loader and bind-safety check

File: `crates/vestige-mcp/src/config.rs`

    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct ServerConfig {
        #[serde(default = "default_bind")]
        pub bind: String,                       // "127.0.0.1:3928"
        #[serde(default = "default_dashboard_port")]
        pub dashboard_port: u16,
        #[serde(default)] pub tls_cert: Option<std::path::PathBuf>,
        #[serde(default)] pub tls_key: Option<std::path::PathBuf>,
        #[serde(default)] pub allowed_origins: Vec<String>,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct AuthConfig {
        #[serde(default = "default_true")]
        pub enabled: bool,
        #[serde(default)] pub session_secret_file: Option<std::path::PathBuf>,
    }

    impl ServerConfig {
        pub fn parsed_bind(&self) -> anyhow::Result<std::net::SocketAddr> {
            self.bind.parse().map_err(|e: std::net::AddrParseError|
                anyhow::anyhow!("invalid bind {}: {}", self.bind, e))
        }
    }

Bind-safety check (called during `start_server`):

    pub fn enforce_bind_safety(server: &ServerConfig, auth: &AuthConfig) -> anyhow::Result<()> {
        let addr = server.parsed_bind()?;
        let is_loopback = match addr.ip() {
            std::net::IpAddr::V4(v) => v.is_loopback(),
            std::net::IpAddr::V6(v) => v.is_loopback(),
        };
        if !is_loopback && !auth.enabled {
            anyhow::bail!(
                "refusing to bind {} with auth disabled; \
                 set [auth] enabled = true in vestige.toml or \
                 change [server] bind to a loopback address",
                addr
            );
        }
        Ok(())
    }

`main.rs` and the `serve` CLI both call `enforce_bind_safety` before
`TcpListener::bind`. On failure: `eprintln!` the error, `std::process::exit(2)`.

Env bridge:

- `VESTIGE_HTTP_BIND` (existing) -> `server.bind` host part.
- `VESTIGE_HTTP_PORT` (existing) -> `server.bind` port part.
- `VESTIGE_DASHBOARD_PORT` (existing) -> `server.dashboard_port`.
- `VESTIGE_AUTH_TOKEN` (deprecated) -- when set, synthesize a virtual
  `ApiKeyRecord` with `id = all-zero UUID`, `scopes = [read, write]`,
  `domain_filter = []`, `active = true`, hash stored in memory only. Log a
  warning on every startup: `VESTIGE_AUTH_TOKEN is deprecated; use 'vestige
  keys create' and set auth.enabled=true instead. Will be removed in v2.2.0.`
- `VESTIGE_SESSION_SECRET` -- see D3.

### D10. Dashboard login + logout

File: `crates/vestige-mcp/src/dashboard/handlers.rs` (additions).

    #[derive(serde::Deserialize)]
    pub struct LoginBody {
        pub api_key: String,
    }

    pub async fn login(
        State(state): State<AppState>,
        jar: SignedCookieJar,
        headers: HeaderMap,
        body: Option<Json<LoginBody>>,
    ) -> Result<(SignedCookieJar, Json<serde_json::Value>), AuthError> {
        // Accept key in either JSON body or X-API-Key header
        let plaintext = body.map(|b| b.0.api_key)
            .or_else(|| headers.get("x-api-key").and_then(|h| h.to_str().ok()).map(String::from))
            .ok_or(AuthError::MissingCredentials)?;

        let hash = crate::auth::keys::hash_key(&plaintext);
        let rec = state.store.find_api_key_by_hash(&hash).await
            .map_err(|_| AuthError::Internal)?
            .ok_or(AuthError::InvalidCredentials)?;
        if !rec.active { return Err(AuthError::Revoked); }

        let secure = state.config.server.tls_cert.is_some();
        let jar = crate::auth::session::issue_session(jar, rec.id, secure);

        Ok((jar, Json(serde_json::json!({
            "ok": true, "key_id": rec.id, "label": rec.label,
            "scopes": rec.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "domains": rec.domain_filter,
        }))))
    }

    pub async fn logout(jar: SignedCookieJar)
        -> (SignedCookieJar, Json<serde_json::Value>)
    {
        (crate::auth::session::revoke_session(jar),
         Json(serde_json::json!({"ok": true})))
    }

Dashboard router integration: login/logout are appended before `auth_layer`
is applied, so they are reachable unauthenticated. The dashboard SPA asset
routes (`/dashboard`, `/dashboard/{*path}`) remain publicly readable so the
login page can load; the `/api/*` dashboard endpoints are gated by
`auth_layer`. (The existing health endpoint keeps its current behaviour.)

### D11. `vestige keys` CLI

File: `crates/vestige-mcp/src/bin/cli.rs` additions.

    #[derive(Subcommand)]
    enum Commands {
        // ... existing
        /// Manage API keys
        Keys {
            #[command(subcommand)]
            sub: KeyCmd,
        },
    }

    #[derive(Subcommand)]
    enum KeyCmd {
        /// Create a new API key
        Create {
            #[arg(long)] label: String,
            #[arg(long, value_delimiter = ',', default_values_t = ["read".to_string(), "write".to_string()])]
            scopes: Vec<String>,
            /// Restrict the key to listed domains (comma-separated). Empty = all domains.
            #[arg(long, value_delimiter = ',')]
            domains: Vec<String>,
        },
        /// List existing keys (never shows plaintext)
        List {
            /// Include revoked keys in the output
            #[arg(long)] all: bool,
        },
        /// Revoke a key by id or by hash prefix
        Revoke {
            /// Id (UUID) or hash prefix (first 12 hex chars)
            id_or_prefix: String,
        },
        /// Revoke and re-create with the same scopes/label
        Rotate {
            id_or_prefix: String,
        },
    }

`Create` outputs the plaintext exactly once on stdout (for piping into env
files) and a confirmation on stderr. Use colored output only on stderr to keep
stdout machine-readable.

    fn run_keys_create(...) -> anyhow::Result<()> {
        let store = open_store()?;   // Arc<dyn MemoryStore + ApiKeyStore>
        let plaintext = crate::auth::keys::generate_key();
        let hash = crate::auth::keys::hash_key(plaintext.as_str());
        let rec = ApiKeyRecord {
            id: uuid::Uuid::new_v4(),
            key_hash: hash, label, scopes, domain_filter: domains,
            created_at: chrono::Utc::now(),
            last_used: None, active: true,
        };
        block_on(store.create_api_key(&rec))?;

        // stderr: human-readable
        eprintln!("{} {}", "Created key:".green().bold(), rec.label);
        eprintln!("  id:      {}", rec.id);
        eprintln!("  scopes:  {}", rec.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","));
        eprintln!("  domains: {}", if rec.domain_filter.is_empty() { "all".to_string() } else { rec.domain_filter.join(",") });
        eprintln!();
        eprintln!("{}", "Store the plaintext key now. It will not be shown again.".yellow());
        // stdout: ONLY the plaintext, for scripting
        println!("{}", plaintext.as_str());
        Ok(())
    }

`List`:

    kid                                    label            scopes        domains    last_used             hash
    d3a8...                                macbook          read,write    all        2026-04-20 11:02      a1b2c3d4e5f6
    ...

Never print the plaintext. Show only `hash[..12]`.

### D12. Migrations

Postgres `0300_api_keys.sql` (idempotent; Phase 2 may have already created the
table, in which case this migration is a no-op `CREATE TABLE IF NOT EXISTS`):

    CREATE TABLE IF NOT EXISTS api_keys (
        id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        key_hash      TEXT NOT NULL UNIQUE,
        label         TEXT NOT NULL,
        scopes        TEXT[] NOT NULL DEFAULT ARRAY['read','write'],
        domain_filter TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
        created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
        last_used     TIMESTAMPTZ,
        active        BOOLEAN NOT NULL DEFAULT true
    );

    CREATE INDEX IF NOT EXISTS idx_api_keys_active
        ON api_keys (active) WHERE active;

SQLite `0300_api_keys.sql`:

    CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL UNIQUE,
        label TEXT NOT NULL,
        scopes TEXT NOT NULL DEFAULT 'read,write',     -- comma-joined
        domain_filter TEXT NOT NULL DEFAULT '',        -- comma-joined, '' = all
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_used TEXT,
        active INTEGER NOT NULL DEFAULT 1
    );

    CREATE INDEX IF NOT EXISTS idx_api_keys_active
        ON api_keys (active) WHERE active = 1;

Both backends' trait impls convert to/from `ApiKeyRecord`.

### D13. Wiring main.rs and the `serve` CLI path

`main.rs` refactor:

1. `Config::load()` reads `vestige.toml` (if present) and overlays env vars.
2. Run `enforce_bind_safety(&cfg.server, &cfg.auth)` before spawning any
   listener. On failure, print to stderr and exit 2.
3. Build `AppCtx` with `Arc<dyn MemoryStore + ApiKeyStore>`, `CognitiveEngine`,
   event bus, `session_key`, `config`.
4. `build_router(ctx)` returns a single Axum `Router` that covers MCP, REST,
   and dashboard.
5. `axum::serve(listener, app).await`.
6. The stdio MCP transport continues to run in parallel (unchanged) for
   desktop / Claude Code single-user scenarios.

`serve` CLI subcommand: identical flow minus stdio.

### D14. Docs

- `docs/env-vars.md` new: table of every supported env var, default, purpose,
  deprecation status.
- Section in `README.md`: "Running Vestige as a network server".
- Cheat-sheet section in `CLAUDE.md` for: create a key, start the server,
  curl smoke test.

## Test Plan

### Unit tests (colocated under `#[cfg(test)]`)

- `auth/keys.rs`:
  - `generate_key_has_prefix_and_length()` -- asserts `vst_` prefix and 34-ish
    char total, regex `^vst_[A-Za-z0-9_-]{29}$`.
  - `hash_key_blake3_is_stable_and_hex()` -- fixed vector test.
  - `verify_key_accepts_same_input()` / `verify_key_rejects_tampered()` /
    `verify_key_rejects_length_mismatch()`.
  - `keys_are_unique_in_a_loop()` -- 10_000 iterations, no collisions.
  - `plaintext_zeroed_on_drop()` -- unsafe peek into the backing buffer
    through a wrapper that exposes bytes for the test only.

- `auth/session.rs`:
  - `round_trip_claims_through_signed_jar()`.
  - `expired_cookie_is_rejected()` -- mint a cookie with `exp = iat - 60` and
    confirm `claims_from` returns `None`.
  - `tampered_cookie_is_rejected()` -- flip one byte in the signed segment,
    confirm the jar drops it.
  - `session_key_env_overrides_file()`.
  - `session_key_generated_file_has_mode_0600_on_unix()`.

- `auth/middleware.rs`:
  - `extract_credentials_prefers_bearer_over_api_key_header()`.
  - `extract_credentials_falls_back_to_cookie()`.
  - `effective_domain_filter_empty_means_all()`.
  - `effective_domain_filter_header_narrows_within_key_filter()`.
  - `effective_domain_filter_rejects_header_outside_key_filter()`.
  - `missing_credentials_returns_401()`.
  - `revoked_key_returns_401()`.
  - `insufficient_scope_returns_403()`.

- `config.rs`:
  - `parse_vestige_toml_with_server_and_auth_sections()`.
  - `env_vars_override_toml_bind()`.
  - `enforce_bind_safety_rejects_0_0_0_0_with_auth_disabled()`.
  - `enforce_bind_safety_allows_0_0_0_0_with_auth_enabled()`.
  - `enforce_bind_safety_allows_loopback_with_auth_disabled()`.

- `http/errors.rs`:
  - `not_found_emits_problem_json_with_correct_content_type()`.
  - `bad_request_includes_detail_field()`.

- `http/mcp.rs`:
  - `post_mcp_unauth_returns_401()` (this would normally be caught by the
    middleware; kept as a unit test by constructing the Router minus the
    middleware to exercise the handler's own error paths).

### Integration tests (`tests/phase_3/`)

All tests spin up the full Axum stack in-process on a random port via
`tokio::net::TcpListener::bind("127.0.0.1:0")`, wire a `SqliteMemoryStore` in
a `TempDir`, and issue HTTP calls with `reqwest`.

Files (each one a standalone binary test file):

- `phase_3/common/mod.rs` -- shared harness (`spawn_server()`,
  `create_test_key()`, `client()`).

- `phase_3/http_mcp_round_trip.rs` -- boot server, mint a key, send
  `initialize` over `POST /mcp` with `Authorization: Bearer vst_...`, follow
  with `tools/list`, assert we see the expected tool count (greater than 20).

- `phase_3/http_sse_stream.rs` -- `POST /api/v1/consolidate` returns 202 +
  `session_id`. `GET /mcp?op=consolidate&session=...` streams at least one
  `progress` and one `done` event. Use `eventsource-client` dev dep, or parse
  the stream manually.

- `phase_3/rest_api_crud.rs` -- exercises each REST endpoint in turn:
  - `POST /api/v1/memories` -> 201 + body.
  - `GET /api/v1/memories/{id}` -> 200.
  - `PUT /api/v1/memories/{id}` -> 200.
  - `POST /api/v1/search` -> 200 with the new memory in results.
  - `POST /api/v1/memories/{id}/promote` -> 200.
  - `GET /api/v1/stats` -> 200.
  - `GET /api/v1/domains` -> 200 (likely empty).
  - `DELETE /api/v1/memories/{id}` -> 204.

- `phase_3/auth_bearer_token.rs`:
  - unauth: `GET /api/v1/stats` returns 401 and `Content-Type:
    application/problem+json`.
  - valid Bearer: same call returns 200.
  - revoked key: `POST /api/v1/keys/{id}` DELETE then reuse -> 401.
  - tampered Bearer (last char flipped) -> 401.

- `phase_3/auth_api_key_header.rs`:
  - `X-API-Key: vst_...` alone -> 200.
  - Both Bearer and X-API-Key with different values -> Bearer wins (asserted
    via a key that is read-only in Bearer + full-scope X-API-Key, then
    confirming a write 403s).

- `phase_3/auth_session_cookie.rs`:
  - `POST /dashboard/login` with valid key -> 200 + `Set-Cookie:
    vestige_session=...; HttpOnly; SameSite=Strict; Path=/`.
  - reuse cookie: `GET /api/v1/stats` returns 200.
  - tampered cookie (change one char): -> 401.
  - `POST /dashboard/logout` -> `Set-Cookie: vestige_session=; Max-Age=0`.

- `phase_3/auth_domain_filter.rs`:
  - Key with `domain_filter = ["dev"]`:
    - `POST /api/v1/search` without header -> search is scoped to `["dev"]`
      (insert fixtures with two domains, assert only `dev` rows returned).
    - `X-Vestige-Domain: dev` -> same.
    - `X-Vestige-Domain: home` -> 403 with detail `domain not permitted`.
  - Key with empty filter + `X-Vestige-Domain: dev` -> scoped to `["dev"]`.
  - Key with empty filter + no header -> no scoping.

- `phase_3/auth_scope_enforcement.rs`:
  - read-only key cannot call `POST /api/v1/memories` -> 403.
  - read-only key CAN call `POST /api/v1/search` -> 200.

- `phase_3/bind_safety_nonlocalhost_without_auth.rs`:
  - Spawn `vestige serve --bind 0.0.0.0:0` as a subprocess with `auth.enabled
    = false` via a temp `vestige.toml`.
  - Assert: non-zero exit, stderr contains `refusing to bind`, no listener
    ever opens (confirm by trying to connect to the configured port and
    expecting connection refused after a short timeout).

- `phase_3/cli_keys_create_list_revoke.rs`:
  - Spawn the `vestige` CLI binary with `--data-dir <tmp>`.
  - Run `vestige keys create --label test --scopes read,write`; capture
    stdout (the plaintext) and stderr (the human summary). Assert `vst_`
    prefix in stdout.
  - Run `vestige keys list`; assert no plaintext, label `test` present.
  - Run `vestige keys revoke <prefix>`; confirm exit 0.
  - Run `vestige keys list`; assert label no longer visible without `--all`.

- `phase_3/dashboard_login_flow.rs`:
  - Full loop: login -> fetch `/dashboard` (gets SPA index, unauthed ok) ->
    fetch `/api/memories` (authed via cookie) -> logout -> fetch `/api/memories`
    (401).

- `phase_3/deprecation_auth_token.rs`:
  - Start the server with `VESTIGE_AUTH_TOKEN=test12345...` and no created
    keys. Send a Bearer request with that token -> 200. Assert stderr log
    contains `deprecated`.

### Smoke test (`tests/phase_3/smoke/`)

- `remote_mcp_client.sh`:

        #!/usr/bin/env bash
        set -euo pipefail
        KEY="${VESTIGE_TEST_KEY:?set me}"
        HOST="${VESTIGE_HOST:-http://127.0.0.1:3928}"
        # Initialize a session
        RESP=$(curl -sS -D /tmp/h -H "Authorization: Bearer $KEY" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","id":1,"method":"initialize",
                 "params":{"protocolVersion":"2025-11-25",
                 "clientInfo":{"name":"smoke","version":"0"},
                 "capabilities":{}}}' \
            "$HOST/mcp")
        SID=$(grep -i 'mcp-session-id:' /tmp/h | awk '{print $2}' | tr -d '\r')
        # tools/list
        curl -sS -H "Authorization: Bearer $KEY" \
            -H "Mcp-Session-Id: $SID" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
            "$HOST/mcp" | jq '.result.tools | length'
        echo "smoke ok"

## Acceptance Criteria

- [ ] `cargo build -p vestige-mcp` -- zero warnings, all feature combinations
      (`--no-default-features`, default, `--features ort-dynamic`).
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] `cargo fmt --all --check`.
- [ ] All `tests/phase_3/*.rs` pass, plus phase_1 and phase_2 remain green.
- [ ] Unauth request to `POST /mcp` returns 401 with
      `Content-Type: application/problem+json` and a body containing `status`,
      `title`, `detail`.
- [ ] Binding `0.0.0.0:<port>` with `[auth].enabled = false` makes the
      process exit with code 2 and print `refusing to bind` to stderr.
- [ ] `vestige keys create --label X` prints exactly one line on stdout
      matching `^vst_[A-Za-z0-9_-]+$`; `vestige keys list` never prints that
      line back.
- [ ] Dashboard login from a browser-like client (tested via the reqwest
      `Client::cookie_store(true)` harness) yields a `Set-Cookie` with
      `HttpOnly`, `SameSite=Strict`, `Path=/`, and Max-Age present.
- [ ] A second machine can run a curl-based MCP client against the server
      (smoke test) and receive successful `tools/list` responses.
- [ ] `VESTIGE_AUTH_TOKEN` still works and emits the deprecation warning.
- [ ] `tests/phase_3/auth_domain_filter.rs` demonstrates that a key scoped to
      `dev` cannot read `home`-domain memories via any of the three auth modes
      and cannot escape with `X-Vestige-Domain`.

## Rollback Notes

- Ship behind an on-by-default Cargo feature `http-server` on
  `vestige-mcp`. Disabling it reverts to stdio + existing localhost HTTP
  (`protocol/http.rs` in its current form) with zero behaviour change.
- SQL: migration `0300_api_keys.sql` is additive only; rollback is a single
  `DROP TABLE api_keys;` in `0300_api_keys.down.sql` for both backends. Keep a
  row count safety check in the down migration and log the deletion.
- Session secret file: deleting `<data_dir>/session_secret` invalidates every
  outstanding cookie; users simply log in again. Safe to rotate.
- Env var sunset schedule:
  - v2.1.x: `VESTIGE_AUTH_TOKEN` emits a warning, still works.
  - v2.2.0: `VESTIGE_AUTH_TOKEN` refused with an error pointing at
    `vestige keys create`.
- Downgrade procedure: `git revert` the Phase 3 merge, then run the down
  migration. No data loss; plaintext keys were only ever in user-side
  secret managers.

## Open Implementation Questions

1. JSON-RPC library: hand-rolled vs jsonrpsee?

   - Candidate A: keep the hand-rolled types in `protocol/types.rs` plus the
     session-aware `post_mcp` handler already in `protocol/http.rs`.
   - Candidate B: switch to `jsonrpsee = "0.24"` with the `server` feature
     and adapt it to Axum via `jsonrpsee::server::Server`.

   RECOMMENDATION: A. Phase 3 is about auth and transport surfaces, not
   library rewrites. The existing types are already correct, tested, and
   compatible with Streamable HTTP; the 29 cognitive modules depend on
   `McpServer::handle_request`, which does not map 1:1 to jsonrpsee's
   `RpcModule` trait. Re-evaluate in a future phase only if we need subscription
   notifications beyond SSE.

2. Streamable HTTP vs plain POST-with-JSON?

   - The MCP spec titled "Streamable HTTP" defines: `POST /mcp` for
     request/response, `GET /mcp` for SSE where the client subscribes to
     server-initiated messages, and an `Mcp-Session-Id` header for session
     correlation. The current implementation already covers POST + session
     header + DELETE; Phase 3 adds the GET/SSE half.

   RECOMMENDATION: implement the full Streamable HTTP transport. Long-running
   tools (dream, consolidate, discover) benefit from SSE progress events, and
   Claude Desktop / Claude Code both speak Streamable HTTP natively. Keeping
   POST-only would work for short calls but block the UX we want for
   background jobs.

3. Session cookie crate?

   - Candidate A: `axum-extra::extract::cookie::SignedCookieJar` with a 64-byte
     `Key`.
   - Candidate B: `tower-sessions = "0.13"` with the `MemoryStore` or
     `PostgresStore` session backend.
   - Candidate C: stateless JWT via `jsonwebtoken`.

   RECOMMENDATION: A. We do not need server-side session state (the `api_keys`
   row is the state; the cookie is merely a signed pointer to it). B adds a
   whole storage backend we do not need. C adds signing-algorithm surface area
   and revocation becomes awkward ("revoked key" with a long-lived JWT).
   `SignedCookieJar` gives us HMAC-signed cookies for free, integrates with
   axum extractors, and the payload is tiny.

4. Key format and length?

   - 22 random bytes base64url-no-pad = 176 bits entropy, encoded ~30 chars,
     full key ~34 chars with the `vst_` prefix. Long enough to make
     brute-force infeasible, short enough to paste into config files.
   - Alternatives: 32 bytes (40 chars, overkill), 16 bytes (128 bits, marginal
     for secret material shared over networks).

   RECOMMENDATION: 22 bytes. Prefix `vst_` is already documented in the PRD
   and gives grep-ability.

5. Rate limiting: in scope for Phase 3?

   - Useful: mitigates slow brute force, runaway agents.
   - Expensive to design well (per-key, per-IP, per-endpoint).

   RECOMMENDATION: OUT of scope. Track as `docs/adr/0002-rate-limiting.md`
   follow-up. Axum + `tower` has `ConcurrencyLimitLayer` (already used); a
   follow-up can add `governor` or `tower_governor` behind the auth layer so
   identity is available.

6. CORS policy defaults for dashboard in server mode?

   - Candidate A: allow only origins derived from `server.bind` host + the
     dashboard port.
   - Candidate B: allow user-listed origins via `server.allowed_origins`
     config, with A as fallback.
   - Candidate C: open CORS to `*` when TLS is configured.

   RECOMMENDATION: B. Auto-populate `allowed_origins` from the bind address
   and dashboard port at start time; if the operator sets the config list,
   use that list verbatim. Never `*` (`allow_credentials = true` is
   incompatible with `*` anyway).

7. Dashboard session lifetime?

   - 8 hours for default; configurable via `auth.session_ttl_hours`.
   - Rotate on each write? (Rolling sessions.)

   RECOMMENDATION: 8 hours fixed, non-rolling. Revisit if users complain.

8. Handling `tools/call` scope granularity?

   - Today, `tools/call` is a single MCP method. Read-only tools like
     `search`, `deep_reference`, `predict` should be callable with a
     read-only key.

   RECOMMENDATION: map tool names to scopes in `McpServer::handle_tools_call`.
   Read-only names: `search`, `session_context`, `memory` with action in
   `{get, state, get_batch}`, `deep_reference`, `cross_reference`, `predict`,
   `explore_connections`, `find_duplicates`, `memory_timeline`,
   `memory_changelog`, `memory_health`, `memory_graph`, `importance_score`,
   `system_status`. Everything else requires `write`. If a read-only key
   calls a write tool, return a JSON-RPC error with code `-32003`
   ("server not initialized" is close but wrong; reuse `-32603 internal` with
   a descriptive message or add a new `-32004 UnauthorizedTool`). RECOMMEND
   adding `-32004`.

9. How to bridge `MemoryStore` trait with dashboard state (`AppState`)?

   - Today `AppState.storage: Arc<Storage>` is a concrete type.
   - Phase 2 introduces `Arc<dyn MemoryStore>`.

   RECOMMENDATION: in Phase 3, introduce `AppCtx { store: Arc<dyn MemoryStore>,
   cognitive, config, event_tx }` as the single state type for the unified
   router. Keep `AppState` as a thin wrapper (or alias) if the dashboard
   handlers need to stay untouched in this phase. Migrate the dashboard
   handlers to the trait in a follow-up refactor to contain the blast radius.

10. Windows support for `session_secret` and `auth_token` file modes?

    - Unix gets `0600` via `OpenOptionsExt`.
    - Windows has no direct equivalent; ACLs differ.

    RECOMMENDATION: document the limitation; use default permissions on
    Windows. Add a `#[cfg(windows)]` placeholder to set owner-only ACLs via
    `windows-acl` in a follow-up, not Phase 3.

### Critical Files for Implementation

- /home/delandtj/prppl/vestige/crates/vestige-mcp/src/protocol/http.rs
- /home/delandtj/prppl/vestige/crates/vestige-mcp/src/dashboard/mod.rs
- /home/delandtj/prppl/vestige/crates/vestige-mcp/src/main.rs
- /home/delandtj/prppl/vestige/crates/vestige-mcp/src/bin/cli.rs
- /home/delandtj/prppl/vestige/crates/vestige-mcp/Cargo.toml
