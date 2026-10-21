#!/usr/bin/env sh
# 06_measure.sh — read the standard measurements for CHAN1.
#
# Sets the measurement source, then queries each measurement in turn. Every
# query prints its own response.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "measurements (CHAN1)"
scpi ":MEAS:SOUR CHAN1"
printf '   Vpp:  %s\n' "$(scpi ':MEAS:VPP?')"
printf '   Vavg: %s\n' "$(scpi ':MEAS:VAVG?')"
printf '   Vrms: %s\n' "$(scpi ':MEAS:VRMS?')"
printf '   Freq: %s\n' "$(scpi ':MEAS:FREQ?')"
printf '   Per:  %s\n' "$(scpi ':MEAS:PER?')"
