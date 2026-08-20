# Vapor Music — Licensing & Compliance

**Version:** 2.0
**Status:** Rebuilt against the Rust tree
**Last reviewed:** 2026-08-20

> What Vapor Music depends on, what those licences require, and what is
> therefore a decision rather than an obligation.
>
> This is an engineering summary of publicly declared licence metadata, not
> legal advice. Confirm with a lawyer before commercial distribution.

---

## What changed, and why version 1.1 was wrong

Version 1.1 concluded **AGPL-3.0** and it followed correctly from its premises:
the Godot build linked **Essentia** (AGPL-3.0) and **Rubber Band**
(GPL-2.0-or-later), both strong copyleft, so the combined work had to match.

Neither ships any more.

* **Essentia is gone.** Analysis is `vapor-dsp`, built on `symphonia` and
  `rustfft`.
* **Rubber Band was rejected** in favour of Signalsmith Stretch (MIT). TD-22
  records that it was rejected on its build system, not its sound — the licence
  was explicitly *not* the objection at the time. It is a consequence anyway.

The premises are gone; the conclusion no longer follows from them. AGPL may
still be the right answer, but as of this version it is a **choice**.

---

## Method

Not from memory. Every package in `vapor-app/src-tauri/Cargo.lock` was resolved
to its vendored source in `~/.cargo/registry/src` and its declared `license`
field read:

```
620 of 624 packages resolved   (the 4 unresolved are this repo's own crates)
```

Reproduce with the script in `docs/workspace/` or re-derive it — the point is
that the table below is measured rather than recalled.

---

## The inventory

| Licence | Packages | Copyleft? |
|---|---|---|
| MIT OR Apache-2.0 *(and spelling variants)* | 481 | ✅ No |
| MIT | 122 | ✅ No |
| **MPL-2.0** | **18** | 🟡 **File-level, weak** |
| Unicode-3.0 | 18 | ✅ No — attribution |
| Zlib OR Apache-2.0 OR MIT | 17 | ✅ No |
| Unlicense OR MIT | 10 | ✅ No |
| Apache-2.0 | 6 | ✅ No |
| BSD-3-Clause | 5 | ✅ No |

**There is no GPL, AGPL, or LGPL-only dependency in the shipped tree.**

The one apparent exception is `r-efi`, offered as
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` — a disjunction, so MIT can simply be
taken. It is a UEFI shim pulled in transitively and is not linked on macOS or
Android regardless.

### The MPL-2.0 set

Thirteen `symphonia` crates (decoding: FLAC, MP3, AAC, ALAC, PCM, Vorbis,
ISO-MP4, OGG, RIFF, metadata), plus `cssparser`, `cssparser-macros`,
`selectors` and `dtoa-short` — Servo-derived CSS handling, pulled in by Tauri —
and `option-ext`.

MPL-2.0 is **weak copyleft at file granularity**. The obligation is on the MPL
files themselves: if you modify one, that file's source must be made available
under MPL. It does **not** reach into code that merely uses the library, and it
does not dictate the licence of the combined work.

Vapor Music does not modify any of them. The practical requirement is therefore
attribution and a pointer to the upstream source.

### Notable direct dependencies

| Component | Licence | Note |
|---|---|---|
| **symphonia** | MPL-2.0 | Decoding. The only meaningful copyleft in the tree. |
| **signalsmith-stretch** | MIT | Rust wrapper (Colin Marc), bundling `signalsmith-stretch` (MIT, Geraint Luff / Signalsmith Audio) and `signalsmith-linear` (MIT, Signalsmith Audio). All three MIT. |
| **rustfft** | MIT OR Apache-2.0 | Analysis. |
| **cpal** | Apache-2.0 | Audio device I/O. |
| **tauri** | MIT OR Apache-2.0 | Shell. Drags in the MPL CSS crates. |
| **lofty** | MIT OR Apache-2.0 | Tag reading. |
| **souvlaki** | MIT | System media controls. |
| **reqwest / rustls** | MIT OR Apache-2.0 | HTTP and TLS. |
| **image** | MIT OR Apache-2.0 | Cover thumbnails. |
| Noun Project icons | CC BY | Attribution required. Already in `THIRD_PARTY_NOTICES.md`. |

---

## What this means

**The obligation triggers on distribution, not on use.** Building and running on
your own machines creates none. The moment a binary reaches anyone else —
a friend, a tester, a store listing, a download link — the terms attach.

On distribution, the shipped tree requires:

1. **Attribution** for MIT, Apache-2.0, BSD, Zlib, Unicode-3.0 and CC BY
   components — the licence text and copyright notices, which is what
   `THIRD_PARTY_NOTICES.md` is for.
2. **MPL-2.0 notice** for the symphonia and CSS crates: state that they are
   MPL-2.0 and where the source can be obtained. Nothing further, because
   nothing in them is modified.
3. **Nothing at all** that constrains the licence of Vapor Music's own code.

---

## The open decision

`LICENSE` in the repo root is AGPL-3.0, and `tauri.conf.json` declares
`"copyright": "Vapor Music. Licensed AGPL-3.0-or-later."`. Both are still the
stated position. Neither is now forced by a dependency.

**Staying AGPL-3.0** is a coherent choice — it is what the project has said
publicly, it needs no action, and it keeps the work copyleft on purpose rather
than by accident. The cost is real and worth stating: AGPL obliges you to offer
complete corresponding source to **every recipient of a binary**, and the
repository is currently private. Handing a `.dmg` to one friend triggers that.

**Relicensing** to something permissive, or to a weaker copyleft, is equally
available now that nothing compels AGPL. It is a decision with consequences that
are hard to reverse — code released under a permissive licence cannot be
recalled — and it is Dylan's to make, not one to be inherited from a dependency
that was removed a phase ago.

**Either way, the four places that state a licence must agree**: `LICENSE`,
`tauri.conf.json`'s copyright string, `THIRD_PARTY_NOTICES.md`, and this
document. They currently do.

---

## History

* **v1.1 (2026-07-31)** — Godot build. Concluded AGPL-3.0 from Essentia and
  Rubber Band. Correct at the time.
* **v2.0 (2026-08-20)** — Rebuilt against the Rust tree from `Cargo.lock`.
  Strong copyleft is gone; MPL-2.0 is the strongest remaining obligation and is
  file-level. AGPL becomes a choice.
