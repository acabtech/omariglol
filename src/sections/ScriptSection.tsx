/**
 * Script section — write and run a Python measurement script
 * (UX journey §4.5, §6.5).
 *
 * A Python editor, a streaming output console and live charts. `print()`
 * output, `sample` events and `status` events all arrive as `script:event`
 * events, so the console and charts update while the script runs.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { Chart, type ChartSample, type ChartType } from "@/components/Chart";
import { Console, type ConsoleLine } from "@/components/Console";
import { Section } from "@/components/Section";
import { Alert, Button } from "@/components/ui";
import * as api from "@/lib/api";
import { toCommandError } from "@/lib/api";
import { translateError } from "@/lib/errors";
import { fileTimestamp } from "@/lib/format";
import type { CommandError, ScriptEvent, SectionProps } from "@/lib/types";
import { useSession } from "@/state/session";

const DEFAULT_SOURCE = `from omariglol import connect

with connect("192.168.1.50") as inst:
    print(inst.idn())
`;

const CHART_TYPES: ChartType[] = ["line", "scatter", "gauge"];

export default function ScriptSection({ onGoToConnection }: SectionProps) {
  const { session } = useSession();

  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [samples, setSamples] = useState<ChartSample[]>([]);
  const [status, setStatus] = useState("");
  const [exitStatus, setExitStatus] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [chartType, setChartType] = useState<ChartType>("line");

  const nextId = useRef(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const pushLine = useCallback((text: string, tone?: ConsoleLine["tone"]) => {
    setLines((prev) => [...prev, { id: nextId.current++, text, tone }]);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      const stop = await api.onScriptEvent((event: ScriptEvent) => {
        switch (event.kind) {
          case "stdout":
            pushLine(event.text);
            break;
          case "stderr":
            pushLine(event.text, "error");
            break;
          case "sample":
            setSamples((prev) => [
              ...prev,
              { series: event.series, value: event.value },
            ]);
            break;
          case "status":
            setStatus(event.message);
            break;
        }
      });
      if (disposed) stop();
      else unlisten = stop;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [pushLine]);

  const run = useCallback(async () => {
    setLines([]);
    setSamples([]);
    setStatus("");
    setExitStatus(null);
    setError(null);
    setRunning(true);
    try {
      await api.runScript(source);
      setExitStatus("exited with code 0");
    } catch (err) {
      const commandError = toCommandError(err);
      if (commandError.kind === "cancelled") {
        setExitStatus("cancelled");
      } else {
        setError(commandError);
        setExitStatus("exited with code 1");
      }
    } finally {
      setRunning(false);
    }
  }, [source]);

  const cancel = useCallback(async () => {
    try {
      await api.cancelScript();
    } catch (err) {
      setError(toCommandError(err));
    }
  }, []);

  // Ctrl+Enter runs, Esc cancels (§3.8).
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        if (!running) void run();
      } else if (event.key === "Escape" && running) {
        event.preventDefault();
        void cancel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [running, run, cancel]);

  const onOpen = useCallback(() => fileInputRef.current?.click(), []);

  const onFileChosen = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      if (!file) return;
      try {
        setSource(await file.text());
      } catch (err) {
        setError(toCommandError(err));
      } finally {
        // Allow re-opening the same file.
        event.target.value = "";
      }
    },
    [],
  );

  const onSave = useCallback(() => {
    const blob = new Blob([source], { type: "text/x-python" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `omariglol-script-${fileTimestamp()}.py`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  }, [source]);

  const errorText = error ? translateError(error) : null;

  return (
    <Section
      id="script"
      title="Script"
      description="Write and run a Python measurement."
      actions={
        <>
          {running ? (
            <Button variant="danger" onClick={() => void cancel()}>
              Cancel
            </Button>
          ) : (
            <Button variant="primary" onClick={() => void run()}>
              Run
            </Button>
          )}
          <Button onClick={onOpen} disabled={running}>
            Open
          </Button>
          <Button onClick={onSave} disabled={running}>
            Save
          </Button>
        </>
      }
    >
      {!session && (
        <Alert tone="info">
          No instrument connected. Scripts can open their own connection with{" "}
          <code>omariglol.connect()</code>, or{" "}
          <button
            type="button"
            className="link-button"
            onClick={onGoToConnection}
          >
            connect an instrument
          </button>
          .
        </Alert>
      )}

      {errorText && (
        <Alert tone="error" title="Script failed">
          {errorText.message}
        </Alert>
      )}

      <div className="script">
        <div className="script__editor">
          <label className="visually-hidden" htmlFor="script-editor">
            Python script
          </label>
          <textarea
            id="script-editor"
            className="editor mono"
            spellCheck={false}
            value={source}
            disabled={running}
            onChange={(event) => setSource(event.target.value)}
          />
          <span className="script__hint">Ctrl+Enter to run</span>
        </div>

        <div className="script__output">
          <div className="script__output-head">
            <span className="subgroup__title">Output</span>
            {status && (
              <span className="progress-line" aria-live="polite">
                {status}
              </span>
            )}
            {exitStatus && (
              <span className="panel__count mono">{exitStatus}</span>
            )}
          </div>
          <Console
            lines={lines}
            label="Script output"
            empty="Run the script to see its output here."
          />
          <div className="script__charts">
            <div className="segmented" role="group" aria-label="Chart type">
              {CHART_TYPES.map((type) => (
                <button
                  key={type}
                  type="button"
                  className="segmented__item"
                  aria-pressed={chartType === type}
                  onClick={() => setChartType(type)}
                >
                  {type}
                </button>
              ))}
            </div>
            <Chart
              samples={samples}
              type={chartType}
              label="Script sample data"
            />
          </div>
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        accept=".py,text/x-python"
        className="visually-hidden"
        onChange={onFileChosen}
      />
    </Section>
  );
}
