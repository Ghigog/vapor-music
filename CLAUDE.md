# Vapor Music

The application is **`vapor-app/`** (Tauri 2 shell in `src-tauri/`, React 19 in
`src/`) on top of **`vapor-core/`** (the Rust crates: `vapor-dsp`,
`vapor-engine`, `vapor-library`). Those two directories are the whole project.

The Godot original was deleted on 2026-08-21 — 479 files, the repo root's
`src/`, `scenes/`, `autoloads/`, `scripts/`, `addons/`, `assets/`, `tests/`,
`project.godot` and `SConstruct`. It is not gone: the tag
**`godot-final-v1.78`** is on origin and its tree was byte-identical to what
was removed, so `git show godot-final-v1.78:<path>` reads any of it back.
Nothing needs to be ported out of it; if something does, take it from the tag.

Worth reading: `docs/TESTING.md`, `docs/FINDINGS.md`, `docs/RELEASE.md`,
`docs/LICENSING.md`, `docs/DECISIONS.md`, `docs/ANDROID.md`,
`docs/theme_system.md`.

## Answer the question that was asked

Work you find on the way to something else is **not** yours to start. Say what
you found, in a sentence, and ask. Dylan decides whether it is worth doing now.

Fixing something you broke in this session is the exception, and only that. It
does not extend to what you notice while fixing it.

Written 2026-08-21, from a session that was asked where a file lived, and
answered forty minutes later having rewritten two clippy lints it did not cause
and read four CI logs. The question went unanswered the whole time.

## Several Claude sessions run in this repo at once

Dylan runs two or three at a time, each briefed on its own scope. Assume you are
not alone.

**Work in a worktree unless the task is one file or none.** Reading the code,
answering a question, a single-file fix, a doc edit — stay in the main checkout.
Anything else, create one and start there. Do not ask first; the cost of a wrong
yes is 152 MB and a minute, and the cost of a wrong no is on record below.

Three commits in this history carry work their author did not do — `278cb5d`,
`050281f`, `eb5fa70` — and each says so in its own message. `278cb5d` landed 63
minutes after the rule against it was written:

> "Not mine, and committed rather than left loose because the tree was being
> committed whole."

That rule did not fail through carelessness. A session holding a dirty tree it
cannot attribute has to choose between leaving the tree dirty and committing it,
and tidiness wins. A worktree removes the choice.

**If you are in the main checkout and `git status` shows work you did not do,
you may commit it — on three conditions.** Relaxed 2026-08-21, at Dylan's
request: the priority is that the tree stays current and conflict-free, not that
loose work sits untouched out of politeness.

1. **Its own commit**, never folded into yours. That was 278cb5d's actual
   mistake — not that it committed someone else's work, but that the work
   arrived inside a message about something else and nobody could see it.
2. **Say whose it is** in the message, and say you could not verify it. You
   cannot tell your own older edits from someone else's; `git status` has no
   author column.
3. **Check it first.** Committing in-flight work can capture a half-finished
   state. Run whatever gate covers it — `npm run typecheck`, `cargo check`, the
   test suite — and if it does not pass, leave it and say so in your reply.
   A red commit helps nobody.

`?? test-results/` is Playwright output and belongs to nobody; leave it.

**The hard rules did not move. Stage explicit paths; never `git add -A`,
`git add .`, `git commit -a`, `git stash`, `git reset --hard`, or a whole-tree
`checkout`/`restore`.** These are a different thing from committing: a blanket
stage sweeps up work nobody looked at, and `stash` is worse still — it takes
another session's changes out of the tree entirely, and they watch their edits
vanish mid-task. Committing leaves everything where it is.

Those are enforced rather than trusted — `.claude/hooks/guard-git-staging.sh`
denies each of them and names what to run instead. If it fires, it is not a
hurdle to work around; the command was about to take something out of the tree
that was not yours.

It ignores prose — heredoc bodies and quoted `-m` arguments are stripped before
matching, so a commit message may name the commands it blocks. If you change the
guard, `bash .claude/hooks/guard-git-staging.test.sh` is beside it.

## Before you start, every time

**This now runs on its own.** `.claude/hooks/session-survey.sh` fires on
SessionStart and puts the answer in your context before your first message:
which trees are dirty and with what, which lanes are occupied, which seams are
being touched, which worktrees are stale, and whether 1420 or 1421 is taken.

Run it by hand any time the picture may have moved — after a long turn, or
before starting something big:

```
bash .claude/hooks/session-survey.sh
```

It was made automatic because the manual version was not run. Worktrees isolate
**trees, not tasks**. On 2026-08-21 two sessions independently diagnosed the
same wry/WKWebView drag bug and wrote the same one-line fix to
`tauri.conf.json`, hours apart, in separate worktrees — byte-identical, and
neither knew. Isolation prevented the collision; only looking prevents the
duplication, and looking is exactly the step a session skips when it is keen to
start.

## Lanes

Pick tasks from different lanes and two sessions will rarely meet.

| Lane | Globs |
|------|-------|
| **shell** — Tauri/Rust backend | `vapor-app/src-tauri/src/**`, `src-tauri/Cargo.*`, `src-tauri/capabilities/**` |
| **screens** — React screens | `vapor-app/src/screens/**`, `vapor-app/src/App.tsx` |
| **components** — shared widgets | `vapor-app/src/components/**` |
| **core** — DSP, engine, library | `vapor-core/**` |
| **platform** — build, CI, packaging | `src-tauri/src/android.rs`, `.github/workflows/**`, `vapor-app/scripts/**`, `docs/RELEASE.md`, `docs/ANDROID.md` |
| **docs** | `docs/**`, `README.md`, `THIRD_PARTY_NOTICES.md`, `LICENSE`, `licenses/**` |

`core` is the cleanest lane in the repo — zero recorded collisions. `components`
is the dirtiest: `PlaylistRail`, `SyncPanel` and `TabMenu` get pulled in by
several features at once.

### Seams, where lanes unavoidably meet

| Seam | Rule |
|------|------|
| `src-tauri/src/lib.rs` | Over 10,000 lines and ~100 commands behind one mutex — the only door to the backend. **One session in the backend at a time** until it is split into `commands/<domain>.rs`. |
| `vapor-app/src/lib/generated/**` | Generated from Rust by `ts-rs`. Never hand-edit: the shell lane regenerates, frontend lanes read. `npm run types:check` already fails on drift — this seam is solved, and is the template for the others. |
| `vapor-app/src/lib/core.ts` | Additive only. Add your wrapper; restructuring the file is its own task with no other session live. |
| `vapor-app/src/app.css`, `tokens.css` | Additive only, append at the end of the token block. Renaming or reorganising a token is its own task. |
| `vapor-app/src/test/ipc.ts` | Holds state and answers, no logic. Add fixtures, never decisions. A diff here that adds `if` or `sort` is the regression signal. |

## Ports and builds are machine-global

Worktree or not, these are shared across every session on the machine.

| Port | Owner |
|------|-------|
| 1420 | `npm run dev` / `npm run app` (`tauri dev`) |
| 1421 | Playwright's e2e server, default only |

Check `lsof -nP -iTCP:1420 -iTCP:1421 -sTCP:LISTEN` before starting either, and
**do not kill a server you did not start** — it is another session's, or
Dylan's own running app.

**1420 is pinned.** `src-tauri/tauri.conf.json` hardcodes
`devUrl: http://localhost:1420` and cannot read an environment variable, so the
dev port cannot move without editing that file too. One `tauri dev` per
machine. If 1420 is taken, the app is already running — attach to it rather
than starting a second one.

**The e2e port moves.** `VAPOR_E2E_PORT` is read by both
`playwright.config.ts` and `vite.e2e.config.ts`; unset, it is 1421. Take a port
of your own when another session may be testing:

```
VAPOR_E2E_PORT=1431 npm run e2e
```

`reuseExistingServer` is now `false` everywhere, so a busy port fails the run
loudly instead of attaching to whatever frontend that server was built from. A
green suite against another session's code was the failure mode this replaced;
if the run refuses to start, move ports, do not free the port by killing what
is on it.

`vapor-app/src-tauri/target` is ~48 GB and shared. Two cargo builds in the same
tree serialize on its lock: "Blocking waiting for file lock on build directory"
is contention, not a hang — wait it out rather than killing the other build.

`npm run test` (vitest, jsdom) is safe to run concurrently.

## How worktrees actually work here

A worktree is a second checkout of this repo in its own directory, on its own
branch, sharing one `.git`. Two sessions in two worktrees cannot collide in
`git status`, cannot stage each other's files, and cannot deadlock on the cargo
build lock. They still share the machine's ports.

**Starting one.** Nothing to set up in advance and no separate terminal — the
session creates it. They land in `.claude/worktrees/<name>/` on a
`claude/<name>` branch. **Name the branch after the task, not the session**, so
`git branch --no-merged main` reads as a list of work in flight.

**Frontend-only work** should share the main tree's Rust build, which makes the
cold-build cost zero:

```
export CARGO_TARGET_DIR=/Users/dylangrowcoot/Documents/personal_apps/vapor-music/vapor-app/src-tauri/target
npm ci     # 152 MB, about a minute, unavoidable
```

A session doing real Rust work should let the worktree build its own `target/`
rather than serialising on the shared lock.

**Finishing one.** The work comes back as a branch merge or a cherry-pick onto
`main`, done by the session that did it. Then `git worktree remove <path>`.

**Measured 2026-08-21:** `node_modules` 152 MB; shared `target/` 48 GB; 627
crates in the graph; an existing frontend-only worktree 169–171 MB.

**Stale worktrees are invisible debt.** They sit outside every `git status`, so
work parked in one is lost until somebody runs `git worktree list`. Land it or
delete it; do not leave it.
