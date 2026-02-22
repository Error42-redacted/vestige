# Changelog

All notable changes to Vestige will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-02-22 — "Cognitive Leap"

The biggest release in Vestige history. A complete visual and cognitive overhaul.

### Added

#### 3D Memory Dashboard
- **SvelteKit 2 + Three.js dashboard** — full 3D neural visualization at `localhost:3927/dashboard`
- **7 interactive pages**: Graph (3D force-directed), Memories (browser), Timeline, Feed (real-time events), Explore (connections), Intentions, Stats
- **WebSocket event bus** — `tokio::broadcast` channel with 16 event types (MemoryCreated, SearchPerformed, DreamStarted/Completed, ConsolidationStarted/Completed, RetentionDecayed, ConnectionDiscovered, ActivationSpread, ImportanceScored, Heartbeat, etc.)
- **Real-time 3D animations** — memories pulse on access, burst particles on creation, shockwave rings on dreams, golden flash lines on connection discovery, fade on decay
- **Bloom post-processing** — cinematic neural network aesthetic with UnrealBloomPass
- **GPU instanced rendering** — 1000+ nodes at 60fps via Three.js InstancedMesh
- **Text label sprites** — distance-based visibility (fade in <40 units, out >80 units), canvas-based rendering
- **Dream visualization mode** — purple ambient, slow-motion orbit, sequential memory replay
- **FSRS retention curves** — SVG `R(t) = e^(-t/S)` with prediction pills at 1d/7d/30d
- **Command palette** — `Cmd+K` navigation with filtered search
- **Keyboard shortcuts** — `G` Graph, `M` Memories, `T` Timeline, `F` Feed, `E` Explore, `I` Intentions, `S` Stats, `/` Search
- **Responsive layout** — desktop sidebar + mobile bottom nav with safe-area-inset
- **PWA support** — installable via `manifest.json`
- **Single binary deployment** — SvelteKit build embedded via `include_dir!` macro

#### Engine Upgrades
- **HyDE query expansion** — template-based Hypothetical Document Embeddings: classify_intent (6 types) → expand_query (3-5 variants) → centroid_embedding. Wired into `semantic_search_raw`
- **fastembed 5.11** — upgraded from 5.9, adds Nomic v2 MoE + Qwen3 reranker support
- **Nomic Embed Text v2 MoE** — opt-in via `--features nomic-v2` (475M params, 305M active, 8 experts, Candle backend)
- **Qwen3 Reranker** — opt-in via `--features qwen3-reranker` (Candle backend, high-precision cross-encoder)
- **Metal GPU acceleration** — opt-in via `--features metal` (Apple Silicon, significantly faster embedding inference)

#### Backend
- **Axum WebSocket** — `/ws` endpoint with 5-second heartbeat, live stats (memory count, avg retention, uptime)
- **7 new REST endpoints** — `POST /api/dream`, `/api/explore`, `/api/predict`, `/api/importance`, `/api/consolidate`, `GET /api/search`, `/api/retention-distribution`, `/api/intentions`
- **Event emission from MCP tools** — `emit_tool_event()` broadcasts events for smart_ingest, search, dream, consolidate, memory, importance_score
- **Shared broadcast channel** — single `tokio::broadcast::channel(1024)` shared between dashboard and MCP server
- **CORS for SvelteKit dev** — `localhost:5173` allowed in dev mode

#### Benchmarks
- **Criterion benchmark suite** — `cosine_similarity` 296ns, `centroid` 1.3µs, HyDE expand 1.4µs, RRF fusion 17µs

### Changed
- Version: 1.8.0 → 2.0.0 (both crates)
- Rust edition: 2024 (MSRV 1.85)
- Tests: 651 → 734 (352 core + 378 mcp + 4 doctests)
- Binary size: ~22MB (includes embedded SvelteKit dashboard)
- CognitiveEngine moved from main.rs binary crate to lib.rs for dashboard access
- Dashboard served at `/dashboard` prefix (legacy HTML kept at `/` and `/graph`)
- `McpServer` now accepts optional `broadcast::Sender<VestigeEvent>` for event emission

### Technical
- `apps/dashboard/` — new SvelteKit app (Svelte 5, Tailwind CSS 4, Three.js 0.172, `@sveltejs/adapter-static`)
- `dashboard/events.rs` — 16-variant `VestigeEvent` enum with `#[serde(tag = "type", content = "data")]`
- `dashboard/websocket.rs` — WebSocket upgrade handler with heartbeat + event forwarding
- `dashboard/static_files.rs` — `include_dir!` macro for embedded SvelteKit build
- `search/hyde.rs` — HyDE module with intent classification and query expansion
- `benches/search_bench.rs` — Criterion benchmarks for search pipeline components

---

## [1.8.0] - 2026-02-21

### Added
- **`session_context` tool** — one-call session initialization replacing 5 separate calls (search × 2, intention check, system_status, predict). Token-budgeted responses (~15K tokens → ~500-1000 tokens). Returns assembled markdown context, `automationTriggers` (needsDream/needsBackup/needsGc), and `expandable` memory IDs for on-demand retrieval.
- **`token_budget` parameter on `search`** — limits response size (100-10000 tokens). Results exceeding budget moved to `expandable` array with `tokensUsed`/`tokenBudget` tracking.
- **Reader/writer connection split** — `Storage` struct uses `Mutex<Connection>` for separate reader/writer SQLite handles with WAL mode. All methods take `&self` (interior mutability). `Arc<Mutex<Storage>>` → `Arc<Storage>` across ~30 files.
- **int8 vector quantization** — `ScalarKind::F16` → `I8` (2x memory savings, <1% recall loss)
- **Migration v7** — FTS5 porter tokenizer (15-30% keyword recall) + page_size 8192 (10-30% faster large-row reads)
- 22 new tests for session_context and token_budget (335 → 357 mcp tests, 651 total)

### Changed
- Tool count: 18 → 19
- `EmbeddingService::init()` changed from `&mut self` to `&self` (dead `model_loaded` field removed)
- CLAUDE.md updated: session start uses `session_context`, 19 tools documented, development section reflects storage architecture

### Performance
- Session init: ~15K tokens → ~500-1000 tokens (single tool call)
- Vector storage: 2x reduction (F16 → I8)
- Keyword search: 15-30% better recall (FTS5 porter stemming)
- Large-row reads: 10-30% faster (page_size 8192)
- Concurrent reads: non-blocking (reader/writer WAL split)

---

## [1.7.0] - 2026-02-20

### Changed
- **Tool consolidation: 23 → 18 tools** — merged redundant tools while maintaining 100% backward compatibility via deprecated redirects
- **`ingest` → `smart_ingest`** — `ingest` was a duplicate of `smart_ingest`; now redirects automatically
- **`session_checkpoint` → `smart_ingest` batch mode** — new `items` parameter on `smart_ingest` accepts up to 20 items, each running the full cognitive pipeline (importance scoring, intent detection, synaptic tagging, hippocampal indexing). Old `session_checkpoint` skipped the cognitive pipeline.
- **`promote_memory` + `demote_memory` → `memory` unified** — new `promote` and `demote` actions on the `memory` tool with optional `reason` parameter and full cognitive feedback pipeline (reward signal, reconsolidation, competition)
- **`health_check` + `stats` → `system_status`** — single tool returns combined health status, full statistics, FSRS preview, cognitive module health, state distribution, warnings, and recommendations
- **CLAUDE.md automation overhaul** — all 18 tools now have explicit auto-trigger rules; session start expanded to 5 steps (added `system_status` + `predict`); full proactive behaviors table

### Added
- `smart_ingest` batch mode with `items` parameter (max 20 items, full cognitive pipeline per item)
- `memory` actions: `promote` and `demote` with optional `reason` parameter
- `system_status` tool combining health check + statistics + cognitive health
- 30 new tests (305 → 335)

### Deprecated (still work via redirects)
- `ingest` → use `smart_ingest`
- `session_checkpoint` → use `smart_ingest` with `items`
- `promote_memory` → use `memory(action="promote")`
- `demote_memory` → use `memory(action="demote")`
- `health_check` → use `system_status`
- `stats` → use `system_status`

---

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
