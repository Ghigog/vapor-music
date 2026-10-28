# Vapor Music

Desktop/Android music player. `vapor-app/` (Tauri 2 shell + React 19) sits on
`vapor-core/` (Rust crates). The Godot original is gone; the tag
`godot-final-v1.78` holds it (`git show godot-final-v1.78:<path>`).

## Where things are

| Path | What | Start at |
|------|------|----------|
| `vapor-app/src/screens/` | React screens | `Library.tsx` |
| `vapor-app/src/components/` | Shared widgets (+ co-located `*.test.tsx`) | `Transport.tsx` |
| `vapor-app/src/lib/core.ts` | Typed wrappers over Tauri IPC | `core.ts` |
| `vapor-app/src/lib/generated/` | `ts-rs` types from Rust; never hand-edit | `npm run types:check` |
| `vapor-app/src/test/ipc.ts` | Fake IPC for vitest (fixtures only, no logic) | `ipc.ts` |
| `vapor-app/e2e/` | Playwright specs | `journeys.spec.ts` |
| `vapor-app/src-tauri/src/lib.rs` | Backend root: `AppState`, `Shared` | `AppState` |
| `vapor-app/src-tauri/src/commands/` | All `#[tauri::command]`s by domain | `playback.rs::play_tracks` |
| `vapor-app/src-tauri/src/` (other) | audio, decoder, sync, webdav, store… | `audio.rs` |
| `vapor-core/crates/vapor-dsp` | Tempo, key, loudness, beats | `src/tempo.rs::estimate` |
| `vapor-core/crates/vapor-engine` | Decks, mixer, transitions | `src/mixer.rs` |
| `vapor-core/crates/vapor-library` | Index, groups, Camelot keys | `src/camelot.rs` |
| `vapor-core/crates/vapor-dj` | DJ/set building | `src/lib.rs` |
| `docs/` | Testing, decisions, release, licensing, Android | `TESTING.md` |
| `.claude/hooks/` | Session survey and git-staging guard | `session-survey.sh` |

## Commands (run in `vapor-app/`)

- `npm run typecheck`, `npm run test` (vitest; fast, the default check)
- `npm run types:check` — fails if generated TS types drifted from Rust
- `npm run e2e` — Playwright; slow, ask first (`VAPOR_E2E_PORT` moves the port)
- `npm run app` — `tauri dev`; port 1420 is pinned, one instance per machine
- Rust: `cargo check` / `cargo test` in `vapor-core/` or `vapor-app/src-tauri/`

## Rules

- Several sessions share this repo: use a worktree
  (`.claude/worktrees/<task>`) for anything beyond a one-file change.
- Stage explicit paths. Never `git add -A`/`.`, `commit -a`, `stash`,
  `reset --hard`, or whole-tree `checkout`/`restore` (a hook blocks them).
- Don't kill servers on 1420/1421 you didn't start.
- `lib.rs`: one session in the backend at a time.
- `core.ts`, `app.css`, `tokens.css`: additive changes only.
- Answer the question asked; report unrelated findings instead of fixing them.
