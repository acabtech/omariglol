# omariglol

**A native-to-Omarchy reimplementation of [lxi-tools](https://github.com/lxi-tools/lxi-tools).**

`omariglol` talks to LXI / SCPI test instruments — oscilloscopes, power supplies,
signal generators, DMMs and vector network analyzers — over VXI-11, raw TCP and
USB (USBTMC), and presents them through a CLI, a themed desktop app, and the
Omarchy shell itself.

The name is a portmanteau of *Omarchy* and *RigLOL*, but it is vendor-neutral:
anything that speaks SCPI is fair game.

---

## Why another lxi-tools?

`lxi-tools` is excellent and this project exists because of it, not in spite of
it. Three things motivated a reimplementation:

1. **It is a C/GTK application.** `omariglol` is Rust + Tauri, which gives a
   single static-ish binary, no GTK theme drift, and a real IPC surface for
   other programs to drive.
2. **Its scripting language is Lua.** `omariglol` uses **Python** instead, so
   test scripts can use the same ecosystem engineers already automate with.
3. **It does not know about Omarchy.** `omariglol` reads the live Omarchy
   palette and inherits whatever theme you are running, with zero per-theme
   code and it can be triggered from the shell's top bar.

## Architecture

Three frontends over one protocol core:

```
┌────────────────┐  ┌──────────────────┐  ┌──────────────────────┐
│  omariglol     │  │  omariglol       │  │  omarchy shell       │
│  CLI (Rust)    │  │  GUI (Tauri+TS)  │  │  plugin (QML)        │
└───────┬────────┘  └────────┬─────────┘  └──────────┬───────────┘
        │                    │                       │
        │            ┌───────┴────────┐              │
        │            │  omariglol-    │              │
        │            │  script        │              │
        │            │  (Python/CPython)             │
        │            └───────┬────────┘              │
        └────────────────────┼───────────────────────┘
                             │
                  ┌──────────┴───────────┐
                  │   omariglol-core     │
                  │ VXI-11 · raw · USBTMC│
                  │   discovery · SCPI   │
                  └──────────────────────┘
```

| Component | Path | Role |
| --- | --- | --- |
| `omariglol-core` | `crates/omariglol-core` | Protocol core: VXI-11 over ONC RPC, raw socket SCPI, USBTMC, discovery, screenshot, benchmark |
| `omariglol-script` | `crates/omariglol-script` | Embedded CPython scripting engine |
| `omariglol` CLI | `crates/omariglol-cli` | Command-line interface, feature-parity with `lxi` |
| Desktop app | `src-tauri` + `src` | Tauri 2 shell with a React/TypeScript frontend |
| Omarchy integration | `omarchy/` | Shell plugin (QML) and theme template |

Full detail in [`docs/architecture.md`](docs/architecture.md).

## Feature set (0.1.6)

The CLI mirrors `lxi-tools`, with Python in place of Lua:

| Command | Status | Notes |
| --- | --- | --- |
| `riglol discover` | implemented | VXI-11 broadcast, non-disruptive UDP portmapper detection, mDNS/DNS-SD, subnet scan, USB enumeration, TTL cache, `--json`, `--add`, `--no-usb` |
| `riglol scpi` | implemented | VXI-11, `--raw` or `--usb` (USBTMC), hex output, interactive mode |
| `riglol screenshot` | implemented | IEEE-488.2 block capture, per-vendor profiles, `--command` override, `--usb`, automatic VXI-11 fallback when raw SCPI is unavailable |
| `riglol capture` | implemented | Full-memory waveform capture with DS1054Z RAW chunk stitching; writes NumPy `.npy` + JSON metadata, or CSV; `--usb`, `--raw`, `--source`, `--mode`, `--data-format` |
| `riglol benchmark` | implemented | SCPI round-trip statistics, `--json`, `--usb` |
| `riglol usb list` | implemented | Enumerate USB instruments and their USBTMC interface status |
| `riglol run` | implemented | Execute a Python measurement script |
| `riglol driver` | implemented | Show the named command surface for an instrument, including the NanoVNA-H USBFix profile |
| `riglol workspace` | implemented | List, create and inspect saved workspaces |
| `riglol measure` | implemented | Measure every instrument in a workspace in parallel |
| `riglol autoscale` | implemented | Autoscale every capable instrument in a workspace |

The desktop app is a workspace-aware workbench with these panels:

| Panel | Status | Notes |
| --- | --- | --- |
| Workspace | implemented | Workspace list, instruments, presets and automation state |
| Discovery | implemented | Live multi-strategy discovery with a TTL cache |
| Instrument | implemented | Connection management and driver controls |
| SCPI Console | implemented | Per-instrument command console |
| Screenshot | implemented | Live instrument display |
| Benchmark | implemented | Throughput measurement with live samples |
| Timeline | implemented | Shared event log across instruments |
| Script | implemented | Python editor, streaming console and live charts |

## Omarchy integration

Two independent integration points, both scaffolded in `omarchy/`:

- **Theme inheritance.** A template at `omarchy/themed/omariglol-theme.toml.tpl`
  is rendered by Omarchy's own theming engine into
  `~/.local/state/omarchy/current/theme/`, so `omariglol` follows every theme
  switch — including themes that did not exist when this was written.
- **Shell plugin.** A Quickshell plugin (`omarchy/plugin/`) that puts an
  instrument in the top bar and lets you fire a measurement without leaving
  whatever you are doing.

See [`docs/omarchy-integration.md`](docs/omarchy-integration.md).

## Building

Requires Rust (stable), Node.js 20+, and the usual Tauri Linux dependencies.
The USB transport also needs the **libusb-1.0** development package
(`libusb-1.0-0-dev` on Debian/Ubuntu, `libusb` on Arch). It is behind the `usb`
cargo feature, which the CLI and app enable by default; build the core with
`--no-default-features` to skip it.

On Linux, libusb cannot open a USBTMC instrument without permission. Install the
bundled udev rule once:

```bash
sudo scripts/install-usb-udev.sh
```

Then replug the instrument (or run `sudo udevadm trigger`).

```bash
# Protocol core + CLI
cargo build

# Frontend
npm install
npm run build

# Desktop app
npm run tauri dev
```

## Waveform capture

`riglol capture` pulls a scope's acquisition memory and writes it to a NumPy
`.npy` file (plus a JSON metadata sidecar) so a script or an LLM agent can
analyse it with a few lines of `numpy` instead of streaming samples through
context:

```bash
riglol capture --usb usb:0x1AB1:0x04CE /tmp/ch1.npy --source CHAN1
python3 -c "import numpy as np; w = np.load('/tmp/ch1.npy'); print(w.shape)"
```

It hides the DS1054Z RAW-mode constraints: it stops the scope, reads the
preamble, walks the memory in 250 000-point chunks via `:WAV:STAR`/`:WAV:STOP`
and stitches the payloads into one sample vector, converting raw codes to volts.
USB is typically 5–6× faster than Ethernet for the same capture. See
[`docs/waveform-capture.md`](docs/waveform-capture.md).

## NanoVNA-H support

A NanoVNA-H running the USBTMC firmware from
[acabtech/NanoVNAUSBFix](https://github.com/acabtech/NanoVNAUSBFix) enumerates as
a normal USBTMC instrument at `usb:0x0483:0x5740`. A matching driver profile is
selected automatically from the `*IDN?` reply and exposes sweep, acquisition,
frequency, S11/S21 data and the stock-shell escape hatch:

```bash
riglol scpi --usb usb:0x0483:0x5740 '*IDN?'
riglol driver --usb usb:0x0483:0x5740
examples/scpi/19_nanovna.sh
```

Screenshots are not available over USBTMC; use the CDC interface for those. See
[`docs/nanovna.md`](docs/nanovna.md) for the full command reference.

## Release notes

See [`CHANGELOG.md`](CHANGELOG.md) for the 0.1.6 changes, including the
repo-root plugin manifest, transport-aware screenshots, parametrised SCPI
queries in the console, and workspace USB reconnects.

## Roadmap

- **0.1.x** — the feature set above; the current release is 0.1.6.
- **0.2.0** — scripting depth: a saved snippet library and a headless daemon so
  scripts can share one instrument connection. Live charts, CSV logging and
  multi-instrument workspace scripts have already landed.
- **Later** — a broader instrument profile database, screenshot diffing, LXI
  event/`*SRQ` subscription and HiSLIP.

## License

MIT — see [`LICENSE.txt`](LICENSE.txt).

`omariglol` is an independent reimplementation. It shares no code with
`lxi-tools`, which is licensed separately.
