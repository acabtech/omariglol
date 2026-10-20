#!/usr/bin/env bash
# Diagnose an omariglol install.
#
# Checks the CLI and desktop app, the Omarchy theme and shell plugin, the
# configuration, the build/runtime dependencies, disk and memory headroom, and
# network connectivity, and prints a report with remediation hints. Exit status
# is non-zero if any check fails.
#
# Usage:
#   scripts/doctor.sh [options]
#
# Options:
#   --system            Also check system-wide paths (escalates to root).
#   --json              Print the report as JSON.
#   -h, --help          Show this help.
#
# Examples:
#   scripts/doctor.sh
#   sudo scripts/doctor.sh --system
set -uo pipefail

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
log_file="$log_dir/doctor.log"

_log() {
  [[ -n "$log_file" ]] || return 0
  printf '%s %s\n' "$(date -Is)" "$*" >>"$log_file" 2>/dev/null || true
}

# Re-exec under sudo when a step needs root (e.g. --system).
ensure_root() {
  [[ "$(id -u)" -eq 0 ]] && return 0
  command -v sudo >/dev/null 2>&1 || die "root is required but sudo was not found"
  printf '\033[1;34m==>\033[0m %s\n' "escalating to root (sudo)"
  exec sudo env OMARIGLOL_STATE_DIR="$state_dir" \
    "$here/scripts/$(basename "$0")" "${orig_args[@]}"
}

have() { command -v "$1" >/dev/null 2>&1; }

# --- Argument parsing -------------------------------------------------------
system_check=0
json=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --system)
      system_check=1
      shift
      ;;
    --json)
      json=1
      shift
      ;;
    -h | --help)
      sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      printf '\033[1;31m==>\033[0m %s\n' "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

# --- Report helpers ---------------------------------------------------------
fails=0
warns=0
results=()
record() { results+=("$1|$2"); }

ok() {
  [[ "$json" == 1 ]] || printf '\033[1;32m[ ok ]\033[0m %s\n' "$*"
  record ok "$*"
  _log "OK    $*"
}
warn() {
  [[ "$json" == 1 ]] || printf '\033[1;33m[warn]\033[0m %s\n' "$*"
  warns=$((warns + 1))
  record warn "$*"
  _log "WARN  $*"
}
bad() {
  [[ "$json" == 1 ]] || printf '\033[1;31m[fail]\033[0m %s\n' "$*"
  fails=$((fails + 1))
  record fail "$*"
  _log "FAIL  $*"
}
info() {
  [[ "$json" == 1 ]] || printf '       %s\n' "$*"
  _log "INFO  $*"
}

[[ "$json" == 1 ]] || printf '\033[1;34m==>\033[0m omariglol doctor\n\n'

# --- Binaries ---------------------------------------------------------------
cli="$target_home/.local/bin/riglol"
[[ -x "$cli" ]] || cli="$(command -v riglol 2>/dev/null || true)"
if [[ -n "$cli" && -x "$cli" ]]; then
  if version="$("$cli" --version 2>/dev/null)"; then
    ok "CLI: $cli ($version)"
  else
    bad "CLI present at $cli but failed to run"
    info "reinstall it with: scripts/install.sh"
  fi
else
  bad "CLI not installed"
  info "install it with: scripts/install.sh"
fi

gui="$here/target/release/omariglol"
if [[ -x "$gui" ]]; then
  ok "desktop app: $gui"
else
  warn "desktop app not built at $gui"
  info "build it with: npm run tauri build"
fi

# --- Service status: shell, plugin, registration ----------------------------
if pgrep -x omarchy-shell >/dev/null 2>&1; then
  ok "omarchy shell is running"
else
  warn "omarchy shell is not running"
  info "start it with: omarchy restart shell"
fi

plugin="$target_home/.config/omarchy/plugins/acabtech.omariglol"
if [[ -f "$plugin/manifest.json" ]]; then
  ok "shell plugin installed"
else
  warn "shell plugin not installed"
  info "install it with: scripts/install.sh"
fi

shell_json="$target_home/.config/omarchy/shell.json"
if [[ -f "$shell_json" ]] && grep -q 'acabtech.omariglol' "$shell_json" 2>/dev/null; then
  ok "plugin registered in shell.json"
else
  warn "plugin not registered in shell.json"
  info "enable it with: omarchy plugin enable acabtech.omariglol"
fi

# --- Theme ------------------------------------------------------------------
tpl="$target_home/.config/omarchy/themed/omariglol-theme.toml.tpl"
if [[ -f "$tpl" ]]; then
  ok "theme template installed"
else
  warn "theme template not installed"
  info "install it with: scripts/install.sh --theme-only"
fi

rendered="$target_home/.local/state/omarchy/current/theme/omariglol-theme.toml"
if [[ -f "$rendered" ]]; then
  ok "theme rendered: $rendered"
else
  info "theme not rendered yet; switch theme once to generate it"
fi

# --- Configuration validity -------------------------------------------------
cfg="$target_home/.config/omariglol/config.toml"
if [[ -f "$cfg" ]]; then
  if have python3; then
    if python3 -c 'import sys, tomllib; tomllib.load(open(sys.argv[1], "rb"))' "$cfg" 2>/dev/null; then
      ok "config is valid TOML: $cfg"
    else
      bad "config is not valid TOML: $cfg"
      info "fix or remove it; defaults are used when it is absent"
    fi
  else
    ok "config present: $cfg (python3 missing; not validated)"
  fi
else
  warn "no config at $cfg (defaults are used)"
  info "create one with: scripts/install.sh"
fi

# --- Dependency availability ------------------------------------------------
for tool in cargo node npm python3 omarchy git; do
  if have "$tool"; then
    ok "dependency: $tool"
  else
    warn "dependency missing: $tool"
  fi
done

# --- Disk usage -------------------------------------------------------------
if have df; then
  avail_kb="$(df -Pk "$target_home" 2>/dev/null | awk 'NR==2 {print $4}')"
  if [[ "$avail_kb" =~ ^[0-9]+$ ]]; then
    avail_mb=$((avail_kb / 1024))
    if ((avail_mb < 512)); then
      warn "low disk space on $target_home: ${avail_mb} MiB free"
      info "free space with: du -sh ~/.cache/* | sort -h"
    else
      ok "disk space on $target_home: ${avail_mb} MiB free"
    fi
  fi
fi

# --- Memory usage -----------------------------------------------------------
if [[ -r /proc/meminfo ]]; then
  mem_avail_kb="$(awk '/^MemAvailable:/{print $2}' /proc/meminfo)"
  mem_total_kb="$(awk '/^MemTotal:/{print $2}' /proc/meminfo)"
  if [[ "$mem_avail_kb" =~ ^[0-9]+$ && "$mem_total_kb" =~ ^[0-9]+$ && "$mem_total_kb" -gt 0 ]]; then
    pct=$((mem_avail_kb * 100 / mem_total_kb))
    if ((pct < 10)); then
      warn "low memory: ${pct}% available ($((mem_avail_kb / 1024)) MiB of $((mem_total_kb / 1024)) MiB)"
      info "close memory-heavy applications before large captures"
    else
      ok "memory: ${pct}% available ($((mem_avail_kb / 1024)) MiB of $((mem_total_kb / 1024)) MiB)"
    fi
  fi
fi

# --- Network connectivity ---------------------------------------------------
# Instruments are reached over the LAN, so a default route and outbound TCP are
# the two things worth checking.
if have ip; then
  if ip route get 1.1.1.1 >/dev/null 2>&1; then
    ok "network: a default route exists"
  else
    warn "network: no default route"
    info "check your network connection"
  fi
fi
if have timeout; then
  if timeout 3 bash -c 'exec 3<>/dev/tcp/1.1.1.1/53' 2>/dev/null; then
    ok "network: outbound TCP connectivity works"
  else
    warn "network: could not reach 1.1.1.1:53"
    info "instrument discovery needs a working LAN; check firewall and routing"
  fi
fi

# --- System-wide paths (--system) -------------------------------------------
if [[ "$system_check" == 1 ]]; then
  ensure_root
  if [[ -x /usr/local/bin/riglol ]]; then
    ok "system CLI: /usr/local/bin/riglol"
  else
    info "no system-wide CLI at /usr/local/bin/riglol"
  fi
fi

# --- Summary ----------------------------------------------------------------
if [[ "$json" == 1 ]]; then
  printf '{\n  "fails": %d,\n  "warns": %d,\n  "checks": [\n' "$fails" "$warns"
  first=1
  for r in "${results[@]}"; do
    sev="${r%%|*}"
    msg="${r#*|}"
    [[ "$first" == 1 ]] || printf ',\n'
    first=0
    printf '    {"severity": "%s", "message": "%s"}' "$sev" "${msg//\"/\\\"}"
  done
  printf '\n  ]\n}\n'
else
  printf '\n%d check(s) failed, %d warning(s)\n' "$fails" "$warns"
fi

[[ "$fails" -eq 0 ]]
