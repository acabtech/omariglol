/**
 * The shared automation timeline (concepts.md §2.5, §4.1).
 *
 * A time-ordered log of every command, query, error, preset, rule and trigger
 * across the workspace. It is the audit trail the user scrubs through and the
 * scripting layer records into.
 *
 * The list is a live region so screen readers announce new events, and each
 * event carries a text kind label so meaning is never colour-only.
 */

import { useMemo, useState } from "react";

import { Badge, Button, EmptyState } from "@/components/ui";
import { cx } from "@/lib/cx";
import type { TimelineEvent, TimelineEventKind } from "@/lib/types";

/** A short label for an event kind. */
function kindLabel(kind: TimelineEventKind): string {
  return kind;
}

/** The badge tone for an event kind. */
function kindTone(
  kind: TimelineEventKind,
): "neutral" | "ok" | "warning" | "error" {
  if (kind === "error") return "error";
  if (kind === "rule" || kind === "trigger") return "warning";
  if (kind === "preset") return "ok";
  return "neutral";
}

/** Format a millisecond timestamp as a local time with milliseconds. */
function formatTime(ms: number): string {
  const date = new Date(ms);
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  return (
    `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` +
    `.${pad(date.getMilliseconds(), 3)}`
  );
}

export interface TimelineViewProps {
  events: TimelineEvent[];
  /** A map from instrument id to label, for readable event rows. */
  instrumentLabels: Record<string, string>;
  onRefresh: () => void;
}

export function TimelineView({
  events,
  instrumentLabels,
  onRefresh,
}: TimelineViewProps) {
  const [filter, setFilter] = useState<TimelineEventKind | "all">("all");

  const kinds = useMemo(() => {
    const seen = new Set<TimelineEventKind>();
    for (const event of events) seen.add(event.kind);
    return Array.from(seen);
  }, [events]);

  const visible = useMemo(
    () =>
      filter === "all"
        ? events
        : events.filter((event) => event.kind === filter),
    [events, filter],
  );

  return (
    <div className="timeline">
      <div className="timeline__toolbar">
        <div className="segmented" role="group" aria-label="Filter by kind">
          <button
            type="button"
            className="segmented__item"
            aria-pressed={filter === "all"}
            onClick={() => setFilter("all")}
          >
            all
          </button>
          {kinds.map((kind) => (
            <button
              key={kind}
              type="button"
              className="segmented__item"
              aria-pressed={filter === kind}
              onClick={() => setFilter(kind)}
            >
              {kindLabel(kind)}
            </button>
          ))}
        </div>
        <span className="timeline__count">{visible.length} events</span>
        <Button onClick={onRefresh}>Refresh</Button>
      </div>

      {visible.length === 0 ? (
        <EmptyState title="Nothing on the timeline yet">
          Connect an instrument, send a command or run a script, and the events
          will appear here.
        </EmptyState>
      ) : (
        <ol className="timeline__list" role="log" aria-live="polite">
          {visible.map((event, index) => (
            <li
              key={`${event.timestamp_ms}-${index}`}
              className={cx(
                "timeline__event",
                event.kind === "error" && "timeline__event--error",
              )}
            >
              <span className="timeline__time mono">
                {formatTime(event.timestamp_ms)}
              </span>
              <Badge tone={kindTone(event.kind)}>{kindLabel(event.kind)}</Badge>
              {event.instrument_id && (
                <span className="timeline__instrument">
                  {instrumentLabels[event.instrument_id] ?? event.instrument_id}
                </span>
              )}
              <span className="timeline__message mono">{event.message}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
