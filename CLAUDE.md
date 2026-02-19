# Vestige v1.5.0 — Cognitive Memory System

Vestige is your long-term memory. It implements real neuroscience: FSRS-6 spaced repetition, synaptic tagging, prediction error gating, hippocampal indexing, spreading activation, and 28 stateful cognitive modules. **Use it automatically.**

---

## Session Start Protocol

Every conversation, before responding to the user:

```
1. search("user preferences instructions")     → recall who the user is
2. search("[current project] context")          → recall project patterns/decisions
3. intention → check (with current context)     → check for triggered reminders
4. codebase → get_context (if coding)           → load patterns and decisions
```

Say "Remembering..." then retrieve context before answering.

---

## The 23 Tools

### Core Memory (2 tools)
| Tool | When to Use |
|------|-------------|
| `ingest` | Store facts, concepts, events. Raw insertion, no dedup. |
| `smart_ingest` | **Default for all saves.** Uses Prediction Error Gating to auto-decide: create, update, reinforce, or supersede. Runs cognitive pipeline (4-channel importance scoring, intent detection, synaptic tagging, hippocampal indexing). |

### Unified Tools (4 tools)
| Tool | Actions | When to Use |
|------|---------|-------------|
| `search` | query + filters | **Every time you need to recall anything.** Hybrid search (BM25 + semantic + RRF fusion). 7-stage pipeline: overfetch → rerank → temporal boost → accessibility filter → context match → competition → spreading activation. Searching strengthens memory (Testing Effect). |
| `memory` | get, delete, state | Retrieve a full memory by ID, delete a memory, or check its cognitive state (Active/Dormant/Silent/Unavailable). |
| `codebase` | remember_pattern, remember_decision, get_context | Store and recall code patterns, architectural decisions, and project context. The killer differentiator. |
| `intention` | set, check, update, list | Prospective memory — "remember to do X when Y happens". Supports time, context, and event triggers. |

### Feedback (2 tools)
| Tool | When to Use |
|------|-------------|
| `promote_memory` | User confirms a memory was helpful or correct. Increases retrieval strength + triggers reward signal + reconsolidation. |
| `demote_memory` | User says a memory was wrong or unhelpful. Decreases retrieval strength + updates competition model. Does NOT delete. |

### Temporal (2 tools)
| Tool | When to Use |
|------|-------------|
| `memory_timeline` | Browse memories chronologically. Grouped by day. Filter by type, tags, date range. Detail levels: brief/summary/full. |
| `memory_changelog` | Audit trail. Per-memory: state transitions. System-wide: consolidations + recent changes. |

### Cognitive (3 tools) — v1.5.0
| Tool | When to Use |
|------|-------------|
| `dream` | Trigger memory consolidation — replays recent memories to discover hidden connections and synthesize insights. Like sleep for AI. |
| `explore_connections` | Graph exploration. Actions: `chain` (reasoning path A→B), `associations` (spreading activation from a node), `bridges` (connecting memories between two nodes). |
| `predict` | Proactive retrieval — predicts what memories you'll need next based on context, activity patterns, and learned behavior. |

### Auto-Save & Dedup (3 tools)
| Tool | When to Use |
|------|-------------|
| `importance_score` | Score content importance before deciding whether to save. 4-channel model: novelty, arousal, reward, attention. Composite > 0.6 = worth saving. |
| `session_checkpoint` | **Batch save up to 20 items in one call.** Each routes through Prediction Error Gating. Use at session end or before context compaction. |
| `find_duplicates` | Find near-duplicate memory clusters via cosine similarity. Returns merge/review suggestions. Run when memory count > 700 or on user request. |

### Maintenance (6 tools)
| Tool | When to Use |
|------|-------------|
| `health_check` | System status: healthy/degraded/critical/empty. Actionable recommendations. |
| `consolidate` | Run FSRS-6 consolidation cycle. Applies decay, generates embeddings, maintenance. Use when memories seem stale. |
| `stats` | Full statistics: total count, retention distribution, embedding coverage, cognitive state breakdown. |
| `backup` | Create SQLite database backup. Returns file path. |
| `export` | Export memories as JSON/JSONL with tag and date filters. |
| `gc` | Garbage collect low-retention memories. Defaults to dry_run=true for safety. |

### Restore (1 tool)
| Tool | When to Use |
|------|-------------|
| `restore` | Restore memories from a JSON backup file. Supports MCP wrapper, RecallResult, and direct array formats. |

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
session_checkpoint({
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
| "This is important" | `smart_ingest` + `promote_memory` |
| "Remind me..." / "Next time..." | `intention` → set |

---

## Under the Hood — Cognitive Pipelines

### Search Pipeline (7 stages)
1. **Overfetch** — Pull 3x results from hybrid search (BM25 + semantic)
2. **Reranker** — Re-score by relevance quality
3. **Temporal boost** — Recent memories get recency bonus
4. **Accessibility filter** — FSRS-6 retention threshold (Ebbinghaus curve)
5. **Context match** — Tulving 1973 encoding specificity (match current context to encoding context)
6. **Competition** — Anderson 1994 retrieval-induced forgetting (winners strengthen, competitors weaken)
7. **Spreading activation** — Side effects: activate related memories, update predictive model, record reconsolidation opportunity

### Ingest Pipeline (cognitive pre/post)
**Pre-ingest:** 4-channel importance scoring (novelty/arousal/reward/attention) + intent detection → auto-tag
**Storage:** Prediction Error Gating decides create/update/reinforce/supersede
**Post-ingest:** Synaptic tagging (Frey & Morris 1997) + novelty model update + hippocampal indexing + cross-project recording

### Feedback Pipeline
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
- User confirms memory was helpful
- Solution worked correctly
- Information was accurate

### Demote when:
- User corrects a mistake
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

- **Crate:** `vestige-mcp` v1.5.0, Rust 2024 edition
- **Tests:** 305 tests, zero warnings (`cargo test -p vestige-mcp`)
- **Build:** `cargo build --release -p vestige-mcp`
- **Features:** `embeddings` + `vector-search` (default on)
- **Architecture:** `McpServer` holds `Arc<Mutex<Storage>>` + `Arc<Mutex<CognitiveEngine>>`
- **Entry:** `src/main.rs` → stdio JSON-RPC server
- **Tools:** `src/tools/` — one file per tool, each exports `schema()` + `execute()`
- **Cognitive:** `src/cognitive.rs` — 28-field struct, initialized once at startup
