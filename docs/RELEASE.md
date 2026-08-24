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

`bundle.macOS.signingIdentity` is `"Vapor Dev"` — a self-signed certificate in
Dylan's login keychain, set 2026-08-23 so a local release build stops being a
new identity every time. **Both CI workflows override it with
`APPLE_SIGNING_IDENTITY: "-"`**, codesign's own spelling of ad-hoc, because that
certificate exists on one machine. `app.yml` was given the override when the
identity was pinned; `release.yml` was not, and got it on 2026-08-24 — the
workflow that had run proved the override, and the workflow that had never run
would have died at the bundle step on the first tag.

None of that lets anyone else open the app. That is the paid half, above, and it
is still open: the first builds handed out need right-click → Open, once, per
machine.

### Android

There is no keystore, so builds go out `--debug`. Two consequences:

* The package is `com.dylangrowcoot.vapormusic.debug`, which installs *beside* a
  release build rather than over it.
* The APK carries full native debug symbols: **591 MB** measured 2026-08-20,
  against a small fraction of that for release. It installs to a Pixel 9 over
  wireless ADB in 58 s, so this is survivable for testing and absurd for
  shipping.

**Wired 2026-08-24. Two of the three are done; the first is still Dylan's.**

1. A keystore. **Dylan generates this, because it sets a password.** Still open.

   ```bash
   mkdir -p ~/.keys && keytool -genkeypair -v \
     -keystore ~/.keys/vapor-upload.jks \
     -alias vapor -keyalg RSA -keysize 4096 -validity 10000
   ```

2. `gen/android/keystore.properties` holding the path and passwords, **listed in
   `.gitignore` before it is created**, not after. Done — the `.gitignore` entry
   was already there, and the file is now written by CI into the checkout and
   deleted again in the same job, so it never exists on a machine that is not
   building.
3. A `signingConfigs` block reading it. Done — **in
   `gen/android/build.gradle.kts`, the root file, not `app/`**. The Tauri CLI
   rewrites `app/build.gradle.kts` from its template on every
   `tauri android build` (AND-3), so a signing block there would survive until
   the next build and then vanish silently, producing an unsigned APK from a
   green build. The root file is the one the CLI leaves alone.

   It reads `keystore.properties` **only if the file exists**, and configures no
   signing when it does not. A release build on a checkout without the file
   fails saying so, which is the honest failure; a hardcoded fallback is how a
   key nobody chose ends up on an APK somebody installed.

### The four secrets CI needs

Once the keystore exists, `release.yml` wants these on the repository. The
`verify` job checks two of them are set before anything builds, because the
Android job is forty minutes of Rust away from the step that would otherwise
notice.

```bash
base64 -i ~/.keys/vapor-upload.jks | pbcopy   # -> ANDROID_KEYSTORE_BASE64
```

| Secret | What it is |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | the `.jks` itself, base64 — a GitHub secret is a string and a keystore is binary |
| `ANDROID_KEYSTORE_PASSWORD` | the store password set by `keytool` |
| `ANDROID_KEY_ALIAS` | `vapor`, per the command above |
| `ANDROID_KEY_PASSWORD` | the key password; the same as the store password unless `keytool` was told otherwise |

The job writes the keystore to `$RUNNER_TEMP` rather than into the checkout, so
no later step can sweep it into an artefact, and deletes both it and
`keystore.properties` before anything is uploaded anywhere.

### What the release APK actually is

**arm64 only, and release rather than debug.** Every Android build before
2026-08-24 was `--debug`: 591 MB of unstripped Rust debuginfo, installing as
`com.dylangrowcoot.vapormusic.debug` beside a real build rather than over it.
Neither is something to hand to somebody else.

armv7 and x86_64 are not built. Every phone that would run this shipped with an
arm64 userspace; armv7 roughly doubles the Rust half of the job for devices
nobody testing this has, and x86_64 is emulators. Adding one is one word in the
`--target` list in `release.yml`.

**The release build type minifies, and nothing has ever exercised that.** R8
does not run on a debug build, so the first signed APK is also the first time
anything here has been shrunk and renamed. `MainActivity` and `PlaybackService`
are reached from Rust over JNI by string, and R8 keeps a class named in the
manifest while remaining free to rename its methods — which is AND-4 again,
where the app starts, renders, and has no sound. `app/proguard-rules.pro` now
keeps `com.dylangrowcoot.vapormusic.**` whole and every `native` method name.
**This is reasoned, not measured**: the first APK is the measurement.

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

### The updater key

Separate from both of the above, and generated 2026-08-21:

| | |
|---|---|
| Private key | `~/.tauri/vapor-music-updater.key`, `chmod 600`, **outside the repository** |
| Password | none — one fewer secret to hold in CI, and the file never leaves the machine or GitHub's secret store |
| Public key | compiled into every desktop binary via `plugins.updater.pubkey` in `tauri.conf.json` |

**Back it up the same way as the Android key** — a file attachment in a password
manager. Losing it is the fatal kind: the public key is already inside every
build that has gone out, and only the matching private key can sign an update
those builds will accept. A new keypair means everyone reinstalls by hand.

**The pair in `tauri.conf.json` is the wrong one, and this blocks the first
tag.** On 2026-08-22 a session printed the private key into a transcript and
rotated it; `~/.tauri/` holds the new pair and the old one beside it, suffixed
`.COMPROMISED-2026-08-22`. `plugins.updater.pubkey` still carries the **old
public half**. A build signed with the live private key therefore produces a
signature the shipped app refuses — which presents as a broken updater rather
than as a wrong key, and cannot be fixed afterwards by shipping an update,
because the public key is compiled in.

`release.yml`'s `verify` job now fails on exactly this: it compares
`plugins.updater.pubkey` against the rotated string and refuses to build. What
closes it, in one sitting and without the private half ever reaching a
terminal's scrollback:

```bash
npx tauri signer generate -w ~/.tauri/vapor-music-updater.key
```

Then paste the **public** half it prints into `plugins.updater.pubkey` in
`vapor-app/src-tauri/tauri.conf.json`, and put the **contents of the key file**
into the `TAURI_SIGNING_PRIVATE_KEY` repository secret — copied from the file,
not echoed. Back the file up as a password-manager attachment in the same
sitting: the public half is compiled into every copy handed out, so a lost
private half means everyone reinstalls by hand.

To release, CI needs the private key as a repository secret. The two names are
Tauri's own, read by `tauri build` without further configuration:

```
TAURI_SIGNING_PRIVATE_KEY           the contents of the key file
TAURI_SIGNING_PRIVATE_KEY_PASSWORD  empty string
```

`bundle.createUpdaterArtifacts` is on, so `tauri build` emits a `.sig` beside
each bundle. The manifest the app actually reads is `latest.json`, listing a
version, notes, a date and a `{ signature, url }` per platform; it has to be
published at the endpoint below, next to the bundles.

`.github/workflows/release.yml` generates and publishes it, via
`tauri-action`'s `includeUpdaterJson`. It runs on a `v*.*.*` tag: `verify`
checks that the three version declarations agree with the tag, that the pubkey
is not the rotated one, and that both signing secrets are set; **four** targets
build into one draft release — macOS universal, Linux x86_64 and Windows x86_64
from the `tauri-action` matrix, and an Android arm64 APK from a job of its own
— then `finalise` attaches `SHA256SUMS`, publishes the draft, and fetches the
endpoint to report what the updater will now answer.

Android is a separate job rather than a matrix row because `tauri-action` wraps
`tauri build`, and the mobile bundle is a different CLI verb with a Gradle
project under it. It waits for the desktop matrix so that the draft it uploads
into already exists.

**The version gate accepts a prerelease suffix** — `v2.0.0-rc.1` is checked
against `2.0.0` and has to agree on the number. The three files carry a
version, not a channel, and the first tag is meant to be an `-rc`. The draft is deliberate — a release
that goes public mid-matrix offers a download that does not exist yet for
somebody's machine.

Its actions are pinned by commit SHA rather than tag, which no other workflow
here does. It is the only one holding the signing key at the same time as
`contents: write`, and a moveable tag is the supply-chain path that matters
once both are in one job.

### The endpoint is baked in, and points somewhere that does not exist yet

```
https://github.com/Ghigog/vapor-music/releases/latest/download/latest.json
```

Compiled into the binary, so it cannot be changed by shipping an update — only
by handing out a new build.

**The repository is public**, confirmed 2026-08-21 with `gh repo view`, which
settles the first half: release assets on a private repository need an
authenticated request and the updater sends none. Public also buys free Actions
minutes and free asset hosting, which is the whole reason v1 costs nothing to
ship. Proprietary and public is a normal combination; public does not mean
open-source, and `LICENSE` still reserves all rights.

Since it is already public, `docs/LICENSING.md`'s precondition applies now
rather than later: settle contributions — disable pull requests or require a
CLA — because one accepted outside contribution freezes the licence choice.

What is left is a release with `latest.json` attached. Until one exists the URL
returns 404, the check fails, and the app logs one line and carries on. That is
the intended state, not a fault.

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

**Applied 2026-08-20.** Vapor Music is **all rights reserved**; an open licence
stays available and the route to one is in `LICENSING.md`. The
reasoning is ordering, not ideology: reserved → open is always available,
distributed-under-AGPL → proprietary never is. Nothing forces AGPL now, and a
donation tier is under consideration, so the reversible order is taken first.

All seven declarations moved in one commit, so no build ships contradicting
itself. Two of them were also factually wrong — Settings and the README both
credited Essentia for analysis, which has not shipped since the Rust rewrite —
and were corrected at the same time.

**Settled 2026-08-20.** CC BY requires attribution *visible to users*, which a
repository file does not provide. Settings → About → Licences now renders
`THIRD_PARTY_NOTICES.md` inside the app — the file itself, not a second copy of
it — which covers the icons and carries the MPL notice for Symphonia too.

> This is an engineering summary of publicly declared licence metadata, not
> legal advice. Confirm with a lawyer before commercial distribution.

## 3. Third-party services

The app talks to **three** strangers. Two are off by default; the third is not,
and this section said there were two until 2026-08-23, when the AUD-16 privacy
pass counted them against the code.

* **LRCLIB** — lyrics. Reached from Liner Notes and now Now Playing.
  Off by default.
* **Deezer** — artist images and album art. Off by default.
* **GitHub** — the updater. **On, on every desktop launch, with no setting to
  turn it off.** `lib.rs` spawns the check in `setup` without consulting
  anything, fetches `releases/latest/download/latest.json`, and silently
  downloads and installs a signed newer release to run from the next launch.
  Android has no updater at all.

The third one is defensible and should stay — it is how a build in a stranger's
hands ever gets patched, and the endpoint and public key are compiled in, so it
is the one feature that cannot be added later by shipping an update. What is
not defensible is leaving it out of the count. GitHub sees an IP address and a
request; nothing about the person or their library goes with it. `PRIVACY.md`
§4 states it in the terms a reader needs.

For a personal build the first two are a setting. For a distributed one they
need:

* Attribution where those services ask for it.
* A privacy statement, because enabling lookups sends **artist and album names
  off the device**. That is user data leaving the machine, and a store listing
  has to declare it. Android's Data Safety form and Apple's privacy nutrition
  labels both ask directly. `PRIVACY.md` is that statement, written 2026-08-23.
* A check on rate limits and terms of use for automated querying. The analysis
  pass can look up a whole library. AUD-18 landed the identification and the
  pacing — a `User-Agent` on every request and a floor between them — and left
  the terms question open, because registering with Deezer or moving to
  MusicBrainz is a decision rather than work.

Keeping the first two off by default is the right shipping posture and should
stay.

---

## 4. Where the coverage stops

Recorded so nobody assumes coverage that does not exist.

| | State |
|---|---|
| **macOS desktop** | The only genuinely exercised target. |
| **Android** | Compiles, installs, launches. Barely used. Audio path unvalidated on device — TD-24. |
| **iOS** | Never built, never run. `cpal` unvalidated there. TD-24. **Not in the first release** — decided 2026-08-24. There is no `gen/ios`, and the blocker is not the code: no route exists to put an iOS build on somebody else's phone without a paid Apple Developer account. Ad-hoc provisioning needs each device's UDID, TestFlight needs the same membership, and an unsigned `.ipa` needs every recipient to re-sign it with their own Apple ID every seven days. |
| **Sync between devices** | Exercised only in-process. Nothing has crossed a real network. TD-55. |

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

* **Version lives in three files** which must agree:
  `vapor-app/package.json`, `vapor-app/src-tauri/tauri.conf.json`,
  `vapor-app/src-tauri/Cargo.toml`. Nothing enforces that they agree. A release
  that disagrees with itself is confusing in a way that surfaces weeks later.
* **The updater is configured, and silent.** Desktop only —
  `tauri-plugin-updater` is not built for Android or iOS, which update through
  their stores. It checks once at launch on a background task and installs what
  it finds; the new version takes over at the next launch, so nothing restarts
  underneath anyone mid-track. There is no update UI, which is deliberate: a
  check that only reported to a screen nobody has built would never install
  anything. Every failure is logged with an `updater:` prefix and swallowed.
* **The release half is built and has never run.** `release.yml` produces the
  bundles, the `.sig` files and the `latest.json` the app goes looking for, on a
  `v*.*.*` tag. Zero runs as of 2026-08-24 — this bullet said "has not been
  built" until the workflow landed on 2026-08-22 and was still saying it two
  days later, which is the shape of staleness worth watching for in this
  document. The updater stays inert until the first tag publishes, which is
  fine, and is why the config went in first: the key and the endpoint are
  compiled into the binary and cannot be added to a build already handed out.
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
- [x] ~~Apply the licence direction across all seven declarations.~~ Done
      2026-08-20; Vapor Music is proprietary, see `LICENSE`.
- [x] ~~Correct the Essentia claims in `Settings.tsx` and `README.md`.~~ Done.
- [x] ~~An About → Licences screen, for the CC BY icons and the MPL notice.~~
      Done 2026-08-20.
- [ ] Apple Developer ID, notarisation, hardened runtime tested against real
      audio output. `signingIdentity` itself is done — `"Vapor Dev"` locally,
      ad-hoc on both CI workflows. The paid half is what is left, and the first
      builds go out needing right-click → Open.
- [ ] **Android keystore created and backed up.** Dylan's, and the last thing
      between here and an installable APK — `keytool` command and the four
      repository secrets in §1. Gitignored: done. `signingConfigs`: done, in the
      root `build.gradle.kts`. Size confirmed sane: waiting on the first build.
- [ ] **A fresh updater keypair, and the public half into `tauri.conf.json`.**
      Also Dylan's, also blocking: `verify` refuses to build while the config
      carries the key rotated on 2026-08-22. See §1.
- [ ] **Updater key backed up** — `~/.tauri/vapor-music-updater.key` copied into
      a password manager as a file attachment. Costs nothing to lose before the
      first build goes out and cannot be replaced after it, because the matching
      public key is compiled into every copy already handed over. Dylan's to do;
      no session can do it for him. See §1.
- [ ] Play App Signing enrolled, if Play.
- [ ] Privacy declaration covering the lookup services; lookups still off by
      default.
- [x] ~~Decide on an updater, or decide knowingly to ship without one.~~ Done
      2026-08-21: configured, desktop only, silent. See §1.
- [x] ~~Make the repository public, so the updater endpoint and free CI both
      work.~~ Already public — confirmed 2026-08-21.
- [x] ~~Settle contributions now the repository is public: disable pull requests
      or require a CLA, before anyone opens one.~~ Done 2026-08-23: pull
      requests from anyone but the owner are closed automatically by
      `.github/workflows/no-outside-prs.yml`, with `CONTRIBUTING.md` beside it.
      GitHub has no setting for this; interaction limits expire after six
      months. See `docs/LICENSING.md` for why it mattered.
- [ ] **Update the supporter count** in `vapor-app/src/lib/supporters.ts` from
      the Ko-fi dashboard, and set `KOFI_HANDLE` if it is still empty — the
      support card does not render at all without it. The number is written by
      hand on purpose (there is no server, and Ko-fi has no read API), so this
      is the only moment it moves. Being a little behind is fine and is the
      only way it can be wrong; claiming support that did not arrive is not.
- [x] ~~A release workflow that builds on a tag and publishes `latest.json`
      alongside the bundles.~~ Done 2026-08-22: `.github/workflows/release.yml`.
      See §1. The first tag should be an `-rc`: a mistake then costs a draft
      rather than the release `releases/latest` resolves to, which is the URL
      every shipped binary has baked in.
- [x] ~~Version agreed across all three files.~~ 2.0.0 in `package.json`,
      `tauri.conf.json` and `Cargo.toml`; the `verify` job now gates it against
      the tag on every release.
- [ ] Reconsider TD-56 — the LAN decision was made for a single trusted network.
- [x] ~~Run on a real iOS device, or state plainly that iOS is unsupported.~~
      Stated plainly 2026-08-24: **iOS is not in the first release.** See §4.
      This is the "state plainly" half, not the "run it" half — the run is still
      worth doing and still has nothing scheduled.
- [ ] Two machines on two different builds, syncing, before anyone else has two.
