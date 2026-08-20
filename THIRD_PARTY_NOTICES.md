# Third-Party Notices

Vapor Music incorporates the following third-party components.

> [!NOTE]
> Rebuilt 2026-08-20 against the Rust tree. The previous version listed
> Essentia (AGPL-3.0) and Rubber Band (GPL-2.0-or-later), which the Godot build
> linked and **which no longer ship** — see `docs/LICENSING.md` v2.0. No
> strong-copyleft component remains. Licence texts ship in `licenses/`.

---

## Audio

### symphonia — MPL-2.0

Container demuxing and audio decoding: FLAC, MP3, AAC, ALAC, PCM, Vorbis,
ISO-MP4, OGG and RIFF, with metadata. Thirteen crates, all MPL-2.0.

<https://github.com/pdeljanov/Symphonia>

> MPL-2.0 is weak copyleft at file granularity: the obligation attaches to the
> MPL-licensed files themselves, not to code that uses them. Vapor Music does
> not modify any of them, so the requirement here is this notice and the pointer
> to upstream source above.

### Signalsmith Stretch — MIT

Time-stretching for beat-matched transitions. Three MIT components:

| Part | Copyright |
|---|---|
| `signalsmith-stretch` Rust wrapper | Copyright 2024 Colin Marc |
| `signalsmith-stretch` C++ | Copyright (c) 2022 Geraint Luff / Signalsmith Audio Ltd. |
| `signalsmith-linear` C++ | Copyright (c) 2025 Signalsmith Audio |

<https://github.com/colinmarc/signalsmith-stretch-rs> —
<https://signalsmith-audio.co.uk/code/stretch/>

### cpal — Apache-2.0

Cross-platform audio device I/O. <https://github.com/RustAudio/cpal>

### rustfft — MIT OR Apache-2.0

FFT behind tempo and key analysis. <https://github.com/ejmahler/RustFFT>

### lofty — MIT OR Apache-2.0

Embedded tag and artwork reading. <https://github.com/Serial-ATA/lofty-rs>

### souvlaki — MIT

System media controls. <https://github.com/Sinono3/souvlaki>

---

## Shell and platform

### Tauri — MIT OR Apache-2.0

Application shell and webview host. <https://tauri.app>

Tauri brings in Servo-derived CSS crates that are **MPL-2.0** — `cssparser`,
`cssparser-macros`, `selectors`, `dtoa-short` — under the same file-level terms
described above, and likewise unmodified.

### The wider Rust dependency tree

620 packages resolved from `Cargo.lock` and checked against their vendored
sources on 2026-08-20. Beyond the MPL-2.0 crates named above, every one is
permissive: MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0 or Unlicense, in
that rough order of frequency. Notable members include `reqwest` and `rustls`
(HTTP and TLS), `image` (cover thumbnails), `serde`, `tokio` and `chrono`.

Their licences require attribution, which this file provides, and nothing else.
The full per-package breakdown and the method used to derive it are in
`docs/LICENSING.md`.

---

## Historical — not shipped

The Godot tree in this repository is archived, not built or distributed. It
linked **Essentia** (AGPL-3.0), **Rubber Band** (GPL-2.0-or-later) and their
transitive dependencies (FFmpeg, TagLib, Chromaprint, FFTW3, libsamplerate,
libyaml), and used the **Godot Engine**, **godot-cpp** and **GUT**, all MIT.
Their licence texts remain in `licenses/` for the record. None of them is part
of any binary produced today.

---

## Assets

### Fonts — SIL Open Font License 1.1

Vendored as woff2 in `vapor-app/public/fonts/` and declared in
`vapor-app/public/fonts.css`. Bundled rather than fetched: the app must work
offline and its CSP blocks remote origins. Only the `latin` and `latin-ext`
subsets ship; each is the variable font, covering the whole weight axis in one
file.

| Font | Copyright | Upstream |
|---|---|---|
| **Inter** | Copyright (c) 2016 The Inter Project Authors | <https://github.com/rsms/inter> |
| **Outfit** | Copyright 2021 The Outfit Project Authors | <https://github.com/Outfitio/Outfit-Fonts> |
| **JetBrains Mono** | Copyright 2020 The JetBrains Mono Project Authors | <https://github.com/JetBrains/JetBrainsMono> |

License texts ship in `licenses/Inter-OFL-1.1.txt`, `licenses/Outfit-OFL-1.1.txt`
and `licenses/JetBrainsMono-OFL-1.1.txt`, taken from each project's own
repository rather than transcribed.

> [!NOTE]
> The OFL is permissive for this use — bundling in an application is explicitly
> allowed, it imposes no obligation on Vapor Music's own license, and its
> condition here is simply that the license and copyright notices travel with
> the fonts, which they do.
>
> These files are Google Fonts' subsetted builds rather than the upstream
> originals, so they *are* modified versions in the license's sense. That
> matters only for the renaming clause, and none of the three declares a
> Reserved Font Name — no copyright line carries the "with Reserved Font Name"
> phrase, so the clause has nothing to bite on and the families keep their
> names. Check that again before vendoring a fourth font: it is a per-font
> fact, not a general one.

### Icons — CC BY (Creative Commons Attribution)

The icons are by **Gregor Cresnar**, from
[the Noun Project](https://thenounproject.com). They ship in the Tauri app as
`vapor-app/public/icons/` — library, vibe, playlist, settings, group, play,
pause, next, song, artist — and are masked to `currentColor` rather than drawn
as images, which changes nothing about the attribution owed. The Godot-era
copies under `assets/icon/` are the same set and are not distributed.

> [!NOTE]
> CC BY requires attribution **visible to users of the work**, not only in a
> repository file. This is currently only recorded here. An in-app
> About → Licenses screen would satisfy it — see `docs/LICENSING.md`.
