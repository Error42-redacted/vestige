#!/bin/bash
# Load ALL memory MD files on every UserPromptSubmit.
# This legacy opt-in hook cats every file in the memory directory into prompt
# context. It is intentionally not enabled by default.

# Resolve per-user Claude Code project memory dir from $HOME.
# Claude Code encodes home path as `-Users-<name>`; allow override via env.
if [ -n "${VESTIGE_MEMORY_DIR:-}" ]; then
  MEM_DIR="$VESTIGE_MEMORY_DIR"
else
  MEM_DIR="$HOME/.claude/projects/$(printf '%s' "$HOME" | tr '/' '-')/memory"
fi

if [ ! -d "$MEM_DIR" ]; then
  exit 0
fi

echo "═══════════════════════════════════════════════════════════════"
echo "[FULL MEMORY DUMP — EVERY FILE LOADED]"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Iterate every .md file in the memory directory (not archive)
for f in "$MEM_DIR"/*.md; do
  if [ -f "$f" ]; then
    filename=$(basename "$f")
    echo ""
    echo "┌─────────────────────────────────────────────────────────────"
    echo "│ FILE: $filename"
    echo "└─────────────────────────────────────────────────────────────"
    cat "$f"
    echo ""
  fi
done

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "[END FULL MEMORY DUMP — $(ls "$MEM_DIR"/*.md 2>/dev/null | wc -l | tr -d ' ') files loaded]"
echo "═══════════════════════════════════════════════════════════════"
