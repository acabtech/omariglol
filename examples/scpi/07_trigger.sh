#!/usr/bin/env sh
# 07_trigger.sh — configure an edge trigger on CHAN1 and read its status.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "edge trigger (CHAN1)"
scpi ":TRIG:MODE EDGE"
scpi ":TRIG:EDGE:SOUR CHAN1"
scpi ":TRIG:EDGE:SLOP POS"
scpi ":TRIG:EDGE:LEV 0.5"
scpi ":TRIG:SWE AUTO"
printf '   status: %s\n' "$(scpi ':TRIG:STAT?')"
