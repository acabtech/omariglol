#!/usr/bin/env sh
# 18_usb.sh — identify a riglol instrument over USB (USBTMC).
#
# Prerequisites:
#   - build with the `usb` feature (the CLI enables it by default)
#   - install the udev rule so libusb can open the device:
#       sudo scripts/install-usb-udev.sh
#   - replug the instrument, or run: sudo udevadm trigger
#
# Usage:
#   ./18_usb.sh                       # Rigol DS1054Z default selector
#   ./18_usb.sh usb:0x1AB1:0x04CE:SN  # pin to a serial number
#
# See also: riglol usb list
set -eu

# Resolve the same binary the other examples use (repo bin/riglol, then PATH).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

SELECTOR="${1:-usb:0x1AB1:0x04CE}"

banner() {
    printf '\n== %s\n' "$1"
}

banner "usb enumerate"
"$RIGLOL_BIN" usb list

banner "identify over USBTMC"
"$RIGLOL_BIN" scpi --usb "$SELECTOR" '*IDN?'

banner "read a measurement over USBTMC"
"$RIGLOL_BIN" scpi --usb "$SELECTOR" ':MEAS:FREQ?'
