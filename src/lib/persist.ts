/**
 * Local persistence for the last-used instrument and window geometry.
 *
 * Everything is best-effort: a missing or corrupt entry, or a webview with
 * storage disabled, must never stop the app from starting. All reads return
 * `null` on any failure.
 */

import type { Transport } from "@/lib/types";

const KEY_INSTRUMENT = "omariglol:last-instrument";
const KEY_GEOMETRY = "omariglol:window-geometry";

/** The instrument the user last connected to, used to prefill Search. */
export interface PersistedInstrument {
  address: string;
  transport: Transport;
  port?: number;
  timeoutSecs: number;
  idn: string;
}

/** A window position and size, in logical pixels. */
export interface WindowGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

function read<T>(key: string): T | null {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage may be unavailable; persistence is a convenience, not a need.
  }
}

function remove(key: string): void {
  try {
    window.localStorage.removeItem(key);
  } catch {
    // Ignore.
  }
}

/** Load the last-used instrument, or `null`. */
export function loadLastInstrument(): PersistedInstrument | null {
  const value = read<PersistedInstrument>(KEY_INSTRUMENT);
  if (!value || typeof value.address !== "string") return null;
  return value;
}

/** Remember the instrument the user just connected to. */
export function saveLastInstrument(instrument: PersistedInstrument): void {
  write(KEY_INSTRUMENT, instrument);
}

/** Forget the last-used instrument. */
export function clearLastInstrument(): void {
  remove(KEY_INSTRUMENT);
}

/** Load the last window geometry, or `null`. */
export function loadWindowGeometry(): WindowGeometry | null {
  const value = read<WindowGeometry>(KEY_GEOMETRY);
  if (
    !value ||
    typeof value.x !== "number" ||
    typeof value.y !== "number" ||
    typeof value.width !== "number" ||
    typeof value.height !== "number"
  ) {
    return null;
  }
  return value;
}

/** Remember the current window geometry. */
export function saveWindowGeometry(geometry: WindowGeometry): void {
  write(KEY_GEOMETRY, geometry);
}
