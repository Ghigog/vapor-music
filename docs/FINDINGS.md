# Findings

Measurements, and decisions with the reason attached. Append-only.

## The one rule

**Nothing here describes what currently works.**

Whether the scan works, whether a test passes, how many tests there are, what
is still missing — none of that belongs in a document. It is knowable by
running the thing, it changes without anyone editing prose, and a stale claim
is worse than no claim because it gets repeated with confidence. `HANDOVER.md`
said "he has never confirmed a scan works" for weeks after the scan worked, and
that sentence was read back to the person whose library it was, three times.
The file is gone.

What goes here instead:

* **A measurement**, with what was measured, on what, and when. `60.6% exact on
  563 tracks, 2026-08-16` is true forever, whatever the number is today.
* **A decision**, with the reason, so it is not relitigated. Especially a
  decision *not* to do something, and especially one that was tried.

Status lives in `docs/workspace/tickets.md`, and nowhere else.
`tests/docs_state_claims.rs` fails the build if this rule is broken.

---

## Design

**The design preserved the original model; the rewrite reduced it.** Audited
2026-08-16 against `design/Vapor Music v2 - Daylight.dc.html`, which
`design/README.md` calls the source of truth. The drift was between the design
and the React app, not between the original app and the design.

What the audit found, and what was rebuilt from it:

* The design defines **three** destinations in code — Library, Vibe, Settings —
  and carries twelve *mockups*, two of which are states rather than screens.
  The rewrite read twelve mockups as twelve sidebar entries. Songs and Search
  are tabs inside Library in the design; Queue is captioned "06 Queue — bottom
  sheet" and belongs to Vibe, which relabels itself "Shuffle" when the DJ is
  off.
* The design's `alternates` are the original's `perfect` / `interesting` /
  `creative` triple under the names MATCH / FRESH / SWITCH, colour-coded green,
  accent and amber, each carrying one of the six transitions. The rewrite kept
  the planner and dropped the chooser, so the screen could plan a set and never
  show the choice it was making.
* Three labels in the rewrite had no source at all: "Wind down" for what the
  original calls Chill Down, "Steady" for a fallback case the original never
  named, and four blurbs written during the port. The curve *ids* were always
  right; only the words drifted.

The engine was a faithful port throughout — same A\* search, same weights, same
Camelot graph, same four curves with identical arithmetic. It was the interface
to it that was reduced.

---

## Analysis

**Key detection, 563 tracks, ground truth from the owner's library.**

| When | Change | Exact | Compatible |
|---|---|---|---|
| Baseline | Port of the GDScript estimator | 34.3% | — |
| 2026-08 | Harmonic-weighted chroma, per-frame normalisation | 48.1% | — |
| 2026-08 | Chroma from spectral peaks rather than every bin | 56.1% | 80.9% |
| 2026-08-16 | Segmented analysis (TD-13) | 60.6% | 82.8% |

A drum hit is broadband and was depositing energy into all twelve pitch
classes; feeding the chroma from peaks is what moved it most.

**Tuning correction was tried and is worse.** 58.1% against 60.6%. Reverted
2026-08-15. Do not try it again without changing something else first.

**Tempo.** Agrees with Essentia on ~81% of the same library. The residual is
metrical error — 3:4 and 2:3 relations, not octaves — and is 10.6% of tracks,
not the 4.4% first assumed. Two attempts at fixing it at the beat level were
made and both reverted: the signal the second one relied on is anti-correlated.
A third attempt means bar-level metre detection.

**Beat grids.** DP beat tracking, F=0.763 mean and F=0.884 median against the
same set. The estimator it replaced measured F=0.470.

**A corrected tempo re-runs the tracker; it does not re-label or subdivide.**
The 10.6% metrical residual above is what the hand correction exists for, and
those are 3:4 and 2:3 relations, which do not subdivide into the tracked beats
at all. Even a clean 2:1 leaves a question arithmetic cannot answer: halving a
grid means dropping every other beat, and *which* every-other decides whether
the result is on the beat or exactly off it — the worst available answer rather
than a near miss. `beats::track` picks it from onset strength, so the
correction re-tracks at the new tempo against the same whole-track onset
function (`vapor_dsp::retrack_beats`). Key, loudness, cue points, waveform and
segments are all independent of tempo and are not recomputed.

Storing it needed `Analysis::beats_bpm` — the tempo a grid was tracked at, kept
separate from the track's tempo, so a grid can never be read at a tempo it was
not built for. Absent means "tracked at `bpm`", which is true of every entry
written before the field existed and is why it did not cost a library-wide
re-analysis.

**Loudness.** The ported LUFS agrees with the C++ original to 0.003 LU.

---

## Mixing

**The PLL's grid term does nothing here, and this was measured rather than
assumed.** Both decks advance from the same audio clock, so a static grid has
no phase error to find: 51.65 ms worst beat deviation uncorrected, 51.65 ms
with the grid term, and 51.67 ms with the original's unscaled version — that
is, slightly worse. The **waveform correlation** is the term that works:
28.29 ms, a 45% improvement. Ported 2026-08-16.

**Standard Crossfade was not equal-power and is now.** The original
interpolated both gains linearly in dB, which puts both decks at −30 dB at the
midpoint — a hole in the middle of every mix. Now a `cos`/`sin` pair whose
squares sum to one.

**Bass Swap clipping is peak-domain, not RMS.** The three-band RMS guard was
ported faithfully and did not fix it: RMS measured 0.257 against a 0.630
threshold, with a crest factor of 3.9. The original would not have caught it
either. A master peak limiter did. All transitions measure 0 clipped samples.

**`vocal_presence` was never a detector.** It is `energy > 0.35`. Half a day
went into planning a vocal detector before anyone grepped for it.

**Signalsmith Stretch is the default, at 0.18 ms.** Chosen over Rubber Band
(GPL plus a C++ build system, which is the dependency tail the migration
existed to remove) and élastique (proprietary). MIT, maintained Rust wrapper,
allocation-free on the audio thread across 200 blocks, finite at every ratio,
exact pass-through at unity. WSOLA remains what wasm uses, since the C++ does
not build there.

| Stretcher | Worst onset error, 128 BPM transition |
|---|---|
| Signalsmith, as first integrated | 118.4 ms — failed |
| WSOLA | 5.84 ms |
| **Signalsmith, corrected** | **0.18 ms** |

All three from `beat_alignment`, which renders a whole transition. Not the same
measurement as the 28.29 ms in the PLL note above — that one is `pll_drift`,
over a longer run and a different quantity — and the two were briefly conflated
in the table this replaces.

**A latency you can only fix on one side.** Signalsmith reports 2646 frames of
input latency and 2646 of output latency at 44.1 kHz. `examples/impulse.rs`
drives the wrapper directly with a click and measures where it comes out; the
mapping is `output = (input + Lᵢ)/ratio + Lₒ`. The two latencies live in
different domains — output latency is already output frames, input latency only
becomes them after dividing by the ratio — so the correction is a one-sided
output discard of `Lₒ + Lᵢ/ratio`, with no pre-roll and no reading behind the
start position.

**Pre-roll cannot fix a latency, and three attempts proving it looked like
three bugs.** Priming with upcoming audio, `seek` pre-roll, and flushing the
latency through `process` each moved the 118.4 ms by *zero*, because pre-roll
shifts input and output together and the difference between them is what the
error is. `seek` — the library's own documented pre-roll API — produces
**bit-identical output to no compensation at all**, which is the fact that
ended the guessing. Measured 1.02/1.05/0.95 ratio: none 119.6/117.7/123.6 ms,
`seek` 119.6/117.7/123.6, push-input 119.7/117.4/123.6, feed-and-discard
119.0/117.3/123.4, output-discard **0.2/0.5/0.4**.

**A `cfg` on a module does not gate its dependency.** `src/signalsmith.rs` was
correctly `#[cfg(not(target_arch = "wasm32"))]` while `signalsmith-stretch` sat
in plain `[dependencies]`, so cargo built the C++ for wasm regardless of the
fact that nothing imported it — `<complex>` has no libc++ on
`wasm32-unknown-unknown`. Red wasm job for two commits. Native-only crates
belong under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.

**One field cannot be both a cursor and a position.** The bug underneath the
above: `read_pos` was the source frame to read from *and* the source frame
being heard. Latency is exactly the statement that those are different numbers,
so the field could not be right for both and the correction had nowhere to
live. Two fields, `read_pos` and `feed_pos`, and the gap between them is the
latency made explicit.

**±6% is the stretch refusal.** Past that it stops sounding like the record, so
the pair plays sequentially instead of mixing.

---

## Memory and I/O

**A deck costs 1 MiB regardless of track length.** It reads a five-second
window that a decoder thread keeps filled. Before: 55 MB for five minutes, and
a track change displaced a ~106 MB buffer.

The window keeps 8192 frames of history because the WSOLA search reads
*backwards*, which a queue cannot answer.

**The audio thread neither allocates nor frees**, asserted by a counting
allocator across a transition and a glide rather than by inspection. Both
counters are per-thread; the process-wide version was measuring the test
harness.

---

## Decisions that were made once and should not be remade

**Dolby Atmos: won't fix.** 22 tracks in one library are E-AC-3 in `.m4a`.
Atmos is a spatial format with no stable stereo image to beat-match against;
Serato and Rekordbox do not support it either. Bundling ffmpeg to decode a
format the app cannot DJ with is a large dependency for a bad trade.

**The core owns no randomness and no wall clock.** `randi()` inside a library
is what made the GDScript mood path untestable. Permutations, PINs and
timestamps are generated in the shell and passed in.

**The core owns no I/O.** Persistence, HTTP and the filesystem live in the
shell, which is what makes the core testable without an engine and reusable in
the browser.

**Unknown renders as "—", never as 0 or a guess.** The Godot stub fabricating
120 BPM is the failure this prevents.

**Lyrics and artwork are off until asked.** Everything else the app knows is
worked out on the device from the audio; a lookup sends the artist and title of
what someone is listening to to a third party. The Godot build did it
unconditionally and said nothing.

**Local sync is off until asked**, for the same reason: a beacon every five
seconds announces this machine to whatever network it has joined.

**SHA-256, not MD5**, for both fingerprints and transfer verification. The
requirement is integrity and MD5 has been collision-broken since 2004.

**The shared-document merge is additive**, so it cannot lose work and converges
in one pass whichever order two devices sync in. The cost is that a deletion
does not travel.

**Windows SMTC was not ported.** 191 lines of C++/WinRT inside a GDExtension
that is being archived. One cross-platform crate replaced all three platform
ports.

---

## Traps that have cost real hours

**`pgrep -f "…"` wait-loops never exit** — the pattern matches the loop's own
command line. Six spun for eight hours. `pkill -f "vite --config …"` also
misses the server, because npm launches it as `node …/.bin/vite`.

**Check exit codes, not grepped output.** A pipe through `head` reported a
non-compiling commit as passing.

**jsdom has no layout.** Zero-height elements, no `DataTransfer`, no
`DragEvent` — all stubbed in `src/test/setup.ts`. A bug that is entirely about
something moving is invisible to it.

**A `#[tauri::command]` takes `State`, which cannot be constructed outside a
running app.** Logic in a command body is logic tests cannot reach. Split it.

**macOS routes media keys to the Now Playing *application*** — meaning a `.app`
bundle. `tauri dev` runs a bare binary and will never receive them. souvlaki's
macOS backend returns `Ok` from `new` and `attach` unconditionally, so nothing
reports this.

**MediaPlayer and AppKit APIs want the main thread.** souvlaki does not marshal
for you.

**`clamp` passes NaN through and panics on a NaN bound.** Handle non-finite
before it, not with it — the value usually came off disk.

**Grep before building.** The port carried parameters across without their
behaviour *and* carried behaviour across that nothing ever called. Both
directions have the same tell: an argument nobody varies.
