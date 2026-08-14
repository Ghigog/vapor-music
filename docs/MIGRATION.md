# Vapor Music — Migration off Godot

**Version:** 1.0
**Status:** Living Document — spike complete, decision pending
**Last reviewed:** 2026-08-14

> Why Vapor Music is moving off Godot, what replaces it, what the spike proved,
> and the order the work happens in.
> Read alongside `docs/ARCHITECTURE.md` (what exists today),
> `docs/CROSS_PLATFORM_DSP.md` (superseded in part — see *Corrections* below)
> and `docs/LICENSING.md`.

---

## The decision

Move the application off Godot to **Tauri 2 + TypeScript UI + a Rust core**,
targeting macOS, Windows, Linux, iOS, Android and the browser from one codebase.

This is not "Godot is bad." Godot is the wrong shape for *this* application, and
the evidence is in this repository:

| Symptom | Where |
|---|---|
| ~400 lines of hand-rolled list virtualization | `scripts/ui/track_table.gd` (`_build_slot_plan`, `_visible_slot_range`, pooling) |
| Hand-rolled window drag/resize, rewritten once to delegate to the OS | PERF-002, `scripts/main.gd`, `sidebar.gd` |
| Hand-rolled responsive breakpoints | `autoloads/PlatformManager.gd` |
| A shader for what `backdrop-filter` does in one CSS declaration | `assets/shaders/premium_glass.gdshader` |
| Two engine bugs filed upstream | `docs/upstream/` |
| A ticket to centre a checkbox icon | UI-009 |

None of these are audio problems. They are all *application shell* problems, and
they are the recurring cost.

Two further forcing factors:

1. **The redesign cannot happen where we are.** Claude Design emits web UI. It
   cannot contribute a single line to a `.tscn` file.
2. **The current cross-platform story is not real.** See below — it is worse
   than `docs/CROSS_PLATFORM_DSP.md` records.

### What Godot is genuinely doing for us

Worth stating plainly, because it is the actual cost of leaving.
`audio_manager.gd` feeds `AudioStreamGenerator.push_buffer()` every frame from
the C++ Rubber Band streaming stretcher, across two decks on separate
`AudioServer` buses, each carrying EQ6 → LowPass → HighPass → Delay → Reverb,
with tween-automated bus parameters and a beat-sync PLL. That is a real DJ mixer
graph, and replacing it is the whole risk of this migration. Hence the spike.

---

## Corrections to `docs/CROSS_PLATFORM_DSP.md`

That document's phase 1 plan is **wrong in a way that matters**, discovered while
scoping this migration.

It states the app "handles MP3, WAV and Ogg Vorbis only
(`audio_manager.gd:2520-2528`)" and concludes that vendoring `dr_mp3`, `dr_wav`
and `stb_vorbis` is sufficient.

The actual library is **324 `.m4a` and 239 `.mp3`** — 57.5% AAC. None of those
three single-header decoders can decode AAC.

Worse: `_load_standard_audio_stream()` is reached **only** when
`ClassDB.class_exists("AudioDSP")` is false — i.e. exactly on Windows and
Android. So today, on every non-macOS platform, **58% of the library fails to
load at all**, returning `false` with
`"Failed to load standard audio stream fallback"`. Not degraded playback —
no playback.

This is not a small correction. It means the Windows and Android builds have
never been able to play the majority of a real library, and the documented fix
would not have changed that.

Symphonia (pure Rust, MPL-2.0) decodes AAC, ALAC, MP3, FLAC, Vorbis and WAV with
no external dependencies, no shell-out and no platform code. Measured on the
real library: **540 of 563 files**, including 301 of the 324 `.m4a`. It does not
decode E-AC-3, which accounts for every remaining failure but one — see
*What the spike proved*.

---

## Target architecture

```
┌─────────────────────────────────────────────────────────────┐
│  UI — TypeScript, one codebase                              │
│  Claude Design output drops in directly                     │
└───────────────┬─────────────────────────────┬───────────────┘
                │ Tauri IPC                   │ direct call
┌───────────────▼─────────────┐   ┌───────────▼───────────────┐
│  Native shell (Tauri 2)     │   │  Browser                  │
│  macOS Win Linux iOS Android│   │                           │
└───────────────┬─────────────┘   └───────────┬───────────────┘
                │                             │
┌───────────────▼─────────────────────────────▼───────────────┐
│  vapor-core — Rust, compiled native AND to wasm             │
│                                                             │
│   vapor-dsp     decode (symphonia) · tempo · key · cues     │
│   vapor-engine  two decks · EQ/filter chain · transitions   │
│   vapor-library playlists · groups · pathfinder · metadata  │
│                                                             │
│  Audio out:  cpal (native)  ·  AudioWorklet (web)           │
│  Cache:      filesystem (native)  ·  OPFS (web)             │
└─────────────────────────────────────────────────────────────┘
```

**One DSP implementation, everywhere.** This is the point of the whole design.
Today the vibe features exist on macOS only, because they are a macOS dylib. A
Rust core that compiles to both native and wasm makes "works on every platform"
the default rather than a per-platform port.

### Why the browser is now a first-class target

The app is cloud-first by design — the library lives in the user's own cloud and
local storage is only a cache. That removes the objection that would otherwise
sink a web build: it never needed durable access to a local music folder.

**OPFS** (Origin Private File System) is the web cache tier. It is supported in
Safari and Firefox as well as Chromium — unlike the File System Access API — and
its synchronous access handles work inside a Worker, which is what a decode and
analysis thread needs.

---

## What the spike proved

The spike deliberately attacked the **highest-risk** component first, not the
easiest. Everything in the DSP stack has an off-the-shelf pure-Rust answer
except beat tracking, so beat tracking is what got built and measured.

`vapor-core/crates/vapor-dsp` implements decode, tempo estimation and key
detection in portable Rust with no platform code, no engine and no audio I/O —
the whole crate is `cargo test`-able on any CI runner.

It is validated against **563 real tracks** from the actual library, using the
Essentia results the Godot app already produced as ground truth
(`vapor-core/tools/extract-fixtures.mjs`).

### Results

Run: 563 tracks, 375.8 s total, **0.67 s/track** (whole library in ~6 minutes,
single-threaded, on an M1).

**Decode — 540/563 (95.9%)**

| Container | Decoded |
|---|---|
| `.mp3` | 239/239 (100%) |
| `.m4a` | 301/324 (92.9%) |

All 23 failures have a known cause, confirmed with `ffprobe`:

* **22 files — Dolby Atmos.** One album (Björk, *Debut*) is **E-AC-3**
  ("Dolby Digital Plus + Dolby Atmos"), 6 channels, in an `.m4a` container.
  Symphonia has no E-AC-3 decoder, so it reports "no decodable audio track".
* **1 file — malformed AAC.** AAC-LC 44.1 kHz stereo that `ffprobe` also flags
  (`channel element 0.0 duplicate`); ffmpeg tolerates it, Symphonia decodes zero
  samples.

**Tempo — vs Essentia, ±2%**

| | Count | Share |
|---|---|---|
| Exact agreement | 438/540 | **81.1%** |
| Octave error (½ / 2×) | 24/540 | 4.4% |
| Usable (either) | 462/540 | 85.6% |

**Key — vs Essentia**

| | Count | Share |
|---|---|---|
| Exact Camelot | 236/540 | **43.7%** |
| Adjacent on the wheel | 167/540 | 30.9% |
| Harmonically compatible (either) | 403/540 | 74.6% |

### How to read these numbers

**Decode beats the status quo off-macOS by an enormous margin, and does not yet
match it on macOS.** This distinction matters and must not be glossed:

* On Windows and Android today, **0 of 324** `.m4a` files play. Symphonia gets
  301. That is not an improvement, it is the difference between working and not.
* On macOS today, Essentia-via-ffmpeg handles all of them, including the Atmos
  album (that was the point of BUG-001). Shipping the Rust core on macOS without
  an E-AC-3 answer would be a **regression on 22 tracks**.

So Atmos needs a decision before phase 4 — see *Open decisions*.

**Tempo is the encouraging result.** 81.1% exact on first implementation, with
only 4.4% octave errors, is a better starting point than expected for the
component identified as the migration's highest risk. It is also untuned: no
beat-grid refinement, no segment-aware analysis, single global tempo estimate.

**Key is the weak result, and partly by construction.** Agreement with Essentia
is a ceiling, not a grade — Essentia's own published key accuracy against human
annotation is roughly 70–80%, so a disagreement may be either implementation
being wrong. Still, 43.7% exact is too low to ship. Two concrete causes, both
addressable:

1. This computes **one chroma for the whole track**. Essentia segments, and the
   app already stores and uses `segment_keys`, `intro_key` and `outro_key` —
   which this does not produce at all yet.
2. No harmonic weighting. A single fundamental spreads energy across its
   overtones, several of which fall outside the tonic's pitch class and bias the
   correlation.

The 74.6% harmonically-compatible figure is the more meaningful number for this
application, because `dj_pathfinder` navigates the Camelot *wheel* — an adjacent
key is a usable transition, not a failure. But it is not a substitute for
fixing (1) and (2).

### What this does *not* prove

Stated explicitly so it is not over-read:

- No audio output yet. `cpal` and `AudioWorklet` are unexercised.
- No time-stretching. Rubber Band's replacement is not chosen or benchmarked.
- No two-deck mixing, no EQ chain, no transitions, no PLL.
- No wasm build yet — the crate has no platform code, so this is expected to be
  straightforward, but "expected" is not "verified."

The spike answered "can we own the analysis?" It did not answer "can we own the
mixer?" That is the next spike.

---

## Phased plan

Each phase ends in something verifiable. Godot keeps running and shipping until
phase 4.

### Phase 0 — CI first *(before any migration work)*

There is currently **no CI**. Better test coverage is a stated goal of this
migration, and CI is what delivers that, not the rewrite.

- GitHub Actions matrix: macOS, Windows, Linux.
- Run the existing 224 GUT tests headless on macOS.
- Run `cargo test` and `cargo clippy` for `vapor-core` on all three.
- Record the 12 currently-failing GUT tests as a known-failing baseline so the
  suite can gate on *regressions* immediately rather than waiting for a green
  board.

No Kubernetes. Docker only where a job needs a pinned Linux toolchain. macOS,
iOS, Windows and Android cannot run in a Linux container; a runner matrix is
what covers them. The migration's own payoff here is large: most logic moves
from "needs a Godot binary" to "`cargo test` on any runner."

### Phase 1 — Finish `vapor-dsp`

- Improve tempo: octave-error resolution, then a beat grid (not just a BPM), so
  `beat_grid` and `downbeats` can be compared against the fixtures too.
- Segmented key analysis to match `segment_keys` / `intro_key` / `outro_key`.
- Port the already-portable parts of `audio_dsp.cpp`: cue in/out, LUFS,
  dynamic range, transients, waveform peaks.
- Gate: agreement targets on the 563-track set, enforced in CI.

### Phase 2 — `vapor-engine`, the mixer *(the real risk)*

- Two decks, sample-accurate, `cpal` output.
- Biquad EQ/filter chain replacing the Godot `AudioServer` bus effects.
- Time-stretch: benchmark `signalsmith-stretch` (MIT) against Rubber Band.
- One beat-matched crossfade, validated against the assertions already written
  in `tests/unit/test_dj_transitions.gd` and `test_beat_sync_pll.gd`.
- Gate: a transition that beat-matches, verified offline by rendering to a file
  and checking beat alignment — not by ear alone.

### Phase 3 — Port the pure logic

Mechanical, test-guarded, and much of it net deletion:

| From | To | Note |
|---|---|---|
| `dj_pathfinder.gd` (682) | `vapor-library` | Pure Camelot graph search |
| `webdav_service.gd` (653) | `reqwest_dav` | Hand-rolled chunked decoding disappears |
| `metadata_service.gd` (1276) | `lofty` + port | `_parse_id3v2_tags` disappears |
| playlist / folder / group / index / settings (~900) | `vapor-library` | Straight port |

The 5,155 lines of GUT tests do not port as a harness, but their **assertions
are the specification** and should be translated first, before the code they
cover.

### Phase 4 — The new shell

- Tauri 2 app, TypeScript UI, Claude Design redesign.
- Web build via wasm + AudioWorklet + OPFS.
- Feature parity checklist against the current app, then cut over.

### Phase 5 — Retire

Archive the Godot tree on a branch. Do not delete it until the new app has run
as the daily driver for a meaningful period.

---

## Risks

**Beat tracking quality.** The largest one, which is why it was spiked first. If
tempo cannot be pushed to an acceptable agreement rate, the fallback is to keep
Essentia via FFI on native and accept that the web build has weaker analysis —
which reintroduces the AGPL obligation and the two-implementations problem, but
does not block the migration.

**Real-time-safe Rust.** No allocation, no locks, no I/O in the audio callback.
This is a genuine discipline gap and the most likely source of subtle,
hard-to-reproduce audio glitches. Mitigate by keeping the audio thread tiny and
pushing everything else to a control plane.

**Mobile audio maturity.** `cpal` on Android and iOS is less battle-tested than
on desktop. Budget real time here, and validate early on device rather than
discovering it in phase 4.

**Scope.** The UI rewrite is ~10k lines. It is cost already accepted for the
redesign, but it is still the largest single block of work and it is the one
most likely to expand.

---

## Licensing consequence

Dropping Essentia and Rubber Band removes both copyleft dependencies. Symphonia
is MPL-2.0 and `signalsmith-stretch` is MIT, so the AGPL-3.0 obligation adopted
in `docs/LICENSING.md` would no longer be forced by dependencies.

**This is a consequence, not a goal.** Do not couple "get off Godot" to "get off
AGPL" — coupling them is how a focused migration turns into an open-ended one.
Keep the AGPL for now; revisit once phases 1 and 2 are done and the dependency
set is actually known.

---

## Open decisions

1. **Dolby Atmos / E-AC-3.** 22 tracks in the current library do not decode
   without ffmpeg. Options, roughly in order of preference:
   - Transcode-on-import to AAC or FLAC once, at add time, and analyse the
     result. Costs nothing at playback and needs no decoder in the core.
   - Keep an optional ffmpeg path on desktop only, accepting that Android and
     web lose these tracks.
   - Accept the gap and surface it honestly in the UI, as the DSP stub now does
     for unanalyzed tracks.

   Not a blocker for phases 1–3, but it must be settled before the macOS cutover
   or it ships as a silent regression.
2. **Web tier scope.** Full parity, or cloud-only with analysis deferred to a
   desktop instance? Affects how state is split in phase 4.
3. **Time-stretch library.** `signalsmith-stretch` (MIT) vs Rubber Band's
   single-file build (GPL). Decide in phase 2 on measured quality.
4. **UI framework** inside Tauri. Not yet chosen; defer until phase 4 so the
   redesign informs it.
