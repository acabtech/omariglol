/**
 * Keyboard conventions (UX journey §3.8).
 *
 * The global bindings live here; section-local bindings (SCPI history, script
 * run, cancel) are handled inside their sections so they can see local state.
 */

import { useEffect } from "react";

/** True when the event target is a text-entry control. */
export function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable
  );
}

/**
 * Bind `Ctrl+1` … `Ctrl+5` to the workbench sections.
 *
 * `onSelect` receives a zero-based section index. The binding never conflicts
 * with typing, so it stays active while a field has focus.
 */
export function useSectionShortcuts(onSelect: (index: number) => void): void {
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const index = Number(event.key) - 1;
      if (Number.isInteger(index) && index >= 0 && index <= 4) {
        event.preventDefault();
        onSelect(index);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onSelect]);
}
