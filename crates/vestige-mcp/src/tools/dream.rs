//! Dream tool — Explicit dream trigger that returns insights.
//! v1.5.0: Wires MemoryDreamer into an MCP tool.

use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::Utc;
use crate::cognitive::CognitiveEngine;
use vestige_core::{DreamHistoryRecord, Storage};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "memory_count": {
                "type": "integer",
                "description": "Number of recent memories to dream about (default: 50)",
                "default": 50
            }
        }
    })
}

pub async fn execute(
    storage: &Arc<Mutex<Storage>>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let memory_count = args
        .as_ref()
        .and_then(|a| a.get("memory_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    let storage_guard = storage.lock().await;
    let all_nodes = storage_guard.get_all_nodes(memory_count as i32, 0)
        .map_err(|e| format!("Failed to load memories: {}", e))?;

    if all_nodes.len() < 5 {
        return Ok(serde_json::json!({
            "status": "insufficient_memories",
            "message": format!("Need at least 5 memories to dream. Current count: {}", all_nodes.len()),
            "count": all_nodes.len()
        }));
    }

    let dream_memories: Vec<vestige_core::DreamMemory> = all_nodes.iter().map(|n| {
        vestige_core::DreamMemory {
            id: n.id.clone(),
            content: n.content.clone(),
            embedding: storage_guard.get_node_embedding(&n.id).ok().flatten(),
            tags: n.tags.clone(),
            created_at: n.created_at,
            access_count: n.reps as u32,
        }
    }).collect();
    // Drop storage lock before taking cognitive lock (strict ordering)
    drop(storage_guard);

    let cog = cognitive.lock().await;
    let dream_result = cog.dreamer.dream(&dream_memories).await;
    let insights = cog.dreamer.synthesize_insights(&dream_memories);
    drop(cog);

    // Persist dream history (non-fatal on failure — dream still happened)
    {
        let mut storage_guard = storage.lock().await;
        let record = DreamHistoryRecord {
            dreamed_at: Utc::now(),
            duration_ms: dream_result.duration_ms as i64,
            memories_replayed: dream_memories.len() as i32,
            connections_found: dream_result.new_connections_found as i32,
            insights_generated: dream_result.insights_generated.len() as i32,
            memories_strengthened: dream_result.memories_strengthened as i32,
            memories_compressed: dream_result.memories_compressed as i32,
        };
        if let Err(e) = storage_guard.save_dream_history(&record) {
            tracing::warn!("Failed to persist dream history: {}", e);
        }
    }

    Ok(serde_json::json!({
        "status": "dreamed",
        "memoriesReplayed": dream_memories.len(),
        "insights": insights.iter().map(|i| serde_json::json!({
            "insight_type": format!("{:?}", i.insight_type),
            "insight": i.insight,
            "source_memories": i.source_memories,
            "confidence": i.confidence,
            "novelty_score": i.novelty_score,
        })).collect::<Vec<_>>(),
        "stats": {
            "new_connections_found": dream_result.new_connections_found,
            "memories_strengthened": dream_result.memories_strengthened,
            "memories_compressed": dream_result.memories_compressed,
            "insights_generated": dream_result.insights_generated.len(),
            "duration_ms": dream_result.duration_ms,
        }
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

    async fn test_storage() -> (Arc<Mutex<Storage>>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(Mutex::new(storage)), dir)
    }

    async fn ingest_n_memories(storage: &Arc<Mutex<Storage>>, n: usize) {
        let mut s = storage.lock().await;
        for i in 0..n {
            s.ingest(vestige_core::IngestInput {
                content: format!("Dream test memory number {}", i),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["dream-test".to_string()],
                valid_from: None,
                valid_until: None,
            })
            .unwrap();
        }
    }

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["memory_count"].is_object());
        assert_eq!(s["properties"]["memory_count"]["default"], 50);
    }

    #[tokio::test]
    async fn test_dream_insufficient_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 3).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "insufficient_memories");
        assert_eq!(value["count"], 3);
    }

    #[tokio::test]
    async fn test_dream_empty_database() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "insufficient_memories");
        assert_eq!(value["count"], 0);
    }

    #[tokio::test]
    async fn test_dream_with_enough_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
        assert!(value["memoriesReplayed"].as_u64().unwrap() >= 5);
        assert!(value["insights"].is_array());
        assert!(value["stats"].is_object());
    }

    #[tokio::test]
    async fn test_dream_custom_memory_count() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;
        let args = serde_json::json!({ "memory_count": 7 });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
        assert!(value["memoriesReplayed"].as_u64().unwrap() <= 7);
    }

    #[tokio::test]
    async fn test_dream_with_exactly_5_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 5).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");
    }

    #[tokio::test]
    async fn test_dream_stats_fields_present() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 6).await;
        let result = execute(&storage, &test_cognitive(), None).await;
        let value = result.unwrap();
        assert!(value["stats"]["new_connections_found"].is_number());
        assert!(value["stats"]["memories_strengthened"].is_number());
        assert!(value["stats"]["memories_compressed"].is_number());
        assert!(value["stats"]["insights_generated"].is_number());
        assert!(value["stats"]["duration_ms"].is_number());
    }

    #[tokio::test]
    async fn test_dream_persists_to_database() {
        let (storage, _dir) = test_storage().await;
        ingest_n_memories(&storage, 10).await;

        // Before dream: no dream history
        {
            let s = storage.lock().await;
            assert!(s.get_last_dream().unwrap().is_none());
        }

        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["status"], "dreamed");

        // After dream: dream history should exist
        {
            let s = storage.lock().await;
            let last = s.get_last_dream().unwrap();
            assert!(last.is_some(), "Dream should have been persisted to database");
        }
    }
}
