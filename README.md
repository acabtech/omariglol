# omariglol

**A native-to-Omarchy reimplementation of [lxi-tools](https://github.com/lxi-tools/lxi-tools).**

`omariglol` talks to LXI / SCPI test instruments oscilloscopes, power supplies,
signal generators, DMMs — over VXI-11 and raw TCP, and presents them through a
CLI, a themed desktop app, and the Omarchy shell itself.

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
                  │   VXI-11 · raw TCP   │
                  │   discovery · SCPI   │
                  └──────────────────────┘
```

| Component | Path | Role |
| --- | --- | --- |
| `omariglol-core` | `crates/omariglol-core` | Protocol core: VXI-11 over ONC RPC, raw socket SCPI, discovery, screenshot, benchmark |
| `omariglol-script` | `crates/omariglol-script` | Embedded CPython scripting engine |
| `omariglol` CLI | `crates/omariglol-cli` | Command-line interface, feature-parity with `lxi` |
| Desktop app | `src-tauri` + `src` | Tauri 2 shell with a React/TypeScript frontend |
| Omarchy integration | `omarchy/` | Shell plugin (QML) and theme template |

Full detail in [`docs/architecture.md`](docs/architecture.md).

## Planned feature set (0.1.0)

Mirrors `lxi-tools`, with Python in place of Lua:

| Command | Status | Notes |
| --- | --- | --- |
| `omariglol discover` | stub | VXI-11 broadcast + mDNS/DNS-SD |
| `omariglol scpi` | stub | VXI-11 or `--raw`, hex output, interactive mode |
| `omariglol screenshot` | stub | IEEE-488.2 block capture, per-vendor profiles |
| `omariglol benchmark` | stub | SCPI round-trip statistics |
| `omariglol run` | stub | Execute a Python measurement script |
| GUI: Search | stub | Instrument browser |
| GUI: SCPI | stub | Command console |
| GUI: Screenshot | stub | Live instrument display |
| GUI: Benchmark | stub | Throughput measurement |
| GUI: Script | stub | Editor + live charts |

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

```bash
# Protocol core + CLI
cargo build

# Frontend
npm install
npm run build

# Desktop app
npm run tauri dev
```

## Roadmap

- **0.1.0** — the feature set above.
- **0.2.0** — scripting depth: live charts, CSV logging, multi-instrument scripts.
- **Later** — instrument profiles, screenshot diffing, headless daemon mode.

## License

MIT — see [`LICENSE.txt`](LICENSE.txt).

`omariglol` is an independent reimplementation. It shares no code with
`lxi-tools`, which is licensed separately.
