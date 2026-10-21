#!/usr/bin/env sh
# 01_identify.sh — identify the instrument (*IDN?).
#
# The simplest possible example: one query, one response.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "identify"
scpi "*IDN?"
