/**
 * The IPC fake, wired for a real browser.
 *
 * `src/test/ipc.ts` is the same fake the component tests use; this is the entry
 * point that stands in for `@tauri-apps/api/core` when the app is served by
 * `vite.e2e.config.ts`. One fake behind both layers, so an end-to-end journey
 * and a component test cannot disagree about what the backend does.
 *
 * It also puts the backend on `window` so a test can arrange state or make a
 * command fail from inside the page — the equivalent of `useBackend` in the
 * component tests.
 */
import { FakeBackend, type FakeOptions } from "./ipc";

declare global {
  interface Window {
    __vaporBackend: FakeBackend;
    /** Rebuild the backend with different options and reload. */
    __vaporReset: (options?: FakeOptions) => void;
  }
}

/**
 * Options survive a reload through `sessionStorage`, because the app has no
 * router: a test that wants to start disconnected has to set that up *before*
 * the app boots, and the only way back to boot is a reload.
 */
function storedOptions(): FakeOptions {
  try {
    const raw = sessionStorage.getItem("vapor:fake-options");
    return raw ? (JSON.parse(raw) as FakeOptions) : {};
  } catch {
    return {};
  }
}

const backend = new FakeBackend(storedOptions());
window.__vaporBackend = backend;
window.__vaporReset = (options: FakeOptions = {}) => {
  sessionStorage.setItem("vapor:fake-options", JSON.stringify(options));
  location.reload();
};

export function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return backend.invoke<T>(cmd, args);
}
