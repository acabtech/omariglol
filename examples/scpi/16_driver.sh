#!/usr/bin/env sh
# 16_driver.sh — show the named command surface for the instrument.
#
# The DS1054Z matches the rich "Rigol DS1000Z" profile, so this lists the full
# operation surface (acquire, trigger, channel, timebase, measure, waveform).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "driver surface"
"$RIGLOL_BIN" driver "$ADDRESS" --raw --port "$RAW_PORT" --timeout "$TIMEOUT"
