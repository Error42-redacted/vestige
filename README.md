# Vestige

**The open-source cognitive engine for AI.**

[![GitHub stars](https://img.shields.io/github/stars/samvallad33/vestige?style=social)](https://github.com/samvallad33/vestige)
[![Release](https://img.shields.io/github/v/release/samvallad33/vestige)](https://github.com/samvallad33/vestige/releases/latest)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![MCP Compatible](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

> Your AI forgets everything between sessions. Vestige fixes that. Built on 130 years of memory research — FSRS-6 spaced repetition, prediction error gating, synaptic tagging — all running in a single Rust binary, 100% local.

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
# New session → "What are my coding preferences?"
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

[Full science documentation →](docs/SCIENCE.md)

---

## Tools

| Tool | What It Does |
|------|-------------|
| `search` | Hybrid search — keyword + semantic + RRF fusion |
| `smart_ingest` | Intelligent storage with automatic CREATE/UPDATE/SUPERSEDE |
| `ingest` | Direct memory storage |
| `memory` | Get, delete, or check memory state |
| `codebase` | Remember patterns and architectural decisions |
| `intention` | Set reminders and future triggers |
| `session_checkpoint` | Batch-save an entire session's work |
| `promote_memory` / `demote_memory` | Feedback loop — strengthen or weaken memories |
| `find_duplicates` | Self-healing — detect and merge redundant memories |
| `consolidate` | Run FSRS-6 decay and maintenance |
| `importance_score` | 4-channel importance scoring (novelty, arousal, reward, attention) |
| `memory_timeline` | Browse memories chronologically |
| `health_check` | System health with warnings and recommendations |

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

[Full CLAUDE.md templates →](docs/CLAUDE-SETUP.md)

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

- **Language:** Rust (42,000 lines)
- **Binary size:** ~20MB
- **Embeddings:** Nomic Embed Text v1.5 (768-dim, local ONNX inference via fastembed)
- **Vector search:** USearch HNSW (20x faster than FAISS)
- **Storage:** SQLite + FTS5 (optional SQLCipher encryption)
- **Transport:** MCP stdio (JSON-RPC 2.0)
- **Dependencies:** Zero runtime dependencies beyond the binary
- **First run:** Downloads embedding model (~130MB), then fully offline
- **Platforms:** macOS (ARM/Intel), Linux (x86_64), Windows

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

[More troubleshooting →](docs/FAQ.md#troubleshooting)

---

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

AGPL-3.0 — free to use, modify, and self-host. If you offer Vestige as a network service, you must open-source your modifications.

---

<p align="center">
  <i>Built by <a href="https://github.com/samvallad33">@samvallad33</a></i>
</p>
