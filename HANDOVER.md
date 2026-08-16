# Handover

**Written:** 2026-08-16
**For:** whoever picks this up next

Read `docs/MIGRATION.md` (the plan), `docs/TECH_DEBT.md` (what is knowingly left
behind) and `docs/TESTING.md` (what is covered and what is not). This file is
the part that is not yet in any of them.

---

## Two rules that keep being earned

**1. Grep the Godot tree before writing a feature.** The port frequently carried
a *parameter* across without its behaviour, so an unused argument looks like a
feature request when it is a half-finished port. Skip history, transition
selection, the energy measure and the vocal-clash mid-cut were each
reimplemented from scratch before anyone checked — all four already existed in
`godot/`. The mid-cut was the clearest case: `vocal_presence` turned out to be
`energy > 0.35`, not a detector, so no detector needed writing.

**2. Nobody has heard this app.** Everything is verified by measurement and by
browser tests against a stubbed IPC. No real server, no real library, no
speaker. Do not describe anything as working end to end.

---

## Running it

`cargo` is not on `PATH`. Every Rust command needs the export, and each `cd` has
to be absolute — a compound `cd` triggers a permission prompt.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd vapor-core           && cargo test --workspace   # 317: unit, property, fuzz
cd vapor-app/src-tauri  && cargo test               #  72: unit, integration
cd vapor-app            && npm test                 #  83: component
cd vapor-app            && npm run e2e              #  17: journeys + monkey
cd vapor-app            && npm run typecheck
```

All five were green at `bac077b`.

---

## The state of things

The migration is essentially feature-complete against the Godot build. Every
screen exists, audio plays, tracks mix with six transition types, playlists have
a view, and a deck costs 1 MiB regardless of track length. A test suite was
built from a starting point of zero frontend tests and now covers five layers.

What follows is what is still wrong.

---

## Bugs to fix

Ranked by what would bite a person first.

### 1. A scan of the wrong folder reports success with zero tracks — TD-49, new

**This is almost certainly what Dylan hit** ("I connected my koofr account but I
don't know if it's able to see my music. doesnt show anything to analyze").

`vapor-app/src-tauri/src/webdav.rs:207`:

```rust
let body = match propfind(&client, &origin, &dir, &auth).await {
    Ok(b) => b,
    // One unreadable directory should not lose the whole library.
    Err(DavError::Auth) => return Err(DavError::Auth),
    Err(_) => continue,
};
```

The comment is right about subdirectories and wrong about the first one. The
base path goes through the same loop, so a `404` on it is swallowed, the queue
drains, and the function returns `Ok(ScanResult { files: [], directories: 1 })`.
The screen then says **"Found 0 tracks"** — which is what an empty library also
says. A wrong folder path is indistinguishable from an empty one, and the folder
path is the single field most likely to be wrong: Koofr wants
`/dav/Koofr/Music`, and `/Music` looks more reasonable.

**Fix:** the base directory's `propfind` must propagate its error; only
subdirectory failures are skippable. Count the skipped ones and report them, so
"found 40 tracks, 2 folders unreadable" is sayable. Both cases are testable from
captured responses at the integration layer — no live server needed.

### 2. Nothing has ever talked to a real server

Four separate defects reached Dylan in one sitting when he first tried, and all
four are fixed: a rename deleting the keychain entry, scan reading the keychain
instead of the form, the password box claiming "unchanged" when nothing was
stored, and the result rendering off screen. **He never came back to confirm a
scan works**, and the keychain had no entry at all when last checked:

```bash
security find-generic-password -s vapor-music -a "$USERNAME"
```

So the credential path is fixed *in theory* and unconfirmed in practice. Fix
TD-49 first — it is likely the thing standing between him and a working scan.

### 3. Fourteen commits are unpushed, so CI has never run

`main` is 14 ahead of `origin/main`. The CI wiring in the last commit —
including a brand-new `end-to-end` job that installs Chromium on Ubuntu — has
**never executed**. The suite has only ever run on this Mac. Expect the first
push to shake out Linux-only problems in the Playwright job specifically.

### 4. The Songs table's header is orphaned ARIA — TD-46

A `role="row"` of `role="columnheader"` buttons sitting above a `listbox` of
`option`s. A `row` must live inside a `table`, `grid` or `treegrid`, and there is
none, so a screen reader announces a table header with no table. Pick one model:
make the whole table a `grid` (which also buys cell-level navigation) or have the
header shed its roles and become plain sort buttons. Announced oddly rather than
unusable, but it gets more expensive as the table grows.

### 5. No media controls on any platform — MIG-023

A parity regression, not a gap. The Godot build answers hardware keys, Control
Center and SMTC (`MediaControlsManager.gd` plus a macOS `.mm` and a Windows
`.cpp`). `vapor-app` answers none of them, **including on macOS, where the old
build works**. One crate (`souvlaki` or equivalent) covers all three desktop
targets from the shell.

### 6. Key detection is 60.6% exact — TD-11

Up from 56.1%; feeding the chroma from spectral peaks rather than every bin is
what did it, because a drum hit is broadband and was depositing energy into all
twelve pitch classes. Two things already tried and settled, so nobody repeats
them: **segmented analysis shipped** (TD-13) and is not the remaining lever, and
**tuning correction measures 58.1%** — worse than doing nothing — and was
reverted. Needs the personal fixture library to go further (TD-43).

### 7. Process gaps

* **TD-41** — the Godot CI job runs without the GDExtension, so DSP-dependent
  tests are uncovered. Deliberate; the tree is being archived, not repaired.
* **TD-43** — the fixture set is a personal library. Nobody else can reproduce
  the analysis numbers. A synthetic corpus would fix it.
* **TD-42** — the 12 failing GUT tests are diagnosed and deliberately not fixed:
  all twelve are stale tests chasing interfaces that moved, no product defect
  behind any of them. Written up so nobody runs it again to find out.

---

## Needs Dylan, not code

Do not start these without asking; each one blocks on something he has and this
process does not.

| What | Why it is blocked |
|---|---|
| **TD-24 / MIG-011** — cpal on iOS and Android | Needs real devices. The least battle-tested part of the audio stack. |
| **TD-22 / MIG-012** — WSOLA vs Rubber Band vs signalsmith | Needs ears. WSOLA is transparent at the ±2% beat-matching uses but was written to prove the approach, not chosen on measured quality. |
| **MIG-002b** — what a corrected BPM should mean | A product decision. |
| **MIG-013** — the browser audio path | The window is portable; the thread that fills it is the shell's, so the browser needs a Worker producer against the same window. |
| **MIG-030–033** — OpenSubsonic | A direction, not a task. |
| **TD-35** — the mark's shape | A design iteration. The app and `design/vapor-mark.js` are byte-identical, so the app is not lagging. |

---

## Traps that have already cost hours

* **`pgrep -f "…"` wait-loops never exit** — the pattern matches the loop's own
  command line. Six of them spun for eight hours before Dylan noticed. Also:
  `pkill -f "vite --config …"` misses the server, because npm launches it as
  `node …/.bin/vite`.
* **Check exit codes, not grepped output.** A pipe through `head` reported a
  non-compiling commit as passing.
* **jsdom has no layout.** Zero-height elements, no `DataTransfer`, no
  `DragEvent` — all stubbed in `src/test/setup.ts`. This is why the BPM-cell bug
  (TD-48) was invisible to 83 component tests and took a real browser: nothing
  moves in jsdom, and that bug was entirely about something moving.
* **A `#[tauri::command]` takes `State`, which cannot be constructed outside a
  running app.** Logic in a command body is logic tests cannot reach. Split it,
  as `set_remote_config` → `apply_remote_config` was.
* **Assert the precondition.** One test here asserted no rows were present while
  the screen was still loading — trivially true, and it would never have failed.
