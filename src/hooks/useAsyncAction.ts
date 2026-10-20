/**
 * The control state machine (UX journey §3.9).
 *
 * Every async control follows the same lifecycle:
 *
 *   idle ──activate──▶ busy ──success──▶ ok
 *                       │
 *                       └──failure──▶ error
 *
 * This hook owns that lifecycle so no control is ever silently inert: a caller
 * gets `busy` to disable itself, `error` to render a translated message, and
 * `reset` to return to idle.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { toCommandError } from "@/lib/api";
import type { CommandError } from "@/lib/types";

export type ActionStatus = "idle" | "busy" | "ok" | "error";

export interface AsyncAction<Args extends unknown[], T> {
  status: ActionStatus;
  /** True while the action is running. */
  busy: boolean;
  /** The last error, translated at the call site. */
  error: CommandError | null;
  /** The last successful result, or `null`. */
  result: T | null;
  /** Run the action. Resolves to the result, or `null` on failure. */
  run: (...args: Args) => Promise<T | null>;
  /** Return to idle and clear the error. */
  reset: () => void;
}

export function useAsyncAction<Args extends unknown[], T>(
  action: (...args: Args) => Promise<T>,
): AsyncAction<Args, T> {
  const [status, setStatus] = useState<ActionStatus>("idle");
  const [error, setError] = useState<CommandError | null>(null);
  const [result, setResult] = useState<T | null>(null);

  // Keep the latest action without making `run` change identity every render,
  // so callers can safely list `run` in their own dependency arrays.
  const actionRef = useRef(action);
  useEffect(() => {
    actionRef.current = action;
  });

  const run = useCallback(async (...args: Args): Promise<T | null> => {
    setStatus("busy");
    setError(null);
    try {
      const value = await actionRef.current(...args);
      setResult(value);
      setStatus("ok");
      return value;
    } catch (err) {
      setError(toCommandError(err));
      setStatus("error");
      return null;
    }
  }, []);

  const reset = useCallback(() => {
    setStatus("idle");
    setError(null);
  }, []);

  return { status, busy: status === "busy", error, result, run, reset };
}
