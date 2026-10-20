/**
 * Error translation contract (UX journey §6.6).
 *
 * Raw error codes are never shown alone. Every `CommandError.kind` maps to a
 * sentence a novice can act on, plus a recovery action where one exists. The
 * `kind` values mirror the `From<Error>` / `From<ScriptError>` impls in
 * `src-tauri/src/commands.rs`.
 */

import type { CommandError } from "@/lib/types";

/** A user-facing sentence plus an optional recovery action. */
export interface TranslatedError {
  /** The sentence shown to the user. */
  message: string;
  /** What the user can do next, when there is something to do. */
  recovery?: string;
}

const TRANSLATIONS: Record<string, TranslatedError> = {
  timeout: {
    message:
      "The instrument did not respond in time. It may be busy or unreachable.",
    recovery: "Try a longer timeout, or check the instrument is powered on.",
  },
  addr: {
    message:
      "That address or port is not valid. Check the format, e.g. 192.168.1.50.",
    recovery: "Correct the address and try again.",
  },
  vxi11: {
    message: "The instrument reported an error.",
    recovery: "Check the command and the instrument's state.",
  },
  "not-found": {
    message: "No instrument answered at that address.",
    recovery: "Check the address and that the instrument is on this subnet.",
  },
  malformed: {
    message: "The instrument sent a response we could not read.",
    recovery: "Retry the operation.",
  },
  "screenshot-unsupported": {
    message: "This instrument does not support screen capture.",
    recovery: "Enter a manual capture command, or use another instrument.",
  },
  unsupported: {
    message: "That operation is not supported by this instrument.",
  },
  rpc: {
    message: "Could not reach the instrument's VXI-11 service.",
    recovery: "Check the connection and try again.",
  },
  portmap: {
    message: "Could not reach the instrument's VXI-11 service.",
    recovery: "Check the connection and try again.",
  },
  io: {
    message:
      "A network or file error occurred. Check the connection and try again.",
  },
  "no-session": {
    message: "No instrument connected.",
    recovery: "Open one from Search.",
  },
  python: {
    message: "The script raised a Python exception.",
    recovery: "See the traceback in the console.",
  },
  cancelled: {
    message: "The operation was cancelled.",
  },
  interpreter: {
    message: "The Python interpreter is unavailable.",
    recovery: "Install CPython 3.10 or newer and try again.",
  },
  internal: {
    message: "Something went wrong inside the app.",
    recovery: "Retry; if it persists, restart omariglol.",
  },
  other: {
    message: "Something went wrong.",
  },
  unknown: {
    message: "Something went wrong.",
  },
};

/**
 * Translate a `CommandError` into a sentence and recovery action.
 *
 * For `vxi11` and `other` the raw message carries the instrument's own words,
 * so it is appended rather than discarded.
 */
export function translateError(error: CommandError): TranslatedError {
  const known = TRANSLATIONS[error.kind];
  if (!known) {
    return { message: error.message || "Something went wrong." };
  }
  if ((error.kind === "vxi11" || error.kind === "other") && error.message) {
    return { ...known, message: `${known.message} ${error.message}`.trim() };
  }
  return known;
}
