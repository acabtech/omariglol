#!/usr/bin/env sh
# Shared helpers for the riglol CLI examples.
#
# Every example sources this file, so binary discovery, the instrument address
# and the transport options live in exactly one place. The examples are thin
# wrappers around the `riglol` CLI: they never speak SCPI themselves, they only
# invoke `riglol scpi` (or a sibling subcommand).
#
# Usage:  ./02_run.sh [ADDRESS]
#
#   ADDRESS   instrument IPv4 address or hostname.
#             Defaults to $RIGLOL_ADDRESS, then 192.168.100.36.
#
# Environment:
#   RIGLOL            path to the riglol binary (default: <repo>/bin/riglol, then $PATH)
#   RIGLOL_ADDRESS    default instrument address
#   RIGLOL_TRANSPORT  "raw" (default) or "vxi11"
#   RIGLOL_RAW_PORT   raw SCPI port (default: 5555)
#   RIGLOL_TIMEOUT    socket timeout in seconds (default: 10)

set -eu

# --- Locate the riglol binary ------------------------------------------------
# Resolve the repository root from this script's own location (examples/scpi/),
# so the examples work no matter which directory they are run from. The binary
# is the one `make` publishes to bin/.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -n "${RIGLOL:-}" ]; then
    RIGLOL_BIN="$RIGLOL"
elif [ -x "$REPO_ROOT/bin/riglol" ]; then
    RIGLOL_BIN="$REPO_ROOT/bin/riglol"
elif command -v riglol >/dev/null 2>&1; then
    RIGLOL_BIN="riglol"
else
    echo "error: riglol not found at $REPO_ROOT/bin/riglol" >&2
    echo "       build it with 'make' (which writes bin/riglol)" >&2
    echo "       or set RIGLOL=/path/to/riglol" >&2
    exit 127
fi

# --- Resolve the instrument address -----------------------------------------
ADDRESS="${1:-${RIGLOL_ADDRESS:-192.168.100.36}}"

# --- Transport options -------------------------------------------------------
# The DS1054Z speaks SCPI on Rigol's raw socket, port 5555 (not the LXI-standard
# 5025), so raw is the default. Set RIGLOL_TRANSPORT=vxi11 to use VXI-11.
RAW_PORT="${RIGLOL_RAW_PORT:-5555}"
# Socket timeout in seconds. Slow operations (auto-scale, *RST, *TST?) need more
# than the 3 s default.
TIMEOUT="${RIGLOL_TIMEOUT:-10}"
TRANSPORT_ARGS="--raw --port $RAW_PORT"
if [ "${RIGLOL_TRANSPORT:-raw}" = "vxi11" ]; then
    TRANSPORT_ARGS=""
fi

# Send one or more SCPI commands to the instrument.
# Queries (ending in '?') print their response; writes are silent.
scpi() {
    # shellcheck disable=SC2086  # TRANSPORT_ARGS is intentionally word-split.
    "$RIGLOL_BIN" scpi "$ADDRESS" $TRANSPORT_ARGS --timeout "$TIMEOUT" "$@"
}

# Send a query and render the raw response as hex, for binary blocks.
scpi_hex() {
    # shellcheck disable=SC2086
    "$RIGLOL_BIN" scpi "$ADDRESS" $TRANSPORT_ARGS --timeout "$TIMEOUT" --hex "$@"
}

# Print a short banner so each example's output is self-describing.
banner() {
    echo "== $1"
    echo "   instrument: $ADDRESS"
}
