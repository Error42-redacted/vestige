//! Memory Timeline Tool
//!
//! Browse memories chronologically. Returns memories in a time range,
//! grouped by day. Defaults to last 7 days.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;


use vestige_core::Storage;

use super::search_unified::format_node;

/// Input schema for memory_timeline tool
pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "start": {
                "type": "string",
                "description": "Start of time range (ISO 8601 date or datetime). Default: 7 days ago."
            },
            "end": {
                "type": "string",
                "description": "End of time range (ISO 8601 date or datetime). Default: now."
            },
            "node_type": {
                "type": "string",
                "description": "Filter by node type (e.g. 'fact', 'concept', 'decision')"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filter by tags (ANY match)"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of memories to return (default: 50, max: 200)",
                "default": 50,
                "minimum": 1,
                "maximum": 200
            },
            "detail_level": {
                "type": "string",
                "description": "Level of detail: 'brief', 'summary' (default), or 'full'",
                "enum": ["brief", "summary", "full"],
                "default": "summary"
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineArgs {
    start: Option<String>,
    end: Option<String>,
    #[serde(alias = "node_type")]
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    limit: Option<i32>,
    #[serde(alias = "detail_level")]
    detail_level: Option<String>,
}

/// Parse an ISO 8601 date or datetime string into a DateTime<Utc>.
/// Supports both `2026-02-01` and `2026-02-01T00:00:00Z` formats.
fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    // Try full datetime first
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try date-only (YYYY-MM-DD)
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| format!("Invalid date: {}", s))?
            .and_utc();
        return Ok(dt);
    }
    Err(format!(
        "Invalid date/datetime '{}'. Use ISO 8601 format: YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ",
        s
    ))
}

/// Execute memory_timeline tool
pub async fn execute(
    storage: &Arc<Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args: TimelineArgs = match args {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?,
        None => TimelineArgs {
            start: None,
            end: None,
            node_type: None,
            tags: None,
            limit: None,
            detail_level: None,
        },
    };

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

    // Parse time range
    let now = Utc::now();
    let start = match &args.start {
        Some(s) => Some(parse_datetime(s)?),
        None => Some(now - chrono::Duration::days(7)),
    };
    let end = match &args.end {
        Some(e) => Some(parse_datetime(e)?),
        None => Some(now),
    };

    let limit = args.limit.unwrap_or(50).clamp(1, 200);


    // Query memories in time range
    let mut results = storage
        .query_time_range(start, end, limit)
        .map_err(|e| e.to_string())?;

    // Post-query filters
    if let Some(ref node_type) = args.node_type {
        results.retain(|n| n.node_type == *node_type);
    }
    if let Some(tags) = args.tags.as_ref().filter(|t| !t.is_empty()) {
        results.retain(|n| tags.iter().any(|t| n.tags.contains(t)));
    }

    // Group by day
    let mut by_day: BTreeMap<NaiveDate, Vec<Value>> = BTreeMap::new();
    for node in &results {
        let date = node.created_at.date_naive();
        by_day
            .entry(date)
            .or_default()
            .push(format_node(node, detail_level));
    }

    // Build timeline (newest first)
    let timeline: Vec<Value> = by_day
        .into_iter()
        .rev()
        .map(|(date, memories)| {
            serde_json::json!({
                "date": date.to_string(),
                "count": memories.len(),
                "memories": memories,
            })
        })
        .collect();

    let total = results.len();
    let days = timeline.len();

    Ok(serde_json::json!({
        "tool": "memory_timeline",
        "range": {
            "start": start.map(|dt| dt.to_rfc3339()),
            "end": end.map(|dt| dt.to_rfc3339()),
        },
        "detailLevel": detail_level,
        "totalMemories": total,
        "days": days,
        "timeline": timeline,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn ingest_test_memory(storage: &Arc<Storage>, content: &str) {
        storage.ingest(vestige_core::IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec!["timeline-test".to_string()],
            valid_from: None,
            valid_until: None,
        })
        .unwrap();
    }

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["start"].is_object());
        assert!(s["properties"]["end"].is_object());
        assert!(s["properties"]["node_type"].is_object());
        assert!(s["properties"]["tags"].is_object());
        assert!(s["properties"]["limit"].is_object());
        assert!(s["properties"]["detail_level"].is_object());
    }

    #[test]
    fn test_parse_datetime_rfc3339() {
        let result = parse_datetime("2026-02-18T10:30:00Z");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_datetime_date_only() {
        let result = parse_datetime("2026-02-18");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_datetime_invalid() {
        let result = parse_datetime("not-a-date");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date/datetime"));
    }

    #[test]
    fn test_parse_datetime_empty() {
        let result = parse_datetime("");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_timeline_no_args_defaults() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["tool"], "memory_timeline");
        assert_eq!(value["detailLevel"], "summary");
        assert!(value["range"]["start"].is_string());
        assert!(value["range"]["end"].is_string());
    }

    #[tokio::test]
    async fn test_timeline_empty_database() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 0);
        assert_eq!(value["days"], 0);
        assert!(value["timeline"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_timeline_with_memories() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Timeline test memory 1").await;
        ingest_test_memory(&storage, "Timeline test memory 2").await;
        let result = execute(&storage, None).await;
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 2);
        assert!(value["days"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_timeline_invalid_detail_level() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "detail_level": "invalid" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid detail_level"));
    }

    #[tokio::test]
    async fn test_timeline_detail_level_brief() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Brief test memory").await;
        let args = serde_json::json!({ "detail_level": "brief" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "brief");
    }

    #[tokio::test]
    async fn test_timeline_detail_level_full() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Full test memory").await;
        let args = serde_json::json!({ "detail_level": "full" });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["detailLevel"], "full");
    }

    #[tokio::test]
    async fn test_timeline_limit_clamped() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "limit": 0 });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok()); // limit clamped to 1, no error
    }

    #[tokio::test]
    async fn test_timeline_with_date_range() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Ranged memory").await;
        let args = serde_json::json!({
            "start": "2020-01-01",
            "end": "2030-12-31"
        });
        let result = execute(&storage, Some(args)).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value["totalMemories"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_timeline_node_type_filter() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "A fact memory").await;
        let args = serde_json::json!({ "node_type": "concept" });
        let result = execute(&storage, Some(args)).await;
        let value = result.unwrap();
        // Ingested as "fact", filtering for "concept" should yield 0
        assert_eq!(value["totalMemories"], 0);
    }

    #[tokio::test]
    async fn test_timeline_tag_filter() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Tagged memory").await;
        let args = serde_json::json!({ "tags": ["timeline-test"] });
        let result = execute(&storage, Some(args)).await;
        let value = result.unwrap();
        assert!(value["totalMemories"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_timeline_tag_filter_no_match() {
        let (storage, _dir) = test_storage().await;
        ingest_test_memory(&storage, "Tagged memory").await;
        let args = serde_json::json!({ "tags": ["nonexistent-tag"] });
        let result = execute(&storage, Some(args)).await;
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 0);
    }
}
