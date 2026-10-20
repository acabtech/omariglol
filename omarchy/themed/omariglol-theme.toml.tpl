# omariglol theme for Omarchy.
#
# This is a *user template*. Omarchy's theming engine renders it into
# ~/.local/state/omarchy/current/theme/omariglol-theme.toml on every theme
# switch, substituting each {{ ... }} placeholder from the active theme's
# colors.toml.
#
# Install with:
#     cp omariglol-theme.toml.tpl ~/.config/omarchy/themed/
#
# Available variables: background, foreground, bright_foreground, accent,
# selection, muted, red, green, yellow, orange, blue, magenta, cyan, brown,
# the bright_* variants, color0..color15, and the background/foreground shades.
#
# Modifiers: {{ name_strip }} drops the leading '#', {{ name_rgb }} emits
# decimal "r,g,b" for rgb() style consumers.

mode = "{{ mode }}"

[colors]
background        = "{{ background }}"
foreground        = "{{ foreground }}"
bright_foreground = "{{ bright_foreground }}"
accent            = "{{ accent }}"
selection         = "{{ selection }}"
muted             = "{{ muted }}"

# Semantic colours, used for status and chart series.
red     = "{{ red }}"
green   = "{{ green }}"
yellow  = "{{ yellow }}"
orange  = "{{ orange }}"
blue    = "{{ blue }}"
magenta = "{{ magenta }}"
cyan    = "{{ cyan }}"
brown   = "{{ brown }}"

bright_red     = "{{ bright_red }}"
bright_green   = "{{ bright_green }}"
bright_yellow  = "{{ bright_yellow }}"
bright_blue    = "{{ bright_blue }}"
bright_magenta = "{{ bright_magenta }}"
bright_cyan    = "{{ bright_cyan }}"

# Surfaces, derived from the background shades so panels sit correctly.
[surfaces]
dark_background    = "{{ dark_background }}"
darker_background  = "{{ darker_background }}"
lighter_background = "{{ lighter_background }}"

# Chart series, in a fixed order so plots stay readable across themes.
[charts]
series = [
  "{{ accent }}",
  "{{ cyan }}",
  "{{ yellow }}",
  "{{ magenta }}",
  "{{ green }}",
  "{{ red }}",
  "{{ blue }}",
  "{{ orange }}",
]
grid  = "{{ muted }}"
axis  = "{{ foreground }}"

# The raw 16-colour ANSI palette, for the SCPI console.
[ansi]
color0  = "{{ color0 }}"
color1  = "{{ color1 }}"
color2  = "{{ color2 }}"
color3  = "{{ color3 }}"
color4  = "{{ color4 }}"
color5  = "{{ color5 }}"
color6  = "{{ color6 }}"
color7  = "{{ color7 }}"
color8  = "{{ color8 }}"
color9  = "{{ color9 }}"
color10 = "{{ color10 }}"
color11 = "{{ color11 }}"
color12 = "{{ color12 }}"
color13 = "{{ color13 }}"
color14 = "{{ color14 }}"
color15 = "{{ color15 }}"
