# OPS — the CI and release-pipeline epic

Opened 2026-08-27, at Dylan's request, immediately after `v2.0.0-rc.8` became
the first build ever shown to launch and play on a phone.

Nothing in here is started. It is parked deliberately: a mobile crash was
reported the same evening and takes precedence. This file exists so the
argument and the measurements survive until someone picks it up, because both
were expensive to get and neither is written down anywhere else.

`docs/TESTING.md` is the reference for what the tests cover. This file is about
what the pipeline *costs* and what it is allowed to block.

## Why this was opened

Dylan, 2026-08-27:

> I think we're being a bit fascist with our CI jobs here; I don't mind some
> sloppy releases just as long as the app is working. [...] It would also make
> the CI much much quicker and we can iterate quicker. I want to get this CI
> down from 50 minutes to, like, 5.

The trigger was `docs_state_claims` failing the entire pipeline over one stale
sentence in `docs/RELEASE.md` — a prose linter taking down three operating
systems. That specific complaint is correct and the fix is cheap.

## The measurements, which changed the plan

Taken from run `33099819390` (`app.yml`, 2026-08-27, on a docs-only commit)
and run `33088493217` (`release.yml`, `v2.0.0-rc.8`).

**Push CI — 14m35s total, not 50.**

| Job | Wall | Detail |
|---|---|---|
| `android (compile)` | 41s | `check` 7s, `clippy` 4s |
| `frontend` | 44s | typecheck 5s, component tests 19s, build 7s |
| `end-to-end` | 1m56s | Chromium install 28s, journeys+monkey 74s |
| `shell (ubuntu)` | 1m58s | **`cargo check` 8s**, `cargo test` 54s |
| `shell (macos)` | 14m30s | **`cargo check` 6m06s** |
| `shell (windows)` | 14m31s | **`cargo check` 11m24s** |

The macOS and Windows jobs were cancelled mid-run, so their totals are floors;
the `Check` step timings are complete and exact.

**Release — 49m12s.** The desktop matrix completes at roughly 34 minutes with
Windows as the long pole; Android then starts and adds about 15.

**What the numbers say.** Every test in the repository — component, e2e, the
whole `cargo test` suite — costs about **two minutes**. Delete all of them and
push CI still takes fourteen, because Windows spends 11m24s in `cargo check`
before a test runs. Ubuntu did the identical `cargo check` in **8 seconds** on
a warm cache: an 85× spread that is cache behaviour, not scope.

So the 5-minute target is reachable, and **removing tests is not the lever that
reaches it.** Not compiling the same Rust on three desktop operating systems on
every push is.

## The disagreement, kept because both halves are right

**Dylan's half.** A severity mismatch is a design bug. `docs_state_claims`
reads markdown and currently runs three times, on three operating systems,
behind an eleven-minute Rust compile, and can take the pipeline down over a
sentence. Compiling identical Rust on three desktop targets every push is
expensive insurance for how rarely it catches anything.

**The other half.** "Sloppy is fine as long as the app works" did not survive
the week that produced it: `v2.0.0-rc.3` through `rc.7` all shipped through a
*green* desktop pipeline and none of them started on a phone. The tests did not
cost that evening; their absence did.

And the sharper one: **a permanently red `main` is more dangerous than a slow
one.** The Windows shell job broke on 2026-08-24 (AND-6) and eight releases
went out past it, because red was the normal state and nobody looked. Fast CI
is worth a great deal. Ignorable CI is worth less than none. Whatever this epic
does to the tiers, the end state has to be that red means something.

## Tier 1 — blocks every push, target under 3 minutes

Everything here already exists and already passes in about two minutes.

* `frontend` — typecheck, component tests, `build:web`
* `shell (ubuntu-latest)` only — fmt, check, clippy, test
* `android (compile)` — check and clippy against the NDK
* `end-to-end` — kept in tier 1 at 1m56s; cheap for what it covers
* **docs lint as its own job.** Pull `docs_state_claims` out of the shell
  matrix so a prose failure is a ten-second red square next to a green
  pipeline, rather than the pipeline. It has no platform dimension and is
  currently run three times for no reason.

## Tier 2 — does not block the push

* `shell (macos-latest)` and `shell (windows-latest)`

Run them nightly and on release tags. The coverage is kept; what stops is
paying six and eleven minutes for it on a documentation commit. AND-6 is the
argument for keeping them at all — the Windows-only failure was real, and only
this job sees it.

**Open question for whoever picks this up:** nightly means a failure is
attributed to a day rather than a commit. With this repository's push rate that
is probably the right trade, but it is a trade and should be made on purpose.

## Tier 3 — the release pipeline

Mostly irreducible: it builds four real platform bundles and that is what it
costs. One free win:

* **Android is serialized behind the entire desktop matrix.** Not because it
  needs the desktop build — because it uploads into the draft release that
  `tauri-action` creates in the matrix. Decoupling that (have the Android job
  create the draft if it is absent, or depend on one platform rather than all
  three) returns roughly **15 minutes** with no coverage lost.

Windows at ~34 minutes is the remaining long pole. Left alone for now;
`sccache` or a better cache key is a separate investigation and belongs behind
the tiering.

## The one thing to add rather than remove

**A smoke test that launches the built application.** Nothing in this
repository has ever started this app on any platform in CI. That is precisely
the gap that cost five tags: `verify-apk.mjs` reads an APK's contents, the e2e
suite drives the frontend in a browser, and neither can see a process that dies
before `main`.

Shape, unresearched and deliberately so — this is the item that wants design
rather than plumbing:

* **Desktop:** launch the bundled binary headless (Xvfb on Linux), wait for the
  webview to report the app booted, exit 0. Catches AND-2 and AND-6's class
  entirely.
* **Android:** an emulator in CI, `monkey -p ... 1`, then read logcat for
  `FATAL EXCEPTION`. This would have caught AND-5 in ten seconds. Emulators in
  CI are slow and flaky, so it belongs in tier 2 or on tags, never tier 1.

Written down as wanted, not committed.

## Explicitly not proposed

* **Deleting tests to go faster.** The measurements say it buys two minutes and
  costs the only coverage that exists.
* **Dropping Windows or macOS from CI entirely.** AND-6 is a live example of a
  platform-specific failure that only that job can see.
* **Turning off `docs_state_claims`.** It caught a genuinely false statement
  that had been read back as true for three days, which is the exact failure it
  was written for. It is in the wrong tier, not wrong.

## Order

1. Docs lint into its own job — smallest, and removes the complaint that opened
   this epic.
2. macOS and Windows to tier 2.
3. Android decoupled in `release.yml`.
4. Smoke test, as its own design conversation.

Items 1–3 are plumbing and could land together. Item 4 should not be rushed to
join them.
