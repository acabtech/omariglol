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
    todo!("resolve the palette through the fallback chain")
}

/// Parse an Omarchy `colors.toml` into a palette.
pub fn parse_colors_toml(text: &str) -> Option<OmarchyPalette> {
    let _ = text;
    todo!("parse colors.toml, filling gaps from the built-in defaults")
}

/// The palette used when Omarchy is not installed.
pub fn builtin_palette() -> OmarchyPalette {
    todo!("return a neutral dark palette")
}

/// Expose the palette to the frontend as `[key, value]` CSS variable pairs.
///
/// Keys are already `--om-`-prefixed, so the frontend can apply them directly.
pub fn as_css_variables(palette: &OmarchyPalette) -> Vec<(String, String)> {
    let _ = palette;
    todo!("flatten the palette into --om-* custom properties")
}

/// Tauri command: return the resolved Omarchy palette.
#[tauri::command]
pub fn omarchy_palette() -> OmarchyPalette {
    read_palette()
}
