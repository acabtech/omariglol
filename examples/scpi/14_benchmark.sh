#!/usr/bin/env sh
# 14_benchmark.sh — measure SCPI round-trip performance.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "benchmark (*IDN?)"
"$RIGLOL_BIN" benchmark "$ADDRESS" --count 25 --command "*IDN?" --raw --port "$RAW_PORT"
