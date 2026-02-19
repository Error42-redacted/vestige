//! Smart Ingest Tool
//!
//! Intelligent memory ingestion with Prediction Error Gating.
//! Automatically decides whether to create, update, or supersede memories
//! based on semantic similarity to existing content.
//!
//! This solves the "bad vs good similar memory" problem by:
//! - Detecting when new content is similar to existing memories
//! - Updating existing memories when appropriate (low prediction error)
//! - Creating new memories when content is substantially different (high PE)
//! - Superseding demoted/outdated memories with better alternatives
//!
//! v1.5.0: Enhanced with cognitive pipeline:
//!   Pre-ingest: importance scoring (4-channel) + intent detection → auto-tag
//!   Post-ingest: synaptic tagging + novelty model update + hippocampal indexing

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{
    ContentType, ImportanceContext, ImportanceEventType, ImportanceEvent, IngestInput, Storage,
};

/// Input schema for smart_ingest tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "content": {
                "type": "string",
                "description": "The content to remember. Will be compared against existing memories."
            },
            "node_type": {
                "type": "string",
                "description": "Type of knowledge: fact, concept, event, person, place, note, pattern, decision",
                "default": "fact"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Tags for categorization"
            },
            "source": {
                "type": "string",
                "description": "Source or reference for this knowledge"
            },
            "forceCreate": {
                "type": "boolean",
                "description": "Force creation of a new memory even if similar content exists",
                "default": false
            }
        },
        "required": ["content"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmartIngestArgs {
    content: String,
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
    force_create: Option<bool>,
}

pub async fn execute(
    storage: &Arc<Mutex<Storage>>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SmartIngestArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // Validate content
    if args.content.trim().is_empty() {
        return Err("Content cannot be empty".to_string());
    }

    if args.content.len() > 1_000_000 {
        return Err("Content too large (max 1MB)".to_string());
    }

    // ====================================================================
    // COGNITIVE PRE-INGEST: importance scoring + intent detection + content analysis
    // ====================================================================
    let mut importance_composite = 0.0_f64;
    let mut tags = args.tags.unwrap_or_default();

    if let Ok(cog) = cognitive.try_lock() {
        // 4A. Full 4-channel importance scoring
        let context = ImportanceContext::current();
        let importance = cog.importance_signals.compute_importance(&args.content, &context);
        importance_composite = importance.composite;

        // 4B. Intent detection → auto-tag
        let intent_result = cog.intent_detector.detect_intent();
        if intent_result.confidence > 0.5 {
            let intent_tag = format!("intent:{:?}", intent_result.primary_intent);
            // Truncate long intent tags
            let intent_tag = if intent_tag.len() > 50 {
                format!("{}...", &intent_tag[..47])
            } else {
                intent_tag
            };
            tags.push(intent_tag);
        }

        // 4D. Adaptive embedding — detect content type for logging
        let _content_type = ContentType::detect(&args.content);
    }

    let input = IngestInput {
        content: args.content.clone(),
        node_type: args.node_type.unwrap_or_else(|| "fact".to_string()),
        source: args.source,
        sentiment_score: 0.0,
        // Store importance composite as sentiment_magnitude for FSRS encoding boost
        sentiment_magnitude: importance_composite,
        tags,
        valid_from: None,
        valid_until: None,
    };

    // ====================================================================
    // INGEST (storage lock)
    // ====================================================================
    let mut storage_guard = storage.lock().await;

    // Check if force_create is enabled
    if args.force_create.unwrap_or(false) {
        let node = storage_guard.ingest(input).map_err(|e| e.to_string())?;
        let node_id = node.id.clone();
        let node_content = node.content.clone();
        let node_type = node.node_type.clone();
        let has_embedding = node.has_embedding.unwrap_or(false);
        drop(storage_guard);

        // Post-ingest cognitive side effects
        run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

        return Ok(serde_json::json!({
            "success": true,
            "decision": "create",
            "nodeId": node_id,
            "message": "Memory created (force_create=true)",
            "hasEmbedding": has_embedding,
            "predictionError": 1.0,
            "importanceScore": importance_composite,
            "reason": "Forced creation - skipped similarity check"
        }));
    }

    // Use smart ingest with prediction error gating
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let result = storage_guard.smart_ingest(input).map_err(|e| e.to_string())?;
        let node_id = result.node.id.clone();
        let node_content = result.node.content.clone();
        let node_type = result.node.node_type.clone();
        let has_embedding = result.node.has_embedding.unwrap_or(false);
        drop(storage_guard);

        // Post-ingest cognitive side effects
        run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

        Ok(serde_json::json!({
            "success": true,
            "decision": result.decision,
            "nodeId": node_id,
            "message": format!("Smart ingest complete: {}", result.reason),
            "hasEmbedding": has_embedding,
            "similarity": result.similarity,
            "predictionError": result.prediction_error,
            "supersededId": result.superseded_id,
            "importanceScore": importance_composite,
            "reason": result.reason,
            "explanation": match result.decision.as_str() {
                "create" => "Created new memory - content was different enough from existing memories",
                "update" => "Updated existing memory - content was similar to an existing memory",
                "reinforce" => "Reinforced existing memory - content was nearly identical",
                "supersede" => "Superseded old memory - new content is an improvement/correction",
                "merge" => "Merged with related memories - content connects multiple topics",
                "replace" => "Replaced existing memory content entirely",
                "add_context" => "Added new content as context to existing memory",
                _ => "Memory processed successfully"
            }
        }))
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let node = storage_guard.ingest(input).map_err(|e| e.to_string())?;
        let node_id = node.id.clone();
        let node_content = node.content.clone();
        let node_type = node.node_type.clone();
        drop(storage_guard);

        run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

        Ok(serde_json::json!({
            "success": true,
            "decision": "create",
            "nodeId": node_id,
            "message": "Memory created (smart ingest requires embeddings feature)",
            "hasEmbedding": false,
            "predictionError": 1.0,
            "importanceScore": importance_composite,
            "reason": "Embeddings not available - used regular ingest"
        }))
    }
}

/// Cognitive post-ingest side effects: synaptic tagging, novelty update, hippocampal indexing.
///
/// Uses try_lock() for non-blocking access. If cognitive is locked, side effects are skipped.
fn run_post_ingest(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    node_id: &str,
    content: &str,
    node_type: &str,
    importance_composite: f64,
) {
    if let Ok(mut cog) = cognitive.try_lock() {
        // 4C. Synaptic tagging for retroactive capture
        if importance_composite > 0.3 {
            cog.synaptic_tagging.tag_memory(node_id);
            if importance_composite > 0.7 {
                // High importance → trigger PRP for nearby memories
                let event = ImportanceEvent::for_memory(node_id, ImportanceEventType::NoveltySpike);
                let _capture = cog.synaptic_tagging.trigger_prp(event);
            }
        }

        // 4E. Update novelty model with new content
        cog.importance_signals.learn_content(content);

        // 4F. Record in hippocampal index
        let _ = cog.hippocampal_index.index_memory(
            node_id,
            content,
            node_type,
            Utc::now(),
            None, // semantic_embedding — generated separately
        );

        // 4G. Cross-project pattern recording
        cog.cross_project.record_project_memory(node_id, "default", None);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Mutex<Storage>>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(Mutex::new(storage)), dir)
    }

    #[tokio::test]
    async fn test_smart_ingest_empty_content_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_smart_ingest_basic_content_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "This is a test fact to remember."
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
        assert!(value["decision"].is_string());
    }

    #[tokio::test]
    async fn test_smart_ingest_force_create() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Force create test content.",
            "forceCreate": true
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["decision"], "create");
        assert!(value["reason"].as_str().unwrap().contains("Forced") ||
                value["reason"].as_str().unwrap().contains("Embeddings not available"));
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["content"].is_object());
        assert!(schema_value["properties"]["forceCreate"].is_object());
        assert!(schema_value["required"].as_array().unwrap().contains(&serde_json::json!("content")));
    }

    #[tokio::test]
    async fn test_smart_ingest_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_smart_ingest_whitespace_only_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "   \t\n  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_smart_ingest_too_large_fails() {
        let (storage, _dir) = test_storage().await;
        let large = "x".repeat(1_000_001);
        let args = serde_json::json!({ "content": large });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[tokio::test]
    async fn test_smart_ingest_exactly_1mb_succeeds() {
        let (storage, _dir) = test_storage().await;
        let content = "x".repeat(1_000_000);
        let args = serde_json::json!({ "content": content });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_smart_ingest_with_node_type() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "A concept to remember",
            "node_type": "concept"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_smart_ingest_with_tags_and_source() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Tagged and sourced memory",
            "tags": ["test", "smart-ingest"],
            "source": "unit-test"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_smart_ingest_response_has_importance_score() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "Important memory content" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["importanceScore"].is_number());
    }

    #[tokio::test]
    async fn test_smart_ingest_missing_content_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "tags": ["test"] });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }
}
