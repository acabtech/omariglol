#!/usr/bin/env sh
# 11_acquire.sh — configure acquisition type, averaging and memory depth.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "acquisition settings"
scpi ":ACQ:TYPE AVER"
scpi ":ACQ:AVER 16"
scpi ":ACQ:MDEP AUTO"
printf '   sample rate: %s Sa/s\n' "$(scpi ':ACQ:SRAT?')"
