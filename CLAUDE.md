# Vapor Music

Godot original at the repo root (`project.godot`, `src/` GDExtension, `scenes/`).
The Tauri + React rewrite is `vapor-app/`; the Rust analysis crates are
`vapor-core/`. Architecture is in `docs/ARCHITECTURE.md`, testing in
`docs/TESTING.md`.

## Several Claude sessions run in this repo at once

Dylan often has two or three sessions working here simultaneously, each briefed
on its own scope. Assume you are not alone.

**Stay in the lane your task describes.** Before editing a file the request did
not name, run `git status` — modified or untracked files you did not create are
another session's work in flight. If your fix needs one of them, say so and let
Dylan route it rather than editing it yourself. Two sessions changing the same
module produces conflicts he has to untangle, and they can fight at runtime
rather than in the diff.

**Stage explicit paths. Never `git add -A`, `git add .`, or `git commit -a`.**
The working tree is shared, so a blanket stage sweeps up whatever another
session had half-finished and commits it under your message.

## Ask about a worktree before you start

A worktree gives the session its own directory and branch, so nothing above
applies inside it. Dylan runs plain sessions by default and does not always
think to ask for one.

**Say so, and wait for his answer, when the task looks like any of these:**

- it will touch more than a handful of files, or run for more than a few turns
- it changes a module another session is likely in — `src/lib/`,
  `src/components/`, `src-tauri/src/lib.rs`, generated types
- it is exploratory: a refactor, a spike, or something that may be thrown away
- `git status` already shows heavy in-flight work from someone else

Phrase it as a question ("this touches the theme system and another session has
files open there — want me in a worktree?"). Only call `EnterWorktree` once he
says yes. Small, contained edits and single-file fixes do not need one; asking
every time is its own kind of noise.

Trade-off worth stating when he asks: a worktree needs its own `node_modules`
(~150 MB) and its own `target/` unless `CARGO_TARGET_DIR` points back at the
main one — a cold Rust build there is slow.

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

`vapor-app/src-tauri/target` is ~45 GB and shared. Two cargo builds in the same
tree serialize on its lock: "Blocking waiting for file lock on build directory"
is contention, not a hang — wait it out rather than killing the other build.

`npm run test` (vitest, jsdom) is safe to run concurrently.

## How worktrees actually work here

For Dylan, so the answer is written down rather than re-explained each time.

A worktree is a second checkout of this repo in its own directory, on its own
branch, sharing one `.git`. Two sessions in two worktrees cannot collide in
`git status`, cannot stage each other's files, and cannot deadlock on the cargo
build lock. They still share the machine's ports.

**Starting one.** Nothing to set up in advance and no separate terminal: ask
the session to work in a worktree, or say yes when it offers, and it creates
one itself. They land in `.claude/worktrees/<name>/` on a `claude/<name>`
branch. Two already exist from earlier sessions:

```
git worktree list
```

**Working in one.** The session `cd`s there on its own; paths it reports are
inside the worktree. Files edited there are invisible to the main checkout
until the branch merges, which is the whole point.

**Finishing one.** The work comes back as a branch merge or a cherry-pick onto
`main` — the session does this when the work is done. Afterwards the directory
is removed with `git worktree remove <path>`; a worktree with nothing in it is
cleaned up automatically.

**What it costs.** Each worktree needs its own `node_modules` (~150 MB —
`npm ci` in the new tree before running anything frontend). Rust is the
expensive half: a fresh `target/` is a cold build of the whole dependency
graph. Point `CARGO_TARGET_DIR` at the main tree's `target/` to skip that, at
the price of re-serializing on the build lock with whoever else is compiling.
Frontend-only sessions should share it; a session doing real Rust work should
not.

**When it is not worth it.** A single-file fix, a doc edit, a question about
the code. The setup cost is real and a contained edit will not collide with
anything.
