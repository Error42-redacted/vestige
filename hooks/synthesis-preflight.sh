#!/bin/bash
# synthesis-preflight.sh — UserPromptSubmit hook (v2: full content injection)
#
# Injects memory content directly via /api/deep_reference so relevant evidence
# is available before Claude drafts a decision-adjacent response.
#
# On every UserPromptSubmit:
#   1. Read JSON stdin, extract user prompt
#   2. Decision-keyword gate (preserved from v1)
#   3. POST the prompt to vestige-mcp /api/deep_reference (single call)
#      — returns recommended memory + reasoning chain + trust-scored evidence
#   4. Inject reasoning + recommended preview + top 3 evidence previews as
#      additionalContext, with explicit "DO NOT IGNORE" framing
#
# Fails open: if vestige-mcp is not running or HTTP request fails, hook
# emits empty context and exit 0. Prompt still proceeds. Never blocks.
#
# Endpoint: POST http://127.0.0.1:3927/api/deep_reference
#   body: {"query": "<prompt>", "depth": 15}
#   resp: {confidence, evidence:[{id, preview, role, trust, date}], reasoning, recommended}

set -u

INPUT="$(cat)"

# Extract prompt from JSON stdin. Fall back to empty if parse fails.
PROMPT="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("prompt",""))' 2>/dev/null || printf '')"

if [ -z "$PROMPT" ]; then
  exit 0
fi

# Decision-keyword gate (preserved from v1). Mirrors synthesis-gate.sh.
DECISION_GATE_RE='submit|submission|benchmark|competition|final|ship|launch|deploy|commit|decide|decision|recommend|should i|should we|what should|purchase|buy|invest|architect|architecture|strategy|prep|prioriti|compose|tradeoff|trade-off|config|which|pick|choose|pitch|forecast|target|plan|roadmap|v2\.|v3\.|scale|grow|growth|distrib|brand|position|moat|vs\.|vs\b|instead of'

if ! printf '%s' "$PROMPT" | LC_ALL=C /usr/bin/grep -iqE "$DECISION_GATE_RE"; then
  exit 0
fi

PORT="${VESTIGE_DASHBOARD_PORT:-3927}"
BASE="http://127.0.0.1:${PORT}"

# Probe dashboard. Fail open if unreachable.
if ! /usr/bin/curl -fsS -m 0.5 "${BASE}/api/health" > /dev/null 2>&1; then
  exit 0
fi

# Build the deep_reference POST body via python3 (avoids shell-escape issues
# with arbitrary prompt characters).
BODY_SCRIPT="$(mktemp -t vestige-preflight-body.XXXXXX)"
trap 'rm -f "$BODY_SCRIPT"' EXIT
cat > "$BODY_SCRIPT" <<'BODY_PYEOF'
import json, os, sys
prompt = os.environ.get("VPRE_PROMPT", "")
# Truncate very long prompts so the deep_reference embedding stays focused.
# 1500 chars is enough signal for hybrid+semantic retrieval without diluting.
if len(prompt) > 1500:
    prompt = prompt[:1500]
print(json.dumps({"query": prompt, "depth": 15}))
BODY_PYEOF

export VPRE_PROMPT="$PROMPT"
DR_BODY="$(/usr/bin/python3 "$BODY_SCRIPT")"

# Single POST to deep_reference. Timeout 5s — deep_reference takes ~1-3s.
DR_RESP="$(/usr/bin/curl -fsS -m 5 -X POST "${BASE}/api/deep_reference" \
  -H 'Content-Type: application/json' \
  -d "$DR_BODY" 2>/dev/null || printf '')"

if [ -z "$DR_RESP" ]; then
  exit 0
fi

# Compose response into additionalContext block. Inject:
#   - confidence
#   - reasoning chain (if present)
#   - recommended memory id + full preview
#   - top 3 evidence with role, trust, preview
COMPOSE_SCRIPT="$(mktemp -t vestige-preflight-compose.XXXXXX)"
trap 'rm -f "$BODY_SCRIPT" "$COMPOSE_SCRIPT"' EXIT
cat > "$COMPOSE_SCRIPT" <<'COMPOSE_PYEOF'
import json, os, sys

raw = os.environ.get("VPRE_DR_RESP", "")
try:
    d = json.loads(raw)
except Exception:
    print("")
    sys.exit(0)

if not isinstance(d, dict):
    print("")
    sys.exit(0)

confidence = d.get("confidence", 0)
intent = d.get("intent", "")
reasoning = (d.get("reasoning") or "").strip()
recommended = d.get("recommended") or {}
evidence = d.get("evidence") or []

# Skip injection if confidence is rock-bottom — likely no relevant memories.
if not evidence and not recommended:
    print("")
    sys.exit(0)

out = []
out.append("[VESTIGE PREFLIGHT — deep_reference auto-injected, DO NOT IGNORE]")
out.append(f"Intent: {intent or 'Synthesis'} | Confidence: {int(confidence*100)}%")
out.append("")

if reasoning:
    out.append("REASONING CHAIN (pre-built by Vestige FSRS-6 trust scoring):")
    # Trim to 1200 chars max to keep context budget reasonable
    rs = reasoning[:1200]
    out.append(rs)
    if len(reasoning) > 1200:
        out.append(f"  ...[reasoning truncated, full chain {len(reasoning)} chars]")
    out.append("")

if recommended:
    rec_id = (recommended.get("memory_id") or recommended.get("id") or "")[:8]
    rec_trust = recommended.get("trust_score", 0)
    rec_date = (recommended.get("date") or "")[:10]
    rec_preview = recommended.get("answer_preview") or recommended.get("preview") or ""
    out.append(f"RECOMMENDED MEMORY [{rec_id}] trust={rec_trust:.2f} date={rec_date}:")
    out.append(rec_preview[:600])
    out.append("")

if evidence:
    out.append(f"TOP {min(len(evidence), 4)} EVIDENCE:")
    for e in evidence[:4]:
        eid = (e.get("id") or "")[:8]
        role = e.get("role", "?")
        trust = e.get("trust", 0)
        date = (e.get("date") or "")[:10]
        preview = (e.get("preview") or "").strip()
        out.append(f"  [{eid}] role={role} trust={trust:.2f} date={date}")
        # 350 chars per evidence preview keeps total injection ~2-3KB
        out.append(f"    {preview[:350]}")
    out.append("")

out.append("ENFORCEMENT: Compose these into your response, do NOT summarize.")
out.append("Use mcp__vestige__memory(action='get', id=...) to expand any preview.")
out.append("Required shape: (a) Composing: [memories] - logic. (b) Never-composed: [combos|None].")
out.append("(c) Recommendation: the user should DO [concrete action].")

print("\n".join(out))
COMPOSE_PYEOF

export VPRE_DR_RESP="$DR_RESP"
SYNTHESIS="$(/usr/bin/python3 "$COMPOSE_SCRIPT")"

if [ -z "$SYNTHESIS" ]; then
  exit 0
fi

# Emit as JSON additionalContext via env var
EMIT_SCRIPT="$(mktemp -t vestige-preflight-emit.XXXXXX)"
trap 'rm -f "$BODY_SCRIPT" "$COMPOSE_SCRIPT" "$EMIT_SCRIPT"' EXIT
cat > "$EMIT_SCRIPT" <<'EMIT_PYEOF'
import json, os
ctx = os.environ.get("VPRE_SYNTHESIS_CTX", "")
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": ctx
    }
}))
EMIT_PYEOF
export VPRE_SYNTHESIS_CTX="$SYNTHESIS"
/usr/bin/python3 "$EMIT_SCRIPT"
exit 0
