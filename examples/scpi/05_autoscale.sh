#!/usr/bin/env sh
# 05_autoscale.sh — auto-scale every channel (:AUT).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "auto-scale"
scpi ":AUT"
scpi "*OPC?"
echo "   auto-scale complete"
