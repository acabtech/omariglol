/**
 * Redesigned application shell, matching plans/assets/codesign/mockup-shell.html.
 *
 * A fixed icon rail, sticky top bar and focus strip, and a scrollable canvas of
 * task surfaces. All existing workspace state and panel functionality are preserved.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { useSectionShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useScrollSpy } from "@/hooks/useScrollSpy";
import { useWindowGeometry } from "@/hooks/useWindowGeometry";
import { PANEL_IDS, PANELS } from "@/lib/types";
import type { InstrumentState } from "@/lib/types";
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

const COLLAPSE_MS = 2500;

function railDotClass(state: InstrumentState): string {
  switch (state) {
    case "connected":
      return "ok";
    case "connecting":
      return "info";
    case "error":
    case "offline":
      return "error";
    default:
      return "";
  }
}

function chipTone(state: InstrumentState): string {
  switch (state) {
    case "connected":
      return "chip--ok";
    case "connecting":
      return "chip--info";
    case "error":
    case "offline":
      return "chip--error";
    default:
      return "";
  }
}

function vendorInitial(label: string): string {
  const trimmed = label.trim();
  return trimmed ? trimmed[0]!.toUpperCase() : "?";
}

function BrandMark() {
  return (
    <div className="brand" aria-label="OMARIGLOL">
      <svg
        className="brand-mark"
        viewBox="0 0 420 64"
        role="img"
        aria-label="OMARIGLOL"
        xmlns="http://www.w3.org/2000/svg"
      >
        <title>OMARIGLOL</title>
        <g fill="currentColor">
          <path d="M0 8h40v8H8v32h24v-8h8v16H0V8zm32 8h8v24h-8V16z" />
          <path d="M48 8h12l10 18 10-18h12v48h-8V22l-14 24-14-24v34h-8V8z" />
          <path d="M100 8h32v48h-8V40h-16v16h-8V8zm8 8v16h16V16h-16z" />
          <path d="M140 8h32v24h-8l8 24h-9l-7-24h-8v24h-8V8zm8 8v8h16v-8h-16z" />
          <path d="M180 8h24v8h-8v32h8v8h-24v-8h8V16h-8V8z" />
          <path d="M212 8h40v8h-32v32h24v-8h-8v-8h16v24h-40V8z" />
          <path d="M260 8h8v40h24v8h-32V8z" />
          <path d="M300 8h40v8h-32v32h24v-8h8v16h-40V8zm32 8h8v24h-8V16z" />
          <path d="M348 8h8v40h24v8h-32V8z" />
        </g>
        <rect x="0" y="60" width="96" height="4" opacity="0.35" />
      </svg>
    </div>
  );
}

function Shell() {
  const {
    workspace,
    instruments,
    loading,
    selectedId,
    selected,
    select,
    connectInstrument,
    disconnectInstrument,
  } = useWorkspace();

  // Initialise and follow the live theme; source is not surfaced in this shell.
  void useState<string>("…");
  useScrollSpy(PANEL_IDS, 120);

  const [railPinned, setRailPinned] = useState(false);
  const [autoExpand, setAutoExpand] = useState(true);
  const collapseTimer = useRef<number | null>(null);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void (async () => {
      const palette = await initialiseTheme();
      void palette?.source;
      stop = watchTheme(() => {
        /* theme tokens are applied globally */
      });
    })();
    return () => stop?.();
  }, []);

  const clearCollapse = useCallback(() => {
    if (collapseTimer.current != null) {
      window.clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
  }, []);

  const setExpanded = useCallback((on: boolean) => {
    document.body.dataset.rail = on ? "expanded" : "collapsed";
    document.documentElement.style.setProperty(
      "--om-ui-rail-w",
      on ? "var(--om-ui-rail-expanded)" : "var(--om-ui-rail-collapsed)",
    );
  }, []);

  const scheduleCollapse = useCallback(() => {
    clearCollapse();
    if (railPinned) return;
    collapseTimer.current = window.setTimeout(() => setExpanded(false), COLLAPSE_MS);
  }, [clearCollapse, railPinned, setExpanded]);

  const intentExpand = useCallback(() => {
    if (autoExpand || railPinned) {
      clearCollapse();
      setExpanded(true);
    }
  }, [autoExpand, railPinned, clearCollapse, setExpanded]);

  useEffect(() => {
    document.body.dataset.railPinned = railPinned ? "true" : "false";
    document.body.dataset.autoExpand = autoExpand ? "true" : "false";
    if (railPinned) {
      clearCollapse();
      setExpanded(true);
    } else if (!autoExpand) {
      scheduleCollapse();
    }
    return clearCollapse;
  }, [railPinned, autoExpand, clearCollapse, setExpanded, scheduleCollapse]);

  useEffect(() => {
    setExpanded(railPinned);
    return () => {
      clearCollapse();
      document.body.dataset.rail = "collapsed";
      document.documentElement.style.setProperty(
        "--om-ui-rail-w",
        "var(--om-ui-rail-collapsed)",
      );
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const scrollToPanel = useCallback((id: string) => {
    const element = document.getElementById(id);
    if (!element) return;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    element.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "start" });
  }, []);

  const goToTopology = useCallback(() => scrollToPanel("workspace"), [scrollToPanel]);

  useSectionShortcuts(
    useCallback(
      (index: number) => {
        const panel = PANELS[index];
        if (panel) scrollToPanel(panel.id);
      },
      [scrollToPanel],
    ),
  );

  const connectedCount = instruments.filter((i) => i.state === "connected").length;
  const connectionChipClass = loading
    ? ""
    : connectedCount > 0
      ? "chip--ok"
      : instruments.some((i) => i.state === "error" || i.state === "offline")
        ? "chip--error"
        : "chip--idle";
  const connectionLabel = loading
    ? "loading…"
    : instruments.length === 0
      ? "no instruments"
      : `${connectedCount}/${instruments.length} connected`;

  const onRailBlur = (e: React.FocusEvent<HTMLElement>) => {
    if (railPinned) return;
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
      scheduleCollapse();
    }
  };

  return (
    <div className="app">
      <aside
        className="rail"
        id="rail"
        aria-label="Instruments and views"
        onMouseEnter={intentExpand}
        onMouseLeave={() => {
          if (!railPinned) scheduleCollapse();
        }}
        onFocus={intentExpand}
        onBlur={onRailBlur}
      >
        <div className="rail-head">
          <span className="ico" aria-hidden="true">
            ◫
          </span>
          <span className="txt">Bench</span>
        </div>

        {instruments.map((instrument) => {
          const active = instrument.id === selectedId;
          const dot = railDotClass(instrument.state);
          return (
            <button
              key={instrument.id}
              type="button"
              className={`rail-item${active ? " active" : ""}`}
              aria-current={active ? "true" : undefined}
              title={`${instrument.label} — ${instrument.state}`}
              onClick={() => select(instrument.id)}
            >
              <span className="ico" aria-hidden="true">
                <span className="vendor">{vendorInitial(instrument.label)}</span>
                <span className="gear">⚙</span>
              </span>
              <span className={`rail-state-dot ${dot}`.trim()} />
              <span className="txt">{instrument.label}</span>
            </button>
          );
        })}

        <div className="rail-sep" role="separator" />
        <div className="rail-head">
          <span className="txt">Views</span>
        </div>

        <a className="rail-item" href="#workspace" title="Workspace" onClick={(e) => { e.preventDefault(); scrollToPanel("workspace"); }}>
          <span className="ico" aria-hidden="true">▣</span>
          <span className="txt">Workspace</span>
        </a>
        <a className="rail-item" href="#discovery" title="Discovery" onClick={(e) => { e.preventDefault(); scrollToPanel("discovery"); }}>
          <span className="ico" aria-hidden="true">⌕</span>
          <span className="txt">Discovery</span>
        </a>
        <a className="rail-item" href="#scpi" title="SCPI Console" onClick={(e) => { e.preventDefault(); scrollToPanel("scpi"); }}>
          <span className="ico mono" aria-hidden="true">&gt;_</span>
          <span className="txt">SCPI Console</span>
        </a>
        <a className="rail-item" href="#screenshot" title="Screenshot" onClick={(e) => { e.preventDefault(); scrollToPanel("screenshot"); }}>
          <span className="ico" aria-hidden="true">
            <svg viewBox="0 0 24 24">
              <path d="M4 7h3l2-2h6l2 2h3a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2z" />
              <circle cx="12" cy="13" r="3.2" />
              <path d="M7 7v2" />
            </svg>
          </span>
          <span className="txt">Screenshot</span>
        </a>
        <a className="rail-item" href="#benchmark" title="Benchmark" onClick={(e) => { e.preventDefault(); scrollToPanel("benchmark"); }}>
          <span className="ico" aria-hidden="true">
            <svg viewBox="0 0 24 24">
              <path d="M12 13V8" />
              <path d="M12 13l3 2" />
              <path d="M5.5 17a8 8 0 1 1 13 0" />
              <path d="M7 17h10" />
            </svg>
          </span>
          <span className="txt">Benchmark</span>
        </a>
        <a className="rail-item" href="#timeline" title="Timeline" onClick={(e) => { e.preventDefault(); scrollToPanel("timeline"); }}>
          <span className="ico" aria-hidden="true">
            <svg viewBox="0 0 24 24">
              <circle cx="12" cy="12" r="8" />
              <path d="M12 8v4l3 2" />
              <path d="M4 12h2M18 12h2" />
            </svg>
          </span>
          <span className="txt">Timeline</span>
        </a>
        <a className="rail-item" href="#script" title="Script" onClick={(e) => { e.preventDefault(); scrollToPanel("script"); }}>
          <span className="ico mono" aria-hidden="true">{"{ }"}</span>
          <span className="txt">Script</span>
        </a>
      </aside>

      <header className="topbar">
        <BrandMark />
        <span className="chip">
          Workspace <span className="muted">{workspace?.name ?? "No workspace"}</span>
        </span>
        <span className={`chip ${connectionChipClass}`.trim()} aria-live="polite">
          <span className="dot" />
          {connectionLabel}
        </span>
        <div className="spacer" />
        <button className="btn btn--primary" type="button" onClick={() => scrollToPanel("discovery")}>
          + Connect
        </button>
        <details className="menu">
          <summary className="btn" aria-haspopup="menu">☰ Sidebar</summary>
          <div className="menu-panel" role="menu" aria-label="Sidebar options">
            <label>
              <input
                id="pinToggle"
                type="checkbox"
                checked={railPinned}
                onChange={(e) => setRailPinned(e.target.checked)}
              />{" "}
              Pin expanded rail
            </label>
            <label>
              <input
                id="autoToggle"
                type="checkbox"
                checked={autoExpand}
                onChange={(e) => setAutoExpand(e.target.checked)}
              />{" "}
              Auto-expand on sidebar hover
            </label>
            <p className="muted" style={{ margin: "6px 4px 0", fontSize: 12 }}>
              Expanded rail resizes content (never covers). Auto-expand triggers on rail hover/focus only.
            </p>
          </div>
        </details>
      </header>

      <main className="main-canvas" id="canvas">
        <div className="focus-strip" aria-label="Focused instrument">
          {selected ? (
            <>
              <span
                className={`rail-state-dot ${railDotClass(selected.state)}`.trim()}
                style={{ position: "static", boxShadow: "none" }}
              />
              <span className="name">{selected.label}</span>
              <span className="chip mono">{selected.address}</span>
              <span className={`chip ${chipTone(selected.state)}`.trim()}>
                <span className="dot" />
                {selected.state}
              </span>
              <span className="muted">
                {selected.transport.toUpperCase()}
                {selected.idn ? ` · ${selected.idn}` : ""}
              </span>
              <div className="spacer" />
              {selected.state === "connected" ? (
                <button
                  className="btn"
                  type="button"
                  onClick={() => void disconnectInstrument(selected.id)}
                >
                  Disconnect
                </button>
              ) : (
                <button
                  className="btn"
                  type="button"
                  onClick={() => void connectInstrument(selected.id)}
                >
                  Connect
                </button>
              )}
            </>
          ) : (
            <>
              <span className="name">No instrument selected</span>
              <span className="muted">Select an instrument from the rail or add one in Discovery.</span>
              <div className="spacer" />
              <button className="btn" type="button" onClick={() => scrollToPanel("discovery")}>
                Open Discovery
              </button>
            </>
          )}
        </div>

        <section className="surface-group" id="workspace-group" aria-labelledby="group-setup">
          <h2 className="group-title" id="group-setup">Set up</h2>
          <div className="task-stack">
            <WorkspaceSection />
            <DiscoverySection />
            <InstrumentSection onGoToTopology={goToTopology} />
          </div>
        </section>

        <section className="surface-group" id="scpi-group" aria-labelledby="group-control">
          <h2 className="group-title" id="group-control">Control</h2>
          <div className="task-stack">
            <ScpiSection onGoToTopology={goToTopology} />
          </div>
        </section>

        <section className="surface-group" id="observe" aria-labelledby="group-observe">
          <h2 className="group-title" id="group-observe">Observe</h2>
          <div className="task-grid">
            <ScreenshotSection onGoToTopology={goToTopology} />
            <BenchmarkSection onGoToTopology={goToTopology} />
          </div>
          <div style={{ marginTop: 12 }}>
            <TimelineSection />
          </div>
        </section>

        <section className="surface-group" id="script-group" aria-labelledby="group-automate">
          <h2 className="group-title" id="group-automate">Automate</h2>
          <div className="task-stack">
            <ScriptSection onGoToTopology={goToTopology} />
          </div>
        </section>
      </main>
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
