# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
semantic versioning.

## [Unreleased]

## [0.1.6] — 2026-11-10

Waveform capture: full-memory reads with chunk stitching and NumPy output.

### Added

- **`riglol capture`** — full-memory waveform capture. It stops the scope, reads
  the `:WAV:PRE?` preamble, walks the acquisition memory in 250 000-point
  windows via `:WAV:STAR`/`:WAV:STOP`, stitches the chunk payloads and converts
  raw codes to volts. The DS1054Z `:WAV:MODE RAW` path otherwise only works
  while stopped and returns at most 250 k points per read.
- **NumPy/CSV output.** `capture` writes a float64 `.npy` array of shape
  `(n, 2)` (`time_s`, `voltage_v`) plus a JSON metadata sidecar, so an agent can
  `numpy.load()` the capture instead of streaming every sample through its
  context. `--csv` writes the two columns as text instead.
- `omariglol-core::waveform` module (`CaptureOptions`, `Waveform`,
  `WaveformPreamble`, `WaveformMode`, `WaveformFormat`, `WordByteOrder`) shared
  by every frontend.
- `docs/waveform-capture.md` documenting the RAW-mode constraints, chunking,
  formats and the CLI.

## [0.1.5] — 2026-11-04

Patch release fixing plugin installation and the three reported transport and
query bugs.

### Added

- NanoVNA USBFix driver profile. A NanoVNA-H running the USBTMC firmware from
  [acabtech/NanoVNAUSBFix](https://github.com/acabtech/NanoVNAUSBFix) is
  classified as a vector network analyzer and exposes sweep, acquisition,
  frequency, S11/S21 data and stock-shell escape-hatch operations.
- `examples/scpi/19_nanovna.sh` and `docs/nanovna.md`.
- A repository-root `manifest.json` whose entry point is
  `omarchy/plugin/BarWidget.qml`, so `omarchy plugin add` accepts both the git
  URL and a checkout path.

### Fixed

- **Screenshot IPC ignored the instrument's transport.** The desktop app always
  dialed raw TCP at the stored address. It now calls `capture_from_device`, so
  USB instruments open over USBTMC, VXI-11-only instruments stay on VXI-11, and
  the `*IDN?` is queried directly instead of relying on a cached value.
- **Parametrised SCPI queries in the console.** The frontend now inspects the
  command header, matching the CLI, so `:MEAS:VPP? CHAN1` is sent as a query
  instead of a write and its reply cannot poison the next query.
- **Workspace reconnect dropped USB selectors.** `Workspace::connect` now uses
  `Target::from_device`, preserving the USB selector and VXI-11 device name.

### Changed

- Documentation now describes installing the shell plugin from the repository
  root or directly from `omarchy/plugin/`.

## [0.1.4] — 2026-11-02

Release hardening after end-to-end testing of 0.1.3 on a Rigol DS1054Z over
USBTMC.

### Changed

- Internal version references now consistently say 0.1.4.

### Fixed

- **SCPI queries with parameters.** `riglol scpi` decided whether to read a
  response with `ends_with('?')`, so `:MEAS:VPP? CHAN1` was sent as a write. The
  unread reply then desynchronised the next query on USBTMC. Query detection now
  inspects the command header, including semicolon-separated compounds.
- `measure` reporting the Rigol `9.9E37` sentinel as a 38-digit integer in its
  error text; the original instrument reply is now preserved.
- Static-host discovery tests were not hermetic: an attached USBTMC instrument
  was merged into `found` even when the test only exercised network probes.
- `examples/scpi/18_usb.sh` now resolves the repository's `bin/riglol` like the
  other examples, and is executable.
- ShellCheck warnings in `scripts/doctor.sh` and `scripts/install-usb-udev.sh`.
- Formatting and Clippy lints that failed under current stable Rust.

### Verified

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo build --release --locked`.
- USB round-trip coverage on a DS1054Z: parameter queries, interactive mode,
  multi-query sessions, `measure`, `autoscale`, screenshots, `benchmark`,
  `driver`, `usb list`, `discover --add`, and Python scripts.
- `npm run build`, the Tauri shell `cargo check`, the documentation site build
  and ShellCheck over all shell examples.

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
