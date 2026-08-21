# Third-Party Notices

Vapor Music incorporates the following third-party components. Vapor Music
itself is proprietary — see `LICENSE`. These are not: each stays under its own
licence, and this is where the attributions they require are given. The full
licence texts ship in `licenses/`.

---

## Audio

### symphonia — MPL-2.0

Container demuxing and audio decoding: FLAC, MP3, AAC, ALAC, PCM, Vorbis,
ISO-MP4, OGG and RIFF, with metadata. Thirteen crates, all MPL-2.0.

<https://github.com/pdeljanov/Symphonia>

> MPL-2.0 is weak copyleft at file granularity, so the obligation attaches to
> the MPL files themselves rather than to code using them. None is modified
> here; this notice and the source pointer above are what it asks for.

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
permissive: MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0 or Unlicense.
Notable members include `reqwest` and `rustls` (HTTP and TLS), `image` (cover
thumbnails), `serde`, `tokio` and `chrono`.

Their licences require attribution, which this file provides, and nothing else.
The per-package breakdown and the method behind it are in `docs/LICENSING.md`.

---

## Assets

### Fonts — SIL Open Font License 1.1

Vendored as woff2 in `vapor-app/public/fonts/`, bundled rather than fetched
because the app must work offline and its CSP blocks remote origins.

| Font | Copyright | Upstream |
|---|---|---|
| **Inter** | Copyright (c) 2016 The Inter Project Authors | <https://github.com/rsms/inter> |
| **Outfit** | Copyright 2021 The Outfit Project Authors | <https://github.com/Outfitio/Outfit-Fonts> |
| **JetBrains Mono** | Copyright 2020 The JetBrains Mono Project Authors | <https://github.com/JetBrains/JetBrainsMono> |

Licence texts ship in `licenses/Inter-OFL-1.1.txt`, `licenses/Outfit-OFL-1.1.txt`
and `licenses/JetBrainsMono-OFL-1.1.txt`, taken from each project's own
repository rather than transcribed. The OFL's renaming clause is examined in
`docs/LICENSING.md`; none of the three carries a Reserved Font Name.

### Icons — CC BY (Creative Commons Attribution)

The icons are by **Gregor Cresnar**, from
[the Noun Project](https://thenounproject.com). They ship as
`vapor-app/public/icons/` — library, vibe, playlist, settings, group, play,
pause, next, song, artist — masked to `currentColor` rather than drawn as
images, which changes nothing about the attribution owed.
