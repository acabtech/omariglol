/**
 * The workbench sidebar (concepts.md §2.2).
 *
 * The topology view is the central navigation surface and lives here: the
 * active workspace, its instruments grouped by transport, and their trigger
 * dependencies. Below it is the panel jump list — a jump list, not a tab bar,
 * since every panel is already visible in the scroll.
 */

import { TopologyView } from "@/components/TopologyView";
import { cx } from "@/lib/cx";
import { PANELS } from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

export interface SidebarProps {
  /** The panel currently in view, from the scroll-spy. */
  activePanel: string | null;
  /** Scroll to a panel by id. */
  onNavigate: (id: string) => void;
}

export function Sidebar({ activePanel, onNavigate }: SidebarProps) {
  const {
    workspace,
    instruments,
    selectedId,
    select,
    loading,
  } = useWorkspace();

  return (
    <aside className="sidebar">
      <div className="sidebar__block">
        <h2 className="sidebar__heading">Workspace</h2>
        {workspace ? (
          <div className="workspace-card">
            <p className="workspace-card__name" title={workspace.name}>
              {workspace.name}
            </p>
            <p className="workspace-card__meta">
              {workspace.instruments.length} instruments ·{" "}
              {workspace.timeline_len} events
            </p>
          </div>
        ) : (
          <p className="sidebar__empty">
            {loading ? "Loading…" : "No workspace."}
          </p>
        )}
      </div>

      <div className="sidebar__block">
        <h2 className="sidebar__heading">Topology</h2>
        <TopologyView
          instruments={instruments}
          triggers={workspace?.triggers ?? []}
          selectedId={selectedId}
          onSelect={select}
        />
      </div>

      <nav className="sidebar__block" aria-label="Panels">
        <h2 className="sidebar__heading">Panels</h2>
        <ul className="sidebar__nav">
          {PANELS.map((panel, index) => {
            const isActive = activePanel === panel.id;
            return (
              <li key={panel.id}>
                <button
                  type="button"
                  className={cx("sidebar__link", isActive && "is-active")}
                  aria-current={isActive ? "true" : undefined}
                  onClick={() => onNavigate(panel.id)}
                >
                  <span className="sidebar__link-label">{panel.label}</span>
                  <span className="sidebar__link-hint" aria-hidden="true">
                    {index + 1}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
    </aside>
  );
}
