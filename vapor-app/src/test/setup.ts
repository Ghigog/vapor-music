/**
 * Test environment setup, run before every test file.
 *
 * Two jobs: give Testing Library its matchers, and replace the Tauri IPC with
 * a fake that each test configures. The mock is installed here rather than per
 * file because `@tauri-apps/api/core` is imported at module load by `core.ts` —
 * mocking it late means the real one is already bound.
 */
import "@testing-library/jest-dom/vitest";
import { forgetCovers } from "../lib/artwork";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { FakeBackend, type FakeOptions } from "./ipc";

/** The backend the current test is running against. */
let backend = new FakeBackend();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) =>
    backend.invoke(cmd, args),
}));

/**
 * Listeners registered through the mocked `listen`, by event name.
 *
 * Events are a backend push, and nothing in the fake pushes on its own — a
 * listener that never fires is the correct quiescent behaviour and keeps a test
 * from waiting on an event that will not come. A test that wants one calls
 * `emitEvent`.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (e: { payload: unknown }) => void) => {
    let set = listeners.get(name);
    if (!set) {
      set = new Set();
      listeners.set(name, set);
    }
    set.add(handler);
    return () => {
      set.delete(handler);
    };
  },
  emit: async () => {},
}));

/**
 * Push a backend event to whatever is listening for it.
 *
 * Synchronous on purpose: the caller wraps it in `act` and asserts straight
 * after, rather than waiting on a timer that has nothing to do with the
 * behaviour under test.
 */
export function emitEvent(name: string, payload: unknown): void {
  for (const handler of listeners.get(name) ?? []) {
    handler({ payload });
  }
}

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
  // Both outlive a test the way module state does: a remembered theme would
  // decide the next test's colours, and a dark OS preference would decide what
  // `auto` resolves to in a test that never mentioned it.
  localStorage.clear();
  setPrefersDark(false);
  // The cover cache is module state and outlives a test. Left alone, one test's
  // artwork answers the next one's rows and a fetch that should have happened
  // never does.
  forgetCovers();
  // `cleanup` unmounts, which runs each effect's teardown and so unregisters
  // the listeners it registered. Clearing anyway: a listener surviving into the
  // next test would fire on its events and be very hard to explain.
  listeners.clear();
  vi.clearAllMocks();
});

/**
 * jsdom has no layout engine, and the table depends on one.
 *
 * Everything has a zero-sized bounding rect, so `@tanstack/react-virtual`
 * measures a viewport of height 0 and correctly renders no rows at all. A test
 * then sees an empty table and reads as "the screen is broken" when the screen
 * is fine and the environment has no pixels.
 *
 * So: give elements a plausible size. 1024x800 is arbitrary but has to be big
 * enough to hold several rows, or the virtualisation tests measure nothing.
 */
const VIEWPORT = { width: 1024, height: 800 };

class ResizeObserverStub {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element) {
    // Report a size immediately. The virtualiser subscribes and then waits;
    // without a first callback it never learns the viewport is not zero.
    this.callback(
      [{ target, contentRect: target.getBoundingClientRect() } as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never;

Element.prototype.getBoundingClientRect = function getBoundingClientRect() {
  return {
    width: VIEWPORT.width,
    height: VIEWPORT.height,
    top: 0,
    left: 0,
    right: VIEWPORT.width,
    bottom: VIEWPORT.height,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect;
};

for (const [prop, value] of [
  ["offsetWidth", VIEWPORT.width],
  ["offsetHeight", VIEWPORT.height],
  ["clientWidth", VIEWPORT.width],
  ["clientHeight", VIEWPORT.height],
] as const) {
  Object.defineProperty(HTMLElement.prototype, prop, {
    configurable: true,
    value,
  });
}

// The keyboard cursor keeps itself visible with this; jsdom does not have it,
// and its absence throws on the first arrow key.
Element.prototype.scrollIntoView ??= function scrollIntoView() {};

/**
 * jsdom implements `DragEvent` but not `DataTransfer`, so a drag payload cannot
 * be constructed — which makes drag and drop untestable, which is how it
 * shipped untested.
 *
 * This is the whole contract the app uses: set a typed payload, read it back,
 * and ask which types are present so a foreign drag can be refused.
 */
class DataTransferStub {
  private data = new Map<string, string>();
  dropEffect = "none";
  effectAllowed = "all";
  readonly files: File[] = [];

  get types(): string[] {
    return [...this.data.keys()];
  }

  setData(format: string, value: string) {
    this.data.set(format, value);
  }

  getData(format: string): string {
    return this.data.get(format) ?? "";
  }

  clearData(format?: string) {
    if (format) this.data.delete(format);
    else this.data.clear();
  }

  setDragImage() {}
}

globalThis.DataTransfer ??= DataTransferStub as never;

/**
 * jsdom has no `DragEvent` either — it is a `MouseEvent` carrying a
 * `dataTransfer`, which is exactly what this provides. Without it every drag
 * and drop path in the app is untestable, and untestable is how it shipped.
 */
class DragEventStub extends MouseEvent {
  readonly dataTransfer: DataTransfer | null;
  constructor(type: string, init: MouseEventInit & { dataTransfer?: DataTransfer } = {}) {
    super(type, init);
    this.dataTransfer = init.dataTransfer ?? new DataTransfer();
  }
}

globalThis.DragEvent ??= DragEventStub as never;

/* ---- Two things this jsdom does not have ------------------------------
 *
 * `localStorage` and `matchMedia` are both absent here, and the theme layer
 * uses both — one to remember the choice across launches, the other to read
 * the OS preference under `auto`. The app guards for their absence, so their
 * being missing did not fail anything; it just made the whole of `auto`
 * untestable, which is the half most likely to break.
 * -------------------------------------------------------------------- */

class StorageStub implements Storage {
  private items = new Map<string, string>();

  get length(): number {
    return this.items.size;
  }
  key(index: number): string | null {
    return [...this.items.keys()][index] ?? null;
  }
  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.items.set(key, String(value));
  }
  removeItem(key: string): void {
    this.items.delete(key);
  }
  clear(): void {
    this.items.clear();
  }
}

globalThis.localStorage ??= new StorageStub() as never;

/** Whether the fake OS is currently asking for dark. */
let prefersDark = false;
const mediaListeners = new Set<(e: MediaQueryListEvent) => void>();

/**
 * Flip the OS preference mid-test and tell everything listening.
 *
 * This is what `auto` is for, so a test of `auto` that cannot move it is only
 * testing the light half.
 */
export function setPrefersDark(dark: boolean): void {
  if (dark === prefersDark) return;
  prefersDark = dark;
  const event = { matches: dark, media: "(prefers-color-scheme: dark)" };
  for (const listener of mediaListeners) {
    listener(event as MediaQueryListEvent);
  }
}

globalThis.matchMedia ??= ((query: string) => ({
  get matches() {
    return prefersDark && query.includes("prefers-color-scheme: dark");
  },
  media: query,
  onchange: null,
  addEventListener: (_: string, listener: (e: MediaQueryListEvent) => void) =>
    void mediaListeners.add(listener),
  removeEventListener: (_: string, listener: (e: MediaQueryListEvent) => void) =>
    void mediaListeners.delete(listener),
  addListener: () => {},
  removeListener: () => {},
  dispatchEvent: () => false,
})) as never;
