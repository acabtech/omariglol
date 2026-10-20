/**
 * A workbench section.
 *
 * The single structural unit of the app: a titled card with an optional
 * description and a small set of actions in its header. Sections are always
 * mounted and stacked in one scroll, so nothing is hidden behind a tab.
 */

import type { ReactNode } from "react";

import { cx } from "@/lib/cx";

export interface SectionProps {
  /** DOM id, used as the scroll anchor and by the scroll-spy. */
  id: string;
  title: string;
  /** A one-line explanation of what the section does. */
  description?: string;
  /** A small set of controls, rendered at the top-right of the header. */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function Section({
  id,
  title,
  description,
  actions,
  children,
  className,
}: SectionProps) {
  return (
    <section
      id={id}
      className={cx("section", className)}
      aria-labelledby={`${id}-title`}
    >
      <header className="section__header">
        <div className="section__heading">
          <h2 id={`${id}-title`} className="section__title">
            {title}
          </h2>
          {description && (
            <p className="section__description">{description}</p>
          )}
        </div>
        {actions && <div className="section__actions">{actions}</div>}
      </header>
      <div className="section__body">{children}</div>
    </section>
  );
}
