#!/usr/bin/env sh
# 13_screenshot.sh — capture the instrument's screen to a PNG.
#
# Uses the dedicated `riglol screenshot` subcommand, which knows the Rigol
# DS1000Z profile and probes the vendor raw port (5555) automatically.
#
# Usage: ./13_screenshot.sh [ADDRESS] [FILE]
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

OUT="${2:-ds1054z-screen.png}"
banner "screenshot -> $OUT"
"$RIGLOL_BIN" screenshot "$ADDRESS" "$OUT" --port "$RAW_PORT" --timeout "$TIMEOUT"
