# NI USB-6009 over USB

The National Instruments **USB-6009** is a USB multifunction I/O (DAQ) card:
analog inputs/outputs, digital lines and counters. Unlike the Rigol oscilloscope,
the NanoVNA and the nanoDLA, it speaks **no SCPI** and exposes **no USBTMC**
interface. It is a `niVISA`/proprietary device that answers no `*IDN?`.

`omariglol` treats it as a **first-class USB device at the detection and
identity level**: it is recognised, labelled and listed, but it deliberately
carries no SCPI command surface and the platform issues no DAQ I/O to it.

## Selector

```text
usb:0x3923:0x717B
```

The VID/PID are National Instruments' stock values (`0x3923:0x717B`). There is
no USBTMC interface to probe, so these ids are the *only* discriminator — which
is exactly why recognition keys on the USB descriptor rather than on an
`*IDN?` reply.

## Recognition (descriptor-based, not IDN-based)

The platform's main extension point, `DRIVER_PROFILES`, is keyed on the `*IDN?`
string and therefore presupposes SCPI. A device that can never answer `*IDN?`
needs a descriptor analogue: `USB_DEVICE_PROFILES`, matched by `vendor:product`.

```rust
// crates/omariglol-core/src/driver.rs
pub static USB_DEVICE_PROFILES: &[UsbDeviceProfile] = &[
    UsbDeviceProfile {
        name: "NI USB-6009",
        vendor: 0x3923,
        product: 0x717B,
        instrument_type: InstrumentType::DataAcquisition,
        manufacturer: "National Instruments",
    },
];
```

Adding a future non-SCPI USB DAQ means appending one row here; the USB backend,
discovery, `riglol usb list` and the UI all read the same table.

## What "first-class" means here

- **Discovery** folds the device in even though `is_usbtmc` is false, because
  the backend marks it `is_daq`. It appears in `riglol usb list` and in a
  discovery scan's USB devices.
- **Identity**: it is classified as `InstrumentType::DataAcquisition` — label
  `data acquisition device`, glyph `daq` — in the CLI table and the GUI.
- **Addressability**: it is addressed by its `usb:0x3923:0x717B` selector, like
  any USB instrument.

## What it does *not* get

`InstrumentType::speaks_scpi()` returns `false` for a data acquisition device,
so `generic_commands` yields an **empty** command surface. There is no
`*IDN?`, no `*RST`, no `:MEAS:*?` — and `riglol scpi --usb usb:0x3923:0x717B`
and `riglol driver --usb ...` will not produce a useful session, because the
device has no SCPI/USBTMC endpoint to talk to.

> **Scope.** This is detection and identity only. Reading analog inputs,
> writing outputs or driving counters would require implementing NI's
> proprietary protocol — out of scope for the current tier.

## udev

`scripts/99-omariglol-usb.rules` grants unprivileged access by explicit id,
because the device matches neither the Rigol rule nor the generic USBTMC
class rule:

```text
SUBSYSTEM=="usb", ATTR{idVendor}=="3923", ATTR{idProduct}=="717b", MODE="0660", GROUP="uucp", TAG+="uaccess"
```

Install once with `sudo scripts/install-usb-udev.sh`, then replug (or
`sudo udevadm trigger`).

## Example

```sh
examples/scpi/21_ni6009.sh
```

The script only enumerates and shows the device's driver kind — there is no
SCPI to send.
