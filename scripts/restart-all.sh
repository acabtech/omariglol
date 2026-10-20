#!/usr/bin/env bash
# Restart every omariglol component in dependency order.
#
# Stops the desktop app, reinstalls the Omarchy theme template and the shell
# plugin, and restarts the shell so the new QML takes effect. The order matters:
# the theme is installed before the plugin, and the shell is restarted last so
# it picks up both. Pass --rebuild/-r to compile first.
#
# Usage:
#   scripts/restart-all.sh [options]
#
# Options:
#   --rebuild, -r       Rebuild the CLI and frontend before restarting.
#   --gui               Also restart the desktop app.
#   --no-shell          Do not restart the Omarchy shell.
#   -h, --help          Show this help.
#
# Examples:
#   scripts/restart-all.sh
#   scripts/restart-all.sh --rebuild
#   scripts/restart-all.sh --gui
set -euo pipefail

orig_args=("$@")

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

# --- Shared conventions -----------------------------------------------------
target_user="${SUDO_USER:-${USER:-$(id -un)}}"
target_home="$(getent passwd "$target_user" 2>/dev/null | cut -d: -f6 || true)"
[[ -n "$target_home" ]] || target_home="${HOME:-/root}"
state_dir="${OMARIGLOL_STATE_DIR:-$target_home/.local/state/omariglol}"
log_dir="$state_dir/logs"
mkdir -p "$log_dir"
log_file="$log_dir/restart-all.log"

_log() {
  [[ -n "$log_file" ]] || return 0
  printf '%s %s\n' "$(date -Is)" "$*" >>"$log_file" 2>/dev/null || true
}
log() {
  printf '\033[1;34m==>\033[0m %s\n' "$*"
  _log "INFO  $*"
}
warn() {
  printf '\033[1;33m==>\033[0m %s\n' "$*" >&2
  _log "WARN  $*"
}
die() {
  printf '\033[1;31m==>\033[0m %s\n' "$*" >&2
  _log "ERROR $*"
  exit 1
}

# Re-exec under sudo when a step needs root (e.g. a root-owned desktop app).
ensure_root() {
  [[ "$(id -u)" -eq 0 ]] && return 0
  command -v sudo >/dev/null 2>&1 || die "root is required but sudo was not found"
  log "escalating to root (sudo)"
  exec sudo env OMARIGLOL_STATE_DIR="$state_dir" \
    "$here/scripts/$(basename "$0")" "${orig_args[@]}"
}

have() { command -v "$1" >/dev/null 2>&1; }

# --- Argument parsing -------------------------------------------------------
rebuild=0
restart_gui=0
restart_shell_flag=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --rebuild | -r)
      rebuild=1
      shift
      ;;
    --gui)
      restart_gui=1
      shift
      ;;
    --no-shell)
      restart_shell_flag=0
      shift
      ;;
    -h | --help)
      sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

# --- Helpers ----------------------------------------------------------------
# Stop the desktop app gracefully (SIGTERM) so it can flush state, then escalate
# to SIGKILL only if it refuses to exit.
stop_gui() {
  pgrep -x omariglol >/dev/null 2>&1 || return 0
  log "stopping the desktop app"
  pkill -TERM -x omariglol 2>/dev/null || true
  for _ in $(seq 1 50); do
    pgrep -x omariglol >/dev/null 2>&1 || break
    sleep 0.1
  done
  if pgrep -x omariglol >/dev/null 2>&1; then
    warn "the desktop app did not exit; sending SIGKILL"
    pkill -9 -x omariglol 2>/dev/null || true
  fi
}

# --- Main -------------------------------------------------------------------
log "restarting omariglol components for $target_user"

# 1. Stop the desktop app first, so it is not holding the old plugin.
stop_gui

# 2. Rebuild, if asked.
if [[ "$rebuild" == 1 ]]; then
  log "rebuilding the CLI (release)"
  # `make build` compiles the workspace and publishes the binaries to bin/;
  # fall back to a plain cargo build when make is unavailable.
  if have make; then
    make build
  else
    cargo build --release --locked --workspace
  fi
  if have npm; then
    log "rebuilding the desktop frontend"
    npm run build
  fi
fi

# 3. Theme first: the plugin and the app read the rendered theme.
log "reinstalling the theme template"
mkdir -p "$target_home/.config/omarchy/themed"
install -m644 "$here/omarchy/themed/omariglol-theme.toml.tpl" \
  "$target_home/.config/omarchy/themed/omariglol-theme.toml.tpl"

# 4. Plugin second: it depends on the theme being in place.
log "reinstalling the shell plugin"
plugin_dest="$target_home/.config/omarchy/plugins/acabtech.omariglol"
rm -rf "$plugin_dest"
mkdir -p "$(dirname "$plugin_dest")"
cp -r "$here/omarchy/plugin" "$plugin_dest"

# 5. Shell last: it must rescan after both the theme and the plugin changed.
if [[ "$restart_shell_flag" == 1 ]]; then
  if have omarchy; then
    log "restarting the shell"
    omarchy restart shell >/dev/null 2>&1 || warn "could not restart the shell; run 'omarchy restart shell'"
  elif have omarchy-shell; then
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || warn "could not rescan plugins"
  else
    warn "omarchy not found; restart the shell manually"
  fi
fi

# 6. Optionally bring the desktop app back up.
if [[ "$restart_gui" == 1 ]]; then
  gui_bin="$here/target/release/omariglol"
  if [[ -x "$gui_bin" ]]; then
    log "starting the desktop app"
    nohup "$gui_bin" >"$log_dir/gui.log" 2>&1 &
  else
    warn "desktop app binary not found at $gui_bin; build it with 'npm run tauri build'"
  fi
fi

log "done"
