# Vapor Music — Tech debt

**Last reviewed:** 2026-08-15 (closed since the port: TD-01, TD-03, TD-08, TD-09, TD-10, TD-12, TD-13, TD-25, TD-26, TD-27, TD-30, TD-33, TD-34, TD-36, TD-37, TD-38, TD-42b, TD-43b)

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
| ~~TD-01~~ | ~~Fonts are not vendored.~~ **Done** — Outfit, Inter and JetBrains Mono ship as variable woff2 in `public/fonts/`, 236 KB for all three across `latin` and `latin-ext`. Variable rather than static: the design uses five weights, which as static files is five downloads per family. Verified in a browser — all six faces load, the ext subsets stay unloaded until a character needs them, and each family measures distinctly from the system fallback rather than silently falling back to it. Cyrillic, Greek and Vietnamese are deliberately absent (see `fonts.css`). | `vapor-app/public/fonts.css` |
| ~~TD-02~~ | ~~No persistence.~~ **Done** — playlists and settings persist via atomic write-and-rename in `store.rs`. The queue is still in memory, which is arguably correct (a stale queue on relaunch is worse than none) but was not a considered decision. | `vapor-app/src-tauri/src/store.rs` |
| ~~TD-03~~ | ~~No audio output in the app.~~ **Done** — `audio.rs` opens a cpal device on a thread of its own, drives `vapor-engine`'s mixer from it, and a supervisor advances the queue when a track ends. Play, pause, stop, seek, volume, next and previous all reach a speaker. The audio thread is proven to neither allocate nor free, including at a track change (`tests/audio_realtime.rs`). What it does *not* do is mix between tracks — see TD-25. | `vapor-app/src-tauri/src/audio.rs` |
| ~~TD-04~~ | ~~No library scan.~~ **Done** — `webdav.rs` walks the tree and rebuilds the index; the password lives in the OS keychain. Rows carry no analysis yet (see TD-06). | `vapor-app/src-tauri/src/webdav.rs` |
| ~~TD-06~~ | ~~Scanned rows carry no analysis.~~ **Done** — `analysis.rs` runs `vapor-dsp` over scanned tracks, caches results and applies them to rows. Blocked in practice by TD-07. | `vapor-app/src-tauri/src/analysis.rs` |
| ~~TD-07~~ | ~~No audio cache layer.~~ **Done** — `cache.rs` fetches on demand, writes atomically, and evicts LRU under a byte bound. Tested with synthetic WAVs, never anyone's library. | `vapor-app/src-tauri/src/cache.rs` |
| ~~TD-08~~ | ~~The cache bound is not configurable and prefetch does not exist.~~ **Done** — the bound lives in `Settings`, is set by `set_cache_max_bytes`, and trims immediately rather than at the next download; `Cache::new` now requires it, because three call sites had been quietly defaulting to 8 GB. A prefetch thread keeps `Queue::lookahead` downloaded ahead of the playhead, one track at a time with backoff on failure. Decoding is still done at the moment of use — see TD-25. | `vapor-app/src-tauri/src/cache.rs` |
| ~~TD-09~~ | ~~A track is decoded whole into memory.~~ **Done** — a deck now reads a five-second window that a decoder thread keeps filled, so it costs **1 MiB regardless of track length** instead of 55 MB for five minutes. The window is addressed by absolute position in the track and keeps 8192 frames of history, because the WSOLA search reads *backwards* and a queue cannot answer that. Streamed output is asserted bit-identical to the loaded path, both in `vapor-dsp` (against `decode_for_playback`) and through the stretcher (against the whole buffer, through a window that wraps repeatedly). Ported from `audio_dsp.cpp`, which already streamed — see MIGRATION for what was kept and what was not. | `source.rs`, `stream.rs`, `decoder.rs` |
| ~~TD-05~~ | ~~Dolby Atmos does not decode.~~ **Won't fix (2026-08-15).** 22 tracks (Björk, *Debut*) are E-AC-3 in `.m4a`. Atmos is a spatial listening format with no stable stereo image to beat-match or EQ against — Serato and Rekordbox do not support it either — and the catalogue is Apple's streaming encoding rather than files people own, which is what this app is for. Bundling ffmpeg to decode a format the app cannot DJ with is a large dependency for a bad trade. Those tracks show as unplayable with the reason (TD-12). Reopen if a real library turns out to carry Atmos files the owner wants mixed. | MIG-003 |

## Correctness

| ID | Item | Notes |
|---|---|---|
| ~~TD-10~~ | ~~Tempo detection lands a metrical relative ~10% of the time; no UI sets `bpm_overrides`.~~ **Partly done** — the detection rate is unchanged and still accepted deliberately, but the escape hatch is now reachable: double-click a BPM cell in Songs to correct it, clear the box to go back to the detected value, and a corrected tempo is marked in the table so it reads as a person's claim rather than the detector's. Implausible values are refused rather than clamped. | MIG-002b |
| TD-11 | **Key detection is 56% exact / 81% harmonically compatible.** Good enough to port with, not good enough to be proud of. Segmented analysis is the remaining lever. | MIG-001 |
| ~~TD-12~~ | ~~One malformed AAC file decodes to zero samples.~~ **Done** — permanent failures are recorded and persisted, so playback refuses immediately with the reason instead of re-downloading the file to fail the same way, and Liner Notes says why. Distinguished from "not downloaded yet", which is retryable and must not condemn a library analysed before its cache warmed. | MIG-004 |
| ~~TD-13~~ | ~~`vapor-dsp` does not emit segment keys.~~ **Done** — key estimation now runs over a 45 s span from `cue_in` and a 45 s span back from `cue_out`, so the pathfinder judges a mix on the keys at the *seam* rather than on the whole-track average. Measured from the audible content, not the file, so a silent lead-in cannot become the intro. Tracks under 135 s report no segments rather than three names for the same music. | — |
| ~~TD-14~~ | ~~Skip history is not persisted or wired.~~ **Done** — ported from AI-010 rather than reinvented: a skip is logged when next is pressed *during* a mix or within ten seconds of one ending, and adds 15.0 to that pair's cost, uncapped. An earlier attempt used a first-third rule and a penalty of 1.5, both invented; see the note below. | — |

## Engine

| ID | Item | Notes |
|---|---|---|
| ~~TD-20~~ | ~~Three transition types of six.~~ **Done** — `delay.rs` adds a feedback delay line and a Schroeder–Moorer reverb, both allocation-free once built, and Echo Out, Reverb Freeze and Tempo Morph are ported from `audio_manager.gd` with its own parameters (350 ms / −10 dB delay; 0.95 room, 0.1 damping; both decks bending to the mean tempo). The selection rule now picks from all six rather than substituting. | MIG-008 |
| ~~TD-21~~ | ~~No PLL, no drift correction, no vocal-clash mid-cut, no phrase-adaptive durations.~~ **Done** — all four ported, and grepping first changed three of them. The **grid half of the PLL is inert here** and measurement says so: both decks advance from the same audio clock, so a static grid has no phase error to find (51.65 ms uncorrected → 51.65 ms with it; the unscaled original is *worse* at 51.67 ms). The **waveform correlation** is the term that works — 28.29 ms, a 45% improvement. The **mid-cut** was already present but unconditional, on the wrong bands, and only on Echo Out; it is now gated on both tracks having vocals, mid-band only, on all six types. `vocal_presence` turned out to be `energy > 0.35` rather than a detector, so no detector was written. **Phrase durations** needed intro/outro boundary detection, ported from `audio_dsp.cpp` as `segments.rs`. | MIG-009 |
| TD-22 | **Time-stretch is a placeholder.** WSOLA works and is transparent at the ±2% beat-matching uses, but was written to prove the approach rather than chosen on measured quality against Rubber Band or signalsmith-stretch. | MIG-012 |
| ~~TD-23~~ | ~~Standard Crossfade is not equal-power.~~ **Done** — now a `cos`/`sin` pair whose squares sum to one at every instant, so the level holds across the whole transition. The original interpolated both gains linearly in dB, putting both decks at −30 dB at the midpoint: a hole in the middle of every mix. Carried across the port so the rest could be verified against it, then fixed — a known defect surviving a migration is an argument against having migrated. | MIG-015 |
| TD-24 | **cpal is unvalidated on iOS and Android.** The least battle-tested part of the audio stack, and phase 4 is where it would be discovered late. TD-03 exercises it on macOS desktop only; that says nothing about either mobile target. | MIG-011 |
| ~~TD-25~~ | ~~The app plays tracks, it does not mix them.~~ **Done** — the supervisor decodes the next track ~30 s before the outgoing one's `cue_out` and schedules a beat-matched mix. Beat grids never cross to the audio thread: the alignment is computed on the control side and only a ratio and a cue position are sent (`Mixer::schedule_prepared`). A mix falls back to plain sequential playback whenever it cannot be arranged — no analysis, or tempi more than ±6% apart — which on a queue in title order is the common case, not an error. **What is still missing: choosing the transition *type* per pair.** Every mix is a Standard Crossfade; the Godot build picked from context. | MIG-008, TD-27 |
| ~~TD-27~~ | ~~Every mix is a Standard Crossfade.~~ **Partly done** — ported the structure of `get_transition_type_between`: bucketed by harmonic distance, tempo distance and genre jump. Three of its six types need delay and reverb (TD-20), so their weight falls onto the nearest available: Echo Out and Reverb Freeze become Filter Sweep, Tempo Morph becomes Bass Swap. That collapses two of the original's tempo buckets — lost nuance, restored when TD-20 lands. | MIG-008, TD-20 |
| ~~TD-26~~ | ~~No repeat or shuffle, and the queue always wraps.~~ **Done** — `Repeat::{Off, All, One}`, defaulting to the wrapping behaviour it always had, plus shuffle with the original order restorable. The permutation is generated in the shell because the core owns no randomness on purpose. Thirteen tests, including the ones that matter: repeat-off leaves the playhead where it stopped, an explicit "play this next" outranks any mode, and unshuffling respects tracks removed while shuffled. | `queue.rs` |

## Shell and UI

| ID | Item | Notes |
|---|---|---|
| ~~TD-30~~ | ~~Two screens of twelve.~~ **Done** — all twelve exist: Onboarding, Library, Songs, Search, Now Playing, Queue, Vibe DJ, Liner Notes, Your Data, Settings, plus Loading and Empty as shared components rather than routes, since they are states every screen falls into. | — |
| ~~TD-36~~ | ~~The transport is a bar, not the Now Playing screen.~~ **Done** — Now Playing has the artwork, the real waveform from analysis, click-to-seek, and the mark driven as an actual readout: `blending` while a mix is armed, `energy` from the measured output level. Artwork is absent (no cover art anywhere yet) and the design's ♥ and playlist header are not built. | `NowPlaying.tsx` |
| ~~TD-37~~ | ~~Nine backend commands had no frontend binding.~~ **Done** — `tests/command_bindings.rs` reads the `generate_handler!` list and `core.ts` and fails if a command cannot be called. Verified to fail by removing a binding. It cannot check argument shapes; that would need real type extraction. | `core.ts` |
| ~~TD-38~~ | ~~Changing the username orphans the old keychain entry.~~ **Done** — `set_remote_config` deletes the previous entry when the name changes. Best effort: a keychain that will not release an entry is not a reason to refuse the rename. | `webdav.rs` |
| ~~TD-41b~~ | ~~Liner Notes has no written notes or credits.~~ **Partly done** — the file's own comment field is shown where it has one, and the screen now says honestly whether what it displays came from tags or from the path. The design's commissioned prose and per-role credits are not there, because no file carries them and nothing writes them. | TD-39 |
| ~~TD-42b~~ | ~~Energy is estimated from loudness.~~ **Done** — ported `audio_dsp.cpp`'s measure: the mean of a one-second RMS envelope over its peak. A dynamics ratio, not a loudness, so a track that sits near its own peak throughout reads as relentless and one with quiet verses reads as varied *at identical LUFS*. An earlier attempt averaged loudness, brightness and tempo, which was invented. | — |
| ~~TD-43b~~ | ~~Vibe DJ conducts from what is analysed, silently.~~ **Done** — the path reports how many tracks it passed over for want of analysis. | `Vibe.tsx` |
| ~~TD-39~~ | ~~No cover art anywhere.~~ **Done** — embedded artwork and tags are read by `lofty` during the analysis pass, when the file is already local and already open, so there is no second walk of the library. Art reaches Now Playing, the Queue and Liner Notes as a data URI. Tags fill *gaps* only: a path-derived artist wins over a tag, because a library filed as `Artist/Album/Track` is a statement and a disagreeing tag is usually the tag being wrong. | — |
| ~~TD-31~~ | ~~No drag and drop.~~ **Partly done** — the Queue reorders by dragging, using the platform's own HTML5 drag events rather than `dnd-kit`; explicit up/down buttons cover the keyboard, which a pointer-only implementation would have needed anyway. Dragging tracks *onto* playlists is still not wired. | — |
| ~~TD-32~~ | ~~Songs column header layout is a compromise.~~ **Done** — the header renders one cell per grid column with the controls inside, so title and artist sit together above the stack they sort. Artist was previously `display: none`, which meant the table could not be sorted by artist from its own header at all. | — |
| ~~TD-33~~ | ~~No keyboard navigation.~~ **Done** — the Songs table is a `listbox` with arrow keys, Page Up/Down, Home/End, Enter to play and Space to select. The cursor is deliberately separate from the selection: selection is what a bulk action applies to, and an arrow key must not silently change what a person is about to add to a playlist. | — |
| ~~TD-34~~ | ~~No error surface.~~ **Done** — one `ErrorNotice` across every screen, with `messageOf` unwrapping a thrown string, an `Error`, or an object with a message field, and the raw text kept behind a Details toggle for when it is the only clue. Search gained a real error path in the process: a failed search used to present as "nothing matched", which is a different answer and a wrong one. | — |
| TD-35 | **The mark's shape is unfinished.** Known and expected; the attribute surface is stable so screens are safe to build against it. | — |

## Process

| ID | Item | Notes |
|---|---|---|
| ~~TD-40~~ | ~~The Tauri shell is not in CI.~~ **Done** — `.github/workflows/app.yml`, path-filtered to `vapor-app/**` and `vapor-core/**` so it does not tax unrelated commits. Now runs `cargo fmt --check` and `cargo test` as well as check and clippy; it previously did neither, so the shell's tests never gated anything and its formatting had drifted. | — |
| TD-41 | **The Godot CI job runs without the GDExtension**, so DSP-dependent tests are not covered. Deliberate — building Essentia from a HEAD-only tap on every run is worse — but the gap is real. | MIG-040 |
| TD-42 | **12 GUT tests fail** and are pinned as a known-failing baseline. Never diagnosed. | — |
| TD-43 | **The fixture set is not reproducible by anyone else.** Validation runs against a personal library via `extract-fixtures.mjs`; there is no synthetic corpus, so no one else can verify the analysis numbers. | — |

## Check the Godot tree before writing a feature

The port often carried a *parameter* across without its behaviour, so an unused
argument looks like a feature request when it is a half-finished port. Skip
history (`dj_pathfinder.gd`), transition selection
(`audio_manager.gd::get_transition_type_between`) and energy (`audio_dsp.cpp`)
were each reimplemented from scratch before anyone checked. `grep` first.

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
