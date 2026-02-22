//! Database Migrations
//!
//! Schema migration definitions for the storage layer.

/// Migration definitions
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "Initial schema with FSRS-6 and embeddings",
        up: MIGRATION_V1_UP,
    },
    Migration {
        version: 2,
        description: "Add temporal columns",
        up: MIGRATION_V2_UP,
    },
    Migration {
        version: 3,
        description: "Add persistence tables for neuroscience features",
        up: MIGRATION_V3_UP,
    },
    Migration {
        version: 4,
        description: "GOD TIER 2026: Temporal knowledge graph, memory scopes, embedding versioning",
        up: MIGRATION_V4_UP,
    },
    Migration {
        version: 5,
        description: "FSRS-6 upgrade: access history, ACT-R activation, personalized decay",
        up: MIGRATION_V5_UP,
    },
    Migration {
        version: 6,
        description: "Dream history persistence for automation triggers",
        up: MIGRATION_V6_UP,
    },
    Migration {
        version: 7,
        description: "Performance: page_size 8192, FTS5 porter tokenizer",
        up: MIGRATION_V7_UP,
    },
    Migration {
        version: 8,
        description: "v1.9.0 Autonomic: waking SWR tags, utility scoring, retention tracking",
        up: MIGRATION_V8_UP,
    },
    Migration {
        version: 9,
        description: "v2.0.0 Cognitive Leap: emotional memory, flashbulb encoding, temporal hierarchy",
        up: MIGRATION_V9_UP,
    },
];

/// A database migration
#[derive(Debug, Clone)]
pub struct Migration {
    /// Version number
    pub version: u32,
    /// Description
    pub description: &'static str,
    /// SQL to apply
    pub up: &'static str,
}

/// V1: Initial schema
const MIGRATION_V1_UP: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_nodes (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    node_type TEXT NOT NULL DEFAULT 'fact',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_accessed TEXT NOT NULL,

    -- FSRS-6 state (21 parameters)
    stability REAL DEFAULT 1.0,
    difficulty REAL DEFAULT 5.0,
    reps INTEGER DEFAULT 0,
    lapses INTEGER DEFAULT 0,
    learning_state TEXT DEFAULT 'new',

    -- Dual-strength model (Bjork & Bjork 1992)
    storage_strength REAL DEFAULT 1.0,
    retrieval_strength REAL DEFAULT 1.0,
    retention_strength REAL DEFAULT 1.0,

    -- Sentiment for emotional memory weighting
    sentiment_score REAL DEFAULT 0.0,
    sentiment_magnitude REAL DEFAULT 0.0,

    -- Scheduling
    next_review TEXT,
    scheduled_days INTEGER DEFAULT 0,

    -- Provenance
    source TEXT,
    tags TEXT DEFAULT '[]',

    -- Embedding metadata
    has_embedding INTEGER DEFAULT 0,
    embedding_model TEXT
);

CREATE INDEX IF NOT EXISTS idx_nodes_retention ON knowledge_nodes(retention_strength);
CREATE INDEX IF NOT EXISTS idx_nodes_next_review ON knowledge_nodes(next_review);
CREATE INDEX IF NOT EXISTS idx_nodes_created ON knowledge_nodes(created_at);
CREATE INDEX IF NOT EXISTS idx_nodes_has_embedding ON knowledge_nodes(has_embedding);

-- Embeddings storage table (binary blob for efficiency)
CREATE TABLE IF NOT EXISTS node_embeddings (
    node_id TEXT PRIMARY KEY REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    dimensions INTEGER NOT NULL DEFAULT 768,
    model TEXT NOT NULL DEFAULT 'BAAI/bge-base-en-v1.5',
    created_at TEXT NOT NULL
);

-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
    id,
    content,
    tags,
    content='knowledge_nodes',
    content_rowid='rowid'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER IF NOT EXISTS knowledge_ai AFTER INSERT ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(rowid, id, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.content, NEW.tags);
END;

CREATE TRIGGER IF NOT EXISTS knowledge_ad AFTER DELETE ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, id, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.content, OLD.tags);
END;

CREATE TRIGGER IF NOT EXISTS knowledge_au AFTER UPDATE ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, id, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.content, OLD.tags);
    INSERT INTO knowledge_fts(rowid, id, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.content, NEW.tags);
END;

-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_version (version, applied_at) VALUES (1, datetime('now'));
"#;

/// V2: Add temporal columns
const MIGRATION_V2_UP: &str = r#"
ALTER TABLE knowledge_nodes ADD COLUMN valid_from TEXT;
ALTER TABLE knowledge_nodes ADD COLUMN valid_until TEXT;

CREATE INDEX IF NOT EXISTS idx_nodes_valid_from ON knowledge_nodes(valid_from);
CREATE INDEX IF NOT EXISTS idx_nodes_valid_until ON knowledge_nodes(valid_until);

UPDATE schema_version SET version = 2, applied_at = datetime('now');
"#;

/// V3: Add persistence tables for neuroscience features
/// Fixes critical gap: intentions, insights, and activation network were IN-MEMORY ONLY
const MIGRATION_V3_UP: &str = r#"
-- 1. INTENTIONS TABLE (Prospective Memory)
-- Stores future intentions/reminders with trigger conditions
CREATE TABLE IF NOT EXISTS intentions (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    trigger_type TEXT NOT NULL,  -- 'time', 'duration', 'event', 'context', 'activity', 'recurring', 'compound'
    trigger_data TEXT NOT NULL,  -- JSON: serialized IntentionTrigger
    priority INTEGER NOT NULL DEFAULT 2,  -- 1=Low, 2=Normal, 3=High, 4=Critical
    status TEXT NOT NULL DEFAULT 'active',  -- 'active', 'triggered', 'fulfilled', 'cancelled', 'expired', 'snoozed'
    created_at TEXT NOT NULL,
    deadline TEXT,
    fulfilled_at TEXT,
    reminder_count INTEGER DEFAULT 0,
    last_reminded_at TEXT,
    notes TEXT,
    tags TEXT DEFAULT '[]',
    related_memories TEXT DEFAULT '[]',
    snoozed_until TEXT,
    source_type TEXT NOT NULL DEFAULT 'api',
    source_data TEXT
);

CREATE INDEX IF NOT EXISTS idx_intentions_status ON intentions(status);
CREATE INDEX IF NOT EXISTS idx_intentions_priority ON intentions(priority);
CREATE INDEX IF NOT EXISTS idx_intentions_deadline ON intentions(deadline);
CREATE INDEX IF NOT EXISTS idx_intentions_snoozed ON intentions(snoozed_until);

-- 2. INSIGHTS TABLE (From Consolidation/Dreams)
-- Stores AI-generated insights discovered during memory consolidation
CREATE TABLE IF NOT EXISTS insights (
    id TEXT PRIMARY KEY,
    insight TEXT NOT NULL,
    source_memories TEXT NOT NULL,  -- JSON array of memory IDs
    confidence REAL NOT NULL,
    novelty_score REAL NOT NULL,
    insight_type TEXT NOT NULL,  -- 'hidden_connection', 'recurring_pattern', 'generalization', 'contradiction', 'knowledge_gap', 'temporal_trend', 'synthesis'
    generated_at TEXT NOT NULL,
    tags TEXT DEFAULT '[]',
    feedback TEXT,  -- 'accepted', 'rejected', or NULL
    applied_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_insights_type ON insights(insight_type);
CREATE INDEX IF NOT EXISTS idx_insights_confidence ON insights(confidence);
CREATE INDEX IF NOT EXISTS idx_insights_generated ON insights(generated_at);
CREATE INDEX IF NOT EXISTS idx_insights_feedback ON insights(feedback);

-- 3. MEMORY_CONNECTIONS TABLE (Activation Network Edges)
-- Stores associations between memories for spreading activation
CREATE TABLE IF NOT EXISTS memory_connections (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    strength REAL NOT NULL,
    link_type TEXT NOT NULL,  -- 'semantic', 'temporal', 'spatial', 'causal', 'part_of', 'user_defined', 'cross_reference', 'sequential', 'shared_concepts', 'pattern'
    created_at TEXT NOT NULL,
    last_activated TEXT NOT NULL,
    activation_count INTEGER DEFAULT 0,
    PRIMARY KEY (source_id, target_id),
    FOREIGN KEY (source_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_connections_source ON memory_connections(source_id);
CREATE INDEX IF NOT EXISTS idx_connections_target ON memory_connections(target_id);
CREATE INDEX IF NOT EXISTS idx_connections_strength ON memory_connections(strength);
CREATE INDEX IF NOT EXISTS idx_connections_type ON memory_connections(link_type);

-- 4. MEMORY_STATES TABLE (Accessibility States)
-- Tracks lifecycle state of each memory (Active/Dormant/Silent/Unavailable)
CREATE TABLE IF NOT EXISTS memory_states (
    memory_id TEXT PRIMARY KEY,
    state TEXT NOT NULL DEFAULT 'active',  -- 'active', 'dormant', 'silent', 'unavailable'
    last_access TEXT NOT NULL,
    access_count INTEGER DEFAULT 1,
    state_entered_at TEXT NOT NULL,
    suppression_until TEXT,
    suppressed_by TEXT DEFAULT '[]',
    time_active_seconds INTEGER DEFAULT 0,
    time_dormant_seconds INTEGER DEFAULT 0,
    time_silent_seconds INTEGER DEFAULT 0,
    time_unavailable_seconds INTEGER DEFAULT 0,
    FOREIGN KEY (memory_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_states_state ON memory_states(state);
CREATE INDEX IF NOT EXISTS idx_states_access ON memory_states(last_access);
CREATE INDEX IF NOT EXISTS idx_states_suppression ON memory_states(suppression_until);

-- 5. FSRS_CARDS TABLE (Extended Review State)
-- Stores complete FSRS-6 card state for spaced repetition
CREATE TABLE IF NOT EXISTS fsrs_cards (
    memory_id TEXT PRIMARY KEY,
    difficulty REAL NOT NULL DEFAULT 5.0,
    stability REAL NOT NULL DEFAULT 1.0,
    state TEXT NOT NULL DEFAULT 'new',  -- 'new', 'learning', 'review', 'relearning'
    reps INTEGER DEFAULT 0,
    lapses INTEGER DEFAULT 0,
    last_review TEXT,
    due_date TEXT,
    elapsed_days INTEGER DEFAULT 0,
    scheduled_days INTEGER DEFAULT 0,
    FOREIGN KEY (memory_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_fsrs_due ON fsrs_cards(due_date);
CREATE INDEX IF NOT EXISTS idx_fsrs_state ON fsrs_cards(state);

-- 6. CONSOLIDATION_HISTORY TABLE (Dream Cycle Records)
-- Tracks when consolidation ran and what it accomplished
CREATE TABLE IF NOT EXISTS consolidation_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    completed_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    memories_replayed INTEGER DEFAULT 0,
    connections_found INTEGER DEFAULT 0,
    connections_strengthened INTEGER DEFAULT 0,
    connections_pruned INTEGER DEFAULT 0,
    insights_generated INTEGER DEFAULT 0,
    memories_transferred TEXT DEFAULT '[]',
    patterns_discovered TEXT DEFAULT '[]'
);

CREATE INDEX IF NOT EXISTS idx_consolidation_completed ON consolidation_history(completed_at);

-- 7. STATE_TRANSITIONS TABLE (Audit Trail)
-- Historical record of state changes for debugging and analytics
CREATE TABLE IF NOT EXISTS state_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id TEXT NOT NULL,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    reason_type TEXT NOT NULL,  -- 'access', 'time_decay', 'cue_reactivation', 'competition_loss', 'interference_resolved', 'user_suppression', 'suppression_expired', 'manual_override', 'system_init'
    reason_data TEXT,
    timestamp TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_transitions_memory ON state_transitions(memory_id);
CREATE INDEX IF NOT EXISTS idx_transitions_timestamp ON state_transitions(timestamp);

UPDATE schema_version SET version = 3, applied_at = datetime('now');
"#;

/// V4: GOD TIER 2026 - Temporal Knowledge Graph, Memory Scopes, Embedding Versioning
/// Competes with Zep's Graphiti and Mem0's memory scopes
const MIGRATION_V4_UP: &str = r#"
-- ============================================================================
-- TEMPORAL KNOWLEDGE GRAPH (Like Zep's Graphiti)
-- ============================================================================

-- Knowledge edges for temporal reasoning
CREATE TABLE IF NOT EXISTS knowledge_edges (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,  -- 'semantic', 'temporal', 'causal', 'derived', 'contradiction', 'refinement'
    weight REAL NOT NULL DEFAULT 1.0,
    -- Temporal validity (bi-temporal model)
    valid_from TEXT,  -- When this relationship started being true
    valid_until TEXT, -- When this relationship stopped being true (NULL = still valid)
    -- Provenance
    created_at TEXT NOT NULL,
    created_by TEXT,  -- 'user', 'system', 'consolidation', 'llm'
    confidence REAL NOT NULL DEFAULT 1.0,  -- Confidence in this edge
    -- Metadata
    metadata TEXT,  -- JSON for edge-specific data
    FOREIGN KEY (source_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_edges_source ON knowledge_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_edges_target ON knowledge_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_edges_type ON knowledge_edges(edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_valid_from ON knowledge_edges(valid_from);
CREATE INDEX IF NOT EXISTS idx_edges_valid_until ON knowledge_edges(valid_until);

-- ============================================================================
-- MEMORY SCOPES (Like Mem0's User/Session/Agent)
-- ============================================================================

-- Add scope column to knowledge_nodes
ALTER TABLE knowledge_nodes ADD COLUMN scope TEXT DEFAULT 'user';
-- Values: 'session' (per-session, cleared on restart)
--         'user' (per-user, persists across sessions)
--         'agent' (global agent knowledge, shared)

CREATE INDEX IF NOT EXISTS idx_nodes_scope ON knowledge_nodes(scope);

-- Session tracking table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL DEFAULT 'default',
    started_at TEXT NOT NULL,
    ended_at TEXT,
    context TEXT,  -- JSON: session metadata
    memory_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

-- ============================================================================
-- EMBEDDING VERSIONING (Track model upgrades)
-- ============================================================================

-- Add embedding version to node_embeddings
ALTER TABLE node_embeddings ADD COLUMN version INTEGER DEFAULT 1;
-- Version 1 = all-MiniLM-L6-v2 (384d, pre-2026)
-- Version 2 = BGE-base-en-v1.5 (768d, GOD TIER 2026)

CREATE INDEX IF NOT EXISTS idx_embeddings_version ON node_embeddings(version);

-- Update existing embeddings to mark as version 1 (old model)
UPDATE node_embeddings SET version = 1 WHERE version IS NULL;

-- ============================================================================
-- MEMORY COMPRESSION (For old memories - Tier 3 prep)
-- ============================================================================

CREATE TABLE IF NOT EXISTS compressed_memories (
    id TEXT PRIMARY KEY,
    original_id TEXT NOT NULL,
    compressed_content TEXT NOT NULL,
    original_length INTEGER NOT NULL,
    compressed_length INTEGER NOT NULL,
    compression_ratio REAL NOT NULL,
    semantic_fidelity REAL NOT NULL,  -- How much meaning was preserved (0-1)
    compressed_at TEXT NOT NULL,
    model_used TEXT NOT NULL DEFAULT 'llm',
    FOREIGN KEY (original_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_compressed_original ON compressed_memories(original_id);
CREATE INDEX IF NOT EXISTS idx_compressed_at ON compressed_memories(compressed_at);

-- ============================================================================
-- EPISODIC vs SEMANTIC MEMORY (Research-backed distinction)
-- ============================================================================

-- Add memory system classification
ALTER TABLE knowledge_nodes ADD COLUMN memory_system TEXT DEFAULT 'semantic';
-- Values: 'episodic' (what happened - events, conversations)
--         'semantic' (what I know - facts, concepts)
--         'procedural' (how-to - never decays)

CREATE INDEX IF NOT EXISTS idx_nodes_memory_system ON knowledge_nodes(memory_system);

UPDATE schema_version SET version = 4, applied_at = datetime('now');
"#;

/// V5: FSRS-6 Upgrade - Access history for ACT-R activation, personalized decay parameters
const MIGRATION_V5_UP: &str = r#"
-- ============================================================================
-- ACCESS HISTORY (For ACT-R Activation + Parameter Training)
-- ============================================================================

-- Logs every search hit, promote, demote for ACT-R activation computation
CREATE TABLE IF NOT EXISTS memory_access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT NOT NULL,
    access_type TEXT NOT NULL,  -- 'search_hit', 'promote', 'demote'
    accessed_at TEXT NOT NULL,
    FOREIGN KEY (node_id) REFERENCES knowledge_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_log_node ON memory_access_log(node_id);
CREATE INDEX IF NOT EXISTS idx_access_log_time ON memory_access_log(accessed_at);

-- ============================================================================
-- ACT-R ACTIVATION (Pre-computed during consolidation)
-- ============================================================================

-- B_i = ln(sum(t_j^(-d))) — NULL until first consolidation computes it
ALTER TABLE knowledge_nodes ADD COLUMN activation REAL;

CREATE INDEX IF NOT EXISTS idx_nodes_activation ON knowledge_nodes(activation);

-- ============================================================================
-- PERSONALIZED FSRS-6 PARAMETERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS fsrs_config (
    key TEXT PRIMARY KEY,
    value REAL NOT NULL,
    updated_at TEXT NOT NULL
);

-- Default w20 (forgetting curve decay parameter)
INSERT OR IGNORE INTO fsrs_config (key, value, updated_at)
VALUES ('w20', 0.1542, datetime('now'));

-- ============================================================================
-- EXTENDED CONSOLIDATION TRACKING
-- ============================================================================

ALTER TABLE consolidation_history ADD COLUMN duplicates_merged INTEGER DEFAULT 0;
ALTER TABLE consolidation_history ADD COLUMN activations_computed INTEGER DEFAULT 0;
ALTER TABLE consolidation_history ADD COLUMN w20_optimized REAL;

UPDATE schema_version SET version = 5, applied_at = datetime('now');
"#;

/// V6: Dream history persistence for automation triggers
/// Dreams were in-memory only (MemoryDreamer.dream_history Vec<DreamResult> lost on restart).
/// This table persists dream metadata so system_status can report when last dream ran.
const MIGRATION_V6_UP: &str = r#"
CREATE TABLE IF NOT EXISTS dream_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dreamed_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    memories_replayed INTEGER NOT NULL DEFAULT 0,
    connections_found INTEGER NOT NULL DEFAULT 0,
    insights_generated INTEGER NOT NULL DEFAULT 0,
    memories_strengthened INTEGER NOT NULL DEFAULT 0,
    memories_compressed INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_dream_history_dreamed_at ON dream_history(dreamed_at);

UPDATE schema_version SET version = 6, applied_at = datetime('now');
"#;

/// V7: Performance — FTS5 porter tokenizer for 15-30% better keyword recall (stemming)
/// page_size upgrade handled in apply_migrations() since VACUUM can't run inside execute_batch
const MIGRATION_V7_UP: &str = r#"
-- FTS5 porter tokenizer upgrade (15-30% better keyword recall via stemming)
DROP TRIGGER IF EXISTS knowledge_ai;
DROP TRIGGER IF EXISTS knowledge_ad;
DROP TRIGGER IF EXISTS knowledge_au;
DROP TABLE IF EXISTS knowledge_fts;

CREATE VIRTUAL TABLE knowledge_fts USING fts5(
    id, content, tags,
    content='knowledge_nodes',
    content_rowid='rowid',
    tokenize='porter ascii'
);

-- Rebuild FTS index from existing data with new tokenizer
INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild');

-- Re-create sync triggers
CREATE TRIGGER knowledge_ai AFTER INSERT ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(rowid, id, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.content, NEW.tags);
END;

CREATE TRIGGER knowledge_ad AFTER DELETE ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, id, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.content, OLD.tags);
END;

CREATE TRIGGER knowledge_au AFTER UPDATE ON knowledge_nodes BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, id, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.content, OLD.tags);
    INSERT INTO knowledge_fts(rowid, id, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.content, NEW.tags);
END;

UPDATE schema_version SET version = 7, applied_at = datetime('now');
"#;

/// V8: v1.9.0 Autonomic — Waking SWR tags, utility scoring, retention trend tracking
const MIGRATION_V8_UP: &str = r#"
-- Waking SWR (Sharp-Wave Ripple) tagging
-- Memories tagged during waking operation get preferential replay during dream cycles
ALTER TABLE knowledge_nodes ADD COLUMN waking_tag BOOLEAN DEFAULT FALSE;
ALTER TABLE knowledge_nodes ADD COLUMN waking_tag_at TEXT;

-- Utility scoring (MemRL-inspired: times_useful / times_retrieved)
ALTER TABLE knowledge_nodes ADD COLUMN utility_score REAL DEFAULT 0.0;
ALTER TABLE knowledge_nodes ADD COLUMN times_retrieved INTEGER DEFAULT 0;
ALTER TABLE knowledge_nodes ADD COLUMN times_useful INTEGER DEFAULT 0;

-- Retention trend tracking (for retention target system)
CREATE TABLE IF NOT EXISTS retention_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_at TEXT NOT NULL,
    avg_retention REAL NOT NULL,
    total_memories INTEGER NOT NULL,
    memories_below_target INTEGER NOT NULL DEFAULT 0,
    gc_triggered BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_retention_snapshots_at ON retention_snapshots(snapshot_at);

UPDATE schema_version SET version = 8, applied_at = datetime('now');
"#;

/// V9: v2.0.0 Cognitive Leap — Emotional Memory, Flashbulb Encoding, Temporal Hierarchy
///
/// Adds columns for:
/// - Emotional memory module (#29): valence scoring + flashbulb encoding (Brown & Kulik 1977)
/// - Temporal Memory Tree: hierarchical summaries (daily/weekly/monthly) for TiMem-style recall
/// - Dream phase tracking: per-phase metrics for 4-phase biologically-accurate dream cycles
const MIGRATION_V9_UP: &str = r#"
-- ============================================================================
-- EMOTIONAL MEMORY (Brown & Kulik 1977, LaBar & Cabeza 2006)
-- ============================================================================

-- Emotional valence: -1.0 (very negative) to 1.0 (very positive)
-- Used for mood-congruent retrieval and emotional decay modulation
ALTER TABLE knowledge_nodes ADD COLUMN emotional_valence REAL DEFAULT 0.0;

-- Flashbulb memory flag: ultra-high-fidelity encoding for high-importance + high-arousal events
-- Flashbulb memories get minimum decay rate and maximum context capture
ALTER TABLE knowledge_nodes ADD COLUMN flashbulb BOOLEAN DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_nodes_flashbulb ON knowledge_nodes(flashbulb);

-- ============================================================================
-- TEMPORAL MEMORY TREE (TiMem-inspired hierarchical consolidation)
-- ============================================================================

-- Temporal hierarchy level for summary nodes produced during dream consolidation
-- NULL = leaf node (raw memory), 'daily'/'weekly'/'monthly' = summary at that level
ALTER TABLE knowledge_nodes ADD COLUMN temporal_level TEXT;

-- Parent summary ID: links a leaf memory to its containing summary
ALTER TABLE knowledge_nodes ADD COLUMN summary_parent_id TEXT;

CREATE INDEX IF NOT EXISTS idx_nodes_temporal_level ON knowledge_nodes(temporal_level);
CREATE INDEX IF NOT EXISTS idx_nodes_summary_parent ON knowledge_nodes(summary_parent_id);

-- ============================================================================
-- 4-PHASE DREAM CYCLE TRACKING (NREM1 → NREM3 → REM → Integration)
-- ============================================================================

-- Extended dream history with per-phase metrics
ALTER TABLE dream_history ADD COLUMN phase_nrem1_ms INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN phase_nrem3_ms INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN phase_rem_ms INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN phase_integration_ms INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN summaries_generated INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN emotional_memories_processed INTEGER DEFAULT 0;
ALTER TABLE dream_history ADD COLUMN creative_connections_found INTEGER DEFAULT 0;

UPDATE schema_version SET version = 9, applied_at = datetime('now');
"#;

/// Get current schema version from database
pub fn get_current_version(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .or(Ok(0))
}

/// Apply pending migrations
pub fn apply_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    let current_version = get_current_version(conn)?;
    let mut applied = 0;

    for migration in MIGRATIONS {
        if migration.version > current_version {
            tracing::info!(
                "Applying migration v{}: {}",
                migration.version,
                migration.description
            );

            // Use execute_batch to handle multi-statement SQL including triggers
            conn.execute_batch(migration.up)?;

            // V7: Upgrade page_size to 8192 (10-30% faster large-row reads)
            // VACUUM rewrites the DB with the new page size — can't run inside execute_batch
            if migration.version == 7 {
                conn.pragma_update(None, "page_size", 8192)?;
                conn.execute_batch("VACUUM;")?;
                tracing::info!("Database page_size upgraded to 8192 via VACUUM");
            }

            applied += 1;
        }
    }

    Ok(applied)
}
