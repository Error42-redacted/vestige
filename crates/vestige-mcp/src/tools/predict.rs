//! Predict tool — Proactive memory prediction ("what will you need next?").
//! v1.5.0: Wires PredictiveMemory + SpeculativeRetriever.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::Storage;

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "context": {
                "type": "object",
                "description": "Current context for prediction",
                "properties": {
                    "current_file": { "type": "string" },
                    "current_topics": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "codebase": { "type": "string" }
                }
            }
        }
    })
}

pub async fn execute(
    _storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let context = args.as_ref().and_then(|a| a.get("context"));

    let cog = cognitive.lock().await;

    // Build session context for predictive memory
    let session_ctx = vestige_core::neuroscience::predictive_retrieval::SessionContext {
        started_at: chrono::Utc::now(),
        current_focus: context
            .and_then(|c| c.get("current_topics"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        active_files: context
            .and_then(|c| c.get("current_file"))
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default(),
        accessed_memories: Vec::new(),
        recent_queries: Vec::new(),
        detected_intent: None,
        project_context: context
            .and_then(|c| c.get("codebase"))
            .and_then(|v| v.as_str())
            .map(|name| vestige_core::neuroscience::predictive_retrieval::ProjectContext {
                name: name.to_string(),
                path: String::new(),
                technologies: Vec::new(),
                primary_language: None,
            }),
    };

    // Get predictions from predictive memory
    let predictions = cog.predictive_memory.predict_needed_memories(&session_ctx)
        .unwrap_or_default();
    let suggestions = cog.predictive_memory.get_proactive_suggestions(0.3)
        .unwrap_or_default();
    let top_interests = cog.predictive_memory.get_top_interests(10)
        .unwrap_or_default();
    let accuracy = cog.predictive_memory.prediction_accuracy()
        .unwrap_or(0.0);

    // Build speculative context
    let speculative_context = vestige_core::PredictionContext {
        open_files: context
            .and_then(|c| c.get("current_file"))
            .and_then(|v| v.as_str())
            .map(|s| vec![PathBuf::from(s)])
            .unwrap_or_default(),
        recent_edits: Vec::new(),
        recent_queries: Vec::new(),
        recent_memory_ids: Vec::new(),
        project_path: context
            .and_then(|c| c.get("codebase"))
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
        timestamp: Some(chrono::Utc::now()),
    };
    let speculative = cog.speculative_retriever.predict_needed(&speculative_context);

    Ok(serde_json::json!({
        "predictions": predictions.iter().map(|p| serde_json::json!({
            "memory_id": p.memory_id,
            "content_preview": p.content_preview,
            "confidence": p.confidence,
            "reasoning": format!("{:?}", p.reasoning),
        })).collect::<Vec<_>>(),
        "suggestions": suggestions.iter().map(|p| serde_json::json!({
            "memory_id": p.memory_id,
            "content_preview": p.content_preview,
            "confidence": p.confidence,
            "reasoning": format!("{:?}", p.reasoning),
        })).collect::<Vec<_>>(),
        "speculative": speculative.iter().map(|p| serde_json::json!({
            "memory_id": p.memory_id,
            "content_preview": p.content_preview,
            "confidence": p.confidence,
            "trigger": format!("{:?}", p.trigger),
        })).collect::<Vec<_>>(),
        "top_interests": top_interests,
        "prediction_accuracy": accuracy,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["context"].is_object());
        assert!(s["properties"]["context"]["properties"]["current_file"].is_object());
        assert!(s["properties"]["context"]["properties"]["current_topics"].is_object());
        assert!(s["properties"]["context"]["properties"]["codebase"].is_object());
    }

    #[tokio::test]
    async fn test_predict_no_args_succeeds() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["predictions"].is_array());
        assert!(value["suggestions"].is_array());
        assert!(value["speculative"].is_array());
        assert!(value["prediction_accuracy"].is_number());
    }

    #[tokio::test]
    async fn test_predict_empty_context() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "context": {} });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["predictions"].is_array());
    }

    #[tokio::test]
    async fn test_predict_with_full_context() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "context": {
                "current_file": "/src/main.rs",
                "current_topics": ["rust", "memory"],
                "codebase": "vestige"
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["predictions"].is_array());
        assert!(value["top_interests"].is_array());
    }

    #[tokio::test]
    async fn test_predict_with_topics_only() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "context": {
                "current_topics": ["debugging", "errors"]
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_predict_accuracy_is_number() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        let value = result.unwrap();
        let accuracy = value["prediction_accuracy"].as_f64().unwrap();
        assert!(accuracy >= 0.0);
    }
}
