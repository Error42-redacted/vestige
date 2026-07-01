#!/bin/bash
# veto-detector.sh — Stop hook (Hallucination Guillotine POC)
#
# Fires AFTER synthesis-stop-validator.sh. Queries vestige-mcp dashboard API
# for memories tagged veto-pattern / deprecated-pattern / suppressed, then
# checks if the last assistant draft contains any of their trigger phrases.
# On match: exit 2 with a VESTIGE VETO stderr message that wakes Claude and
# forces a rewrite.
#
# This is the command-type proof-of-concept of the agent-type Hallucination
# Guillotine (Integration #1 in Vestige memory 3c4bd820). Full agent-type
# version uses a subagent to call deep_reference on extracted claims and do
# real contradiction analysis; this version pattern-matches against curated
# veto memories stored in Vestige.
#
# Fails open: if vestige-mcp is not running or no veto memories exist,
# exits 0 silently. Never blocks on infrastructure errors.

set -u

INPUT="$(cat)"
TRANSCRIPT_PATH="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("transcript_path",""))' 2>/dev/null || printf '')"

if [ -z "$TRANSCRIPT_PATH" ] || [ ! -f "$TRANSCRIPT_PATH" ]; then
  exit 0
fi

PORT="${VESTIGE_DASHBOARD_PORT:-3927}"
if ! /usr/bin/curl -fsS -m 0.5 "http://127.0.0.1:${PORT}/api/health" > /dev/null 2>&1; then
  exit 0
fi

# Fetch memories tagged veto-pattern. The /api/memories?tag=<tag> filter
# returns exactly the tag-matching rows (verified 2026-04-20). Keyword
# search (/api/memories?q=...) is semantic so it misses literal "VETO" hits.
VETO_JSON="$(/usr/bin/curl -fsS -m 2 "http://127.0.0.1:${PORT}/api/memories?tag=veto-pattern&limit=50" 2>/dev/null || printf '')"

if [ -z "$VETO_JSON" ]; then
  exit 0
fi

export TRANSCRIPT_PATH
export VETO_JSON

VETO_SCRIPT="$(mktemp -t vestige-veto.XXXXXX)"
trap 'rm -f "$VETO_SCRIPT"' EXIT
cat > "$VETO_SCRIPT" <<'VETO_PYEOF'
import json, os, re, sys

transcript = os.environ.get("TRANSCRIPT_PATH", "")
veto_json = os.environ.get("VETO_JSON", "")

if not transcript or not veto_json:
    sys.exit(0)

# Parse veto memories. Filter to those tagged veto-pattern, deprecated-pattern,
# or suppressed, and extract a VETO_TRIGGER phrase from the content if present.
try:
    vdata = json.loads(veto_json)
except Exception:
    sys.exit(0)

veto_memories = []
for m in vdata.get("memories", []) or []:
    tags = set((m.get("tags") or []))
    if not (tags & {"veto-pattern", "deprecated-pattern", "suppressed"}):
        continue
    content = m.get("content") or ""
    # Look for a "VETO_TRIGGER:" or "TRIGGER PHRASE:" line
    triggers = re.findall(r"(?:VETO_TRIGGER|TRIGGER PHRASE|TRIGGER):\s*(.+?)(?:\n|$)", content)
    for t in triggers:
        t = t.strip().strip("`\"' ")
        if t and len(t) >= 3:
            veto_memories.append({
                "id": m.get("id", "?"),
                "trigger": t,
                "content": content[:300],
                "retention": m.get("retentionStrength", 0),
            })

if not veto_memories:
    sys.exit(0)

# Read last assistant message from transcript JSONL
last_assistant = ""
try:
    with open(transcript) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except Exception:
                continue
            role = obj.get("role") or obj.get("type", "")
            content = obj.get("message", {}).get("content", obj.get("content", ""))
            text = ""
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        text += block.get("text", "") + "\n"
            elif isinstance(content, str):
                text = content
            if role == "assistant":
                last_assistant = text
except Exception:
    sys.exit(0)

if not last_assistant:
    sys.exit(0)

# Check each veto trigger against the assistant draft. Only treat high-retention
# memories (>= 0.5) as load-bearing to avoid false positives on decayed content.
hits = []
for v in veto_memories:
    if v["retention"] < 0.5:
        continue
    trig = v["trigger"]
    # Case-insensitive substring match with word-boundary preference
    if re.search(r"(?i)" + re.escape(trig), last_assistant):
        hits.append(v)

if not hits:
    sys.exit(0)

# Emit the VESTIGE VETO message. Newest/highest-retention hit leads.
hits.sort(key=lambda x: x["retention"], reverse=True)
top = hits[0]
nid = top["id"][:8] if len(top["id"]) >= 8 else top["id"]
trigger = top["trigger"]
retention_pct = int(top["retention"] * 100)

print(f"VETO_HIT:{nid}:{trigger}:{retention_pct}")
VETO_PYEOF

RESULT="$(/usr/bin/python3 "$VETO_SCRIPT" 2>/dev/null || printf '')"

if [ -z "$RESULT" ]; then
  exit 0
fi

# Parse the result
NODE_ID="$(printf '%s' "$RESULT" | /usr/bin/awk -F: '{print $2}')"
TRIGGER="$(printf '%s' "$RESULT" | /usr/bin/awk -F: '{print $3}')"
RETENTION="$(printf '%s' "$RESULT" | /usr/bin/awk -F: '{print $4}')"

cat >&2 <<VETO_MSG
[VESTIGE VETO — synthesis-composer subagent rejected draft]

Contradicts suppressed memory node #${NODE_ID} (trust ${RETENTION}%).
Trigger phrase detected: "${TRIGGER}"

The draft response contains a pattern that Vestige has explicitly marked as
deprecated, suppressed, or contradicted by higher-trust memories. You may NOT
output this response.

Rewrite WITHOUT the flagged pattern. Cite the correct replacement pattern by
querying mcp__vestige__memory(action='get', id='${NODE_ID}') to see the
suppression context and the replacement guidance.

This is the command-type proof-of-concept of the Hallucination Guillotine
(Integration #1, Vestige memory 3c4bd820). Full agent-type version with
deep_reference contradiction analysis ships later this week.
VETO_MSG

exit 2
