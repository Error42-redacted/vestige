# Changelog

All notable changes to Vestige will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.6.0] - 2026-02-19

### Changed
- **F16 vector quantization** — USearch vectors stored as F16 instead of F32 (2x storage savings)
- **Matryoshka 256-dim truncation** — embedding dimensions reduced from 768 to 256 (3x embedding storage savings)
- **Convex Combination fusion** — replaced RRF with 0.3 keyword / 0.7 semantic weighted fusion for better score preservation
- **Cross-encoder reranker** — added Jina Reranker v1 Turbo (fastembed TextRerank) for neural reranking (~20% retrieval quality improvement)
- Combined: **6x vector storage reduction** with better retrieval quality
- Cross-encoder loads in background — server starts instantly
- Old 768-dim embeddings auto-migrated on load

---

## [1.5.0] - 2026-02-18

### Added
- **CognitiveEngine** — 28-module stateful engine with full neuroscience pipeline on every tool call
- **`dream`** tool — memory consolidation via replay, discovers hidden connections and synthesizes insights
- **`explore_connections`** tool — graph traversal with chain, associations, and bridges actions
- **`predict`** tool — proactive retrieval based on context and activity patterns
- **`restore`** tool — restore memories from JSON backup files
- **Automatic consolidation** — FSRS-6 decay runs on a 6-hour timer + inline every 100 tool calls
- ACT-R base-level activation with full access history
- Episodic-to-semantic auto-merge during consolidation
- Cross-memory reinforcement on access
- Park et al. triple retrieval scoring
- Personalized w20 optimization

### Changed
- All existing tools upgraded with cognitive pre/post processing pipelines
- Tool count: 19 → 23

---

## [1.3.0] - 2026-02-12

### Added
- **`importance_score`** tool — 4-channel neuroscience scoring (novelty, arousal, reward, attention)
- **`session_checkpoint`** tool — batch smart_ingest up to 20 items with Prediction Error Gating
- **`find_duplicates`** tool — cosine similarity clustering with union-find for dedup
- `vestige ingest` CLI command for memory ingestion via command line

### Changed
- Tool count: 16 → 19
- Made `get_node_embedding` public in core API
- Added `get_all_embeddings` for duplicate scanning

---

## [1.2.0] - 2026-02-12

### Added
- **Web dashboard** — Axum-based on port 3927 with memory browser, search, and system stats
- **`memory_timeline`** tool — browse memories chronologically, grouped by day
- **`memory_changelog`** tool — audit trail of memory state transitions
- **`health_check`** tool — system health status with recommendations
- **`consolidate`** tool — run FSRS-6 maintenance cycle
- **`stats`** tool — full memory system statistics
- **`backup`** tool — create SQLite database backups
- **`export`** tool — export memories as JSON/JSONL with filters
- **`gc`** tool — garbage collect low-retention memories
- `backup_to()` and `get_recent_state_transitions()` storage APIs

### Changed
- Search now supports `detail_level` (brief/summary/full) to control token usage
- Tool count: 8 → 16

---

## [1.1.3] - 2026-02-12

### Changed
- Upgraded to Rust edition 2024
- Security hardening and dependency updates

### Fixed
- Dedup on ingest edge cases
- Intel Mac CI builds
- NPM package version alignment
- Removed dead TypeScript package

---

## [1.1.2] - 2025-01-27

### Fixed
- Embedding model cache now uses platform-appropriate directories instead of polluting project folders
  - macOS: `~/Library/Caches/com.vestige.core/fastembed`
  - Linux: `~/.cache/vestige/fastembed`
  - Windows: `%LOCALAPPDATA%\vestige\cache\fastembed`
- Can still override with `FASTEMBED_CACHE_PATH` environment variable

---

## [1.1.1] - 2025-01-27

### Fixed
- UTF-8 string slicing issues in keyword search and prospective memory
- Silent error handling in MCP stdio protocol
- Feature flag forwarding between crates
- All GitHub issues resolved (#1, #3, #4)

### Added
- Pre-built binaries for Linux, Windows, and macOS (Intel & ARM)
- GitHub Actions CI/CD for automated releases

---

## [1.1.0] - 2025-01-26

### Changed
- **Tool Consolidation**: 29 tools → 8 cognitive primitives
  - `recall`, `semantic_search`, `hybrid_search` → `search`
  - `get_knowledge`, `delete_knowledge`, `get_memory_state` → `memory`
  - `remember_pattern`, `remember_decision`, `get_codebase_context` → `codebase`
  - 5 intention tools → `intention`
- Stats and maintenance moved from MCP to CLI (`vestige stats`, `vestige health`, etc.)

### Added
- CLI admin commands: `vestige stats`, `vestige health`, `vestige consolidate`, `vestige restore`
- Feedback tools: `promote_memory`, `demote_memory`
- 30+ FAQ entries with verified neuroscience claims
- Storage modes documentation: Global, per-project, multi-Claude household
- CLAUDE.md templates for proactive memory use
- Version pinning via git tags

### Deprecated
- Old tool names (still work with warnings, removed in v2.0)

---

## [1.0.0] - 2025-01-25

### Added
- FSRS-6 spaced repetition algorithm with 21 parameters
- Bjork & Bjork dual-strength memory model (storage + retrieval strength)
- Local semantic embeddings with fastembed v5 (BGE-base-en-v1.5, 768 dimensions)
- HNSW vector search with USearch (20x faster than FAISS)
- Hybrid search combining BM25 keyword + semantic + RRF fusion
- Two-stage retrieval with reranking (+15-20% precision)
- MCP server for Claude Desktop integration
- Tauri desktop application
- Codebase memory module for AI code understanding
- Neuroscience-inspired memory mechanisms:
  - Synaptic Tagging and Capture (retroactive importance)
  - Context-Dependent Memory (Tulving encoding specificity)
  - Spreading Activation Networks
  - Memory States (Active/Dormant/Silent/Unavailable)
  - Multi-channel Importance Signals (Novelty/Arousal/Reward/Attention)
  - Hippocampal Indexing (Teyler & Rudy 2007)
- Prospective memory (intentions and reminders)
- Sleep consolidation with 5-stage processing
- Memory compression for long-term storage
- Cross-project learning for universal patterns

### Changed
- Upgraded embedding model from all-MiniLM-L6-v2 (384d) to BGE-base-en-v1.5 (768d)
- Upgraded fastembed from v4 to v5

### Fixed
- SQL injection protection in FTS5 queries
- Infinite loop prevention in file watcher
- SIGSEGV crash in vector index (reserve before add)
- Memory safety with Mutex wrapper for embedding model

---

## [0.1.0] - 2025-01-24

### Added
- Initial release
- Core memory storage with SQLite + FTS5
- Basic FSRS scheduling
- MCP protocol support
- Desktop app skeleton
