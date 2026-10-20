#!/usr/bin/env bash
# Install omariglol for the invoking user.
#
# Installs the CLI, the Omarchy theme template, and the shell plugin, sets up a
# default configuration, and registers the plugin with the running shell. The
# install is per-user and idempotent: re-running it after a pull replaces the
# plugin tree and keeps an existing configuration. Steps that need to write
# outside the user's home (e.g. --system) escalate to root with sudo.
#
# Usage:
#   scripts/install.sh [options]
#
# Options:
#   --system            Install the CLI to /usr/local/bin (escalates to root).
#   --no-build          Do not build; use the binaries already in target/release.
#   --theme-only        Install only the Omarchy theme template.
#   --plugin-only       Install only the shell plugin.
#   --force             Overwrite an existing configuration file.
#   -h, --help          Show this help.
#
# Examples:
#   scripts/install.sh
#   scripts/install.sh --no-build
#   sudo scripts/install.sh --system
set -euo pipefail

orig_args=("$@")

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

# --- Shared conventions -----------------------------------------------------
# Resolve the invoking user and their home so per-user paths and the shared log
# directory stay correct even after escalating to root with sudo.
target_user="${SUDO_USER:-${USER:-$(id -un)}}"
target_home="$(getent passwd "$target_user" 2>/dev/null | cut -d: -f6 || true)"
[[ -n "$target_home" ]] || target_home="${HOME:-/root}"
state_dir="${OMARIGLOL_STATE_DIR:-$target_home/.local/state/omariglol}"
log_dir="$state_dir/logs"
mkdir -p "$log_dir"
log_file="$log_dir/install.log"

# Every message is mirrored into the shared log directory with a timestamp.
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

# Re-exec the whole script under sudo when a step needs root. The state dir is
# passed through so the escalated run logs to the same place.
ensure_root() {
  [[ "$(id -u)" -eq 0 ]] && return 0
  command -v sudo >/dev/null 2>&1 || die "root is required but sudo was not found"
  log "escalating to root (sudo)"
  exec sudo env OMARIGLOL_STATE_DIR="$state_dir" \
    "$here/scripts/$(basename "$0")" "${orig_args[@]}"
}

# When running as root, hand a freshly written path back to the invoking user so
# a `sudo scripts/install.sh` does not leave root-owned files in their home.
own() {
  [[ "$(id -u)" -eq 0 && -n "$target_user" && "$target_user" != "root" ]] || return 0
  chown -R "$target_user" "$@" 2>/dev/null || true
}

have() { command -v "$1" >/dev/null 2>&1; }

# --- Argument parsing -------------------------------------------------------
system_install=0
do_build=1
theme_only=0
plugin_only=0
force=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --system)
      system_install=1
      shift
      ;;
    --no-build)
      do_build=0
      shift
      ;;
    --theme-only)
      theme_only=1
      shift
      ;;
    --plugin-only)
      plugin_only=1
      shift
      ;;
    --force)
      force=1
      shift
      ;;
    -h | --help)
      sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

# --- Dependency resolution --------------------------------------------------
# The build needs cargo; the GUI build needs node/npm; the plugin registration
# needs omarchy. Missing optional tools are warnings; a missing required tool is
# fatal only when the step that needs it will actually run.
resolve_deps() {
  log "resolving dependencies"
  local missing=0
  if [[ "$do_build" == 1 ]]; then
    if have cargo; then
      log "cargo: $(cargo --version 2>/dev/null)"
    else
      warn "cargo not found; install Rust (https://rustup.rs) or pass --no-build"
      missing=1
    fi
  fi
  if have node; then log "node: $(node --version 2>/dev/null)"; else warn "node not found; the GUI build will be skipped"; fi
  if have npm; then log "npm: $(npm --version 2>/dev/null)"; else warn "npm not found; the GUI build will be skipped"; fi
  if have python3; then log "python3: $(python3 --version 2>/dev/null)"; else warn "python3 not found; scripting support will be unavailable"; fi
  if have omarchy; then log "omarchy: found"; else warn "omarchy not found; the plugin cannot be registered automatically"; fi
  if have git; then log "git: $(git --version 2>/dev/null)"; else warn "git not found"; fi
  [[ "$missing" == 0 ]] || die "required dependencies are missing"
}

# --- Build ------------------------------------------------------------------
build_binaries() {
  [[ "$do_build" == 1 ]] || {
    log "skipping build (--no-build)"
    return 0
  }
  log "building the CLI (release)"
  # `make build` compiles the workspace and publishes the binaries to bin/;
  # fall back to a plain cargo build when make is unavailable.
  if have make; then
    make build
  else
    cargo build --release --locked --workspace
  fi
  if have npm; then
    log "building the desktop frontend"
    npm install --no-audit --no-fund
    npm run build
  else
    warn "npm not found; skipping the desktop frontend build"
  fi
}

# --- Install steps ----------------------------------------------------------
install_cli() {
  local src="$here/target/release/riglol"
  [[ -x "$src" ]] || die "CLI binary not found at $src; run without --no-build"
  if [[ "$system_install" == 1 ]]; then
    ensure_root
    log "installing the CLI to /usr/local/bin/riglol"
    install -Dm755 "$src" /usr/local/bin/riglol
  else
    log "installing the CLI to $target_home/.local/bin/riglol"
    install -Dm755 "$src" "$target_home/.local/bin/riglol"
    own "$target_home/.local/bin/riglol"
  fi
}

setup_config() {
  local cfg_dir="$target_home/.config/omariglol"
  local cfg="$cfg_dir/config.toml"
  if [[ -f "$cfg" && "$force" != 1 ]]; then
    log "keeping existing $cfg"
    return 0
  fi
  log "writing default configuration to $cfg"
  mkdir -p "$cfg_dir"
  cat >"$cfg" <<'TOML'
# omariglol configuration.
# See docs/architecture.md for the full reference.

[discovery]
timeout_ms = 2000
mdns = true

[transport]
default = "vxi11"
port = 5025

[ui]
theme = "omarchy"
TOML
  own "$cfg_dir"
}

install_theme() {
  local src="$here/omarchy/themed/omariglol-theme.toml.tpl"
  [[ -f "$src" ]] || die "theme template not found at $src"
  local dest="$target_home/.config/omarchy/themed"
  log "installing the theme template to $dest"
  mkdir -p "$dest"
  install -m644 "$src" "$dest/omariglol-theme.toml.tpl"
  own "$dest"
  if have omarchy-theme-refresh; then
    log "regenerating themed configs"
    omarchy-theme-refresh >/dev/null 2>&1 || warn "theme refresh failed; switch theme once manually"
  else
    warn "omarchy-theme-refresh not found; switch theme once to generate the config"
  fi
}

install_plugin() {
  local src="$here/omarchy/plugin"
  [[ -d "$src" ]] || die "plugin directory not found at $src"
  local dest="$target_home/.config/omarchy/plugins/acabtech.omariglol"
  log "installing the shell plugin to $dest"
  mkdir -p "$(dirname "$dest")"
  # Replace the plugin tree wholesale so a newer version's dropped files do not
  # linger next to the freshly installed ones.
  rm -rf "$dest"
  cp -r "$src" "$dest"
  own "$(dirname "$dest")"
  if have omarchy; then
    log "registering the plugin with omarchy"
    omarchy plugin add "$dest" --enable >/dev/null 2>&1 ||
      omarchy plugin enable acabtech.omariglol >/dev/null 2>&1 ||
      warn "could not register the plugin; use Setup > Plugins in the Omarchy menu"
  else
    warn "omarchy not found; enable the plugin via Setup > Plugins"
  fi
}

restart_shell() {
  if have omarchy; then
    log "restarting the shell"
    omarchy restart shell >/dev/null 2>&1 || warn "could not restart the shell; run 'omarchy restart shell'"
  elif have omarchy-shell; then
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || warn "could not rescan plugins"
  else
    warn "omarchy not found; restart the shell manually"
  fi
}

# --- Main -------------------------------------------------------------------
log "installing omariglol for $target_user"

if [[ "$theme_only" == 0 ]]; then
  resolve_deps
  build_binaries
fi

if [[ "$plugin_only" == 0 ]]; then
  install_theme
fi

if [[ "$theme_only" == 0 ]]; then
  install_cli
  setup_config
  install_plugin
  restart_shell
fi

log "omariglol installed"
if [[ "$system_install" == 1 ]]; then
  echo "  CLI:    /usr/local/bin/riglol"
else
  echo "  CLI:    $target_home/.local/bin/riglol"
fi
echo "  config: $target_home/.config/omariglol/config.toml"
echo "  plugin: $target_home/.config/omarchy/plugins/acabtech.omariglol"
echo "  logs:   $log_dir"
