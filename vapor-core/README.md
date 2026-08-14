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
| `vapor-dsp` | spike | Decode, tempo estimation, key detection |
| `vapor-engine` | not started | Two-deck mixer, EQ chain, transitions |
| `vapor-library` | not started | Playlists, groups, pathfinder, metadata |

`vapor-dsp` has no audio I/O, no engine and no platform code, so the whole crate
runs under `cargo test` on any CI runner — the property the 224-test GUT suite
lacks.

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
