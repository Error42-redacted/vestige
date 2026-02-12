//! Memory Changelog Tool
//!
//! View audit trail of memory changes.
//! Per-memory mode: state transitions for a single memory.
//! System-wide mode: consolidations + recent state transitions.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use vestige_core::Storage;

/// Input schema for memory_changelog tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "memory_id": {
                "type": "string",
                "description": "Scope to a single memory's audit trail. If omitted, returns system-wide changelog."
            },
            "start": {
                "type": "string",
                "description": "Start of time range (ISO 8601). Only used in system-wide mode."
            },
            "end": {
                "type": "string",
                "description": "End of time range (ISO 8601). Only used in system-wide mode."
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of entries (default: 20, max: 100)",
                "default": 20,
                "minimum": 1,
                "maximum": 100
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangelogArgs {
    memory_id: Option<String>,
    #[allow(dead_code)]
    start: Option<String>,
    #[allow(dead_code)]
    end: Option<String>,
    limit: Option<i32>,
}

/// Execute memory_changelog tool
pub async fn execute(
    storage: &Arc<Mutex<Storage>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: ChangelogArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => ChangelogArgs {
            memory_id: None,
            start: None,
            end: None,
            limit: None,
        },
    };

    let limit = args.limit.unwrap_or(20).clamp(1, 100);
    let storage = storage.lock().await;

    if let Some(ref memory_id) = args.memory_id {
        // Per-memory mode: state transitions for a specific memory
        execute_per_memory(&storage, memory_id, limit)
    } else {
        // System-wide mode: consolidations + recent transitions
        execute_system_wide(&storage, limit)
    }
}

/// Per-memory changelog: state transition audit trail
fn execute_per_memory(
    storage: &Storage,
    memory_id: &str,
    limit: i32,
) -> Result<Value, String> {
    // Validate UUID format
    Uuid::parse_str(memory_id)
        .map_err(|_| format!("Invalid memory_id '{}'. Must be a valid UUID.", memory_id))?;

    // Get the memory for context
    let node = storage
        .get_node(memory_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Memory '{}' not found.", memory_id))?;

    // Get state transitions
    let transitions = storage
        .get_state_transitions(memory_id, limit)
        .map_err(|e| e.to_string())?;

    let formatted_transitions: Vec<Value> = transitions
        .iter()
        .map(|t| {
            serde_json::json!({
                "fromState": t.from_state,
                "toState": t.to_state,
                "reasonType": t.reason_type,
                "reasonData": t.reason_data,
                "timestamp": t.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "tool": "memory_changelog",
        "mode": "per_memory",
        "memoryId": memory_id,
        "memoryContent": node.content,
        "memoryType": node.node_type,
        "currentRetention": node.retention_strength,
        "totalTransitions": formatted_transitions.len(),
        "transitions": formatted_transitions,
    }))
}

/// System-wide changelog: consolidations + recent state transitions
fn execute_system_wide(
    storage: &Storage,
    limit: i32,
) -> Result<Value, String> {
    // Get consolidation history
    let consolidations = storage
        .get_consolidation_history(limit)
        .map_err(|e| e.to_string())?;

    // Get recent state transitions across all memories
    let transitions = storage
        .get_recent_state_transitions(limit)
        .map_err(|e| e.to_string())?;

    // Build unified event list
    let mut events: Vec<(DateTime<Utc>, Value)> = Vec::new();

    for c in &consolidations {
        events.push((
            c.completed_at,
            serde_json::json!({
                "type": "consolidation",
                "timestamp": c.completed_at.to_rfc3339(),
                "durationMs": c.duration_ms,
                "memoriesReplayed": c.memories_replayed,
                "connectionFound": c.connections_found,
                "connectionsStrengthened": c.connections_strengthened,
                "connectionsPruned": c.connections_pruned,
                "insightsGenerated": c.insights_generated,
            }),
        ));
    }

    for t in &transitions {
        events.push((
            t.timestamp,
            serde_json::json!({
                "type": "state_transition",
                "timestamp": t.timestamp.to_rfc3339(),
                "memoryId": t.memory_id,
                "fromState": t.from_state,
                "toState": t.to_state,
                "reasonType": t.reason_type,
                "reasonData": t.reason_data,
            }),
        ));
    }

    // Sort by timestamp descending
    events.sort_by(|a, b| b.0.cmp(&a.0));

    // Truncate to limit
    events.truncate(limit as usize);

    let formatted_events: Vec<Value> = events.into_iter().map(|(_, v)| v).collect();

    Ok(serde_json::json!({
        "tool": "memory_changelog",
        "mode": "system_wide",
        "totalEvents": formatted_events.len(),
        "events": formatted_events,
    }))
}
