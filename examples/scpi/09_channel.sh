#!/usr/bin/env sh
# 09_channel.sh — set up CHAN1: display, coupling, probe, scale, offset.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "channel setup (CHAN1)"
scpi ":CHAN1:DISP ON"
scpi ":CHAN1:COUP DC"
scpi ":CHAN1:PROB 10"
scpi ":CHAN1:SCAL 0.5"
scpi ":CHAN1:OFFS 0"
echo "   CHAN1 configured"
