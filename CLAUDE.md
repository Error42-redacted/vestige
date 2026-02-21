# Vestige v1.8.0 — Cognitive Memory System

Vestige is your long-term memory. It implements real neuroscience: FSRS-6 spaced repetition, synaptic tagging, prediction error gating, hippocampal indexing, spreading activation, and 28 stateful cognitive modules. **Use it automatically.**

---

## Session Start Protocol

Every conversation, before responding to the user:

```
1. session_context({                            → ONE CALL replaces steps 1-5
     queries: ["user preferences", "[project] context"],
     context: { codebase: "[project]", topics: ["[current topics]"] },
     token_budget: 1000
   })
2. Check automationTriggers from response:
   - needsDream == true  → call dream
   - needsBackup == true → call backup
   - needsGc == true     → call gc(dry_run: true)
   - totalMemories > 700 → call find_duplicates
```

Say "Remembering..." then retrieve context before answering.

> **Fallback:** If `session_context` is unavailable, use the 5-call sequence: `search` × 2 → `intention` check → `system_status` → `predict`.

---

## The 19 Tools

### Context Packets (1 tool) — v1.8.0
| Tool | When to Use |
|------|-------------|
| `session_context` | **One-call session initialization.** Replaces 5 separate calls (search × 2, intention check, system_status, predict) with a single token-budgeted response. Returns markdown context + `automationTriggers` (needsDream/needsBackup/needsGc) + `expandable` IDs for on-demand full retrieval. Params: `queries` (string[]), `token_budget` (100-10000, default 1000), `context` ({codebase, topics, file}), `include_status/include_intentions/include_predictions` (bool). |

### Core Memory (1 tool)
| Tool | When to Use |
|------|-------------|
| `smart_ingest` | **Default for all saves.** Single mode: provide `content` for auto-decide CREATE/UPDATE/SUPERSEDE via Prediction Error Gating. Batch mode: provide `items` array (max 20) for session-end saves — each item runs full cognitive pipeline (importance scoring, intent detection, synaptic tagging, hippocampal indexing). |

### Unified Tools (4 tools)
| Tool | Actions | When to Use |
|------|---------|-------------|
| `search` | query + filters | **Every time you need to recall anything.** Hybrid search (BM25 + semantic + convex combination fusion). 7-stage pipeline: overfetch → rerank → temporal boost → accessibility filter → context match → competition → spreading activation. Searching strengthens memory (Testing Effect). **v1.8.0:** optional `token_budget` param (100-10000) limits response size; results exceeding budget moved to `expandable` array. |
| `memory` | get, delete, state, promote, demote | Retrieve a full memory by ID, delete a memory, check its cognitive state (Active/Dormant/Silent/Unavailable), promote (thumbs up — increases retrieval strength), or demote (thumbs down — decreases retrieval strength, does NOT delete). |
| `codebase` | remember_pattern, remember_decision, get_context | Store and recall code patterns, architectural decisions, and project context. The killer differentiator. |
| `intention` | set, check, update, list | Prospective memory — "remember to do X when Y happens". Supports time, context, and event triggers. |

### Temporal (2 tools)
| Tool | When to Use |
|------|-------------|
| `memory_timeline` | Browse memories chronologically. Grouped by day. Filter by type, tags, date range. When user references a time period ("last week", "yesterday"). |
| `memory_changelog` | Audit trail. Per-memory: state transitions. System-wide: consolidations + recent changes. When debugging memory issues. |

### Cognitive (3 tools) — v1.5.0
| Tool | When to Use |
|------|-------------|
| `dream` | Trigger memory consolidation — replays recent memories to discover hidden connections and synthesize insights. At session start if >24h since last dream, after every 50 saves. |
| `explore_connections` | Graph exploration. Actions: `chain` (reasoning path A→B), `associations` (spreading activation from a node), `bridges` (connecting memories between two nodes). When search returns 3+ related results. |
| `predict` | Proactive retrieval — predicts what memories you'll need next based on context, activity patterns, and learned behavior. At session start, when switching projects. |

### Auto-Save & Dedup (2 tools)
| Tool | When to Use |
|------|-------------|
| `importance_score` | Score content importance before deciding whether to save. 4-channel model: novelty, arousal, reward, attention. Composite > 0.6 = worth saving. |
| `find_duplicates` | Find near-duplicate memory clusters via cosine similarity. Returns merge/review suggestions. Run when memory count > 700 or on user request. |

### Maintenance (5 tools)
| Tool | When to Use |
|------|-------------|
| `system_status` | **Combined health + stats.** Returns status (healthy/degraded/critical/empty), full statistics, FSRS preview, cognitive module health, state distribution, warnings, and recommendations. At session start (or use `session_context` which includes this). |
| `consolidate` | Run FSRS-6 consolidation cycle. Applies decay, generates embeddings, maintenance. At session end, when retention drops. |
| `backup` | Create SQLite database backup. Before major upgrades, weekly. |
| `export` | Export memories as JSON/JSONL with tag and date filters. |
| `gc` | Garbage collect low-retention memories. When system_status shows degraded + high count. Defaults to dry_run=true. |

### Restore (1 tool)
| Tool | When to Use |
|------|-------------|
| `restore` | Restore memories from a JSON backup file. Supports MCP wrapper, RecallResult, and direct array formats. |

### Deprecated (still work via redirects)
| Old Tool | Redirects To |
|----------|-------------|
| `ingest` | `smart_ingest` |
| `session_checkpoint` | `smart_ingest` (batch mode) |
| `promote_memory` | `memory(action="promote")` |
| `demote_memory` | `memory(action="demote")` |
| `health_check` | `system_status` |
| `stats` | `system_status` |

---

## Mandatory Save Gates

**RULE: You MUST NOT proceed past a save gate without executing the save.**

### BUG_FIX — After any error is resolved
Your next tool call after confirming a fix MUST be `smart_ingest`:
```
smart_ingest({
  content: "BUG FIX: [exact error]\nRoot cause: [why]\nSolution: [what fixed it]\nFiles: [paths]",
  tags: ["bug-fix", "[project]"], node_type: "fact"
})
```

### DECISION — After any architectural or design choice
```
codebase({
  action: "remember_decision",
  decision: "[what]", rationale: "[why]",
  alternatives: ["[A]", "[B]"], files: ["[affected]"], codebase: "[project]"
})
```

### CODE_CHANGE — After writing significant code (>20 lines or new pattern)
```
codebase({
  action: "remember_pattern",
  name: "[pattern]", description: "[how/when to use]",
  files: ["[files]"], codebase: "[project]"
})
```

### SESSION_END — Before stopping or compaction
```
smart_ingest({
  items: [
    { content: "SESSION: [work done]\nFixes: [list]\nDecisions: [list]", tags: ["session-end", "[project]"] },
    // ... any unsaved fixes, decisions, patterns
  ]
})
```

---

## Trigger Words — Auto-Save

| User Says | Action |
|-----------|--------|
| "Remember this" / "Don't forget" | `smart_ingest` immediately |
| "I always..." / "I never..." / "I prefer..." | Save as preference |
| "This is important" | `smart_ingest` + `memory(action="promote")` |
| "Remind me..." / "Next time..." | `intention` → set |

---

## Under the Hood — Cognitive Pipelines

### Search Pipeline (7 stages)
1. **Overfetch** — Pull 3x results from hybrid search (BM25 + semantic)
2. **Reranker** — Re-score by relevance quality (cross-encoder)
3. **Temporal boost** — Recent memories get recency bonus
4. **Accessibility filter** — FSRS-6 retention threshold (Ebbinghaus curve)
5. **Context match** — Tulving 1973 encoding specificity (match current context to encoding context)
6. **Competition** — Anderson 1994 retrieval-induced forgetting (winners strengthen, competitors weaken)
7. **Spreading activation** — Side effects: activate related memories, update predictive model, record reconsolidation opportunity

### Ingest Pipeline (cognitive pre/post)
**Pre-ingest:** 4-channel importance scoring (novelty/arousal/reward/attention) + intent detection → auto-tag
**Storage:** Prediction Error Gating decides create/update/reinforce/supersede
**Post-ingest:** Synaptic tagging (Frey & Morris 1997) + novelty model update + hippocampal indexing + cross-project recording

### Feedback Pipeline (via memory promote/demote)
**Promote:** Reward signal + importance boost + reconsolidation (memory becomes modifiable for 24-48h) + activation spread
**Demote:** Competition suppression + retrieval strength decrease (does NOT delete — alternatives surface instead)

---

## CognitiveEngine — 28 Modules

All modules persist across tool calls as stateful instances:

**Neuroscience (15):** ActivationNetwork, SynapticTaggingSystem, HippocampalIndex, ContextMatcher, AccessibilityCalculator, CompetitionManager, StateUpdateService, ImportanceSignals, NoveltySignal, ArousalSignal, RewardSignal, AttentionSignal, PredictiveMemory, ProspectiveMemory, IntentionParser

**Advanced (11):** ImportanceTracker, ReconsolidationManager, IntentDetector, ActivityTracker, MemoryDreamer, MemoryChainBuilder, MemoryCompressor, CrossProjectLearner, AdaptiveEmbedder, SpeculativeRetriever, ConsolidationScheduler

**Search (2):** Reranker, TemporalSearcher

---

## Memory Hygiene

### Promote when:
- User confirms memory was helpful → `memory(action="promote")`
- Solution worked correctly
- Information was accurate

### Demote when:
- User corrects a mistake → `memory(action="demote")`
- Information was wrong
- Memory led to bad outcome

### Never save:
- Secrets, API keys, passwords
- Temporary debugging state
- Obvious/trivial information

---

## The One Rule

**When in doubt, save. The cost of a duplicate is near zero (Prediction Error Gating handles dedup). The cost of lost knowledge is permanent.**

Memory is retrieval. Searching strengthens memory. Search liberally, save aggressively.

---

## Development

- **Crate:** `vestige-mcp` v1.8.0, Rust 2024 edition, Rust 1.93.1
- **Tests:** 651 tests (313 core + 338 mcp), zero warnings
- **Build:** `cargo build --release -p vestige-mcp`
- **Features:** `embeddings` + `vector-search` (default on)
- **Architecture:** `McpServer` holds `Arc<Storage>` + `Arc<Mutex<CognitiveEngine>>`
- **Storage:** Interior mutability — `Storage` uses `Mutex<Connection>` for reader/writer split, all methods take `&self`. WAL mode for concurrent reads + writes.
- **Entry:** `src/main.rs` → stdio JSON-RPC server
- **Tools:** `src/tools/` — one file per tool, each exports `schema()` + `execute()`
- **Cognitive:** `src/cognitive.rs` — 28-field struct, initialized once at startup
