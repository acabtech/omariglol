#!/usr/bin/env sh
# 17_reset.sh — reset the instrument, clear its status and run a self-test.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "reset"
scpi "*CLS"
scpi "*RST"
scpi "*OPC?"
printf '   self-test (0 = pass): %s\n' "$(scpi '*TST?')"
