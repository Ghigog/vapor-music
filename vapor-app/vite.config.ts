/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { buildStamp } from "./scripts/build-stamp.mjs";

// Tauri drives the dev server, so the port is fixed and failures must be loud:
// silently falling back to another port leaves the app window pointing at
// nothing.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // Version and commit, for the About screen. See scripts/build-stamp.mjs.
  define: buildStamp(),
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri is Rust; cargo watches it. Watching it here too causes the
      // frontend to reload on every backend rebuild.
      ignored: ["**/src-tauri/**"],
    },
    fs: {
      // The Vibe help sheet imports docs/ai_dj_workflow.md verbatim, the way
      // the Godot original read it from res://. That file is above this root,
      // so the dev server has to be allowed to serve the repo root.
      allow: [".."],
    },
  },
  // Component tests run in jsdom against the fake IPC in src/test/ipc.ts.
  // Screens are driven the way a person drives them, which is why the
  // environment needs a DOM rather than a mock of one.
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    // e2e/ is Playwright's; running it here would start a browser inside jsdom.
    exclude: ["e2e/**", "node_modules/**"],
    css: false,
    // Five seconds is Vitest's default and it was tuned for a smaller suite.
    // These are Testing Library tests driving a real user-event sequence
    // against jsdom, and the files run in parallel — under that load a single
    // `userEvent.type` of a URL into a form can take seconds on its own.
    //
    // Raised rather than left to fail intermittently: a suite that goes red
    // when the machine is busy teaches everyone to re-run it, and a re-run is
    // how a real failure gets waved through. Nothing here is *waiting* on
    // anything, so a generous ceiling costs nothing when tests pass.
    testTimeout: 20_000,
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/test/**", "src/**/*.test.{ts,tsx}", "src/main.tsx"],
    },
  },

  // Safari 13 is the floor for the oldest WebKit Tauri targets on macOS.
  build: {
    target: "safari14",
    sourcemap: true,
  },
});
