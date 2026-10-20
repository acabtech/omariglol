/**
 * SCPI mode — a command console.
 *
 * Mirrors lxi-tools' SCPI panel: type a command, see the response, with an
 * option to render binary responses as hex.
 */

export default function ScpiMode() {
  return (
    <div className="panel">
      <p className="stub">The SCPI console is not implemented yet.</p>
      <p className="stub">
        Will send commands over the active session and print responses, with a
        hex view for binary blocks and an interactive history.
      </p>
    </div>
  );
}
