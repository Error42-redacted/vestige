//! Unified Memory Tool
//!
//! Merges get_knowledge, delete_knowledge, and get_memory_state into a single
//! `memory` tool with action-based dispatch.

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{MemoryState, Modification, OutcomeType, Storage};

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
                "enum": ["get", "get_batch", "delete", "purge", "state", "promote", "demote", "edit", "supersede"],
                "description": "Action to perform: 'get' retrieves full memory node, 'get_batch' retrieves multiple memories by IDs (use 'ids' array), 'purge' permanently removes memory content and embeddings after confirm=true, 'delete' is a backwards-compatible alias for purge and also requires confirm=true, 'state' returns accessibility state, 'promote' increases retrieval strength (thumbs up), 'demote' decreases retrieval strength (thumbs down), 'edit' updates content and/or tags in-place (preserves FSRS state), 'supersede' marks memory 'id' as superseded by 'winnerId' (demotes + stamps valid_until/superseded_by, keeps it queryable for audit, reversible via dedup undo; refuses protected targets)"
            },
            "id": {
                "type": "string",
                "description": "The ID of the memory node (for single-memory actions)"
            },
            "ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Array of memory IDs (for get_batch action). Max 20 IDs per call."
            },
            "reason": {
                "type": "string",
                "description": "Why this memory is being promoted/demoted/purged (optional, for logging)."
            },
            "confirm": {
                "type": "boolean",
                "description": "Required for action='purge' and action='delete'. Purge/delete permanently removes memory content and embeddings; only a non-content tombstone remains.",
                "default": false
            },
            "content": {
                "type": "string",
                "description": "New content for edit action. Replaces existing content, regenerates embedding, preserves FSRS state. May be combined with 'tags'."
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "For edit action: replaces the memory's FULL tag array (v2.2.9). FSRS state, content, and embedding untouched; the FTS keyword index syncs automatically. Omit to leave tags unchanged; [] clears all tags. Tags are trimmed, empties dropped, exact duplicates removed. May be combined with 'content'."
            },
            "winnerId": {
                "type": "string",
                "description": "For action='supersede': the memory that replaces 'id'. The loser ('id') is demoted and bitemporally stamped (valid_until + superseded_by); the operation is fully reversible via dedup undo (stamps and FSRS state restored)."
            }
        },
        "required": ["action"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryArgs {
    action: String,
    id: Option<String>,
    ids: Option<Vec<String>>,
    reason: Option<String>,
    confirm: Option<bool>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    winner_id: Option<String>,
}

/// Execute the unified memory tool
pub async fn execute(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: MemoryArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => return Err("Missing arguments".to_string()),
    };

    // get_batch uses 'ids' array, all other actions use 'id'
    if args.action == "get_batch" {
        let ids = args.ids.ok_or("get_batch requires 'ids' array")?;
        if ids.is_empty() {
            return Err("ids array cannot be empty".to_string());
        }
        if ids.len() > 20 {
            return Err("get_batch supports max 20 IDs per call".to_string());
        }
        for id in &ids {
            uuid::Uuid::parse_str(id).map_err(|_| format!("Invalid memory ID format: {}", id))?;
        }
        return execute_get_batch(storage, &ids).await;
    }

    // All other actions require 'id'
    let id = args.id.ok_or("This action requires 'id' parameter")?;
    uuid::Uuid::parse_str(&id).map_err(|_| "Invalid memory ID format".to_string())?;

    match args.action.as_str() {
        "get" => execute_get(storage, &id).await,
        "delete" => {
            execute_purge(
                storage,
                &id,
                args.reason,
                args.confirm.unwrap_or(false),
                "delete",
            )
            .await
        }
        "purge" => {
            execute_purge(
                storage,
                &id,
                args.reason,
                args.confirm.unwrap_or(false),
                "purge",
            )
            .await
        }
        "state" => execute_state(storage, &id).await,
        "promote" => execute_promote(storage, cognitive, &id, args.reason).await,
        "demote" => execute_demote(storage, cognitive, &id, args.reason).await,
        "edit" => execute_edit(storage, &id, args.content, args.tags).await,
        "supersede" => execute_supersede(storage, &id, args.winner_id).await,
        _ => Err(format!(
            "Invalid action '{}'. Must be one of: get, get_batch, delete, purge, state, promote, demote, edit, supersede",
            args.action
        )),
    }
}

/// v2.2.5 "Consent to Supersede": confirm-after-suggest surface. Marks `id`
/// (the loser) as superseded by `winner_id` via the unified enriched helper —
/// demote + bitemporal stamp (valid_until + superseded_by) + reversible
/// MergeOperation. The loser stays queryable for audit; protected losers are
/// refused.
async fn execute_supersede(
    storage: &Arc<Storage>,
    loser_id: &str,
    winner_id: Option<String>,
) -> Result<Value, String> {
    let winner_id = winner_id
        .ok_or("action='supersede' requires 'winnerId' (the memory that replaces 'id')")?;
    uuid::Uuid::parse_str(&winner_id).map_err(|_| "Invalid winnerId format".to_string())?;
    if loser_id == winner_id {
        return Err("'id' (the superseded memory) and 'winnerId' must differ".to_string());
    }

    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let op = storage
            .supersede_memory(loser_id, &winner_id)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "action": "supersede",
            "success": true,
            "loserId": loser_id,
            "winnerId": winner_id,
            "operationId": op.id,
            "message": format!(
                "Memory {} superseded by {}: demoted + stamped valid_until/superseded_by, still queryable for audit. Fully reversible via dedup {{action:'undo', operation_id:'{}'}} (restores stamps AND pre-supersede FSRS state).",
                loser_id, winner_id, op.id
            ),
        }))
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let _ = (storage, loser_id, winner_id);
        Err("supersede requires a build with the embeddings + vector-search features".to_string())
    }
}

/// Get full memory node with all metadata
async fn execute_get(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
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
        None => {
            // v2.2.4: a purged (or auto-dedup-merged) memory leaves a
            // content-free deletion tombstone — surface it instead of a bare
            // "not found" so callers can tell "removed with audit trail"
            // apart from "never existed".
            if let Ok(Some(tombstone)) = storage.get_deletion_tombstone(id) {
                return Ok(serde_json::json!({
                    "action": "get",
                    "found": false,
                    "nodeId": id,
                    "tombstone": {
                        "deletedAt": tombstone.deleted_at,
                        "reason": tombstone.reason,
                        "nodeType": tombstone.node_type,
                        "tags": tombstone.tags,
                    },
                    "message": "Memory purged; content and embeddings removed. Non-content tombstone retained for audit.",
                }));
            }
            Ok(serde_json::json!({
                "action": "get",
                "found": false,
                "nodeId": id,
                "message": "Memory not found",
            }))
        }
    }
}

/// Get multiple full memory nodes by ID (batch retrieval for expandable IDs)
async fn execute_get_batch(storage: &Arc<Storage>, ids: &[String]) -> Result<Value, String> {
    let mut results = Vec::with_capacity(ids.len());
    let mut found_count = 0;

    for id in ids {
        match storage.get_node(id) {
            Ok(Some(n)) => {
                found_count += 1;
                results.push(serde_json::json!({
                    "id": n.id,
                    "content": n.content,
                    "nodeType": n.node_type,
                    "createdAt": n.created_at.to_rfc3339(),
                    "updatedAt": n.updated_at.to_rfc3339(),
                    "tags": n.tags,
                    "retentionStrength": n.retention_strength,
                    "source": n.source,
                }));
            }
            Ok(None) => {
                results.push(serde_json::json!({
                    "id": id,
                    "found": false,
                }));
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "id": id,
                    "error": e.to_string(),
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "action": "get_batch",
        "requested": ids.len(),
        "found": found_count,
        "results": results,
    }))
}

/// Permanently purge a memory and return cleanup details.
async fn execute_purge(
    storage: &Arc<Storage>,
    id: &str,
    reason: Option<String>,
    confirm: bool,
    action: &str,
) -> Result<Value, String> {
    if !confirm {
        return Err(
            "Purge is irreversible. Pass confirm=true to permanently remove memory content and embeddings."
                .to_string(),
        );
    }

    let report = storage
        .purge_node(id, reason.as_deref())
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "action": action,
        "success": report.deleted,
        "nodeId": id,
        "message": if report.deleted {
            "Memory purged permanently; content and embeddings removed. Non-content tombstone retained for sync/audit."
        } else {
            "Memory not found"
        },
        "deletedAt": report.deleted_at.to_rfc3339(),
        "edgesPruned": report.edges_pruned,
        "insightsRewritten": report.insights_rewritten,
        "insightsDeleted": report.insights_deleted,
        "childrenOrphaned": report.children_orphaned,
    }))
}

/// Get accessibility state of a memory (Active/Dormant/Silent/Unavailable)
async fn execute_state(storage: &Arc<Storage>, id: &str) -> Result<Value, String> {
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

/// Promote a memory (thumbs up) — increases retrieval strength with cognitive feedback pipeline
async fn execute_promote(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    id: &str,
    reason: Option<String>,
) -> Result<Value, String> {
    let before = storage
        .get_node(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node not found: {}", id))?;

    let node = storage.promote_memory(id).map_err(|e| e.to_string())?;

    // Cognitive feedback pipeline
    if let Ok(mut cog) = cognitive.try_lock() {
        cog.reward_signal.record_outcome(id, OutcomeType::Helpful);
        cog.importance_tracker.on_retrieved(id, true);
        if cog.reconsolidation.is_labile(id) {
            cog.reconsolidation.apply_modification(
                id,
                Modification::StrengthenConnection {
                    target_memory_id: id.to_string(),
                    boost: 0.2,
                },
            );
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "action": "promoted",
        "nodeId": node.id,
        "reason": reason,
        "changes": {
            "retrievalStrength": {
                "before": before.retrieval_strength,
                "after": node.retrieval_strength,
                "delta": "+0.20"
            },
            "retentionStrength": {
                "before": before.retention_strength,
                "after": node.retention_strength,
                "delta": "+0.10"
            },
            "stability": {
                "before": before.stability,
                "after": node.stability,
                "multiplier": "1.5x"
            }
        },
        "message": format!("Memory promoted. It will now surface more often in searches. Retrieval: {:.2} -> {:.2}",
            before.retrieval_strength, node.retrieval_strength),
    }))
}

/// Demote a memory (thumbs down) — decreases retrieval strength with cognitive feedback pipeline
async fn execute_demote(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    id: &str,
    reason: Option<String>,
) -> Result<Value, String> {
    let before = storage
        .get_node(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node not found: {}", id))?;

    let node = storage.demote_memory(id).map_err(|e| e.to_string())?;

    // Cognitive feedback pipeline
    if let Ok(mut cog) = cognitive.try_lock() {
        cog.reward_signal
            .record_outcome(id, OutcomeType::NotHelpful);
        cog.importance_tracker.on_retrieved(id, false);
        if cog.reconsolidation.is_labile(id) {
            cog.reconsolidation.apply_modification(
                id,
                Modification::AddContext {
                    context: "User reported this memory was wrong/unhelpful".to_string(),
                },
            );
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "action": "demoted",
        "nodeId": node.id,
        "reason": reason,
        "changes": {
            "retrievalStrength": {
                "before": before.retrieval_strength,
                "after": node.retrieval_strength,
                "delta": "-0.30"
            },
            "retentionStrength": {
                "before": before.retention_strength,
                "after": node.retention_strength,
                "delta": "-0.15"
            },
            "stability": {
                "before": before.stability,
                "after": node.stability,
                "multiplier": "0.5x"
            }
        },
        "message": format!("Memory demoted. Better alternatives will now surface instead. Retrieval: {:.2} -> {:.2}",
            before.retrieval_strength, node.retrieval_strength),
        "note": "Memory is NOT deleted - it remains searchable but ranks lower."
    }))
}

/// Edit a memory's content and/or tags in-place — preserves FSRS state.
/// Content edits regenerate the embedding; tag edits (v2.2.9 "Convergent
/// Evolution") replace the full tag array without touching the embedding.
async fn execute_edit(
    storage: &Arc<Storage>,
    id: &str,
    content: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<Value, String> {
    if content.is_none() && tags.is_none() {
        return Err(
            "Edit action requires 'content' and/or 'tags'. Pass 'content' to rewrite the memory, 'tags' to replace its tag array ([] clears), or both.".to_string(),
        );
    }

    if let Some(ref c) = content
        && c.trim().is_empty()
    {
        return Err("Content cannot be empty".to_string());
    }

    // Get existing node to capture old content/tags (and verify existence)
    let old_node = storage
        .get_node(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Memory not found: {}", id))?;

    let mut response = serde_json::json!({
        "success": true,
        "action": "edit",
        "nodeId": id,
    });
    let mut notes: Vec<&str> =
        vec!["FSRS state preserved (stability, difficulty, reps, lapses unchanged)."];

    if let Some(new_content) = content {
        // Update content (regenerates embedding, syncs FTS5)
        storage
            .update_node_content(id, &new_content)
            .map_err(|e| e.to_string())?;

        // Truncate previews for response (char-safe to avoid UTF-8 panics)
        let old_preview = if old_node.content.chars().count() > 200 {
            let truncated: String = old_node.content.chars().take(197).collect();
            format!("{}...", truncated)
        } else {
            old_node.content.clone()
        };
        let new_preview = if new_content.chars().count() > 200 {
            let truncated: String = new_content.chars().take(197).collect();
            format!("{}...", truncated)
        } else {
            new_content.clone()
        };
        response["oldContentPreview"] = serde_json::json!(old_preview);
        response["newContentPreview"] = serde_json::json!(new_preview);
        notes.push("Embedding regenerated for new content.");
    }

    if let Some(raw_tags) = tags {
        // Normalize: trim, drop empties, dedupe exact duplicates (order kept).
        // No case-folding — tag casing conventions are the user's, not the engine's.
        let mut new_tags: Vec<String> = Vec::with_capacity(raw_tags.len());
        for t in raw_tags {
            let trimmed = t.trim();
            if !trimmed.is_empty() && !new_tags.iter().any(|existing| existing == trimmed) {
                new_tags.push(trimmed.to_string());
            }
        }

        storage
            .update_node_tags(id, &new_tags)
            .map_err(|e| e.to_string())?;

        response["oldTags"] = serde_json::json!(old_node.tags);
        response["newTags"] = serde_json::json!(new_tags);
        notes.push("Tags replaced; embedding untouched (embeddings derive from content only), FTS keyword index synced.");
    }

    response["note"] = serde_json::json!(notes.join(" "));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_thresholds() {
        // Test Active state
        let accessibility = compute_accessibility(0.9, 0.8, 0.7);
        assert!(accessibility >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Active
        ));

        // Test Dormant state
        let accessibility = compute_accessibility(0.5, 0.5, 0.5);
        assert!((ACCESSIBILITY_DORMANT..ACCESSIBILITY_ACTIVE).contains(&accessibility));
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Dormant
        ));

        // Test Silent state
        let accessibility = compute_accessibility(0.2, 0.2, 0.2);
        assert!((ACCESSIBILITY_SILENT..ACCESSIBILITY_DORMANT).contains(&accessibility));
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Silent
        ));

        // Test Unavailable state
        let accessibility = compute_accessibility(0.05, 0.05, 0.05);
        assert!(accessibility < ACCESSIBILITY_SILENT);
        assert!(matches!(
            state_from_accessibility(accessibility),
            MemoryState::Unavailable
        ));
    }

    #[test]
    fn test_schema_structure() {
        let schema = schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["id"].is_object());
        assert!(schema["properties"]["reason"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["action"]));
        assert!(schema["properties"]["ids"].is_object()); // get_batch support
        // Verify all 9 actions are in enum
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 9);
        assert!(actions.contains(&serde_json::json!("get_batch")));
        assert!(actions.contains(&serde_json::json!("purge")));
        assert!(actions.contains(&serde_json::json!("edit")));
        assert!(actions.contains(&serde_json::json!("promote")));
        assert!(actions.contains(&serde_json::json!("demote")));
        assert!(actions.contains(&serde_json::json!("supersede")));
        assert!(schema["properties"]["confirm"].is_object());
        assert!(schema["properties"]["winnerId"].is_object()); // supersede support
    }

    // === INTEGRATION TESTS ===

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn ingest_memory(storage: &Arc<Storage>) -> String {
        let node = storage
            .ingest(vestige_core::IngestInput {
                content: "Memory unified test content".to_string(),
                node_type: "fact".to_string(),
                source: Some("test".to_string()),
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec!["test-tag".to_string()],
                valid_from: None,
                valid_until: None,
                source_envelope: None,
            })
            .unwrap();
        node.id
    }

    #[tokio::test]
    async fn test_missing_args_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    #[tokio::test]
    async fn test_invalid_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "invalid", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid action"));
    }

    #[tokio::test]
    async fn test_invalid_uuid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "get", "id": "not-a-uuid" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_get_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let args =
            serde_json::json!({ "action": "get", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["found"], false);
        assert_eq!(value["message"], "Memory not found");
    }

    #[tokio::test]
    async fn test_delete_requires_confirm() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "delete", "id": id.clone() });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("confirm=true"));
        assert!(storage.get_node(&id).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_existing_memory_with_confirm() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "delete", "id": id, "confirm": true });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "delete");
        assert_eq!(value["success"], true);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_memory() {
        let (storage, _dir) = test_storage().await;
        // Ingest+delete a throwaway memory to warm writer after WAL migration
        let warmup_id = storage
            .ingest(vestige_core::IngestInput {
                content: "warmup".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let _ = storage.delete_node(&warmup_id);
        let args = serde_json::json!({
            "action": "delete",
            "id": "00000000-0000-0000-0000-000000000000",
            "confirm": true
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], false);
        assert!(value["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_then_get_returns_not_found() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let del_args = serde_json::json!({ "action": "delete", "id": id, "confirm": true });
        execute(&storage, &test_cognitive(), Some(del_args))
            .await
            .unwrap();
        let get_args = serde_json::json!({ "action": "get", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(get_args)).await;
        let value = result.unwrap();
        assert_eq!(value["found"], false);
    }

    #[tokio::test]
    async fn test_purge_requires_confirm() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "purge", "id": id.clone() });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("confirm=true"));
        assert!(storage.get_node(&id).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_purge_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({
            "action": "purge",
            "id": id,
            "confirm": true,
            "reason": "test cleanup"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["action"], "purge");
        assert_eq!(value["success"], true);
        assert!(
            value["message"]
                .as_str()
                .unwrap()
                .contains("purged permanently")
        );
        assert_eq!(value["edgesPruned"], 0);
        assert!(storage.get_node(&id).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_state_existing_memory() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "state", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
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
        let args =
            serde_json::json!({ "action": "state", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_accessibility_boundary_active() {
        let a = compute_accessibility(1.0, 0.7, 0.5);
        assert!(a >= ACCESSIBILITY_ACTIVE);
        assert!(matches!(state_from_accessibility(a), MemoryState::Active));
    }

    #[test]
    fn test_accessibility_boundary_zero() {
        let a = compute_accessibility(0.0, 0.0, 0.0);
        assert_eq!(a, 0.0);
        assert!(matches!(
            state_from_accessibility(a),
            MemoryState::Unavailable
        ));
    }

    // ========================================================================
    // PROMOTE/DEMOTE TESTS (ported from feedback.rs, v1.7.0 merge)
    // ========================================================================

    #[tokio::test]
    async fn test_promote_missing_id_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "promote", "id": "not-a-uuid" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_promote_nonexistent_node_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "promote", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Node not found"));
    }

    #[tokio::test]
    async fn test_promote_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id, "reason": "It was helpful" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "promoted");
        assert_eq!(value["nodeId"], id);
        assert_eq!(value["reason"], "It was helpful");
        assert!(value["changes"]["retrievalStrength"].is_object());
    }

    #[tokio::test]
    async fn test_promote_without_reason_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert!(value["reason"].is_null());
    }

    #[tokio::test]
    async fn test_promote_changes_contain_expected_fields() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "promote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["changes"]["retrievalStrength"]["before"].is_number());
        assert!(value["changes"]["retrievalStrength"]["after"].is_number());
        assert_eq!(value["changes"]["retrievalStrength"]["delta"], "+0.20");
        assert!(value["changes"]["retentionStrength"]["before"].is_number());
        assert_eq!(value["changes"]["retentionStrength"]["delta"], "+0.10");
        assert_eq!(value["changes"]["stability"]["multiplier"], "1.5x");
    }

    #[tokio::test]
    async fn test_demote_invalid_uuid_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "demote", "id": "bad-id" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory ID format"));
    }

    #[tokio::test]
    async fn test_demote_nonexistent_node_fails() {
        let (storage, _dir) = test_storage().await;
        let args =
            serde_json::json!({ "action": "demote", "id": "00000000-0000-0000-0000-000000000000" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Node not found"));
    }

    #[tokio::test]
    async fn test_demote_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "demote", "id": id, "reason": "It was wrong" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "demoted");
        assert_eq!(value["nodeId"], id);
        assert_eq!(value["reason"], "It was wrong");
        assert!(value["note"].as_str().unwrap().contains("NOT deleted"));
    }

    #[tokio::test]
    async fn test_demote_changes_contain_expected_fields() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "demote", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        let value = result.unwrap();
        assert!(value["changes"]["retrievalStrength"]["before"].is_number());
        assert_eq!(value["changes"]["retrievalStrength"]["delta"], "-0.30");
        assert_eq!(value["changes"]["retentionStrength"]["delta"], "-0.15");
        assert_eq!(value["changes"]["stability"]["multiplier"], "0.5x");
    }

    // ========================================================================
    // EDIT TESTS (v1.9.2)
    // ========================================================================

    #[tokio::test]
    async fn test_edit_succeeds() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": "Updated memory content"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "edit");
        assert_eq!(value["nodeId"], id);
        assert!(
            value["oldContentPreview"]
                .as_str()
                .unwrap()
                .contains("Memory unified test content")
        );
        assert!(
            value["newContentPreview"]
                .as_str()
                .unwrap()
                .contains("Updated memory content")
        );
        assert!(
            value["note"]
                .as_str()
                .unwrap()
                .contains("FSRS state preserved")
        );
    }

    #[tokio::test]
    async fn test_edit_preserves_fsrs_state() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;

        // Get FSRS state before edit
        let before = storage.get_node(&id).unwrap().unwrap();

        // Edit content
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": "Completely new content after edit"
        });
        execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        // Verify FSRS state preserved
        let after = storage.get_node(&id).unwrap().unwrap();
        assert_eq!(after.stability, before.stability);
        assert_eq!(after.difficulty, before.difficulty);
        assert_eq!(after.reps, before.reps);
        assert_eq!(after.lapses, before.lapses);
        assert_eq!(after.retention_strength, before.retention_strength);
        // Content should be updated
        assert_eq!(after.content, "Completely new content after edit");
        assert_ne!(after.content, before.content);
    }

    #[tokio::test]
    async fn test_edit_missing_content_fails() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "edit", "id": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("content"));
    }

    #[tokio::test]
    async fn test_edit_empty_content_fails() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "edit", "id": id, "content": "  " });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_edit_nonexistent_memory_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "edit",
            "id": "00000000-0000-0000-0000-000000000000",
            "content": "New content"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_with_multibyte_utf8_content() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        // Content with emoji and CJK characters (multi-byte UTF-8)
        let long_content = "🧠".repeat(100); // 100 brain emoji = 400 bytes but only 100 chars
        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": long_content
        });
        // This must NOT panic (previous code would panic on byte-level truncation)
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["success"], true);
    }

    // === RETAG VIA EDIT (v2.2.9 "Convergent Evolution") ===

    #[tokio::test]
    async fn test_edit_tags_only_replaces_tags_preserves_content_and_fsrs() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let before = storage.get_node(&id).unwrap().unwrap();

        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "tags": ["rotn-clean-edition", "upstream-check"]
        });
        let value = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(value["success"], true);
        assert_eq!(value["oldTags"], serde_json::json!(["test-tag"]));
        assert_eq!(
            value["newTags"],
            serde_json::json!(["rotn-clean-edition", "upstream-check"])
        );
        // A tags-only edit must not emit content previews
        assert!(value.get("oldContentPreview").is_none());

        let after = storage.get_node(&id).unwrap().unwrap();
        assert_eq!(
            after.tags,
            vec!["rotn-clean-edition".to_string(), "upstream-check".to_string()]
        );
        assert_eq!(after.content, before.content, "content untouched");
        assert_eq!(after.stability, before.stability);
        assert_eq!(after.difficulty, before.difficulty);
        assert_eq!(after.reps, before.reps);
        assert_eq!(after.lapses, before.lapses);
        assert_eq!(after.retention_strength, before.retention_strength);
    }

    #[tokio::test]
    async fn test_edit_content_and_tags_together() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;

        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "content": "Rewritten content with fresh tags",
            "tags": ["combined-edit"]
        });
        let value = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(value["success"], true);
        assert!(
            value["newContentPreview"]
                .as_str()
                .unwrap()
                .contains("Rewritten content")
        );
        assert_eq!(value["newTags"], serde_json::json!(["combined-edit"]));

        let after = storage.get_node(&id).unwrap().unwrap();
        assert_eq!(after.content, "Rewritten content with fresh tags");
        assert_eq!(after.tags, vec!["combined-edit".to_string()]);
    }

    #[tokio::test]
    async fn test_edit_empty_tags_array_clears_tags() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;

        let args = serde_json::json!({ "action": "edit", "id": id, "tags": [] });
        let value = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(value["newTags"], serde_json::json!([]));
        let after = storage.get_node(&id).unwrap().unwrap();
        assert!(after.tags.is_empty(), "explicit [] must clear all tags");
    }

    #[tokio::test]
    async fn test_edit_tags_are_trimmed_deduped_and_empties_dropped() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;

        let args = serde_json::json!({
            "action": "edit",
            "id": id,
            "tags": ["  vestige  ", "vestige", "", "   ", "dev-backlog"]
        });
        let value = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(
            value["newTags"],
            serde_json::json!(["vestige", "dev-backlog"])
        );
    }

    // === SUPERSEDE ACTION (v2.2.5 "Consent to Supersede") ===

    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    #[tokio::test]
    async fn test_supersede_demotes_and_stamps_loser() {
        let (storage, _dir) = test_storage().await;
        let loser = ingest_memory(&storage).await;
        let winner = ingest_memory(&storage).await;
        let before = storage.get_node(&loser).unwrap().unwrap();

        let args = serde_json::json!({
            "action": "supersede",
            "id": loser,
            "winnerId": winner
        });
        let value = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "supersede");
        assert_eq!(value["loserId"], serde_json::json!(loser));
        assert_eq!(value["winnerId"], serde_json::json!(winner));
        assert!(value["operationId"].is_string(), "reversible operation id returned");

        // Enriched end-state: demoted + stamped + still queryable.
        assert!(storage.superseded_node_ids().unwrap().contains(&loser));
        let after = storage.get_node(&loser).unwrap().unwrap();
        assert!(after.retrieval_strength < before.retrieval_strength);
        assert_eq!(after.content, before.content, "content untouched, audit-queryable");
    }

    #[tokio::test]
    async fn test_supersede_requires_winner_id() {
        let (storage, _dir) = test_storage().await;
        let loser = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "supersede", "id": loser });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("winnerId"));
    }

    #[tokio::test]
    async fn test_supersede_rejects_same_ids() {
        let (storage, _dir) = test_storage().await;
        let id = ingest_memory(&storage).await;
        let args = serde_json::json!({ "action": "supersede", "id": id, "winnerId": id });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must differ"));
    }

    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    #[tokio::test]
    async fn test_supersede_refuses_protected_loser() {
        let (storage, _dir) = test_storage().await;
        let loser = ingest_memory(&storage).await;
        let winner = ingest_memory(&storage).await;
        storage.set_protected(&loser, true).unwrap();

        let args = serde_json::json!({
            "action": "supersede",
            "id": loser,
            "winnerId": winner
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("protected"));
        assert!(!storage.superseded_node_ids().unwrap().contains(&loser));
    }
}
