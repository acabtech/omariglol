# Architecture

## Design goals

1. **One protocol core, three frontends.** The CLI, the desktop app and the
   Omarchy shell plugin must not each grow their own SCPI handling.
2. **Blocking core.** Instruments are slow, SCPI is request/response, and every
   caller already has a place to run blocking work (a thread, a Tauri
   `spawn_blocking`, a CLI main). An async runtime in the core would add
   complexity for no throughput.
3. **No UI framework in the protocol layer.** `omariglol-core` depends on
   `serde`, `serde_json`, `thiserror`, `socket2`, `rand` and `base64`. Nothing
   else.
4. **Theme-agnostic UI.** The frontend never names a colour.

## Crates

### `omariglol-core`

The whole instrument-facing surface. Module map:

| Module | Responsibility |
| --- | --- |
| `device` | `Device`, `Target`, `Transport` — the shared instrument model |
| `error` | `Error`, `Result`, VXI-11 error-code naming |
| `transport::rpc` | ONC RPC client, XDR codec, portmapper (`GETPORT`, `DUMP`) |
| `transport::vxi11` | VXI-11 `device_core` channel: `create_link`, `device_write`, `device_read`, … |
| `transport::raw` | LXI raw socket on TCP 5025 |
| `usb` | USB abstraction: `UsbId`, `UsbSelector`, enumeration, driver registry |
| `usb::usbtmc` | USBTMC transport, implementing `ScpiTransport` |
| `transport` | `ScpiTransport` trait and the `Session` convenience wrapper |
| `connection` | A live connection with health and statistics tracking |
| `instrument` | `Instrument`, `InstrumentId`, `InstrumentState` and configuration |
| `workspace` | `Workspace`, `WorkspaceManager` and JSON persistence |
| `preset` / `rule` / `trigger` | Batch presets, conditional routing and the trigger graph |
| `timeline` | The shared, timestamped event log |
| `driver` | `InstrumentDriver` — a named command surface derived from `*IDN?` |
| `measure` | Parallel measurement fan-out across a workspace |
| `autoscale` | Parallel autoscale across instruments that support it |
| `netconfig` | Local interface, subnet, gateway and VLAN detection |
| `discovery` | VXI-11 broadcast, UDP portmapper detection, mDNS/DNS-SD and direct SCPI probes |
| `discovery_engine` | Cached multi-strategy discovery with TTL, incremental rescans and USB enumeration |
| `screenshot` | IEEE-488.2 block parsing, per-vendor capture profiles |
| `benchmark` | SCPI round-trip statistics |

#### Why implement ONC RPC by hand?

VXI-11 is an ONC RPC program, so a full RPC stack is the obvious dependency.
We implement the slice we need instead — `CALL`/`REPLY` framing with record
marking, `AUTH_NULL` credentials, and two portmapper calls — because:

- It is roughly 200 lines and fully testable.
- The portmapper `DUMP` call has to run over **UDP broadcast**, which most
  RPC libraries do not expose conveniently.
- It removes a large dependency from a tool that must build on both Linux and
  Windows.

#### Discovery

Four network mechanisms plus local USB enumeration, deliberately independent:

- **VXI-11 broadcast.** A `PMAP_DUMP` RPC call sent to the broadcast address on
  port 111. Hosts that answer *and* have program `0x0607AF` (VXI-11
  `DEVICE_CORE`) registered on TCP are instruments. Subnet-local by design,
  which is also its limitation.
- **UDP portmapper fallback.** A unicast `GETPORT` for `DEVICE_CORE`, used by
  the subnet scan when raw SCPI is unavailable. It identifies an instrument
  without creating a VXI-11 link or opening a raw socket, so repeated discovery
  does not disturb the instrument. This is the mechanism required for scopes
  whose LAN SCPI is disabled while USB is attached.
- **mDNS / DNS-SD.** Browse `_lxi._tcp.local`, `_scpi-raw._tcp.local` and
  `_vxi-11._tcp.local` on `224.0.0.251`. Finds instruments that advertise
  themselves and can cross a routed boundary.
- **Subnet scan.** Direct SCPI `*IDN?` probes over TCP (port 5025, Rigol's 5555
  and the other known vendor ports) against the local subnet and any extra
  hosts, run in parallel with a bounded timeout.
- **USB enumeration.** Any attached USBTMC interface becomes a `Device` keyed by
  its vendor/product ids (and serial when readable). Opening the interface is
  not required for detection, so LAN SCPI is not pre-empted.

Network results are merged by IP address, preferring the record with more
information, so an mDNS hit supplies a friendly name for a device that the
broadcast only found as a bare IP. USB results are keyed by their selector.
The `discovery_engine` wraps every strategy in a TTL cache and can rescan
incrementally, skipping network hosts whose entry is still fresh.

The portmapper path was measured in
[`spikes/M0-portmapper-detection.md`](spikes/M0-portmapper-detection.md): UDP
`GETPORT` is stable across repeated runs and roughly two orders of magnitude
faster than a TCP round trip.

### `omariglol-script`

Embeds CPython via PyO3 and exposes an instrument API to Python. This is where
`lxi-tools`' Lua lives, replaced deliberately:

- Engineers already automate in Python.
- `numpy` / `matplotlib` are available for free.
- PyO3 with `abi3` means one wheel-free binary works across Python 3.10+.

Python surface:

```python
from omariglol import connect

with connect("192.168.1.50") as inst:
    print(inst.idn())
    inst.write(":CHAN1:SCAL 0.5")
    freq = float(inst.query(":MEAS:FREQ?"))
```

### `omariglol-cli`

A `clap`-derived binary. Subcommands mirror `lxi` so muscle memory transfers:

```
riglol discover [--mdns] [--no-scan] [--no-usb] [--timeout N] [--json] [--add]
riglol scpi [--raw|--usb] [--port N] [--hex] <address> [command...]
riglol screenshot [--raw|--vxi11|--usb] [--command CMD] <address> <file>
riglol benchmark [--raw|--usb] [--count N] [--command CMD] [--json] <address>
riglol run <script.py> [args...]
riglol driver [--raw|--usb] [--json] <address>
riglol usb list [--json]
riglol workspace list|create|show
riglol measure [--workspace NAME] [--command CMD] [--json]
riglol autoscale [--workspace NAME] [--json]
```

USB addresses use `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]`, e.g.
`usb:0x1AB1:0x04CE:DS1ZA203012565`.

## Desktop app

`src-tauri` is the backend, `src/` is the frontend. State is organised around the
workspace model: a `WorkspaceManager` owns the set of workspaces and the active
one, and a map of live connections holds one transport session per connected
instrument. Workspaces persist to `~/.config/omariglol/workspaces`.

```
src-tauri/src/
├── main.rs        entry point
├── lib.rs         Tauri builder, plugin registration
├── commands.rs    #[tauri::command] handlers — the entire IPC surface
└── theme.rs       Omarchy palette discovery and reading
```

Long-running operations (discovery, benchmarks, screenshots) run via
`spawn_blocking` and report progress through Tauri events, so the UI never
blocks and a benchmark can stream live numbers.

## Frontend

React 19 + TypeScript + Vite, matching the house style of `acabtech/NuxSSHTerm`.

```
src/
├── main.tsx            bootstrap
├── App.tsx             shell layout and scroll-spy navigation
├── styles.css          all styling, driven by CSS custom properties
├── theme/omarchy.ts    applies the resolved palette as CSS custom properties
├── state/workspace.tsx workspace context: instruments, timeline, events
├── hooks/              keyboard shortcuts, scroll spy, window geometry
├── lib/api.ts          typed wrappers over the Tauri IPC commands
├── lib/types.ts        types mirroring the Rust structs
├── components/         shared UI (Sidebar, Console, Chart, Timeline/Topology)
└── sections/           one component per workbench panel
```

The workbench panels are Workspace, Discovery, Instrument, SCPI Console,
Screenshot, Benchmark, Timeline and Script. They are scrolled in one pane rather
than switched as tabs, so a running benchmark or script stays visible while you
work elsewhere.

### Theming contract

Every colour in the UI comes from a CSS custom property defined on `:root`
(`--om-bg`, `--om-fg`, `--om-accent`, `--om-red`, …). The Rust bridge
(`src-tauri/src/theme.rs`) resolves the live Omarchy palette following the
documented fallback chain, and `theme/omarchy.ts` is the only frontend module
that turns it into CSS custom properties. Swapping the theme engine means
editing those two files at most. Details in
[`omarchy-integration.md`](omarchy-integration.md).
