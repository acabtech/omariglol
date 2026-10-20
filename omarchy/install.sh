#!/usr/bin/env bash
# Install the omariglol Omarchy integration.
#
#   ./omarchy/install.sh            install the theme template and the plugin
#   ./omarchy/install.sh --theme    install only the theme template
#   ./omarchy/install.sh --plugin   install only the shell plugin
#
# Idempotent: safe to re-run after pulling changes.

set -euo pipefail

THEME_ONLY=0
PLUGIN_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --theme) THEME_ONLY=1 ;;
    --plugin) PLUGIN_ONLY=1 ;;
    -h | --help)
      sed -n '2,8p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
THEMED_DIR="${HOME}/.config/omarchy/themed"
PLUGIN_DIR="${HOME}/.config/omarchy/plugins/acabtech.omariglol"

install_theme() {
  echo "==> installing the theme template"
  mkdir -p "${THEMED_DIR}"
  cp "${REPO_ROOT}/omarchy/themed/omariglol-theme.toml.tpl" "${THEMED_DIR}/"
  echo "    ${THEMED_DIR}/omariglol-theme.toml.tpl"

  # The template only regenerates on a theme switch, so trigger one.
  if command -v omarchy-theme-refresh >/dev/null 2>&1; then
    echo "==> regenerating themed configs"
    omarchy-theme-refresh || echo "    (theme refresh failed; switch theme once manually)"
  else
    echo "    note: omarchy-theme-refresh not found; switch theme once to generate"
  fi
}

install_plugin() {
  echo "==> installing the shell plugin"
  mkdir -p "$(dirname "${PLUGIN_DIR}")"

  if [[ -d "${PLUGIN_DIR}/.git" ]]; then
    echo "    updating existing install"
    git -C "${PLUGIN_DIR}" pull --ff-only
  else
    rm -rf "${PLUGIN_DIR}"
    cp -r "${REPO_ROOT}/omarchy/plugin" "${PLUGIN_DIR}"
  fi
  echo "    ${PLUGIN_DIR}"

  if command -v omarchy >/dev/null 2>&1; then
    echo "==> enabling acabtech.omariglol"
    omarchy plugin enable acabtech.omariglol ||
      echo "    (enable failed; use Setup > Plugins in the Omarchy menu)"
  else
    echo "    note: 'omarchy' not on PATH; enable via Setup > Plugins"
  fi
}

if [[ ${PLUGIN_ONLY} -eq 0 ]]; then
  install_theme
fi
if [[ ${THEME_ONLY} -eq 0 ]]; then
  install_plugin
fi

echo
echo "Done."
