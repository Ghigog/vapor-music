# Licenses

Vapor Music is **proprietary — all rights reserved**. The terms are in
[`../LICENSE`](../LICENSE).

This file said AGPL-3.0 until 2026-08-21. That was true of the Godot build and
stopped being true on 2026-08-20, when the licence moved to all rights reserved
across seven declarations; this was an eighth nobody had counted. It is recorded
rather than quietly overwritten because a stale licence grant is the one kind of
mistake that cannot be taken back once someone has relied on it.

This directory holds the license texts of bundled third-party components, so
they travel with the application as the licenses require. See
[`../THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) for what each component
is used for, and [`../docs/LICENSING.md`](../docs/LICENSING.md) for the
compliance reasoning.

## Icons

Icons in `vapor-app/public/assets/icon/` are by **Gregor Cresnar** via
[the Noun Project](https://thenounproject.com), used under the
[Creative Commons Attribution 4.0 licence](https://creativecommons.org/licenses/by/4.0/).

CC BY requires attribution visible to users of the work, not only in the source
repository — this is what the in-app About → Licenses screen is for.

## What was removed with the Godot tree

`Essentia-AGPL-3.0.txt`, `RubberBand-GPL-2.0.txt`, `GodotEngine-MIT.txt`,
`godot-cpp-MIT.txt` and `GUT-MIT.txt` were deleted on 2026-08-21 along with the
tree that used them. Essentia and Rubber Band were the strong-copyleft
dependencies behind the original AGPL conclusion; neither survives in the Rust
build, where analysis is `vapor-dsp` and stretching is Signalsmith (MIT). None
of the five shipped in a Rust build, so nothing that was ever distributed is
missing a notice. `godot-final-v1.78` has them if the history is needed.
