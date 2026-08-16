import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests, against the app served with a fake IPC.
 *
 * One browser: the app ships inside a WebView, so a cross-browser matrix would
 * be testing engines it never runs in. Chromium is the closest to the WebView2
 * and WKWebView it does run in, and the component tests already cover the
 * behaviour that is engine-independent.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "line" : "list",
  use: {
    baseURL: "http://localhost:1421",
    // Kept on failure only: a passing monkey run would otherwise write a
    // hundred megabytes of trace nobody reads.
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "npx vite --config vite.e2e.config.ts",
    url: "http://localhost:1421",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
