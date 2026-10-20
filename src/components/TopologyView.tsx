/**
 * The instrument topology view (concepts.md §2.2).
 *
 * The central navigation surface: every instrument in the workspace, grouped by
 * transport, with its connection status, type and trigger dependencies. Clicking
 * an instrument selects it for the detail panels.
 *
 * It is a real list of buttons, so it is fully keyboard-navigable, and status is
 * never conveyed by colour alone — each row carries a text state label.
 */

import { Badge, StatusDot, type StatusState } from "@/components/ui";
import { cx } from "@/lib/cx";
import type { InstrumentState, InstrumentView, Trigger } from "@/lib/types";

/** Map an instrument state onto the shared status-dot states. */
export function statusFor(state: InstrumentState): StatusState {
  if (state === "connected") return "ok";
  if (state === "error" || state === "offline") return "error";
  return "idle";
}

/** A short, human label for an instrument state. */
export function stateLabel(state: InstrumentState): string {
  switch (state) {
    case "connected":
      return "connected";
    case "connecting":
      return "connecting";
    case "error":
      return "error";
    case "offline":
      return "offline";
    default:
      return "disconnected";
  }
}

export interface TopologyViewProps {
  instruments: InstrumentView[];
  triggers: Trigger[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function TopologyView({
  instruments,
  triggers,
  selectedId,
  onSelect,
}: TopologyViewProps) {
  if (instruments.length === 0) {
    return (
      <p className="topology__empty">
        No instruments in this workspace. Add one from Discovery.
      </p>
    );
  }

  // Group by transport, so the topology reads as a network map.
  const groups = new Map<string, InstrumentView[]>();
  for (const instrument of instruments) {
    const key = instrument.transport;
    const list = groups.get(key);
    if (list) list.push(instrument);
    else groups.set(key, [instrument]);
  }

  return (
    <div className="topology">
      {Array.from(groups.entries()).map(([transport, members]) => (
        <div key={transport} className="topology__group">
          <h3 className="topology__group-title">
            {transport}
            <span className="topology__group-count">{members.length}</span>
          </h3>
          <ul className="topology__list">
            {members.map((instrument) => {
              const isSelected = instrument.id === selectedId;
              const outgoing = triggers.filter(
                (trigger) => trigger.source === instrument.id,
              );
              return (
                <li key={instrument.id}>
                  <button
                    type="button"
                    className={cx(
                      "topology__node",
                      isSelected && "is-selected",
                    )}
                    aria-current={isSelected ? "true" : undefined}
                    onClick={() => onSelect(instrument.id)}
                  >
                    <span className="topology__glyph" aria-hidden="true">
                      {instrument.instrument_glyph}
                    </span>
                    <span className="topology__body">
                      <span className="topology__label" title={instrument.label}>
                        {instrument.label}
                      </span>
                      <span className="topology__meta mono">
                        {instrument.address}
                        {instrument.port != null ? `:${instrument.port}` : ""}
                      </span>
                    </span>
                    <span className="topology__status">
                      <StatusDot state={statusFor(instrument.state)} />
                      <span className="topology__state">
                        {stateLabel(instrument.state)}
                      </span>
                    </span>
                  </button>
                  {outgoing.length > 0 && (
                    <ul className="topology__edges">
                      {outgoing.map((trigger) => {
                        const target = instruments.find(
                          (i) => i.id === trigger.target,
                        );
                        return (
                          <li key={trigger.id} className="topology__edge">
                            <span aria-hidden="true">↳</span>
                            <span className="topology__edge-label">
                              triggers {target?.label ?? trigger.target}
                            </span>
                            <Badge
                              tone={
                                trigger.state === "armed"
                                  ? "ok"
                                  : trigger.state === "fired"
                                    ? "warning"
                                    : "neutral"
                              }
                            >
                              {trigger.state}
                            </Badge>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </div>
  );
}
