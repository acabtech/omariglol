/**
 * Application shell (concepts.md §2.4).
 *
 * A workspace-aware workbench: a sticky top bar, a persistent sidebar holding
 * the instrument topology, and one scrollable pane of panels. Nothing is hidden
 * behind a tab switch, so a running benchmark or script stays in view while the
 * user works elsewhere.
 */

import { useCallback, useEffect, useState } from "react";

import { Sidebar } from "@/components/Sidebar";
import { StatusDot, type StatusState } from "@/components/ui";
import { useSectionShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useScrollSpy } from "@/hooks/useScrollSpy";
import { useWindowGeometry } from "@/hooks/useWindowGeometry";
import { PANEL_IDS, PANELS } from "@/lib/types";
import BenchmarkSection from "@/sections/BenchmarkSection";
import DiscoverySection from "@/sections/DiscoverySection";
import InstrumentSection from "@/sections/InstrumentSection";
import ScreenshotSection from "@/sections/ScreenshotSection";
import ScpiSection from "@/sections/ScpiSection";
import ScriptSection from "@/sections/ScriptSection";
import TimelineSection from "@/sections/TimelineSection";
import WorkspaceSection from "@/sections/WorkspaceSection";
import { WorkspaceProvider, useWorkspace } from "@/state/workspace";
import { initialiseTheme, watchTheme } from "@/theme/omarchy";

function Shell() {
  const { workspace, instruments, loading } = useWorkspace();
  const [themeSource, setThemeSource] = useState<string>("…");
  const activePanel = useScrollSpy(PANEL_IDS);

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

  const scrollToPanel = useCallback((id: string) => {
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

  const goToTopology = useCallback(
    () => scrollToPanel("workspace"),
    [scrollToPanel],
  );

  // Ctrl+1 … Ctrl+8 jump to a panel (§3.8).
  useSectionShortcuts(
    useCallback(
      (index: number) => {
        const panel = PANELS[index];
        if (panel) scrollToPanel(panel.id);
      },
      [scrollToPanel],
    ),
  );

  const connected = instruments.filter((i) => i.state === "connected").length;
  const statusState: StatusState = loading
    ? "idle"
    : connected > 0
      ? "ok"
      : instruments.some((i) => i.state === "error" || i.state === "offline")
        ? "error"
        : "idle";
  const statusLabel = loading
    ? "loading…"
    : instruments.length === 0
      ? "no instruments"
      : `${connected}/${instruments.length} connected`;

  return (
    <div className="app">
      <header className="topbar">
        <span className="topbar__brand">omariglol</span>
        <span className="topbar__status" aria-live="polite">
          <StatusDot state={statusState} />
          <span className="topbar__status-label">{statusLabel}</span>
        </span>
        {workspace && (
          <span className="topbar__workspace" title="Active workspace">
            {workspace.name}
          </span>
        )}
        <span className="topbar__spacer" />
        <span
          className="topbar__theme"
          title="Where the palette was resolved from"
        >
          theme: {themeSource}
        </span>
      </header>

      <div className="app__body">
        <Sidebar activePanel={activePanel} onNavigate={scrollToPanel} />

        <main className="workbench">
          <WorkspaceSection />
          <DiscoverySection />
          <InstrumentSection onGoToTopology={goToTopology} />
          <ScpiSection onGoToTopology={goToTopology} />
          <div className="workbench__row">
            <ScreenshotSection onGoToTopology={goToTopology} />
            <BenchmarkSection onGoToTopology={goToTopology} />
          </div>
          <TimelineSection />
          <ScriptSection onGoToTopology={goToTopology} />
        </main>
      </div>
    </div>
  );
}

export default function App() {
  useWindowGeometry();
  return (
    <WorkspaceProvider>
      <Shell />
    </WorkspaceProvider>
  );
}
