/**
 * Typed wrappers over the Tauri IPC commands.
 *
 * Components import from here rather than calling `invoke` directly, so the
 * command names and argument shapes live in exactly one place.
 *
 * Long-running commands stream progress through Tauri events; the `on*`
 * helpers below subscribe to them.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BenchmarkReport,
  BenchmarkSample,
  CommandError,
  Device,
  Screenshot,
  ScriptEvent,
  Transport,
} from "@/lib/types";

/** The application version. */
export function version(): Promise<string> {
  return invoke<string>("version");
}

/** Find instruments on the network. */
export function discover(
  mdns: boolean,
  timeoutSecs = 3,
): Promise<Device[]> {
  return invoke<Device[]>("discover", {
    request: { mdns, timeout_secs: timeoutSecs },
  });
}

/** Open a session and return the instrument's `*IDN?` string. */
export function connect(
  address: string,
  transport: Transport,
  port?: number,
  timeoutSecs = 3,
): Promise<string> {
  return invoke<string>("connect", {
    request: { address, transport, port: port ?? null, timeout_secs: timeoutSecs },
  });
}

/** Close the current session. */
export function disconnect(): Promise<void> {
  return invoke<void>("disconnect");
}

/** Send a SCPI query and return the response text. */
export function scpiQuery(command: string): Promise<string> {
  return invoke<string>("scpi_query", { command });
}

/** Send a SCPI command that produces no response. */
export function scpiWrite(command: string): Promise<void> {
  return invoke<void>("scpi_write", { command });
}

/** Capture the instrument's screen. */
export function captureScreenshot(
  command?: string,
): Promise<Screenshot> {
  return invoke<Screenshot>("capture_screenshot", {
    request: { command: command ?? null },
  });
}

/** Run a benchmark. Progress arrives via `benchmark:sample` events. */
export function runBenchmark(
  command = "*IDN?",
  iterations = 100,
): Promise<BenchmarkReport> {
  return invoke<BenchmarkReport>("run_benchmark", {
    request: { command, iterations },
  });
}

/** Execute a Python measurement script. Progress via `script:event`. */
export function runScript(source: string, path?: string): Promise<void> {
  return invoke<void>("run_script", {
    request: { source, path: path ?? null },
  });
}

/** Ask a running script to stop. */
export function cancelScript(): Promise<void> {
  return invoke<void>("cancel_script");
}

/**
 * Subscribe to incremental discovery results.
 *
 * Returns an unlisten function; call it to stop receiving events.
 */
export function onDiscoverDevice(
  handler: (device: Device) => void,
): Promise<UnlistenFn> {
  return listen<Device>("discover:device", (event) => handler(event.payload));
}

/** Subscribe to live benchmark progress samples. */
export function onBenchmarkSample(
  handler: (sample: BenchmarkSample) => void,
): Promise<UnlistenFn> {
  return listen<BenchmarkSample>("benchmark:sample", (event) =>
    handler(event.payload),
  );
}

/** Subscribe to script output, samples and status updates. */
export function onScriptEvent(
  handler: (event: ScriptEvent) => void,
): Promise<UnlistenFn> {
  return listen<ScriptEvent>("script:event", (event) => handler(event.payload));
}

/** Narrow an unknown thrown value to a `CommandError`. */
export function toCommandError(error: unknown): CommandError {
  if (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error
  ) {
    return error as CommandError;
  }
  return { kind: "unknown", message: String(error) };
}
