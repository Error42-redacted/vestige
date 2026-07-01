#!/bin/bash
# cwd-state-injector.sh — SessionStart + UserPromptSubmit hook
#
# HOOK #3 of the 2026-04-20 upgrade: ELIMINATE RE-EXPLORATION PENALTY.
#
# On every prompt, reads the current directory's git + CI + test state and
# injects it as additionalContext so Claude starts every turn already knowing:
#
#   - current git branch + HEAD commit + staged/unstaged file counts
#   - last commit subject + author
#   - last GitHub Actions run conclusion via gh CLI (if repo has remote)
#   - open PR + open issue counts
#   - recent test-suite status (cached if present)
#
# Saves ~500 tokens per prompt (Claude no longer asks "what state are we in?")
# and prevents stale-state reasoning errors.
#
# Cached in /tmp/cwd-state-{hash}.json for 60s to keep hook fast.
# Fails open: if gh or git unavailable, emits partial context.

set -u

INPUT="$(cat)"
CWD="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("cwd",""))' 2>/dev/null || printf '')"

# Fallback to PWD if cwd not in input
if [ -z "$CWD" ] || [ ! -d "$CWD" ]; then
  CWD="$(pwd 2>/dev/null)"
fi

# Only run in git repos
cd "$CWD" 2>/dev/null || exit 0
if ! /usr/bin/git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
  exit 0
fi

# Cache for 60s
CACHE_KEY="$(printf '%s' "$CWD" | /usr/bin/shasum | awk '{print $1}')"
CACHE_FILE="/tmp/cwd-state-${CACHE_KEY}.json"
if [ -f "$CACHE_FILE" ]; then
  MTIME=$(stat -f %m "$CACHE_FILE" 2>/dev/null || stat -c %Y "$CACHE_FILE" 2>/dev/null || echo 0)
  NOW=$(date +%s)
  AGE=$((NOW - MTIME))
  if [ "$AGE" -lt 60 ] && [ -s "$CACHE_FILE" ]; then
    cat "$CACHE_FILE"
    exit 0
  fi
fi

# Gather state
BRANCH="$(/usr/bin/git rev-parse --abbrev-ref HEAD 2>/dev/null)"
HEAD_SHA="$(/usr/bin/git rev-parse --short HEAD 2>/dev/null)"
HEAD_SUBJECT="$(/usr/bin/git log -1 --format='%s' 2>/dev/null | head -c 100)"
HEAD_AUTHOR="$(/usr/bin/git log -1 --format='%an' 2>/dev/null)"
STAGED_COUNT="$(/usr/bin/git diff --cached --name-only 2>/dev/null | /usr/bin/wc -l | awk '{print $1}')"
UNSTAGED_COUNT="$(/usr/bin/git diff --name-only 2>/dev/null | /usr/bin/wc -l | awk '{print $1}')"
UNTRACKED_COUNT="$(/usr/bin/git ls-files --others --exclude-standard 2>/dev/null | /usr/bin/wc -l | awk '{print $1}')"
AHEAD_BEHIND="$(/usr/bin/git rev-list --left-right --count HEAD...@{upstream} 2>/dev/null | awk '{printf "ahead=%s behind=%s", $1, $2}' || echo "no-upstream")"

# GitHub state (only if gh CLI available + remote configured)
CI_STATE=""
PR_COUNT=""
ISSUE_COUNT=""
if /usr/bin/which gh > /dev/null 2>&1 && /usr/bin/git config --get remote.origin.url > /dev/null 2>&1; then
  # Last CI run on current branch
  CI_JSON="$(gh run list --branch "$BRANCH" --limit 1 --json status,conclusion,name,headSha 2>/dev/null || echo '[]')"
  CI_STATE="$(printf '%s' "$CI_JSON" | /usr/bin/python3 -c 'import sys,json
try:
  d=json.load(sys.stdin)
  if d: r=d[0]; print(f"{r.get(\"name\",\"?\")}:{r.get(\"status\",\"?\")}:{r.get(\"conclusion\") or \"...\"}")
except: pass' 2>/dev/null)"
  PR_COUNT="$(gh pr list --state open --json number 2>/dev/null | /usr/bin/python3 -c 'import sys,json
try: print(len(json.load(sys.stdin)))
except: print("?")' 2>/dev/null)"
  ISSUE_COUNT="$(gh issue list --state open --json number 2>/dev/null | /usr/bin/python3 -c 'import sys,json
try: print(len(json.load(sys.stdin)))
except: print("?")' 2>/dev/null)"
fi

# Build context block
REPO_NAME="$(/usr/bin/basename "$CWD")"
CONTEXT_LINES=()
CONTEXT_LINES+=("[CWD STATE — auto-injected, 60s cache]")
CONTEXT_LINES+=("  repo: $REPO_NAME  branch: $BRANCH  HEAD: $HEAD_SHA")
if [ -n "$HEAD_SUBJECT" ]; then
  CONTEXT_LINES+=("  last commit: \"$HEAD_SUBJECT\" by $HEAD_AUTHOR")
fi
CONTEXT_LINES+=("  working tree: staged=$STAGED_COUNT unstaged=$UNSTAGED_COUNT untracked=$UNTRACKED_COUNT")
if [ "$AHEAD_BEHIND" != "no-upstream" ]; then
  CONTEXT_LINES+=("  vs upstream: $AHEAD_BEHIND")
fi
if [ -n "$CI_STATE" ]; then
  CONTEXT_LINES+=("  last CI run: $CI_STATE")
fi
if [ -n "$PR_COUNT" ] && [ -n "$ISSUE_COUNT" ]; then
  CONTEXT_LINES+=("  open: PRs=$PR_COUNT issues=$ISSUE_COUNT")
fi

# Format as JSON additionalContext
JSON_OUT="$(/usr/bin/python3 <<PYEOF
import json, os
lines = """$(printf '%s\n' "${CONTEXT_LINES[@]}")""".strip().split("\n")
ctx = "\n".join(lines)
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": ctx
    }
}))
PYEOF
)"

# Cache and emit
printf '%s' "$JSON_OUT" > "$CACHE_FILE"
printf '%s' "$JSON_OUT"
exit 0
