#!/usr/bin/env sh
# 10_timebase.sh — set the horizontal timebase and read it back.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "timebase"
scpi ":TIM:MODE MAIN"
scpi ":TIM:MAIN:SCAL 0.001"
scpi ":TIM:MAIN:OFFS 0"
printf '   scale: %s s/div\n' "$(scpi ':TIM:MAIN:SCAL?')"
