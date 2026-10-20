/** Types mirroring the Rust structs, so the IPC boundary is typed end to end. */

/** How an instrument was located. Mirrors `DiscoveryMethod` in Rust. */
export type DiscoveryMethod = "vxi11-broadcast" | "mdns" | "manual";

/** Which transport to use. Mirrors `Transport` in Rust. */
export type Transport = "vxi11" | "raw";

/** A discovered or manually configured LXI instrument. */
export interface Device {
  address: string;
  hostname?: string;
  vxi11_port?: number;
  raw_port?: number;
  mdns_name?: string;
  device_name: string;
  method: DiscoveryMethod;
}

/** One benchmark progress sample, emitted as `benchmark:sample`. */
export interface BenchmarkSample {
  iteration: number;
  sample_ms: number;
}

/** Statistics from one benchmark run. Mirrors `BenchmarkReport` in Rust. */
export interface BenchmarkReport {
  command: string;
  iterations: number;
  failures: number;
  elapsed_ms: number;
  min_ms: number;
  max_ms: number;
  mean_ms: number;
  median_ms: number;
  p95_ms: number;
  stddev_ms: number;
  messages_per_second: number;
  bytes_received: number;
}

/** A captured screenshot. Mirrors `Screenshot` in Rust. */
export interface Screenshot {
  /** Base64 image data, supplied by the Rust side. */
  data: string;
  format: "png" | "bmp" | "jpeg" | "unknown";
  idn: string;
  profile: string;
}

/** Error returned by a Tauri command. */
export interface CommandError {
  kind: string;
  message: string;
}

/** A progress event emitted while a script runs. */
export type ScriptEvent =
  | { kind: "stdout"; text: string }
  | { kind: "stderr"; text: string }
  | { kind: "sample"; series: string; value: number }
  | { kind: "status"; message: string };

/** The GUI modes, matching lxi-tools' five. */
export type Mode = "search" | "scpi" | "screenshot" | "benchmark" | "script";

/** Display metadata for the mode switcher. */
export const MODES: ReadonlyArray<{ id: Mode; label: string }> = [
  { id: "search", label: "Search" },
  { id: "scpi", label: "SCPI" },
  { id: "screenshot", label: "Screenshot" },
  { id: "benchmark", label: "Benchmark" },
  { id: "script", label: "Script" },
];
