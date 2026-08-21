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

**The library screen opens on shelves rather than on the album grid.** Decided
2026-08-21, and a deliberate departure from the Daylight design, whose Library
carries four tabs and lands on Albums.

The reason is what someone is doing when they open the app. Almost nobody
arrives at their own library looking for a particular record; they arrive
wanting something on, and the thing they reach for is a playlist they already
have. An album grid answers "which record", which is the rarer question, and it
was the first and only thing the screen offered. Tidal and Spotify both land on
rows of recent and most-played collections for the same reason.

So: a search field, then four shelves ranked most-played-first — playlists,
smart groups, artists, albums — each scrolling sideways and capped at twelve
tiles, with the four grouping tabs kept behind them for the visit that does
know what it wants. Smart groups are second, above artists, because they are
the thing this app has that the others do not.

Three things fell out of the decision and are worth writing down:

* **A shelf needs play counts, and there were none.** Tracks now earn a listen
  after 30 seconds (or halfway, under a minute), and a playlist or group earns
  one alongside whichever of its tracks did — credited by id rather than by
  name, so renaming a playlist does not reset it.
* **Ranking falls through four keys**, because each runs out: direct plays,
  then plays of the member tracks, then when it was last played, then size. The
  second is what stops a playlist built this morning out of worn-out records
  sorting below one nobody has ever opened. The fourth is what the shelf says
  on the day the library is new and every count is zero — biggest first is at
  least about the person's music, where alphabetical would put A first for
  ever.
* **Typing searches; it does not filter the shelves.** A shelf holds the first
  twelve of something ranked by plays, so narrowing one would answer "you have
  no such album" for an album that is in the library and merely thirteenth.

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

**The third attempt was made on 2026-08-16 and also failed. Here is what it
cost and what it learned, so a fourth starts further along.**

*The validator could not run at all.* Fixtures name audio by the Godot build's
MD5-of-href; the Rust shell names it `fnv1a(href)`. `validate` found zero files
and reported a clean sweep of zero. It now looks under both. **44 of the 563
fixtures have local audio** — every number below is from those 44, which is
thin, and that is part of the finding rather than a caveat on it.

*The residual is two different things.* `bin/metre-probe.rs` splits the six
failures by whether the comb filter — the evidence — or `octave_prior` — the
opinion — chose the wrong answer:

| | Count |
|---|---|
| comb preferred Essentia's answer, prior overrode it | 3 |
| comb itself preferred ours | 3 |

*The prior is load-bearing and must not simply be weakened.* On tracks that come
out **right**, the true tempo's margin over its nearest metrical rival runs from
−1.127 to +0.333 (median +0.121). On at least one, the rival's comb score is
over twice the truth's and the prior is the only reason the answer is right.
That is why the two earlier attempts were reverted, and it is measurable now
rather than folklore.

*The bass band is the one untried signal that points the right way.* The onset
function spans 30 Hz – 5 kHz and whitens, so the kick is one voice among many.
Restricted to 30–120 Hz, on the six failures it prefers the true tempo on four
and is neutral on two, where the broadband comb prefers the wrong one on all
six. But on the 38 that already work its margin is +0.018 median across a
−0.357 to +0.342 spread. **Six errors, no held-out set: any blend tuned on this
is fitting noise, so none was.**

**Downbeat detection does not work on this library, and the reason is
musical.** `vapor_dsp::metre` is correct on synthetic audio — every phase of
4/4, a 3/4 bar, and no metre claimed for a track with no percussion. Against
Essentia's downbeats on the 26 tracks whose beat grids already agree:

| | Result |
|---|---|
| bar length | 4/4 on all 24, correct |
| downbeat F-measure | mean 0.194, median 0.006 |
| F ≥ 0.8 | 3 of 24 (12.5%) |
| chance for a 4-phase choice | 25% |

Below chance. The premise — the kick marks the downbeat — is false for
four-on-the-floor, where the kick is on *every* beat and distinguishes no phase.
Spectral novelty between beats was tried as the alternative and scored 0.213,
still half of chance, while claiming a metre for a track with no percussion.

The confidence score does not rescue it: the two highest-contrast tracks score
F = 0.011 and 0.021. **Confidence is uncorrelated with correctness, so there is
no threshold at which reporting a downbeat becomes honest** — which is why
`Analysis` carries no downbeat field. A field that is right 12.5% of the time
with a confidence that does not know it is wrong is worse than no field, because
`phrase_duration` would happily align mixes to it.

Next attempt: not another onset feature. The downbeat in this music is carried
by harmony and arrangement — the bassline changing note, 4- and 8-bar phrase
structure — which is a self-similarity problem over beat-synchronous features.
And get more than 44 tracks cached first.

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

**The shared-document merge is additive for everything that exists**, so it
cannot lose work and converges in one pass whichever order two devices sync in.

**A deletion is the exception, because there is nothing to add for a record
that is gone.** It travels as a tombstone — an id and the time it happened —
kept indefinitely, since a device that has been off for a year still has the
playlist and the document is the only place it will ever hear otherwise.

A tombstone applies unconditionally, so a deletion beats a concurrent edit it
never saw. That is a real loss and it is the chosen behaviour. Weighed against a
deletion that previously failed to travel *every* time, blunt won.

**`PlaylistStore::get_mut` is now private, which removes the first objection to
refining it.** Every change to a playlist goes through a method on the store, so
a modification time could be stamped in one place and the compiler would find
any mutation that failed to supply it. Both stores were checked: no public
field, and no method on either hands out a `&mut` to a record.

**The second objection is clock skew, and it is the one that should decide it.**
"An edit newer than the tombstone keeps the playlist" compares a timestamp taken
on one device against a timestamp taken on another. Two machines whose clocks
differ by minutes — which is ordinary, and which nothing in the sync path
detects or corrects — make that comparison meaningless in whichever direction
the skew runs, and the failure is silent. The current rule reads no clock to
decide anything: a tombstone's timestamp is recorded and carried, never
compared. That is why it cannot be fooled by a wrong one.

So the remaining choice is not "blunt versus correct", it is "a deletion that is
always obeyed" against "an edit that survives only if two clocks agree". A
version vector or a Lamport clock would settle it without reading wall time, and
that is the shape to reach for if this is ever revisited — not a timestamp.

**Bumping `SHARED_VERSION` was the load-bearing part of that change**, not
bookkeeping. The tombstone field is `#[serde(default)]`, so a build that
predates it would read the document, drop the tombstones it did not understand,
and write back one with every deletion undone. The version check turning that
into a refusal is the whole reason the check exists.

**A parser can pass every test and never have worked.** `genre_of` reads
`genres.data[0].name`, which is correct — for the *full album* response. It was
being handed the *album search* response, which carries no `genres` object at
any level, only a numeric `genre_id`. So it returned an empty string for every
track ever looked up, which looks exactly like an album with no genre. Fifteen
tests covered it, all green, all written from reading the GDScript rather than
from a real response. Verified against the live service 2026-08-16:
`/search/album` for *Discovery* → no `genres`; `/album/302127` → `Electro`.

The lesson is about where the canned bodies come from, not about the parser.
Test fixtures invented from a spec test the spec. `metadata.rs` now holds four
bodies captured from the real services, and an `#[ignore]`d test that re-checks
them against the network on demand.

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

## The DJ workflow document, split (2026-08-19)

`docs/ai_dj_workflow.md` was two documents in one file. It is the help sheet the
Vibe screen renders verbatim — and it also carried file paths, function names,
the changelog of what each rule used to be, and the measurements behind them.
Rendered at 17px in a modal, that reads as an engineering note somebody left on
a user's screen: `(current_track_index + 1) % playlist.size()`, "the thresholds
are in `exit_between` in `vapor-app/src-tauri/src/lib.rs`", "§2–§4".

The document is now the help text, in plain words. What follows is what came out
of it, which is the half worth keeping and the wrong half to show a listener.

**The three exits replaced Match / Fresh / Switch.** Fresh meant "similar genre,
deliberately about 15 BPM and 0.25 energy away" — it proposed a track the set had
not planned, so the suggestion and the queue could disagree about what was coming
next, and the screen showed a badge for one and a highlight for the other. Follow
is defined as *whatever is queued*, which is what makes those two agree by
construction.

**The four-step choice cycle is gone.** The original cycled Match → Fresh →
Match → Switch and called whichever step it was on the "AI Choice". The default
answer depended on a hidden counter, so the same pair of tracks got a different
verdict depending on when you arrived — and the counter advanced on manual
overrides too, so overruling one transition silently changed the next.

**The `🤖 AI Choice` badge is gone with it.** It stayed on the DJ's own pick
while the highlight moved to a manual override: two marks answering one question
is a screen disagreeing with itself.

**Three of the four curves were unreachable.** `play_harmonic_shuffle()` called
`DJPathfinder.generate_mood_path()` with two arguments, so `target_curve` always
fell to its default of `"build"`. Build, Chill, Wave and Hold Steady were all
implemented; one of them could be chosen.

**Energy was a dynamics ratio — mean RMS over peak RMS — until 2026-08-17.** That
measures how *consistent* a track is rather than how hard it goes: one that sits
at a single level scores high, one with a breakdown scores low. On the 534-track
library it put drum & bass at 0.661 against 0.629 for ballads, ranges overlapping
completely, with the quietest records at the top. Integrated loudness mapped over
−30 to −5 LUFS separates the same two groups by 0.256 instead of 0.031. It was
deciding the curves, the energy term in the transition cost, and whether two
tracks count as a match.

**The spec claimed energy was "loudness, brightness and tempo in equal parts".**
It never was. The sentence outlived the code it described.

**Exit thresholds live in `exit_between` in `vapor-app/src-tauri/src/lib.rs`**
and were measured against that same 534-track library rather than chosen.

**Stay is asked of every candidate, not only of the ones inside its band.** A
library holding nothing within 8 BPM still has a closest track, and showing two
cards because the third missed a threshold is the screen withholding an answer it
has. Switch is then taken from what Stay left.

## The mix "pops and scratches" (2026-08-20)

Reported as a stutter first, then as "record scratch" and "interference static".
Three theories were asserted before any were measured; two were wrong. What the
instrumentation actually showed:

**Nothing starves.** `audio-faults.log` recorded no deck starvation across an
afternoon of playback including transitions. The decoder, the prefetcher and the
WebDAV path are not involved, which retires the original bandwidth theory —
`arm_mix` blocking on a full download was real but was never this.

**The stretcher outputs above full scale at beat-matching ratios.** Measured
against a 0.99 tone at 44.1 kHz:

```
ratio 1.00 -> 0.9900   (passthrough, bit-exact)
ratio 1.02 -> 1.0272   (+0.23 dB)
ratio 1.03 -> 1.0362
ratio 1.06 -> 1.0676   (+0.57 dB)
ratio 0.97 -> 0.9987
```

Overlapping windows interfere constructively; a phase vocoder is expected to do
this and it is not a defect. The limiter catching it is correct.

**The limiter is what makes the noise.** `Limiter::process` computes one gain
per block from the block peak, attacks instantaneously, and multiplies the whole
block by it — so every change lands as a step discontinuity at a block boundary.
One step is a click; a run of them at the block rate is a buzz. Observed in the
field as five consecutive 250 ms windows with steps, deepening to −1.7 dB, at
the moment the noise was heard.

`write_out` already ramps the master volume across the block for exactly this
reason, and says so in its own comment. The limiter was the one gain stage that
did not.

**Untagged as a mix, but caused by one.** The post-transition tempo glide
(`TEMPO_GLIDE_SECS`, 6 s) keeps the stretcher running after the transition has
completed, so `transition_armed()` is already false while the deck is still
being stretched. Faults logged during the glide carry no "(during a mix)" tag
and look like ordinary playback.

**`webdav::tests::a_changed_password_is_not_served_from_before` flakes on the
first run after a rebuild, on macOS.** It writes to the real keychain, and
keychain ACLs are bound to the requesting binary's code signature. Test binaries
are ad-hoc signed, so the signature is a hash of the binary — every rebuild is a
new identity, "Always Allow" no longer matches, and macOS asks again. Unanswered,
the write fails and the test fails; run it again against the same binary and it
passes. Observed 2026-08-20 on the first run after a full dependency recompile.
Not a product defect and not a race: the same reason `npm run app` re-asks for
the keychain on every build.

**LIM-001 confirmed fixed in the field (2026-08-20).** Ramping the limiter's
gain across the block removed the noise without touching how much it reduces.
Same listener, same library, both directions measured:

```
before   run max -5.4 dB, windows up to 19 steps   -> "pops and scratches"
after    run max -8.3 dB, 30 windows over 11 s     -> "completely clean"
```

The confirming case is the one that matters: a beat-matched Bass Swap between
two loud masters (Power Glove, 128.1/4A into Vancouver Beatdown, 128/4A), which
drove the limiter *deeper* than anything recorded before the fix and was
inaudible. A quieter result would have proved nothing, since three earlier clean
mixes turned out to be dissolves that never engaged the limiter at all.

**Two warts in the instrumentation that wrote this finding**, recorded because
they cost real confusion at the time:

* `limiter_deepest_db` is a **run-wide** minimum that only ratchets. Read as a
  per-window depth — which it looks like in the log line — it makes a flat value
  seem like a sustained deep reduction, and a rising one seem like a single
  window deepening. The per-window figure that is honest is the step count.
* `clock_time` stamps **UTC** while the person reading the log is on local time,
  so lines appear to be five hours in the past.

**Dissolves cannot produce this.** Echo Out and Reverb Freeze run at ratio 1.0,
which is the stretcher's passthrough path — a bit-exact copy. Only a
beat-matched move (Bass Swap, Tempo Morph) runs the vocoder, and only the
vocoder manufactures gain. When every exit on the Vibe screen reads "echo out",
no pair reachable from that track can reproduce it.
