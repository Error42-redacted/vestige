//! Storage Module
//!
//! SQLite-based storage layer with:
//! - FTS5 full-text search with query sanitization
//! - Embedded vector storage
//! - FSRS-6 state management
//! - Temporal memory support

mod migrations;
mod sqlite;

pub use migrations::MIGRATIONS;
pub use sqlite::{
    ConnectionRecord, ConsolidationHistoryRecord, DreamHistoryRecord, InsightRecord,
    IntentionRecord, Result, SmartIngestResult, StateTransitionRecord, Storage, StorageError,
};
