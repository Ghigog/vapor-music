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
mixer?" That is the next spike — below.

---

## What the mixer spike proved

`vapor-core/crates/vapor-engine` rebuilds the two-deck DJ mixer: decks, a biquad
EQ/filter chain replacing the Godot bus effects, WSOLA time-stretching replacing
Rubber Band, and transition envelopes replacing the tween automation. Like
`vapor-dsp` it has no platform code — audio output lives in a separate binary, so
the engine stays `cargo test`-able and **builds for wasm**.

### Result: beat-matching works

Rendering a transition between two click tracks at 128 and 124 BPM and measuring
where the onsets actually landed in the output:

> **Worst beat deviation across the transition: 6.11 ms.**

That is inside the WSOLA search window (±256 frames ≈ 5.8 ms) and well inside
what reads as tight for a DJ mix. The measurement is end-to-end through decode,
stretch, EQ, envelope and mix — not a check of the arithmetic.

### Structural improvement over the Godot design

Alignment is solved **up front** rather than corrected afterwards. Both decks
advance from the same audio clock — the count of rendered frames — so once
phase-locked they cannot drift. The Godot build polls `get_playback_position()`
from `_process` at frame rate, which is why it needs a PLL to chase drift at all.
In this design the PLL becomes a correction for real-world tempo wander, not the
mechanism that achieves sync.

### Three bugs the spike caught, and how

Worth recording because they show which tests were load-bearing:

1. **`tempo_ratio` was inverted.** To make a 124 BPM track sound like 128 you
   play it *faster*; the code had `incoming / outgoing`. The unit test asserted
   `ratio < 1.0` — it encoded the same misunderstanding and passed happily. Only
   rendering audio and measuring onsets caught it. Beats were landing a half
   period off, i.e. squarely on the offbeat.
2. **The WSOLA cross-fade used a full Hann window** (0 → 1 → 0) instead of a
   rising ramp, so the last sample of every grain reverted to the stored tail —
   a discontinuity every 23 ms.
3. **The stretcher produced nothing at all**, because the first grain required a
   search margin that a read position of zero could never satisfy.

The lesson for phase 2: **unit tests on scheduling arithmetic are not sufficient
for audio.** Render and measure.

### Findings on real music

Rendering a real transition from the library surfaced two things the click-track
tests could not:

* **Tempo failures include metrical errors, not just octave errors.** The
  detector called a track 83.4 BPM that Essentia calls 110.9 — a 3:4 relation,
  which the validator's octave check (½, 2×, ⅓, 3×) does not count. The earlier
  "4.4% octave error" figure therefore *understates* the metrical-error class.
  Feeds MIG-002.
* **Bass Swap clips; Standard Crossfade does not.** Measured over the transition
  window: crossfade peaks at 0.869 with zero clipped samples; Bass Swap peaks at
  1.000 with 152 clipped samples (0.022%). The cause is structural — Bass Swap
  holds the outgoing deck at 0 dB while the incoming reaches full level. This is
  exactly what the Godot build's three-band RMS clipping prevention
  (`calculate_chunk_rms`, `_clipping_attenuation_db`) exists to solve, and it is
  not ported. New item MIG-006.

### What this spike does *not* prove

* ~~Not real-time safe yet.~~ **Resolved (MIG-010).** `play` renders inside the
  audio callback, and `Mixer::render` is proven allocation-free by a counting
  global allocator rather than by inspection.
* **Three transition types, not six.** Echo Out, Reverb Freeze and Tempo Morph
  need delay and reverb, which are not implemented.
* **No PLL, no drift correction, no vocal-clash mid-cut, no phrase-adaptive
  durations.**
* **Post-transition tempo snaps back instantly.** The Godot build glides it
  (`_pitch_ramp_tween`). New item MIG-007.
* **Beat grids on real music are synthesised from BPM**, because `vapor-dsp`
  does not emit real beat positions yet (MIG-002). Alignment on real tracks is
  therefore only as good as that assumption — which is precisely why the
  automated tests use click tracks with known beat positions.

---

## Phase 1 progress

### MIG-002 — beat grids (done) and MIG-014 — metrical errors (done)

`vapor-dsp` now emits real beat positions via dynamic-programming beat tracking
(Ellis 2007), not just a BPM. `vapor-engine` consumes them; the synthesised
"beat every 60/bpm from zero" grid is now only a fallback for when tracking
fails.

Measured over the same 563-track set:

| Metric | Result |
|---|---|
| Beat F-measure, mean | **0.763** |
| Beat F-measure, median | **0.884** |
| Tracks at F ≥ 0.8 | 324/540 (60.0%) |
| Tempo exact | 437/540 (80.9%) |
| Tempo metrical error | 57/540 (10.6%) |
| Tempo periodic (either) | 494/540 (91.5%) |
| Time per track | 0.51 s |

**Two corrections to figures reported earlier in this document.**

1. **The metrical-error class was undercounted.** The original check tested only
   octave relations (½, 2, ⅓, 3) and reported 4.4%. Adding 2:3, 3:2, 3:4 and
   4:3 raises it to **10.6%** — roughly 2.4× larger. Tempo *agreement* is
   unchanged at ~81%; what changed is the honest accounting of the near-misses.
   The encouraging read is that **91.5% of tracks land on a genuinely periodic
   grid**: pulse detection is nearly always right, and it is the metrical level
   that is wrong.
2. **A 22% metrical-error figure quoted mid-work was subset-specific** — it came
   from a 60-track sample that happened to be entirely `.m4a`. The full-library
   number is 10.6%.

### A defect this measurement caught

The first beat-tracking run scored F=0.470, and the cause was not the tracker.
Analysis ran over a 120 s window; Essentia's grids span the whole track, so
recall was capped at roughly `120 / duration` — about 0.41 on a 289 s track —
before tracking quality mattered at all.

The product consequence was worse than the metric: **a windowed grid has no
beats near the outro, which is exactly where transitions are scheduled.**

Fixed by giving each stage the span it actually needs:

| Stage | Span | Why |
|---|---|---|
| Beat tracking | whole track | Coverage at the outro is the point |
| Tempo | 120 s interior window | Fade-ins and run-outs cost accuracy (76.7% → 73.3% without it) |
| Key | 120 s interior window | Global property; its 8192-point FFT is the expensive one |

Only one FFT runs per stage either way — the tempo window is applied to the
onset function, not to the audio.

| | Windowed (before) | Whole-track | Split (current) |
|---|---|---|---|
| Beat F, mean | 0.470 | 0.757 | **0.769** |
| Beat F, median | 0.485 | 0.861 | **0.861** |
| F ≥ 0.8 | 0/60 | 37/60 | **38/60** |
| Tempo exact | 76.7% | 73.3% | **76.7%** |

*(that comparison is the 60-track subset used while iterating; the table above
is the full 563-track run)*

### MIG-002b — metrical level: attempted and reverted

The metrical-level error is the highest-value remaining target: a half- or
double-time grid caps F-measure at about 0.67 however well phased it is, so
fixing it lifts tempo accuracy **and** beat F-measure together.

A first attempt was made and **reverted**. Recorded here so the next one does
not repeat it.

**The approach.** Replace the perceptual prior's blunt pull toward 120 BPM with
two measurements that say which *direction* is wrong:

* **Alternation** — at double time every other "beat" lands on an offbeat, so
  even and odd beat strengths diverge.
* **Midpoint energy** — at half time the skipped real beats sit exactly at the
  midpoints, so the gaps are as strong as the beats.

Candidates at ×1, ×½, ×2, ×⅔, ×³⁄₂, ×¾ and ×⁴⁄₃ were scored on those two
signals.

**The result, A/B on 108 identical tracks:**

| | Refinement off | Refinement on | After drift fix |
|---|---|---|---|
| Tempo exact | **78.7%** | 40.7% | 54.6% |
| Metrical error | **20.4%** | 53.7% | 43.5% |
| Beat F, mean | **0.754** | 0.586 | 0.653 |
| F ≥ 0.8 | **56.5%** | 26.9% | 37.0% |

**What was learned, and it is worth keeping:**

1. **Integer period stepping accumulates fatal drift.** The scorer rounded the
   beat period to whole frames and added it repeatedly. A period of 57.42
   stepped as 57 loses 0.42 frames per beat — about 190 frames over a
   five-minute onset function, far more than a whole beat — so the grid samples
   effectively random positions and the score becomes noise. Fixing this
   recovered 40.7% → 54.6%, which is why the mechanism looked plausible. **Any
   future work here must step in fractional frames and round at use.** The same
   mistake first appeared in a *test fixture*, where it was harmless; in
   production over long signals it was not.
2. **Even with the drift fixed, the scorer is systematically biased.** The guard
   test "a correct tempo must be left alone" fails: it moves tempos that were
   already right. The likely cause is that `on_mean` is not comparable across
   candidates — a slower candidate samples fewer, more selective points and
   scores higher for that reason alone, which biases toward halving.
3. **Do not tune the weights against the fixture set.** Two constants against
   563 tracks is a straight path to overfitting a number that will not
   generalise. The next attempt needs a formulation that is *scale-free by
   construction* — judging only metrical level, with the existing comb score
   left to judge which period has energy — rather than better-chosen weights.

Baseline confirmed restored at 78.7% / 0.754 after the revert.

### MIG-006, MIG-007, MIG-001, MIG-005 — completed

**MIG-006, clipping.** The three-band RMS guard was ported faithfully and did
*not* fix the problem, which is worth recording because the original diagnosis
in this document was wrong. Measured on the real Bass Swap transition:

```
  RMS  0.257    guard threshold 0.630   -> guard correctly does nothing
  peak 1.000    crest factor 3.9x       -> the clipping is peak-domain
```

Modern masters run a crest factor near 4x, so two decks sum past full scale on
transients while their combined RMS stays well under any sane threshold. No RMS
guard catches that, and neither would the Godot original have. A master peak
limiter (block look-ahead, 0.99 ceiling, instant attack, 250 ms release) is what
actually fixes it. Both are kept: the RMS guard still prevents sustained
low-end buildup, which is a different failure.

| Transition | Before | After |
|---|---|---|
| Standard Crossfade | peak 0.869, 0 clipped | peak 0.869, 0 clipped |
| Bass Swap | peak 1.000, **152 clipped** | peak 0.990, **0 clipped** |
| Filter Sweep | peak 1.000, **1006 clipped** | peak 0.990, **0 clipped** |

RMS is essentially unchanged (0.257 → 0.255), so the limiter is catching
transients rather than squashing the mix.

**MIG-007, tempo glide.** The incoming deck now eases back to its own tempo over
6 s with sine easing instead of snapping. Tested for monotonicity — a ratio that
wandered on the way back would be heard as wow and flutter.

**MIG-001, key detection.** Two changes, each addressing an identified cause:

| | Exact | Compatible |
|---|---|---|
| Baseline | 34.3% | 67.6% |
| + harmonic-weighted chroma | 39.8% | 71.3% |
| + per-frame normalisation | **48.1%** | **76.9%** |

A harmonic pitch class profile fixes a real bias: a note at *f* also radiates at
2*f*, 3*f* and 4*f*, whose classes are the octave, fifth and major third, so
assigning each bin to its own class credits a plain C with energy at G and E and
tilts every key toward major. Per-frame normalisation stops the loudest section
deciding the key.

Still short of shippable, and segmented analysis remains outstanding.

**MIG-005, cue points and loudness.** `_analyze_samples_impl` was already pure
portable code, so this is a translation rather than a reimplementation:

| Metric | Mean abs error | Within tolerance |
|---|---|---|
| `cue_in` | 0.043 s | 99.1% within 0.5 s |
| `cue_out` | 0.088 s | 96.3% within 0.5 s |
| `lufs` | **0.003 LU** | **100%** within 1 LU |

Agreement to 0.003 LU indicates the K-weighting and gating are faithful, not
merely plausible. The LUFS test anchors on the standard rather than on Essentia
— a −20 dBFS 1 kHz tone must read about −20 LUFS — so the port is checked
against the specification too. These are load-bearing: `get_transition_trigger_time`
schedules mixes from the cue points.

### MIG-002b, second attempt — and why this line of attack is closed

A second attempt applied every lesson above: fractional stepping, only
dimensionless ratios (evenness of even/odd beats, midpoint energy over beat
energy), octave candidates only, and an override margin so an ambiguous case
keeps the autocorrelation answer.

It scored **66.7% exact / 0.727 mean F** against the 78.7% / 0.754 baseline —
better than the first attempt, still worse than doing nothing.

Rather than tune the margin, a diagnostic measured the prior question: **does
the fit signal separate correct estimates from metrical errors at all?** For
each track it classified the unrefined estimate against Essentia and recorded
the margin `fit(alternative) − fit(estimate)`.

| Population | n | mean margin | median |
|---|---|---|---|
| Estimate already correct — margin must be **negative** | 135 | **+0.049** | +0.059 |
| Estimate needs doubling — margin must be **positive** | 8 | +0.030 | +0.124 |
| Estimate needs halving | 0 | — | — |

The distributions overlap completely, and in the wrong direction: correct
estimates score *higher* than ones needing correction. Sweeping the threshold
confirms no separating value exists —

```
  margin 0.00:  fixes 5/8   breaks 84/135   net -79
  margin 0.15:  fixes 3/8   breaks 47/135   net -44
  margin 0.50:  fixes 0/8   breaks 15/135   net -15
```

**Two conclusions, both firm:**

1. **The on-beat / midpoint / alternation signal is uninformative on real
   music.** It is not weak or badly weighted — it is slightly *anti*-correlated
   with the thing it is supposed to detect. No threshold and no reweighting
   rescues it. This line of attack is closed.
2. **Octave relations are not where the errors are.** Only 8 of ~200 tracks had
   an octave error, and none needed halving. The residual ~10% is dominated by
   **triple relations** (2:3, 3:4) — compound meter read as straight or vice
   versa. A perfect octave corrector would fix roughly 4% of tracks.

**What a third attempt would have to do**, if the error rate ever becomes worth
paying for: work at the *bar* level rather than the beat level. Triple-versus-
duple is a question about metre, so it needs downbeat detection and a metrical
hierarchy (is the pattern grouping in 3s or 4s?), not a better beat-level
statistic. That is a substantially larger piece of work than either attempt
here, and it should not be started without first deciding whether ~90% tempo
agreement is actually good enough for the product — see *Open questions*.

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

## Decided: speak OpenSubsonic as a client

**Decision (2026-08-14): adopted.** Vapor gains an OpenSubsonic *client*
alongside the existing WebDAV backend. It does not become a server and does not
require one.

This does not contradict the "no server required" positioning in the README.
Vapor still runs no infrastructure — it supports bring-your-own-cloud (WebDAV)
*and* bring-your-own-server (OpenSubsonic), and the user picks.

**Why:**

- **An installed base with no migration cost.** Anyone already running
  Navidrome, Gonic or Airsonic has an indexed library and can point Vapor at it
  immediately.
- **It deletes code.** `metadata_service.gd` is 1,276 lines of Deezer CDN
  scraping and a hand-rolled `_parse_id3v2_tags`. An OpenSubsonic server serves
  MusicBrainz-resolved metadata and cover art over a documented API.
- **It may reduce analysis load.** Where a server exposes BPM via extended tags,
  those tracks need no on-device analysis.

> [!IMPORTANT]
> **Analysis must request the original stream, never a transcode.**
> Subsonic servers transcode on the fly (typically to 128 kbps Opus) for
> constrained networks. That is fine for playback and destroys the fidelity the
> DJ engine needs for beat-grid extraction and time-stretching. The client must
> keep two distinct request paths — original for analysis, optionally transcoded
> for playback — and this has to be designed in from the start, not retrofitted.

**Security note.** The legacy Subsonic auth scheme is `token=md5(password+salt)`
passed as query parameters, which means credentials appear in any upstream proxy
log and force weak server-side password storage. Compatibility requires
supporting it. Mitigations: require TLS, store the credential in the OS keychain
rather than the settings file, and prefer any modern auth the server offers.

**Deliberately out of scope.** The wider homelab ecosystem — ActivityPub
federation (Funkwhale), multi-room sync (Lyrion), jukebox mode, podcast RSS,
running a server — is not this product.

---

## Scope

**This is a refactor, not a quality project.** The goal is the same app, off
Godot. Analysis accuracy, key detection and mixing polish are explicitly *later*
work — they are tracked so they are not forgotten, not because they gate the
port. When a choice arises between matching existing behaviour and improving on
it, match it.

---

## Decisions (2026-08-14)

| # | Decision |
|---|---|
| Tempo | **Accept ~81% agreement.** Ship a manual BPM override in the UI. Refine later. |
| Key | **Defer.** 56.1% exact / 80.9% compatible is good enough to port with. |
| Dolby Atmos | **Transcode on import** to AAC or FLAC once, at add time. |
| Crossfade | **Keep Godot's dB-linear envelope**, midpoint dip and all. Fidelity over correctness. |

---

## Open decisions

1. ~~**Dolby Atmos / E-AC-3.**~~ **Decided: transcode on import.** 22 tracks do not decode
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
3. **Time-stretch library.** WSOLA is in place and works; `signalsmith-stretch`
   vs Rubber Band is a quality question, so it falls under *later*.
4. **UI framework** inside Tauri. Not yet chosen; defer until phase 4 so the
   redesign informs it.

---

## Actionable backlog

Every risk and finding above, as trackable items. Phase is when it must be
resolved by, not when it must be started.

### Correctness and coverage

| ID | Item | Phase | Source |
|---|---|---|---|
| MIG-001 | Key detection. **Partly done** — harmonic-weighted chroma and per-frame normalisation took it from 34.3% to 48.1% exact on a 108-track subset. Segmented analysis (`segment_keys` / `intro_key` / `outro_key`) is still outstanding. | 1 | Spike results |
| ~~MIG-002~~ | ~~Add beat-grid output so grids can be diffed against fixtures, not just BPM.~~ **Done** — DP beat tracking, F=0.763 mean / 0.884 median. See *Phase 1 progress*. | 1 | Spike results |
| MIG-002b | Resolve the ~10% tempo **metrical** errors. **Two attempts made and reverted; the beat-level approach is closed** — the signal is anti-correlated, and the errors are triple relations, not octaves. A third attempt means bar-level metre detection. Read *Phase 1 progress* first, and settle the product question before starting. | 1 | Phase 1 |
| MIG-003 | Decide the E-AC-3 / Dolby Atmos path. Shipping on macOS without one is a silent regression on 22 tracks. | 4 | Spike results, BUG-001 |
| MIG-004 | One malformed AAC file (`channel element 0.0 duplicate`) decodes to zero samples where ffmpeg tolerates it. Decide whether to harden or to surface as unplayable. | 2 | Spike results |
| ~~MIG-005~~ | ~~Port the portable parts of `audio_dsp.cpp`.~~ **Mostly done** — cue in/out, LUFS and waveform peaks ported and validated (LUFS agrees to 0.003 LU). Dynamic range and transients still outstanding. | 1 | `audio_dsp.cpp` |
| ~~MIG-006~~ | ~~Bass Swap clips.~~ **Done** — three-band RMS guard ported *and* a master peak limiter added. The RMS port alone did not fix it: the clipping is peak-domain (RMS 0.257 vs a 0.630 threshold, crest factor 3.9x), so the Godot original would not have caught it either. All three transitions now measure 0 clipped samples. | 2 | Mixer spike |
| ~~MIG-007~~ | ~~Post-transition tempo snaps back to 1.0.~~ **Done** — eased over 6 s with sine easing, matching `_pitch_ramp_tween`. Tested for monotonicity, since a wandering ratio would be heard as wow and flutter. | 2 | Mixer spike |
| MIG-008 | Implement the remaining transition types — Echo Out, Reverb Freeze, Tempo Morph — which need delay and reverb. | 2 | Mixer spike |
| MIG-009 | Port the PLL drift correction, vocal-clash mid-cut and phrase-adaptive durations from `audio_manager.gd`. | 2 | Mixer spike |
| ~~MIG-014~~ | ~~Octave-error detection must also cover metrical errors (3:4, 2:3).~~ **Done** — the class is 10.6%, not 4.4%. | 1 | Mixer spike |
| MIG-015 | Decide whether Standard Crossfade should become equal-power. The Godot envelope is dB-linear and dips at the midpoint; the behaviour is currently replicated deliberately and pinned by a test. | 2 | Mixer spike |
| ~~MIG-016~~ | ~~No resampler; mismatched sample rates were refused.~~ **Done** — windowed-sinc (32-tap, Blackman, 512 phases), converting at load rather than per block. Tested for level preservation and for anti-aliasing on downsampling, which is the failure that makes a naive resampler unusable. | 2 | Mixer spike |

### Engineering risk

| ID | Item | Phase | Source |
|---|---|---|---|
| ~~MIG-010~~ | ~~Real-time-safe audio thread discipline.~~ **Done** — `Mixer::render` is allocation-free, *asserted* by a counting global allocator (0 allocations across a transition and glide) rather than by inspection. `play` now renders inside the audio callback. Transitions are scheduled ahead via `schedule_transition`, keeping the fallible decision off the audio thread. | 2 | Risks |
| MIG-011 | Validate `cpal` on Android and iOS **early**, on device. Less battle-tested than desktop; discovering this in phase 4 is too late. | 2 | Risks |
| MIG-012 | Choose the time-stretch library on measured quality: `signalsmith-stretch` (MIT) vs Rubber Band single-file (GPL). | 2 | Open decision 3 |
| MIG-013 | Verify the wasm audio path end to end — the crate compiles for wasm, but AudioWorklet integration is unexercised. | 4 | Spike limits |

### Mobile and platform

| ID | Item | Phase | Source |
|---|---|---|---|
| MIG-020 | **iOS background sync will stall.** Cloud-first caching means large background downloads, which iOS Background App Refresh throttles and silently kills — the documented failure mode in comparable clients. Design chunked, resumable transfers with honest progress UI; do not assume a large sync completes in the background. | 4 | Ecosystem research |
| MIG-021 | Android background sync must adapt to Doze rather than fight it. | 4 | Ecosystem research |
| MIG-022 | Windows SMTC media-control code has never been compiled on any machine. Either build and test it, or drop it from the port. | 4 | `docs/CROSS_PLATFORM_DSP.md` |

### Interop

| ID | Item | Phase | Source |
|---|---|---|---|
| MIG-030 | OpenSubsonic client: separate request paths for original (analysis) vs transcoded (playback) streams. | 3 | OpenSubsonic decision |
| MIG-031 | OpenSubsonic auth: TLS required, credential in OS keychain, never in the settings file. | 3 | OpenSubsonic decision |
| MIG-032 | Verify OpenSubsonic extension names and field shapes against the actual specification before designing to them — the secondary sources reviewed contained errors. | 3 | Ecosystem research |
| MIG-033 | Consume server-provided BPM/key via extended tags where available, to skip redundant on-device analysis. | 3 | OpenSubsonic decision |

### Process

| ID | Item | Phase | Source |
|---|---|---|---|
| MIG-040 | Stand up CI before migration work. Record the 12 currently-failing GUT tests as a known-failing baseline so the suite gates on regressions immediately. | 0 | Phase 0 |
| MIG-041 | Translate GUT test *assertions* to Rust before porting the code they cover — they are the specification. | 3 | Phase 3 |
| MIG-042 | Do not couple the AGPL exit to the migration. Revisit licensing only once phases 1–2 fix the dependency set. | 5 | Licensing |
