/**
 * Types mirroring the Rust structs, so the IPC boundary is typed end to end.
 *
 * The multi-instrument platform (concepts.md §4) adds workspaces, instruments,
 * presets, rules, triggers, a shared timeline and a discovery cache. The
 * single-instrument types from 0.1.0 remain for the legacy commands.
 */

/** How an instrument was located. Mirrors `DiscoveryMethod` in Rust. */
export type DiscoveryMethod =
  | "vxi11-broadcast"
  | "mdns"
  | "scpi-tcp"
  | "manual";

/** Which transport to use. Mirrors `Transport` in Rust. */
export type Transport = "vxi11" | "raw" | "usb";

/** The connection lifecycle state of an instrument. */
export type InstrumentState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error"
  | "offline";

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

/** An instrument in a workspace, with derived display fields. */
export interface InstrumentView {
  id: string;
  label: string;
  address: string;
  transport: Transport;
  port?: number;
  state: InstrumentState;
  idn?: string;
  instrument_type: string;
  instrument_glyph: string;
  last_error?: string;
  capabilities: string[];
}

/** A workspace summary for the workspace list. */
export interface WorkspaceSummary {
  id: string;
  name: string;
  instrument_count: number;
  connected_count: number;
  modified_ms: number;
  active: boolean;
}

/** A named batch of SCPI commands, one list per instrument. */
export interface Preset {
  id: string;
  name: string;
  commands: Record<string, string[]>;
  parameters: Record<string, string>;
}

/** How a rule's condition is compared. */
export type Comparison = ">" | ">=" | "<" | "<=" | "==" | "!=";

/** When a rule is evaluated. */
export type EvaluationMode = "continuous" | "on-demand";

/** A conditional routing rule. */
export interface Rule {
  id: string;
  name: string;
  source: string;
  query: string;
  comparison: Comparison;
  threshold: number;
  target: string;
  action: string;
  fallback?: string;
  mode: EvaluationMode;
}

/** The armed state of a trigger. */
export type TriggerState = "disarmed" | "armed" | "fired";

/** A directed dependency edge between two instruments. */
export interface Trigger {
  id: string;
  source: string;
  target: string;
  condition: string;
  action: string;
  state: TriggerState;
  last_fired_ms?: number;
}

/** The full active workspace. */
export interface WorkspaceView {
  id: string;
  name: string;
  instruments: InstrumentView[];
  presets: Preset[];
  rules: Rule[];
  triggers: Trigger[];
  timeline_len: number;
  modified_ms: number;
}

/** A cached discovery result. */
export interface CachedInstrument {
  device: Device;
  idn: string;
  vendor: string;
  model: string;
  serial: string;
  capabilities: string[];
  vlan?: number;
  discovered_ms: number;
  ttl_secs: number;
}

/** A network interface and the subnet it is on. */
export interface InterfaceInfo {
  name: string;
  address: string;
  netmask: string;
  prefix_len: number;
  network: string;
  broadcast: string;
  vlan?: number;
  is_link_local: boolean;
}

/** The detected network configuration. */
export interface NetworkConfig {
  primary?: InterfaceInfo;
  interfaces: InterfaceInfo[];
  gateway?: string;
}

/** The outcome of a discovery run. */
export interface DiscoveryReport {
  config: NetworkConfig;
  found: CachedInstrument[];
  probed_hosts: number;
  skipped_hosts: number;
  elapsed_ms: number;
  strategies: string[];
}

/** A structured event from the discovery engine. */
export type DiscoveryEvent =
  | { kind: "started"; strategies: string[] }
  | { kind: "network-detected"; config: NetworkConfig }
  | { kind: "instrument-found"; instrument: CachedInstrument }
  | { kind: "progress"; probed: number; total: number }
  | { kind: "finished"; found: number; elapsed_ms: number };

/** A named operation a driver exposes. */
export interface DriverCommand {
  name: string;
  scpi: string;
  query: boolean;
  description: string;
  group: string;
}

/** A description of an instrument family's capabilities. */
export interface InstrumentDriver {
  instrument_type: string;
  vendor: string;
  model: string;
  profile?: string;
  screenshot_profile?: string;
  commands: DriverCommand[];
}

/** What kind of thing happened on the timeline. */
export type TimelineEventKind =
  | "command"
  | "query"
  | "error"
  | "preset"
  | "rule"
  | "trigger"
  | "script"
  | "marker";

/** One timestamped event on the shared timeline. */
export interface TimelineEvent {
  timestamp_ms: number;
  instrument_id?: string;
  kind: TimelineEventKind;
  message: string;
  data?: unknown;
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

/**
 * The workbench panels, in navigation order.
 *
 * The app is a workspace-aware workbench: the topology view is the primary
 * navigation surface, and each panel is bound to an instrument or a
 * cross-instrument view (timeline, discovery).
 */
export interface PanelMeta {
  /** DOM id, used as the scroll anchor. */
  id: string;
  /** Label shown in the navigation. */
  label: string;
}

/**
 * Display metadata for the panel navigation.
 *
 * The topology view is the sidebar, not a panel, so it is not listed here.
 */
export const PANELS: ReadonlyArray<PanelMeta> = [
  { id: "workspace", label: "Workspace" },
  { id: "discovery", label: "Discovery" },
  { id: "instrument", label: "Instrument" },
  { id: "scpi", label: "SCPI Console" },
  { id: "screenshot", label: "Screenshot" },
  { id: "benchmark", label: "Benchmark" },
  { id: "timeline", label: "Timeline" },
  { id: "script", label: "Script" },
];

/** The panel ids, as a stable array for the scroll-spy observer. */
export const PANEL_IDS: ReadonlyArray<string> = PANELS.map((p) => p.id);

/** Props every instrument-bound panel receives from the application shell. */
export interface PanelProps {
  /** Scroll to the topology panel; used by the connect prompt. */
  onGoToTopology: () => void;
}
