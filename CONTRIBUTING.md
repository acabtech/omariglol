# Contributing to omariglol

## Getting set up

```bash
git clone git@github.com:acabtech/omariglol.git
cd omariglol

# Rust workspace (protocol core + CLI)
cargo build

# Frontend
npm install
```

You will also need the Tauri Linux prerequisites (webkit2gtk-4.1, libsoup-3.0,
GTK 3) and, for the scripting crate, CPython development headers.

## Repository layout

```
crates/omariglol-core/   Protocol core — no UI, no CLI, no Python
crates/omariglol-script/ Embedded CPython scripting engine
crates/omariglol-cli/    The `riglol` binary
src-tauri/               Tauri 2 backend (desktop app)
src/                     React + TypeScript frontend
omarchy/                 Omarchy shell plugin and theme template
scripts/                 install.sh, uninstall.sh, doctor.sh and helpers
site/                    Static documentation site generator and landing page
examples/                Python and shell examples
docs/                    Architecture and integration notes
```

## Ground rules

1. **`omariglol-core` stays UI-agnostic and blocking.** Instruments are slow and
   the protocol is strictly request/response. Callers move work off the UI
   thread; the core does not grow an async runtime.
2. **No feature code without a test.** The protocol codecs (XDR, IEEE-488.2
   blocks, DNS name compression) are exactly the kind of thing that looks right
   and is wrong. They must be covered by unit tests.
3. **Omarchy compatibility is a hard requirement.** Never hard-code a colour.
   Read the palette at runtime or ship a template.
4. **Keep the frontends thin.** If logic could live in `omariglol-core`, it
   should.

## Commit style

Conventional Commits, scoped by component:

```
feat(core): implement VXI-11 create_link
fix(screenshot): correct IEEE-488.2 length-digit parsing
docs(omarchy): document the theme template pipeline
```

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test

npm run build          # type-check + bundle the frontend
```

## Adding an instrument profile

Screenshot capture is vendor-specific. Screenshot profiles live in
`crates/omariglol-core/src/screenshot.rs` as a table of `*IDN?` prefixes mapped
to setup commands and a capture query. The named command surface shown in the
app and exposed by `riglol driver` lives in `crates/omariglol-core/src/driver.rs`
and currently carries a rich profile for the Rigol DS1000Z family on top of a
generic per-type command set. Add a profile, add a test asserting the prefix
matches, and include a captured sample if you can.
