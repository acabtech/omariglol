/**
 * Benchmark mode — measure SCPI round-trip performance.
 *
 * Mirrors lxi-tools' Benchmark panel. Samples arrive as events so the run can
 * be drawn live rather than appearing all at once at the end.
 */

export default function BenchmarkMode() {
  return (
    <div className="panel">
      <p className="stub">Benchmarking is not implemented yet.</p>
      <p className="stub">
        Will repeat a query and report min / mean / median / p95 round-trip
        time, throughput and failures, streaming each sample as it is measured.
      </p>
    </div>
  );
}
