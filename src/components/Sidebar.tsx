/**
 * The workbench sidebar.
 *
 * A persistent session card plus section navigation. It is a jump list, not a
 * tab bar: every section is already visible in the scroll, and clicking a link
 * simply scrolls to it.
 */

import { Button } from "@/components/ui";
import { cx } from "@/lib/cx";
import { SECTIONS } from "@/lib/types";
import { useSession } from "@/state/session";

export interface SidebarProps {
  /** The section currently in view, from the scroll-spy. */
  activeSection: string | null;
  /** Scroll to a section by id. */
  onNavigate: (id: string) => void;
}

export function Sidebar({ activeSection, onNavigate }: SidebarProps) {
  const { session, connecting, disconnect } = useSession();

  return (
    <aside className="sidebar">
      <div className="sidebar__block">
        <h2 className="sidebar__heading">Session</h2>
        {session ? (
          <div className="session-card">
            <p className="session-card__idn mono" title={session.idn}>
              {session.idn}
            </p>
            <dl className="session-card__meta">
              <div className="session-card__row">
                <dt>Address</dt>
                <dd className="mono">{session.address}</dd>
              </div>
              <div className="session-card__row">
                <dt>Transport</dt>
                <dd className="mono">
                  {session.transport}
                  {session.port != null ? `:${session.port}` : ""}
                </dd>
              </div>
            </dl>
            <Button variant="danger" onClick={() => void disconnect()}>
              Disconnect
            </Button>
          </div>
        ) : (
          <p className="sidebar__empty">
            {connecting ? "Connecting…" : "No instrument connected."}
          </p>
        )}
      </div>

      <nav className="sidebar__block" aria-label="Sections">
        <h2 className="sidebar__heading">Sections</h2>
        <ul className="sidebar__nav">
          {SECTIONS.map((section, index) => {
            const isActive = activeSection === section.id;
            return (
              <li key={section.id}>
                <button
                  type="button"
                  className={cx("sidebar__link", isActive && "is-active")}
                  aria-current={isActive ? "true" : undefined}
                  onClick={() => onNavigate(section.id)}
                >
                  <span className="sidebar__link-label">{section.label}</span>
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
