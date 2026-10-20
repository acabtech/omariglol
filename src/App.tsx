/**
 * Application shell.
 *
 * A single flowing workbench rather than a tabbed utility: a sticky top bar, a
 * persistent sidebar, and one scrollable pane holding every section. Nothing
 * is hidden behind a tab switch, so a running benchmark or script stays in
 * view while the user works elsewhere.
 */

import { useCallback, useEffect, useState } from "react";

import { Sidebar } from "@/components/Sidebar";
import { StatusDot, type StatusState } from "@/components/ui";
import { useSectionShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useScrollSpy } from "@/hooks/useScrollSpy";
import { useWindowGeometry } from "@/hooks/useWindowGeometry";
import { SECTION_IDS, SECTIONS } from "@/lib/types";
import BenchmarkSection from "@/sections/BenchmarkSection";
import ConnectionSection from "@/sections/ConnectionSection";
import ScpiSection from "@/sections/ScpiSection";
import ScreenshotSection from "@/sections/ScreenshotSection";
import ScriptSection from "@/sections/ScriptSection";
import { SessionProvider, useSession } from "@/state/session";
import { initialiseTheme, watchTheme } from "@/theme/omarchy";

function Shell() {
  const { session, connecting, error } = useSession();
  const [themeSource, setThemeSource] = useState<string>("…");
  const activeSection = useScrollSpy(SECTION_IDS);

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

  const scrollToSection = useCallback((id: string) => {
    const element = document.getElementById(id);
    if (!element) return;
    const reduceMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    element.scrollIntoView({
      behavior: reduceMotion ? "auto" : "smooth",
      block: "start",
    });
  }, []);

  const goToConnection = useCallback(
    () => scrollToSection("connection"),
    [scrollToSection],
  );

  // Ctrl+1 … Ctrl+5 jump to a section (§3.8).
  useSectionShortcuts(
    useCallback(
      (index: number) => {
        const section = SECTIONS[index];
        if (section) scrollToSection(section.id);
      },
      [scrollToSection],
    ),
  );

  const statusState: StatusState = connecting
    ? "idle"
    : session
      ? "ok"
      : error
        ? "error"
        : "idle";
  const statusLabel = connecting
    ? "connecting…"
    : session
      ? `connected: ${session.idn}`
      : "no instrument connected";

  return (
    <div className="app">
      <header className="topbar">
        <span className="topbar__brand">omariglol</span>
        <span className="topbar__status" aria-live="polite">
          <StatusDot state={statusState} />
          <span className="topbar__status-label">{statusLabel}</span>
        </span>
        <span className="topbar__spacer" />
        <span
          className="topbar__theme"
          title="Where the palette was resolved from"
        >
          theme: {themeSource}
        </span>
      </header>

      <div className="app__body">
        <Sidebar activeSection={activeSection} onNavigate={scrollToSection} />

        <main className="workbench">
          <ConnectionSection />
          <ScpiSection onGoToConnection={goToConnection} />
          <div className="workbench__row">
            <ScreenshotSection onGoToConnection={goToConnection} />
            <BenchmarkSection onGoToConnection={goToConnection} />
          </div>
          <ScriptSection onGoToConnection={goToConnection} />
        </main>
      </div>
    </div>
  );
}

export default function App() {
  useWindowGeometry();
  return (
    <SessionProvider>
      <Shell />
    </SessionProvider>
  );
}
