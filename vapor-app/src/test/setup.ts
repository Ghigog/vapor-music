/**
 * Test environment setup, run before every test file.
 *
 * Two jobs: give Testing Library its matchers, and replace the Tauri IPC with
 * a fake that each test configures. The mock is installed here rather than per
 * file because `@tauri-apps/api/core` is imported at module load by `core.ts` —
 * mocking it late means the real one is already bound.
 */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { FakeBackend, type FakeOptions } from "./ipc";

/** The backend the current test is running against. */
let backend = new FakeBackend();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) =>
    backend.invoke(cmd, args),
}));

// Events are a backend push. Nothing in the fake pushes, so listeners are
// registered and never fire — which is the correct quiescent behaviour and
// keeps a test from waiting on an event that will not come.
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
  emit: async () => {},
}));

/**
 * Install a fresh backend for this test and return it.
 *
 * Call at the top of a test that cares about backend state or wants to make a
 * command fail. Tests that only render can ignore it — `beforeEach` already
 * gives every test a clean one.
 */
export function useBackend(options: FakeOptions = {}): FakeBackend {
  backend = new FakeBackend(options);
  return backend;
}

beforeEach(() => {
  backend = new FakeBackend();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/**
 * jsdom implements neither of these, and the app uses both: the virtualiser
 * measures with `ResizeObserver`, and `scrollIntoView` is how the keyboard
 * cursor keeps itself visible. Absent them, a screen throws on mount and the
 * failure looks like a bug in the screen.
 */
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never;
Element.prototype.scrollIntoView ??= function scrollIntoView() {};
