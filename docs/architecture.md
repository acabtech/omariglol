# Architecture

## Design goals

1. **One protocol core, three frontends.** The CLI, the desktop app and the
   Omarchy shell plugin must not each grow their own SCPI handling.
2. **Blocking core.** Instruments are slow, SCPI is request/response, and every
   caller already has a place to run blocking work (a thread, a Tauri
   `spawn_blocking`, a CLI main). An async runtime in the core would add
   complexity for no throughput.
3. **No UI framework in the protocol layer.** `omariglol-core` depends on
   `serde`, `thiserror`, `socket2`, `rand` and `base64`. Nothing else.
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
| `transport` | `ScpiTransport` trait and the `Session` convenience wrapper |
| `discovery` | VXI-11 broadcast and mDNS/DNS-SD browsing |
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

Two mechanisms, deliberately independent:

- **VXI-11 broadcast.** A `PMAP_DUMP` RPC call sent to the broadcast address on
  port 111. Hosts that answer *and* have program `0x0607AF` (VXI-11
  `DEVICE_CORE`) registered on TCP are instruments. Subnet-local by design,
  which is also its limitation.
- **mDNS / DNS-SD.** Browse `_lxi._tcp.local`, `_scpi-raw._tcp.local` and
  `_vxi-11._tcp.local` on `224.0.0.251`. Finds instruments that advertise
  themselves and can cross a routed boundary.

Results from both are merged by IP address, preferring the record with more
information, so an mDNS hit supplies a friendly name for a device that the
broadcast only found as a bare IP.

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
omariglol discover [--mdns] [--timeout N]
omariglol scpi [--raw] [--port N] <address> [command...]
omariglol screenshot [--raw] <address> <file>
omariglol benchmark [--count N] [--command CMD] <address>
omariglol run <script.py> [args...]
```

## Desktop app

`src-tauri` is the backend, `src/` is the frontend.

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
├── App.tsx             shell layout + mode switching
├── styles.css          all styling, driven by CSS custom properties
├── theme/omarchy.ts    resolves the live Omarchy palette into CSS variables
├── lib/api.ts          typed wrappers over the Tauri IPC commands
├── lib/types.ts        types mirroring the Rust structs
└── modes/              one component per GUI mode
```

### Theming contract

Every colour in the UI comes from a CSS custom property defined on `:root`
(`--om-bg`, `--om-fg`, `--om-accent`, `--om-red`, …). `theme/omarchy.ts` is the
only module that knows where those values come from. Swapping the theme engine
means editing one file. Details in
[`omarchy-integration.md`](omarchy-integration.md).
