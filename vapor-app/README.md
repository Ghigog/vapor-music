# vapor-app

The Tauri 2 shell — the app that replaces the Godot build.

```
vapor-app/
├─ src/               React + TypeScript UI
│  ├─ lib/core.ts     Typed client for the Rust core. Every call goes through here.
│  ├─ components/     Shared UI, incl. the <vapor-mark> wrapper
│  └─ screens/        One per screen in the design
├─ public/            tokens.css, vapor-mark.js, icons — copied from design/
└─ src-tauri/         Rust shell: a thin adapter over vapor-core
```

## Running

```bash
npm install
npm run app          # tauri dev — builds the Rust side, opens the window
```

The first Rust build compiles the whole Tauri dependency tree and takes a
while. Later builds are incremental.

```bash
npm run typecheck    # tsc, no emit
npm run app:build    # production bundle
```

## The rule that keeps this thin

**No decisions in `src-tauri`.** If a command does more than translate between
JSON and a core type, the logic belongs in `vapor-core` where it can be tested
without a window.

Two reasons, and the second is the one that bites later:

1. The Godot version put logic in services that could only run inside the
   engine, which is why its test suite needs a Godot binary and why so little
   of it could be verified.
2. The browser build calls the same core crates compiled to wasm, with no
   Tauri IPC at all. Anything implemented in the shell would have to be written
   a second time.

## Design

`public/tokens.css` and `public/vapor-mark.js` are copied from `design/`. They
are **not** the source — edit `design/`, then re-copy. See `design/README.md`
for the system, and note that `--sov` (the green) is reserved for "this is on
your device and it is yours" and nothing else.

Fonts are not yet vendored: `public/fonts.css` is an empty placeholder and the
UI falls back to the system font. Drop woff2 files in `public/fonts/` and
declare them there — do not link Google Fonts, since the CSP blocks remote
origins and the app is meant to work offline.

## Status

Scaffold only. The shell boots, renders the generative mark, and makes one live
call into the core to prove the seam works. No screens are built yet.
