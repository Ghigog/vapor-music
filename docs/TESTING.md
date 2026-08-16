# Vapor Music — Testing

**Status:** Living document
**Last reviewed:** 2026-08-16

**Current counts:** 357 core (unit + property + fuzz), 136 shell (unit +
integration), 174 component, 39 end-to-end including three monkey seeds.

> What is tested, at which layer, and what is deliberately not. Read alongside
> `docs/MIGRATION.md` (the plan) and `docs/TECH_DEBT.md` (what is knowingly
> left behind).

---

## Why this document exists

The suite had 357 tests and shipped five defects to the first person who tried
to connect a real server in one sitting:

| What happened | What was wrong |
|---|---|
| A red error bar permanently on screen | `ErrorNotice` renders whatever it is given; nothing guarded it |
| "Scan library" appeared to do nothing | Its result rendered below four cards, off screen |
| Correcting a username logged the account out | A rename deleted the keychain entry; an empty password box means "unchanged" |
| "No password saved" while a password was visible in the box | Scan reads the keychain, not the form |
| The box said "unchanged" when nothing was stored | The screen could not ask whether anything was stored |

**Not one of them was in the DSP, and not one was findable by the tests that
existed.** The suite tested tempo estimation against 563 real tracks and did not
test whether a button did anything. That is the imbalance this corrects.

The shape of the failure is worth naming, because it decides the plan below:
every defect lived either **in a screen** or **in the seam between a screen and
the shell**. Those were exactly the two layers with no coverage.

---

## The pyramid, and who owns what

Five layers. Each one exists because the layer below it cannot answer the
question, which is the only good reason to add a layer.

### 1. Unit — pure logic

**Where:** beside the code, in `#[cfg(test)] mod tests`.
**Runs:** `cargo test` in `vapor-core` and `vapor-app/src-tauri`.

The existing strength: 295 tests over the DSP, the mixer, the library logic.
Camelot arithmetic, WSOLA behaviour, LUFS against the standard, envelope shapes,
queue semantics.

Keep writing these first. They are the fastest to run and the most precise when
they fail.

### 2. Property and fuzz — the cases nobody thinks to write

**Where:** `vapor-core/crates/*/tests/prop_*.rs`.
**Runs:** with the unit tests.

Example-based tests check the cases the author imagined. `proptest` generates
thousands that nobody imagined, shrinks a failure to its smallest form, and is
the closest thing to monkey testing that a pure function can have.

Reserved for code with an **invariant that can be stated**:

* A queue that is shuffled and unshuffled holds the same tracks.
* Rate conversion output length follows the ratio, for every ratio.
* A window's frames read back at the position they were written to, for every
  sequence of writes and reads.
* Camelot distance is symmetric, and zero only for a key with itself.

Not for code whose correct answer is a matter of taste or measurement — key
detection accuracy is a *number against a fixture set*, not a property.

### 3. Integration — the shell's command surface

**Where:** `vapor-app/src-tauri/src/lib.rs`'s test module, and `tests/`.
**Runs:** `cargo test`.

A real `AppState` on a temporary data directory, loaded, mutated and saved.
This is the layer that would have caught the rename deleting a password,
because that defect lived entirely inside one command's behaviour and was
invisible to any unit test of the pieces.

**A `#[tauri::command]` takes `State`, which cannot be constructed outside a
running app** — so a command whose logic sits in its own body cannot be tested
at this layer at all. That is why `set_remote_config` is now a thin wrapper
over `apply_remote_config(&mut AppState, ..)`. Any command that grows real
logic should be split the same way; a command body is a place tests cannot
reach.

Inside `lib.rs` rather than `tests/` because `AppState` is private and should
stay so — the alternative is widening the crate's public surface for the
benefit of tests, which is worse.

### 4. Component — screens, against a fake IPC

**Where:** `vapor-app/src/**/*.test.tsx`.
**Runs:** `npm test` (Vitest + Testing Library, jsdom).

**The layer that did not exist.** Every screen rendered, driven the way a person
drives it — click the button, type in the box, read what it says back.

The IPC is faked at `src/test/ipc.ts`: one module implementing every command
with the real shapes, so a screen can be rendered without a backend. That fake
is committed and shared, not rebuilt per test — it had already been written by
hand three times as a throwaway before this.

What this layer is for:

* **Does the button do the thing.** Pressing Scan calls the scan command.
* **Does the screen say what happened**, where the person is looking.
* **Unhappy paths.** Every command rejected in turn, asserting the screen says
  so rather than failing silently or hanging on a spinner.
* **States.** Empty, loading, error, populated — for every screen that has them.

Queried by role and text, as a person perceives the screen, not by CSS class.
A test that clicks `.settings__button--primary` passes when the button has been
relabelled to something meaningless.

### 5. End-to-end — journeys, and a monkey

**Where:** `vapor-app/e2e/`.
**Runs:** `npm run e2e` (Playwright).

The real UI in a real browser, against the same fake IPC. Whole journeys rather
than single screens: first run through to something playing, build a playlist
from the table, correct a tempo and see it in the mix.

Plus a **monkey**: a seeded random walk that clicks, types and navigates for a
few thousand actions and asserts the app never throws, never blanks, and never
leaves a spinner running. Seeded so a failure is reproducible from its seed —
an unreproducible monkey failure is a rumour.

Plus an **affordance sweep** (`affordances.spec.ts`). For every screen that
lists tracks it asserts three things: that no visible track title sits outside
something pressable, that pressing one puts that track in the transport, and
that the keyboard can do the same. The first of those is generic — it walks the
content column and reports any title with no operable ancestor — so it covers
screens nobody has written a test for yet. See rule 6.

#### Why not drive the real Tauri binary

`tauri-driver` exists and would exercise the true Rust backend. It is not used
here because it needs a built app per platform per run and a WebDriver that is
Linux/Windows only — on macOS, which is where this is developed, it does not
work at all. The seam it would cover, *commands wired to screens*, is instead
covered from both sides: `command_bindings.rs` proves every command has a
binding, and the component tests prove every binding is called. That is not the
same as end-to-end through the real backend, and it is written down here rather
than implied.

---

## What is deliberately not tested

Recorded so nobody "fixes" the gap without reading why.

* **Audio you can hear.** No test asserts a mix sounds good. Beat alignment is
  measured in milliseconds, clipping in samples, and dropouts in allocation
  counts — all objective. Whether it sounds *right* needs ears and is the one
  thing the suite cannot do.
* **The real WebDAV server.** Nothing in CI talks to Koofr. The transport is
  thin and the parsing is tested from captured responses; a live server in CI
  buys flakiness and a credential in a secret store.
* **LRCLIB and Deezer.** Same reasoning, one degree worse: `metadata.rs` splits
  its parsing from its transport so every response shape is driven from a
  canned string, but those shapes were read out of `metadata_service.gd` rather
  than off the wire, and nothing has ever confirmed them (TD-51). If a lookup
  comes back empty on a real machine, suspect the shape before the parser.
* **Analysis accuracy in CI.** The 563-track figures need a personal library
  (TD-43). CI runs the DSP against synthetic signals; the fixture numbers are
  produced by hand and recorded in `docs/MIGRATION.md`.
* **The Godot tree.** Pinned at a known-failing baseline and diagnosed in TD-42.
  It is being archived, not repaired.
* **Visual appearance.** No screenshot diffing. The design is still moving and a
  snapshot suite would be a wall of noise. Layout *behaviour* that matters —
  a notice being reachable, a control being focusable — is asserted directly.

---

## Rules

Seven, each earned by a specific failure in this repository.

**1. Assert behaviour, not wording.** `errors_are_actionable` asserted an error
message contained the word "Settings". It did — while being displayed *on* the
Settings screen, telling the reader to go where they already were. The test
pinned the defect. Assert what the message must *achieve*: that it names the
field and the action.

**2. A test that cannot fail is worse than no test.** Every counting-allocator
test here has a companion asserting the counter observes a known allocation.
Where a test could pass vacuously — a mix that never ran, a measurement over an
empty range — assert the precondition explicitly.

**3. Measure, do not infer.** Rendering audio and measuring onsets caught an
inverted tempo ratio that a unit test had asserted *the wrong way round* and
passed. Where the answer is a number, produce the number.

**4. Cross the boundary the app crosses.** The keychain was a mock store for
the whole life of the project (TD-50): saving returned `Ok(())` and the secret
lived inside that one `Entry` object. Every test passed, because no test ever
saved in one `Entry` and read in another — which is what the app does on every
call, since each command builds its own. `save(..).is_ok()` asserted nothing.
Where a component is used across a process, a request or an object lifetime,
the test has to cross that same line.

**5. One fake, shared.** The IPC fake is a committed module. Three separate
hand-rolled stubs had already been written and thrown away, each with slightly
different shapes, which is how a fake drifts from the thing it fakes.

**6. Rendering is not working.** Library's album cards were `<article>`
elements with no click handler for the whole life of the screen: the home
screen, showing the user their own music, and pressing a track did nothing. The
component test asked whether the grid rendered and whether the tabs regrouped.
The end-to-end sweep asserted, for this screen and seven others,
`expect(page.getByRole("main")).not.toBeEmpty()`. Every one passed. A screen has
a primary action — the reason a person opened it — and a test that never
performs it has not tested the screen. Where a control exists, press it and
assert what a person would look at to see whether it worked.

**7. A fake that cannot be wrong cannot catch anything.** `mix_candidates` was
faked by slicing three arbitrary rows and labelling them Match, Fresh and
Switch in order — so "one card per kind" was true by construction and no
fixture could have falsified it. The fake now classifies by the same rule the
engine does. In the same file, `track_details` returned `cover: null`
unconditionally while every other cover-bearing command honoured the `covers`
option, so no test of Liner Notes could tell a track with artwork from one
without. **A fake is allowed to be simpler than the backend; it is not allowed
to be incapable of the answer the test is looking for.**

---

## Running it

```bash
export PATH="$HOME/.cargo/bin:$PATH"     # cargo is not on PATH by default

cd vapor-core        && cargo test --workspace     # unit + property
cd vapor-app/src-tauri && cargo test               # unit + integration
cd vapor-app         && npm test                   # component
cd vapor-app         && npm run e2e                # journeys + monkey
cd vapor-app         && npm run typecheck
```

## In CI

`.github/workflows/` runs every layer on every push.

| Workflow | Job | Layer | Platforms |
|---|---|---|---|
| `ci.yml` | `core` | unit, property, fuzz | macOS, Windows, Linux |
| `ci.yml` | `wasm` | the core still builds for the browser | Linux |
| `ci.yml` | `godot` | the retiring tree, at a pinned baseline | macOS |
| `app.yml` | `frontend` | typecheck, component | Linux |
| `app.yml` | `end-to-end` | journeys, monkey | Linux |
| `app.yml` | `shell` | unit, integration | macOS, Linux |

`app.yml` is path-filtered to `vapor-app/**` and `vapor-core/**`, so a commit
touching only the Godot tree or the docs does not pay for a Tauri build.

The three-platform matrix is not ceremony: the audio device, the keychain and
the filesystem all differ, and the app has to open a device on each.
