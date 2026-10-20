/**
 * Application shell.
 *
 * Owns the mode switcher and the theme lifecycle. Each mode is a separate
 * component under `src/modes/`; this file deliberately knows nothing about
 * instruments.
 */

import { useEffect, useState } from "react";

import BenchmarkMode from "@/modes/BenchmarkMode";
import ScpiMode from "@/modes/ScpiMode";
import ScreenshotMode from "@/modes/ScreenshotMode";
import ScriptMode from "@/modes/ScriptMode";
import SearchMode from "@/modes/SearchMode";
import { MODES, type Mode } from "@/lib/types";
import { initialiseTheme, watchTheme } from "@/theme/omarchy";

export default function App() {
  const [mode, setMode] = useState<Mode>("search");
  const [themeSource, setThemeSource] = useState<string>("…");

  useEffect(() => {
    let stop: (() => void) | undefined;

    void (async () => {
      const palette = await initialiseTheme();
      setThemeSource(palette?.source ?? "builtin");
      // Follow live theme switches without a restart.
      stop = watchTheme((next) => setThemeSource(next.source));
    })();

    return () => stop?.();
  }, []);

  return (
    <div className="app">
      <header className="titlebar">
        <span className="titlebar__title">omariglol</span>
        <span className="status-dot" />
        <span className="stub">no instrument connected</span>
        <span className="titlebar__spacer" />
        <span className="stub" title="Where the palette was resolved from">
          theme: {themeSource}
        </span>
      </header>

      <main>
        <nav className="modes" role="tablist" aria-label="Modes">
          {MODES.map(({ id, label }) => (
            <button
              key={id}
              role="tab"
              type="button"
              className="mode-tab"
              aria-selected={mode === id}
              onClick={() => setMode(id)}
            >
              {label}
            </button>
          ))}
        </nav>

        <section className="mode-body" role="tabpanel">
          {mode === "search" && <SearchMode />}
          {mode === "scpi" && <ScpiMode />}
          {mode === "screenshot" && <ScreenshotMode />}
          {mode === "benchmark" && <BenchmarkMode />}
          {mode === "script" && <ScriptMode />}
        </section>
      </main>
    </div>
  );
}
