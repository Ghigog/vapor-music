# vapor-core

Portable Rust core for Vapor Music. See [`docs/MIGRATION.md`](../docs/MIGRATION.md)
for why this exists and where it is going.

At this stage it is a **spike**, not a product: it exists to answer one question
before the migration commits to anything — can the audio analysis that currently
only works on macOS (Essentia via a GDExtension, linking the whole ffmpeg stack
and shelling out to Homebrew binaries) be reproduced in portable Rust?

## Crates

| Crate | Status | Purpose |
|---|---|---|
| `vapor-dsp` | spike | Decode, tempo, beat grids, key, cue points, loudness |
| `vapor-engine` | spike | Two-deck mixer, EQ chain, time-stretch, transitions, limiter |
| `vapor-library` | not started | Playlists, groups, pathfinder, metadata |

Neither crate has audio I/O, an engine or platform code, so both run under
`cargo test` on any CI runner — the property the 224-test GUT suite lacks — and
both build for `wasm32-unknown-unknown`. Audio output lives only in the `play`
binary, which is why cpal is a target-gated dependency rather than a crate one.

### Where things stand, measured

Against 563 real tracks, using the Essentia results the Godot app already
produced as ground truth. See `docs/MIGRATION.md` for how to read these — they
are agreement with another estimator, not a correctness score.

| | Result |
|---|---|
| Decode | 540/563 (E-AC-3 / Dolby Atmos is the gap) |
| Tempo, exact | ~81% |
| Beat grid, F-measure | 0.763 mean / 0.884 median |
| Key, exact Camelot | ~48% — **not yet shippable** |
| `lufs` | agrees to 0.003 LU |
| Beat-match accuracy | 6.11 ms worst deviation, rendered |

## Listening to a transition

Numbers prove alignment; ears judge whether a mix sounds right.

```bash
cargo run --release -p vapor-engine --bin render -- \
    trackA.mp3 trackB.mp3 out.wav bassswap 8
```

Transitions: `crossfade`, `bassswap`, `filtersweep`. Add `--bpm-a=` / `--bpm-b=`
to override the detector — useful while MIG-001/MIG-002 are open, since a wrong
BPM estimate will refuse an otherwise fine transition.

To play it through the default output device instead:

```bash
cargo run --release -p vapor-engine --bin play -- trackA.mp3 trackB.mp3 bassswap 8
```

> [!NOTE]
> `play` renders the whole mix before starting the stream; the callback only
> copies. That is deliberate for a spike — it separates "does output work" from
> "is the engine real-time safe". Rendering inside the callback is phase 2
> (MIG-010).

## Running the tests

```bash
cd vapor-core && cargo test --release
```

These are synthetic-signal tests: a click track at a known tempo, a synthesised
C major triad, and structural properties of the Camelot wheel. They verify
correctness on cases where the answer is known independently of any reference
implementation.

## Validating against the real library

The synthetic tests say nothing about how the analysis behaves on actual music.
For that, the Godot app's own Essentia output is used as ground truth.

Generate the fixtures (reads the running app's metadata cache — not committed,
as it is multi-megabyte and contains personal track paths):

```bash
node vapor-core/tools/extract-fixtures.mjs
```

Then run the comparison:

```bash
cd vapor-core && cargo run --release --bin validate -- "$HOME/Library/Application Support/Godot/app_userdata/vapor music/audio_cache"
```

It reports decode success by container, tempo agreement (separating octave
errors, the characteristic beat-tracker failure), and key agreement (separating
exact matches from harmonically adjacent ones).

Pass a trailing number to limit the run:

```bash
cargo run --release --bin validate -- "<audio_cache_dir>" 40
```

## A note on "agreement"

Agreement with Essentia is a **proxy**, not a correctness measure. Essentia is
itself an estimator — its published key accuracy against human annotation is
roughly 70–80%. A disagreement may be this implementation being wrong, Essentia
being wrong, or the track being genuinely ambiguous. Treat these numbers as a
regression signal, not a grade.
