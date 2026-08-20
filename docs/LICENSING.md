# Vapor Music — Licensing & Compliance

**Version:** 1.1
**Status:** ⚠️ **STALE — describes the Godot build.** See `docs/RELEASE.md` §2.
**Last reviewed:** 2026-07-31

> [!WARNING]
> The inventory below is the **Godot** dependency set. The conclusion — AGPL-3.0
> — follows from Essentia (AGPL) and Rubber Band (GPL-2.0-or-later), and
> **neither ships any more**: analysis is `vapor-dsp` on `symphonia` and
> `rustfft`, and the stretcher is Signalsmith (MIT), which TD-22 chose over
> Rubber Band. The obligation may therefore be materially different from what
> this document concludes.
>
> Whether Vapor Music *should* be AGPL is a separate question from whether it
> *must* be, and the answer here was inherited from a dependency that is gone.
> Redo the inventory against the Rust tree before distributing anything.

> What Vapor Music depends on, what those licenses require, and the options for
> complying. Read alongside `docs/FINDINGS.md`.
>
> This is an engineering summary of publicly documented license terms, not legal
> advice. Confirm with a lawyer before any commercial distribution.

---

## Dependency inventory

| Component | License | Copyleft? | Notes |
|---|---|---|---|
| **Essentia** | **AGPL-3.0** | 🔴 Strong | BPM + key analysis. Commercial license offered by MTG/UPF. |
| **Rubber Band** | **GPL-2.0-or-later** | 🔴 Strong | Time-stretch. Commercial license offered by Breakfast Quay. |
| Godot Engine | MIT | ✅ No | Attribution only. |
| godot-cpp | MIT | ✅ No | Attribution only. |
| GUT | MIT | ✅ No | Test-only, not shipped. |
| Noun Project icons | CC BY | ✅ No | Attribution required — already in `THIRD_PARTY_NOTICES.md`. |
| ffmpeg / taglib / chromaprint | LGPL / GPL / LGPL | 🟡 Varies | Pulled in transitively by Essentia. **Phase 1 of the DSP plan removes these entirely.** |

Verified from `/opt/homebrew/opt/essentia/COPYING.txt` (AGPL v3) and Homebrew
formula metadata (`rubberband: GPL-2.0-or-later`).

---

## Current position

- Source repository is **private** (`github.com/Ghigog/vapor-music`).
- **No `LICENSE` file** exists in the repo.
- `THIRD_PARTY_NOTICES.md` lists **only the icons** — neither Essentia nor
  Rubber Band is mentioned.
- Exported binaries (`.dmg`, `.exe`, `.apk`) link both copyleft libraries.

> [!IMPORTANT]
> **The obligation triggers on distribution, not on use.**
> Building and running Vapor Music on your own machines creates no obligation
> whatsoever. The moment a binary is given to *anyone else* — a friend, a
> tester, a store listing, a download link — the AGPL and GPL terms attach.
>
> Note that the Godot-era macOS `.dmg` shipped without the Essentia and Rubber
> Band dylibs bundled (they resolved to Homebrew paths at runtime), which
> muddied the picture rather than avoiding it. The DSP plan makes the binary
> genuinely self-contained, at which point the obligation is unambiguous.

---

## What the licenses require

Both are strong copyleft. When you distribute a binary that links them:

1. **The whole combined work** must be licensed under the same terms — that
   means Vapor Music's own GDScript and C++, not just the libraries.
2. **Complete corresponding source** must be offered to every recipient, either
   alongside the binary or via a written offer valid for three years.
3. **License texts and copyright notices** must be included with the
   distribution.
4. **Modifications** to the libraries must be marked as such.
5. AGPL-3.0 adds §13: users interacting with the software *over a network* must
   also be offered source. Vapor Music's WebDAV support acts as a **client**, so
   this clause is not triggered. It would be if a hosted or server component
   were ever added.

**License compatibility:** Rubber Band is GPL-2.0-**or-later**, so it can be
taken as GPL-3.0, which is compatible with AGPL-3.0. Combining them is fine —
the combined work is then **AGPL-3.0**. (A GPL-2.0-*only* dependency would have
been incompatible with AGPL-3.0 and unfixable without replacing it.)

---

## 2026-08-15 — the AGPL is no longer forced

The migration removed both copyleft dependencies. `vapor-core` links neither
Essentia nor Rubber Band, and nothing that replaced them is copyleft:

| Dependency | Replaces | Licence |
|---|---|---|
| symphonia | Essentia's loaders, ffmpeg, taglib | MPL-2.0 |
| rustfft | Essentia's FFT | MIT OR Apache-2.0 |
| WSOLA (written here) | Rubber Band | — |
| cpal | Godot's `AudioServer` | Apache-2.0 |
| serde, regex | — | MIT OR Apache-2.0 |

MPL-2.0 is file-level copyleft: modifications to symphonia's own files must be
published, but linking it imposes nothing on the rest of the work. It does not
propagate the way the AGPL does.

**So the AGPL is now a choice rather than an obligation.** It became one the
moment the Rust core stopped linking Essentia — the Godot tree still carries
the old dependencies, so the obligation persists only for as long as that tree
is shipped.

### Recommendation: keep AGPL-3.0

Not because anything requires it, but because it costs nothing and buys
something:

- **It costs nothing.** All copyright is held by one person, so dual-licensing
  or relicensing later is available at any time without collecting agreements
  from contributors. The AGPL binds other people's derivatives, not the
  copyright holder.
- **It matches the product's argument.** An app whose entire pitch is "own your
  music, own your data" is oddly served by a licence that lets someone take it
  closed and host it as a service. AGPL §13 is precisely the clause that stops
  that.
- **It is the reversible direction.** AGPL → MIT can be done later. MIT → AGPL
  cannot, in any meaningful sense: every copy already released stays MIT and
  can be forked from.

### What changes if the licence changes

Only if it moves to a permissive licence:

- `THIRD_PARTY_NOTICES.md` and `licenses/` shrink to the MIT/Apache/MPL set —
  the Essentia and Rubber Band entries go with the Godot tree.
- The in-app About screen's "Free software under the AGPL" line changes.
- MPL-2.0 still requires symphonia's own notices to be kept.

None of that is urgent. The obligation only attaches on distribution, and
nothing new has been distributed.

---

## Options

### ✅ Option A — Release Vapor Music as open source under AGPL-3.0 — **CHOSEN**

**Decision (2026-07-31):** adopted. The vibe features are not reasonably
achievable without Essentia and Rubber Band, and the alternatives either cost
money (Option B) or mean reimplementing beat tracking (Option C). Vapor Music is
therefore AGPL-3.0-or-later.

**Rollout checklist:**

- [x] Add `LICENSE` at repo root containing the full AGPL-3.0 text.
- [x] Expand `THIRD_PARTY_NOTICES.md` to list every row in the inventory above,
      with license names and upstream URLs.
- [x] Vendor the third-party license texts into `licenses/` so they can ship
      with the application.
- [x] State the license and source location in the README.
- [ ] **Make `github.com/Ghigog/vapor-music` public.** While it is private,
      complete source must be handed to every recipient on request.
      *(Owner action — must be done before sharing any build.)*
- [ ] Include `licenses/` in the export. The Android and Windows presets use
      `export_filter="all_resources"` with an empty `include_filter`, so plain
      `.txt` files are **not** packed. Add `licenses/*` to the include filter of
      each preset.
- [ ] Add an in-app **About → Licenses** screen listing dependencies and
      attributions. Required for the CC BY icon attribution, which must be
      visible to users rather than sitting in a repo file.
- [ ] Optional: add `SPDX-License-Identifier: AGPL-3.0-or-later` headers to
      first-party source files.

**Consequence accepted:** anyone may fork, modify and redistribute Vapor Music,
and must in turn keep it AGPL and publish their source.

### Option B — Buy commercial licenses, stay closed source

Both projects dual-license precisely for this:

- **Essentia** — commercial licensing via the Music Technology Group, UPF.
- **Rubber Band** — commercial licensing via Breakfast Quay.

You must hold *both*; either one alone still forces copyleft on the whole work.
Cost is quote-based. Worth pricing if a paid product is the goal.

### Option C — Remove the copyleft dependencies

Highest effort, but leaves you free to license however you like.

- **Time-stretch → solvable today.** `signalsmith-stretch` is **MIT**,
  header-only C++11, and a credible Rubber Band replacement. Phase 2 of the DSP
  plan is already a swap; making it this library instead removes the GPL
  obligation at no extra cost.
- **Analysis → the hard half.** `KeyExtractor` is reimplementable in roughly 200
  lines (chroma + Krumhansl/Temperley profile correlation — the code already
  configures `profileType: "temperley"`). Beat tracking to `RhythmExtractor2013`
  quality is a serious undertaking. Note that **aubio is GPL-3.0**, so it is not
  an escape route.

A realistic middle path: take signalsmith-stretch in phase 2 (removes GPL),
leaving Essentia/AGPL as the only remaining obligation to decide about.

---

## Consequences for the DSP plan

Option A removes the licensing constraint on `docs/FINDINGS.md` —
both libraries may be used freely on every platform.

One choice is still worth making deliberately in **phase 2**: vendoring
`signalsmith-stretch` (MIT) instead of `RubberBandSingle.cpp` costs nothing
extra and would leave Essentia as the only copyleft dependency. That keeps the
door open to relicensing later without a rewrite. Sticking with Rubber Band is
equally valid under AGPL — it is simply a one-way door.

### Store considerations

- **Direct `.dmg` / `.exe` download** — no conflict with either option.
- **Google Play** — GPL/AGPL distribution is workable in practice.
- **Apple App Store** — its terms have historically conflicted with GPL-family
  licenses (the VLC removal being the well-known precedent). Assume Option A is
  incompatible with an App Store release.

---

## References

- [GNU AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
- [GNU GPL-2.0](https://www.gnu.org/licenses/old-licenses/gpl-2.0.html)
- [Essentia licensing / FAQ](https://essentia.upf.edu/FAQ.html)
- [Rubber Band Library](https://breakfastquay.com/rubberband/)
- [signalsmith-stretch (MIT)](https://github.com/Signalsmith-Audio/signalsmith-stretch)
