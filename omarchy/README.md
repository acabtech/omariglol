# Omarchy integration

Two independent pieces.

```
omarchy/
├── themed/
│   └── omariglol-theme.toml.tpl   # rendered by Omarchy on every theme switch
├── plugin/                        # Quickshell bar widget
│   ├── manifest.json
│   ├── BarWidget.qml
│   ├── Panel.qml
│   └── Model.js
└── install.sh                     # copies both into place
```

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

Full variable reference: [`docs/omarchy-integration.md`](../docs/omarchy-integration.md).

## Shell plugin

A Quickshell `bar-widget` that measures every instrument in the active
workspace from the top bar.

```bash
omarchy plugin add /path/to/omariglol/omarchy/plugin --enable
```

The plugin does not speak SCPI itself — it shells out to the `riglol` binary.
That keeps the protocol in one place and the QML trivial, which matters because
Omarchy plugins run unsandboxed inside the long-lived shell process.

### What it does

- The bar face shows a glyph and the last run's `measured/total` count.
- **Left click** opens the panel; **middle click** measures all; **right click**
  refreshes.
- The panel has three things:
  1. a **trigger button** that measures every instrument in the workspace at
     once — unless a Python script is configured, in which case it runs the
     script instead;
  2. the **instrument list**, with each instrument's last reading or error;
  3. a small **script button** indicating the loaded Python script (optional).

The trigger calls `riglol measure --json`, which loads the active workspace
from `~/.config/omariglol/workspaces`, connects to every instrument, runs a
per-type default measurement (scope `:MEAS:FREQ?`, DMM `:MEAS:VOLT?`, spectrum
analyzer `:CALC:MARK:Y?`) in parallel, and prints a JSON report.

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
