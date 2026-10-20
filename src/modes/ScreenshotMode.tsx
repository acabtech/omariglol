/**
 * Screenshot mode — capture and display the instrument's screen.
 *
 * Mirrors lxi-tools' Screenshot panel. The capture query is chosen from a
 * per-vendor profile matched against `*IDN?`, with a manual override for
 * instruments that are not in the table.
 */

export default function ScreenshotMode() {
  return (
    <div className="panel">
      <p className="stub">Screenshot capture is not implemented yet.</p>
      <p className="stub">
        Will select a profile from the instrument's <code>*IDN?</code>, read the
        IEEE-488.2 image block, and render it here with a save button.
      </p>
    </div>
  );
}
