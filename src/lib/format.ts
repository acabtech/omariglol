/**
 * Presentation helpers.
 *
 * Machine data (addresses, SCPI, hex, metrics) is rendered in the monospace
 * font; these helpers only decide *what* the text is, never how it looks.
 */

/** Format a duration in milliseconds with a sensible unit. */
export function formatMs(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  if (ms < 1) return `${(ms * 1000).toFixed(0)} µs`;
  if (ms < 1000) return `${ms.toFixed(3)} ms`;
  return `${(ms / 1000).toFixed(3)} s`;
}

/** Format a byte count with binary units. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

/** Format a rate, e.g. `293.1 msg/s`. */
export function formatRate(value: number, unit: string): string {
  if (!Number.isFinite(value)) return "—";
  return `${value.toFixed(1)} ${unit}`;
}

/** A filesystem-safe timestamp, e.g. `20261020-182301`. */
export function fileTimestamp(date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}` +
    `-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`
  );
}

/** Turn an `*IDN?` string into a short, filesystem-safe label. */
export function idnLabel(idn: string): string {
  const first = idn.split(",")[1] ?? idn.split(",")[0] ?? "instrument";
  return first.trim().replace(/[^A-Za-z0-9._-]+/g, "-") || "instrument";
}

/** True when a string contains characters that are not printable ASCII. */
export function hasNonPrintable(text: string): boolean {
  // eslint-disable-next-line no-control-regex
  return /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/.test(text);
}

/** Number of UTF-8 bytes a string occupies. */
export function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}

/**
 * Render a string as a hex dump.
 *
 * Used by the SCPI console's hex toggle for binary responses. The response
 * crosses the IPC boundary as text, so the dump is of its UTF-8 bytes.
 */
export function toHexDump(text: string, bytesPerLine = 16): string {
  const bytes = new TextEncoder().encode(text);
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += bytesPerLine) {
    const slice = bytes.subarray(offset, offset + bytesPerLine);
    const hex = Array.from(slice)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ");
    const ascii = Array.from(slice)
      .map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "."))
      .join("");
    lines.push(
      `${offset.toString(16).padStart(8, "0")}  ${hex.padEnd(
        bytesPerLine * 3 - 1,
      )}  |${ascii}|`,
    );
  }
  return lines.join("\n");
}
