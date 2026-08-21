import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests, against the app served with a fake IPC.
 *
 * One browser: the app ships inside a WebView, so a cross-browser matrix would
 * be testing engines it never runs in. Chromium is the closest to the WebView2
 * and WKWebView it does run in, and the component tests already cover the
 * behaviour that is engine-independent.
 */
// Same variable vite.e2e.config.ts reads: one session per port, so a suite
// never reports against a frontend it did not build.
const port = Number(process.env.VAPOR_E2E_PORT ?? 1421);
const baseURL = `http://localhost:${port}`;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "line" : "list",
  use: {
    baseURL,
    // Kept on failure only: a passing monkey run would otherwise write a
    // hundred megabytes of trace nobody reads.
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  /*
   * Two viewports, because the app has two layouts and only one of them was
   * ever tested.
   *
   * Every mobile defect Dylan hit on the phone — no navigation at all below
   * 768px, headings overprinting each other, titles clipped to three
   * characters — was invisible here, because the only project was a 1280px
   * desktop window where none of those layouts apply.
   *
   * The narrow project runs its own suite rather than the whole one: the
   * journeys reach for the playlist rail and other things the sidebar owns,
   * and a sidebar hidden by design is not a failure worth reporting on every
   * run. What is worth pinning is in `narrow.spec.ts`.
   */
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      testIgnore: /narrow\.spec\.ts/,
    },
    {
      // A real phone preset rather than a small window: it brings the touch
      // input and `hover: none` that the layout branches on.
      name: "narrow",
      use: { ...devices["Pixel 7"] },
      testMatch: /narrow\.spec\.ts/,
    },
  ],
  webServer: {
    command: "npx vite --config vite.e2e.config.ts",
    url: baseURL,
    // Never reuse. Outside CI this used to attach to whatever was already on
    // the port, which on a machine running several sessions meant a green run
    // against another session's frontend. Failing to start is the honest
    // outcome: take a port of your own with VAPOR_E2E_PORT.
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
