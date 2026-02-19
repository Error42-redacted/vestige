//! Dashboard API endpoint handlers

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::state::AppState;

/// Serve the dashboard HTML
pub async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("../dashboard.html"))
}

#[derive(Debug, Deserialize)]
pub struct MemoryListParams {
    pub q: Option<String>,
    pub node_type: Option<String>,
    pub tag: Option<String>,
    pub min_retention: Option<f64>,
    pub sort: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// List memories with optional search
pub async fn list_memories(
    State(state): State<AppState>,
    Query(params): Query<MemoryListParams>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    if let Some(query) = params.q.as_ref().filter(|q| !q.trim().is_empty()) {
        {
            // Use hybrid search
            let results = storage
                .hybrid_search(query, limit, 0.3, 0.7)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let formatted: Vec<Value> = results
                .into_iter()
                .filter(|r| {
                    if let Some(min_ret) = params.min_retention {
                        r.node.retention_strength >= min_ret
                    } else {
                        true
                    }
                })
                .map(|r| {
                    serde_json::json!({
                        "id": r.node.id,
                        "content": r.node.content,
                        "nodeType": r.node.node_type,
                        "tags": r.node.tags,
                        "retentionStrength": r.node.retention_strength,
                        "storageStrength": r.node.storage_strength,
                        "retrievalStrength": r.node.retrieval_strength,
                        "createdAt": r.node.created_at.to_rfc3339(),
                        "updatedAt": r.node.updated_at.to_rfc3339(),
                        "combinedScore": r.combined_score,
                        "source": r.node.source,
                        "reviewCount": r.node.reps,
                    })
                })
                .collect();

            return Ok(Json(serde_json::json!({
                "total": formatted.len(),
                "memories": formatted,
            })));
        }
    }

    // No search query — list all memories
    let mut nodes = storage
        .get_all_nodes(limit, offset)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Apply filters
    if let Some(ref node_type) = params.node_type {
        nodes.retain(|n| n.node_type == *node_type);
    }
    if let Some(ref tag) = params.tag {
        nodes.retain(|n| n.tags.iter().any(|t| t == tag));
    }
    if let Some(min_ret) = params.min_retention {
        nodes.retain(|n| n.retention_strength >= min_ret);
    }

    let formatted: Vec<Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content,
                "nodeType": n.node_type,
                "tags": n.tags,
                "retentionStrength": n.retention_strength,
                "storageStrength": n.storage_strength,
                "retrievalStrength": n.retrieval_strength,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "source": n.source,
                "reviewCount": n.reps,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "total": formatted.len(),
        "memories": formatted,
    })))
}

/// Get a single memory by ID
pub async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let node = storage
        .get_node(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": node.id,
        "content": node.content,
        "nodeType": node.node_type,
        "tags": node.tags,
        "retentionStrength": node.retention_strength,
        "storageStrength": node.storage_strength,
        "retrievalStrength": node.retrieval_strength,
        "sentimentScore": node.sentiment_score,
        "sentimentMagnitude": node.sentiment_magnitude,
        "source": node.source,
        "createdAt": node.created_at.to_rfc3339(),
        "updatedAt": node.updated_at.to_rfc3339(),
        "lastAccessedAt": node.last_accessed.to_rfc3339(),
        "nextReviewAt": node.next_review.map(|dt| dt.to_rfc3339()),
        "reviewCount": node.reps,
        "validFrom": node.valid_from.map(|dt| dt.to_rfc3339()),
        "validUntil": node.valid_until.map(|dt| dt.to_rfc3339()),
    })))
}

/// Delete a memory by ID
pub async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let mut storage = state.storage.lock().await;
    let deleted = storage
        .delete_node(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Promote a memory
pub async fn promote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let node = storage
        .promote_memory(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "promoted": true,
        "id": node.id,
        "retentionStrength": node.retention_strength,
    })))
}

/// Demote a memory
pub async fn demote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let node = storage
        .demote_memory(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "demoted": true,
        "id": node.id,
        "retentionStrength": node.retention_strength,
    })))
}

/// Get system stats
pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let stats = storage
        .get_stats()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "withEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": embedding_coverage,
        "embeddingModel": stats.embedding_model,
        "oldestMemory": stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|dt| dt.to_rfc3339()),
    })))
}

#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    pub days: Option<i64>,
    pub limit: Option<i32>,
}

/// Get timeline data
pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<TimelineParams>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let days = params.days.unwrap_or(7).clamp(1, 90);
    let limit = params.limit.unwrap_or(200).clamp(1, 500);

    let start = Utc::now() - Duration::days(days);
    let nodes = storage
        .query_time_range(Some(start), Some(Utc::now()), limit)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Group by day
    let mut by_day: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
    for node in &nodes {
        let date = node.created_at.format("%Y-%m-%d").to_string();
        let content_preview: String = {
            let preview: String = node.content.chars().take(100).collect();
            if preview.len() < node.content.len() {
                format!("{}...", preview)
            } else {
                preview
            }
        };
        by_day.entry(date).or_default().push(serde_json::json!({
            "id": node.id,
            "content": content_preview,
            "nodeType": node.node_type,
            "retentionStrength": node.retention_strength,
            "createdAt": node.created_at.to_rfc3339(),
        }));
    }

    let timeline: Vec<Value> = by_day
        .into_iter()
        .rev()
        .map(|(date, memories)| {
            serde_json::json!({
                "date": date,
                "count": memories.len(),
                "memories": memories,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "days": days,
        "totalMemories": nodes.len(),
        "timeline": timeline,
    })))
}

/// Health check
pub async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let storage = state.storage.lock().await;
    let stats = storage
        .get_stats()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    Ok(Json(serde_json::json!({
        "status": status,
        "totalMemories": stats.total_nodes,
        "averageRetention": stats.average_retention,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
