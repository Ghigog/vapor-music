# Third-Party Notices

Vapor Music incorporates the following third-party components.

> [!NOTE]
> Two of these are strong-copyleft licensed (Essentia — AGPL-3.0, Rubber Band —
> GPL-2.0-or-later), which is why Vapor Music itself is licensed **AGPL-3.0**
> (see `LICENSE`). License texts ship in `licenses/`. Remaining compliance steps
> are tracked in `docs/LICENSING.md`.

---

## Native audio analysis and processing

Linked into the `AudioDSP` GDExtension (`bin/libaudio_dsp.*`).

### Essentia — AGPL-3.0

Audio analysis library from the Music Technology Group, Universitat Pompeu
Fabra. Used for BPM and beat-grid extraction (`RhythmExtractor2013`) and musical
key detection (`KeyExtractor`).

<https://essentia.upf.edu> — <https://github.com/MTG/essentia>

Commercial licensing is available from the MTG/UPF as an alternative to the
AGPL.

### Rubber Band Library — GPL-2.0-or-later

Audio time-stretching and pitch-shifting library by Breakfast Quay. Used for
pitch-independent tempo adjustment during transitions.

<https://breakfastquay.com/rubberband/>

Commercial licensing is available from Breakfast Quay as an alternative to the
GPL.

### Transitive dependencies of Essentia

The current macOS build links these via Essentia's audio-loading and metadata
algorithms: **FFmpeg** (libavformat, libavcodec, libavutil, libswresample —
LGPL-2.1-or-later, some builds GPL), **TagLib** (LGPL-2.1 / MPL-1.1),
**Chromaprint** (LGPL-2.1-or-later), **libsamplerate** (BSD-2-Clause),
**FFTW3** (GPL-2.0-or-later), **libyaml** (MIT).

Phase 1 of `docs/CROSS_PLATFORM_DSP.md` removes this entire set by replacing
Essentia's file loaders with self-contained decoders.

---

## Engine and framework

### Godot Engine — MIT

<https://godotengine.org> — Copyright (c) 2014-present Godot Engine
contributors; (c) 2007-2014 Juan Linietsky, Ariel Manzur.

### godot-cpp — MIT

GDExtension C++ bindings. Copyright (c) 2017-present Godot Engine contributors.

<https://github.com/godotengine/godot-cpp>

### GUT (Godot Unit Test) — MIT

Test framework, `addons/gut/`. Development-only; not included in exported
builds.

<https://github.com/bitwes/Gut>

---

## Assets

### Icons — CC BY (Creative Commons Attribution)

The icons in `assets/icon/` (library, vibe, playlist, settings, group, play,
pause, stop, song, artist) are by **Gregor Cresnar**, from
[the Noun Project](https://thenounproject.com).

> [!NOTE]
> CC BY requires attribution **visible to users of the work**, not only in a
> repository file. This is currently only recorded here. An in-app
> About → Licenses screen would satisfy it — see `docs/LICENSING.md`.
