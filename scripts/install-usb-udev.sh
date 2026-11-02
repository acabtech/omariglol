#!/usr/bin/env sh
# Install the omariglol USB udev rule so `riglol --usb` can open USBTMC devices
# without root. Must be run as root (e.g. with sudo).
set -eu

RULE="99-omariglol-usb.rules"
SRC_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
DEST_DIR="${DEST_DIR:-/etc/udev/rules.d}"

if [ "$(id -u)" -ne 0 ]; then
    echo "error: run this script as root (sudo $0)" >&2
    exit 1
fi

install -m 0644 "$SRC_DIR/$RULE" "$DEST_DIR/$RULE"
echo "installed $DEST_DIR/$RULE"

# Reload and re-apply, so an already-attached instrument picks up the rule.
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules
    udevadm trigger --subsystem-match=usb --action=add
    echo "reloaded udev rules; replug the instrument if it is still inaccessible"
fi

echo "note: the active desktop user may need to log out/in once for uaccess ACLs"
