//! Unified Memory Tool
//!
//! Merges get_knowledge, delete_knowledge, and get_memory_state into a single
//! `memory` tool with action-based dispatch.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use vestige_core::{MemoryState, Storage};

// Accessibility thresholds based on retention strength
const ACCESSIBILITY_ACTIVE: f64 = 0.7;
const ACCESSIBILITY_DORMANT: f64 = 0.4;
const ACCESSIBILITY_SILENT: f64 = 0.1;

/// Compute accessibility score from memory strengths
/// Combines retention, retrieval, and storage strengths
fn compute_accessibility(retention: f64, retrieval: f64, storage: f64) -> f64 {
    // Weighted combination: retention is most important for accessibility
    retention * 0.5 + retrieval * 0.3 + storage * 0.2
}

/// Determine memory state from accessibility score
fn state_from_accessibility(accessibility: f64) -> MemoryState {
    if accessibility >= ACCESSIBILITY_ACTIVE {
        MemoryState::Active
    } else if accessibility >= ACCESSIBILITY_DORMANT {
        MemoryState::Dormant
    } else if accessibility >= ACCESSIBILITY_SILENT {
        MemoryState::Silent
    } else {
        MemoryState::Unavailable
    }
}

/// Input schema for the unified memory tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["get", "delete", "state"],
                "description": "Action to perform: 'get' retrieves full memory node, 'delete' removes memory, 'state' returns accessibility state"
            },
            "id": {
                "type": "string",
                "description": "The ID of the memory node"
            }
        },
        "required": ["action", "id"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryArgs {
    action: String,
    id: String,
}

/// Execute the unified memory tool
pub async fn execute(
    storage: &Arc<Mutex<Storage>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: MemoryArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // Validate UUID format
    uuid::Uuid::parse_str(&args.id).map_err(|_| "Invalid memory ID format".to_string())?;

    match args.action.as_str() {
        "get" => execute_get(storage, &args.id).await,
        "delete" => execute_delete(storage, &args.id).await,
        "state" => execute_state(storage, &args.id).await,
        _ => Err(format!(
            "Invalid action '{}'. Must be one of: get, delete, state",
            args.action
        )),
    }
}

/// Get full memory node with all metadata
async fn execute_get(storage: &Arc<Mutex<Storage>>, id: &str) -> Result<Value, String> {
    let storage = storage.lock().await;
    let node = storage.get_node(id).map_err(|e| e.to_string())?;

    match node {
        Some(n) => Ok(serde_json::json!({
            "action": "get",
            "found": true,
            "node": {
                "id": n.id,
                "content": n.content,
                "nodeType": n.node_type,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "lastAccessed": n.last_accessed.to_rfc3339(),
                "stability": n.stability,
                "difficulty": n.difficulty,
                "reps": n.reps,
                "lapses": n.lapses,
                "storageStrength": n.storage_strength,
                "retrievalStrength": n.retrieval_strength,
                "retentionStrength": n.retention_strength,
                "sentimentScore": n.sentiment_score,
                "sentimentMagnitude": n.sentiment_magnitude,
                "nextReview": n.next_review.map(|d| d.to_rfc3339()),
                "source": n.source,
                "tags": n.tags,
                "hasEmbedding": n.has_embedding,
                "embeddingModel": n.embedding_model,
            }
        })),
        None => Ok(serde_json::json!({
            "action": "get",
            "found": false,
            "nodeId": id,
            "message": "Memory not found",
        })),
    }
}

/// Delete a memory and return success status
async fn execute_delete(storage: &Arc<Mutex<Storage>>, id: &str) -> Result<Value, String> {
    let mut storage = storage.lock().await;
    let deleted = storage.delete_node(id).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "action": "delete",
        "success": deleted,
        "nodeId": id,
        "message": if deleted { "Memory deleted successfully" } else { "Memory not found" },
    }))
}

/// Get accessibility state of a memory (Active/Dormant/Silent/Unavailable)
async fn execute_state(storage: &Arc<Mutex<Storage>>, id: &str) -> Result<Value, String> {
    let storage = storage.lock().await;

    // Get the memory
    let memory = storage
        .get_node(id)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or("Memory not found")?;

    // Calculate accessibility score
    let accessibility = compute_accessibility(
        memory.retention_strength,
        memory.retrieval_strength,
        memory.storage_strength,
    );

    // Determine state
    let state = state_from_accessibility(accessibility);

    let state_description = match state {
        MemoryState::Active => "Easily retrievable - this memory is fresh and accessible",
        MemoryState::Dormant => "Retrievable with effort - may need cues to recall",
        MemoryState::Silent => "Difficult to retrieve - exists but hard to access",
        MemoryState::Unavailable => "Cannot be retrieved - needs significant reinforcement",
    };

    Ok(serde_json::json!({
        "action": "state",
        "memoryId": id,
        "content": memory.content,
        "state": format!("{:?}", state),
        "accessibility": accessibility,
        "description": state_description,
        "components": {
            "retentionStrength": memory.retention_strength,
            "retrievalStrength": memory.retrieval_strength,
            "storageStrength": memory.storage_strength
        },
        "thresholds": {
            "active": ACCESSIBILITY_ACTIVE,
            "dormant": ACCESSIBILITY_DORMANT,
            "silent": ACCESSIBILITY_SILENT
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_thresholds() {
        // Test Active state
        let accessibility = compute_accessibility(0.9, 0.8, 0.7);
        assert!(accessibility >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(state_from_accessibility(accessibility), MemoryState::Active));

        // Test Dormant state
        let accessibility = compute_accessibility(0.5, 0.5, 0.5);
        assert!(accessibility >= ACCESSIBILITY_DORMANT && accessibility < ACCESSIBILITY_ACTIVE);
        assert!(matches!(state_from_accessibility(accessibility), MemoryState::Dormant));

        // Test Silent state
        let accessibility = compute_accessibility(0.2, 0.2, 0.2);
        assert!(accessibility >= ACCESSIBILITY_SILENT && accessibility < ACCESSIBILITY_DORMANT);
        assert!(matches!(state_from_accessibility(accessibility), MemoryState::Silent));

        // Test Unavailable state
        let accessibility = compute_accessibility(0.05, 0.05, 0.05);
        assert!(accessibility < ACCESSIBILITY_SILENT);
        assert!(matches!(state_from_accessibility(accessibility), MemoryState::Unavailable));
    }

    #[test]
    fn test_schema_structure() {
        let schema = schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["id"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["action", "id"]));
    }

    // === INTEGRATION TESTS ===

    async fn test_storage() -> (Arc<Mutex<Storage>>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(Mutex::new(storage)), dir)
    }

    async fn ingest_memory(storage: &Arc<Mutex<Storage>>) -> String {
        let mut s = storage.lock().await;
        let node = s
            .ingest(vestige_core::IngestInput {
                content: "Memory unified test content".to_string(),
                node_type: "fact".to_string(),
                source: Some("test".to_string()),
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test-tag".to_string()],
                valid_from: None,
                valid_until: None,
            })
            .unwrap();
        node.id
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_invalid_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "invalid", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid action"));
    }

    #[tokio::test]
    async fn test_invalid_uuid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "get", "id": "not-a-uuid" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_get_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "get");
        assert_eq!(value["found"], true);
        assert_eq!(value["node"]["id"], id);
        assert_eq!(value["node"]["content"], "Memory unified test content");
        assert_eq!(value["node"]["nodeType"], "fact");
        assert!(value["node"]["createdAt"].is_string());
        assert!(value["node"]["tags"].is_array());
    }

    #[tokio::test]
    async fn test_get_nonexistent_memory() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "get", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["found"], false);
        assert_eq!(value["message"], "Memory not found");
    }

    #[tokio::test]
    async fn test_delete_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "delete", "id": id });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "delete");
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_memory() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "delete", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], false);
        assert!(value["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_then_get_returns_not_found() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let del_args = serde_json::json!({ "action": "delete", "id": id });
        execute(&storage, Some(del_args)).await.unwrap();
        let get_args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, Some(get_args)).await;
        let value = result.unwrap();
        assert_eq!(value["found"], false);
    }

    #[tokio::test]
    async fn test_state_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "state", "id": id });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "state");
        assert_eq!(value["memoryId"], id);
        assert!(value["accessibility"].is_number());
        assert!(value["state"].is_string());
        assert!(value["description"].is_string());
        assert!(value["components"]["retentionStrength"].is_number());
        assert!(value["components"]["retrievalStrength"].is_number());
        assert!(value["components"]["storageStrength"].is_number());
        assert_eq!(value["thresholds"]["active"], 0.7);
        assert_eq!(value["thresholds"]["dormant"], 0.4);
        assert_eq!(value["thresholds"]["silent"], 0.1);
    }

    #[tokio::test]
    async fn test_state_nonexistent_memory_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "state", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_accessibility_boundary_active() {
        // Exactly at active threshold
        let a = compute_accessibility(1.0, 0.7, 0.5);
        assert!(a >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(state_from_accessibility(a), MemoryState::Active));
    }

    #[test]
    fn test_accessibility_boundary_zero() {
        let a = compute_accessibility(0.0, 0.0, 0.0);
        assert_eq!(a, 0.0);
        assert!(matches!(state_from_accessibility(a), MemoryState::Unavailable));
    }
}
