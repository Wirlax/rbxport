#!/usr/bin/env bash
# Invoke Claude headless to run the auto-ship-open-tickets skill once.
# Called by the launchd agent installed by setup-schedule.sh, and safe to run by
# hand. A lock makes overlapping runs a no-op, so an hourly tick that lands while
# a previous run is still shipping tickets just exits.
set -euo pipefail

# --- Resolve the repository containing this skill ---------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
WORKTREE_PARENT="$(cd "$REPO_DIR/.." && pwd)/rbxport-worktrees"

# --- Config (override via env before calling) -------------------------------
CLAUDE_BIN="${CLAUDE_BIN:-$HOME/.local/bin/claude}"

LOG_DIR="$SCRIPT_DIR/logs"
LOCK_DIR="$SCRIPT_DIR/.run.lock"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/run-$(date +%Y%m%d-%H%M%S).log"

log() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" | tee -a "$LOG_FILE"; }

# --- Single-instance lock ---------------------------------------------------
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
  log "Another run is in progress ($LOCK_DIR exists); exiting."
  exit 0
fi
trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT

# --- Run --------------------------------------------------------------------
cd "$REPO_DIR"
mkdir -p "$WORKTREE_PARENT"
log "Starting auto-ship run in $REPO_DIR (worktrees: $WORKTREE_PARENT)"

PROMPT='Invoke the auto-ship-open-tickets skill: repair open rbxport PRs with failed validation or merge conflicts, then ship every actionable open issue to dev per the skill, fanning out one subagent per queue item. Revalidate repaired PRs before merging. When finished, print the summary table.'

set +e
"$CLAUDE_BIN" -p "$PROMPT" \
  --permission-mode bypassPermissions \
  --dangerously-skip-permissions \
  --add-dir "$WORKTREE_PARENT" \
  --output-format text \
  >>"$LOG_FILE" 2>&1
status=$?
set -e

log "Claude exited with status $status"
exit "$status"
