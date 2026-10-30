"""Identify a Rigol DS1054Z over USB (USBTMC).

Prerequisites:
  - the app/CLI built with the `usb` feature (on by default)
  - the udev rule installed: sudo scripts/install-usb-udev.sh
  - the instrument replugged, or: sudo udevadm trigger

Run from the Script panel or with `riglol run examples/python/16_usb.py`.
"""

from omariglol import connect

# `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]`. The DS1054Z is 0x1AB1:0x04CE.
SELECTOR = "usb:0x1AB1:0x04CE"

with connect(SELECTOR, transport="usb") as scope:
    print("IDN:", scope.idn())
    print("frequency:", scope.query(":MEAS:FREQ?"), "Hz")
