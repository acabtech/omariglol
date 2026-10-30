# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
semantic versioning.

## [0.1.3] — 2026-10-30

SCPI over USB (USBTMC) plus non-disruptive LXI discovery.

### Added

- **USB / USBTMC transport.** `UsbId`, `UsbSelector`, libusb enumeration, the
  USBTMC framing transport, `Transport::Usb`, and
  `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]` addressing.
- **USB in the CLI.** `riglol scpi|screenshot|benchmark|driver --usb` and
  `riglol usb list`.
- **USB in discovery.** `riglol discover` enumerates attached USBTMC
  instruments, merges them with network results, and supports `--no-usb`.
  USB entries are keyed by selector rather than the unspecified IP address.
- **Non-disruptive LXI detection.** Discovery falls back to a unicast UDP
  portmapper `GETPORT` for VXI-11 `DEVICE_CORE`, so an instrument is identified
  without creating a link or opening a raw socket. Measured in
  [`docs/spikes/M0-portmapper-detection.md`](docs/spikes/M0-portmapper-detection.md).
- **Future-scope scaffold.** A `DriverProfile` registry in
  `omariglol-core::driver` makes a new instrument family a table entry rather
  than a change to the generic path.
- **USB-aware workspace fan-out.** `measure`, `autoscale`, snapshots and `run`
  capture over USBTMC when the workspace instrument is a USB device.

### Changed

- `discover` reports the USB selector in its address column and uses the
  selector for instrument ids, eliminating `0.0.0.0` collisions.
- `measure` and `autoscale` report the selector for USB instruments.
- Unavailable measurements (the Rigol `9.9E37` sentinel) are no longer rendered
  as a 38-digit value.
- `screenshot` falls back from raw TCP to VXI-11 automatically when no explicit
  transport is given.
- VXI-11 reads rely on the `END` reason bit rather than a terminator character.
- Internal version references now consistently say 0.1.3.

### Fixed

- **`PMAP_DUMP` decoding.** XDR encodes a `pmaplist` with a presence boolean
  *before* each element; the parser read it after, misaligning every field. This
  also broke broadcast discovery.
- Workspace snapshots for USB instruments, which previously tried to connect to
  `0.0.0.0` over TCP.
- USB instrument ids and discovery cache entries collapsing onto one address.

### Verified

- Full USB and LXI round-trip coverage on a Rigol DS1054Z: identify, writes,
  measurements, waveform transfer, driver surface, benchmark, screenshots,
  workspace measure/autoscale and script snapshots.
- `cargo test -p omariglol-core` and `cargo check --workspace`.

### Known limitations

- The DS1054Z firmware disables LAN SCPI while USB is attached; the transports
  are discovered independently but cannot run SCPI simultaneously on this unit.
- Portmapper-detected LXI instruments show an empty model/serial in `discover`
  because no `*IDN?` is requested during the scan. USB entries use USB
  descriptors.
- VXI-11 is noticeably slower than USBTMC on this instrument (~0.6 s vs ~0.5 ms
  per query); screenshot transfer over VXI-11 is also slower.
