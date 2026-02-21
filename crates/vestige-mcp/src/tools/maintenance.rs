//! Maintenance MCP Tools
//!
//! Exposes CLI-only operations as MCP tools so Claude can trigger them automatically:
//! system_status, consolidate, backup, export, gc.

use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::advanced::compression::MemoryForCompression;
use vestige_core::{FSRSScheduler, MemoryLifecycle, MemoryState, Storage};

// ============================================================================
// SCHEMAS
// ============================================================================

/// Deprecated in v1.7 — use system_status_schema() instead
#[allow(dead_code)]
pub fn health_check_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

pub fn consolidate_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

/// Deprecated in v1.7 — use system_status_schema() instead
#[allow(dead_code)]
pub fn stats_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

pub fn backup_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

pub fn export_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "format": {
                "type": "string",
                "description": "Export format: 'json' (default) or 'jsonl'",
                "enum": ["json", "jsonl"],
                "default": "json"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filter by tags (ALL must match)"
            },
            "since": {
                "type": "string",
                "description": "Only export memories created after this date (YYYY-MM-DD)"
            },
            "path": {
                "type": "string",
                "description": "Custom filename (not path). File is saved in ~/.vestige/exports/. Default: memories-{timestamp}.{format}"
            }
        }
    })
}

pub fn gc_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "min_retention": {
                "type": "number",
                "description": "Delete memories with retention below this threshold (default: 0.1)",
                "default": 0.1,
                "minimum": 0.0,
                "maximum": 1.0
            },
            "max_age_days": {
                "type": "integer",
                "description": "Only delete memories older than this many days (optional additional filter)",
                "minimum": 1
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true (default), only report what would be deleted without actually deleting",
                "default": true
            }
        }
    })
}

/// Combined system status schema (replaces health_check + stats in v1.7.0)
pub fn system_status_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

// ============================================================================
// EXECUTE FUNCTIONS
// ============================================================================

/// Combined system status tool (merges health_check + stats, v1.7.0)
///
/// Returns system health status, full statistics, FSRS preview,
/// cognitive module health, state distribution, and actionable recommendations.
pub async fn execute_system_status(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    _args: Option<Value>,
) -> Result<Value, String> {
    let stats = storage.get_stats().map_err(|e| e.to_string())?;

    // === Health assessment ===
    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    let embedding_ready = storage.is_embedding_ready();

    let mut warnings = Vec::new();
    if stats.average_retention < 0.5 && stats.total_nodes > 0 {
        warnings.push("Low average retention - consider running consolidation");
    }
    if stats.nodes_due_for_review > 10 {
        warnings.push("Many memories are due for review");
    }
    if stats.total_nodes > 0 && stats.nodes_with_embeddings == 0 {
        warnings.push("No embeddings generated - semantic search unavailable");
    }
    if embedding_coverage < 50.0 && stats.total_nodes > 10 {
        warnings.push("Low embedding coverage - run consolidate to improve semantic search");
    }

    let mut recommendations = Vec::new();
    if status == "critical" {
        recommendations.push("CRITICAL: Many memories have very low retention. Review important memories.");
    }
    if stats.nodes_due_for_review > 5 {
        recommendations.push("Review due memories to strengthen retention.");
    }
    if stats.nodes_with_embeddings < stats.total_nodes {
        recommendations.push("Run 'consolidate' to generate missing embeddings.");
    }
    if stats.total_nodes > 100 && stats.average_retention < 0.7 {
        recommendations.push("Consider running periodic consolidation.");
    }
    if status == "healthy" && recommendations.is_empty() {
        recommendations.push("Memory system is healthy!");
    }

    // === State distribution ===
    let nodes = storage.get_all_nodes(500, 0).map_err(|e| e.to_string())?;
    let total = nodes.len();
    let (active, dormant, silent, unavailable) = if total > 0 {
        let mut a = 0usize;
        let mut d = 0usize;
        let mut s = 0usize;
        let mut u = 0usize;
        for node in &nodes {
            let accessibility = node.retention_strength * 0.5
                + node.retrieval_strength * 0.3
                + node.storage_strength * 0.2;
            if accessibility >= 0.7 {
                a += 1;
            } else if accessibility >= 0.4 {
                d += 1;
            } else if accessibility >= 0.1 {
                s += 1;
            } else {
                u += 1;
            }
        }
        (a, d, s, u)
    } else {
        (0, 0, 0, 0)
    };

    // === FSRS Preview ===
    let scheduler = FSRSScheduler::default();
    let fsrs_preview = if let Some(representative) = nodes.first() {
        let mut state = scheduler.new_card();
        state.difficulty = representative.difficulty;
        state.stability = representative.stability;
        state.reps = representative.reps;
        state.lapses = representative.lapses;
        state.last_review = representative.last_accessed;
        let elapsed = scheduler.days_since_review(&state.last_review);
        let preview = scheduler.preview_reviews(&state, elapsed);
        Some(serde_json::json!({
            "representativeMemoryId": representative.id,
            "elapsedDays": format!("{:.1}", elapsed),
            "intervalIfGood": preview.good.interval,
            "intervalIfEasy": preview.easy.interval,
            "intervalIfHard": preview.hard.interval,
            "currentRetrievability": format!("{:.3}", preview.good.retrievability),
        }))
    } else {
        None
    };

    // === Cognitive health ===
    let cognitive_health = if let Ok(cog) = cognitive.try_lock() {
        let activation_count = cog.activation_network.get_associations("_probe_").len();
        let prediction_accuracy = cog.predictive_memory.prediction_accuracy().unwrap_or(0.0);
        let scheduler_stats = cog.consolidation_scheduler.get_activity_stats();
        Some(serde_json::json!({
            "activationNetworkSize": activation_count,
            "predictionAccuracy": format!("{:.2}", prediction_accuracy),
            "modulesActive": 28,
            "schedulerStats": {
                "totalEvents": scheduler_stats.total_events,
                "eventsPerMinute": scheduler_stats.events_per_minute,
                "isIdle": scheduler_stats.is_idle,
                "timeUntilNextConsolidation": format!("{:?}", cog.consolidation_scheduler.time_until_next()),
            },
        }))
    } else {
        None
    };

    // === Automation triggers (for conditional dream/backup/gc at session start) ===
    let last_consolidation = storage.get_last_consolidation().ok().flatten();
    let last_dream = storage.get_last_dream().ok().flatten();
    let saves_since_last_dream = match &last_dream {
        Some(dt) => storage.count_memories_since(*dt).unwrap_or(0),
        None => stats.total_nodes as i64,
    };
    let last_backup = Storage::get_last_backup_timestamp();


    Ok(serde_json::json!({
        "tool": "system_status",
        // Health
        "status": status,
        "warnings": warnings,
        "recommendations": recommendations,
        "embeddingReady": embedding_ready,
        // Stats
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "withEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": format!("{:.1}%", embedding_coverage),
        "embeddingModel": stats.embedding_model,
        "oldestMemory": stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|dt| dt.to_rfc3339()),
        // Distribution
        "stateDistribution": {
            "active": active,
            "dormant": dormant,
            "silent": silent,
            "unavailable": unavailable,
            "sampled": total,
        },
        // FSRS
        "fsrsPreview": fsrs_preview,
        // Cognitive
        "cognitiveHealth": cognitive_health,
        // Automation triggers — Claude uses these to decide when to dream/backup/gc
        "automationTriggers": {
            "lastDreamTimestamp": last_dream.map(|dt| dt.to_rfc3339()),
            "savesSinceLastDream": saves_since_last_dream,
            "lastBackupTimestamp": last_backup.map(|dt| dt.to_rfc3339()),
            "lastConsolidationTimestamp": last_consolidation.map(|dt| dt.to_rfc3339()),
        },
    }))
}

/// Health check tool — deprecated in v1.7, use execute_system_status() instead
#[allow(dead_code)]
pub async fn execute_health_check(
    storage: &Arc<Storage>,
    _args: Option<Value>,
) -> Result<Value, String> {
    let stats = storage.get_stats().map_err(|e| e.to_string())?;

    let status = if stats.total_nodes == 0 {
        "empty"
    } else if stats.average_retention < 0.3 {
        "critical"
    } else if stats.average_retention < 0.5 {
        "degraded"
    } else {
        "healthy"
    };

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    let embedding_ready = storage.is_embedding_ready();

    let mut warnings = Vec::new();
    if stats.average_retention < 0.5 && stats.total_nodes > 0 {
        warnings.push("Low average retention - consider running consolidation");
    }
    if stats.nodes_due_for_review > 10 {
        warnings.push("Many memories are due for review");
    }
    if stats.total_nodes > 0 && stats.nodes_with_embeddings == 0 {
        warnings.push("No embeddings generated - semantic search unavailable");
    }
    if embedding_coverage < 50.0 && stats.total_nodes > 10 {
        warnings.push("Low embedding coverage - run consolidate to improve semantic search");
    }

    let mut recommendations = Vec::new();
    if status == "critical" {
        recommendations.push("CRITICAL: Many memories have very low retention. Review important memories.");
    }
    if stats.nodes_due_for_review > 5 {
        recommendations.push("Review due memories to strengthen retention.");
    }
    if stats.nodes_with_embeddings < stats.total_nodes {
        recommendations.push("Run 'consolidate' to generate missing embeddings.");
    }
    if stats.total_nodes > 100 && stats.average_retention < 0.7 {
        recommendations.push("Consider running periodic consolidation.");
    }
    if status == "healthy" && recommendations.is_empty() {
        recommendations.push("Memory system is healthy!");
    }

    Ok(serde_json::json!({
        "tool": "health_check",
        "status": status,
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "embeddingCoverage": format!("{:.1}%", embedding_coverage),
        "embeddingReady": embedding_ready,
        "warnings": warnings,
        "recommendations": recommendations,
    }))
}

/// Consolidate tool
pub async fn execute_consolidate(
    storage: &Arc<Storage>,
    _args: Option<Value>,
) -> Result<Value, String> {
    let result = storage.run_consolidation().map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "tool": "consolidate",
        "nodesProcessed": result.nodes_processed,
        "nodesPromoted": result.nodes_promoted,
        "nodesPruned": result.nodes_pruned,
        "decayApplied": result.decay_applied,
        "embeddingsGenerated": result.embeddings_generated,
        "duplicatesMerged": result.duplicates_merged,
        "activationsComputed": result.activations_computed,
        "w20Optimized": result.w20_optimized,
        "durationMs": result.duration_ms,
    }))
}

/// Stats tool — deprecated in v1.7, use execute_system_status() instead
#[allow(dead_code)]
pub async fn execute_stats(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    _args: Option<Value>,
) -> Result<Value, String> {
    let stats = storage.get_stats().map_err(|e| e.to_string())?;

    // Compute state distribution from a sample of nodes
    let nodes = storage.get_all_nodes(500, 0).map_err(|e| e.to_string())?;
    let total = nodes.len();
    let (active, dormant, silent, unavailable) = if total > 0 {
        let mut a = 0usize;
        let mut d = 0usize;
        let mut s = 0usize;
        let mut u = 0usize;
        for node in &nodes {
            let accessibility = node.retention_strength * 0.5
                + node.retrieval_strength * 0.3
                + node.storage_strength * 0.2;
            if accessibility >= 0.7 {
                a += 1;
            } else if accessibility >= 0.4 {
                d += 1;
            } else if accessibility >= 0.1 {
                s += 1;
            } else {
                u += 1;
            }
        }
        (a, d, s, u)
    } else {
        (0, 0, 0, 0)
    };

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    // ====================================================================
    // FSRS Preview: Show optimal intervals for a representative memory
    // ====================================================================
    let scheduler = FSRSScheduler::default();
    let fsrs_preview = if let Some(representative) = nodes.first() {
        let mut state = scheduler.new_card();
        state.difficulty = representative.difficulty;
        state.stability = representative.stability;
        state.reps = representative.reps;
        state.lapses = representative.lapses;
        state.last_review = representative.last_accessed;
        let elapsed = scheduler.days_since_review(&state.last_review);
        let preview = scheduler.preview_reviews(&state, elapsed);
        Some(serde_json::json!({
            "representativeMemoryId": representative.id,
            "elapsedDays": format!("{:.1}", elapsed),
            "intervalIfGood": preview.good.interval,
            "intervalIfEasy": preview.easy.interval,
            "intervalIfHard": preview.hard.interval,
            "currentRetrievability": format!("{:.3}", preview.good.retrievability),
        }))
    } else {
        None
    };

    // ====================================================================
    // STATE SERVICE: Proper state transitions via Bjork model
    // ====================================================================
    let state_distribution_precise = if let Ok(cog) = cognitive.try_lock() {
        let mut lifecycles: Vec<MemoryLifecycle> = nodes
            .iter()
            .take(100) // Sample 100 for performance
            .map(|node| {
                let mut lc = MemoryLifecycle::new();
                lc.last_access = node.last_accessed;
                lc.access_count = node.reps as u32;
                lc.state = if node.retention_strength > 0.7 {
                    MemoryState::Active
                } else if node.retention_strength > 0.3 {
                    MemoryState::Dormant
                } else if node.retention_strength > 0.1 {
                    MemoryState::Silent
                } else {
                    MemoryState::Unavailable
                };
                lc
            })
            .collect();
        let batch_result = cog.state_service.batch_update(&mut lifecycles);
        Some(serde_json::json!({
            "totalTransitions": batch_result.total_transitions,
            "activeToDormant": batch_result.active_to_dormant,
            "dormantToSilent": batch_result.dormant_to_silent,
            "suppressionsResolved": batch_result.suppressions_resolved,
            "sampled": lifecycles.len(),
        }))
    } else {
        None
    };

    // ====================================================================
    // COMPRESSOR: Find compressible memory groups
    // ====================================================================
    let compressible_groups = if let Ok(cog) = cognitive.try_lock() {
        let memories_for_compression: Vec<MemoryForCompression> = nodes
            .iter()
            .filter(|n| n.retention_strength < 0.5) // Only consider low-retention memories
            .take(50) // Cap for performance
            .map(|n| MemoryForCompression {
                id: n.id.clone(),
                content: n.content.clone(),
                tags: n.tags.clone(),
                created_at: n.created_at,
                last_accessed: Some(n.last_accessed),
                embedding: None,
            })
            .collect();
        if !memories_for_compression.is_empty() {
            let groups = cog.compressor.find_compressible_groups(&memories_for_compression);
            Some(serde_json::json!({
                "groupCount": groups.len(),
                "totalCompressible": groups.iter().map(|g| g.len()).sum::<usize>(),
            }))
        } else {
            None
        }
    } else {
        None
    };

    // ====================================================================
    // COGNITIVE: Module health summary
    // ====================================================================
    let cognitive_health = if let Ok(cog) = cognitive.try_lock() {
        let activation_count = cog.activation_network.get_associations("_probe_").len();
        let prediction_accuracy = cog.predictive_memory.prediction_accuracy().unwrap_or(0.0);
        let scheduler_stats = cog.consolidation_scheduler.get_activity_stats();
        Some(serde_json::json!({
            "activationNetworkSize": activation_count,
            "predictionAccuracy": format!("{:.2}", prediction_accuracy),
            "modulesActive": 28,
            "schedulerStats": {
                "totalEvents": scheduler_stats.total_events,
                "eventsPerMinute": scheduler_stats.events_per_minute,
                "isIdle": scheduler_stats.is_idle,
                "timeUntilNextConsolidation": format!("{:?}", cog.consolidation_scheduler.time_until_next()),
            },
        }))
    } else {
        None
    };

    Ok(serde_json::json!({
        "tool": "stats",
        "totalMemories": stats.total_nodes,
        "dueForReview": stats.nodes_due_for_review,
        "averageRetention": stats.average_retention,
        "averageStorageStrength": stats.average_storage_strength,
        "averageRetrievalStrength": stats.average_retrieval_strength,
        "withEmbeddings": stats.nodes_with_embeddings,
        "embeddingCoverage": format!("{:.1}%", embedding_coverage),
        "embeddingModel": stats.embedding_model,
        "oldestMemory": stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        "newestMemory": stats.newest_memory.map(|dt| dt.to_rfc3339()),
        "stateDistribution": {
            "active": active,
            "dormant": dormant,
            "silent": silent,
            "unavailable": unavailable,
            "sampled": total,
        },
        "fsrsPreview": fsrs_preview,
        "cognitiveHealth": cognitive_health,
        "stateTransitions": state_distribution_precise,
        "compressibleMemories": compressible_groups,
    }))
}

/// Backup tool
pub async fn execute_backup(
    storage: &Arc<Storage>,
    _args: Option<Value>,
) -> Result<Value, String> {
    // Determine backup path
    let vestige_dir = directories::ProjectDirs::from("com", "vestige", "core")
        .ok_or("Could not determine data directory")?;
    let backup_dir = vestige_dir.data_dir().parent()
        .unwrap_or(vestige_dir.data_dir())
        .join("backups");

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path = backup_dir.join(format!("vestige-{}.db", timestamp));

    // Use VACUUM INTO for a consistent backup (handles WAL properly)
    {
        storage.backup_to(&backup_path)
            .map_err(|e| format!("Failed to create backup: {}", e))?;
    }

    let file_size = std::fs::metadata(&backup_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(serde_json::json!({
        "tool": "backup",
        "path": backup_path.display().to_string(),
        "sizeBytes": file_size,
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    format: Option<String>,
    tags: Option<Vec<String>>,
    since: Option<String>,
    path: Option<String>,
}

/// Export tool
pub async fn execute_export(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: ExportArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => ExportArgs {
            format: None,
            tags: None,
            since: None,
            path: None,
        },
    };

    let format = args.format.unwrap_or_else(|| "json".to_string());
    if format != "json" && format != "jsonl" {
        return Err(format!("Invalid format '{}'. Must be 'json' or 'jsonl'.", format));
    }

    // Parse since date
    let since_date = match &args.since {
        Some(date_str) => {
            let naive = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{}': {}. Use YYYY-MM-DD.", date_str, e))?;
            Some(naive.and_hms_opt(0, 0, 0).unwrap().and_utc())
        }
        None => None,
    };

    let tag_filter: Vec<String> = args.tags.unwrap_or_default();

    // Fetch all nodes (capped at 100K to prevent OOM)
    let mut all_nodes = Vec::new();
    let page_size = 500;
    let max_nodes = 100_000;
    let mut offset = 0;
    loop {
        let batch = storage.get_all_nodes(page_size, offset).map_err(|e| e.to_string())?;
        let batch_len = batch.len();
        all_nodes.extend(batch);
        if batch_len < page_size as usize || all_nodes.len() >= max_nodes {
            break;
        }
        offset += page_size;
    }

    // Apply filters
    let filtered: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            if since_date.as_ref().is_some_and(|since_dt| node.created_at < *since_dt) {
                return false;
            }
            if !tag_filter.is_empty() {
                for tag in &tag_filter {
                    if !node.tags.iter().any(|t| t == tag) {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // Determine export path — always constrained to vestige exports directory
    let vestige_dir = directories::ProjectDirs::from("com", "vestige", "core")
        .ok_or("Could not determine data directory")?;
    let export_dir = vestige_dir.data_dir().parent()
        .unwrap_or(vestige_dir.data_dir())
        .join("exports");
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create export directory: {}", e))?;

    let export_path = match args.path {
        Some(ref p) => {
            // Only allow a filename, not a path — prevent path traversal
            let filename = std::path::Path::new(p)
                .file_name()
                .ok_or("Invalid export filename: must be a simple filename, not a path")?;
            let name_str = filename.to_str().ok_or("Invalid filename encoding")?;
            if name_str.contains("..") {
                return Err("Invalid export filename: '..' not allowed".to_string());
            }
            export_dir.join(filename)
        }
        None => {
            let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
            export_dir.join(format!("memories-{}.{}", timestamp, format))
        }
    };

    // Write export
    let file = std::fs::File::create(&export_path)
        .map_err(|e| format!("Failed to create export file: {}", e))?;
    let mut writer = std::io::BufWriter::new(file);

    use std::io::Write;
    match format.as_str() {
        "json" => {
            serde_json::to_writer_pretty(&mut writer, &filtered)
                .map_err(|e| format!("Failed to write JSON: {}", e))?;
            writer.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        "jsonl" => {
            for node in &filtered {
                serde_json::to_writer(&mut writer, node)
                    .map_err(|e| format!("Failed to write JSONL: {}", e))?;
                writer.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }
        _ => unreachable!(),
    }
    writer.flush().map_err(|e| e.to_string())?;

    let file_size = std::fs::metadata(&export_path).map(|m| m.len()).unwrap_or(0);

    Ok(serde_json::json!({
        "tool": "export",
        "path": export_path.display().to_string(),
        "format": format,
        "memoriesExported": filtered.len(),
        "totalMemories": all_nodes.len(),
        "sizeBytes": file_size,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GcArgs {
    min_retention: Option<f64>,
    max_age_days: Option<u64>,
    dry_run: Option<bool>,
}

/// Garbage collection tool
pub async fn execute_gc(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: GcArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => GcArgs {
            min_retention: None,
            max_age_days: None,
            dry_run: None,
        },
    };

    let min_retention = args.min_retention.unwrap_or(0.1).clamp(0.0, 1.0);
    let max_age_days = args.max_age_days;
    let dry_run = args.dry_run.unwrap_or(true); // Default to dry_run for safety

    let now = Utc::now();

    // Fetch all nodes (capped at 100K to prevent OOM)
    let mut all_nodes = Vec::new();
    let page_size = 500;
    let max_nodes = 100_000;
    let mut offset = 0;
    loop {
        let batch = storage.get_all_nodes(page_size, offset).map_err(|e| e.to_string())?;
        let batch_len = batch.len();
        all_nodes.extend(batch);
        if batch_len < page_size as usize || all_nodes.len() >= max_nodes {
            break;
        }
        offset += page_size;
    }

    // Find candidates
    let candidates: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            if node.retention_strength >= min_retention {
                return false;
            }
            if let Some(max_days) = max_age_days {
                let age_days = (now - node.created_at).num_days();
                if age_days < 0 || (age_days as u64) < max_days {
                    return false;
                }
            }
            true
        })
        .collect();

    let candidate_count = candidates.len();

    // Build sample for display
    let sample: Vec<Value> = candidates
        .iter()
        .take(10)
        .map(|node| {
            let age_days = (now - node.created_at).num_days();
            let content_preview: String = {
                let preview: String = node.content.chars().take(60).collect();
                if preview.len() < node.content.len() {
                    format!("{}...", preview)
                } else {
                    preview
                }
            };
            serde_json::json!({
                "id": &node.id[..8.min(node.id.len())],
                "retention": node.retention_strength,
                "ageDays": age_days,
                "contentPreview": content_preview,
            })
        })
        .collect();

    if dry_run {
        return Ok(serde_json::json!({
            "tool": "gc",
            "dryRun": true,
            "minRetention": min_retention,
            "maxAgeDays": max_age_days,
            "candidateCount": candidate_count,
            "totalMemories": all_nodes.len(),
            "sample": sample,
            "message": format!("{} memories would be deleted. Set dry_run=false to delete.", candidate_count),
        }));
    }

    // Perform actual deletion
    let mut deleted = 0usize;
    let mut errors = 0usize;
    let ids: Vec<String> = candidates.iter().map(|n| n.id.clone()).collect();

    for id in &ids {
        match storage.delete_node(id) {
            Ok(true) => deleted += 1,
            Ok(false) => errors += 1,
            Err(_) => errors += 1,
        }
    }

    Ok(serde_json::json!({
        "tool": "gc",
        "dryRun": false,
        "minRetention": min_retention,
        "maxAgeDays": max_age_days,
        "deleted": deleted,
        "errors": errors,
        "totalBefore": all_nodes.len(),
        "totalAfter": all_nodes.len() - deleted,
    }))
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

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[test]
    fn test_system_status_schema() {
        let schema = system_status_schema();
        assert_eq!(schema["type"], "object");
    }

    #[tokio::test]
    async fn test_system_status_empty_db() {
        let (storage, _dir) = test_storage().await;
        let result = execute_system_status(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["tool"], "system_status");
        assert_eq!(value["status"], "empty");
        assert_eq!(value["totalMemories"], 0);
        assert!(value["warnings"].is_array());
        assert!(value["recommendations"].is_array());
    }

    #[tokio::test]
    async fn test_system_status_with_memories() {
        let (storage, _dir) = test_storage().await;
        {
            storage.ingest(vestige_core::IngestInput {
                content: "Test memory for status".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec![],
                valid_from: None,
                valid_until: None,
            }).unwrap();
        }
        let result = execute_system_status(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 1);
        assert!(value["stateDistribution"].is_object());
        assert!(value["embeddingCoverage"].is_string());
    }

    #[tokio::test]
    async fn test_system_status_has_cognitive_health() {
        let (storage, _dir) = test_storage().await;
        let result = execute_system_status(&storage, &test_cognitive(), None).await;
        let value = result.unwrap();
        assert!(value["cognitiveHealth"].is_object());
        assert_eq!(value["cognitiveHealth"]["modulesActive"], 28);
    }

    #[tokio::test]
    async fn test_system_status_has_automation_triggers() {
        let (storage, _dir) = test_storage().await;
        let result = execute_system_status(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());
        let value = result.unwrap();

        let triggers = &value["automationTriggers"];
        assert!(triggers.is_object(), "automationTriggers should be present");
        assert!(triggers["lastDreamTimestamp"].is_null(), "No dreams yet");
        assert_eq!(triggers["savesSinceLastDream"], 0, "Empty DB = 0 saves");
        assert!(triggers["lastConsolidationTimestamp"].is_null(), "No consolidation yet");
        // lastBackupTimestamp depends on filesystem state, just check it exists
        assert!(triggers.get("lastBackupTimestamp").is_some());
    }

    #[tokio::test]
    async fn test_system_status_automation_triggers_with_memories() {
        let (storage, _dir) = test_storage().await;
        {
            for i in 0..3 {
                storage.ingest(vestige_core::IngestInput {
                    content: format!("Automation trigger test memory {}", i),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec![],
                    valid_from: None,
                    valid_until: None,
                }).unwrap();
            }
        }
        let result = execute_system_status(&storage, &test_cognitive(), None).await;
        let value = result.unwrap();

        let triggers = &value["automationTriggers"];
        // No dream ever → savesSinceLastDream == totalMemories
        assert_eq!(triggers["savesSinceLastDream"], 3);
        assert!(triggers["lastDreamTimestamp"].is_null());
    }
}
