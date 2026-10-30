#!/usr/bin/env bash
# Remove omariglol for the invoking user.
#
# Stops the desktop app, removes the shell plugin and the theme template, and
# deletes the CLI and configuration. A backup of the configuration and state is
# written to the shared state directory first, so a removal can be undone. The
# script asks for confirmation unless --yes is given.
#
# Usage:
#   scripts/uninstall.sh [options]
#
# Options:
#   --yes, -y           Do not prompt for confirmation.
#   --keep-config       Keep ~/.config/omariglol.
#   --keep-state        Keep ~/.local/state/omariglol (logs and backups).
#   --no-backup         Do not write a backup archive.
#   --purge             Remove everything, including config and state.
#   --target-user <name>
#                       Act on <name> instead of the invoking user.
#   -h, --help          Show this help.
#
# Examples:
#   scripts/uninstall.sh
#   scripts/uninstall.sh --yes --keep-config
#   scripts/uninstall.sh --purge --yes
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
log_file="$log_dir/uninstall.log"

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

# Re-exec under sudo when a step needs root (e.g. removing a system-wide CLI).
ensure_root() {
  [[ "$(id -u)" -eq 0 ]] && return 0
  command -v sudo >/dev/null 2>&1 || die "root is required but sudo was not found"
  log "escalating to root (sudo)"
  exec sudo env OMARIGLOL_STATE_DIR="$state_dir" \
    "$here/scripts/$(basename "$0")" "${orig_args[@]}"
}

have() { command -v "$1" >/dev/null 2>&1; }

# --- Argument parsing -------------------------------------------------------
assume_yes=0
keep_config=0
keep_state=0
do_backup=1
purge=0
target_user_opt=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes | -y)
      assume_yes=1
      shift
      ;;
    --keep-config)
      keep_config=1
      shift
      ;;
    --keep-state)
      keep_state=1
      shift
      ;;
    --no-backup)
      do_backup=0
      shift
      ;;
    --purge)
      purge=1
      shift
      ;;
    --target-user)
      [[ $# -ge 2 ]] || die "--target-user requires a username"
      target_user_opt="$2"
      shift 2
      ;;
    -h | --help)
      sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

# --target-user overrides the invoking user; re-resolve the home and state dir.
if [[ -n "$target_user_opt" ]]; then
  target_user="$target_user_opt"
  target_home="$(getent passwd "$target_user" 2>/dev/null | cut -d: -f6 || true)"
  [[ -n "$target_home" ]] || die "no such user: $target_user"
  state_dir="${OMARIGLOL_STATE_DIR:-$target_home/.local/state/omariglol}"
  log_dir="$state_dir/logs"
  mkdir -p "$log_dir"
  log_file="$log_dir/uninstall.log"
fi

# --- Confirmation -----------------------------------------------------------
if [[ "$assume_yes" != 1 ]]; then
  printf 'This will remove omariglol for %s:\n' "$target_user"
  printf '  - the desktop app (if running)\n'
  printf '  - the shell plugin and theme template\n'
  printf '  - the CLI at %s/.local/bin/riglol\n' "$target_home"
  if [[ "$purge" == 1 || "$keep_config" == 0 ]]; then
    printf '  - the configuration at %s/.config/omariglol\n' "$target_home"
  fi
  printf 'Continue? [y/N] '
  read -r reply || reply=""
  case "$reply" in
    [yY] | [yY][eE][sS]) ;;
    *)
      log "aborted"
      exit 0
      ;;
  esac
fi

# --- Backup -----------------------------------------------------------------
archive=""
if [[ "$do_backup" == 1 ]]; then
  backup_dir="$state_dir/backups"
  mkdir -p "$backup_dir"
  stamp="$(date +%Y%m%d-%H%M%S)"
  archive="$backup_dir/omariglol-$stamp.tar.gz"
  log "writing a backup to $archive"
  tar -czf "$archive" --ignore-failed-read -C "$target_home" \
    .config/omariglol \
    .config/omarchy/themed/omariglol-theme.toml.tpl \
    .config/omarchy/plugins/tech.alatcerdas.omariglol 2>/dev/null ||
    warn "backup was incomplete (some paths were missing)"
fi

# --- Stop the desktop app ---------------------------------------------------
if pgrep -x omariglol >/dev/null 2>&1; then
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
fi

# --- Remove the shell plugin ------------------------------------------------
# Use omarchy's own command so its registry and shell.json are updated; fall
# back to removing the directory directly.
log "removing the shell plugin"
if have omarchy; then
  omarchy plugin remove tech.alatcerdas.omariglol --yes >/dev/null 2>&1 ||
    warn "omarchy plugin remove failed; removing files directly"
  # Best-effort removal of the pre-rename plugin id.
  omarchy plugin remove acabtech.omariglol --yes >/dev/null 2>&1 || true
fi
rm -rf "$target_home/.config/omarchy/plugins/tech.alatcerdas.omariglol"
rm -rf "$target_home/.config/omarchy/plugins/acabtech.omariglol"

# --- Remove the theme template ----------------------------------------------
log "removing the theme template"
rm -f "$target_home/.config/omarchy/themed/omariglol-theme.toml.tpl"

# --- Remove the CLI ---------------------------------------------------------
log "removing the CLI"
rm -f "$target_home/.local/bin/riglol"
# A system-wide install, if present, needs root to remove.
if [[ -e /usr/local/bin/riglol ]]; then
  ensure_root
  log "removing the system-wide CLI at /usr/local/bin/riglol"
  rm -f /usr/local/bin/riglol
fi

# --- Remove the configuration -----------------------------------------------
if [[ "$purge" == 1 || "$keep_config" == 0 ]]; then
  log "removing the configuration"
  rm -rf "$target_home/.config/omariglol"
else
  log "keeping $target_home/.config/omariglol"
fi

# --- Remove state and logs --------------------------------------------------
if [[ "$purge" == 1 ]]; then
  log "removing state and logs"
  rm -rf "$state_dir"
elif [[ "$keep_state" == 1 ]]; then
  log "keeping $state_dir"
else
  # Keep the backups directory so a removal can still be undone.
  log "removing logs (keeping backups)"
  rm -rf "$log_dir"
fi

# --- Restart the shell so the widget disappears -----------------------------
if have omarchy; then
  log "restarting the shell"
  omarchy restart shell >/dev/null 2>&1 || warn "could not restart the shell; run 'omarchy restart shell'"
fi

log "omariglol removed for $target_user"
if [[ -n "$archive" && -f "$archive" ]]; then
  echo "  backup: $archive"
fi
