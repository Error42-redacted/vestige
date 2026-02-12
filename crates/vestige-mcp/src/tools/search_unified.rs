//! Unified Search Tool
//!
//! Merges recall, semantic_search, and hybrid_search into a single `search` tool.
//! Always uses hybrid search internally (keyword + semantic + RRF fusion).
//! Implements Testing Effect (Roediger & Karpicke 2006) by auto-strengthening memories on access.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use vestige_core::Storage;

/// Input schema for unified search tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of results (default: 10)",
                "default": 10,
                "minimum": 1,
                "maximum": 100
            },
            "min_retention": {
                "type": "number",
                "description": "Minimum retention strength (0.0-1.0, default: 0.0)",
                "default": 0.0,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "min_similarity": {
                "type": "number",
                "description": "Minimum similarity threshold (0.0-1.0, default: 0.5)",
                "default": 0.5,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "detail_level": {
                "type": "string",
                "description": "Level of detail in results. 'brief' = id/type/tags/score only (saves tokens). 'summary' = default 8-field response. 'full' = all fields including FSRS state and timestamps.",
                "enum": ["brief", "summary", "full"],
                "default": "summary"
            }
        },
        "required": ["query"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    query: String,
    limit: Option<i32>,
    min_retention: Option<f64>,
    min_similarity: Option<f32>,
    #[serde(alias = "detail_level")]
    detail_level: Option<String>,
}

/// Execute unified search
///
/// Uses hybrid search (keyword + semantic + RRF fusion) internally.
/// Auto-strengthens memories on access (Testing Effect - Roediger & Karpicke 2006).
pub async fn execute(
    storage: &Arc<Mutex<Storage>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: SearchArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    if args.query.trim().is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    // Validate detail_level
    let detail_level = match args.detail_level.as_deref() {
        Some("brief") => "brief",
        Some("full") => "full",
        Some("summary") | None => "summary",
        Some(invalid) => {
            return Err(format!(
                "Invalid detail_level '{}'. Must be 'brief', 'summary', or 'full'.",
                invalid
            ));
        }
    };

    // Clamp all parameters to valid ranges
    let limit = args.limit.unwrap_or(10).clamp(1, 100);
    let min_retention = args.min_retention.unwrap_or(0.0).clamp(0.0, 1.0);
    let min_similarity = args.min_similarity.unwrap_or(0.5).clamp(0.0, 1.0);

    // Use balanced weights for hybrid search (keyword + semantic)
    let keyword_weight = 0.5_f32;
    let semantic_weight = 0.5_f32;

    let storage = storage.lock().await;

    // Execute hybrid search
    let results = storage
        .hybrid_search(&args.query, limit, keyword_weight, semantic_weight)
        .map_err(|e| e.to_string())?;

    // Filter results by min_retention and min_similarity
    let filtered_results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            // Check retention strength
            if r.node.retention_strength < min_retention {
                return false;
            }
            // Check similarity if semantic score is available
            if let Some(sem_score) = r.semantic_score
                && sem_score < min_similarity
            {
                return false;
            }
            true
        })
        .collect();

    // Auto-strengthen memories on access (Testing Effect - Roediger & Karpicke 2006)
    // This implements "use it or lose it" - accessed memories get stronger
    let ids: Vec<&str> = filtered_results.iter().map(|r| r.node.id.as_str()).collect();
    let _ = storage.strengthen_batch_on_access(&ids); // Ignore errors, don't fail search

    // Format results based on detail_level
    let formatted: Vec<Value> = filtered_results
        .iter()
        .map(|r| format_search_result(r, detail_level))
        .collect();

    Ok(serde_json::json!({
        "query": args.query,
        "method": "hybrid",
        "detailLevel": detail_level,
        "total": formatted.len(),
        "results": formatted,
    }))
}

/// Format a search result based on the requested detail level.
fn format_search_result(r: &vestige_core::SearchResult, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": r.node.id,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "combinedScore": r.combined_score,
        }),
        "full" => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
            "storageStrength": r.node.storage_strength,
            "retrievalStrength": r.node.retrieval_strength,
            "source": r.node.source,
            "sentimentScore": r.node.sentiment_score,
            "sentimentMagnitude": r.node.sentiment_magnitude,
            "createdAt": r.node.created_at.to_rfc3339(),
            "updatedAt": r.node.updated_at.to_rfc3339(),
            "lastAccessed": r.node.last_accessed.to_rfc3339(),
            "nextReview": r.node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": r.node.stability,
            "difficulty": r.node.difficulty,
            "reps": r.node.reps,
            "lapses": r.node.lapses,
            "validFrom": r.node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": r.node.valid_until.map(|dt| dt.to_rfc3339()),
            "matchType": format!("{:?}", r.match_type),
        }),
        // "summary" (default) — backwards compatible
        _ => serde_json::json!({
            "id": r.node.id,
            "content": r.node.content,
            "combinedScore": r.combined_score,
            "keywordScore": r.keyword_score,
            "semanticScore": r.semantic_score,
            "nodeType": r.node.node_type,
            "tags": r.node.tags,
            "retentionStrength": r.node.retention_strength,
        }),
    }
}

/// Format a KnowledgeNode based on the requested detail level.
/// Reusable across search, timeline, and other tools.
pub fn format_node(node: &vestige_core::KnowledgeNode, detail_level: &str) -> Value {
    match detail_level {
        "brief" => serde_json::json!({
            "id": node.id,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
        "full" => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
            "storageStrength": node.storage_strength,
            "retrievalStrength": node.retrieval_strength,
            "source": node.source,
            "sentimentScore": node.sentiment_score,
            "sentimentMagnitude": node.sentiment_magnitude,
            "createdAt": node.created_at.to_rfc3339(),
            "updatedAt": node.updated_at.to_rfc3339(),
            "lastAccessed": node.last_accessed.to_rfc3339(),
            "nextReview": node.next_review.map(|dt| dt.to_rfc3339()),
            "stability": node.stability,
            "difficulty": node.difficulty,
            "reps": node.reps,
            "lapses": node.lapses,
            "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
            "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
        }),
        // "summary" (default)
        _ => serde_json::json!({
            "id": node.id,
            "content": node.content,
            "nodeType": node.node_type,
            "tags": node.tags,
            "retentionStrength": node.retention_strength,
        }),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use vestige_core::IngestInput;

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Mutex<Storage>>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(Mutex::new(storage)), dir)
    }

    /// Helper to ingest test content
    async fn ingest_test_content(storage: &Arc<Mutex<Storage>>, content: &str) -> String {
        let input = IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec![],
            valid_from: None,
            valid_until: None,
        };
        let mut storage_lock = storage.lock().await;
        let node = storage_lock.ingest(input).unwrap();
        node.id
    }

    // ========================================================================
    // QUERY VALIDATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_empty_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_whitespace_only_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "   \t\n  " });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_missing_arguments_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_search_missing_query_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 10 });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }

    // ========================================================================
    // LIMIT CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_limit_clamped_to_minimum() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for limit clamping").await;

        // Try with limit 0 - should clamp to 1
        let args = serde_json::json!({
            "query": "test",
            "limit": 0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_limit_clamped_to_maximum() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max limit").await;

        // Try with limit 1000 - should clamp to 100
        let args = serde_json::json!({
            "query": "test",
            "limit": 1000
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_negative_limit_clamped() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for negative limit").await;

        let args = serde_json::json!({
            "query": "test",
            "limit": -5
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // MIN_RETENTION CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_min_retention_clamped_to_zero() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for retention clamping").await;

        let args = serde_json::json!({
            "query": "test",
            "min_retention": -0.5
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_min_retention_clamped_to_one() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max retention").await;

        let args = serde_json::json!({
            "query": "test",
            "min_retention": 1.5
        });
        let result = execute(&storage, Some(args)).await;
        // Should succeed but may return no results (retention > 1.0 clamped to 1.0)
        assert!(result.is_ok());
    }

    // ========================================================================
    // MIN_SIMILARITY CLAMPING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_min_similarity_clamped_to_zero() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for similarity clamping").await;

        let args = serde_json::json!({
            "query": "test",
            "min_similarity": -0.5
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_min_similarity_clamped_to_one() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test content for max similarity").await;

        let args = serde_json::json!({
            "query": "test",
            "min_similarity": 1.5
        });
        let result = execute(&storage, Some(args)).await;
        // Should succeed but may return no results
        assert!(result.is_ok());
    }

    // ========================================================================
    // SUCCESSFUL SEARCH TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_basic_query_succeeds() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "The Rust programming language is memory safe.").await;

        let args = serde_json::json!({ "query": "rust" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["query"], "rust");
        assert_eq!(value["method"], "hybrid");
        assert!(value["total"].is_number());
        assert!(value["results"].is_array());
    }

    #[tokio::test]
    async fn test_search_returns_matching_content() {
        let (storage, _dir) = test_storage().await;
        let node_id =
            ingest_test_content(&storage, "Python is a dynamic programming language.").await;

        let args = serde_json::json!({
            "query": "python",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["id"], node_id);
    }

    #[tokio::test]
    async fn test_search_with_limit() {
        let (storage, _dir) = test_storage().await;
        // Ingest multiple items
        ingest_test_content(&storage, "Testing content one").await;
        ingest_test_content(&storage, "Testing content two").await;
        ingest_test_content(&storage, "Testing content three").await;

        let args = serde_json::json!({
            "query": "testing",
            "limit": 2,
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_search_empty_database_returns_empty_array() {
        let (storage, _dir) = test_storage().await;
        // Don't ingest anything - database is empty

        let args = serde_json::json!({ "query": "anything" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["total"], 0);
        assert!(value["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_search_result_contains_expected_fields() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Testing field presence in search results.").await;

        let args = serde_json::json!({
            "query": "testing",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            assert!(first["id"].is_string());
            assert!(first["content"].is_string());
            assert!(first["combinedScore"].is_number());
            // keywordScore and semanticScore may be null if not matched
            assert!(first["nodeType"].is_string());
            assert!(first["tags"].is_array());
            assert!(first["retentionStrength"].is_number());
        }
    }

    // ========================================================================
    // DEFAULT VALUES TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_default_limit_is_10() {
        let (storage, _dir) = test_storage().await;
        // Ingest more than 10 items
        for i in 0..15 {
            ingest_test_content(&storage, &format!("Item number {}", i)).await;
        }

        let args = serde_json::json!({
            "query": "item",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let results = value["results"].as_array().unwrap();
        assert!(results.len() <= 10);
    }

    // ========================================================================
    // SCHEMA TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_required_fields() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["query"].is_object());
        assert!(schema_value["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("query")));
    }

    #[test]
    fn test_schema_has_optional_fields() {
        let schema_value = schema();
        assert!(schema_value["properties"]["limit"].is_object());
        assert!(schema_value["properties"]["min_retention"].is_object());
        assert!(schema_value["properties"]["min_similarity"].is_object());
    }

    #[test]
    fn test_schema_limit_has_bounds() {
        let schema_value = schema();
        let limit_schema = &schema_value["properties"]["limit"];
        assert_eq!(limit_schema["minimum"], 1);
        assert_eq!(limit_schema["maximum"], 100);
        assert_eq!(limit_schema["default"], 10);
    }

    #[test]
    fn test_schema_min_retention_has_bounds() {
        let schema_value = schema();
        let retention_schema = &schema_value["properties"]["min_retention"];
        assert_eq!(retention_schema["minimum"], 0.0);
        assert_eq!(retention_schema["maximum"], 1.0);
        assert_eq!(retention_schema["default"], 0.0);
    }

    #[test]
    fn test_schema_min_similarity_has_bounds() {
        let schema_value = schema();
        let similarity_schema = &schema_value["properties"]["min_similarity"];
        assert_eq!(similarity_schema["minimum"], 0.0);
        assert_eq!(similarity_schema["maximum"], 1.0);
        assert_eq!(similarity_schema["default"], 0.5);
    }

    // ========================================================================
    // DETAIL LEVEL TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_detail_level() {
        let schema_value = schema();
        let dl = &schema_value["properties"]["detail_level"];
        assert!(dl.is_object());
        assert_eq!(dl["default"], "summary");
        let enum_values = dl["enum"].as_array().unwrap();
        assert!(enum_values.contains(&serde_json::json!("brief")));
        assert!(enum_values.contains(&serde_json::json!("summary")));
        assert!(enum_values.contains(&serde_json::json!("full")));
    }

    #[tokio::test]
    async fn test_search_detail_level_brief_excludes_content() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Brief mode test content for search.").await;

        let args = serde_json::json!({
            "query": "brief",
            "detail_level": "brief",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "brief");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Brief should NOT have content
            assert!(first.get("content").is_none() || first["content"].is_null());
            // Brief should have these fields
            assert!(first["id"].is_string());
            assert!(first["nodeType"].is_string());
            assert!(first["tags"].is_array());
            assert!(first["retentionStrength"].is_number());
            assert!(first["combinedScore"].is_number());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_full_includes_timestamps() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Full mode test content for search.").await;

        let args = serde_json::json!({
            "query": "full",
            "detail_level": "full",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "full");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Full should have timestamps
            assert!(first["createdAt"].is_string());
            assert!(first["updatedAt"].is_string());
            assert!(first["content"].is_string());
            assert!(first["storageStrength"].is_number());
            assert!(first["retrievalStrength"].is_number());
            assert!(first["matchType"].is_string());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_default_is_summary() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Default detail level test content.").await;

        let args = serde_json::json!({
            "query": "default",
            "min_similarity": 0.0
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "summary");
        let results = value["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            // Summary should have content but not timestamps
            assert!(first["content"].is_string());
            assert!(first["id"].is_string());
            assert!(first.get("createdAt").is_none() || first["createdAt"].is_null());
        }
    }

    #[tokio::test]
    async fn test_search_detail_level_invalid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "query": "test",
            "detail_level": "invalid_level"
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid detail_level"));
    }
}
