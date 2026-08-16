# Handover

**Written:** 2026-08-16 (updated after the SYNC family landed)
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

**1b. And grep this tree too, for the inverse.** The same pattern runs the other
way: logic ported faithfully, tested, and then reached by nothing. Playlist
folders (`FolderStore`, `Playlist::folder_id`), the Vibe Limit
(`transition_cost`'s energy threshold) and the Match/Fresh/Switch classifier's
inputs were all sitting in `vapor-library`, working, with no way to reach them —
so three of the four features built on 2026-08-16 were wiring, not engineering.
**The tell in both directions is an argument nobody varies.** Before building a
control, grep for the parameter it would set.

**2. Nobody has heard this app.** Everything is verified by measurement and by
browser tests against a stubbed IPC. No real server, no real library, no
speaker. Do not describe anything as working end to end. **This now extends to
the network too**: `metadata.rs` is tested against canned response bodies and
has never spoken to LRCLIB or Deezer (TD-51).

---

## Running it

`cargo` is not on `PATH`. Every Rust command needs the export, and each `cd` has
to be absolute — a compound `cd` triggers a permission prompt.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd vapor-core           && cargo test --workspace   # 357: unit, property, fuzz
cd vapor-app/src-tauri  && cargo test               # 136: unit, integration
cd vapor-app            && npm test                 # 174: component
cd vapor-app            && npm run e2e              #  39: journeys + monkey
cd vapor-app            && npm run typecheck
```

All five green locally, and green in CI — both workflows, six jobs, including
the Ubuntu Playwright job that had never run anywhere but this Mac.

---

## The state of things

The migration is essentially feature-complete against the Godot build. Every
screen exists, audio plays, tracks mix with six transition types, playlists have
a view and folders to sit in, and a deck costs 1 MiB regardless of track length.
The Vibe screen offers the three exits, badges the DJ's own choice and takes an
override; the Mix Tuner sets the Vibe Limit. Lyrics and artwork can be looked up
from LRCLIB and Deezer, off by default. A test suite was built from a starting
point of zero frontend tests and now covers five layers.

Local sync landed too: two devices on one Wi-Fi find each other, pair with a
code, and move tracks and playlists directly, with a shared document on the
WebDAV server for the times they are never switched on together.

`docs/DESIGN_DRIFT.md` is the record of what the rewrite dropped and what was
put back; its table of ❌s is now closed. **Every ticket that could be resolved
in code has been.** The end of `docs/MIGRATION.md` lists what is left and what
each one is actually waiting for — a fixture corpus, real devices, ears, a
product decision, or a machine someone is sitting at. None of it is waiting on
someone finding the time.

What follows is what is still wrong.

---

## Bugs to fix

Ranked by what would bite a person first.

### 1. Nothing has ever talked to a real server

**The credential store was a mock for the entire life of the project (TD-50)** —
`keyring = "3"` with no backend feature keeps the secret in one `Entry` object
and reports success. Nothing ever reached the keychain, so the WebDAV path has
never worked once, on any machine. That is now fixed, and it means every
downstream claim about the remote path is untested against reality.

Six separate defects reached Dylan when he first tried, and all six are now
fixed: a rename deleting the keychain entry, scan reading the keychain instead
of the form, the password box claiming "unchanged" when nothing was stored, the
result rendering off screen, and **TD-49 — a mistyped folder reporting "Found 0
tracks" instead of saying the folder was not there**, which is the likeliest
explanation for "doesn't show anything to analyze".

**He has never confirmed a scan works.** The keychain had no entry when checked,
which was read at the time as "he never typed one in" — it was TD-50's symptom:

```bash
security find-generic-password -s com.dylangrowcoot.vapormusic.webdav -a "$USERNAME"
```

So the credential and scan paths are fixed *in theory* and unconfirmed in
practice. This is the top item: everything downstream — analysis, playback,
mixing — is unexercised against real files, and none of it can be trusted until
one real library has been through it.

### 2. Nothing on this list has been used by a person

Not one bug, but the shape of all of them. The media controls (MIG-023) are
compiled on three platforms and pressed on none (TD-54). The lyrics lookup
(MIG-052) is parsed from canned bodies and has never spoken to LRCLIB (TD-51).
The credential store is fixed and unconfirmed. Everything below the top item
follows from the top item.

### 3. Key detection is 60.6% exact — TD-11

Up from 56.1%; feeding the chroma from spectral peaks rather than every bin is
what did it, because a drum hit is broadband and was depositing energy into all
twelve pitch classes. Two things already tried and settled, so nobody repeats
them: **segmented analysis shipped** (TD-13) and is not the remaining lever, and
**tuning correction measures 58.1%** — worse than doing nothing — and was
reverted. Needs the personal fixture library to go further (TD-43).

### 4. Process gaps

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
