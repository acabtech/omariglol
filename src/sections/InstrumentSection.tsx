/**
 * Instrument control surface (concepts.md §4.1, §6.2).
 *
 * Renders the selected instrument's driver: every named operation the
 * instrument exposes, grouped by function. For the Rigol DS1000Z family (the
 * DS1054Z) this is the full operation surface — run/stop/single/auto, trigger
 * configuration, per-channel setup, timebase, measurements and waveform
 * transfer — while remaining generic for any other instrument.
 *
 * Queries show their last response inline; writes take an optional argument.
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { TextInput } from "@/components/Field";
import { Section } from "@/components/Section";
import { Alert, Badge, Button, ConnectPrompt, EmptyState } from "@/components/ui";
import * as api from "@/lib/api";
import { toCommandError } from "@/lib/api";
import { translateError } from "@/lib/errors";
import type { DriverCommand, InstrumentDriver, PanelProps } from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

/** Commands promoted to the quick-action row, in order. */
const QUICK_ACTIONS = [
  "run",
  "stop",
  "single",
  "autoscale",
  "force_trigger",
];

export default function InstrumentSection({ onGoToTopology }: PanelProps) {
  const { selected, connectInstrument, refreshTimeline } = useWorkspace();

  const [driver, setDriver] = useState<InstrumentDriver | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<Record<string, string>>({});
  const [args, setArgs] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const connected = selected?.state === "connected";

  // Load the driver whenever the selected instrument changes.
  useEffect(() => {
    setDriver(null);
    setResults({});
    setError(null);
    if (!selected || !connected) return;
    let disposed = false;
    setLoading(true);
    void (async () => {
      try {
        const loaded = await api.instrumentDriver(selected.id);
        if (!disposed) setDriver(loaded);
      } catch (err) {
        if (!disposed) setError(translateError(toCommandError(err)).message);
      } finally {
        if (!disposed) setLoading(false);
      }
    })();
    return () => {
      disposed = true;
    };
  }, [selected, connected]);

  const run = useCallback(
    async (command: DriverCommand) => {
      if (!selected) return;
      setBusy(command.name);
      setError(null);
      try {
        const response = await api.instrumentRunCommand(
          selected.id,
          command.name,
          args[command.name],
        );
        if (response != null) {
          setResults((prev) => ({ ...prev, [command.name]: response }));
        }
        await refreshTimeline();
      } catch (err) {
        setError(translateError(toCommandError(err)).message);
      } finally {
        setBusy(null);
      }
    },
    [selected, args, refreshTimeline],
  );

  const groups = useMemo(() => {
    if (!driver) return [];
    const map = new Map<string, DriverCommand[]>();
    for (const command of driver.commands) {
      const key = command.group || "other";
      const list = map.get(key);
      if (list) list.push(command);
      else map.set(key, [command]);
    }
    return Array.from(map.entries());
  }, [driver]);

  const quick = useMemo(
    () =>
      QUICK_ACTIONS.map((name) =>
        driver?.commands.find((command) => command.name === name),
      ).filter((command): command is DriverCommand => command != null),
    [driver],
  );

  return (
    <Section
      id="instrument"
      title="Instrument"
      description="Every operation the selected instrument exposes."
      actions={
        selected && (
          <span className="action-row__hint">
            {selected.label} · {selected.instrument_type}
          </span>
        )
      }
    >
      {!selected ? (
        <ConnectPrompt onGoToConnection={onGoToTopology} />
      ) : !connected ? (
        <div className="connect-prompt">
          <span className="connect-prompt__text">
            {selected.label} is {selected.state}.
          </span>
          <Button
            variant="primary"
            onClick={() => void connectInstrument(selected.id)}
          >
            Connect
          </Button>
        </div>
      ) : (
        <>
          {error && (
            <Alert tone="error" title="Command failed">
              {error}
            </Alert>
          )}

          {loading && <p className="progress-line">loading driver…</p>}

          {driver && (
            <>
              <div className="driver-head">
                <span className="mono">{driver.vendor}</span>
                <span className="mono">{driver.model}</span>
                {driver.profile && <Badge tone="ok">{driver.profile}</Badge>}
                {driver.screenshot_profile && (
                  <Badge>screenshot: {driver.screenshot_profile}</Badge>
                )}
              </div>

              {quick.length > 0 && (
                <div className="quick-actions" role="group" aria-label="Quick actions">
                  {quick.map((command) => (
                    <Button
                      key={command.name}
                      variant={command.name === "run" ? "primary" : "default"}
                      busy={busy === command.name}
                      onClick={() => void run(command)}
                    >
                      {command.name.replace(/_/g, " ")}
                    </Button>
                  ))}
                </div>
              )}

              {groups.length === 0 ? (
                <EmptyState title="No commands">
                  This instrument's driver exposes no named operations.
                </EmptyState>
              ) : (
                groups.map(([group, commands]) => (
                  <div key={group} className="driver-group">
                    <h3 className="subgroup__title">{group}</h3>
                    <div className="table-wrap">
                      <table className="table">
                        <thead>
                          <tr>
                            <th scope="col">Operation</th>
                            <th scope="col">SCPI</th>
                            <th scope="col">Argument</th>
                            <th scope="col">Result</th>
                            <th scope="col">Action</th>
                          </tr>
                        </thead>
                        <tbody>
                          {commands.map((command) => (
                            <tr key={command.name}>
                              <th scope="row" title={command.description}>
                                {command.name}
                              </th>
                              <td className="mono">{command.scpi}</td>
                              <td>
                                {command.query ? (
                                  <span className="muted">—</span>
                                ) : (
                                  <TextInput
                                    className="mono input--arg"
                                    aria-label={`Argument for ${command.name}`}
                                    value={args[command.name] ?? ""}
                                    onChange={(event) =>
                                      setArgs((prev) => ({
                                        ...prev,
                                        [command.name]: event.target.value,
                                      }))
                                    }
                                    onKeyDown={(event) => {
                                      if (event.key === "Enter") {
                                        void run(command);
                                      }
                                    }}
                                  />
                                )}
                              </td>
                              <td className="mono driver-result">
                                {results[command.name] ?? ""}
                              </td>
                              <td>
                                <Button
                                  variant={command.query ? "default" : "primary"}
                                  busy={busy === command.name}
                                  onClick={() => void run(command)}
                                >
                                  {command.query ? "Read" : "Send"}
                                </Button>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                ))
              )}
            </>
          )}
        </>
      )}
    </Section>
  );
}
