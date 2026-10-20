/**
 * Omarchy theme bridge (frontend).
 *
 * The single place in the UI that knows where colours come from. Every
 * component reads `--om-*` custom properties; this module is the only thing
 * that writes them.
 *
 * Palette resolution happens in Rust (`src-tauri/src/theme.rs`) because it has
 * to read files outside the webview sandbox. See `docs/omarchy-integration.md`
 * for the fallback chain.
 */

import { invoke } from "@tauri-apps/api/core";

/** Palette shape returned by the `omarchy_palette` command. */
export interface OmarchyPalette {
  background: string;
  foreground: string;
  bright_foreground: string;
  accent: string;
  selection: string;
  muted: string;
  red: string;
  green: string;
  yellow: string;
  orange: string;
  blue: string;
  magenta: string;
  cyan: string;
  brown: string;
  bright_red: string;
  bright_green: string;
  bright_yellow: string;
  bright_blue: string;
  bright_magenta: string;
  bright_cyan: string;
  background_shades: Record<string, string>;
  ansi: string[];
  source: PaletteSource;
  is_light: boolean;
}

export type PaletteSource =
  | "generated-template"
  | "omarchy-colors"
  | "theme-color-helper"
  | "builtin";

/**
 * Map a palette field to its CSS custom property.
 *
 * Snake_case keys become kebab-case properties, so `bright_foreground`
 * becomes `--om-bright-foreground`.
 */
export function paletteToCssVariables(
  palette: OmarchyPalette,
): Record<string, string> {
  const variables: Record<string, string> = {};

  for (const [key, value] of Object.entries(palette)) {
    if (typeof value !== "string") continue;
    variables[`--om-${key.replace(/_/g, "-")}`] = value;
  }

  palette.ansi.forEach((colour, index) => {
    variables[`--om-color${index}`] = colour;
  });

  for (const [name, value] of Object.entries(palette.background_shades)) {
    variables[`--om-${name.replace(/_/g, "-")}`] = value;
  }

  return variables;
}

/** Apply a palette to the document root. */
export function applyPalette(palette: OmarchyPalette): void {
  const root = document.documentElement;
  for (const [property, value] of Object.entries(
    paletteToCssVariables(palette),
  )) {
    root.style.setProperty(property, value);
  }
  root.dataset.omarchySource = palette.source;
  root.dataset.omarchyMode = palette.is_light ? "light" : "dark";
}

/** Fetch the resolved palette from the Rust side. */
export async function fetchPalette(): Promise<OmarchyPalette> {
  return invoke<OmarchyPalette>("omarchy_palette");
}

/**
 * Resolve and apply the palette, returning it.
 *
 * Errors are swallowed deliberately: a broken theme read must not stop the app
 * from starting, because the stylesheet already carries a usable fallback.
 */
export async function initialiseTheme(): Promise<OmarchyPalette | null> {
  try {
    const palette = await fetchPalette();
    applyPalette(palette);
    return palette;
  } catch (error) {
    console.warn("could not read the Omarchy palette, using fallback", error);
    return null;
  }
}

/**
 * Watch for theme changes.
 *
 * Omarchy rewrites `colors.toml` on every theme switch. Watching the file
 * directly needs a filesystem watcher on the Rust side; until that exists this
 * polls, which is cheap because the read is a single small file.
 *
 * Returns a function that stops the watcher.
 */
export function watchTheme(
  onChange: (palette: OmarchyPalette) => void,
  intervalMs = 2000,
): () => void {
  let lastAccent: string | null = null;
  let stopped = false;

  const tick = async () => {
    if (stopped) return;
    try {
      const palette = await fetchPalette();
      if (palette.accent !== lastAccent) {
        lastAccent = palette.accent;
        applyPalette(palette);
        onChange(palette);
      }
    } catch {
      // Ignore transient failures; the next tick retries.
    }
    if (!stopped) {
      window.setTimeout(tick, intervalMs);
    }
  };

  void tick();

  return () => {
    stopped = true;
  };
}
