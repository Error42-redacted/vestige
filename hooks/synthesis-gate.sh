#!/bin/bash
# synthesis-gate.sh — optional UserPromptSubmit hook
#
# Injects a compact synthesis contract for decision-adjacent prompts. The hook
# is intentionally generic and public-safe: it does not depend on private local
# files, personal examples, or hidden project notes.

set -euo pipefail

INPUT="$(cat)"
PROMPT="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("prompt","") or d.get("user_prompt",""))' 2>/dev/null || printf '')"

DECISION_REGEX='(submit|submission|final|ship|launch|deploy|commit|decide|decision|recommend|should i|what should|purchase|buy|invest|architect|architecture|strategy|prep|prioriti|compose|tradeoff|trade-off|config|which (should|model|approach|one)|pick|choose|benchmark|competition|perform)'

if printf '%s' "$PROMPT" | /usr/bin/grep -qiE "$DECISION_REGEX"; then
  /usr/bin/python3 <<'PYEOF'
import json

msg = (
    "[VESTIGE SYNTHESIS GATE]\n\n"
    "This prompt appears decision-adjacent. If Vestige is available and relevant, use the smallest retrieval plan that can change the answer.\n\n"
    "Reasoning contract:\n"
    "1. Retrieve evidence from adjacent topics, not only the exact topic.\n"
    "2. Convert each useful memory into: fact -> implication -> action.\n"
    "3. Surface contradictions or stale memories before recommending.\n"
    "4. Do not list memory summaries as the final answer. Compose them into a concrete recommendation.\n"
    "5. If no memory materially changes the answer, say so briefly and proceed from source evidence."
)

print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": msg
    }
}))
PYEOF
fi

exit 0
