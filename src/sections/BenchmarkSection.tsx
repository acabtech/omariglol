/**
 * Benchmark panel (UX journey §4.4, §6.4).
 *
 * Measures SCPI round-trip performance for the selected instrument. Samples
 * stream in via `benchmark:sample` events so the run is drawn live.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Chart } from "@/components/Chart";
import { Field, NumberInput, TextInput } from "@/components/Field";
import { Section } from "@/components/Section";
import {
  Alert,
  Badge,
  Button,
  ConnectPrompt,
  Disclosure,
  ProgressBar,
} from "@/components/ui";
import * as api from "@/lib/api";
import { toCommandError } from "@/lib/api";
import { translateError } from "@/lib/errors";
import { formatBytes, formatMs, formatRate } from "@/lib/format";
import type {
  BenchmarkReport,
  BenchmarkSample,
  CommandError,
  PanelProps,
} from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

/** Client-side statistics, used for the partial view after a cancel. */
interface PartialStats {
  count: number;
  min: number;
  max: number;
  mean: number;
  median: number;
  p95: number;
  elapsed: number;
}

function summarise(values: number[]): PartialStats | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const sum = sorted.reduce((acc, value) => acc + value, 0);
  const percentile = (p: number) => {
    const index = Math.min(
      sorted.length - 1,
      Math.floor((p / 100) * sorted.length),
    );
    return sorted[index];
  };
  return {
    count: sorted.length,
    min: sorted[0],
    max: sorted[sorted.length - 1],
    mean: sum / sorted.length,
    median: percentile(50),
    p95: percentile(95),
    elapsed: sum,
  };
}

export default function BenchmarkSection({ onGoToTopology }: PanelProps) {
  const { selected, refreshTimeline } = useWorkspace();
  const connected = selected?.state === "connected";

  const [command, setCommand] = useState("*IDN?");
  const [iterations, setIterations] = useState(25);
  const [samples, setSamples] = useState<BenchmarkSample[]>([]);
  const [report, setReport] = useState<BenchmarkReport | null>(null);
  const [running, setRunning] = useState(false);
  const [cancelled, setCancelled] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const runningRef = useRef(false);
  const cancelledRef = useRef(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      const stop = await api.onBenchmarkSample((sample) => {
        if (!runningRef.current || cancelledRef.current) return;
        setSamples((prev) => [...prev, sample]);
      });
      if (disposed) stop();
      else unlisten = stop;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const run = useCallback(async () => {
    if (!selected) return;
    setSamples([]);
    setReport(null);
    setCancelled(false);
    setError(null);
    cancelledRef.current = false;
    runningRef.current = true;
    setRunning(true);
    try {
      const result = await api.runBenchmark(command, iterations, selected.id);
      setReport(result);
      await refreshTimeline();
    } catch (err) {
      setError(toCommandError(err));
    } finally {
      runningRef.current = false;
      setRunning(false);
    }
  }, [command, iterations, selected, refreshTimeline]);

  const cancel = useCallback(() => {
    cancelledRef.current = true;
    setCancelled(true);
  }, []);

  // Esc cancels a running benchmark (§3.8).
  useEffect(() => {
    if (!running) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [running, cancel]);

  const partial = useMemo(
    () => (cancelled ? summarise(samples.map((s) => s.sample_ms)) : null),
    [cancelled, samples],
  );

  const lastSample = samples.at(-1);
  const errorText = error ? translateError(error) : null;

  return (
    <Section
      id="benchmark"
      title="Benchmark"
      description={
        selected
          ? `Measure ${selected.label}'s SCPI round-trip performance.`
          : "Measure SCPI round-trip performance."
      }
      actions={
        running ? (
          <Button variant="danger" onClick={cancel}>
            Cancel
          </Button>
        ) : (
          <Button
            variant="primary"
            disabled={!connected}
            onClick={() => void run()}
          >
            Run
          </Button>
        )
      }
    >
      {connected ? (
        <>
          <div className="form-grid">
            <Field label="Command" htmlFor="benchmark-command">
              <TextInput
                id="benchmark-command"
                className="mono"
                value={command}
                disabled={running}
                onChange={(event) => setCommand(event.target.value)}
              />
            </Field>
            <Field label="Iterations" htmlFor="benchmark-iterations">
              <NumberInput
                id="benchmark-iterations"
                min={1}
                max={10000}
                value={iterations}
                disabled={running}
                onChange={(event) =>
                  setIterations(Math.max(1, Number(event.target.value) || 1))
                }
              />
            </Field>
          </div>

          <Disclosure summary="advanced">
            <p className="field__help">
              The run stops early after 5 consecutive failures. Samples are
              measured per round trip.
            </p>
          </Disclosure>

          {errorText && (
            <Alert tone="error" title="Benchmark failed">
              {errorText.message}
            </Alert>
          )}

          <div className="benchmark__progress">
            <p className="progress-line" aria-live="polite">
              {samples.length}/{iterations}
              {lastSample ? ` (${formatMs(lastSample.sample_ms)})` : ""}
              {cancelled && <Badge tone="warning">partial</Badge>}
            </p>
            <ProgressBar
              value={samples.length}
              max={iterations}
              label="Benchmark progress"
            />
            <Chart
              samples={samples.map((sample) => ({
                series: "latency",
                value: sample.sample_ms,
              }))}
              type="line"
              label="Round-trip latency per iteration"
            />
          </div>

          {(report || partial) && (
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">Metric</th>
                    <th scope="col">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {report ? (
                    <ReportRows report={report} />
                  ) : (
                    partial && <PartialRows stats={partial} />
                  )}
                </tbody>
              </table>
            </div>
          )}
        </>
      ) : (
        <ConnectPrompt onGoToConnection={onGoToTopology} />
      )}
    </Section>
  );
}

function MetricRow({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "warning";
}) {
  return (
    <tr className={tone ? `is-${tone}` : undefined}>
      <th scope="row">{label}</th>
      <td className="mono">{value}</td>
    </tr>
  );
}

function ReportRows({ report }: { report: BenchmarkReport }) {
  const bandwidth =
    report.elapsed_ms > 0
      ? report.bytes_received / (report.elapsed_ms / 1000) / 1024
      : 0;
  return (
    <>
      <MetricRow label="command" value={report.command} />
      <MetricRow label="iterations" value={String(report.iterations)} />
      <MetricRow
        label="failures"
        value={String(report.failures)}
        tone={report.failures > 0 ? "warning" : undefined}
      />
      <MetricRow label="elapsed" value={formatMs(report.elapsed_ms)} />
      <MetricRow label="min" value={formatMs(report.min_ms)} />
      <MetricRow label="max" value={formatMs(report.max_ms)} />
      <MetricRow label="mean" value={formatMs(report.mean_ms)} />
      <MetricRow label="median" value={formatMs(report.median_ms)} />
      <MetricRow label="p95" value={formatMs(report.p95_ms)} />
      <MetricRow label="stddev" value={formatMs(report.stddev_ms)} />
      <MetricRow
        label="throughput"
        value={formatRate(report.messages_per_second, "msg/s")}
      />
      <MetricRow label="bandwidth" value={`${bandwidth.toFixed(1)} KiB/s`} />
      <MetricRow label="bytes" value={formatBytes(report.bytes_received)} />
    </>
  );
}

function PartialRows({ stats }: { stats: PartialStats }) {
  return (
    <>
      <MetricRow label="iterations" value={String(stats.count)} />
      <MetricRow label="elapsed" value={formatMs(stats.elapsed)} />
      <MetricRow label="min" value={formatMs(stats.min)} />
      <MetricRow label="max" value={formatMs(stats.max)} />
      <MetricRow label="mean" value={formatMs(stats.mean)} />
      <MetricRow label="median" value={formatMs(stats.median)} />
      <MetricRow label="p95" value={formatMs(stats.p95)} />
    </>
  );
}
