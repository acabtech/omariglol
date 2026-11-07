//! Omarchy theme bridge.
//!
//! The frontend never names a colour. This module resolves the live Omarchy
//! palette and hands it to the UI as CSS custom properties.
//!
//! ## Where the palette lives
//!
//! Omarchy's theming engine renders `*.tpl` templates from
//! `$OMARCHY_PATH/default/themed/` and `~/.config/omarchy/themed/` into
//!
//! ```text
//! ~/.local/state/omarchy/current/theme/
//! ```
//!
//! so the live palette is always readable from
//!
//! ```text
//! ~/.local/state/omarchy/current/theme/colors.toml
//! ```
//!
//! Reading that file directly is what makes theme switching instant with no
//! setup. A generated `omariglol-theme.toml` (from our own template) is
//! preferred when present, so a theme author can override specific colours.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The resolved palette handed to the frontend.
///
/// Field names mirror Omarchy's `colors.toml` keys and the template variables,
/// so a theme author can predict them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmarchyPalette {
    pub background: String,
    pub foreground: String,
    pub bright_foreground: String,
    pub accent: String,
    pub selection: String,
    pub muted: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub orange: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub brown: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    /// `dark_background`, `darker_background`, `lighter_background` when set.
    pub background_shades: BTreeMap<String, String>,
    /// `color0` through `color15`.
    pub ansi: Vec<String>,
    /// Which source the palette came from, for diagnostics in the About panel.
    pub source: PaletteSource,
    /// Whether the theme declares itself light.
    pub is_light: bool,
}

/// Where a palette was resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaletteSource {
    /// Our generated template output. Highest priority.
    GeneratedTemplate,
    /// Omarchy's live `colors.toml`.
    OmarchyColors,
    /// The `omarchy-theme-color` helper, for unusual environments.
    ThemeColorHelper,
    /// Built-in fallback so the app is usable outside Omarchy.
    Builtin,
}

/// The state directory Omarchy writes the active theme into.
pub fn omarchy_theme_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/state/omarchy/current/theme"))
}

/// Path of the live palette file.
pub fn omarchy_colors_path() -> Option<PathBuf> {
    omarchy_theme_dir().map(|d| d.join("colors.toml"))
}

/// Path of our generated template output, when the template is installed.
pub fn generated_theme_path() -> Option<PathBuf> {
    omarchy_theme_dir().map(|d| d.join("omariglol-theme.toml"))
}

/// Read the palette, following the documented fallback chain:
///
/// 1. generated template output
/// 2. live `colors.toml`
/// 3. `omarchy-theme-color --all`
/// 4. built-in defaults
pub fn read_palette() -> OmarchyPalette {
    // 1. Our generated template output, which a theme author may have tuned.
    if let Some(path) = generated_theme_path()
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Some(mut palette) = parse_colors_toml(&text)
    {
        palette.source = PaletteSource::GeneratedTemplate;
        return palette;
    }

    // 2. Omarchy's live colors.toml — the common case, and what makes theme
    //    switching instant.
    if let Some(path) = omarchy_colors_path()
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Some(palette) = parse_colors_toml(&text)
    {
        return palette;
    }

    // 3. The helper, for environments where the file is not where we expect.
    if let Some(palette) = read_theme_color_helper() {
        return palette;
    }

    // 4. Built-in defaults, so the app is usable outside Omarchy.
    builtin_palette()
}

/// Ask `omarchy-theme-color --all` for the palette.
fn read_theme_color_helper() -> Option<OmarchyPalette> {
    let output = std::process::Command::new("omarchy-theme-color")
        .arg("--all")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut palette = parse_theme_color_output(&text)?;
    palette.source = PaletteSource::ThemeColorHelper;
    Some(palette)
}

/// Parse the helper's output, which may be TOML or `key=value` lines.
fn parse_theme_color_output(text: &str) -> Option<OmarchyPalette> {
    if let Some(palette) = parse_colors_toml(text) {
        return Some(palette);
    }

    // Fall back to `key=value` / `key = value` lines.
    let mut toml = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if !key.is_empty() {
                toml.push_str(&format!("{key} = \"{value}\"\n"));
            }
        }
    }
    if toml.is_empty() {
        return None;
    }
    parse_colors_toml(&toml)
}

/// Parse an Omarchy `colors.toml` into a palette.
///
/// Missing keys are filled from the built-in defaults, so a partial theme file
/// still yields a complete palette.
pub fn parse_colors_toml(text: &str) -> Option<OmarchyPalette> {
    let value: toml::Value = toml::from_str(text).ok()?;
    let table = value.as_table()?;

    let mut palette = builtin_palette();
    palette.source = PaletteSource::OmarchyColors;

    // Our generated template nests colours under `[colors]`, background shades
    // under `[surfaces]`, and the ANSI palette under `[ansi]`, while Omarchy's
    // live `colors.toml` keeps everything at the document root. Look up the
    // root first, then the template's section, so both layouts resolve.
    let get = |key: &str, section: &str| {
        table
            .get(key)
            .and_then(|value| value.as_str())
            .or_else(|| {
                table
                    .get(section)
                    .and_then(|value| value.as_table())
                    .and_then(|section| section.get(key))
                    .and_then(|value| value.as_str())
            })
            .map(str::to_string)
    };

    macro_rules! fill {
        ($($field:ident),* $(,)?) => {
            $(
                if let Some(value) = get(stringify!($field), "colors") {
                    palette.$field = value;
                }
            )*
        };
    }

    fill!(
        background,
        foreground,
        bright_foreground,
        accent,
        selection,
        muted,
        red,
        green,
        yellow,
        orange,
        blue,
        magenta,
        cyan,
        brown,
        bright_red,
        bright_green,
        bright_yellow,
        bright_blue,
        bright_magenta,
        bright_cyan,
    );

    for key in ["dark_background", "darker_background", "lighter_background"] {
        if let Some(value) = get(key, "surfaces") {
            palette.background_shades.insert(key.to_string(), value);
        }
    }

    for (index, slot) in palette.ansi.iter_mut().enumerate() {
        if let Some(value) = get(&format!("color{index}"), "ansi") {
            *slot = value;
        }
    }

    if let Some(value) = table.get("is_light").and_then(|value| value.as_bool()) {
        palette.is_light = value;
    } else if let Some(value) = table.get("mode").and_then(|value| value.as_str()) {
        palette.is_light = value.eq_ignore_ascii_case("light");
    }

    Some(palette)
}

/// The palette used when Omarchy is not installed.
///
/// A neutral dark palette, close to Omarchy's own default, so the app looks at
/// home even without a theme.
pub fn builtin_palette() -> OmarchyPalette {
    OmarchyPalette {
        background: "#1d2021".to_string(),
        foreground: "#d5c4a1".to_string(),
        bright_foreground: "#fbf1c7".to_string(),
        accent: "#83a598".to_string(),
        selection: "#458588".to_string(),
        muted: "#928374".to_string(),
        red: "#fb4934".to_string(),
        green: "#b8bb26".to_string(),
        yellow: "#fabd2f".to_string(),
        orange: "#fe8019".to_string(),
        blue: "#83a598".to_string(),
        magenta: "#d3869b".to_string(),
        cyan: "#8ec07c".to_string(),
        brown: "#d65d0e".to_string(),
        bright_red: "#fb4934".to_string(),
        bright_green: "#b8bb26".to_string(),
        bright_yellow: "#fabd2f".to_string(),
        bright_blue: "#83a598".to_string(),
        bright_magenta: "#d3869b".to_string(),
        bright_cyan: "#8ec07c".to_string(),
        background_shades: BTreeMap::new(),
        ansi: vec![
            "#282828".to_string(),
            "#cc241d".to_string(),
            "#98971a".to_string(),
            "#d79921".to_string(),
            "#458588".to_string(),
            "#b16286".to_string(),
            "#689d6a".to_string(),
            "#a89984".to_string(),
            "#928374".to_string(),
            "#fb4934".to_string(),
            "#b8bb26".to_string(),
            "#fabd2f".to_string(),
            "#83a598".to_string(),
            "#d3869b".to_string(),
            "#8ec07c".to_string(),
            "#ebdbb2".to_string(),
        ],
        source: PaletteSource::Builtin,
        is_light: false,
    }
}

/// Append a `--om-*` custom property.
fn push_var(vars: &mut Vec<(String, String)>, name: &str, value: &str) {
    vars.push((format!("--om-{name}"), value.to_string()));
}

/// Expose the palette to the frontend as `[key, value]` CSS variable pairs.
///
/// Keys are already `--om-`-prefixed, so the frontend can apply them directly.
pub fn as_css_variables(palette: &OmarchyPalette) -> Vec<(String, String)> {
    let mut vars = Vec::new();

    push_var(&mut vars, "background", &palette.background);
    push_var(&mut vars, "foreground", &palette.foreground);
    push_var(&mut vars, "bright-foreground", &palette.bright_foreground);
    push_var(&mut vars, "accent", &palette.accent);
    push_var(&mut vars, "selection", &palette.selection);
    push_var(&mut vars, "muted", &palette.muted);
    push_var(&mut vars, "red", &palette.red);
    push_var(&mut vars, "green", &palette.green);
    push_var(&mut vars, "yellow", &palette.yellow);
    push_var(&mut vars, "orange", &palette.orange);
    push_var(&mut vars, "blue", &palette.blue);
    push_var(&mut vars, "magenta", &palette.magenta);
    push_var(&mut vars, "cyan", &palette.cyan);
    push_var(&mut vars, "brown", &palette.brown);
    push_var(&mut vars, "bright-red", &palette.bright_red);
    push_var(&mut vars, "bright-green", &palette.bright_green);
    push_var(&mut vars, "bright-yellow", &palette.bright_yellow);
    push_var(&mut vars, "bright-blue", &palette.bright_blue);
    push_var(&mut vars, "bright-magenta", &palette.bright_magenta);
    push_var(&mut vars, "bright-cyan", &palette.bright_cyan);

    for (name, value) in &palette.background_shades {
        push_var(&mut vars, &name.replace('_', "-"), value);
    }
    for (index, value) in palette.ansi.iter().enumerate() {
        push_var(&mut vars, &format!("color{index}"), value);
    }

    vars
}

/// Tauri command: return the resolved Omarchy palette.
#[tauri::command]
pub fn omarchy_palette() -> OmarchyPalette {
    read_palette()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_palette_is_complete() {
        let palette = builtin_palette();
        assert_eq!(palette.source, PaletteSource::Builtin);
        assert!(!palette.is_light);
        assert_eq!(palette.ansi.len(), 16);
        assert!(palette.background.starts_with('#'));
    }

    #[test]
    fn test_parse_colors_toml_overrides_and_fills() {
        let text = r##"
            background = "#000000"
            accent = "#ff0000"
            color3 = "#123456"
            is_light = true
        "##;
        let palette = parse_colors_toml(text).unwrap();
        assert_eq!(palette.background, "#000000");
        assert_eq!(palette.accent, "#ff0000");
        assert_eq!(palette.ansi[3], "#123456");
        assert!(palette.is_light);
        // A key the file omits keeps the built-in default.
        assert_eq!(palette.foreground, builtin_palette().foreground);
        assert_eq!(palette.source, PaletteSource::OmarchyColors);
    }

    #[test]
    fn test_parse_colors_toml_nested_template_sections() {
        // Shape of the file rendered from `omariglol-theme.toml.tpl`.
        let text = r##"
            mode = "light"

            [colors]
            background = "#000000"
            accent = "#ff0000"
            bright_foreground = "#ffffff"

            [surfaces]
            dark_background = "#010101"

            [ansi]
            color3 = "#123456"
        "##;
        let palette = parse_colors_toml(text).unwrap();
        assert_eq!(palette.background, "#000000");
        assert_eq!(palette.accent, "#ff0000");
        assert_eq!(palette.bright_foreground, "#ffffff");
        assert_eq!(palette.ansi[3], "#123456");
        assert_eq!(
            palette.background_shades.get("dark_background"),
            Some(&"#010101".to_string())
        );
        assert!(palette.is_light);
        assert_ne!(palette.background, builtin_palette().background);
    }

    #[test]
    fn test_parse_colors_toml_background_shades() {
        let text = r##"
            background = "#000000"
            dark_background = "#010101"
            lighter_background = "#020202"
        "##;
        let palette = parse_colors_toml(text).unwrap();
        assert_eq!(
            palette.background_shades.get("dark_background"),
            Some(&"#010101".to_string())
        );
        assert_eq!(
            palette.background_shades.get("lighter_background"),
            Some(&"#020202".to_string())
        );
    }

    #[test]
    fn test_parse_colors_toml_rejects_garbage() {
        assert!(parse_colors_toml("this is not toml = = =").is_none());
    }

    #[test]
    fn test_parse_theme_color_output_key_value() {
        let text = "background=#101010\naccent = #abcdef\n";
        let palette = parse_theme_color_output(text).unwrap();
        assert_eq!(palette.background, "#101010");
        assert_eq!(palette.accent, "#abcdef");
    }

    #[test]
    fn test_as_css_variables() {
        let palette = builtin_palette();
        let vars = as_css_variables(&palette);
        let map: std::collections::HashMap<_, _> = vars.iter().cloned().collect();
        assert_eq!(
            map.get("--om-background"),
            Some(&palette.background.clone())
        );
        assert_eq!(
            map.get("--om-bright-foreground"),
            Some(&palette.bright_foreground.clone())
        );
        assert_eq!(map.get("--om-color0"), Some(&palette.ansi[0].clone()));
        assert_eq!(map.get("--om-color15"), Some(&palette.ansi[15].clone()));
    }
}
