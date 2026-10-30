# Omarchy integration

`omariglol` should look like it shipped with Omarchy. That means two things:
it inherits the active theme, and it can be driven from the shell.

## 1. Theme inheritance

### How Omarchy theming actually works

A theme is a `colors.toml` file. On theme switch, Omarchy's
`omarchy-theme-set-templates` script walks every `*.tpl` file in

- `$OMARCHY_PATH/default/themed/` (shipped), then
- `~/.config/omarchy/themed/` (user, **takes priority**)

and renders each one into

```
~/.local/state/omarchy/current/theme/<name-without-.tpl>
```

The live palette is therefore always readable at:

```
~/.local/state/omarchy/current/theme/colors.toml
```

### Template variables

| Variable | Meaning |
| --- | --- |
| `{{ background }}` | main background |
| `{{ foreground }}` | main text |
| `{{ bright_foreground }}` | bright text / cursor |
| `{{ accent }}` | theme accent |
| `{{ selection }}` | selection highlight |
| `{{ muted }}` | de-emphasised text |
| `{{ red }} {{ green }} {{ yellow }} {{ blue }} {{ magenta }} {{ cyan }} {{ orange }} {{ brown }}` | semantic colours |
| `{{ bright_* }}` | bright variants of the above |
| `{{ color0 }}` … `{{ color15 }}` | the raw 16-colour terminal palette |
| `{{ dark_background }} {{ darker_background }} {{ lighter_background }}` | background shades |
| `{{ dark_foreground }} {{ light_foreground }}` | foreground shades |

Modifiers:

- `{{ background_strip }}` → `1a1b26` (no `#`)
- `{{ background_rgb }}` → `26,27,38` (decimal, for `rgb(...)`)

### What omariglol ships

`omarchy/themed/omariglol-theme.toml.tpl` is a **user template**. Installing it
into `~/.config/omarchy/themed/` makes Omarchy generate an `omariglol-theme.toml`
on every theme switch, which the app then reads.

```toml
[colors]
background = "{{ background }}"
foreground = "{{ foreground }}"
accent     = "{{ accent }}"
```

### Fallback chain

Because templates only regenerate on theme switch, the app must not depend on
the generated file existing. `theme/omarchy.ts` resolves the palette in order:

1. `~/.local/state/omarchy/current/theme/omariglol-theme.toml` (template output)
2. `~/.local/state/omarchy/current/theme/colors.toml` (live palette, parsed directly)
3. `omarchy-theme-color --all` (shell helper, for odd environments)
4. Built-in defaults

Level 2 is what makes theme switching instant with no setup at all. Level 1
exists so that a theme author can override specific omariglol colours.

## 2. Shell plugin

### How Omarchy plugins work

The desktop is one long-lived Quickshell process (`omarchy-shell`) and
essentially everything on screen is a plugin inside it.

- First-party plugins: `$OMARCHY_PATH/shell/plugins/`
- Third-party plugins: `~/.config/omarchy/plugins/<id>/`
- A plugin is **a git repo with a `manifest.json` at its root**
- Plugins are QML and run **unsandboxed** with your user permissions

Plugin kinds and their entry points:

| Kind | `entryPoints` key | File | Used for |
| --- | --- | --- | --- |
| `bar-widget` | `barWidget` | `BarWidget.qml` | an item in the top bar |
| `panel` | `panel` | `Panel.qml` | a floating surface |
| `overlay` | `overlay` | `Overlay.qml` | fullscreen surface |
| `menu` | `menu` | `Menu.qml` | a summoned menu |
| `service` | `service` | `Service.qml` | a headless singleton |
| `bar` | `bar` | `Bar.qml` | full bar replacement |

Third-party plugin ids are namespaced; the `omarchy.` prefix is reserved. Ours
is `acabtech.omariglol`.

### Ours

`omarchy/plugin/` is a `bar-widget` plugin: a top-bar item showing how many
instruments the last run measured, with a panel that lists the workspace's
instruments and fires a simultaneous measurement of all of them.

Install with:

```bash
omarchy plugin add /path/to/omariglol/omarchy/plugin --enable
# or, once the repo is public:
omarchy plugin add https://github.com/acabtech/omariglol.git --enable
```

The panel has three things:

1. a **trigger button** that measures every instrument in the active workspace
   at once — unless a Python script is configured, in which case it runs the
   script instead;
2. the **instrument list**, with each instrument's last reading or error;
3. a small **script button** indicating the loaded Python script (optional).

The plugin does **not** speak SCPI itself. It shells out to the `riglol`
binary, keeping the protocol in one place and the QML trivial. The trigger runs
`riglol measure --json`, which loads the active workspace from
`~/.config/omariglol/workspaces`, connects to every instrument, runs a per-type
default measurement in parallel, and prints a JSON report. Each instrument is
measured according to its `*IDN?` type: a scope answers `:MEAS:FREQ?`, a DMM
`:MEAS:VOLT?`, a spectrum analyzer `:CALC:MARK:Y?`, and anything else is probed
for liveness with `*IDN?`. Passing `--command` overrides the default for every
instrument.

### Safety note

Omarchy tells you plainly before installing a plugin that plugins run as
arbitrary unsandboxed code. Ours should stay small and obvious for exactly that
reason: no network code, no dynamic evaluation — just a `Process` call to the
CLI and a UI to show the answer.
