//! Dashboard API endpoint handlers
//!
//! v2.0: Adds cognitive operation endpoints (dream, explore, predict, importance, consolidation)

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::events::VestigeEvent;
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
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    if let Some(query) = params.q.as_ref().filter(|q| !q.trim().is_empty()) {
        // Use hybrid search
        let results = state.storage
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

    // No search query — list all memories
    let mut nodes = state.storage
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
    let node = state.storage
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
    let deleted = state.storage
        .delete_node(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        state.emit(VestigeEvent::MemoryDeleted {
            id: id.clone(),
            timestamp: chrono::Utc::now(),
        });
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
    let node = state.storage
        .promote_memory(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.emit(VestigeEvent::MemoryPromoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

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
    let node = state.storage
        .demote_memory(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.emit(VestigeEvent::MemoryDemoted {
        id: node.id.clone(),
        new_retention: node.retention_strength,
        timestamp: chrono::Utc::now(),
    });

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
    let stats = state.storage
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
    let days = params.days.unwrap_or(7).clamp(1, 90);
    let limit = params.limit.unwrap_or(200).clamp(1, 500);

    let start = Utc::now() - Duration::days(days);
    let nodes = state.storage
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
    let stats = state.storage
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

// ============================================================================
// MEMORY GRAPH
// ============================================================================

/// Serve the memory graph visualization HTML
pub async fn serve_graph() -> Html<&'static str> {
    Html(include_str!("../graph.html"))
}

#[derive(Debug, Deserialize)]
pub struct GraphParams {
    pub query: Option<String>,
    pub center_id: Option<String>,
    pub depth: Option<u32>,
    pub max_nodes: Option<usize>,
}

/// Get memory graph data (nodes + edges with layout positions)
pub async fn get_graph(
    State(state): State<AppState>,
    Query(params): Query<GraphParams>,
) -> Result<Json<Value>, StatusCode> {
    let depth = params.depth.unwrap_or(2).clamp(1, 3);
    let max_nodes = params.max_nodes.unwrap_or(50).clamp(1, 200);

    // Determine center node
    let center_id = if let Some(ref id) = params.center_id {
        id.clone()
    } else if let Some(ref query) = params.query {
        let results = state.storage
            .search(query, 1)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        results.first()
            .map(|n| n.id.clone())
            .ok_or(StatusCode::NOT_FOUND)?
    } else {
        // Default: most recent memory
        let recent = state.storage
            .get_all_nodes(1, 0)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        recent.first()
            .map(|n| n.id.clone())
            .ok_or(StatusCode::NOT_FOUND)?
    };

    // Get subgraph
    let (nodes, edges) = state.storage
        .get_memory_subgraph(&center_id, depth, max_nodes)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if nodes.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Build nodes JSON with timestamps for recency calculation
    let nodes_json: Vec<Value> = nodes.iter()
        .map(|n| {
            let label = if n.content.chars().count() > 80 {
                format!("{}...", n.content.chars().take(77).collect::<String>())
            } else {
                n.content.clone()
            };
            serde_json::json!({
                "id": n.id,
                "label": label,
                "type": n.node_type,
                "retention": n.retention_strength,
                "tags": n.tags,
                "createdAt": n.created_at.to_rfc3339(),
                "updatedAt": n.updated_at.to_rfc3339(),
                "isCenter": n.id == center_id,
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges.iter()
        .map(|e| {
            serde_json::json!({
                "source": e.source_id,
                "target": e.target_id,
                "weight": e.strength,
                "type": e.link_type,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "nodes": nodes_json,
        "edges": edges_json,
        "center_id": center_id,
        "depth": depth,
        "nodeCount": nodes.len(),
        "edgeCount": edges.len(),
    })))
}

// ============================================================================
// SEARCH (dedicated endpoint)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<i32>,
    pub min_retention: Option<f64>,
}

/// Search memories with hybrid search
pub async fn search_memories(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let start = std::time::Instant::now();

    let results = state
        .storage
        .hybrid_search(&params.q, limit, 0.3, 0.7)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let result_ids: Vec<String> = results.iter().map(|r| r.node.id.clone()).collect();

    // Emit search event
    state.emit(VestigeEvent::SearchPerformed {
        query: params.q.clone(),
        result_count: results.len(),
        result_ids: result_ids.clone(),
        duration_ms,
        timestamp: Utc::now(),
    });

    let formatted: Vec<Value> = results
        .into_iter()
        .filter(|r| {
            params
                .min_retention
                .is_none_or(|min| r.node.retention_strength >= min)
        })
        .map(|r| {
            serde_json::json!({
                "id": r.node.id,
                "content": r.node.content,
                "nodeType": r.node.node_type,
                "tags": r.node.tags,
                "retentionStrength": r.node.retention_strength,
                "combinedScore": r.combined_score,
                "createdAt": r.node.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "query": params.q,
        "total": formatted.len(),
        "durationMs": duration_ms,
        "results": formatted,
    })))
}

// ============================================================================
// COGNITIVE OPERATIONS (v2.0)
// ============================================================================

/// Trigger a dream cycle via CognitiveEngine
pub async fn trigger_dream(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let cognitive = state.cognitive.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let start = std::time::Instant::now();
    let memory_count: usize = 50;

    // Load memories for dreaming
    let all_nodes = state
        .storage
        .get_all_nodes(memory_count as i32, 0)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if all_nodes.len() < 5 {
        return Ok(Json(serde_json::json!({
            "status": "insufficient_memories",
            "message": format!("Need at least 5 memories. Current: {}", all_nodes.len()),
        })));
    }

    // Emit start event
    state.emit(VestigeEvent::DreamStarted {
        memory_count: all_nodes.len(),
        timestamp: Utc::now(),
    });

    // Build dream memories
    let dream_memories: Vec<vestige_core::DreamMemory> = all_nodes
        .iter()
        .map(|n| vestige_core::DreamMemory {
            id: n.id.clone(),
            content: n.content.clone(),
            embedding: state.storage.get_node_embedding(&n.id).ok().flatten(),
            tags: n.tags.clone(),
            created_at: n.created_at,
            access_count: n.reps as u32,
        })
        .collect();

    // Run dream through CognitiveEngine
    let cog = cognitive.lock().await;
    let pre_dream_count = cog.dreamer.get_connections().len();
    let dream_result = cog.dreamer.dream(&dream_memories).await;
    let insights = cog.dreamer.synthesize_insights(&dream_memories);
    let all_connections = cog.dreamer.get_connections();
    drop(cog);

    // Persist new connections
    let new_connections = &all_connections[pre_dream_count..];
    let mut connections_persisted = 0u64;
    let now = Utc::now();
    for conn in new_connections {
        let link_type = match conn.connection_type {
            vestige_core::DiscoveredConnectionType::Semantic => "semantic",
            vestige_core::DiscoveredConnectionType::SharedConcept => "shared_concepts",
            vestige_core::DiscoveredConnectionType::Temporal => "temporal",
            vestige_core::DiscoveredConnectionType::Complementary => "complementary",
            vestige_core::DiscoveredConnectionType::CausalChain => "causal",
        };
        let record = vestige_core::ConnectionRecord {
            source_id: conn.from_id.clone(),
            target_id: conn.to_id.clone(),
            strength: conn.similarity,
            link_type: link_type.to_string(),
            created_at: now,
            last_activated: now,
            activation_count: 1,
        };
        if state.storage.save_connection(&record).is_ok() {
            connections_persisted += 1;
        }

        // Emit connection events
        state.emit(VestigeEvent::ConnectionDiscovered {
            source_id: conn.from_id.clone(),
            target_id: conn.to_id.clone(),
            connection_type: link_type.to_string(),
            weight: conn.similarity,
            timestamp: now,
        });
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    // Emit completion event
    state.emit(VestigeEvent::DreamCompleted {
        memories_replayed: dream_memories.len(),
        connections_found: connections_persisted as usize,
        insights_generated: insights.len(),
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "status": "dreamed",
        "memoriesReplayed": dream_memories.len(),
        "connectionsPersisted": connections_persisted,
        "insights": insights.iter().map(|i| serde_json::json!({
            "type": format!("{:?}", i.insight_type),
            "insight": i.insight,
            "sourceMemories": i.source_memories,
            "confidence": i.confidence,
            "noveltyScore": i.novelty_score,
        })).collect::<Vec<Value>>(),
        "stats": {
            "newConnectionsFound": dream_result.new_connections_found,
            "connectionsPersisted": connections_persisted,
            "memoriesStrengthened": dream_result.memories_strengthened,
            "memoriesCompressed": dream_result.memories_compressed,
            "insightsGenerated": dream_result.insights_generated.len(),
            "durationMs": duration_ms,
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct ExploreRequest {
    pub from_id: String,
    pub to_id: Option<String>,
    pub action: Option<String>, // "associations", "chains", "bridges"
    pub limit: Option<usize>,
}

/// Explore connections between memories
pub async fn explore_connections(
    State(state): State<AppState>,
    Json(req): Json<ExploreRequest>,
) -> Result<Json<Value>, StatusCode> {
    let action = req.action.as_deref().unwrap_or("associations");
    let limit = req.limit.unwrap_or(10).clamp(1, 50);

    match action {
        "associations" => {
            // Get the source memory content for similarity search
            let source_node = state
                .storage
                .get_node(&req.from_id)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;

            // Use hybrid search with source content to find associated memories
            let results = state
                .storage
                .hybrid_search(&source_node.content, limit as i32, 0.3, 0.7)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let formatted: Vec<Value> = results
                .iter()
                .filter(|r| r.node.id != req.from_id) // Exclude self
                .map(|r| {
                    serde_json::json!({
                        "id": r.node.id,
                        "content": r.node.content,
                        "nodeType": r.node.node_type,
                        "score": r.combined_score,
                        "retention": r.node.retention_strength,
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "action": "associations",
                "fromId": req.from_id,
                "results": formatted,
            })))
        }
        "chains" | "bridges" => {
            let to_id = req.to_id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

            let (nodes, edges) = state
                .storage
                .get_memory_subgraph(&req.from_id, 2, limit)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let nodes_json: Vec<Value> = nodes
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "id": n.id,
                        "content": n.content.chars().take(100).collect::<String>(),
                        "nodeType": n.node_type,
                        "retention": n.retention_strength,
                    })
                })
                .collect();

            let edges_json: Vec<Value> = edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "source": e.source_id,
                        "target": e.target_id,
                        "weight": e.strength,
                        "type": e.link_type,
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "action": action,
                "fromId": req.from_id,
                "toId": to_id,
                "nodes": nodes_json,
                "edges": edges_json,
            })))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Predict which memories will be needed
pub async fn predict_memories(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    // Get recent memories as predictions based on activity
    let recent = state
        .storage
        .get_all_nodes(10, 0)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let predictions: Vec<Value> = recent
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "content": n.content.chars().take(100).collect::<String>(),
                "nodeType": n.node_type,
                "retention": n.retention_strength,
                "predictedNeed": "high",
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "predictions": predictions,
        "basedOn": "recent_activity",
    })))
}

#[derive(Debug, Deserialize)]
pub struct ImportanceRequest {
    pub content: String,
}

/// Score content importance using 4-channel model
pub async fn score_importance(
    State(state): State<AppState>,
    Json(req): Json<ImportanceRequest>,
) -> Result<Json<Value>, StatusCode> {
    if let Some(ref cognitive) = state.cognitive {
        let context = vestige_core::ImportanceContext::current();
        let cog = cognitive.lock().await;
        let score = cog.importance_signals.compute_importance(&req.content, &context);
        drop(cog);

        let composite = score.composite;
        let novelty = score.novelty;
        let arousal = score.arousal;
        let reward = score.reward;
        let attention = score.attention;

        state.emit(VestigeEvent::ImportanceScored {
            content_preview: req.content.chars().take(80).collect(),
            composite_score: composite,
            novelty,
            arousal,
            reward,
            attention,
            timestamp: Utc::now(),
        });

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": novelty,
                "arousal": arousal,
                "reward": reward,
                "attention": attention,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    } else {
        // Fallback: basic heuristic scoring
        let word_count = req.content.split_whitespace().count();
        let has_code = req.content.contains("```") || req.content.contains("fn ");
        let composite = if has_code { 0.7 } else { (word_count as f64 / 100.0).min(0.8) };

        Ok(Json(serde_json::json!({
            "composite": composite,
            "channels": {
                "novelty": composite,
                "arousal": 0.5,
                "reward": 0.5,
                "attention": composite,
            },
            "recommendation": if composite > 0.6 { "save" } else { "skip" },
        })))
    }
}

/// Trigger consolidation
pub async fn trigger_consolidation(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    state.emit(VestigeEvent::ConsolidationStarted {
        timestamp: Utc::now(),
    });

    let start = std::time::Instant::now();

    let result = state
        .storage
        .run_consolidation()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let duration_ms = start.elapsed().as_millis() as u64;

    state.emit(VestigeEvent::ConsolidationCompleted {
        nodes_processed: result.nodes_processed as usize,
        decay_applied: result.decay_applied as usize,
        embeddings_generated: result.embeddings_generated as usize,
        duration_ms,
        timestamp: Utc::now(),
    });

    Ok(Json(serde_json::json!({
        "nodesProcessed": result.nodes_processed,
        "decayApplied": result.decay_applied,
        "embeddingsGenerated": result.embeddings_generated,
        "duplicatesMerged": result.duplicates_merged,
        "activationsComputed": result.activations_computed,
        "durationMs": duration_ms,
    })))
}

/// Get retention distribution (for histogram visualization)
pub async fn retention_distribution(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let nodes = state
        .storage
        .get_all_nodes(10000, 0)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Build distribution buckets
    let mut buckets = [0u32; 10]; // 0-10%, 10-20%, ..., 90-100%
    let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut endangered = Vec::new();

    for node in &nodes {
        let bucket = ((node.retention_strength * 10.0).floor() as usize).min(9);
        buckets[bucket] += 1;
        *by_type.entry(node.node_type.clone()).or_default() += 1;

        // Endangered: retention below 30%
        if node.retention_strength < 0.3 {
            endangered.push(serde_json::json!({
                "id": node.id,
                "content": node.content.chars().take(60).collect::<String>(),
                "retention": node.retention_strength,
                "nodeType": node.node_type,
            }));
        }
    }

    let distribution: Vec<Value> = buckets
        .iter()
        .enumerate()
        .map(|(i, &count)| {
            serde_json::json!({
                "range": format!("{}-{}%", i * 10, (i + 1) * 10),
                "count": count,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "distribution": distribution,
        "byType": by_type,
        "endangered": endangered,
        "total": nodes.len(),
    })))
}

// ============================================================================
// INTENTIONS (v2.0)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct IntentionListParams {
    pub status: Option<String>,
}

/// List intentions
pub async fn list_intentions(
    State(state): State<AppState>,
    Query(params): Query<IntentionListParams>,
) -> Result<Json<Value>, StatusCode> {
    let status_filter = params.status.unwrap_or_else(|| "active".to_string());

    let intentions = if status_filter == "all" {
        // Get all statuses
        let mut all = state.storage.get_active_intentions()
            .unwrap_or_default();
        all.extend(state.storage.get_intentions_by_status("fulfilled").unwrap_or_default());
        all.extend(state.storage.get_intentions_by_status("cancelled").unwrap_or_default());
        all.extend(state.storage.get_intentions_by_status("snoozed").unwrap_or_default());
        all
    } else if status_filter == "active" {
        state.storage.get_active_intentions()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        state.storage.get_intentions_by_status(&status_filter)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let count = intentions.len();
    Ok(Json(serde_json::json!({
        "intentions": intentions,
        "total": count,
        "filter": status_filter,
    })))
}
