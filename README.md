# Vestige

**The open-source cognitive engine for AI.**

[![GitHub stars](https://img.shields.io/github/stars/samvallad33/vestige?style=social)](https://github.com/samvallad33/vestige)
[![Release](https://img.shields.io/github/v/release/samvallad33/vestige)](https://github.com/samvallad33/vestige/releases/latest)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![MCP Compatible](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

> Your AI forgets everything between sessions. Vestige fixes that. Built on 130 years of memory research — FSRS-6 spaced repetition, prediction error gating, synaptic tagging — all running in a single Rust binary, 100% local.

### What's New in v1.9.1

- **Self-regulating memory** — Retention Target System auto-GCs decaying memories, Auto-Promote boosts frequently accessed memories, Waking SWR Tags give promoted memories preferential dream replay
- **`memory_health`** — retention dashboard with distribution buckets, trend tracking, and recommendations
- **`memory_graph`** — knowledge graph visualization with Fruchterman-Reingold force-directed layout
- **Dream persistence** — dream-discovered connections now persist to database, enabling graph traversal across your knowledge network
- **21 MCP tools** — up from 19

See [CHANGELOG](CHANGELOG.md) for full version history.

---

## Give Your AI a Brain in 30 Seconds

```bash
# 1. Install
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-aarch64-apple-darwin.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/

# 2. Connect
claude mcp add vestige vestige-mcp -s user

# 3. Test
# "Remember that I prefer TypeScript over JavaScript"
# New session -> "What are my coding preferences?"
# It remembers.
```

<details>
<summary>Other platforms & install methods</summary>

**macOS (Intel):**
```bash
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-x86_64-apple-darwin.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/
```

**Linux:**
```bash
curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/
```

**Windows:** Download from [Releases](https://github.com/samvallad33/vestige/releases/latest)

**Build from source:**
```bash
git clone https://github.com/samvallad33/vestige && cd vestige
cargo build --release
sudo cp target/release/{vestige-mcp,vestige,vestige-restore} /usr/local/bin/
```

**npm:**
```bash
npm install -g vestige-mcp
```
</details>

---

## Works Everywhere

Vestige speaks MCP — the universal protocol for AI tools. One brain, every IDE.

| IDE | Setup |
|-----|-------|
| **Claude Code** | `claude mcp add vestige vestige-mcp -s user` |
| **Claude Desktop** | [2-min setup](docs/CONFIGURATION.md#claude-desktop-macos) |
| **Xcode 26.3** | [Integration guide](docs/integrations/xcode.md) |
| **Cursor** | [Integration guide](docs/integrations/cursor.md) |
| **VS Code (Copilot)** | [Integration guide](docs/integrations/vscode.md) |
| **JetBrains** | [Integration guide](docs/integrations/jetbrains.md) |
| **Windsurf** | [Integration guide](docs/integrations/windsurf.md) |

Fix a bug in VS Code. Open Xcode. Your AI already knows about the fix.

---

## Why Not Just Use RAG?

RAG is a dumb bucket. Vestige is an active organ.

| | RAG / Vector Store | Vestige |
|---|---|---|
| **Storage** | Store everything, retrieve everything | **Prediction Error Gating** — only stores what's surprising or new |
| **Retrieval** | Nearest-neighbor similarity | **Spreading activation** — finds related memories through association chains |
| **Decay** | Nothing ever expires | **FSRS-6** — memories fade like yours do, keeping context lean |
| **Duplicates** | Manual dedup or none | **Self-healing** — automatically merges "likes dark mode" + "prefers dark themes" |
| **Importance** | All memories are equal | **Synaptic tagging** — retroactively strengthens memories that turn out to matter |
| **Health** | No visibility | **Retention dashboard** — track avg retention, distribution, trends, and recommendations |
| **Privacy** | Usually cloud-dependent | **100% local** — your data never leaves your machine |

---

## The Cognitive Science Stack

This isn't a key-value store with an embedding model bolted on. Vestige implements real neuroscience:

**Prediction Error Gating** — The bouncer for your brain. When new information arrives, Vestige compares it against existing memories. Redundant? Merged. Contradictory? Superseded. Novel? Stored. Just like the hippocampus.

**FSRS-6 Spaced Repetition** — 21 parameters governing the mathematics of forgetting. Frequently-used memories stay strong. Unused memories naturally decay. Your context window stays clean.

**Synaptic Tagging** — A memory that seemed trivial this morning can be retroactively tagged as critical tonight. Based on [Frey & Morris, 1997](https://doi.org/10.1038/385533a0).

**Spreading Activation** — Search for "auth bug" and find the related memory about the JWT library update you saved last week. Memories form a graph, not a flat list. Based on [Collins & Loftus, 1975](https://doi.org/10.1037/0033-295X.82.6.407).

**Dual-Strength Model** — Every memory has two values: storage strength (how well it's encoded) and retrieval strength (how easily it surfaces). A memory can be deeply stored but temporarily hard to retrieve — just like real forgetting. Based on [Bjork & Bjork, 1992](https://doi.org/10.1016/S0079-7421(08)60016-9).

**Memory States** — Active, Dormant, Silent, Unavailable. Memories transition between states based on usage patterns, exactly like human cognitive architecture.

**Memory Dreaming** *(v1.5.0)* — Like sleep consolidation. Replays recent memories to discover hidden connections, strengthen important patterns, and synthesize insights. Connections persist to a graph database for traversal. Based on the [Active Dreaming Memory](https://engrxiv.org/preprint/download/5919/9826/8234) framework.

**ACT-R Activation** *(v1.5.0)* — Retrieval strength depends on BOTH recency AND frequency of access, computed from full access history. A memory accessed 50 times over 3 weeks is stronger than one accessed once yesterday. Based on [Anderson, 1993](http://act-r.psy.cmu.edu/).

**Waking SWR Tagging** *(v1.9.0)* — Memories promoted during waking use get sharp-wave ripple tags for preferential replay during dream consolidation. 70/30 tagged-to-random ratio ensures important memories get replayed first. Based on [Buzsaki, 2015](https://doi.org/10.1038/nn.3963).

**Autonomic Regulation** *(v1.9.0)* — Self-regulating memory health. Auto-promotes memories accessed 3+ times in 24h (frequency-dependent potentiation). Auto-GCs low-retention memories when average retention falls below target. Consolidation triggers on 6h staleness or 2h active use.

[Full science documentation ->](docs/SCIENCE.md)

---

## Tools — 21 MCP Tools

### Context Packets (v1.8.0)
| Tool | What It Does |
|------|-------------|
| `session_context` | **One-call session init** — replaces 5 calls with a single token-budgeted response. Returns context, automation triggers, and expandable memory IDs |

### Core Memory
| Tool | What It Does |
|------|-------------|
| `search` | 7-stage cognitive search — keyword + semantic + convex fusion + reranking + temporal boost + competition + spreading activation. Optional `token_budget` for cost control |
| `smart_ingest` | Intelligent storage with automatic CREATE/UPDATE/SUPERSEDE via Prediction Error Gating. Batch mode for session-end saves |
| `memory` | Get, delete, check state, promote (thumbs up), or demote (thumbs down) |
| `codebase` | Remember code patterns and architectural decisions per-project |
| `intention` | Prospective memory — "remind me to X when Y happens" |

### Cognitive Engine
| Tool | What It Does |
|------|-------------|
| `dream` | Memory consolidation via replay — discovers hidden connections, synthesizes insights, persists connections to graph database |
| `explore_connections` | Graph traversal — reasoning chains, associations via spreading activation, bridges between memories |
| `predict` | Proactive retrieval — predicts what memories you'll need next based on context and activity patterns |

### Autonomic (v1.9.0)
| Tool | What It Does |
|------|-------------|
| `memory_health` | Retention dashboard — avg retention, distribution buckets (0-20%, 20-40%, etc.), trend (improving/declining/stable), recommendations |
| `memory_graph` | Knowledge graph visualization — subgraph export with Fruchterman-Reingold force-directed layout, up to 200 nodes with edge weights |

### Scoring & Dedup
| Tool | What It Does |
|------|-------------|
| `importance_score` | 4-channel neuroscience scoring (novelty, arousal, reward, attention) |
| `find_duplicates` | Self-healing — detect and merge redundant memories via cosine similarity |

### Maintenance & Data
| Tool | What It Does |
|------|-------------|
| `system_status` | Combined health + statistics + cognitive state breakdown + recommendations |
| `consolidate` | Run FSRS-6 decay cycle (also runs automatically every 6 hours) |
| `memory_timeline` | Browse memories chronologically, grouped by day |
| `memory_changelog` | Audit trail of memory state transitions |
| `backup` / `export` / `gc` | Database backup, JSON export, garbage collection |
| `restore` | Restore memories from JSON backup files |

---

## Make Your AI Use Vestige Automatically

Add this to your `CLAUDE.md` and your AI becomes proactive:

```markdown
## Memory

At the start of every session:
1. Search Vestige for user preferences and project context
2. Save bug fixes, decisions, and patterns without being asked
3. Create reminders when the user mentions deadlines
```

| You Say | AI Does |
|---------|---------|
| "Remember this" | Saves immediately |
| "I prefer..." / "I always..." | Saves as preference |
| "Remind me..." | Creates a future trigger |
| "This is important" | Saves + strengthens |

[Full CLAUDE.md templates ->](docs/CLAUDE-SETUP.md)

---

## CLI

```bash
vestige stats              # Memory statistics
vestige stats --tagging    # Retention distribution
vestige stats --states     # Cognitive state breakdown
vestige health             # System health check
vestige consolidate        # Run memory maintenance
vestige restore <file>     # Restore from backup
```

---

## Technical Details

- **Language:** Rust (55,000+ lines, 1,100+ tests)
- **Binary size:** ~20MB
- **Embeddings:** Nomic Embed Text v1.5 (768-dim, local ONNX inference via fastembed)
- **Vector search:** USearch HNSW (20x faster than FAISS)
- **Storage:** SQLite + FTS5 (optional SQLCipher encryption)
- **Transport:** MCP stdio (JSON-RPC 2.0)
- **Dependencies:** Zero runtime dependencies beyond the binary
- **First run:** Downloads embedding model (~130MB), then fully offline
- **Platforms:** macOS (ARM/Intel), Linux (x86_64), Windows
- **Cognitive modules:** 28 stateful modules (15 neuroscience, 11 advanced, 2 search)

---

## Documentation

| Document | Contents |
|----------|----------|
| [FAQ](docs/FAQ.md) | 30+ answers to common questions |
| [How It Works](docs/SCIENCE.md) | The neuroscience behind every feature |
| [Storage Modes](docs/STORAGE.md) | Global, per-project, multi-instance setup |
| [CLAUDE.md Setup](docs/CLAUDE-SETUP.md) | Templates for proactive memory |
| [Configuration](docs/CONFIGURATION.md) | CLI commands, environment variables |
| [Integrations](docs/integrations/) | Xcode, Cursor, VS Code, JetBrains, Windsurf |
| [Changelog](CHANGELOG.md) | Version history |

---

## Troubleshooting

<details>
<summary>"Command not found" after installation</summary>

Ensure `vestige-mcp` is in your PATH:
```bash
which vestige-mcp
```

Or use the full path:
```bash
claude mcp add vestige /usr/local/bin/vestige-mcp -s user
```
</details>

<details>
<summary>Embedding model download fails</summary>

First run downloads ~130MB from Hugging Face. If behind a proxy:
```bash
export HTTPS_PROXY=your-proxy:port
```

Cache locations:
- **macOS**: `~/Library/Caches/com.vestige.core/fastembed`
- **Linux**: `~/.cache/vestige/fastembed`
- **Windows**: `%LOCALAPPDATA%\vestige\cache\fastembed`
</details>

[More troubleshooting ->](docs/FAQ.md#troubleshooting)

---

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

AGPL-3.0 — free to use, modify, and self-host. If you offer Vestige as a network service, you must open-source your modifications.

---

<p align="center">
  <i>Built by <a href="https://github.com/samvallad33">@samvallad33</a></i>
</p>
