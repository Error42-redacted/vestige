//! Memory Web Dashboard
//!
//! Self-contained web UI at localhost:3927 for browsing, searching,
//! and managing Vestige memories. Auto-starts inside the MCP server process.

pub mod handlers;
pub mod state;

use axum::routing::{delete, get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{info, warn};

use state::AppState;
use vestige_core::Storage;

/// Build the axum router with all dashboard routes
pub fn build_router(storage: Arc<Storage>, port: u16) -> Router {
    let state = AppState { storage };

    let origin = format!("http://127.0.0.1:{}", port)
        .parse::<axum::http::HeaderValue>()
        .expect("valid origin");
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::DELETE])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let csp = SetResponseHeaderLayer::overriding(
        axum::http::header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_static("default-src 'self' 'unsafe-inline'"),
    );

    Router::new()
        // Dashboard UI
        .route("/", get(handlers::serve_dashboard))
        // API endpoints
        .route("/api/memories", get(handlers::list_memories))
        .route("/api/memories/{id}", get(handlers::get_memory))
        .route("/api/memories/{id}", delete(handlers::delete_memory))
        .route("/api/memories/{id}/promote", post(handlers::promote_memory))
        .route("/api/memories/{id}/demote", post(handlers::demote_memory))
        .route("/api/stats", get(handlers::get_stats))
        .route("/api/timeline", get(handlers::get_timeline))
        .route("/api/health", get(handlers::health_check))
        .layer(
            ServiceBuilder::new()
                .concurrency_limit(10)
                .layer(cors)
                .layer(csp)
        )
        .with_state(state)
}

/// Start the dashboard HTTP server (blocking — use in CLI mode)
pub async fn start_dashboard(
    storage: Arc<Storage>,
    port: u16,
    open_browser: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_router(storage, port);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    info!("Dashboard starting at http://127.0.0.1:{}", port);

    if open_browser {
        let url = format!("http://127.0.0.1:{}", port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = open::that(&url);
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start the dashboard as a background task (non-blocking — use in MCP server)
pub async fn start_background(
    storage: Arc<Storage>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_router(storage, port);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(
                "Dashboard could not bind to port {}: {} (MCP server continues without dashboard)",
                port, e
            );
            return Err(Box::new(e));
        }
    };

    info!("Dashboard available at http://127.0.0.1:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
