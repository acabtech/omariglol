#!/usr/bin/env sh
# 20_capture.sh — capture the full waveform memory to a NumPy .npy + JSON sidecar.
#
# Uses `riglol capture`, which stops the scope, reads the preamble, walks the RAW
# memory in 250k-point chunks and stitches them into one sample vector. The .npy
# loads directly with `numpy.load()` (shape (n, 2): time_s, voltage_v).
#
# Usage: ./20_capture.sh [ADDRESS] [FILE] [SOURCE]
#
#   SOURCE   waveform source, default CHAN1 (also CHAN2..CHAN4, MATH)
#
# The scope is stopped by capture; run `./02_run.sh` afterwards to resume.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

OUT="${2:-ds1054z-capture.npy}"
SOURCE="${3:-CHAN1}"
banner "capture $SOURCE -> $OUT"
# shellcheck disable=SC2086  # TRANSPORT_ARGS is intentionally word-split.
"$RIGLOL_BIN" capture "$ADDRESS" "$OUT" \
    --source "$SOURCE" \
    --mode raw \
    --data-format word \
    --timeout "$TIMEOUT" \
    $TRANSPORT_ARGS

if command -v python3 >/dev/null 2>&1 && python3 -c 'import numpy' 2>/dev/null; then
    python3 - "$OUT" <<'PY'
import sys
import numpy as np

wave = np.load(sys.argv[1])
print(f"   numpy: shape={wave.shape} dtype={wave.dtype}")
print(f"   time:  {wave[0, 0]:.6e} .. {wave[-1, 0]:.6e} s")
print(f"   volts: {wave[:, 1].min():.3f} .. {wave[:, 1].max():.3f} V")
PY
fi
