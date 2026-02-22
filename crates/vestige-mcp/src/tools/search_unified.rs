//! Unified Search Tool
//!
//! Merges recall, semantic_search, and hybrid_search into a single `search` tool.
//! Always uses hybrid search internally (keyword + semantic + RRF fusion).
//! Implements Testing Effect (Roediger & Karpicke 2006) by auto-strengthening memories on access.
//!
//! v1.5.0: Enhanced 7-stage cognitive pipeline:
//!   1. Reranker (over-fetch 3x, rerank down)
//!   2. Temporal boosting (recency + validity)
//!   3. Memory state accessibility filtering
//!   4. Context matching (topic overlap)
//!   5. Spreading activation associations
//!   6. Predictive memory recording
//!   7. Reconsolidation (mark labile)

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{
    CompetitionCandidate, EncodingContext, MemoryLifecycle, MemorySnapshot, MemoryState, Storage,
    TopicalContext,
};

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
            },
            "context_topics": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional topics for context-dependent retrieval boosting"
            },
            "token_budget": {
                "type": "integer",
                "description": "Max tokens for response. Server truncates content to fit budget. Use memory(action='get') for full content of specific IDs.",
                "minimum": 100,
                "maximum": 10000
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
    context_topics: Option<Vec<String>>,
    #[serde(alias = "token_budget")]
    token_budget: Option<i32>,
}

/// Execute unified search with 7-stage cognitive pipeline.
///
/// Pipeline:
///   1. Hybrid search (keyword + semantic + RRF) with 3x over-fetch
///   2. Reranker (BM25-like rescoring, trim to limit)
///   3. Temporal boosting (recency + validity windows)
///   4. Memory state accessibility filtering (Active/Dormant/Silent/Unavailable)
///   5. Context matching (topic overlap boosting)
///   6. Spreading activation (find associated memories)
///   7. Side effects: predictive memory recording + reconsolidation labile marking
///
/// Also applies Testing Effect (Roediger & Karpicke 2006) by auto-strengthening on access.
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
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

    // Favor semantic search — research shows 0.3/0.7 outperforms equal weights
    let keyword_weight = 0.3_f32;
    let semantic_weight = 0.7_f32;

    // ====================================================================
    // STAGE 1: Hybrid search with 3x over-fetch for reranking pool
    // ====================================================================
    let overfetch_limit = (limit * 3).min(100); // Cap at 100 to avoid excessive DB load

    let results = storage
        .hybrid_search(&args.query, overfetch_limit, keyword_weight, semantic_weight)
        .map_err(|e| e.to_string())?;

    // Filter by min_retention and min_similarity first (cheap filters)
    let mut filtered_results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            if r.node.retention_strength < min_retention {
                return false;
            }
            if let Some(sem_score) = r.semantic_score
                && sem_score < min_similarity
            {
                return false;
            }
            true
        })
        .collect();

    // ====================================================================
    // STAGE 2: Reranker (BM25-like rescoring, trim to requested limit)
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        let candidates: Vec<_> = filtered_results
            .iter()
            .map(|r| (r.clone(), r.node.content.clone()))
            .collect();

        if let Ok(reranked) = cog.reranker.rerank(&args.query, candidates, Some(limit as usize)) {
            // Replace filtered_results with reranked items (preserves original SearchResult)
            filtered_results = reranked.into_iter().map(|rr| rr.item).collect();
        } else {
            // Reranker failed — fall back to original order, just truncate
            filtered_results.truncate(limit as usize);
        }
    } else {
        // Couldn't acquire cognitive lock — truncate to limit
        filtered_results.truncate(limit as usize);
    }

    // ====================================================================
    // STAGE 3: Temporal boosting (recency + validity windows)
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        for result in &mut filtered_results {
            let recency = cog.temporal_searcher.recency_boost(result.node.created_at);
            let validity = cog.temporal_searcher.validity_boost(
                result.node.valid_from,
                result.node.valid_until,
                None,
            );
            // Blend: 85% relevance + 15% temporal signal
            let temporal_factor = recency * validity;
            result.combined_score =
                result.combined_score * 0.85 + (result.combined_score * temporal_factor as f32) * 0.15;
        }
    }

    // ====================================================================
    // STAGE 4: Memory state accessibility filtering
    // ====================================================================
    if let Ok(cog) = cognitive.try_lock() {
        for result in &mut filtered_results {
            // Build a MemoryLifecycle from node data for the calculator
            let mut lifecycle = MemoryLifecycle::new();
            lifecycle.last_access = result.node.last_accessed;
            lifecycle.access_count = result.node.reps as u32;
            // Determine state from retention strength
            lifecycle.state = if result.node.retention_strength > 0.7 {
                MemoryState::Active
            } else if result.node.retention_strength > 0.3 {
                MemoryState::Dormant
            } else if result.node.retention_strength > 0.1 {
                MemoryState::Silent
            } else {
                MemoryState::Unavailable
            };

            let adjusted = cog
                .accessibility_calc
                .calculate(&lifecycle, result.combined_score as f64);
            result.combined_score = adjusted as f32;
        }
    }

    // ====================================================================
    // STAGE 5: Context matching (Tulving 1973 encoding specificity)
    // ====================================================================
    if let Some(ref topics) = args.context_topics
        && !topics.is_empty()
    {
        let retrieval_ctx = EncodingContext::new()
            .with_topical(TopicalContext::with_topics(topics.clone()));
        if let Ok(cog) = cognitive.try_lock() {
            for result in &mut filtered_results {
                // Build encoding context from memory's tags
                let encoding_ctx = EncodingContext::new()
                    .with_topical(TopicalContext::with_topics(result.node.tags.clone()));
                let context_score = cog.context_matcher.match_contexts(&encoding_ctx, &retrieval_ctx);
                // Blend: context match boosts relevance up to +30%
                result.combined_score *= 1.0 + (context_score as f32 * 0.3);
            }
        }
    }

    // Context reinstatement for top result (helps Claude understand WHY this memory matched)
    let reinstatement_info: Option<Value> = if let Ok(cog) = cognitive.try_lock() {
        if let Some(first) = filtered_results.first() {
            let current_ctx = if let Some(ref topics) = args.context_topics {
                EncodingContext::new().with_topical(TopicalContext::with_topics(topics.clone()))
            } else {
                EncodingContext::new()
            };
            let reinstatement = cog.context_matcher.reinstate_context(&first.node.id, &current_ctx);
            Some(serde_json::json!({
                "memoryId": reinstatement.memory_id,
                "temporalHint": reinstatement.temporal_hint,
                "topicalHint": reinstatement.topical_hint,
                "sessionHint": reinstatement.session_hint,
                "relatedMemories": reinstatement.related_memories,
            }))
        } else {
            None
        }
    } else {
        None
    };

    // ====================================================================
    // STAGE 5B: Retrieval competition (Anderson et al. 1994)
    // ====================================================================
    let mut suppressed_count = 0_usize;
    if filtered_results.len() > 1
        && let Ok(mut cog) = cognitive.try_lock()
    {
        let candidates: Vec<CompetitionCandidate> = filtered_results
            .iter()
            .map(|r| CompetitionCandidate {
                memory_id: r.node.id.clone(),
                relevance_score: r.combined_score as f64,
                similarity_to_query: r.semantic_score.unwrap_or(0.0) as f64,
            })
            .collect();
        if let Some(result) = cog.competition_mgr.run_competition(&candidates, 0.7) {
            // Apply suppression: losers get penalized
            for suppressed_id in &result.suppressed_ids {
                if let Some(r) = filtered_results.iter_mut().find(|r| &r.node.id == suppressed_id) {
                    r.combined_score *= 0.85; // 15% suppression penalty
                    suppressed_count += 1;
                }
            }
        }
    }

    // ====================================================================
    // STAGE 5C: Utility-based ranking (MemRL-inspired)
    // Memories that proved useful in past sessions get a retrieval boost.
    // utility_score = times_useful / times_retrieved (0.0 to 1.0)
    // ====================================================================
    for result in &mut filtered_results {
        let utility = result.node.utility_score.unwrap_or(0.0) as f32;
        if utility > 0.0 {
            // Utility boost: up to +15% for memories with utility_score = 1.0
            result.combined_score *= 1.0 + (utility * 0.15);
        }
    }

    // Re-sort by adjusted combined_score (descending) after all score modifications
    filtered_results.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ====================================================================
    // STAGE 6: Spreading activation (find associated memories)
    // ====================================================================
    let associations: Vec<Value> = if let Ok(mut cog) = cognitive.try_lock() {
        if let Some(first) = filtered_results.first() {
            let activated = cog.activation_network.activate(&first.node.id, 1.0);
            activated
                .iter()
                .take(3)
                .map(|a| {
                    serde_json::json!({
                        "memoryId": a.memory_id,
                        "activation": a.activation,
                        "distance": a.distance,
                    })
                })
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // ====================================================================
    // Auto-strengthen on access (Testing Effect)
    // ====================================================================
    let ids: Vec<&str> = filtered_results.iter().map(|r| r.node.id.as_str()).collect();
    let _ = storage.strengthen_batch_on_access(&ids);

    // Drop storage lock before acquiring cognitive for side effects

    // ====================================================================
    // STAGE 7: Side effects — predictive memory + reconsolidation
    // ====================================================================
    if let Ok(mut cog) = cognitive.try_lock() {
        // 7A. Record query for predictive memory
        let _ = cog.predictive_memory.record_query(&args.query, &[]);

        // 7B. Record each accessed memory for predictive/speculative models
        for result in &filtered_results {
            let _ = cog.predictive_memory.record_memory_access(
                &result.node.id,
                &result.node.content.chars().take(100).collect::<String>(),
                &result.node.tags,
            );

            cog.speculative_retriever.record_access(
                &result.node.id,
                None,                           // file_context
                Some(args.query.as_str()),       // query_context
                None,                            // was_helpful (unknown yet)
            );

            // 7C. Mark labile for reconsolidation window (5 min)
            let snapshot = MemorySnapshot {
                content: result.node.content.clone(),
                tags: result.node.tags.clone(),
                retention_strength: result.node.retention_strength,
                storage_strength: result.node.storage_strength,
                retrieval_strength: result.node.retrieval_strength,
                connection_ids: vec![],
                captured_at: Utc::now(),
            };
            cog.reconsolidation.mark_labile(&result.node.id, snapshot);
        }
    }

    // ====================================================================
    // Format and return
    // ====================================================================
    let mut formatted: Vec<Value> = filtered_results
        .iter()
        .map(|r| format_search_result(r, detail_level))
        .collect();

    // ====================================================================
    // Token budget enforcement (v1.8.0)
    // ====================================================================
    let mut budget_expandable: Vec<String> = Vec::new();
    let mut budget_tokens_used: Option<usize> = None;
    if let Some(budget) = args.token_budget {
        let budget = budget.clamp(100, 10000) as usize;
        let budget_chars = budget * 4;
        let mut used = 0;
        let mut budgeted = Vec::new();

        for result in &formatted {
            let size = serde_json::to_string(result).unwrap_or_default().len();
            if used + size > budget_chars {
                if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                    budget_expandable.push(id.to_string());
                }
                continue;
            }
            used += size;
            budgeted.push(result.clone());
        }

        budget_tokens_used = Some(used / 4);
        formatted = budgeted;
    }

    // Check learning mode via attention signal
    let learning_mode = cognitive.try_lock().ok().map(|cog| cog.attention_signal.is_learning_mode()).unwrap_or(false);

    let mut response = serde_json::json!({
        "query": args.query,
        "method": "hybrid+cognitive",
        "detailLevel": detail_level,
        "total": formatted.len(),
        "results": formatted,
    });

    // Include associations if any were found
    if !associations.is_empty() {
        response["associations"] = serde_json::json!(associations);
    }
    // Include context reinstatement if computed
    if let Some(ri) = reinstatement_info {
        response["contextReinstatement"] = ri;
    }
    // Include competition stats
    if suppressed_count > 0 {
        response["competitionSuppressed"] = serde_json::json!(suppressed_count);
    }
    // Include learning mode detection
    if learning_mode {
        response["learningModeDetected"] = serde_json::json!(true);
    }
    // Include token budget info (v1.8.0)
    if !budget_expandable.is_empty() {
        response["expandable"] = serde_json::json!(budget_expandable);
    }
    if let Some(budget) = args.token_budget {
        response["tokenBudget"] = serde_json::json!(budget);
    }
    if let Some(used) = budget_tokens_used {
        response["tokensUsed"] = serde_json::json!(used);
    }

    Ok(response)
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
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;
    use vestige_core::IngestInput;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    /// Create a test storage instance with a temporary database
    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    /// Helper to ingest test content
    async fn ingest_test_content(storage: &Arc<Storage>, content: &str) -> String {
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
        let node = storage.ingest(input).unwrap();
        node.id
    }

    // ========================================================================
    // QUERY VALIDATION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_search_empty_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_whitespace_only_query_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "query": "   \t\n  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_missing_arguments_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_search_missing_query_field_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 10 });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["query"], "rust");
        assert_eq!(value["method"], "hybrid+cognitive");
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid detail_level"));
    }

    // ========================================================================
    // TOKEN BUDGET TESTS (v1.8.0)
    // ========================================================================

    #[tokio::test]
    async fn test_token_budget_limits_results() {
        let (storage, _dir) = test_storage().await;
        for i in 0..10 {
            ingest_test_content(
                &storage,
                &format!("Budget test content number {} with some extra text to increase size.", i),
            )
            .await;
        }

        // Small budget should reduce results
        let args = serde_json::json!({
            "query": "budget test",
            "token_budget": 200,
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["tokenBudget"].as_i64().unwrap() == 200);
        assert!(value["tokensUsed"].is_number());
    }

    #[tokio::test]
    async fn test_token_budget_expandable() {
        let (storage, _dir) = test_storage().await;
        for i in 0..15 {
            ingest_test_content(
                &storage,
                &format!(
                    "Expandable budget test number {} with quite a bit of content to ensure we exceed the token budget allocation threshold.",
                    i
                ),
            )
            .await;
        }

        let args = serde_json::json!({
            "query": "expandable budget test",
            "token_budget": 150,
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        // expandable field should exist if results were dropped
        if let Some(expandable) = value.get("expandable") {
            assert!(expandable.is_array());
        }
    }

    #[tokio::test]
    async fn test_no_budget_unchanged() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "No budget test content.").await;

        let args = serde_json::json!({
            "query": "no budget",
            "min_similarity": 0.0
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        // No budget fields should be present
        assert!(value.get("tokenBudget").is_none());
        assert!(value.get("tokensUsed").is_none());
        assert!(value.get("expandable").is_none());
    }

    #[test]
    fn test_schema_has_token_budget() {
        let schema_value = schema();
        let tb = &schema_value["properties"]["token_budget"];
        assert!(tb.is_object());
        assert_eq!(tb["minimum"], 100);
        assert_eq!(tb["maximum"], 10000);
    }
}
