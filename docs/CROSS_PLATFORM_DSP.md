# Vapor Music — Cross-Platform DSP

**Version:** 1.0
**Status:** Living Document — plan, not yet implemented
**Last reviewed:** 2026-07-31

> How to make the "vibe" features (BPM, musical key, beat grid, time-stretch)
> work on Windows and Android, not just macOS.
> Read alongside `docs/LICENSING.md`, which covers obligations this plan creates.

---

## Current state

The `AudioDSP` GDExtension only exists as a macOS `.dylib`. On every other
platform `ClassDB.class_exists("AudioDSP")` is false and the app falls back to
`scripts/services/audio_dsp_stub.gd`.

The stub deliberately returns **nothing** rather than fabricated values, so
unanalyzed tracks show `—` in the library table and `dj_pathfinder` skips them.
What is lost off-macOS:

| Capability | macOS | Windows / Android |
|---|---|---|
| BPM + beat grid | ✅ Essentia | ❌ unanalyzed |
| Musical key → Camelot | ✅ Essentia | ❌ unanalyzed |
| Beat-matched transitions | ✅ Rubber Band | ❌ passthrough, no sync |
| Cue points / LUFS | ✅ | ❌ (portable code, just not reached) |
| Playback, library, playlists, WebDAV | ✅ | ✅ |

---

## What the vibe features actually need

`src/audio_dsp.cpp` is ~1770 lines, but the entire analysis capability rests on
**two Essentia algorithms**:

| Algorithm | Produces | Call site |
|---|---|---|
| `RhythmExtractor2013` | BPM, beat grid | `audio_dsp.cpp:228` |
| `KeyExtractor` | key + scale → Camelot | `audio_dsp.cpp:101`, `:183` |

Both are **pure DSP** operating on a `std::vector<float>`. Neither requires
ffmpeg, taglib, chromaprint or libsamplerate.

Three further Essentia algorithms — `MonoLoader`, `EasyLoader`, `MetadataReader`
— only decode audio files and read tags. **They are the sole reason** the built
dylib links the entire ffmpeg stack:

```
libessentia → libavformat, libavcodec, libavutil, libswresample,
              libtag, libchromaprint, libsamplerate, libfftw3f, libyaml
```

Replace the loaders and that whole tail disappears.

---

## The real blocker

Not the compiler — the design. `audio_dsp.cpp:1726` and `:1756`:

```cpp
std::string cmd = "\"" + ffmpeg_bin + "\" -y -i \"" + input_path + "\" -ac 1 ...";
int ret = std::system(cmd.c_str());
```

`analyze_file()` shells out to the **ffmpeg and ffprobe command-line binaries**
via `popen`/`system`, with hardcoded Homebrew paths (`/opt/homebrew/bin/...`).

Android has no shell and no ffmpeg binary; this cannot be made to work there at
any price. On Windows it would require ffmpeg on `PATH`. It also means the
current macOS build silently depends on Homebrew at *runtime*, not just at
build time.

Meanwhile `analyze_samples()` (`audio_dsp.cpp:551` → `_analyze_samples_impl` at
`:560`) is already pure portable C++ with **no Essentia calls at all** — it just
only computes `cue_in`, `cue_out` and `lufs` today.

**The portable architecture already exists in the codebase. The analysis path
simply isn't using it.**

---

## Target architecture

Godot decodes → C++ analyses samples. No file I/O in the extension.

```
                 ┌─────────────────────────────────────┐
  audio file ──► │ decode (dr_mp3 / dr_wav / stb_vorbis)│  single-header, 0 deps
                 └───────────────┬─────────────────────┘
                                 │ PackedFloat32Array
                 ┌───────────────▼─────────────────────┐
                 │ analyze_samples_full()              │
                 │   RhythmExtractor2013  → bpm, grid  │  Essentia (lightweight)
                 │   KeyExtractor         → key        │
                 │   existing cue/LUFS code            │  already portable
                 └─────────────────────────────────────┘

  playback ────► RubberBandSingle.cpp / signalsmith-stretch   0 deps
```

### Component decisions

**Time-stretch — solved outright.** Rubber Band ships `RubberBandSingle.cpp`: a
single compilation unit using its built-in FFT and resampler with *no external
dependencies*. One file into `SConstruct` and it builds everywhere, including
macOS — which also removes the Homebrew runtime dependency there.
(See `docs/LICENSING.md` — `signalsmith-stretch` is the MIT alternative if the
GPL obligation is unacceptable.)

**Decoding — single-header libraries.** The app handles MP3, WAV and Ogg Vorbis
only (`audio_manager.gd:2520-2528`). `dr_mp3`, `dr_wav` and `stb_vorbis` are
public-domain, single-file and dependency-free. This deletes ffmpeg, taglib,
chromaprint and libsamplerate in one move.

**Analysis — lightweight Essentia.** Android cross-compilation is officially
documented:

```bash
export PATH=~/Dev/android/toolchain/bin:$PATH
./waf configure --cross-compile-android --lightweight= --fft=KISS --prefix=...
./waf && ./waf install
```

`--lightweight=` strips third-party dependencies to a bare minimum; `--fft=KISS`
uses the bundled KissFFT so nothing external is linked. Windows: MinGW
cross-compilation is the documented route; a community CMake fork exists for
MSVC.

---

## Phased plan

### Phase 1 — Move analysis onto samples *(no new platforms yet)*

1. Add `analyze_samples_full(PackedFloat32Array samples, float rate)` running
   `RhythmExtractor2013` + `KeyExtractor` + the existing cue/LUFS code.
2. Vendor `dr_mp3` / `dr_wav` / `stb_vorbis` into `src/third_party/`.
3. Decode via those instead of `MonoLoader` / `EasyLoader`.
4. Replace `MetadataReader` duration lookups with decoder-reported duration.
5. **Delete** `get_channels_via_ffprobe()` and `downmix_via_ffmpeg()`.
6. Rebuild macOS and diff analysis output against the current implementation for
   a fixed set of tracks — BPM and key must match.

Entirely verifiable on macOS. Removes the ffmpeg/taglib/chromaprint dependency
tail and makes the Mac build self-contained, which independently fixes the
bundling problem (the exported `.dmg` currently only runs on machines with
Homebrew Essentia installed).

### Phase 2 — Dependency-free time-stretch

Swap the Homebrew Rubber Band for `RubberBandSingle.cpp`. Verify transitions
still beat-match on macOS.

### Phase 3 — Windows

Build lightweight Essentia (MinGW or the CMake/MSVC fork), extend `SConstruct`,
restore the `windows.*` entries in `bin/audio_dsp.gdextension`.

### Phase 4 — Android

Cross-compile lightweight Essentia with the NDK, add `android.*` arm64 entries,
build `libaudio_dsp.android.*.arm64.so`.

Phases 1 and 2 carry most of the value at the least risk, and both are testable
without leaving macOS.

---

## Known risks

**Android STL/ABI mismatch.** Essentia issue
[#767](https://github.com/MTG/essentia/issues/767) reports undefined references
from a gnustl-vs-libc++ mismatch, unresolved. Everything must be built against
the NDK's libc++. Budget real time for phase 4.

*Fallback:* `KeyExtractor` is replaceable in roughly 200 lines — a chroma vector
plus Krumhansl/Temperley profile correlation. The code already configures
`profileType: "temperley"`, so the target is well defined. Beat tracking is the
harder half to replace.

**Windows compiler conflict.** Essentia's easy path is MinGW, but the SMTC media
controls in `src/platform/windows/` use C++/WinRT and need MSVC — and
MinGW-built C++ libraries do not link against MSVC. Either use the CMake/MSVC
fork, or ship Windows without media-key integration initially. Note that the
Windows SMTC code has never been compiled on any machine.

**`.gdextension` declarations are load-bearing.** Declaring a platform whose
library file is missing is a *hard export failure* on Windows and a *silent
success shipping no extension* on macOS. Only add entries once the binary
exists. See the header comment in `bin/audio_dsp.gdextension`.

---

## References

- [Essentia — Installing](https://essentia.upf.edu/installing.html)
- [Essentia — FAQ, cross-compiling for Android](https://essentia.upf.edu/FAQ.html)
- [Rubber Band — COMPILING.md](https://github.com/breakfastquay/rubberband/blob/default/COMPILING.md)
- [Essentia CMake/MSVC fork](https://github.com/wo80/essentia/tree/cmake)
- [signalsmith-stretch](https://github.com/Signalsmith-Audio/signalsmith-stretch)
