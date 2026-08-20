# Going live

What has to be true before a build of Vapor Music is given to anyone who is not
Dylan. Nothing here blocks building or running it locally — every obligation in
this document triggers on **distribution**.

Written 2026-08-20. Where a number appears it was measured; where a decision
appears it was made deliberately and is recorded as such.

---

## 1. Signing

Neither platform is signed for release today. The dev loop *is* — see
`src-tauri/.cargo/config.toml` — but that is a different problem solved for a
different reason.

### macOS

`tauri build` produces an ad-hoc signed `.app`:

```
Identifier=com.dylangrowcoot.vapormusic
Signature=adhoc
```

Ad-hoc means the signature is a hash of the binary, so **every build is a
different identity**. Locally that is merely annoying. For distribution it does
not work at all: Gatekeeper refuses an unsigned app downloaded from anywhere,
and the user-visible failure is "Vapor Music is damaged and can't be opened",
which is not a message anyone debugs correctly.

Needed, in order:

1. **Apple Developer Program membership** — the only way to obtain a Developer
   ID Application certificate. Paid, annual, and there is no substitute.
2. `signingIdentity` set under `bundle.macOS` in `tauri.conf.json`.
3. **Notarisation.** Signing alone is no longer sufficient; the app must be
   submitted to Apple, stapled, and only then will Gatekeeper open it without a
   right-click dance. Tauri supports this through `APPLE_ID`,
   `APPLE_PASSWORD` and `APPLE_TEAM_ID` in the build environment.
4. **Hardened runtime**, which notarisation requires. Worth testing early: it
   can break audio device access and dynamic loading, and finding that out
   during a release is the wrong time.

### Android

There is no keystore, so builds go out `--debug`. Two consequences:

* The package is `com.dylangrowcoot.vapormusic.debug`, which installs *beside* a
  release build rather than over it.
* The APK carries full native debug symbols: **591 MB** measured 2026-08-20,
  against a small fraction of that for release. It installs to a Pixel 9 over
  wireless ADB in 58 s, so this is survivable for testing and absurd for
  shipping.

Needed:

1. A keystore. **Dylan generates this, because it sets a password.**

   ```bash
   mkdir -p ~/.keys && keytool -genkeypair -v \
     -keystore ~/.keys/vapor-upload.jks \
     -alias vapor -keyalg RSA -keysize 4096 -validity 10000
   ```

2. `gen/android/keystore.properties` holding the path and passwords, **listed in
   `.gitignore` before it is created**, not after.
3. A `signingConfigs` block in `gen/android/app/build.gradle.kts` reading it.

### Where the Android key lives, and how bad losing it is

`~/.keys/vapor-upload.jks`, `chmod 600`, **outside the repository** so it cannot
be committed by accident. The backup that matters is a copy in a password
manager as a file attachment — not iCloud Drive or Dropbox in the clear, and not
the repo even when gitignored.

The stakes depend entirely on how it ships, and this was overstated once already:

| How it ships | Losing the key costs |
|---|---|
| Sideloaded to your own phone | An uninstall and reinstall. App data goes. That is all. |
| Play, **with** Play App Signing | Google holds the app signing key; yours is only an *upload* key and can be reset through support. |
| Play, **without** Play App Signing (legacy) | The app can never be updated again. Fatal. |

**Enrol in Play App Signing if it ever goes to Play.** It is the default for new
apps and it converts the worst case into a support ticket.

---

## 2. Licensing — resolved 2026-08-20

`docs/LICENSING.md` v1.1 concluded AGPL-3.0, correctly, from Essentia (AGPL) and
Rubber Band (GPL). **Neither ships since the Rust rewrite.** The inventory has
been redone against `Cargo.lock` — 620 of 624 packages resolved to their
vendored sources and their declared licences read:

* **No GPL, AGPL or LGPL-only dependency remains.**
* The strongest obligation is **MPL-2.0** — thirteen `symphonia` crates plus
  four Servo-derived CSS crates from Tauri. Weak copyleft at file granularity;
  none of them is modified, so it requires a notice and a source pointer and
  nothing more.
* Everything else is MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0 or
  Unlicense.

`docs/LICENSING.md` v2.0 and `THIRD_PARTY_NOTICES.md` are rebuilt accordingly.

**Direction decided, not yet applied.** Move to **all rights reserved before
anything is distributed**, then choose an open licence later if wanted. The
reasoning is ordering, not ideology: reserved → open is always available,
distributed-under-AGPL → proprietary never is. Nothing forces AGPL now, and a
donation tier is under consideration, so the reversible order is taken first.

The code still says AGPL in **seven places** — `LICENSE`, both `Cargo.toml`s,
`tauri.conf.json`, `README.md`, `Settings.tsx` and `THIRD_PARTY_NOTICES.md` —
and they have to move together, in one commit, or a build ships contradicting
itself. `docs/LICENSING.md` §"The direction" holds the list and the route back.

**Two of those are wrong today regardless of the licence:** `Settings.tsx` tells
users "Tempo and key detection use Essentia" and `README.md` explains the AGPL
as a consequence of Essentia and Rubber Band. Neither has shipped since the Rust
rewrite. The Settings line is user-facing.

**Still outstanding:** CC BY requires attribution *visible to users*, and the
icon credit currently lives only in a repository file. An About → Licences
screen would settle it, and would be the natural home for the MPL notice too.

> This is an engineering summary of publicly declared licence metadata, not
> legal advice. Confirm with a lawyer before commercial distribution.

## 3. Third-party services

The app talks to two strangers, both **off by default**:

* **LRCLIB** — lyrics. Reached from Liner Notes and now Now Playing.
* **Deezer** — artist images and album art.

For a personal build this is a setting. For a distributed one it needs:

* Attribution where those services ask for it.
* A privacy statement, because enabling lookups sends **artist and album names
  off the device**. That is user data leaving the machine, and a store listing
  has to declare it. Android's Data Safety form and Apple's privacy nutrition
  labels both ask directly.
* A check on rate limits and terms of use for automated querying. The analysis
  pass can look up a whole library.

Keeping them off by default is the right shipping posture and should stay.

---

## 4. What has never been exercised

Recorded so nobody assumes coverage that does not exist.

| | State |
|---|---|
| **macOS desktop** | The only genuinely exercised target. |
| **Android** | Compiles, installs, launches. Barely used. Audio path unvalidated on device — TD-24. |
| **iOS** | Never built, never run. `cpal` unvalidated there. TD-24. |
| **Sync between devices** | 46 tests, all in one process. Nothing has crossed a real network. TD-55. |

The sync gap has a specific shape worth restating: both sides are compiled from
the same enum, so the one bug class the tests cannot catch is a **wire-format
mismatch between two versions**. That becomes a live concern the moment two
machines can be on different releases — which is the first day of shipping.

---

## 5. Accepted limitations

Decisions, not oversights. Each is recorded where it was made.

* **LAN sync is unencrypted** (TD-56, decided 2026-08-20). Pairing authenticates
  the device; the transfer is plain TCP. On a network the owner does not
  control, a sync is readable off the wire. **Revisit before giving the app to
  anyone else** — the decision was made for one person on one home network, and
  distribution changes the premise it rests on.
* **Key detection is 60.6% exact / 82.8% harmonically compatible** (TD-11).
  Roughly one transition in six is planned against a key that is not the track's
  key. It rarely sounds broken because the transition chooser masks clashes, but
  it is the ceiling on mix quality. Blocked on TD-43: there is no reproducible
  fixture corpus, so an improvement cannot be told from noise.
* **Downbeat detection (`vapor_dsp::metre`) is deliberately unwired.** It
  measures *below chance* on real music — mean F 0.194 against 0.25 for a coin
  toss — and its confidence score is uncorrelated with correctness, so no
  threshold makes it honest.

---

## 6. Mechanics

* **Version lives in three files** and they are currently all `2.0.0`:
  `vapor-app/package.json`, `vapor-app/src-tauri/tauri.conf.json`,
  `vapor-app/src-tauri/Cargo.toml`. Nothing enforces that they agree. A release
  that disagrees with itself is confusing in a way that surfaces weeks later.
* **There is no updater.** `plugins` in `tauri.conf.json` is empty. Shipping
  without one means every user is on whatever build they installed, for ever.
  Tauri's updater needs a signing key of its own and a place to host a manifest;
  decide before the first release, because retrofitting it means the first cohort
  can never be updated automatically.
* **`[profile.dev.package."*"] opt-level = 2`** exists because unoptimised image
  decoding made thumbnail generation 330 ms per cover. It affects dev builds
  only; release is unaffected.
* **Data on disk**, for anything that has to describe storage use:
  `~/Library/Application Support/com.dylangrowcoot.vapormusic` — `analysis.json`
  (4.5 MB), `covers/` (149 MB full covers plus 3.8 MB thumbnails), `audio/`
  (bounded by the cache setting), `index.json`, `tags.json`. The WebDAV password
  is in the **keychain**, not in any of these.

---

## 7. Before the first build goes out

- [x] ~~Redo the licence inventory against the Rust tree.~~ Done 2026-08-20;
      `LICENSING.md` v2.0 and `THIRD_PARTY_NOTICES.md` rebuilt.
- [ ] Apply the licence direction: all rights reserved across all seven
      declarations, in one commit. See `LICENSING.md` §"The direction".
- [ ] Correct the Essentia claims in `Settings.tsx` and `README.md` — wrong
      today, user-facing, and independent of the licence decision.
- [ ] An About → Licences screen, for the CC BY icons and the MPL notice.
- [ ] Apple Developer ID, `signingIdentity`, notarisation, hardened runtime
      tested against real audio output.
- [ ] Android keystore created, backed up, gitignored; release APK built and its
      size confirmed sane.
- [ ] Play App Signing enrolled, if Play.
- [ ] Privacy declaration covering the lookup services; lookups still off by
      default.
- [ ] Decide on an updater, or decide knowingly to ship without one.
- [ ] Version agreed across all three files.
- [ ] Reconsider TD-56 — the LAN decision was made for a single trusted network.
- [ ] Run on a real iOS device, or state plainly that iOS is unsupported.
- [ ] Two machines on two different builds, syncing, before anyone else has two.
