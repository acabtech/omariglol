/**
 * Lightweight SVG charts for live sample data.
 *
 * Deliberately dependency-free: the sample volumes here are small and the
 * shapes are simple, so a few hundred bytes of SVG beat a charting library.
 * The viewBox is measured from the container so the chart fills the available
 * width without distorting its strokes. Every chart carries a visually-hidden
 * data table as its text alternative (§6.5).
 */

import { useEffect, useId, useRef, useState } from "react";

import { cx } from "@/lib/cx";

export type ChartType = "line" | "scatter" | "gauge";

export interface ChartSample {
  series: string;
  value: number;
}

export interface ChartProps {
  samples: ChartSample[];
  type: ChartType;
  /** Accessible name for the chart. */
  label: string;
  height?: number;
  className?: string;
}

/** Data-series colours, in the Rigol channel order from §2.1. */
const SERIES_COLORS = [
  "var(--om-yellow)",
  "var(--om-cyan)",
  "var(--om-magenta)",
  "var(--om-blue)",
  "var(--om-green)",
  "var(--om-orange)",
];

const PAD = 10;

/** Group samples by series, preserving first-seen order. */
function groupBySeries(samples: ChartSample[]): Map<string, number[]> {
  const grouped = new Map<string, number[]>();
  for (const sample of samples) {
    const values = grouped.get(sample.series);
    if (values) {
      values.push(sample.value);
    } else {
      grouped.set(sample.series, [sample.value]);
    }
  }
  return grouped;
}

/** A padded [min, max] range that never collapses to zero width. */
function range(values: number[]): [number, number] {
  if (values.length === 0) return [0, 1];
  let min = Math.min(...values);
  let max = Math.max(...values);
  if (min === max) {
    min -= 1;
    max += 1;
  }
  return [min, max];
}

function polar(cx: number, cy: number, r: number, degrees: number) {
  const radians = ((degrees - 90) * Math.PI) / 180;
  return { x: cx + r * Math.cos(radians), y: cy + r * Math.sin(radians) };
}

function arcPath(
  cx: number,
  cy: number,
  r: number,
  startDeg: number,
  endDeg: number,
): string {
  const start = polar(cx, cy, r, endDeg);
  const end = polar(cx, cy, r, startDeg);
  const largeArc = endDeg - startDeg <= 180 ? 0 : 1;
  return `M ${start.x} ${start.y} A ${r} ${r} 0 ${largeArc} 0 ${end.x} ${end.y}`;
}

export function Chart({
  samples,
  type,
  label,
  height = 160,
  className,
}: ChartProps) {
  const titleId = useId();
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);

  // Measure the container so the viewBox matches the rendered pixels.
  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const measured = entries[0]?.contentRect.width;
      if (measured && measured > 0) setWidth(measured);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const grouped = groupBySeries(samples);
  const series = Array.from(grouped.entries());

  const xFor = (index: number, count: number) =>
    PAD + (count <= 1 ? 0.5 : index / (count - 1)) * (width - 2 * PAD);

  const allValues = samples.map((sample) => sample.value);
  const [min, max] = range(allValues);
  const span = max - min;
  const yFor = (value: number) =>
    height - PAD - ((value - min) / span) * (height - 2 * PAD);

  return (
    <figure className={cx("chart", className)} ref={containerRef}>
      {samples.length === 0 ? (
        <div className="chart--empty" style={{ height }}>
          <span className="chart__empty">No samples yet.</span>
        </div>
      ) : (
        <svg
          className="chart__svg"
          viewBox={`0 0 ${width} ${height}`}
          style={{ height }}
          role="img"
          aria-labelledby={titleId}
        >
          <title id={titleId}>{label}</title>
          {type === "gauge" ? (
            <Gauge
              value={series[0]?.[1].at(-1) ?? 0}
              min={min}
              max={max}
              width={width}
              height={height}
            />
          ) : (
            series.map(([name, values], seriesIndex) => {
              const colour = SERIES_COLORS[seriesIndex % SERIES_COLORS.length];
              if (type === "scatter") {
                return (
                  <g key={name}>
                    {values.map((value, index) => (
                      <circle
                        key={index}
                        cx={xFor(index, values.length)}
                        cy={yFor(value)}
                        r={3}
                        fill={colour}
                      />
                    ))}
                  </g>
                );
              }
              const points = values
                .map(
                  (value, index) => `${xFor(index, values.length)},${yFor(value)}`,
                )
                .join(" ");
              return (
                <polyline
                  key={name}
                  points={points}
                  fill="none"
                  stroke={colour}
                  strokeWidth={2}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                />
              );
            })
          )}
        </svg>
      )}

      {series.length > 1 && (
        <figcaption className="chart__legend">
          {series.map(([name], index) => (
            <span key={name} className="chart__legend-item">
              <span
                className="chart__swatch"
                style={{
                  background: SERIES_COLORS[index % SERIES_COLORS.length],
                }}
                aria-hidden="true"
              />
              {name}
            </span>
          ))}
        </figcaption>
      )}

      {/* Text alternative: the same data as a table. */}
      <table className="visually-hidden">
        <caption>{label}</caption>
        <thead>
          <tr>
            <th scope="col">Series</th>
            <th scope="col">Latest value</th>
            <th scope="col">Samples</th>
          </tr>
        </thead>
        <tbody>
          {series.map(([name, values]) => (
            <tr key={name}>
              <th scope="row">{name}</th>
              <td>{values.at(-1)}</td>
              <td>{values.length}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}

function Gauge({
  value,
  min,
  max,
  width,
  height,
}: {
  value: number;
  min: number;
  max: number;
  width: number;
  height: number;
}) {
  const cx = width / 2;
  const cy = height - PAD;
  const radius = Math.min(width / 2, height) - PAD;
  const fraction = max > min ? (value - min) / (max - min) : 0;
  const clamped = Math.min(1, Math.max(0, fraction));
  const startDeg = -90;
  const endDeg = 90;
  const valueDeg = startDeg + clamped * (endDeg - startDeg);

  return (
    <g>
      <path
        d={arcPath(cx, cy, radius, startDeg, endDeg)}
        fill="none"
        stroke="var(--om-muted)"
        strokeWidth={10}
        strokeLinecap="round"
      />
      <path
        d={arcPath(cx, cy, radius, startDeg, valueDeg)}
        fill="none"
        stroke="var(--om-accent)"
        strokeWidth={10}
        strokeLinecap="round"
      />
      <text
        x={cx}
        y={cy - radius / 2}
        textAnchor="middle"
        className="chart__gauge-value"
      >
        {value.toFixed(3)}
      </text>
    </g>
  );
}
