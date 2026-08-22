/**
 * The app served against the IPC fake, for end-to-end tests.
 *
 * Committed rather than improvised: the same fake the component tests use is
 * aliased in place of the Tauri IPC, so a journey test and a component test
 * cannot form different opinions about what the backend does.
 *
 * This is not the real Rust backend — see the note on `tauri-driver` in
 * docs/TESTING.md for why, and for what covers that seam instead.
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { buildStamp } from "./scripts/build-stamp.mjs";

/*
 * The port is per-session. Several Claude sessions share this machine, and a
 * suite that finds 1421 already listening would otherwise run against whatever
 * frontend that server was built from. Set VAPOR_E2E_PORT to take a port of
 * your own; playwright.config.ts reads the same variable.
 */
const port = Number(process.env.VAPOR_E2E_PORT ?? 1421);

export default defineConfig({
  plugins: [react()],
  // The same stamp as the real build; the About screen renders it either way.
  define: buildStamp(),
  resolve: {
    alias: {
      "@tauri-apps/api/core": path.resolve(__dirname, "src/test/browser-ipc.ts"),
      "@tauri-apps/api/event": path.resolve(__dirname, "src/test/browser-event.ts"),
    },
  },
  // Same allowance as the dev config: the Vibe help sheet imports
  // docs/ai_dj_workflow.md, which sits above this root.
  server: { port, strictPort: true, fs: { allow: [".."] } },
  preview: { port, strictPort: true },
});
