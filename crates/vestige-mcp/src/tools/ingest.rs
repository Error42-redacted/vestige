//! Ingest Tool
//!
//! Add new knowledge to memory.
//!
//! v1.5.0: Enhanced with same cognitive pipeline as smart_ingest:
//!   Pre-ingest: importance scoring + intent detection
//!   Post-ingest: synaptic tagging + novelty model update + hippocampal indexing

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{
    ContentType, ImportanceContext, ImportanceEvent, ImportanceEventType, IngestInput, Storage,
};

/// Input schema for ingest tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "content": {
                "type": "string",
                "description": "The content to remember"
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
            }
        },
        "required": ["content"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestArgs {
    content: String,
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
}

pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: IngestArgs = match args {
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
    // COGNITIVE PRE-INGEST: importance scoring + intent detection
    // ====================================================================
    let mut importance_composite = 0.0_f64;
    let mut tags = args.tags.unwrap_or_default();
    let mut is_novel = false;
    let mut embedding_strategy = String::new();

    if let Ok(cog) = cognitive.try_lock() {
        // Full 4-channel importance scoring
        let context = ImportanceContext::current();
        let importance = cog.importance_signals.compute_importance(&args.content, &context);
        importance_composite = importance.composite;

        // Standalone novelty check (dopaminergic signal)
        let novelty_ctx = vestige_core::neuroscience::importance_signals::Context::default();
        is_novel = cog.novelty_signal.is_novel(&args.content, &novelty_ctx);

        // Intent detection → auto-tag
        let intent_result = cog.intent_detector.detect_intent();
        if intent_result.confidence > 0.5 {
            let intent_tag = format!("intent:{:?}", intent_result.primary_intent);
            let intent_tag = if intent_tag.len() > 50 {
                format!("{}...", &intent_tag[..47])
            } else {
                intent_tag
            };
            tags.push(intent_tag);
        }

        // Detect content type → select adaptive embedding strategy
        let content_type = ContentType::detect(&args.content);
        let strategy = cog.adaptive_embedder.select_strategy(&content_type);
        embedding_strategy = format!("{:?}", strategy);
    }

    let input = IngestInput {
        content: args.content.clone(),
        node_type: args.node_type.unwrap_or_else(|| "fact".to_string()),
        source: args.source,
        sentiment_score: 0.0,
        sentiment_magnitude: importance_composite,
        tags,
        valid_from: None,
        valid_until: None,
    };

    // ====================================================================
    // INGEST (storage lock)
    // ====================================================================

    // Route through smart_ingest when embeddings are available to prevent duplicates.
    // Falls back to raw ingest only when embeddings aren't ready.
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let fallback_input = input.clone();
        match storage.smart_ingest(input) {
            Ok(result) => {
                let node_id = result.node.id.clone();
                let node_content = result.node.content.clone();
                let node_type = result.node.node_type.clone();
                let has_embedding = result.node.has_embedding.unwrap_or(false);

                run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

                Ok(serde_json::json!({
                    "success": true,
                    "nodeId": node_id,
                    "decision": result.decision,
                    "message": format!("Knowledge ingested successfully. Node ID: {} ({})", node_id, result.decision),
                    "hasEmbedding": has_embedding,
                    "similarity": result.similarity,
                    "reason": result.reason,
                    "isNovel": is_novel,
                    "embeddingStrategy": embedding_strategy,
                }))
            }
            Err(_) => {
                let node = storage.ingest(fallback_input).map_err(|e| e.to_string())?;
                let node_id = node.id.clone();
                let node_content = node.content.clone();
                let node_type = node.node_type.clone();
                let has_embedding = node.has_embedding.unwrap_or(false);

                run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

                Ok(serde_json::json!({
                    "success": true,
                    "nodeId": node_id,
                    "decision": "create",
                    "message": format!("Knowledge ingested successfully. Node ID: {}", node_id),
                    "hasEmbedding": has_embedding,
                    "isNovel": is_novel,
                    "embeddingStrategy": embedding_strategy,
                }))
            }
        }
    }

    // Fallback for builds without embedding features
    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let node = storage.ingest(input).map_err(|e| e.to_string())?;
        let node_id = node.id.clone();
        let node_content = node.content.clone();
        let node_type = node.node_type.clone();
        let has_embedding = node.has_embedding.unwrap_or(false);

        run_post_ingest(cognitive, &node_id, &node_content, &node_type, importance_composite);

        Ok(serde_json::json!({
            "success": true,
            "nodeId": node_id,
            "decision": "create",
            "message": format!("Knowledge ingested successfully. Node ID: {}", node_id),
            "hasEmbedding": has_embedding,
            "isNovel": is_novel,
            "embeddingStrategy": embedding_strategy,
        }))
    }
}

/// Cognitive post-ingest side effects: synaptic tagging, novelty update, hippocampal indexing.
fn run_post_ingest(
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    node_id: &str,
    content: &str,
    node_type: &str,
    importance_composite: f64,
) {
    if let Ok(mut cog) = cognitive.try_lock() {
        // Synaptic tagging for retroactive capture
        if importance_composite > 0.3 {
            cog.synaptic_tagging.tag_memory(node_id);
            if importance_composite > 0.7 {
                let event = ImportanceEvent::for_memory(node_id, ImportanceEventType::NoveltySpike);
                let _capture = cog.synaptic_tagging.trigger_prp(event);
            }
        }

        // Update novelty model
        cog.importance_signals.learn_content(content);

        // Record in hippocampal index
        let _ = cog.hippocampal_index.index_memory(
            node_id,
            content,
            node_type,
            Utc::now(),
            None,
        );

        // Cross-project pattern recording
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
    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    // ========================================================================
    // INPUT VALIDATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_ingest_empty_content_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_ingest_whitespace_only_content_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "content": "   \n\t  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_ingest_missing_arguments_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_ingest_missing_content_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "node_type": "fact" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }

    // ========================================================================
    // LARGE CONTENT TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_ingest_large_content_fails() {
        let (storage, _dir) = test_storage().await;
        // Create content larger than 1MB
        let large_content = "x".repeat(1_000_001);
        let args = serde_json::json!({ "content": large_content });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[tokio::test]
    async fn test_ingest_exactly_1mb_succeeds() {
        let (storage, _dir) = test_storage().await;
        // Create content exactly 1MB
        let exact_content = "x".repeat(1_000_000);
        let args = serde_json::json!({ "content": exact_content });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // SUCCESSFUL INGEST TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_ingest_basic_content_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "This is a test fact to remember."
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
        assert!(value["message"].as_str().unwrap().contains("successfully"));
    }

    #[tokio::test]
    async fn test_ingest_with_node_type() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Error handling should use Result<T, E> pattern.",
            "node_type": "pattern"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_ingest_with_tags() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "The Rust programming language emphasizes safety.",
            "tags": ["rust", "programming", "safety"]
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_ingest_with_source() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "MCP protocol version 2024-11-05 is the current standard.",
            "source": "https://modelcontextprotocol.io/spec"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_ingest_with_all_optional_fields() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Complex memory with all metadata.",
            "node_type": "decision",
            "tags": ["architecture", "design"],
            "source": "team meeting notes"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["nodeId"].is_string());
    }

    // ========================================================================
    // NODE TYPE DEFAULTS
    // ========================================================================

    #[tokio::test]
    async fn test_ingest_default_node_type_is_fact() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "content": "Default type test content."
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        // Verify node was created - the default type is "fact"
        let node_id = result.unwrap()["nodeId"].as_str().unwrap().to_string();
        let node = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(node.node_type, "fact");
    }

    // ========================================================================
    // SCHEMA TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_required_fields() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["content"].is_object());
        assert!(schema_value["required"].as_array().unwrap().contains(&serde_json::json!("content")));
    }

    #[test]
    fn test_schema_has_optional_fields() {
        let schema_value = schema();
        assert!(schema_value["properties"]["node_type"].is_object());
        assert!(schema_value["properties"]["tags"].is_object());
        assert!(schema_value["properties"]["source"].is_object());
    }
}
