#!/bin/bash
# synthesis-stop-validator.sh — optional Stop hook
#
# Blocks a narrow failure mode: a response that cites multiple memories but
# stops at summary instead of composing them into a decision. This public-safe
# version contains no private examples or local-user paths.

set -euo pipefail

INPUT="$(cat)"
TRANSCRIPT_PATH="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("transcript_path",""))' 2>/dev/null || printf '')"

if [ -z "$TRANSCRIPT_PATH" ] || [ ! -f "$TRANSCRIPT_PATH" ]; then
  exit 0
fi

export TRANSCRIPT_PATH
PYFILE=$(mktemp -t vestige-stop-validator.XXXXXX)
trap 'rm -f "$PYFILE"' EXIT
cat > "$PYFILE" <<'PYEOF'
import json, os, re, sys

transcript = os.environ.get("TRANSCRIPT_PATH", "")
last_user = ""
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
            if role == "user":
                last_user = text
            elif role == "assistant":
                last_assistant = text
except Exception:
    sys.exit(0)

decision_re = re.compile(
    r"(submit|submission|final|ship|launch|deploy|commit|decide|decision|"
    r"recommend|should i|what should|purchase|buy|invest|architect|architecture|"
    r"strategy|prep|prioriti|compose|tradeoff|trade-off|config|which "
    r"(should|model|approach|one)|pick|choose|benchmark|competition|perform)",
    re.IGNORECASE,
)
if not decision_re.search(last_user):
    sys.exit(0)

memory_re = re.compile(
    r"(memory|vestige|recall|retriev|saved memor|stored memor|prior memor|"
    r"fsrs|trust score|deep_reference|smart_ingest)",
    re.IGNORECASE,
)
if not memory_re.search(last_assistant):
    sys.exit(0)

summary_patterns = [
    r"memory\s+[a-f0-9]{4,}",
    r"saved memory",
    r"according to memory",
    r"the memory (says|states|notes|indicates)",
    r"per memory",
    r"memories? (say|says|state|note|indicate)",
]
summary_hits = 0
for pat in summary_patterns:
    summary_hits += len(re.findall(pat, last_assistant, re.IGNORECASE))

composition_re = re.compile(
    r"(compos|combin|together|concrete action|recommend(ation)? [:\-]|"
    r"never[- ]composed|the synthesis is|therefore|so the action is)",
    re.IGNORECASE,
)
composition_hits = len(composition_re.findall(last_assistant))

if summary_hits >= 3 and composition_hits == 0:
    print("BLOCK_SUMMARY")
    sys.exit(0)

print("PASS")
PYEOF

RESULT="$(/usr/bin/python3 "$PYFILE")"

case "$RESULT" in
  BLOCK_SUMMARY)
    cat >&2 <<'BLOCKMSG'
[STOP BLOCKED — VESTIGE SYNTHESIS VALIDATOR]

The response cites multiple memories but does not compose them into a decision.
Rewrite it so the retrieved evidence becomes:

1. Evidence: the memory facts that matter.
2. Implication: what those facts change.
3. Action: the concrete recommendation.

Do not stop at "Memory A says X, Memory B says Y." Compose the evidence.
BLOCKMSG
    exit 2
    ;;
  *)
    exit 0
    ;;
esac
