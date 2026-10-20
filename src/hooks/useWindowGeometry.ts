/**
 * Persist and restore the window's position and size.
 *
 * Best-effort and Tauri-only: in a plain browser (or if the window API is
 * unavailable) this is a no-op, so the app still starts and is usable outside
 * Omarchy (§6.7, M12 exit criteria).
 */

import { useEffect } from "react";

import { loadWindowGeometry, saveWindowGeometry } from "@/lib/persist";

/** True when running inside a Tauri webview. */
function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function useWindowGeometry(): void {
  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let timer: number | undefined;
    const unlisten: Array<() => void> = [];

    void (async () => {
      try {
        const [{ getCurrentWindow }, { LogicalPosition, LogicalSize }] =
          await Promise.all([
            import("@tauri-apps/api/window"),
            import("@tauri-apps/api/dpi"),
          ]);
        const win = getCurrentWindow();

        const saved = loadWindowGeometry();
        if (saved && !disposed) {
          try {
            await win.setSize(new LogicalSize(saved.width, saved.height));
            await win.setPosition(new LogicalPosition(saved.x, saved.y));
          } catch {
            // A stale position (e.g. a monitor was removed) is not fatal.
          }
        }

        const persist = async () => {
          try {
            const [size, position, scale] = await Promise.all([
              win.outerSize(),
              win.outerPosition(),
              win.scaleFactor(),
            ]);
            saveWindowGeometry({
              x: Math.round(position.x / scale),
              y: Math.round(position.y / scale),
              width: Math.round(size.width / scale),
              height: Math.round(size.height / scale),
            });
          } catch {
            // Ignore transient failures.
          }
        };

        const schedule = () => {
          if (timer) window.clearTimeout(timer);
          timer = window.setTimeout(() => void persist(), 400);
        };

        unlisten.push(await win.onMoved(schedule));
        unlisten.push(await win.onResized(schedule));
      } catch {
        // The window API is unavailable; geometry persistence is optional.
      }
    })();

    return () => {
      disposed = true;
      if (timer) window.clearTimeout(timer);
      for (const stop of unlisten) stop();
    };
  }, []);
}
