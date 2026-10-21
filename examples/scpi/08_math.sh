#!/usr/bin/env sh
# 08_math.sh — enable the math waveform (CHAN1 + CHAN2) and show it.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "math waveform (CHAN1 + CHAN2)"
scpi ":MATH:OPER ADD"
scpi ":MATH:SOUR1 CHAN1"
scpi ":MATH:SOUR2 CHAN2"
scpi ":MATH:DISP ON"
echo "   math waveform enabled"
