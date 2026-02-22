//! Vestige MCP Server v1.0 - Cognitive Memory for Claude
//!
//! A bleeding-edge Rust MCP (Model Context Protocol) server that provides
//! Claude and other AI assistants with long-term memory capabilities
//! powered by 130 years of memory research.
//!
//! Core Features:
//! - FSRS-6 spaced repetition algorithm (21 parameters, 30% more efficient than SM-2)
//! - Bjork dual-strength memory model
//! - Local semantic embeddings (768-dim BGE, no external API)
//! - HNSW vector search (20x faster than FAISS)
//! - Hybrid search (BM25 + semantic + RRF fusion)
//!
//! Neuroscience Features:
//! - Synaptic Tagging & Capture (retroactive importance)
//! - Spreading Activation Networks (multi-hop associations)
//! - Hippocampal Indexing (two-phase retrieval)
//! - Memory States (active/dormant/silent/unavailable)
//! - Context-Dependent Memory (encoding specificity)
//! - Multi-Channel Importance Signals
//! - Predictive Retrieval
//! - Prospective Memory (intentions with triggers)
//!
//! Advanced Features:
//! - Memory Dreams (insight generation during consolidation)
//! - Memory Compression
//! - Reconsolidation (memories editable on retrieval)
//! - Memory Chains (reasoning paths)

// cognitive is exported from lib.rs for dashboard access
use vestige_mcp::cognitive;
mod protocol;
mod resources;
mod server;
mod tools;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn, Level};
use tracing_subscriber::EnvFilter;

// Use vestige-core for the cognitive science engine
use vestige_core::Storage;

use crate::protocol::stdio::StdioTransport;
use crate::server::McpServer;

/// Parse command-line arguments and return the optional data directory path.
/// Returns `None` for the path if no `--data-dir` was specified.
/// Exits the process if `--help` or `--version` is requested.
fn parse_args() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let mut data_dir: Option<PathBuf> = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Vestige MCP Server v{}", env!("CARGO_PKG_VERSION"));
                println!();
                println!("FSRS-6 powered AI memory server using the Model Context Protocol.");
                println!();
                println!("USAGE:");
                println!("    vestige-mcp [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --help              Print help information");
                println!("    -V, --version           Print version information");
                println!("    --data-dir <PATH>       Custom data directory");
                println!();
                println!("ENVIRONMENT:");
                println!("    RUST_LOG               Log level filter (e.g., debug, info, warn, error)");
                println!();
                println!("EXAMPLES:");
                println!("    vestige-mcp");
                println!("    vestige-mcp --data-dir /custom/path");
                println!("    RUST_LOG=debug vestige-mcp");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("vestige-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--data-dir" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --data-dir requires a path argument");
                    eprintln!("Usage: vestige-mcp --data-dir <PATH>");
                    std::process::exit(1);
                }
                data_dir = Some(PathBuf::from(&args[i]));
            }
            arg if arg.starts_with("--data-dir=") => {
                // Safe: we just verified the prefix exists with starts_with
                let path = arg.strip_prefix("--data-dir=").unwrap_or("");
                if path.is_empty() {
                    eprintln!("error: --data-dir requires a path argument");
                    eprintln!("Usage: vestige-mcp --data-dir <PATH>");
                    std::process::exit(1);
                }
                data_dir = Some(PathBuf::from(path));
            }
            arg => {
                eprintln!("error: unknown argument '{}'", arg);
                eprintln!("Usage: vestige-mcp [OPTIONS]");
                eprintln!("Try 'vestige-mcp --help' for more information.");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    data_dir
}

#[tokio::main]
async fn main() {
    // Parse CLI arguments first (before logging init, so --help/--version work cleanly)
    let data_dir = parse_args();

    // Initialize logging to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(Level::INFO.into())
        )
        .with_writer(io::stderr)
        .with_target(false)
        .with_ansi(false)
        .init();

    info!("Vestige MCP Server v{} starting...", env!("CARGO_PKG_VERSION"));

    // Initialize storage with optional custom data directory
    let storage = match Storage::new(data_dir) {
        Ok(s) => {
            info!("Storage initialized successfully");

            // Try to initialize embeddings early and log any issues
            #[cfg(feature = "embeddings")]
            {
                if let Err(e) = s.init_embeddings() {
                    error!("Failed to initialize embedding service: {}", e);
                    error!("Smart ingest will fall back to regular ingest without deduplication");
                    error!("Hint: Check FASTEMBED_CACHE_PATH or ensure ~/.fastembed_cache exists");
                } else {
                    info!("Embedding service initialized successfully");
                }
            }

            Arc::new(s)
        }
        Err(e) => {
            error!("Failed to initialize storage: {}", e);
            std::process::exit(1);
        }
    };

    // Spawn periodic auto-consolidation so FSRS-6 decay scores stay fresh.
    // Runs on startup (if needed) and then every N hours (default: 6).
    // Configurable via VESTIGE_CONSOLIDATION_INTERVAL_HOURS env var.
    {
        let storage_clone = storage.clone();
        tokio::spawn(async move {
            let interval_hours: u64 = std::env::var("VESTIGE_CONSOLIDATION_INTERVAL_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(6);

            // Small delay so we don't block server startup / stdio handshake
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            loop {
                // Check whether consolidation is actually needed
                let should_run = match storage_clone.get_last_consolidation() {
                    Ok(Some(last)) => {
                        let elapsed = chrono::Utc::now() - last;
                        let stale = elapsed > chrono::Duration::hours(interval_hours as i64);
                        if !stale {
                            info!(
                                last_consolidation = %last,
                                "Skipping auto-consolidation (last run was < {} hours ago)",
                                interval_hours
                            );
                        }
                        stale
                    }
                    Ok(None) => {
                        info!("No previous consolidation found — running first auto-consolidation");
                        true
                    }
                    Err(e) => {
                        warn!("Could not read consolidation history: {} — running anyway", e);
                        true
                    }
                };

                if should_run {
                    match storage_clone.run_consolidation() {
                        Ok(result) => {
                            info!(
                                nodes_processed = result.nodes_processed,
                                decay_applied = result.decay_applied,
                                embeddings_generated = result.embeddings_generated,
                                duplicates_merged = result.duplicates_merged,
                                activations_computed = result.activations_computed,
                                duration_ms = result.duration_ms,
                                "Periodic auto-consolidation complete"
                            );
                        }
                        Err(e) => {
                            warn!("Periodic auto-consolidation failed: {}", e);
                        }
                    }
                }

                // Sleep until next check
                tokio::time::sleep(std::time::Duration::from_secs(interval_hours * 3600)).await;
            }
        });
    }

    // Create cognitive engine (stateful neuroscience modules)
    let cognitive = Arc::new(Mutex::new(cognitive::CognitiveEngine::new()));
    info!("CognitiveEngine initialized (28 modules)");

    // Create shared event broadcast channel for dashboard <-> MCP tool events
    let (event_tx, _) = tokio::sync::broadcast::channel::<vestige_mcp::dashboard::events::VestigeEvent>(1024);

    // Spawn dashboard HTTP server alongside MCP server (now with CognitiveEngine access)
    {
        let dashboard_port = std::env::var("VESTIGE_DASHBOARD_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(3927);
        let dashboard_storage = Arc::clone(&storage);
        let dashboard_cognitive = Arc::clone(&cognitive);
        let dashboard_event_tx = event_tx.clone();
        tokio::spawn(async move {
            match vestige_mcp::dashboard::start_background_with_event_tx(
                dashboard_storage,
                Some(dashboard_cognitive),
                dashboard_event_tx,
                dashboard_port,
            ).await {
                Ok(_state) => {
                    info!("Dashboard started with WebSocket + CognitiveEngine + shared event bus");
                }
                Err(e) => {
                    warn!("Dashboard failed to start: {}", e);
                }
            }
        });
    }

    // Load cross-encoder reranker in the background (downloads ~150MB on first run)
    #[cfg(feature = "embeddings")]
    {
        let cog_clone = Arc::clone(&cognitive);
        tokio::spawn(async move {
            // Small delay so we don't block the stdio handshake
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut cog = cog_clone.lock().await;
            cog.reranker.init_cross_encoder();
        });
    }

    // Create MCP server with shared event channel for dashboard broadcasts
    let server = McpServer::new_with_events(storage, cognitive, event_tx);

    // Create stdio transport
    let transport = StdioTransport::new();

    info!("Starting MCP server on stdio...");

    // Run the server
    if let Err(e) = transport.run(server).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    info!("Vestige MCP Server shutting down");
}
