<div class="hero">
<h1>omariglol</h1>
<p class="tagline">LXI / SCPI instrument control, native to Omarchy.</p>
<p class="lede">A native reimplementation of <a href="https://github.com/lxi-tools/lxi-tools">lxi-tools</a>: talk to oscilloscopes, power supplies, signal generators and DMMs over VXI-11 and raw TCP — from a CLI, a themed desktop app, and the Omarchy shell itself.</p>
<div class="badges">
<span class="badge ok">v0.1.0</span>
<span class="badge">Omarchy 4.x</span>
<span class="badge">Rust + Tauri</span>
<span class="badge">Python scripting</span>
<span class="badge">MIT</span>
</div>
<div class="buttons">
<a class="btn primary" href="docs/overview.html">Get started</a>
<a class="btn" href="docs/index.html">Read the docs</a>
<a class="btn ghost" href="https://github.com/acabtech/omariglol">View source ↗</a>
</div>
</div>

<!--screenshot-->

## What it is

`omariglol` talks to LXI / SCPI test instruments — oscilloscopes, power supplies,
signal generators, DMMs — over VXI-11 and raw TCP, and presents them through a
CLI, a themed desktop app, and the Omarchy shell itself.

The name is a portmanteau of *Omarchy* and *RigLOL*, but it is vendor-neutral:
anything that speaks SCPI is fair game.

## Why another lxi-tools?

`lxi-tools` is excellent and this project exists because of it, not in spite of
it. Three things motivated a reimplementation:

1. **It is a C/GTK application.** `omariglol` is Rust + Tauri — a single
   static-ish binary, no GTK theme drift, and a real IPC surface for other
   programs to drive.
2. **Its scripting language is Lua.** `omariglol` uses **Python** instead, so
   test scripts can use the same ecosystem engineers already automate with.
3. **It does not know about Omarchy.** `omariglol` reads the live Omarchy
   palette and inherits whatever theme you are running, with zero per-theme
   code — and it can be triggered from the shell's top bar.

## Highlights

<div class="grid">
<div class="card"><span class="ico">🔌</span><h3>VXI-11 and raw TCP</h3><p>A hand-written ONC RPC stack drives VXI-11, with a raw socket transport on TCP 5025 for instruments that prefer it.</p></div>
<div class="card"><span class="ico">🔎</span><h3>Discovery</h3><p>VXI-11 broadcast and mDNS/DNS-SD browsing, merged by address so a friendly name from mDNS enriches a bare IP from the broadcast.</p></div>
<div class="card"><span class="ico">🐍</span><h3>Python scripting</h3><p>An embedded CPython engine exposes a small instrument API — <code>connect()</code>, <code>write</code>, <code>query</code>, <code>idn</code> — with <code>numpy</code> and <code>matplotlib</code> for free.</p></div>
<div class="card"><span class="ico">🖼️</span><h3>Screenshots</h3><p>IEEE-488.2 block capture with per-vendor profiles for Rigol, Keysight and Rohde & Schwarz instruments.</p></div>
<div class="card"><span class="ico">🎨</span><h3>Native to Omarchy</h3><p>A Quickshell bar widget plus a theme template, so the app follows every theme switch — including themes that did not exist when this was written.</p></div>
<div class="card"><span class="ico">🧩</span><h3>One protocol core</h3><p>The CLI, the desktop app and the shell plugin all sit on <code>omariglol-core</code>. No frontend grows its own SCPI handling.</p></div>
</div>

## Install

Build the workspace and publish the CLI to `bin/`:

```sh
make                 # builds bin/riglol
```

The desktop app builds with Tauri:

```sh
npm install
npm run tauri build
```

Full setup, including the Omarchy theme template and shell plugin, is in
[Overview](docs/overview.html) and
[Omarchy integration](docs/omarchy-integration.html).

## Requirements

- **Omarchy 4.x** — Hyprland and `omarchy-shell` (Quickshell). Arch Linux.
- Rust (stable), Node.js 20+, and the usual Tauri Linux dependencies.
- CPython development headers for the scripting crate.

## Documentation

- [Overview](docs/overview.html) — what it is and how to build it.
- [Architecture](docs/architecture.html) — how the pieces fit together.
- [Omarchy integration](docs/omarchy-integration.html) — theming and the shell plugin.
- [Python examples](docs/python-examples.html) — scripts for the desktop app.
- [CLI examples](docs/cli-examples.html) — shell scripts around `riglol`.
- [Roadmap](docs/roadmap.html) — where the project is going.
- [Full index](docs/index.html)

## Attribution

Thank you to the [lxi-tools](https://github.com/lxi-tools/lxi-tools) project —
for the reference implementation, and for the years of instrument wrangling it
saved. This project is a grateful homage.
