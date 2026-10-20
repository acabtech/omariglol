/**
 * Track which workbench section is currently in view.
 *
 * The app is one continuous scroll, so the sidebar highlights the section the
 * user is reading rather than a selected tab. An `IntersectionObserver` keeps
 * this cheap: only section boundaries trigger work, never the scroll itself.
 */

import { useEffect, useState } from "react";

/**
 * Observe `ids` and return the id of the topmost section in view.
 *
 * `topOffset` accounts for the sticky top bar so a section counts as active
 * once its heading clears the chrome.
 */
export function useScrollSpy(
  ids: ReadonlyArray<string>,
  topOffset = 64,
): string | null {
  const [active, setActive] = useState<string | null>(ids[0] ?? null);

  useEffect(() => {
    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => element !== null);
    if (elements.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort(
            (a, b) => a.boundingClientRect.top - b.boundingClientRect.top,
          );
        if (visible.length > 0) {
          setActive(visible[0].target.id);
        }
      },
      {
        // A section is "active" while it occupies the band just below the
        // top bar; the bottom margin biases the choice toward the top.
        rootMargin: `-${topOffset}px 0px -55% 0px`,
        threshold: 0,
      },
    );

    for (const element of elements) observer.observe(element);
    return () => observer.disconnect();
  }, [ids, topOffset]);

  return active;
}
