#!/usr/bin/env sh
# 12_waveform.sh — read the waveform preamble and data block for CHAN1.
#
# The preamble is ASCII; the data block is binary, so it is rendered as hex.
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner "waveform transfer (CHAN1)"
scpi ":WAV:SOUR CHAN1"
scpi ":WAV:MODE NORM"
scpi ":WAV:FORM BYTE"
scpi ":WAV:POIN 1200"
printf '   preamble: %s\n' "$(scpi ':WAV:PRE?')"
echo "   data (first bytes, hex):"
scpi_hex ":WAV:DATA?" | cut -c1-120
