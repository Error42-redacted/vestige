//! stdio Transport for MCP
//!
//! Handles JSON-RPC communication over stdin/stdout.
//! v1.9.2: Async tokio I/O with error resilience.

use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

use super::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::server::McpServer;

/// Maximum consecutive I/O errors before giving up
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

/// stdio Transport for MCP server
pub struct StdioTransport;

impl StdioTransport {
    pub fn new() -> Self {
        Self
    }

    /// Run the MCP server over stdio with error resilience.
    pub async fn run(self, mut server: McpServer) -> Result<(), io::Error> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let mut reader = BufReader::new(stdin);
        let mut stdout = stdout;
        let mut consecutive_errors: u32 = 0;
        let mut line_buf = String::new();

        loop {
            line_buf.clear();

            tokio::select! {
                result = reader.read_line(&mut line_buf) => {
                    match result {
                        Ok(0) => {
                            // Clean EOF — stdin closed
                            info!("stdin closed (EOF), shutting down");
                            break;
                        }
                        Ok(_) => {
                            consecutive_errors = 0;
                            let line = line_buf.trim();

                            if line.is_empty() {
                                continue;
                            }

                            debug!("Received: {} bytes", line.len());

                            // Parse JSON-RPC request
                            let request: JsonRpcRequest = match serde_json::from_str(line) {
                                Ok(r) => r,
                                Err(e) => {
                                    warn!("Failed to parse request: {}", e);
                                    let error_response = JsonRpcResponse::error(None, JsonRpcError::parse_error());
                                    match serde_json::to_string(&error_response) {
                                        Ok(response_json) => {
                                            let out = format!("{}\n", response_json);
                                            stdout.write_all(out.as_bytes()).await?;
                                            stdout.flush().await?;
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize error response: {}", e);
                                            let fallback = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n";
                                            let _ = stdout.write_all(fallback.as_bytes()).await;
                                            let _ = stdout.flush().await;
                                        }
                                    }
                                    continue;
                                }
                            };

                            // Handle the request
                            if let Some(response) = server.handle_request(request).await {
                                match serde_json::to_string(&response) {
                                    Ok(response_json) => {
                                        debug!("Sending: {} bytes", response_json.len());
                                        let out = format!("{}\n", response_json);
                                        stdout.write_all(out.as_bytes()).await?;
                                        stdout.flush().await?;
                                    }
                                    Err(e) => {
                                        error!("Failed to serialize response: {}", e);
                                        let fallback = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n";
                                        let _ = stdout.write_all(fallback.as_bytes()).await;
                                        let _ = stdout.flush().await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            warn!(
                                "I/O error reading stdin ({}/{}): {}",
                                consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                            );
                            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                error!(
                                    "Too many consecutive I/O errors ({}), shutting down",
                                    consecutive_errors
                                );
                                break;
                            }
                            // Brief pause before retrying
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}
