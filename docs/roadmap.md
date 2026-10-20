# Roadmap

## 0.1.0 — parity with lxi-tools

The goal: someone who uses `lxi-tools` can switch and lose nothing, and gain
Python scripting plus Omarchy integration.

### `omariglol-core`

- [ ] `transport::rpc` — XDR codec, ONC RPC `CALL`/`REPLY`, record marking
- [ ] `transport::rpc` — portmapper `GETPORT` (TCP and UDP) and `DUMP`
- [ ] `transport::vxi11` — `create_link`, `device_write`, `device_read`,
      `destroy_link`
- [ ] `transport::vxi11` — `device_readstb`, `device_trigger`, `device_clear`,
      `device_lock`, `device_remote`/`local`
- [ ] `transport::raw` — TCP 5025 with newline framing and partial-read handling
- [ ] `discovery` — VXI-11 broadcast `PMAP_DUMP`, merged with mDNS results
- [ ] `discovery` — mDNS/DNS-SD browse of `_lxi`, `_scpi-raw`, `_vxi-11`
- [ ] `screenshot` — IEEE-488.2 definite- and indefinite-length block parsing
- [ ] `screenshot` — profiles for Rigol DS1000Z/DS2000/DS4000, Keysight
      InfiniiVision, Rohde & Schwarz
- [ ] `benchmark` — round-trip statistics with percentiles

### `omariglol-script`

- [x] Embed CPython via PyO3 (`abi3-py310`, `auto-initialize`)
- [x] Expose `omariglol.connect()` returning an instrument object with
      `write`, `query`, `query_raw`, `idn`, `timeout`
- [x] Stream `print()` output and progress back to the caller
- [x] CSV logging helper
- [x] Cancel a running script

### CLI

- [ ] `discover` with `--mdns`, `--timeout`, `--json`
- [ ] `scpi` with `--raw`, `--port`, `--hex`, and an interactive mode
- [ ] `screenshot` with `--raw`, `--command` override
- [ ] `benchmark` with `--count`, `--command`, `--json`
- [x] `run` for Python scripts

### Desktop app

- [ ] Tauri commands for every core operation, progress via events
- [ ] Search / SCPI / Screenshot / Benchmark / Script modes
- [ ] Omarchy palette resolution with the documented fallback chain
- [ ] Persist last-used instrument and window geometry

### Omarchy

- [ ] `omarchy/themed/omariglol-theme.toml.tpl` template
- [ ] `acabtech.omariglol` bar-widget plugin + panel
- [ ] Installer script that copies the template and registers the plugin

## 0.2.0 — scripting depth

- [ ] Live charts in the Script mode (line, scatter, gauge)
- [ ] Multi-instrument scripts
- [ ] Script library with saved snippets
- [ ] Headless daemon mode so scripts can share one instrument connection

## Later

- [ ] Instrument profile database (beyond screenshots)
- [ ] Screenshot diffing for regression runs
- [ ] Data export (CSV, HDF5)
- [ ] LXI event / `*SRQ` subscription
- [ ] HiSLIP transport
