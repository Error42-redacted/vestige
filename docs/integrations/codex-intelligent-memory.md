# Codex Intelligent Memory Protocol

Codex can connect to Vestige through MCP, but MCP registration alone only makes
the tools available. It does not make Codex automatically reason with memory.

Use this protocol when configuring a Codex workspace that should behave like it
has long-term cognitive memory.

## 1. Register Vestige MCP

```toml
[mcp_servers.vestige]
command = "/absolute/path/to/vestige-mcp"
```

Restart Codex after changing MCP configuration.

## 2. Add An `AGENTS.md` Trigger

Codex reads `AGENTS.md` files as workspace instructions. Put a file at the repo
root, or a higher workspace root, with a rule like:

```markdown
Before answering substantive prompts, consult Vestige using the current prompt
plus project and user context. Use `session_context` for broad context, `search`
for quick memory checks, and `deep_reference` for decisions, contradictions, or
accuracy-sensitive questions. Compose memories into actions; do not summarize
retrievals.
```

This is the Codex equivalent of the lightweight top-bread memory trigger.

## 3. Use A Query Router

Use the smallest call that can change the answer:

- `session_context`: start of a topic or project switch.
- `search`: identity, preference, exact memory, or quick project context.
- `deep_reference` / `cross_reference`: decision history, contradictions,
  timelines, or root-cause analysis.
- `memory(get_batch)`: expand specific load-bearing memories.
- `smart_ingest`: save durable corrections, decisions, or new preferences.

## 4. Compose, Do Not Summarize

Retrieved memory is evidence, not the final answer.

Use this mental transform:

```text
memory fact -> implication -> action
```

If memory does not change the action, do not mention it. If it does, make the
changed recommendation clear.

## 5. Know The Limit

Claude Code's Cognitive Sandwich can use `UserPromptSubmit` and `Stop` hooks to
wrap every response. Codex may expose different hook events depending on version.
Do not assume Claude's hook chain is active in Codex just because Vestige MCP is
registered.

For Codex, the reliable portable layer is:

1. MCP server configured.
2. `AGENTS.md` instruction trigger.
3. Local Codex rule docs.
4. Explicit agent discipline: call Vestige before substantive answers.

If a future Codex version supports a stable pre-prompt hook, wire that hook to
inject a short Vestige reminder or context packet before the model answers.
