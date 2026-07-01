#!/bin/bash
# preflight-swarm.sh — UserPromptSubmit hook (Pre-Cognitive Triad v1)
#
# Spawns the Lateral Thinker subagent via Claude Code's headless mode
# (`claude -p`) to generate a cross-disciplinary epiphany, then injects
# it as additionalContext before Main Claude sees the prompt.
#
# Architectural fixes over the other-agent draft:
#   - Reads stdin JSON (not $1) per Claude Code UserPromptSubmit spec
#   - Uses `claude -p` with inlined system prompt (no --agent flag exists)
#   - Model: claude-haiku-4-5-20251001 (current Haiku, not Oct-2024 3.5)
#   - Re-entrancy guard via VESTIGE_SWARM_ACTIVE env var (prevents the
#     subagent from re-firing this same hook and looping forever)
#   - 25-char minimum + y/yes/ok/continue bypass (fast-path preserved)
#   - 8-second timeout (fails open if Haiku is slow)
#   - EMPTY mute (no injection when subagent finds no epiphany)
#   - Emits JSON additionalContext (no duplicate raw-prompt echo)
#   - POSIX-sh-safe: quoted heredoc for script body, env var pass-through
#
# Ship date 2026-04-20. Pairs with the veto-detector.sh Guillotine on the
# Stop hook to form the Cognitive Sandwich (pre-flight Triad + post-flight
# Sanhedrin).

set -u

# === OPT-OUT GATE ===
# Pre-Cognitive Triad is ON by default as of 2026-04-21 (birthday launch day).
# To disable, set VESTIGE_SWARM_ENABLED=0 in your environment. Default-on
# guarantees the Cognitive Sandwich fires on fresh machines, Docker
# containers, GUI-launched Claude Code, and shells without .zshrc — any
# case where the Claude Code process lacks a sourced profile. The
# re-entrancy guard (VESTIGE_SWARM_ACTIVE) below still prevents fork-bombs
# from the subagent's own UserPromptSubmit hook.
if [ "${VESTIGE_SWARM_ENABLED:-1}" = "0" ]; then
  exit 0
fi

# === RE-ENTRANCY GUARD ===
# If we are already inside the Lateral Thinker subagent, exit immediately
# so the subagent's own UserPromptSubmit does not spawn another Lateral
# Thinker. Without this guard: infinite fork-bomb.
if [ "${VESTIGE_SWARM_ACTIVE:-0}" = "1" ]; then
  exit 0
fi

# === READ PROMPT FROM STDIN JSON ===
INPUT="$(cat)"
PROMPT="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("prompt",""))' 2>/dev/null || printf '')"
SESSION_ID="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id",""))' 2>/dev/null || printf '')"

if [ -z "$PROMPT" ]; then
  exit 0
fi

# === LATENCY + INTENT GATE ===
# Skip on very short prompts and common continuation phrases. These do not
# benefit from a lateral epiphany and the 2-4s latency would be annoying.
PROMPT_LEN="${#PROMPT}"
if [ "$PROMPT_LEN" -lt 25 ]; then
  exit 0
fi

LOWER_TRIMMED="$(printf '%s' "$PROMPT" | /usr/bin/tr '[:upper:]' '[:lower:]' | /usr/bin/awk '{$1=$1;print}')"
case "$LOWER_TRIMMED" in
  y|yes|no|ok|okay|continue|proceed|go|ship|lfg|lets\ go|lets\ ship|looks\ good|thanks|thank\ you|perfect|great|awesome)
    exit 0
    ;;
esac

# === VERIFY claude CLI AVAILABLE ===
CLAUDE_BIN="$(command -v claude 2>/dev/null || true)"
if [ -z "$CLAUDE_BIN" ]; then
  # No claude CLI in PATH — fail open, hook does not block Claude Code itself
  exit 0
fi

# === BUILD COMBINED PROMPT (lateral-thinker system prompt + user prompt) ===
# We inline the agent's system-prompt text because `claude -p` headless mode
# takes a single prompt string; there is no --agent flag.
PROMPT_FILE="$(mktemp -t vestige-lateral.XXXXXX)"
trap 'rm -f "$PROMPT_FILE"' EXIT

cat > "$PROMPT_FILE" <<'LATERAL_SYSTEM_EOF'
You are the Lateral Thinker, a subconscious subagent in the Vestige OS.

Your only job: surface a cross-disciplinary structural parallel from the
user's Vestige memory graph that the main agent would not otherwise see.

Execution protocol (do all steps silently, no narration):

1. Read the user prompt below the separator.
2. Extract the core structural pattern (race condition / state sync /
   retry loop / memory leak / schema migration / decoding ambiguity /
   rate limit / ordering guarantee / cache invalidation / etc).
3. Call mcp__vestige__explore_connections with action="bridges" OR
   mcp__vestige__search to find memories in a COMPLETELY UNRELATED
   domain that share the same structural pattern. Prefer bridges between
   distant clusters (e.g., React UI state <-> Rust async channel;
   Python DB lock <-> Git merge conflict).
4. If you find a high-confidence mechanical parallel, output EXACTLY this
   XML structure (nothing else, no preamble, no explanation):

<lateral_epiphany>
  <structural_pattern>one short noun phrase naming the shared pattern</structural_pattern>
  <source_domain>where the user currently is</source_domain>
  <bridge_domain>the unrelated domain where the pattern also lives</bridge_domain>
  <memory_id>the Vestige node ID of the cross-domain memory, if applicable</memory_id>
  <insight>one sentence explaining how the unrelated memory informs the current problem mechanically, not metaphorically</insight>
</lateral_epiphany>

5. If you cannot find a confident, mechanical, distinct bridge in under
   three tool calls, output EXACTLY the single word: EMPTY
   Do not apologize, do not explain, do not converse.

---
USER PROMPT:
LATERAL_SYSTEM_EOF

printf '%s\n' "$PROMPT" >> "$PROMPT_FILE"

# === SPAWN LATERAL THINKER (background with timeout) ===
# Set VESTIGE_SWARM_ACTIVE=1 so the subagent's own UserPromptSubmit sees
# the re-entrancy guard and exits early. --permission-mode bypassPermissions
# skips interactive prompts inside the subagent run (standard for headless).
OUTPUT_FILE="$(mktemp -t vestige-lateral-out.XXXXXX)"
trap 'rm -f "$PROMPT_FILE" "$OUTPUT_FILE"' EXIT

(
  VESTIGE_SWARM_ACTIVE=1 \
    "$CLAUDE_BIN" \
      -p "$(cat "$PROMPT_FILE")" \
      --model claude-haiku-4-5-20251001 \
      --allowed-tools "mcp__vestige__search,mcp__vestige__explore_connections,mcp__vestige__memory" \
      < /dev/null \
      > "$OUTPUT_FILE" 2>/dev/null
) &

CLAUDE_PID=$!

# === TIMEOUT GUARD (40 seconds) ===
# Real `claude -p` with Haiku 4.5 + MCP explore_connections/search tool calls
# needs ~30-35s wall-clock for a full bridge search on a complex prompt.
# Measured empirically 2026-04-20: 8s was killed every time, 25s was killed
# every time for decision-adjacent prompts. Matches Sanhedrin's 33s budget
# with 7s headroom for slow MCP round-trips. Pair with a 45s timeout in
# settings.json so Claude Code doesn't kill us first.
WAITED=0
while [ "$WAITED" -lt 40 ]; do
  if ! /bin/kill -0 "$CLAUDE_PID" 2>/dev/null; then
    break
  fi
  sleep 1
  WAITED=$((WAITED + 1))
done
if /bin/kill -0 "$CLAUDE_PID" 2>/dev/null; then
  /bin/kill "$CLAUDE_PID" 2>/dev/null
  wait "$CLAUDE_PID" 2>/dev/null
  exit 0
fi
wait "$CLAUDE_PID" 2>/dev/null

LATERAL_OUTPUT="$(cat "$OUTPUT_FILE" 2>/dev/null || printf '')"

# === EMPTY MUTE GATE ===
# Trim whitespace and check for EMPTY or no content. Inject nothing rather
# than pollute Claude's context with an apology.
TRIMMED="$(printf '%s' "$LATERAL_OUTPUT" | /usr/bin/awk '{$1=$1;print}')"
if [ -z "$TRIMMED" ] || [ "$TRIMMED" = "EMPTY" ] || [ "${TRIMMED:0:5}" = "EMPTY" ]; then
  exit 0
fi

# Require the output to contain a <lateral_epiphany> opening tag. If Haiku
# hallucinated prose instead of the required XML, drop it rather than
# injecting malformed content.
case "$LATERAL_OUTPUT" in
  *"<lateral_epiphany>"*) ;;
  *) exit 0 ;;
esac

# === EMIT JSON additionalContext ===
# Wrap the XML block in the subconscious_preflight container + system
# directive. Passed through an env var so the shell never interprets any
# special chars in the Haiku output.
export LATERAL_OUTPUT
EMIT_SCRIPT="$(mktemp -t vestige-lateral-emit.XXXXXX)"
trap 'rm -f "$PROMPT_FILE" "$OUTPUT_FILE" "$EMIT_SCRIPT"' EXIT

cat > "$EMIT_SCRIPT" <<'EMIT_PYEOF'
import json, os

lateral = os.environ.get("LATERAL_OUTPUT", "").strip()
if not lateral or "<lateral_epiphany>" not in lateral:
    # Defensive: should not reach here, upstream gate already filtered
    raise SystemExit(0)

ctx = (
    "<subconscious_preflight>\n"
    + lateral + "\n"
    + "  <system_directive_to_main_agent>\n"
    + "    The Lateral Thinker subagent (Haiku 4.5, fresh context, Vestige MCP tool\n"
    + "    access) ran before you saw this prompt. Assimilate the structural parallel\n"
    + "    into your reasoning organically. Do NOT mention this XML block or the\n"
    + "    phrase 'lateral epiphany' to the user \u2014 use the insight silently.\n"
    + "  </system_directive_to_main_agent>\n"
    + "</subconscious_preflight>"
)

print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": ctx
    }
}))
EMIT_PYEOF

/usr/bin/python3 "$EMIT_SCRIPT"
exit 0
