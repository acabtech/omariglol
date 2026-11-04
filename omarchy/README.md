# Omarchy integration

Two independent pieces.

```
omarchy/
├── themed/
│   └── omariglol-theme.toml.tpl   # rendered by Omarchy on every theme switch
└── plugin/                        # Quickshell bar widget
    ├── manifest.json
    ├── BarWidget.qml
    ├── Panel.qml
    └── Model.js
```

Installing both is done by the single installer,
[`scripts/install.sh`](../scripts/install.sh); there is deliberately no second
installer in this directory.

The design is summarised here and in
[`docs/omarchy-integration.md`](../docs/omarchy-integration.md).

## Theme template

Omarchy's theming engine renders every `*.tpl` in `~/.config/omarchy/themed/`
into `~/.local/state/omarchy/current/theme/<name>`, substituting `{{ ... }}`
placeholders from the active theme's `colors.toml`.

Installing our template means the app inherits **any** theme — including themes
that did not exist when this was written.

```bash
mkdir -p ~/.config/omarchy/themed
cp omarchy/themed/omariglol-theme.toml.tpl ~/.config/omarchy/themed/
# Then switch theme once to generate the file.
```

Or let [`scripts/install.sh`](../scripts/install.sh) do it.

Full variable reference: [`docs/omarchy-integration.md`](../docs/omarchy-integration.md).

## Shell plugin

A Quickshell `bar-widget` that measures every instrument in the active
workspace from the top bar.

Use [`scripts/install.sh`](../scripts/install.sh), which copies the plugin into
`~/.config/omarchy/plugins/` and enables it:

```bash
scripts/install.sh --plugin-only
```

You can also install it with Omarchy's plugin manager. The repository root has
a `manifest.json` that points at `omarchy/plugin/BarWidget.qml`, so both the
repository URL and a local checkout work:

```bash
omarchy plugin add https://github.com/acabtech/omariglol.git --enable
omarchy plugin add /path/to/omariglol --enable
```

The plugin directory itself has a manifest too, so adding
`/path/to/omariglol/omarchy/plugin` directly is equally valid.

A bar widget also needs a **section**; enabling it without one leaves it
enabled but invisible. If you enable it by hand:

```bash
omarchy plugin enable tech.alatcerdas.omariglol --section right
```

The plugin does not speak SCPI itself — it shells out to the `riglol` binary.
That keeps the protocol in one place and the QML trivial, which matters because
Omarchy plugins run unsandboxed inside the long-lived shell process.

### What it does

- The bar face shows a glyph and the last run's `measured/total` count.
- **Left click** opens the panel; **middle click** measures all; **right click**
  refreshes.
- The panel has four things:
  1. a **trigger button** that measures every instrument in the workspace at
     once — unless a Python script is configured, in which case it runs the
     script instead;
  2. a **Detect** button that scans the network from the shell itself, lists
     discovered instruments with their address and discovery method, and adds
     them to the active workspace so the trigger can measure them;
  3. the **workspace instrument list**, with each instrument's last reading or
     error;
  4. a clickable **script card**: `NO SCRIPT` opens a file picker, a loaded
     script runs, and `CHANGE` picks a different one. The choice is persisted
     in `shell.json`'s widget entry.

The trigger calls `riglol measure --json`, which loads the active workspace
from `~/.config/omariglol/workspaces`, connects to every instrument, runs a
per-type default measurement (scope `:MEAS:FREQ?`, DMM `:MEAS:VOLT?`, spectrum
analyzer `:CALC:MARK:Y?`) in parallel, and prints a JSON report. **Detect**
calls `riglol discover --json --mdns --add`, so discovery uses the same engine
as the CLI and desktop app and persists what it finds to the active workspace,
or to the configured `workspace` setting, creating a `Default` one when none
exists. That is what makes detect → measure work: the measurement fan-out only
reads the workspace, never the discovery cache.

### Settings

Declared in `manifest.json` and editable from the Omarchy settings UI:

| Key | Default | Meaning |
| --- | --- | --- |
| `binary` | `riglol` | The CLI binary to invoke. |
| `workspace` | `""` | Workspace name/id; empty = the active workspace. |
| `scriptPath` | `""` | Optional Python script; when set, the trigger runs it. |
| `measureCommand` | `""` | Optional measurement query override for every instrument. |

### Safety

No network code, no dynamic evaluation — just a `Process` call to the configured
binary. The plugin stays small enough to audit at a glance, which is the point:
Omarchy tells you plainly that plugins run as arbitrary unsandboxed code.
