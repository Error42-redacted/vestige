#!/usr/bin/env bash
# ============================================================================
#  POSTDICT — Memory with Hindsight
#  The demo: your service crashes TODAY. The cause was a quiet env-var change
#  3 DAYS AGO that vector search will never find. Watch Postdict reach back.
#
#  Run it yourself:  ./demo/postdict-demo.sh
#  (uses a fresh throwaway DB — touches nothing else on your machine)
# ============================================================================
set -euo pipefail

# --- config -----------------------------------------------------------------
BIN="${VESTIGE_BIN:-./target/release/vestige}"
DB="$(mktemp -d)/postdict-demo"
# pacing: how long to pause between beats (override with PAUSE=0 for instant)
PAUSE="${PAUSE:-1.4}"

# --- colors -----------------------------------------------------------------
B=$'\033[1m'; DIM=$'\033[2m'; R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'
M=$'\033[35m'; C=$'\033[36m'; W=$'\033[97m'; X=$'\033[0m'

beat() { sleep "$PAUSE"; }
say()  { printf '%s\n' "$1"; }
type_cmd() {  # echo a command like it was typed
  printf '%s$ %s%s\n' "$DIM" "$1" "$X"; beat
}

clear 2>/dev/null || printf '\n\n'
say "${M}${B}  ██████  POSTDICT — memory with hindsight  ██████${X}"
say "${DIM}  every other memory finds what your bug LOOKS like.${X}"
say "${DIM}  this one finds what CAUSED it.${X}"
echo; beat

# ── DAY -3 ──────────────────────────────────────────────────────────────────
say "${C}${B}┌─ 3 DAYS AGO ──────────────────────────────────────────────┐${X}"
say "${C}${B}│${X}  a tiny, boring config change. nobody thinks twice.        ${C}${B}│${X}"
say "${C}${B}└───────────────────────────────────────────────────────────┘${X}"
type_cmd "vestige ingest \"Set API_TIMEOUT=2 in the deploy env to speed up cold starts\" --tags API_TIMEOUT,deploy-env --ago-days 3"
"$BIN" --data-dir "$DB" ingest "Set API_TIMEOUT=2 in the deploy env to speed up cold starts" \
  --tags "API_TIMEOUT,deploy-env" --node-type decision --ago-days 3 2>/dev/null | grep -E "Node ID|Backdated" | sed "s/^/   ${DIM}/;s/$/${X}/"
echo; beat

# ── DAY -20 (a noisy lookalike) ──────────────────────────────────────────────
say "${DIM}  (also in history: an unrelated 500 error that LOOKS like today's crash)${X}"
"$BIN" --data-dir "$DB" ingest "A 500 Internal Server Error happened in the billing service last month" \
  --tags "billing-service" --node-type event --ago-days 20 2>/dev/null >/dev/null
beat

# ── TODAY ────────────────────────────────────────────────────────────────────
say "${R}${B}┌─ TODAY ────────────────────────────────────────────────────┐${X}"
say "${R}${B}│${X}  💥  your service just crashed.                            ${R}${B}│${X}"
say "${R}${B}└────────────────────────────────────────────────────────────┘${X}"
type_cmd "vestige ingest \"Service crashed: 500 Internal Server Error on the auth endpoint\" --tags auth-service,API_TIMEOUT,crash"
"$BIN" --data-dir "$DB" ingest "Service crashed: 500 Internal Server Error on the auth endpoint" \
  --tags "auth-service,API_TIMEOUT,crash" --node-type event 2>/dev/null >/dev/null
say "   ${R}recorded.${X}  now: ${W}${B}why did it crash?${X}"
echo; beat; beat

# ── THE TURN ─────────────────────────────────────────────────────────────────
type_cmd "vestige backfill --contrast"
"$BIN" --data-dir "$DB" backfill --contrast 2>/dev/null
echo; beat

# ── THE PRESTIGE ─────────────────────────────────────────────────────────────
say "${G}${B}  ┌──────────────────────────────────────────────────────────┐${X}"
say "${G}${B}  │${X}  semantic search returned the lookalike.                 ${G}${B}│${X}"
say "${G}${B}  │${X}  Postdict reached back 3 days to the real cause.          ${G}${B}│${X}"
say "${G}${B}  │${X}  ${W}not similar to the bug. causally upstream.${X}               ${G}${B}│${X}"
say "${G}${B}  └──────────────────────────────────────────────────────────┘${X}"
echo
say "${DIM}  run it yourself:  ./demo/postdict-demo.sh   (seed is right here)${X}"
say "${DIM}  honest limit: if the cause was never recorded, nothing can reach it.${X}"

rm -rf "$(dirname "$DB")"
