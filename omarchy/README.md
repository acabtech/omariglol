# Omarchy integration

Two independent pieces, both scaffolded here.

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

A Quickshell `bar-widget` that puts an instrument in the top bar.

```bash
omarchy plugin add /path/to/omariglol/omarchy/plugin --enable
```

The plugin does not speak SCPI itself — it shells out to the `omariglol`
binary. That keeps the protocol in one place and the QML trivial, which matters
because Omarchy plugins run unsandboxed inside the long-lived shell process.

### Status

The QML here is a **skeleton**. Omarchy exposes a different interface to
plugins depending on their kind, and third-party plugins get a reduced one.
Before implementing, clone a built-in of the same kind to copy the contract
exactly:

```bash
omarchy plugin clone omarchy.clock --edit
```

Then mirror its structure. See the manual's shell plugins chapter for the
plugin-kind reference table.
