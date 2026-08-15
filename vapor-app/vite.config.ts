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
  // Safari 13 is the floor for the oldest WebKit Tauri targets on macOS.
  build: {
    target: "safari14",
    sourcemap: true,
  },
});
