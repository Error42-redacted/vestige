#!/bin/bash
# vestige-pulse-daemon.sh — UserPromptSubmit hook for recent Vestige insights
#
# HOOK #2 of the 2026-04-20 upgrade: v2.2 PULSE AT THE CLAUDE-CODE LAYER.
#
# This hook polls the vestige-mcp event changelog at
# http://127.0.0.1:3927/api/changelog and watches for DreamCompleted or
# ConnectionDiscovered events with meaningful insight payloads. When one fires,
# it prints context to stdout and exits 0. Claude Code injects UserPromptSubmit
# stdout into the next turn's context.
#
# Rate limit: fires at most once per 20 minutes per session to avoid
# interrupting flow state during focused work.
#
# The effect: fresh Vestige dream/connection events can reach Claude before it
# answers the next prompt, without blocking the user or requiring a manual MCP
# call first.
#
# Fails open: if vestige-mcp is not running or the dashboard API is unavailable,
# exits 0 silently. Never blocks Claude.

set -u

# State files for rate limiting
STATE_DIR="${VESTIGE_PULSE_STATE_DIR:-/tmp/vestige-pulse-daemon}"
mkdir -p "$STATE_DIR"
LAST_FIRE_FILE="$STATE_DIR/last_fire"
SESSION_ID_FILE="$STATE_DIR/session_id"

INPUT="$(cat)"
SESSION_ID="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id",""))' 2>/dev/null || printf '')"

# Rate limit: 20 minutes between fires per session
MIN_INTERVAL_SEC=1200
NOW=$(date +%s)

if [ -f "$LAST_FIRE_FILE" ]; then
  LAST_FIRE=$(cat "$LAST_FIRE_FILE" 2>/dev/null || echo 0)
  LAST_SESSION=$(cat "$SESSION_ID_FILE" 2>/dev/null || echo "")
  # Only rate-limit within the same session
  if [ "$LAST_SESSION" = "$SESSION_ID" ] && [ $((NOW - LAST_FIRE)) -lt $MIN_INTERVAL_SEC ]; then
    exit 0
  fi
fi

PORT="${VESTIGE_DASHBOARD_PORT:-3927}"

# Probe health before polling the changelog
if ! /usr/bin/curl -fsS -m 0.5 "http://127.0.0.1:${PORT}/api/health" > /dev/null 2>&1; then
  exit 0
fi

# Check recent events via the REST changelog API for DreamCompleted in the
# last 15 minutes. This is simpler than a full WebSocket subscription and
# works with UserPromptSubmit semantics (inject once per prompt, not persistent).
# If a DreamCompleted event with insights_generated > 0 is found, inject context.
RESULT="$(/usr/bin/curl -fsS -m 2 \
  "http://127.0.0.1:${PORT}/api/changelog?start=$(date -u -v-15M +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d '15 minutes ago' +%Y-%m-%dT%H:%M:%SZ)&limit=50" \
  2>/dev/null || printf '')"

if [ -z "$RESULT" ]; then
  exit 0
fi

INSIGHT="$(VESTIGE_CHANGELOG_JSON="$RESULT" /usr/bin/python3 <<'PYEOF'
import json, os, sys


def as_int(value):
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0

try:
    data = json.loads(os.environ.get("VESTIGE_CHANGELOG_JSON", ""))
except Exception:
    sys.exit(0)

if not isinstance(data, dict):
    sys.exit(0)

events = data.get("events", []) or []

# Find the most recent DreamCompleted with insights_generated > 0
# OR ConnectionDiscovered with a meaningful target
for ev in events:
    if not isinstance(ev, dict):
        continue
    etype = ev.get("type", "")
    payload = ev.get("data")
    if not isinstance(payload, dict):
        payload = ev

    if etype in ("DreamCompleted", "dream", "consolidation"):
        insights = as_int(payload.get("insights_generated") or payload.get("insightsGenerated"))
        if insights > 0:
            stats = payload.get("stats") or {}
            connections = as_int(
                payload.get("connections_persisted")
                or payload.get("connectionsPersisted")
                or payload.get("connections_found")
                or payload.get("connectionsFound")
                or payload.get("connectionFound")
                or stats.get("connections")
            )
            print(f"DREAM: {insights} insights, {connections} new connections. Dream cycle completed while you were working. Consider calling mcp__vestige__dream(memory_count=50) to inspect the fresh cluster bridges; or mcp__vestige__explore_connections(action='bridges') on the latest activity.")
            sys.exit(0)
    if etype in ("ConnectionDiscovered", "connection"):
        src = str(payload.get("source_id") or payload.get("sourceId") or payload.get("source") or "")[:8]
        tgt = str(payload.get("target_id") or payload.get("targetId") or payload.get("target") or "")[:8]
        if src and tgt:
            print(f"CONNECTION: Vestige discovered a new edge [{src}] <-> [{tgt}] while you were working. Spreading activation surfaced a bridge you had not queried. Inspect via mcp__vestige__explore_connections(action='bridges', from='{src}', to='{tgt}').")
            sys.exit(0)
PYEOF
)"

if [ -z "$INSIGHT" ]; then
  exit 0
fi

# Update rate-limit state
echo "$NOW" > "$LAST_FIRE_FILE"
echo "$SESSION_ID" > "$SESSION_ID_FILE"

# UserPromptSubmit stdout is injected into Claude's context. Do not use exit 2
# here: Claude Code treats that as a blocking prompt validation failure.
cat <<PULSEMSG
[VESTIGE PULSE — autonomous insight from the cognitive engine]

$INSIGHT

This context was injected because Vestige generated a fresh insight while the session was active. Mention it naturally if it is relevant to the user's current prompt.

Rate-limited to 1 pulse per 20 minutes per session. See ~/.claude/hooks/vestige-pulse-daemon.sh.
PULSEMSG
exit 0
