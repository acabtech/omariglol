#!/usr/bin/env sh
# 02_run.sh — start acquisition (:RUN).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "start acquisition"
scpi ":RUN"
echo "   acquisition started"
