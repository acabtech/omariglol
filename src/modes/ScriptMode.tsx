/**
 * Script mode — write and run a Python measurement script.
 *
 * This is where omariglol deliberately diverges from lxi-tools, which uses
 * Lua. Scripts get the `omariglol` module with a `connect()` helper, and
 * `print()` output plus numeric samples stream back here as events.
 */

export default function ScriptMode() {
  return (
    <div className="panel">
      <p className="stub">The script editor is not implemented yet.</p>
      <p className="stub">
        Will host a Python editor, an output console and live charts. The
        embedded interpreter lives in <code>crates/omariglol-script</code>.
      </p>
    </div>
  );
}
