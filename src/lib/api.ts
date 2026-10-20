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
  CachedInstrument,
  CommandError,
  Device,
  DiscoveryEvent,
  DiscoveryReport,
  InstrumentDriver,
  InstrumentView,
  NetworkConfig,
  Screenshot,
  ScriptEvent,
  TimelineEvent,
  Transport,
  WorkspaceSummary,
  WorkspaceView,
} from "@/lib/types";

/* -------------------------------------------------------------------------
 * Version and theme
 * ---------------------------------------------------------------------- */

/** The application version. */
export function version(): Promise<string> {
  return invoke<string>("version");
}

/* -------------------------------------------------------------------------
 * Discovery
 * ---------------------------------------------------------------------- */

/** Find instruments with the lightweight broadcast/mDNS path. */
export function discover(
  mdns: boolean,
  timeoutSecs = 3,
): Promise<Device[]> {
  return invoke<Device[]>("discover", {
    request: { mdns, timeout_secs: timeoutSecs },
  });
}

/** Arguments for the full discovery engine. */
export interface DiscoveryScanArgs {
  useBroadcast?: boolean;
  useMdns?: boolean;
  scanSubnet?: boolean;
  extraHosts?: string[];
  timeoutSecs?: number;
  ttlSecs?: number;
  incremental?: boolean;
}

/** Run the multi-strategy discovery engine. */
export function discoveryScan(
  args: DiscoveryScanArgs = {},
): Promise<DiscoveryReport> {
  return invoke<DiscoveryReport>("discovery_scan", {
    request: {
      use_broadcast: args.useBroadcast ?? true,
      use_mdns: args.useMdns ?? true,
      scan_subnet: args.scanSubnet ?? true,
      extra_hosts: args.extraHosts ?? [],
      timeout_secs: args.timeoutSecs ?? 3,
      ttl_secs: args.ttlSecs ?? 300,
      incremental: args.incremental ?? true,
    },
  });
}

/** Every cached discovery result, fresh or not. */
export function discoveryCache(): Promise<CachedInstrument[]> {
  return invoke<CachedInstrument[]>("discovery_cache");
}

/** Clear the discovery cache. */
export function discoveryInvalidate(): Promise<void> {
  return invoke<void>("discovery_invalidate");
}

/** Detect the current network configuration. */
export function networkConfig(): Promise<NetworkConfig> {
  return invoke<NetworkConfig>("network_config");
}

/* -------------------------------------------------------------------------
 * Workspace management
 * ---------------------------------------------------------------------- */

/** Create a new workspace and make it active. */
export function workspaceCreate(name: string): Promise<WorkspaceView> {
  return invoke<WorkspaceView>("workspace_create", { name });
}

/** List all saved workspaces. */
export function workspaceList(): Promise<WorkspaceSummary[]> {
  return invoke<WorkspaceSummary[]>("workspace_list");
}

/** Open a workspace, making it active. */
export function workspaceOpen(id: string): Promise<WorkspaceView> {
  return invoke<WorkspaceView>("workspace_open", { id });
}

/** Persist the active workspace. */
export function workspaceSave(): Promise<void> {
  return invoke<void>("workspace_save");
}

/** Delete a workspace. */
export function workspaceDelete(id: string): Promise<void> {
  return invoke<void>("workspace_delete", { id });
}

/** Return the active workspace, creating a default one if none exists. */
export function workspaceGet(): Promise<WorkspaceView> {
  return invoke<WorkspaceView>("workspace_get");
}

/* -------------------------------------------------------------------------
 * Instrument management
 * ---------------------------------------------------------------------- */

/** Arguments for adding an instrument. */
export interface AddInstrumentArgs {
  address: string;
  transport: Transport;
  port?: number;
  timeoutSecs?: number;
}

/** Add an instrument to the active workspace. */
export function workspaceAddInstrument(
  args: AddInstrumentArgs,
): Promise<InstrumentView> {
  return invoke<InstrumentView>("workspace_add_instrument", {
    request: {
      address: args.address,
      transport: args.transport,
      port: args.port ?? null,
      timeout_secs: args.timeoutSecs ?? 3,
    },
  });
}

/** Remove an instrument from the active workspace. */
export function workspaceRemoveInstrument(instrumentId: string): Promise<void> {
  return invoke<void>("workspace_remove_instrument", { instrumentId });
}

/** List all instruments in the active workspace. */
export function instrumentList(): Promise<InstrumentView[]> {
  return invoke<InstrumentView[]>("instrument_list");
}

/** Open a connection to an instrument. */
export function instrumentConnect(
  instrumentId: string,
): Promise<InstrumentView> {
  return invoke<InstrumentView>("instrument_connect", { instrumentId });
}

/** Close the connection to an instrument. */
export function instrumentDisconnect(instrumentId: string): Promise<void> {
  return invoke<void>("instrument_disconnect", { instrumentId });
}

/** Send a SCPI query to a specific instrument. */
export function instrumentQuery(
  instrumentId: string,
  command: string,
): Promise<string> {
  return invoke<string>("instrument_query", { instrumentId, command });
}

/** Send a SCPI command to a specific instrument. */
export function instrumentWrite(
  instrumentId: string,
  command: string,
): Promise<void> {
  return invoke<void>("instrument_write", { instrumentId, command });
}

/** Return the driver description for an instrument. */
export function instrumentDriver(
  instrumentId: string,
): Promise<InstrumentDriver> {
  return invoke<InstrumentDriver>("instrument_driver", { instrumentId });
}

/** Run a named driver command against an instrument. */
export function instrumentRunCommand(
  instrumentId: string,
  name: string,
  args?: string,
): Promise<string | null> {
  return invoke<string | null>("instrument_run_command", {
    instrumentId,
    name,
    args: args ?? null,
  });
}

/* -------------------------------------------------------------------------
 * Batch and routing
 * ---------------------------------------------------------------------- */

/** Send commands to multiple instruments in parallel. */
export function instrumentBatchWrite(
  commands: Record<string, string>,
): Promise<Record<string, string | null>> {
  return invoke<Record<string, string | null>>("instrument_batch_write", {
    request: { commands },
  });
}

/** Query multiple instruments in parallel. */
export function instrumentBatchQuery(
  commands: Record<string, string>,
): Promise<Record<string, string | null>> {
  return invoke<Record<string, string | null>>("instrument_batch_query", {
    request: { commands },
  });
}

/** Evaluate a conditional routing rule. */
export function ruleEvaluate(ruleId: string): Promise<string | null> {
  return invoke<string | null>("rule_evaluate", { ruleId });
}

/** Arm a trigger. */
export function triggerArm(triggerId: string): Promise<void> {
  return invoke<void>("trigger_arm", { triggerId });
}

/** Disarm a trigger. */
export function triggerDisarm(triggerId: string): Promise<void> {
  return invoke<void>("trigger_disarm", { triggerId });
}

/** Add a trigger edge. */
export function triggerAdd(
  source: string,
  target: string,
  condition: string,
  action: string,
): Promise<unknown> {
  return invoke("trigger_add", { source, target, condition, action });
}

/* -------------------------------------------------------------------------
 * Presets and timeline
 * ---------------------------------------------------------------------- */

/** Apply a preset to all instruments in the active workspace. */
export function presetApply(
  presetId: string,
): Promise<Record<string, string | null>> {
  return invoke<Record<string, string | null>>("preset_apply", { presetId });
}

/** Retrieve timeline events for a time range. */
export function timelineGet(
  startMs: number,
  endMs: number,
): Promise<TimelineEvent[]> {
  return invoke<TimelineEvent[]>("timeline_get", {
    startMs,
    endMs,
  });
}

/** Jump to a point in the timeline. */
export function timelineScrub(
  timestampMs: number,
): Promise<TimelineEvent | null> {
  return invoke<TimelineEvent | null>("timeline_scrub", { timestampMs });
}

/* -------------------------------------------------------------------------
 * Legacy single-instrument commands
 * ---------------------------------------------------------------------- */

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

/** Close every connection in the active workspace. */
export function disconnect(): Promise<void> {
  return invoke<void>("disconnect");
}

/** Send a SCPI query to the first connected instrument. */
export function scpiQuery(command: string): Promise<string> {
  return invoke<string>("scpi_query", { command });
}

/** Send a SCPI command to the first connected instrument. */
export function scpiWrite(command: string): Promise<void> {
  return invoke<void>("scpi_write", { command });
}

/** Capture an instrument's screen. */
export function captureScreenshot(
  command?: string,
  instrumentId?: string,
): Promise<Screenshot> {
  return invoke<Screenshot>("capture_screenshot", {
    request: { command: command ?? null, instrument_id: instrumentId ?? null },
  });
}

/** Run a benchmark. Progress arrives via `benchmark:sample` events. */
export function runBenchmark(
  command = "*IDN?",
  iterations = 100,
  instrumentId?: string,
): Promise<BenchmarkReport> {
  return invoke<BenchmarkReport>("run_benchmark", {
    request: {
      command,
      iterations,
      instrument_id: instrumentId ?? null,
    },
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

/* -------------------------------------------------------------------------
 * Events
 * ---------------------------------------------------------------------- */

/** Subscribe to incremental discovery results. */
export function onDiscoverDevice(
  handler: (device: Device) => void,
): Promise<UnlistenFn> {
  return listen<Device>("discover:device", (event) => handler(event.payload));
}

/** Subscribe to structured discovery-engine events. */
export function onDiscoveryEvent(
  handler: (event: DiscoveryEvent) => void,
): Promise<UnlistenFn> {
  return listen<DiscoveryEvent>("discovery:event", (event) =>
    handler(event.payload),
  );
}

/** Subscribe to instrument state changes. */
export function onInstrumentState(
  handler: (event: unknown) => void,
): Promise<UnlistenFn> {
  return listen("workspace:instrument-state", (event) =>
    handler(event.payload),
  );
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
