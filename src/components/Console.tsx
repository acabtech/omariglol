/**
 * A monospace scrolling output region.
 *
 * Used by SCPI and Script modes. It is a live region (`role="log"`,
 * `aria-live="polite"`) so screen readers announce new output (§6.2, §6.5).
 */

import { useEffect, useRef } from "react";

import { cx } from "@/lib/cx";

export type ConsoleTone = "default" | "echo" | "muted" | "error";

export interface ConsoleLine {
  id: number;
  text: string;
  tone?: ConsoleTone;
}

export interface ConsoleProps {
  lines: ConsoleLine[];
  /** Accessible name for the log region. */
  label: string;
  /** Extra class, e.g. `console--error`. */
  className?: string;
  /** Follow the tail as new lines arrive. */
  autoScroll?: boolean;
  /** Placeholder shown when there is no output yet. */
  empty?: string;
}

export function Console({
  lines,
  label,
  className,
  autoScroll = true,
  empty = "No output yet.",
}: ConsoleProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  // Scroll the console's own container, never the page.
  useEffect(() => {
    if (autoScroll && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [lines, autoScroll]);

  return (
    <div
      ref={containerRef}
      className={cx("console", className)}
      role="log"
      aria-live="polite"
      aria-label={label}
    >
      {lines.length === 0 ? (
        <div className="console__line console__line--muted">{empty}</div>
      ) : (
        lines.map((line) => (
          <div
            key={line.id}
            className={cx(
              "console__line",
              line.tone && line.tone !== "default" && `console__line--${line.tone}`,
            )}
          >
            {line.text}
          </div>
        ))
      )}
    </div>
  );
}
