import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// Tauri sets TAURI_DEV_HOST when developing against a device on the LAN.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],

  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // Tauri shows its own output; don't let Vite wipe it.
  clearScreen: false,

  server: {
    port: 1420,
    // Fail loudly rather than silently drifting to another port, which would
    // break the devUrl in tauri.conf.json.
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // The Rust side has its own watcher.
      ignored: ["**/src-tauri/**"],
    },
  },

  build: {
    // Tauri ships a modern webview, so we can target current syntax.
    target: "esnext",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    // Leave minification to Vite's default bundler unless debugging, where
    // readable output is worth more than bundle size.
    ...(process.env.TAURI_ENV_DEBUG ? { minify: false as const } : {}),
  },
});
