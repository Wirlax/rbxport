#!/usr/bin/env bash
# Install (or manage) a launchd agent that runs run.sh every hour, so Claude
# ships open RBX tickets on a schedule without anyone kicking it off.
#
# Usage:
#   ./setup-schedule.sh install     # create + load the hourly agent (default)
#   ./setup-schedule.sh uninstall   # stop + remove the agent
#   ./setup-schedule.sh status      # is it loaded? recent state
#   ./setup-schedule.sh run-now     # trigger one run immediately
#   ./setup-schedule.sh logs        # tail the latest run log
#
# launchd won't start a second copy while one is still running, so an hourly
# tick that lands mid-run is skipped rather than stacked.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNNER="$SCRIPT_DIR/run.sh"
LABEL="com.chrisle.rbxport.auto-ship-open-tickets"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
INTERVAL="${INTERVAL:-3600}"   # seconds; 1 hour
DOMAIN="gui/$(id -u)"
LAUNCHD_OUT="$SCRIPT_DIR/logs/launchd.out.log"
LAUNCHD_ERR="$SCRIPT_DIR/logs/launchd.err.log"

# A PATH launchd's minimal environment lacks — needs claude, gh, git, op, node.
AGENT_PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

write_plist() {
  mkdir -p "$HOME/Library/LaunchAgents" "$SCRIPT_DIR/logs"
  chmod +x "$RUNNER"
  cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>$RUNNER</string>
    </array>
    <key>StartInterval</key>
    <integer>$INTERVAL</integer>
    <key>RunAtLoad</key>
    <false/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>WorkingDirectory</key>
    <string>$SCRIPT_DIR</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>$AGENT_PATH</string>
        <key>HOME</key>
        <string>$HOME</string>
    </dict>
    <key>StandardOutPath</key>
    <string>$LAUNCHD_OUT</string>
    <key>StandardErrorPath</key>
    <string>$LAUNCHD_ERR</string>
</dict>
</plist>
EOF
}

load_agent() {
  launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
  launchctl bootstrap "$DOMAIN" "$PLIST" 2>/dev/null \
    || launchctl load -w "$PLIST"
  launchctl enable "$DOMAIN/$LABEL" 2>/dev/null || true
}

case "${1:-install}" in
  install)
    write_plist
    load_agent
    echo "Installed: $LABEL"
    echo "  Runs:     $RUNNER"
    echo "  Interval: every $INTERVAL s"
    echo "  Plist:    $PLIST"
    echo "  Logs:     $SCRIPT_DIR/logs/"
    echo "Trigger one run now with: $0 run-now"
    ;;
  uninstall)
    launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null \
      || launchctl unload -w "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "Uninstalled: $LABEL (plist removed). Logs kept in $SCRIPT_DIR/logs/"
    ;;
  status)
    if launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1; then
      echo "Loaded: $LABEL"
      launchctl print "$DOMAIN/$LABEL" | grep -E 'state|last exit|runs|program =' || true
    else
      echo "Not loaded: $LABEL"
    fi
    ;;
  run-now)
    launchctl kickstart -k "$DOMAIN/$LABEL" 2>/dev/null \
      || { echo "Agent not loaded; running the script directly."; exec /bin/bash "$RUNNER"; }
    echo "Kicked off a run. Watch: $0 logs"
    ;;
  logs)
    latest="$(ls -t "$SCRIPT_DIR"/logs/run-*.log 2>/dev/null | head -1 || true)"
    [ -n "$latest" ] && { echo "== $latest =="; tail -n 40 "$latest"; } || echo "No run logs yet in $SCRIPT_DIR/logs/"
    ;;
  *)
    echo "Usage: $0 {install|uninstall|status|run-now|logs}"; exit 2 ;;
esac
