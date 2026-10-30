// omariglol — bar widget state and formatting.
//
// Pure functions only: no I/O, no process launching, no state beyond what is
// passed in. The QML stays declarative and the `Process` calls live in
// Panel.qml. See plans/omarchy-integration.md §4.3.
//
// QML's JS engine is not an ES module loader: top-level `var` and `function`
// declarations are exposed through `import "Model.js" as Model`, and `export`
// is a syntax error. Keep this file free of `export`.

// Nerd Font glyphs. Written as escapes so the file stays plain ASCII.
var GLYPH = "\uF201"; // nf-fa-chart_line — the widget's own mark
var GLYPH_BOLT = "\uF0E7"; // nf-fa-bolt
var GLYPH_GAUGE = "\uF0E4"; // nf-fa-tachometer
var GLYPH_SIGNAL = "\uF012"; // nf-fa-signal
var GLYPH_PLUG = "\uF1E6"; // nf-fa-plug
var GLYPH_UNKNOWN = "\uF128"; // nf-fa-question

/** The argv for `riglol measure --json`. */
function measureCommand(binary, workspace, command) {
  var args = [binary || "riglol", "measure", "--json"];
  if (workspace) args.push("--workspace", String(workspace));
  if (command) args.push("--command", String(command));
  return args;
}

/** The argv for `riglol run <script>`. */
function scriptCommand(binary, path) {
  return [binary || "riglol", "run", String(path)];
}

/**
 * Parse a `riglol measure --json` report.
 *
 * Returns the report object, or null when the text is not a report (a CLI
 * error, a partial write, an empty stream).
 */
function parseReport(text) {
  var raw = String(text || "").trim();
  if (!raw) return null;
  try {
    var parsed = JSON.parse(raw);
    if (!parsed || !Array.isArray(parsed.results)) return null;
    return parsed;
  } catch (e) {
    return null;
  }
}

/** The first non-empty line of a blob of text, for error messages. */
function firstLine(text) {
  var lines = String(text || "").split("\n");
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();
    if (line) return line;
  }
  return "";
}

/** The basename of a path, for the script button. */
function scriptName(path) {
  var value = String(path || "");
  if (!value) return "";
  var parts = value.split("/");
  return parts[parts.length - 1] || value;
}

/** Format a number with an SI prefix, e.g. 1234567 -> "1.23 M". */
function formatMeasurement(value, unit) {
  var n = Number(value);
  if (!isFinite(n)) return String(value);
  if (n === 0) return "0 " + (unit || "");

  var abs = Math.abs(n);
  var prefixes = [
    [1e9, "G"],
    [1e6, "M"],
    [1e3, "k"],
    [1, ""],
    [1e-3, "m"],
    [1e-6, "\u00B5"],
    [1e-9, "n"],
  ];
  for (var i = 0; i < prefixes.length; i++) {
    if (abs >= prefixes[i][0] || i === prefixes.length - 1) {
      var scaled = n / prefixes[i][0];
      var magnitude = Math.abs(scaled);
      var text =
        magnitude >= 100
          ? scaled.toFixed(0)
          : magnitude >= 10
            ? scaled.toFixed(1)
            : scaled.toFixed(2);
      return text + " " + prefixes[i][1] + (unit || "");
    }
  }
  return String(value);
}

/** The glyph for an instrument type (kebab-case, as the CLI emits it). */
function instrumentGlyph(type) {
  switch (String(type || "")) {
    case "oscilloscope":
    case "spectrum-analyzer":
      return GLYPH;
    case "multimeter":
      return GLYPH_GAUGE;
    case "power-supply":
    case "electronic-load":
      return GLYPH_BOLT;
    case "signal-generator":
      return GLYPH_SIGNAL;
    case "switch-module":
      return GLYPH_PLUG;
    default:
      return GLYPH_UNKNOWN;
  }
}

/** The value cell for one result: the reading, or the error. */
function resultValue(result) {
  if (!result) return "-";
  if (result.error) return "! " + result.error;
  if (result.value === undefined || result.value === null || result.value === "")
    return "-";
  return String(result.value);
}

/** A one-line summary of a report, e.g. "3 measured · 1 failed". */
function summarize(report) {
  if (!report || !Array.isArray(report.results)) return "";
  var total = report.results.length;
  var measured = 0;
  var failed = 0;
  for (var i = 0; i < total; i++) {
    var r = report.results[i];
    if (r.error) failed++;
    else if (r.is_measurement) measured++;
  }
  var parts = [measured + " measured"];
  if (failed > 0) parts.push(failed + " failed");
  parts.push(total + " total");
  return parts.join(" \u00B7 ");
}

/** Visual state for the widget: "idle" | "measuring" | "error". */
function widgetState(measuring, lastError) {
  if (lastError) return "error";
  if (measuring) return "measuring";
  return "idle";
}

/**
 * The compact bar label.
 *
 * A glyph plus the measured/total count once a report exists, a glyph plus an
 * ellipsis while measuring, a glyph plus a bang on error, and the bare glyph
 * before the first run.
 */
function barLabel(report, measuring, lastError) {
  if (lastError) return GLYPH + " !";
  if (measuring) return GLYPH + " \u2026";
  if (!report || !Array.isArray(report.results)) return GLYPH;
  var total = report.results.length;
  var measured = 0;
  for (var i = 0; i < total; i++) {
    if (report.results[i].is_measurement && !report.results[i].error) measured++;
  }
  return GLYPH + " " + measured + "/" + total;
}
