/**
 * Search mode — find instruments on the network.
 *
 * Mirrors lxi-tools' Search panel: run a discovery, list what answered, and
 * pick one to work with.
 */

export default function SearchMode() {
  return (
    <div className="panel">
      <p className="stub">
        Instrument discovery is not implemented yet.
      </p>
      <p className="stub">
        Will broadcast a VXI-11 <code>PMAP_DUMP</code> and optionally browse
        mDNS, then list each instrument with its address, ports and discovery
        method. See <code>docs/roadmap.md</code>.
      </p>
    </div>
  );
}
