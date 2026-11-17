#!/usr/bin/env sh
# 21_ni6009.sh — show the NI USB-6009 DAQ as a first-class USB device.
#
# The National Instruments USB-6009 (3923:717B) speaks no SCPI and exposes no
# USBTMC interface, so there is nothing to send it. This example only proves
# the detection half: the card is enumerated, recognised by its vendor/product
# id, labelled "NI USB-6009", and reported with driver kind "daq".
#
# Prerequisites:
#   - build with the `usb` feature (the CLI enables it by default)
#   - install the udev rule so libusb can read the descriptor:
#       sudo scripts/install-usb-udev.sh
#   - replug the card, or run: sudo udevadm trigger
#
# See also: docs/ni-usb6009.md
set -eu

# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

banner() {
    printf '\n== %s\n' "$1"
}

banner "usb enumerate"
"$RIGLOL_BIN" usb list

banner "the USB-6009 line only"
# Driver column shows "daq"; exit non-zero just for the grep, not the demo.
"$RIGLOL_BIN" usb list | grep -i '3923:717b' || echo "NI USB-6009 not attached"

banner "SCPI is not applicable"
cat <<'EOF'
The USB-6009 has no SCPI command surface (InstrumentType::DataAcquisition does
not speak SCPI), so `riglol scpi --usb usb:0x3923:0x717B` has no endpoint to
talk to. Detection and identity only — see docs/ni-usb6009.md.
EOF
