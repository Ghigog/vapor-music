# Vapor Music — Tech debt

**Last reviewed:** 2026-08-15 (TD-03 closed)

Known shortcuts, deferred work and things that are wrong but not yet worth
fixing. Kept separate from `docs/MIGRATION.md`, which tracks the *plan*; this
tracks what the plan is knowingly leaving behind.

Entries are only removed when the thing is actually fixed, not when it stops
being annoying.

---

## Blocking a release

Things that would be a defect if the app shipped today.

| ID | Item | Where |
|---|---|---|
| TD-01 | **Fonts are not vendored.** The design specifies Outfit, Inter and JetBrains Mono; `public/fonts.css` is an empty placeholder and the UI falls back to the system font, so nothing currently matches the design's typography. The CSP correctly blocks remote origins, so they must be shipped as woff2 in `public/fonts/`. | `vapor-app/public/fonts.css` |
| ~~TD-02~~ | ~~No persistence.~~ **Done** — playlists and settings persist via atomic write-and-rename in `store.rs`. The queue is still in memory, which is arguably correct (a stale queue on relaunch is worse than none) but was not a considered decision. | `vapor-app/src-tauri/src/store.rs` |
| ~~TD-03~~ | ~~No audio output in the app.~~ **Done** — `audio.rs` opens a cpal device on a thread of its own, drives `vapor-engine`'s mixer from it, and a supervisor advances the queue when a track ends. Play, pause, stop, seek, volume, next and previous all reach a speaker. The audio thread is proven to neither allocate nor free, including at a track change (`tests/audio_realtime.rs`). What it does *not* do is mix between tracks — see TD-25. | `vapor-app/src-tauri/src/audio.rs` |
| ~~TD-04~~ | ~~No library scan.~~ **Done** — `webdav.rs` walks the tree and rebuilds the index; the password lives in the OS keychain. Rows carry no analysis yet (see TD-06). | `vapor-app/src-tauri/src/webdav.rs` |
| ~~TD-06~~ | ~~Scanned rows carry no analysis.~~ **Done** — `analysis.rs` runs `vapor-dsp` over scanned tracks, caches results and applies them to rows. Blocked in practice by TD-07. | `vapor-app/src-tauri/src/analysis.rs` |
| ~~TD-07~~ | ~~No audio cache layer.~~ **Done** — `cache.rs` fetches on demand, writes atomically, and evicts LRU under a byte bound. Tested with synthetic WAVs, never anyone's library. | `vapor-app/src-tauri/src/cache.rs` |
| TD-08 | **The cache bound is not configurable and prefetch does not exist.** 8 GB is hardcoded, and tracks are only fetched when analysis or playback asks — nothing warms the queue's lookahead, so the next track downloads at the moment it is needed. Now audible rather than theoretical: TD-03 made the gap between tracks a real download plus a real decode, and `Queue::lookahead` already returns exactly the three hrefs a prefetcher would want. | `vapor-app/src-tauri/src/cache.rs` |
| TD-09 | **A track is decoded whole into memory.** `Deck` holds a `Vec<[f32; 2]>`, so playing a five-minute track costs ~106 MB at 44.1 kHz and ~115 MB at 48 kHz, briefly doubling at a track change while the displaced buffer waits to be freed. Fine on a desktop, wrong on a phone. Streaming needs a windowed source in `vapor-engine`, which the WSOLA stretcher's search window complicates — a real piece of work, not an oversight. | `vapor-core/crates/vapor-engine/src/deck.rs` |
| TD-05 | **Dolby Atmos does not decode.** 22 tracks in a real library are E-AC-3, which Symphonia cannot read. Shipping on macOS without a decision here is a silent regression against the Godot build, which handled them via ffmpeg. Recommendation stands: transcode on import. | MIG-003 |

## Correctness

| ID | Item | Notes |
|---|---|---|
| TD-10 | **Tempo detection lands a metrical relative ~10% of the time.** Accepted deliberately. `bpm_overrides` is now honoured by the table and the pathfinder, but **no UI sets it**, so the escape hatch is still unreachable. | MIG-002b |
| TD-11 | **Key detection is 56% exact / 81% harmonically compatible.** Good enough to port with, not good enough to be proud of. Segmented analysis is the remaining lever. | MIG-001 |
| TD-12 | **One malformed AAC file decodes to zero samples** where ffmpeg tolerates it. Unhandled — it will present as a track that silently fails. | MIG-004 |
| TD-13 | **`vapor-dsp` does not emit segment keys.** `TrackMeta` has `intro_key`/`outro_key` and the pathfinder prefers them, but nothing populates them, so every transition is judged on the whole-track key. | — |
| TD-14 | **Skip history is not persisted or wired.** `transition_cost` takes a skip penalty and `generate_mood_path` takes a history map; the shell always passes an empty one, so the app cannot learn from a skip. | — |

## Engine

| ID | Item | Notes |
|---|---|---|
| TD-20 | **Three transition types of six.** Echo Out, Reverb Freeze and Tempo Morph need delay and reverb, which are not implemented. | MIG-008 |
| TD-21 | **No PLL, no drift correction, no vocal-clash mid-cut, no phrase-adaptive durations.** All present in `audio_manager.gd`, none ported. | MIG-009 |
| TD-22 | **Time-stretch is a placeholder.** WSOLA works and is transparent at the ±2% beat-matching uses, but was written to prove the approach rather than chosen on measured quality against Rubber Band or signalsmith-stretch. | MIG-012 |
| TD-23 | **Standard Crossfade is not equal-power.** Replicated from Godot including its ~3 dB midpoint dip. Deliberate — fidelity over correctness during a refactor — and pinned by a test so changing it is visible. | MIG-015 |
| TD-24 | **cpal is unvalidated on iOS and Android.** The least battle-tested part of the audio stack, and phase 4 is where it would be discovered late. TD-03 exercises it on macOS desktop only; that says nothing about either mobile target. | MIG-011 |
| TD-25 | **The app plays tracks, it does not mix them.** `vapor-engine` beat-matches, and the shell drives one deck sequentially: a track ends, the next loads and starts. So the product's whole reason for existing — the transition — is unreachable from the app even though the engine and the analysis both support it. Blocked on TD-08: you cannot beat-match into a track that has not been downloaded and decoded ahead of time. | MIG-008, TD-08 |
| TD-26 | **No repeat or shuffle, and the queue always wraps.** `Queue::next` advances modulo the queue length, so the end of a queue silently restarts it and a one-track queue repeats forever. That is the core's pinned behaviour, inherited rather than chosen; there is no UI to change it and no "stop at the end" mode. | `vapor-core/crates/vapor-library/src/queue.rs` |

## Shell and UI

| ID | Item | Notes |
|---|---|---|
| TD-30 | **Two screens of twelve.** Library and Songs, plus the transport bar. Missing: Onboarding, Search, Now Playing, Queue, Vibe DJ, Liner Notes, Your Data, Settings, Loading, Empty. | — |
| TD-36 | **The transport is a bar, not the Now Playing screen.** It has the controls, a scrubber, a clock and volume, and none of the design's artwork, waveform or logo state readout. The mark exposes `state` and `energy` for exactly this and nothing drives them yet. | `Transport.tsx` |
| TD-31 | **No drag and drop.** The Godot build supported dragging tracks onto playlists and reordering within one. `dnd-kit` is the intended answer; nothing is wired. | — |
| TD-32 | **Songs column header layout is a compromise.** Artist shares a grid cell with the title, so its header is hidden rather than aligned. Works, but the header row and the data row agree by convention rather than by construction. | `songs.css` |
| TD-33 | **No keyboard navigation.** Rows are click-only; a table this size needs arrow keys, and the Godot version did not have this either. | — |
| TD-34 | **No error surface.** IPC failures render as a raw string. Fine for a scaffold, wrong for a person. | — |
| TD-35 | **The mark's shape is unfinished.** Known and expected; the attribute surface is stable so screens are safe to build against it. | — |

## Process

| ID | Item | Notes |
|---|---|---|
| ~~TD-40~~ | ~~The Tauri shell is not in CI.~~ **Done** — `.github/workflows/app.yml`, path-filtered to `vapor-app/**` and `vapor-core/**` so it does not tax unrelated commits. Now runs `cargo fmt --check` and `cargo test` as well as check and clippy; it previously did neither, so the shell's tests never gated anything and its formatting had drifted. | — |
| TD-41 | **The Godot CI job runs without the GDExtension**, so DSP-dependent tests are not covered. Deliberate — building Essentia from a HEAD-only tap on every run is worse — but the gap is real. | MIG-040 |
| TD-42 | **12 GUT tests fail** and are pinned as a known-failing baseline. Never diagnosed. | — |
| TD-43 | **The fixture set is not reproducible by anyone else.** Validation runs against a personal library via `extract-fixtures.mjs`; there is no synthetic corpus, so no one else can verify the analysis numbers. | — |

## Deliberately not debt

Recorded so they do not get "fixed" by someone reading this list:

- **`vapor-core` owns no I/O.** Persistence, HTTP and the filesystem live in the
  shell on purpose. That is what makes the core testable without an engine and
  reusable in the browser.
- **Ids are supplied by the caller**, not generated inside the core. Keeps the
  core deterministic.
- **Settings carry no password field.** The credential belongs in the OS
  keychain; a test asserts it cannot be serialised.
- **Unknown values render as "—", never as 0 or a guess.** The Godot stub
  fabricating 120 BPM is the failure this prevents.
