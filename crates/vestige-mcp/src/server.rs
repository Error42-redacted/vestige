//! MCP Server Core
//!
//! Handles the main MCP server logic, routing requests to appropriate
//! tool and resource handlers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::cognitive::CognitiveEngine;
use crate::protocol::messages::{
    CallToolRequest, CallToolResult, InitializeRequest, InitializeResult,
    ListResourcesResult, ListToolsResult, ReadResourceRequest, ReadResourceResult,
    ResourceDescription, ServerCapabilities, ServerInfo, ToolDescription,
};
use crate::protocol::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, MCP_VERSION};
use crate::resources;
use crate::tools;
use vestige_core::Storage;

/// MCP Server implementation
pub struct McpServer {
    storage: Arc<Mutex<Storage>>,
    cognitive: Arc<Mutex<CognitiveEngine>>,
    initialized: bool,
    /// Tool call counter for inline consolidation trigger (every 100 calls)
    tool_call_count: AtomicU64,
}

impl McpServer {
    pub fn new(storage: Arc<Mutex<Storage>>, cognitive: Arc<Mutex<CognitiveEngine>>) -> Self {
        Self {
            storage,
            cognitive,
            initialized: false,
            tool_call_count: AtomicU64::new(0),
        }
    }

    /// Handle an incoming JSON-RPC request
    pub async fn handle_request(&mut self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!("Handling request: {}", request.method);

        // Check initialization for non-initialize requests
        if !self.initialized && request.method != "initialize" && request.method != "notifications/initialized" {
            warn!("Rejecting request '{}': server not initialized", request.method);
            return Some(JsonRpcResponse::error(
                request.id,
                JsonRpcError::server_not_initialized(),
            ));
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "notifications/initialized" => {
                // Notification, no response needed
                return None;
            }
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(request.params).await,
            "resources/list" => self.handle_resources_list().await,
            "resources/read" => self.handle_resources_read(request.params).await,
            "ping" => Ok(serde_json::json!({})),
            method => {
                warn!("Unknown method: {}", method);
                Err(JsonRpcError::method_not_found())
            }
        };

        Some(match result {
            Ok(result) => JsonRpcResponse::success(request.id, result),
            Err(error) => JsonRpcResponse::error(request.id, error),
        })
    }

    /// Handle initialize request
    async fn handle_initialize(
        &mut self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: InitializeRequest = match params {
            Some(p) => serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => InitializeRequest::default(),
        };

        // Version negotiation: use client's version if older than server's
        // Claude Desktop rejects servers with newer protocol versions
        let negotiated_version = if request.protocol_version.as_str() < MCP_VERSION {
            info!("Client requested older protocol version {}, using it", request.protocol_version);
            request.protocol_version.clone()
        } else {
            MCP_VERSION.to_string()
        };

        self.initialized = true;
        info!("MCP session initialized with protocol version {}", negotiated_version);

        let result = InitializeResult {
            protocol_version: negotiated_version,
            server_info: ServerInfo {
                name: "vestige".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities {
                tools: Some({
                    let mut map = HashMap::new();
                    map.insert("listChanged".to_string(), serde_json::json!(false));
                    map
                }),
                resources: Some({
                    let mut map = HashMap::new();
                    map.insert("listChanged".to_string(), serde_json::json!(false));
                    map
                }),
                prompts: None,
            },
            instructions: Some(
                "Vestige is your long-term memory system. Use it to remember important information, \
                 recall past knowledge, and maintain context across sessions. The system uses \
                 FSRS-6 spaced repetition to naturally decay memories over time. \
                 \n\nFeedback Protocol: If the user explicitly confirms a memory was helpful, use \
                 memory(action='promote'). If they correct a hallucination or say a memory was wrong, use \
                 memory(action='demote'). Do not ask for permission - just act on their feedback.".to_string()
            ),
        };

        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        // v1.7: 18 tools. Deprecated tools still work via redirects in handle_tools_call.
        let tools = vec![
            // ================================================================
            // UNIFIED TOOLS (v1.1+)
            // ================================================================
            ToolDescription {
                name: "search".to_string(),
                description: Some("Unified search tool. Uses hybrid search (keyword + semantic + convex combination fusion) internally. Auto-strengthens memories on access (Testing Effect).".to_string()),
                input_schema: tools::search_unified::schema(),
            },
            ToolDescription {
                name: "memory".to_string(),
                description: Some("Unified memory management tool. Actions: 'get' (retrieve full node), 'delete' (remove memory), 'state' (get accessibility state), 'promote' (thumbs up — increases retrieval strength), 'demote' (thumbs down — decreases retrieval strength, does NOT delete).".to_string()),
                input_schema: tools::memory_unified::schema(),
            },
            ToolDescription {
                name: "codebase".to_string(),
                description: Some("Unified codebase tool. Actions: 'remember_pattern' (store code pattern), 'remember_decision' (store architectural decision), 'get_context' (retrieve patterns and decisions).".to_string()),
                input_schema: tools::codebase_unified::schema(),
            },
            ToolDescription {
                name: "intention".to_string(),
                description: Some("Unified intention management tool. Actions: 'set' (create), 'check' (find triggered), 'update' (complete/snooze/cancel), 'list' (show intentions).".to_string()),
                input_schema: tools::intention_unified::schema(),
            },
            // ================================================================
            // CORE MEMORY (v1.7: smart_ingest absorbs ingest + checkpoint)
            // ================================================================
            ToolDescription {
                name: "smart_ingest".to_string(),
                description: Some("INTELLIGENT memory ingestion with Prediction Error Gating. Single mode: provide 'content' to auto-decide CREATE/UPDATE/SUPERSEDE. Batch mode: provide 'items' array (max 20) for session-end saves — each item runs the full cognitive pipeline (importance scoring, intent detection, synaptic tagging).".to_string()),
                input_schema: tools::smart_ingest::schema(),
            },
            // ================================================================
            // TEMPORAL TOOLS (v1.2+)
            // ================================================================
            ToolDescription {
                name: "memory_timeline".to_string(),
                description: Some("Browse memories chronologically. Returns memories in a time range, grouped by day. Defaults to last 7 days.".to_string()),
                input_schema: tools::timeline::schema(),
            },
            ToolDescription {
                name: "memory_changelog".to_string(),
                description: Some("View audit trail of memory changes. Per-memory: state transitions. System-wide: consolidations + recent state changes.".to_string()),
                input_schema: tools::changelog::schema(),
            },
            // ================================================================
            // MAINTENANCE TOOLS (v1.7: system_status replaces health_check + stats)
            // ================================================================
            ToolDescription {
                name: "system_status".to_string(),
                description: Some("Combined system health and statistics. Returns status (healthy/degraded/critical/empty), full stats, FSRS preview, cognitive module health, state distribution, warnings, and recommendations.".to_string()),
                input_schema: tools::maintenance::system_status_schema(),
            },
            ToolDescription {
                name: "consolidate".to_string(),
                description: Some("Run FSRS-6 memory consolidation cycle. Applies decay, generates embeddings, and performs maintenance. Use when memories seem stale.".to_string()),
                input_schema: tools::maintenance::consolidate_schema(),
            },
            ToolDescription {
                name: "backup".to_string(),
                description: Some("Create a SQLite database backup. Returns the backup file path.".to_string()),
                input_schema: tools::maintenance::backup_schema(),
            },
            ToolDescription {
                name: "export".to_string(),
                description: Some("Export memories as JSON or JSONL. Supports tag and date filters.".to_string()),
                input_schema: tools::maintenance::export_schema(),
            },
            ToolDescription {
                name: "gc".to_string(),
                description: Some("Garbage collect stale memories below retention threshold. Defaults to dry_run=true for safety.".to_string()),
                input_schema: tools::maintenance::gc_schema(),
            },
            // ================================================================
            // AUTO-SAVE & DEDUP TOOLS (v1.3+)
            // ================================================================
            ToolDescription {
                name: "importance_score".to_string(),
                description: Some("Score content importance using 4-channel neuroscience model (novelty/arousal/reward/attention). Returns composite score, channel breakdown, encoding boost, and explanations.".to_string()),
                input_schema: tools::importance::schema(),
            },
            ToolDescription {
                name: "find_duplicates".to_string(),
                description: Some("Find duplicate and near-duplicate memory clusters using cosine similarity on embeddings. Returns clusters with suggested actions (merge/review). Use to clean up redundant memories.".to_string()),
                input_schema: tools::dedup::schema(),
            },
            // ================================================================
            // COGNITIVE TOOLS (v1.5+)
            // ================================================================
            ToolDescription {
                name: "dream".to_string(),
                description: Some("Trigger memory dreaming — replays recent memories to discover hidden connections, synthesize insights, and strengthen important patterns. Returns insights, connections, and dream stats.".to_string()),
                input_schema: tools::dream::schema(),
            },
            ToolDescription {
                name: "explore_connections".to_string(),
                description: Some("Graph exploration tool for memory connections. Actions: 'chain' (build reasoning path between memories), 'associations' (find related memories via spreading activation + hippocampal index), 'bridges' (find connecting memories between two nodes).".to_string()),
                input_schema: tools::explore::schema(),
            },
            ToolDescription {
                name: "predict".to_string(),
                description: Some("Proactive memory prediction — predicts what memories you'll need next based on context, recent activity, and learned patterns. Returns predictions, suggestions, and speculative retrievals.".to_string()),
                input_schema: tools::predict::schema(),
            },
            // ================================================================
            // RESTORE TOOL (v1.5+)
            // ================================================================
            ToolDescription {
                name: "restore".to_string(),
                description: Some("Restore memories from a JSON backup file. Supports MCP wrapper format, RecallResult format, and direct memory array format.".to_string()),
                input_schema: tools::restore::schema(),
            },
        ];

        let result = ListToolsResult { tools };
        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle tools/call request
    async fn handle_tools_call(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: CallToolRequest = match params {
            Some(p) => serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => return Err(JsonRpcError::invalid_params("Missing tool call parameters")),
        };

        // Record activity on every tool call (non-blocking)
        if let Ok(mut cog) = self.cognitive.try_lock() {
            cog.activity_tracker.record_activity();
            cog.consolidation_scheduler.record_activity();
        }

        let result = match request.name.as_str() {
            // ================================================================
            // UNIFIED TOOLS (v1.1+) - Preferred API
            // ================================================================
            "search" => tools::search_unified::execute(&self.storage, &self.cognitive, request.arguments).await,
            "memory" => tools::memory_unified::execute(&self.storage, &self.cognitive, request.arguments).await,
            "codebase" => tools::codebase_unified::execute(&self.storage, &self.cognitive, request.arguments).await,
            "intention" => tools::intention_unified::execute(&self.storage, &self.cognitive, request.arguments).await,

            // ================================================================
            // Core memory (v1.7: smart_ingest absorbs ingest + checkpoint)
            // ================================================================
            "smart_ingest" => tools::smart_ingest::execute(&self.storage, &self.cognitive, request.arguments).await,

            // ================================================================
            // DEPRECATED (v1.7): ingest → smart_ingest
            // ================================================================
            "ingest" => {
                warn!("Tool 'ingest' is deprecated in v1.7. Use 'smart_ingest' instead.");
                tools::smart_ingest::execute(&self.storage, &self.cognitive, request.arguments).await
            }

            // ================================================================
            // DEPRECATED (v1.7): session_checkpoint → smart_ingest (batch mode)
            // ================================================================
            "session_checkpoint" => {
                warn!("Tool 'session_checkpoint' is deprecated in v1.7. Use 'smart_ingest' with 'items' parameter instead.");
                tools::smart_ingest::execute(&self.storage, &self.cognitive, request.arguments).await
            }

            // ================================================================
            // DEPRECATED (v1.7): promote_memory → memory(action='promote')
            // ================================================================
            "promote_memory" => {
                warn!("Tool 'promote_memory' is deprecated in v1.7. Use 'memory' with action='promote' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("promote"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "promote"})),
                };
                tools::memory_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "demote_memory" => {
                warn!("Tool 'demote_memory' is deprecated in v1.7. Use 'memory' with action='demote' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("demote"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "demote"})),
                };
                tools::memory_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }

            // ================================================================
            // DEPRECATED (v1.7): health_check, stats → system_status
            // ================================================================
            "health_check" => {
                warn!("Tool 'health_check' is deprecated in v1.7. Use 'system_status' instead.");
                tools::maintenance::execute_system_status(&self.storage, &self.cognitive, request.arguments).await
            }
            "stats" => {
                warn!("Tool 'stats' is deprecated in v1.7. Use 'system_status' instead.");
                tools::maintenance::execute_system_status(&self.storage, &self.cognitive, request.arguments).await
            }

            // ================================================================
            // SYSTEM STATUS (v1.7: replaces health_check + stats)
            // ================================================================
            "system_status" => tools::maintenance::execute_system_status(&self.storage, &self.cognitive, request.arguments).await,

            "mark_reviewed" => tools::review::execute(&self.storage, request.arguments).await,

            // ================================================================
            // DEPRECATED: Search tools - redirect to unified 'search'
            // ================================================================
            "recall" | "semantic_search" | "hybrid_search" => {
                warn!("Tool '{}' is deprecated. Use 'search' instead.", request.name);
                tools::search_unified::execute(&self.storage, &self.cognitive, request.arguments).await
            }

            // ================================================================
            // DEPRECATED: Memory tools - redirect to unified 'memory'
            // ================================================================
            "get_knowledge" => {
                warn!("Tool 'get_knowledge' is deprecated. Use 'memory' with action='get' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let id = args.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        Some(serde_json::json!({
                            "action": "get",
                            "id": id
                        }))
                    }
                    None => None,
                };
                tools::memory_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "delete_knowledge" => {
                warn!("Tool 'delete_knowledge' is deprecated. Use 'memory' with action='delete' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let id = args.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        Some(serde_json::json!({
                            "action": "delete",
                            "id": id
                        }))
                    }
                    None => None,
                };
                tools::memory_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "get_memory_state" => {
                warn!("Tool 'get_memory_state' is deprecated. Use 'memory' with action='state' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let id = args.get("memory_id").cloned().unwrap_or(serde_json::Value::Null);
                        Some(serde_json::json!({
                            "action": "state",
                            "id": id
                        }))
                    }
                    None => None,
                };
                tools::memory_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }

            // ================================================================
            // DEPRECATED: Codebase tools - redirect to unified 'codebase'
            // ================================================================
            "remember_pattern" => {
                warn!("Tool 'remember_pattern' is deprecated. Use 'codebase' with action='remember_pattern' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("remember_pattern"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "remember_pattern"})),
                };
                tools::codebase_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "remember_decision" => {
                warn!("Tool 'remember_decision' is deprecated. Use 'codebase' with action='remember_decision' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("remember_decision"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "remember_decision"})),
                };
                tools::codebase_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "get_codebase_context" => {
                warn!("Tool 'get_codebase_context' is deprecated. Use 'codebase' with action='get_context' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("get_context"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "get_context"})),
                };
                tools::codebase_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }

            // ================================================================
            // DEPRECATED: Intention tools - redirect to unified 'intention'
            // ================================================================
            "set_intention" => {
                warn!("Tool 'set_intention' is deprecated. Use 'intention' with action='set' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("set"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "set"})),
                };
                tools::intention_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "check_intentions" => {
                warn!("Tool 'check_intentions' is deprecated. Use 'intention' with action='check' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("check"));
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "check"})),
                };
                tools::intention_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "complete_intention" => {
                warn!("Tool 'complete_intention' is deprecated. Use 'intention' with action='update', status='complete' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let id = args.get("intentionId").cloned().unwrap_or(serde_json::Value::Null);
                        Some(serde_json::json!({
                            "action": "update",
                            "id": id,
                            "status": "complete"
                        }))
                    }
                    None => None,
                };
                tools::intention_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "snooze_intention" => {
                warn!("Tool 'snooze_intention' is deprecated. Use 'intention' with action='update', status='snooze' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let id = args.get("intentionId").cloned().unwrap_or(serde_json::Value::Null);
                        let minutes = args.get("minutes").cloned().unwrap_or(serde_json::json!(30));
                        Some(serde_json::json!({
                            "action": "update",
                            "id": id,
                            "status": "snooze",
                            "snooze_minutes": minutes
                        }))
                    }
                    None => None,
                };
                tools::intention_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }
            "list_intentions" => {
                warn!("Tool 'list_intentions' is deprecated. Use 'intention' with action='list' instead.");
                let unified_args = match request.arguments {
                    Some(ref args) => {
                        let mut new_args = args.clone();
                        if let Some(obj) = new_args.as_object_mut() {
                            obj.insert("action".to_string(), serde_json::json!("list"));
                            if let Some(status) = obj.remove("status") {
                                obj.insert("filter_status".to_string(), status);
                            }
                        }
                        Some(new_args)
                    }
                    None => Some(serde_json::json!({"action": "list"})),
                };
                tools::intention_unified::execute(&self.storage, &self.cognitive, unified_args).await
            }

            // ================================================================
            // Neuroscience tools (internal, not in tools/list)
            // ================================================================
            "list_by_state" => tools::memory_states::execute_list(&self.storage, request.arguments).await,
            "state_stats" => tools::memory_states::execute_stats(&self.storage).await,
            "trigger_importance" => tools::tagging::execute_trigger(&self.storage, request.arguments).await,
            "find_tagged" => tools::tagging::execute_find(&self.storage, request.arguments).await,
            "tagging_stats" => tools::tagging::execute_stats(&self.storage).await,
            "match_context" => tools::context::execute(&self.storage, request.arguments).await,

            // ================================================================
            // Feedback (internal, still used by request_feedback)
            // ================================================================
            "request_feedback" => tools::feedback::execute_request_feedback(&self.storage, request.arguments).await,

            // ================================================================
            // TEMPORAL TOOLS (v1.2+)
            // ================================================================
            "memory_timeline" => tools::timeline::execute(&self.storage, request.arguments).await,
            "memory_changelog" => tools::changelog::execute(&self.storage, request.arguments).await,

            // ================================================================
            // MAINTENANCE TOOLS (v1.2+, non-deprecated)
            // ================================================================
            "consolidate" => tools::maintenance::execute_consolidate(&self.storage, request.arguments).await,
            "backup" => tools::maintenance::execute_backup(&self.storage, request.arguments).await,
            "export" => tools::maintenance::execute_export(&self.storage, request.arguments).await,
            "gc" => tools::maintenance::execute_gc(&self.storage, request.arguments).await,

            // ================================================================
            // AUTO-SAVE & DEDUP TOOLS (v1.3+)
            // ================================================================
            "importance_score" => tools::importance::execute(&self.storage, &self.cognitive, request.arguments).await,
            "find_duplicates" => tools::dedup::execute(&self.storage, request.arguments).await,

            // ================================================================
            // COGNITIVE TOOLS (v1.5+)
            // ================================================================
            "dream" => tools::dream::execute(&self.storage, &self.cognitive, request.arguments).await,
            "explore_connections" => tools::explore::execute(&self.storage, &self.cognitive, request.arguments).await,
            "predict" => tools::predict::execute(&self.storage, &self.cognitive, request.arguments).await,
            "restore" => tools::restore::execute(&self.storage, request.arguments).await,

            name => {
                return Err(JsonRpcError::method_not_found_with_message(&format!(
                    "Unknown tool: {}",
                    name
                )));
            }
        };

        let response = match result {
            Ok(content) => {
                let call_result = CallToolResult {
                    content: vec![crate::protocol::messages::ToolResultContent {
                        content_type: "text".to_string(),
                        text: serde_json::to_string_pretty(&content).unwrap_or_else(|_| content.to_string()),
                    }],
                    is_error: Some(false),
                };
                serde_json::to_value(call_result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
            Err(e) => {
                let call_result = CallToolResult {
                    content: vec![crate::protocol::messages::ToolResultContent {
                        content_type: "text".to_string(),
                        text: serde_json::json!({ "error": e }).to_string(),
                    }],
                    is_error: Some(true),
                };
                serde_json::to_value(call_result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
        };

        // Inline consolidation trigger: uses ConsolidationScheduler instead of fixed count
        let count = self.tool_call_count.fetch_add(1, Ordering::Relaxed) + 1;
        let should_consolidate = self.cognitive.try_lock()
            .ok()
            .map(|cog| cog.consolidation_scheduler.should_consolidate())
            .unwrap_or(count.is_multiple_of(100)); // Fallback to count-based if lock unavailable

        if should_consolidate {
            let storage_clone = Arc::clone(&self.storage);
            let cognitive_clone = Arc::clone(&self.cognitive);
            tokio::spawn(async move {
                // Expire labile reconsolidation windows
                if let Ok(mut cog) = cognitive_clone.try_lock() {
                    let _expired = cog.reconsolidation.reconsolidate_expired();
                }

                if let Ok(mut storage) = storage_clone.try_lock() {
                    match storage.run_consolidation() {
                        Ok(result) => {
                            tracing::info!(
                                tool_calls = count,
                                decay_applied = result.decay_applied,
                                duplicates_merged = result.duplicates_merged,
                                activations_computed = result.activations_computed,
                                duration_ms = result.duration_ms,
                                "Inline consolidation triggered (scheduler)"
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Inline consolidation failed: {}", e);
                        }
                    }
                }
            });
        }

        response
    }

    /// Handle resources/list request
    async fn handle_resources_list(&self) -> Result<serde_json::Value, JsonRpcError> {
        let resources = vec![
            // Memory resources
            ResourceDescription {
                uri: "memory://stats".to_string(),
                name: "Memory Statistics".to_string(),
                description: Some("Current memory system statistics and health status".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "memory://recent".to_string(),
                name: "Recent Memories".to_string(),
                description: Some("Recently added memories (last 10)".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "memory://decaying".to_string(),
                name: "Decaying Memories".to_string(),
                description: Some("Memories with low retention that need review".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "memory://due".to_string(),
                name: "Due for Review".to_string(),
                description: Some("Memories scheduled for review today".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            // Codebase resources
            ResourceDescription {
                uri: "codebase://structure".to_string(),
                name: "Codebase Structure".to_string(),
                description: Some("Remembered project structure and organization".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "codebase://patterns".to_string(),
                name: "Code Patterns".to_string(),
                description: Some("Remembered code patterns and conventions".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "codebase://decisions".to_string(),
                name: "Architectural Decisions".to_string(),
                description: Some("Remembered architectural and design decisions".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            // Consolidation resources
            ResourceDescription {
                uri: "memory://insights".to_string(),
                name: "Consolidation Insights".to_string(),
                description: Some("Insights generated during memory consolidation".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "memory://consolidation-log".to_string(),
                name: "Consolidation Log".to_string(),
                description: Some("History of memory consolidation runs".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            // Prospective memory resources
            ResourceDescription {
                uri: "memory://intentions".to_string(),
                name: "Active Intentions".to_string(),
                description: Some("Future intentions (prospective memory) waiting to be triggered".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            ResourceDescription {
                uri: "memory://intentions/due".to_string(),
                name: "Triggered Intentions".to_string(),
                description: Some("Intentions that have been triggered or are overdue".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ];

        let result = ListResourcesResult { resources };
        serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
    }

    /// Handle resources/read request
    async fn handle_resources_read(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let request: ReadResourceRequest = match params {
            Some(p) => serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?,
            None => return Err(JsonRpcError::invalid_params("Missing resource URI")),
        };

        let uri = &request.uri;
        let content = if uri.starts_with("memory://") {
            resources::memory::read(&self.storage, uri).await
        } else if uri.starts_with("codebase://") {
            resources::codebase::read(&self.storage, uri).await
        } else {
            Err(format!("Unknown resource scheme: {}", uri))
        };

        match content {
            Ok(text) => {
                let result = ReadResourceResult {
                    contents: vec![crate::protocol::messages::ResourceContent {
                        uri: uri.clone(),
                        mime_type: Some("application/json".to_string()),
                        text: Some(text),
                        blob: None,
                    }],
                };
                serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(&e.to_string()))
            }
            Err(e) => Err(JsonRpcError::internal_error(&e)),
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Mutex<Storage>>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(Mutex::new(storage)), dir)
    }

    /// Create a test server with temporary storage
    async fn test_server() -> (McpServer, TempDir) {
        let (storage, dir) = test_storage().await;
        let cognitive = Arc::new(Mutex::new(CognitiveEngine::new()));
        let server = McpServer::new(storage, cognitive);
        (server, dir)
    }

    /// Create a JSON-RPC request
    fn make_request(method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: method.to_string(),
            params,
        }
    }

    // ========================================================================
    // INITIALIZATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_initialize_sets_initialized_flag() {
        let (mut server, _dir) = test_server().await;
        assert!(!server.initialized);

        let request = make_request("initialize", Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        })));

        let response = server.handle_request(request).await;
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert!(server.initialized);
    }

    #[tokio::test]
    async fn test_initialize_returns_server_info() {
        let (mut server, _dir) = test_server().await;
        // Send with current protocol version to get it back
        let params = serde_json::json!({
            "protocolVersion": MCP_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        });
        let request = make_request("initialize", Some(params));

        let response = server.handle_request(request).await.unwrap();
        let result = response.result.unwrap();

        assert_eq!(result["protocolVersion"], MCP_VERSION);
        assert_eq!(result["serverInfo"]["name"], "vestige");
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["capabilities"]["resources"].is_object());
        assert!(result["instructions"].is_string());
    }

    #[tokio::test]
    async fn test_initialize_with_default_params() {
        let (mut server, _dir) = test_server().await;
        let request = make_request("initialize", None);

        let response = server.handle_request(request).await.unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    // ========================================================================
    // UNINITIALIZED SERVER TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_request_before_initialize_returns_error() {
        let (mut server, _dir) = test_server().await;

        let request = make_request("tools/list", None);
        let response = server.handle_request(request).await.unwrap();

        assert!(response.result.is_none());
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32003); // ServerNotInitialized
    }

    #[tokio::test]
    async fn test_ping_before_initialize_returns_error() {
        let (mut server, _dir) = test_server().await;

        let request = make_request("ping", None);
        let response = server.handle_request(request).await.unwrap();

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32003);
    }

    // ========================================================================
    // NOTIFICATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_initialized_notification_returns_none() {
        let (mut server, _dir) = test_server().await;

        // First initialize
        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        // Send initialized notification
        let notification = make_request("notifications/initialized", None);
        let response = server.handle_request(notification).await;

        // Notifications should return None
        assert!(response.is_none());
    }

    // ========================================================================
    // TOOLS/LIST TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_tools_list_returns_all_tools() {
        let (mut server, _dir) = test_server().await;

        // Initialize first
        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("tools/list", None);
        let response = server.handle_request(request).await.unwrap();

        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();

        // v1.7: 18 tools (4 unified + 1 core + 2 temporal + 5 maintenance + 2 auto-save + 3 cognitive + 1 restore)
        assert_eq!(tools.len(), 18, "Expected exactly 18 tools in v1.7+");

        let tool_names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();

        // Unified tools
        assert!(tool_names.contains(&"search"));
        assert!(tool_names.contains(&"memory"));
        assert!(tool_names.contains(&"codebase"));
        assert!(tool_names.contains(&"intention"));

        // Core memory (smart_ingest absorbs ingest + checkpoint in v1.7)
        assert!(tool_names.contains(&"smart_ingest"));
        assert!(!tool_names.contains(&"ingest"), "ingest should be removed in v1.7");
        assert!(!tool_names.contains(&"session_checkpoint"), "session_checkpoint should be removed in v1.7");

        // Feedback merged into memory tool (v1.7)
        assert!(!tool_names.contains(&"promote_memory"), "promote_memory should be removed in v1.7");
        assert!(!tool_names.contains(&"demote_memory"), "demote_memory should be removed in v1.7");

        // Temporal tools (v1.2)
        assert!(tool_names.contains(&"memory_timeline"));
        assert!(tool_names.contains(&"memory_changelog"));

        // Maintenance tools (v1.7: system_status replaces health_check + stats)
        assert!(tool_names.contains(&"system_status"));
        assert!(!tool_names.contains(&"health_check"), "health_check should be removed in v1.7");
        assert!(!tool_names.contains(&"stats"), "stats should be removed in v1.7");
        assert!(tool_names.contains(&"consolidate"));
        assert!(tool_names.contains(&"backup"));
        assert!(tool_names.contains(&"export"));
        assert!(tool_names.contains(&"gc"));

        // Auto-save & dedup tools (v1.3)
        assert!(tool_names.contains(&"importance_score"));
        assert!(tool_names.contains(&"find_duplicates"));

        // Cognitive tools (v1.5)
        assert!(tool_names.contains(&"dream"));
        assert!(tool_names.contains(&"explore_connections"));
        assert!(tool_names.contains(&"predict"));
        assert!(tool_names.contains(&"restore"));
    }

    #[tokio::test]
    async fn test_tools_have_descriptions_and_schemas() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("tools/list", None);
        let response = server.handle_request(request).await.unwrap();

        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();

        for tool in tools {
            assert!(tool["name"].is_string(), "Tool should have a name");
            assert!(tool["description"].is_string(), "Tool should have a description");
            assert!(tool["inputSchema"].is_object(), "Tool should have an input schema");
        }
    }

    // ========================================================================
    // RESOURCES/LIST TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_resources_list_returns_all_resources() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("resources/list", None);
        let response = server.handle_request(request).await.unwrap();

        let result = response.result.unwrap();
        let resources = result["resources"].as_array().unwrap();

        // Verify expected resources are present
        let resource_uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();

        assert!(resource_uris.contains(&"memory://stats"));
        assert!(resource_uris.contains(&"memory://recent"));
        assert!(resource_uris.contains(&"memory://decaying"));
        assert!(resource_uris.contains(&"memory://due"));
        assert!(resource_uris.contains(&"memory://intentions"));
        assert!(resource_uris.contains(&"codebase://structure"));
        assert!(resource_uris.contains(&"codebase://patterns"));
        assert!(resource_uris.contains(&"codebase://decisions"));
    }

    #[tokio::test]
    async fn test_resources_have_descriptions() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("resources/list", None);
        let response = server.handle_request(request).await.unwrap();

        let result = response.result.unwrap();
        let resources = result["resources"].as_array().unwrap();

        for resource in resources {
            assert!(resource["uri"].is_string(), "Resource should have a URI");
            assert!(resource["name"].is_string(), "Resource should have a name");
            assert!(resource["description"].is_string(), "Resource should have a description");
        }
    }

    // ========================================================================
    // UNKNOWN METHOD TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_unknown_method_returns_error() {
        let (mut server, _dir) = test_server().await;

        // Initialize first
        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("unknown/method", None);
        let response = server.handle_request(request).await.unwrap();

        assert!(response.result.is_none());
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601); // MethodNotFound
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("tools/call", Some(serde_json::json!({
            "name": "nonexistent_tool",
            "arguments": {}
        })));

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    // ========================================================================
    // PING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_ping_returns_empty_object() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("ping", None);
        let response = server.handle_request(request).await.unwrap();

        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap(), serde_json::json!({}));
    }

    // ========================================================================
    // TOOLS/CALL TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_tools_call_missing_params_returns_error() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("tools/call", None);
        let response = server.handle_request(request).await.unwrap();

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602); // InvalidParams
    }

    #[tokio::test]
    async fn test_tools_call_invalid_params_returns_error() {
        let (mut server, _dir) = test_server().await;

        let init_request = make_request("initialize", None);
        server.handle_request(init_request).await;

        let request = make_request("tools/call", Some(serde_json::json!({
            "invalid": "params"
        })));

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }
}
