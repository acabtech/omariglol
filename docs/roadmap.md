# Roadmap

## 0.1.4 → 0.1.7 — SCPI over USB

USB work is pulled forward out of 0.2.0. 0.1.3 delivered the initial DS1054Z
transport; 0.1.4 is the hardening release that followed end-to-end testing and
0.1.5 is a patch release for the plugin manifest and transport fixes. The full
plan is in [`plans/0.1.3-usb-scpi.md`](../plans/0.1.3-usb-scpi.md).

### 0.1.3 — Rigol DS1054Z over USBTMC (delivered)

- [x] `usb` — `UsbId`, `UsbSelector`, enumeration, driver status
- [x] `usb::backend` — rusb-backed bulk/control transfers, feature-gated
- [x] `usb::usbtmc` — USBTMC framing, implementing `ScpiTransport`
- [x] `Transport::Usb` and `usb:0xVVVV:0xPPPP[:SERIAL]` addressing
- [x] `riglol scpi|screenshot|benchmark|driver --usb`
- [x] `riglol usb list`
- [x] Linux udev rule and installer for libusb permissions
- [x] Hardware round-trip verification after the udev rule is installed
- [x] USB instruments merged into `discover` and workspace snapshots
- [x] Non-disruptive UDP portmapper LXI detection (M0 spike)
- [x] `PMAP_DUMP` presence-flag decoding fix for broadcast discovery

### 0.1.4 — hardening, generic USBTMC and USB discovery

- [x] End-to-end hardening after 0.1.3 hardware testing (see `CHANGELOG.md`)
- [ ] Any USBTMC-class instrument verified on hardware
- [ ] Hotplug events
- [x] USB merged into discovery and the Discovery panel

### 0.1.5 — plugin install and transport fixes

- [x] Repository-root plugin manifest for `omarchy plugin add`
- [x] Transport-aware screenshot IPC (USB and VXI-11)
- [x] Parametrised SCPI query detection in the console
- [x] Workspace reconnect preserves the USB selector and device name

### 0.1.6 — SUMP capture

- [ ] `usb::sump` for the Saleae clone
- [x] `waveform` model and `riglol capture` (full-memory capture with DS1000Z
      RAW chunk stitching, NumPy `.npy` + JSON metadata output)

### 0.1.7 — all targets over USB

- [ ] NI USB-6001 and Sipeed SLogic16U3 stubs
- [ ] Every listed device reachable (capture or a clear not-yet-implemented error)

## 0.1.0 — parity with lxi-tools

The goal: someone who uses `lxi-tools` can switch and lose nothing, and gain
Python scripting plus Omarchy integration.

### `omariglol-core`

- [x] `transport::rpc` — XDR codec, ONC RPC `CALL`/`REPLY`, record marking
- [x] `transport::rpc` — portmapper `GETPORT` (TCP and UDP) and `DUMP`
- [x] `transport::vxi11` — `create_link`, `device_write`, `device_read`,
      `destroy_link`
- [x] `transport::vxi11` — `device_readstb`, `device_trigger`, `device_clear`,
      `device_lock`, `device_remote`/`local`
- [x] `transport::raw` — TCP 5025 with newline framing and partial-read handling
- [x] `discovery` — VXI-11 broadcast `PMAP_DUMP`, merged with mDNS results
      (parser fixed against the presence-flag XDR list encoding, M0 spike)
- [x] `discovery` — mDNS/DNS-SD browse of `_lxi`, `_scpi-raw`, `_vxi-11`
- [x] `discovery` — non-disruptive portmapper-only LXI detection over UDP
      (`GETPORT`, no VXI-11 link), see [`spikes/M0-portmapper-detection.md`](spikes/M0-portmapper-detection.md)
- [x] `discovery` — parallel subnet scan with direct SCPI `*IDN?` probes
- [x] `discovery_engine` — TTL cache with incremental rescans
- [x] `screenshot` — IEEE-488.2 definite- and indefinite-length block parsing
- [x] `screenshot` — profiles for Rigol DS1000Z/DS2000/DS4000, Keysight
      InfiniiVision, Rohde & Schwarz
- [x] `waveform` — preamble parsing, DS1000Z RAW chunk stitching (`:WAV:STAR`/
      `:WAV:STOP` at 250 k points), byte/word/ASCII decoding, NumPy `.npy` and
      CSV writers
- [x] `benchmark` — round-trip statistics with percentiles

### `omariglol-script`

- [x] Embed CPython via PyO3 (`abi3-py310`, `auto-initialize`)
- [x] Expose `omariglol.connect()` returning an instrument object with
      `write`, `query`, `query_raw`, `idn`, `timeout`
- [x] Stream `print()` output and progress back to the caller
- [x] CSV logging helper (`CsvLogger`)
- [x] Progress helpers (`sample`, `status`, `checkpoint`, `working_dir`)
- [x] Cancel a running script
- [x] Multi-instrument `Workspace` API with batch queries, presets and runs

### CLI

- [x] `discover` with `--mdns`, `--timeout`, `--json`, `--add`
- [x] `scpi` with `--raw`, `--port`, `--hex`, and an interactive mode
- [x] `screenshot` with `--raw`, `--vxi11`, `--command` override
- [x] `capture` with `--source`, `--mode`, `--data-format`, `--usb`, `--raw`,
      `--csv`, `--json`
- [x] `benchmark` with `--count`, `--command`, `--json`
- [x] `run` for Python scripts
- [x] `driver`, `workspace`, `measure` and `autoscale`

### Desktop app

- [x] Tauri commands for every core operation, progress via events
- [x] Workspace, Discovery, Instrument, SCPI Console, Screenshot, Benchmark,
      Timeline and Script panels
- [x] Omarchy palette resolution with the documented fallback chain
- [x] Persist workspaces, the active instrument and window geometry

### Omarchy

- [x] `omarchy/themed/omariglol-theme.toml.tpl` template
- [x] `tech.alatcerdas.omariglol` bar-widget plugin + panel
- [x] Installer script that copies the template and registers the plugin

### Multi-instrument platform

- [x] Workspaces holding instruments, presets, rules and triggers
- [x] Shared timeline across instruments
- [x] Driver command surface derived from `*IDN?`
- [x] Parallel workspace measurement and autoscale
- [x] Snapshot capture (PNG + measurement CSV) after measure and script runs

## 0.2.0 — scripting depth

- [x] Live charts in the Script mode (line, scatter, gauge)
- [x] Multi-instrument scripts
- [x] CSV logging
- [ ] Script library with saved snippets
- [ ] Headless daemon mode so scripts can share one instrument connection

## Later

- [ ] Instrument profile database beyond the current driver table
- [ ] Screenshot diffing for regression runs
- [ ] HDF5 data export (CSV already lands through `CsvLogger`)
- [ ] LXI event / `*SRQ` subscription
- [ ] HiSLIP transport
