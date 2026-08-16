# Handoff — building the test suite

**Live document.** If a session ends mid-way, this is where it resumes from.
Delete it when the work in `docs/TESTING.md` is finished and CI is green.

## Why this exists

Five defects reached the person using the app in one sitting: an error bar that
rendered permanently, a notice placed below the fold, a rename that deleted the
stored password, a Scan that could not see what had just been typed, and a
password field that claimed "unchanged" when nothing was stored.

Every one of them is a **frontend or shell-integration** defect. At the time
there were 357 tests and **none of them touched a screen**. The suite tested the
DSP thoroughly and the product not at all.

## Plan

Documented in `docs/TESTING.md` — read that first, it is the spec.

## Progress

| # | Step | State |
|---|---|---|
| 1 | `docs/TESTING.md` written | done |
| 2 | Frontend harness — Vitest, Testing Library, committed IPC fake | done |
| 3 | Component tests — 83 across every screen | done |
| 4 | Backend integration tests — 10 added, set_remote_config extracted so it is testable | done |
| 5 | Property/fuzz tests — 21 across window, library, DSP. Found and fixed a crash | done |
| 6 | E2E journeys + monkey | in progress |
| 7 | CI wiring | not started |

## Commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # cargo is not on PATH by default here

cd vapor-app && npm test               # frontend unit + component
cd vapor-app && npm run typecheck
cd vapor-app/src-tauri && cargo test   # shell
cd vapor-core && cargo test --workspace
```

## Notes for whoever picks this up

* `vapor-app/src/test/ipc.ts` is the fake IPC. It is the committed replacement
  for a throwaway stub that had been rebuilt by hand three times. Its shapes
  must match `src-tauri/src/lib.rs`; `command_bindings.rs` checks the names, not
  the shapes, so a drift here is caught only by these tests failing oddly.
* Do not add a test that asserts an exact user-facing string unless the string
  *is* the behaviour. `errors_are_actionable` used to assert the word
  "Settings" and pinned a defect in place for months.
