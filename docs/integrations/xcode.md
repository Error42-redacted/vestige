# Xcode 26.3

> Give Apple Intelligence a brain that remembers.

Xcode 26.3 supports [agentic coding](https://developer.apple.com/documentation/xcode/giving-agentic-coding-tools-access-to-xcode) with full MCP (Model Context Protocol) integration. Vestige plugs directly into Xcode's Claude Agent, giving it persistent memory across every coding session.

**Vestige is the first cognitive memory server for Xcode.**

---

## Prerequisites

- **Xcode 26.3** or later (Release Candidate or stable)
- **vestige-mcp** binary installed ([Installation guide](../../README.md#quick-start))

Verify Vestige is installed:

```bash
which vestige-mcp
# Should output: /usr/local/bin/vestige-mcp
```

If you installed to a different location, note the **absolute path** — you'll need it below.

---

## Setup

### 1. Open the config file

Xcode's Claude Agent reads MCP configuration from:

```
~/Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig/.claude
```

Create or edit this file:

```bash
mkdir -p ~/Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig
open -e ~/Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig/.claude
```

### 2. Add Vestige

Paste the following configuration:

```json
{
  "projects": {
    "*": {
      "mcpServers": {
        "vestige": {
          "type": "stdio",
          "command": "/usr/local/bin/vestige-mcp",
          "args": [],
          "env": {
            "PATH": "/usr/local/bin:/usr/bin:/bin"
          }
        }
      },
      "hasTrustDialogAccepted": true
    }
  }
}
```

> **Important:** Xcode runs agents in a sandboxed environment that does **not** inherit your shell configuration (`.zshrc`, `.bashrc`, etc.). You **must** use absolute paths for the `command` field and explicitly set `PATH` in the `env` block.

#### Project-specific memory

To give each project its own isolated memory, use `--data-dir`:

```json
{
  "projects": {
    "/Users/you/Developer/MyApp": {
      "mcpServers": {
        "vestige": {
          "type": "stdio",
          "command": "/usr/local/bin/vestige-mcp",
          "args": ["--data-dir", "/Users/you/Developer/MyApp/.vestige"],
          "env": {
            "PATH": "/usr/local/bin:/usr/bin:/bin"
          }
        }
      },
      "hasTrustDialogAccepted": true
    }
  }
}
```

### 3. Restart Xcode

Quit and reopen Xcode. The Claude Agent will now load Vestige on startup.

### 4. Verify

In Xcode's Agent panel, type:

```
/context
```

You should see `vestige` listed as an available MCP server with its tools (search, smart_ingest, memory, etc.).

---

## First Use

Open any project and ask the Claude Agent:

> "Remember that this project uses SwiftUI with MVVM architecture and targets iOS 18+"

Start a **new session**, then ask:

> "What architecture does this project use?"

It remembers.

---

## What Vestige Does for Xcode

| Without Vestige | With Vestige |
|-----------------|--------------|
| Every session starts from zero | Agent recalls your architecture, patterns, and preferences |
| Re-explain SwiftUI conventions each time | Agent knows your conventions from day one |
| Bug fixes are forgotten | Agent remembers past fixes and avoids regressions |
| No context between Xcode and other IDEs | Shared memory across Xcode, Cursor, VS Code, and more |

### Example Workflows

**Architecture decisions:**
> "Remember: we chose Observation framework over Combine for state management because it's simpler and Apple-recommended for iOS 17+."

**Bug documentation:**
> The agent fixes a Core Data migration crash? Vestige automatically stores the fix. Next time it encounters a migration issue, it remembers the solution.

**Cross-IDE memory:**
> Fix a backend bug in VS Code. Open the iOS app in Xcode. The agent already knows about the API change because Vestige shares memory across all your tools.

---

## Tips

### Use a CLAUDE.md for proactive memory

Place a `CLAUDE.md` in your project root to make the agent use Vestige automatically:

```markdown
## Memory

At the start of every session:
1. Search Vestige for this project's context
2. Recall architecture decisions and coding patterns
3. Save important decisions and bug fixes without being asked
```

See [CLAUDE.md templates](../CLAUDE-SETUP.md) for a full setup.

### Embedding model cache

The first time Vestige runs, it downloads the Nomic embedding model (~130MB). In Xcode's sandboxed environment, the cache location is:

```
~/Library/Caches/com.vestige.core/fastembed
```

If the download fails behind a corporate proxy, pre-download by running `vestige-mcp` once from your terminal before using it in Xcode.

### Skills integration

You can add Vestige-related skills to Xcode's skills directory:

```
~/Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig/skills/
```

Drop a markdown file describing how the agent should use memory for your iOS/macOS projects.

---

## Troubleshooting

<details>
<summary>"vestige" not showing in /context</summary>

1. Verify the binary path is correct and absolute:
   ```bash
   ls -la /usr/local/bin/vestige-mcp
   ```

2. Check that the config file is valid JSON:
   ```bash
   cat ~/Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig/.claude | python3 -m json.tool
   ```

3. Ensure you fully quit and restarted Xcode (Cmd+Q, not just close window).

4. Check Xcode's agent logs for errors:
   ```bash
   log show --predicate 'subsystem == "com.apple.dt.Xcode"' --last 5m | grep -i mcp
   ```
</details>

<details>
<summary>Agent can't find vestige-mcp binary</summary>

Xcode's sandbox does not inherit your shell PATH. Use the full absolute path:

```json
"command": "/usr/local/bin/vestige-mcp"
```

If you installed via `cargo build`, the binary is likely at:
```
/Users/you/.cargo/bin/vestige-mcp
```

Or wherever you copied it. Run `which vestige-mcp` in your terminal to find it.
</details>

<details>
<summary>Embedding model fails to download</summary>

The first run downloads ~130MB. If Xcode's sandbox blocks the download:

1. Run `vestige-mcp` once from your terminal to cache the model
2. The cache at `~/Library/Caches/com.vestige.core/fastembed` will be available to the sandboxed instance

Behind a proxy:
```bash
HTTPS_PROXY=your-proxy:port vestige-mcp
```
</details>

---

## Also Works With

Vestige uses the MCP standard — the same memory works across all your tools:

| IDE | Guide |
|-----|-------|
| Claude Code | [Setup](../CONFIGURATION.md#claude-code-one-liner) |
| Claude Desktop | [Setup](../CONFIGURATION.md#claude-desktop-macos) |
| Cursor | [Setup](./cursor.md) |
| VS Code (Copilot) | [Setup](./vscode.md) |
| JetBrains | [Setup](./jetbrains.md) |
| Windsurf | [Setup](./windsurf.md) |

Your AI remembers everything, everywhere.

---

<p align="center">
  <a href="../../README.md">Back to README</a>
</p>
