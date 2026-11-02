#!/usr/bin/env sh
# 19_nanovna.sh — drive a NanoVNA-H running the USBTMC firmware.
#
# Flash NanoVNA-H-1.2.50-USBTMC.bin from acabtech/NanoVNAUSBFix first. The VNA
# then exposes a USBTMC interface next to its normal CDC serial port. The
# selector is the stock ST VID/PID because only the interface class changes.
#
# Usage:
#   ./19_nanovna.sh
#   ./19_nanovna.sh usb:0x0483:0x5740
set -eu

# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

SELECTOR="${1:-usb:0x0483:0x5740}"

banner() {
    printf '\n== %s\n' "$1"
}

scpi_usb() {
    "$RIGLOL_BIN" scpi --usb "$SELECTOR" "$@"
}

banner "usb enumerate"
"$RIGLOL_BIN" usb list

banner "identify"
scpi_usb '*IDN?'

banner "configure sweep"
scpi_usb ':VNA:SWE:START 1000000'
scpi_usb ':VNA:SWE:STOP 30000000'
scpi_usb ':VNA:SWE:POINTS 101'

banner "run one sweep"
scpi_usb ':VNA:SWE:EXEC'

banner "read S11 points"
scpi_usb ':VNA:DATA:CHUNK? 0,0,8'

banner "stock shell escape hatch"
scpi_usb ':VNA:CMD? info'
