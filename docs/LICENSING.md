# Vapor Music — Licensing & Compliance

**Version:** 2.1
**Status:** Direction decided (§ The direction) — **not yet applied to the code**
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

## The direction (decided 2026-08-20)

**Move to all rights reserved, before anything is distributed.** Not a rejection
of open source — a decision about *ordering*, taken because only one direction
is reversible.

| From | To | Possible? |
|---|---|---|
| All rights reserved | Any open licence | Always. Nothing granted, nothing to revoke. |
| AGPL, distributed | Proprietary | **No.** Every version already shipped stays free, for ever. |

Nothing forces AGPL any more (see the inventory above), and monetisation is
under consideration. Distributing under AGPL first would close the paid door
permanently while the reverse costs nothing — so the reserved position is taken
first and the open licence stays available whenever it is wanted.

The window for this is **now**, and it closes quietly: the moment a pull request
is accepted under an open licence, that contributor's copyright cannot be
unilaterally relicensed. Today the repository is private, nothing is
distributed, and there are no outside contributors.

> Not legal advice. Confirm with a lawyer before commercial distribution.

### What has to change, when it is done

Seven declarations, and they have to agree or the position is incoherent:

| Where | Currently says |
|---|---|
| `LICENSE` | Full AGPL-3.0 text, 661 lines |
| `vapor-app/src-tauri/tauri.conf.json` | `"copyright": "Vapor Music. Licensed AGPL-3.0-or-later."` |
| `vapor-app/src-tauri/Cargo.toml` | `license = "AGPL-3.0-or-later"` |
| `vapor-core/Cargo.toml` | `license = "AGPL-3.0-or-later"` |
| `README.md` | A licensing section explaining the AGPL consequence |
| `vapor-app/src/screens/Settings.tsx` | User-facing: "free software under the AGPL-3.0" |
| `THIRD_PARTY_NOTICES.md` | Header note |

There is also a stale copy under `.claude/worktrees/`, which is not the working
tree and should be ignored.

> [!IMPORTANT]
> **Two of those are factually wrong today, regardless of the licence.**
> `Settings.tsx` tells users "Tempo and key detection use Essentia", and
> `README.md` says the AGPL follows from linking Essentia and Rubber Band.
> Neither library has shipped since the Rust rewrite. The Settings line is
> user-facing and should be corrected whether or not the licence moves.

---

## The route back to open source

Kept deliberately, so the reserved position reads as a pause rather than a door
that was welded shut.

**Preconditions — decide these first, not during:**

1. **Is there a paid tier, and does it need the licence to hold it up?** If the
   answer is "no, it is a thank-you" — which is the current intent, see below —
   then copyleft costs almost nothing and this route is short.
2. **Which licence.** AGPL-3.0 is the position this project already understood
   and documented; GPL-3.0 is the same without the network clause, which is
   irrelevant to a local app; MIT/Apache-2.0 gives the work away entirely,
   including to anyone who wants to sell it.
3. **Contributors.** Until this is settled, either keep the repository closed to
   outside contributions or take a CLA. A single accepted PR under one licence
   makes the other unavailable without that person's agreement.

**Then, in order:**

1. Confirm the dependency inventory still holds — re-run the method above
   against `Cargo.lock`. A new dependency can reintroduce copyleft silently.
2. Change all seven declarations in one commit, so no build ever ships with them
   disagreeing.
3. Make the repository public, or publish source alongside binaries. AGPL
   obliges complete corresponding source to **every recipient of a binary**, so
   a private repository plus a shared `.dmg` is a breach — this is the specific
   trap the current arrangement would have walked into.
4. Add the About → Licences screen (see below). It is required for CC BY under
   any licence, and it is where the MPL notice belongs too.

---

## Donations and customisation

The intended model, recorded so the licence choice can be checked against it
rather than guessed at later:

**Customisation options sit behind a donation.** They are a *thank-you for
paying*, not a product being sold and not a right being withheld.

**The enforcement is deliberately weak, and that is accepted.** Vapor Music is a
local application with no server, so any "has donated" check runs on the user's
machine and is a boolean somebody can flip. Anyone sufficiently motivated can
compile the features themselves. **That is fine** — the model is honour-system
by design, and treating it as one avoids building DRM into a music player.

What the licence changes here is narrower than it looks:

* Under **all rights reserved**, patching the check is not *licensed*, even
  though it is trivially possible. Redistribution of a patched build is not
  permitted.
* Under **AGPL**, the same patch is explicitly a right, and redistributing the
  result is too. Ardour runs precisely this model on purpose — GPL source, paid
  binaries, free if you compile it yourself — and it works.

Since the paywall is an honour system either way, this is not the argument for
the reserved position. The argument is ordering: reserved first keeps both
models available, and the choice can be made once there is any evidence about
whether people donate.

## History

* **v1.1 (2026-07-31)** — Godot build. Concluded AGPL-3.0 from Essentia and
  Rubber Band. Correct at the time.
* **v2.0 (2026-08-20)** — Rebuilt against the Rust tree from `Cargo.lock`.
  Strong copyleft is gone; MPL-2.0 is the strongest remaining obligation and is
  file-level. AGPL becomes a choice.
* **v2.1 (2026-08-20)** — Direction decided: all rights reserved before any
  distribution, on ordering grounds, with the route back to open source and the
  donation model written down. **Not yet applied** — the seven declarations
  still say AGPL.
