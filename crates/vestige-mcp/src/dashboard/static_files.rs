//! Embedded SvelteKit dashboard static file server.
//!
//! The built SvelteKit app is embedded into the binary at compile time
//! using `include_dir!`. This serves it at `/dashboard/` prefix.

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use include_dir::{include_dir, Dir};

/// Embed the entire SvelteKit build output into the binary.
/// Build with: cd apps/dashboard && pnpm build
/// The build output goes to apps/dashboard/build/
static DASHBOARD_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../apps/dashboard/build");

/// Serve the SvelteKit dashboard index
pub async fn serve_dashboard_spa() -> impl IntoResponse {
    match DASHBOARD_DIR.get_file("index.html") {
        Some(file) => Html(
            String::from_utf8_lossy(file.contents()).to_string(),
        )
        .into_response(),
        None => (StatusCode::NOT_FOUND, "Dashboard not built. Run: cd apps/dashboard && pnpm build")
            .into_response(),
    }
}

/// Serve static assets from the embedded SvelteKit build
pub async fn serve_dashboard_asset(Path(path): Path<String>) -> Response {
    // Try exact path
    if let Some(file) = DASHBOARD_DIR.get_file(&path) {
        let mime = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime),
                (
                    header::CACHE_CONTROL,
                    if path.contains("/_app/") {
                        // Immutable assets (hashed filenames)
                        "public, max-age=31536000, immutable".to_string()
                    } else {
                        "public, max-age=60".to_string()
                    },
                ),
            ],
            file.contents().to_vec(),
        )
            .into_response();
    }

    // SPA fallback: serve index.html for client-side routing
    match DASHBOARD_DIR.get_file("index.html") {
        Some(file) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html".to_string())],
            file.contents().to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}
