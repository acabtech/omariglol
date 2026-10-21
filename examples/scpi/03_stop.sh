#!/usr/bin/env sh
# 03_stop.sh — stop acquisition (:STOP).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "stop acquisition"
scpi ":STOP"
echo "   acquisition stopped"
