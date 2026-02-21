//! Session Checkpoint Tool
//!
//! Batch smart_ingest for session-end saves. Accepts up to 20 items
//! in a single call, routing each through Prediction Error Gating.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;


use vestige_core::{IngestInput, Storage};

/// Input schema for session_checkpoint tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "description": "Array of items to save (max 20). Each goes through Prediction Error Gating.",
                "maxItems": 20,
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The content to remember"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tags for categorization"
                        },
                        "node_type": {
                            "type": "string",
                            "description": "Type: fact, concept, event, person, place, note, pattern, decision",
                            "default": "fact"
                        },
                        "source": {
                            "type": "string",
                            "description": "Source reference"
                        }
                    },
                    "required": ["content"]
                }
            }
        },
        "required": ["items"]
    })
}

#[derive(Debug, Deserialize)]
struct CheckpointArgs {
    items: Vec<CheckpointItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointItem {
    content: String,
    tags: Option<Vec<String>>,
    node_type: Option<String>,
    source: Option<String>,
}

pub async fn execute(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: CheckpointArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    if args.items.is_empty() {
        return Err("Items array cannot be empty".to_string());
    }

    if args.items.len() > 20 {
        return Err("Maximum 20 items per checkpoint".to_string());
    }

    let mut results = Vec::new();
    let mut created = 0u32;
    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for (i, item) in args.items.into_iter().enumerate() {
        if item.content.trim().is_empty() {
            results.push(serde_json::json!({
                "index": i,
                "status": "skipped",
                "reason": "Empty content"
            }));
            skipped += 1;
            continue;
        }

        let input = IngestInput {
            content: item.content,
            node_type: item.node_type.unwrap_or_else(|| "fact".to_string()),
            source: item.source,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: item.tags.unwrap_or_default(),
            valid_from: None,
            valid_until: None,
        };

        #[cfg(all(feature = "embeddings", feature = "vector-search"))]
        {
            match storage.smart_ingest(input) {
                Ok(result) => {
                    match result.decision.as_str() {
                        "create" | "supersede" | "replace" => created += 1,
                        "update" | "reinforce" | "merge" | "add_context" => updated += 1,
                        _ => created += 1,
                    }
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": result.decision,
                        "nodeId": result.node.id,
                        "similarity": result.similarity,
                        "reason": result.reason
                    }));
                }
                Err(e) => {
                    errors += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "error",
                        "reason": e.to_string()
                    }));
                }
            }
        }

        #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
        {
            match storage.ingest(input) {
                Ok(node) => {
                    created += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "saved",
                        "decision": "create",
                        "nodeId": node.id,
                        "reason": "Embeddings not available - used regular ingest"
                    }));
                }
                Err(e) => {
                    errors += 1;
                    results.push(serde_json::json!({
                        "index": i,
                        "status": "error",
                        "reason": e.to_string()
                    }));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "success": errors == 0,
        "summary": {
            "total": results.len(),
            "created": created,
            "updated": updated,
            "skipped": skipped,
            "errors": errors
        },
        "results": results
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema = schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["items"].is_object());
    }

    #[tokio::test]
    async fn test_empty_items_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, Some(serde_json::json!({ "items": [] }))).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_ingest() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [
                    { "content": "First checkpoint item", "tags": ["test"] },
                    { "content": "Second checkpoint item", "tags": ["test"] }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["total"], 2);
    }

    #[tokio::test]
    async fn test_skips_empty_content() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [
                    { "content": "Valid item" },
                    { "content": "" },
                    { "content": "Another valid item" }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["skipped"], 1);
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_exceeds_20_items_fails() {
        let (storage, _dir) = test_storage().await;
        let items: Vec<serde_json::Value> = (0..21)
            .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
            .collect();
        let result = execute(&storage, Some(serde_json::json!({ "items": items }))).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum 20 items"));
    }

    #[tokio::test]
    async fn test_exactly_20_items_succeeds() {
        let (storage, _dir) = test_storage().await;
        let items: Vec<serde_json::Value> = (0..20)
            .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
            .collect();
        let result = execute(&storage, Some(serde_json::json!({ "items": items }))).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["total"], 20);
    }

    #[tokio::test]
    async fn test_skips_whitespace_only_content() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [
                    { "content": "   \t\n  " },
                    { "content": "Valid content" }
                ]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["skipped"], 1);
        assert_eq!(value["summary"]["created"], 1);
    }

    #[tokio::test]
    async fn test_single_item_succeeds() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [{ "content": "Single item" }]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["total"], 1);
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_items_with_all_fields() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [{
                    "content": "Full fields item",
                    "tags": ["test", "checkpoint"],
                    "node_type": "decision",
                    "source": "test-suite"
                }]
            })),
        )
        .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["summary"]["created"], 1);
    }

    #[tokio::test]
    async fn test_results_array_matches_items() {
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [
                    { "content": "First" },
                    { "content": "" },
                    { "content": "Third" }
                ]
            })),
        )
        .await;
        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["index"], 0);
        assert_eq!(results[1]["index"], 1);
        assert_eq!(results[1]["status"], "skipped");
        assert_eq!(results[2]["index"], 2);
    }

    #[tokio::test]
    async fn test_success_false_when_errors() {
        // All items empty = all skipped = 0 errors = success true
        let (storage, _dir) = test_storage().await;
        let result = execute(
            &storage,
            Some(serde_json::json!({
                "items": [
                    { "content": "" },
                    { "content": "   " }
                ]
            })),
        )
        .await;
        let value = result.unwrap();
        assert_eq!(value["success"], true); // skipped ≠ errors
        assert_eq!(value["summary"]["errors"], 0);
        assert_eq!(value["summary"]["skipped"], 2);
    }
}
