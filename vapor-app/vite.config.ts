/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives the dev server, so the port is fixed and failures must be loud:
// silently falling back to another port leaves the app window pointing at
// nothing.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri is Rust; cargo watches it. Watching it here too causes the
      // frontend to reload on every backend rebuild.
      ignored: ["**/src-tauri/**"],
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
