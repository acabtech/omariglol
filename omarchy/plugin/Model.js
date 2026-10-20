// omariglol — bar widget state and formatting.
//
// STATUS: skeleton.
//
// Kept as plain JS so the QML stays declarative. Everything here is pure: no
// I/O, no process launching, no state beyond what is passed in.
//
// Intended contents:
//   - reading the saved instrument list from the plugin's settings
//   - formatting a measurement result for the bar (value + unit, SI prefixes)
//   - deciding the widget's visual state (idle / measuring / error)

/** Format a measurement with an SI prefix, e.g. 1234567 -> "1.23 MHz". */
export function formatMeasurement(value, unit) {
  if (!Number.isFinite(value)) return "--";
  return `${value} ${unit}`;
}

/** Short label for the bar, kept to a few characters. */
export function barLabel(instrument) {
  if (!instrument) return "LXI";
  return instrument;
}

/** Visual state for the widget: "idle" | "measuring" | "error". */
export function widgetState({ measuring, lastError }) {
  if (lastError) return "error";
  if (measuring) return "measuring";
  return "idle";
}
