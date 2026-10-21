#!/usr/bin/env sh
# 04_single.sh — arm a single acquisition (:SING).
#
# A single sweep runs once and then stops. `*OPC?` blocks until it completes.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "single acquisition"
scpi ":SING"
scpi "*OPC?"
echo "   single sweep complete"
