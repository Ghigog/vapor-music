# REL — the release epic

Everything between here and a build a stranger can download and run. Opened
2026-08-23 because these items block each other in an order that is not
visible when they sit in `tickets.md` as eight unrelated rows.

`docs/RELEASE.md` is the reference — signing, licensing, third-party services,
accepted limitations, mechanics. This file is the *state*: what is done, what
blocks what, and which items are waiting on Dylan rather than on work.

## The 2026-08-24 call: v1 ships without onboarding

Dylan, 2026-08-24: **cut a build now**, for himself and a handful of friends to
install and use. Onboarding does not hold it.

That changes one thing and only one thing about this file — the order. Every
item below is where it was; **"What is being built first" is no longer what the
release waits on.** Onboarding (AUD-23) moves *into* this epic as the content of
the release after, and the two releases are:

| | Tag | Contains |
|---|---|---|
| **v1**, as Dylan says it | `v2.0.0-rc.1` then `v2.0.0` | Whatever exists on 2026-08-24. Donation is in; onboarding is not. |
| **v1.1**, as Dylan says it | `v2.1.0` | Onboarding — AUD-23, unchanged in scope. Plus whatever the first build's testers find. |

**Why the tags say 2 and Dylan says 1.** `v1.0.0` and `v1.1.0` are taken: they
are the Godot releases from June, and `V1.78` is the last of them. The Rust
rewrite is `2.0.0` in all three version files and has been since before this
epic opened. So the numbers on the tags are the numbers the updater compares
and the numbers GitHub sorts by, and "v1" is what the thing is called. Both are
right and they are not the same sentence — written down here because the first
person to read `2.0.0` on a release page and `v1` in a message will otherwise
assume one of them is a mistake.

**Scope of this first build.** macOS, Windows, Linux, Android. **iOS is not in
it** — decided 2026-08-24, and it is a real decision rather than an oversight:
there is no `gen/ios` in the tree, no iOS build has ever been attempted, `cpal`
is unvalidated there (TD-24), and none of that is the blocker. The blocker is
that no route exists to put an iOS build on somebody else's phone without a
paid Apple Developer account — ad-hoc provisioning needs each device's UDID,
TestFlight needs the same membership, and an unsigned `.ipa` needs every friend
to re-sign it with their own Apple ID every seven days. It stays "wanted, not
committed", exactly as it was.

## The dependency chain

The only hard chain is the one that ends in a signed artefact:

```
keys exist  ->  builds are signed  ->  the pipeline runs on a tag
```

`release.yml`'s `verify` job fails without the signing secret, so the tag step
cannot be exercised at all until the keys are answered. Nothing else in this
epic is blocked by anything else in it — the rest are independent, and several
are already done.

**Those first two steps stopped being tickets on 2026-08-23**, at Dylan's
direction: signing is not a defect somebody forgot to fix, it is the last part
of shipping, and a row on the board that can only be closed at the very end
reads as neglect every time anyone scans the list. They are steps 1 and 2 of
the pipeline below. AUD-21 and REL-001 point here.

## The release pipeline, in order

Nothing here starts until the feature work does — see "What is being built
first" at the end of this file.

**1. Keys.** Both platforms, and the decision is where they live rather than
how to make them.

*macOS — the free half is done (2026-08-23).* `tauri build` produced an ad-hoc
signature, which is a hash of the binary, so every release build was a new
identity and a keychain grant did not survive one.
`bundle.macOS.signingIdentity` is now `"Vapor Dev"`, the same self-signed
certificate `src-tauri/.cargo/config.toml` already uses for the dev loop, so a
local release build stops changing identity every time. CI passes
`APPLE_SIGNING_IDENTITY: "-"` — the CLI reads that and it wins over the config
— because the certificate lives in one login keychain and a runner would fail
looking for it; `-` is codesign's own spelling of ad-hoc, which is what those
builds were doing anyway.

**Verified by a real bundle, 2026-08-23.** It went in unverified — a release
build needs disk this machine did not have at the time — and the `App` workflow
proved it on the next push: `installers (macos-latest)` built the bundle green
with the override in place. Recorded because the caveat was written down first
and a caveat nobody closes is worse than one nobody wrote.

*macOS — the paid half, still a decision.* None of the above lets anybody else
open the app: Gatekeeper refuses a self-signed identity on a machine that does
not trust it. Handing a build to another person needs a **paid Apple Developer
account and notarisation**, and that is a cost decision rather than a task. It
is the only part of macOS signing that is still open.

*Android.* No keystore, so builds go out `--debug`. Two consequences: the
package is `com.dylangrowcoot.vapormusic.debug`, which installs *beside* a
release build rather than replacing it, and the APK carries full native debug
symbols — **591 MB**, measured 2026-08-20. It installs to a Pixel 9 over
wireless ADB in 58 s, so the size is survivable while the builds are for
Dylan's own device, which is what they are today. The work is `keytool
-genkeypair`, a `keystore.properties` kept out of git, and a `signingConfigs`
block in `gen/android/app/build.gradle.kts`. **A lost Android upload key cannot
be replaced for an app already on Play**, which is the whole reason "where does
it live" is the question rather than "how do I make one".

*The keypair that already exists is wrong.* **Closed 2026-08-24 — see below.**
On 2026-08-22 a session printed the updater private key into a transcript and
rotated it. `~/.tauri/` holds a new keypair and the old one beside it, suffixed
`.COMPROMISED-2026-08-22`. **`tauri.conf.json` still carries the old public
key**, so the config trusts a key whose private half is the compromised file,
and a build signed with the new key would produce a signature the app refuses.

**What it actually took: one line of config.** The framing above had everyone
expecting a key ceremony, and the fix was to read the public half off the `.pub`
file the rotation already wrote and paste it in — `F35ECFB640B295DF` out,
`E17781A5EB1BC8D6` in. Nothing was generated, and the private key never went
near a terminal. Worth noticing that "the keypair is wrong" was never true: the
keypair was fine, the *config* was stale, and eight lines of this epic described
it as the former. Nothing depends on it until a
release is signed — but the first signed build fails on it, and the standing
instruction holds until then: no private key is saved or managed while nothing
ships, because every handling of it is a chance to leak it. When distribution is
real: generate fresh, public half into `tauri.conf.json`, private half straight
into a password manager, never read back into a terminal.

**2. Signed builds.** Wiring the above into `tauri.conf.json` and the Gradle
config. Mechanical once step 1 is answered.

**3. The first tag.** An `-rc`, per `docs/RELEASE.md`, so a mistake costs a
draft rather than the release that `releases/latest` resolves to — which is the
URL compiled into every binary. This is also the first time `release.yml` runs
at all.

## Status

| Item | State | Waiting on |
|---|---|---|
| **AUD-20** supply chain | **done** — 23 actions SHA-pinned, Dependabot, `cargo deny` gate in CI | — |
| **AUD-19** Windows CI | **done** — all eight jobs green, NSIS installer builds | — |
| `release.yml` dead pins | **done** — two of five pinned SHAs did not exist upstream and would have failed on the first tag push | — |
| **AUD-16** privacy / EULA / support | **partly done** — `PRIVACY.md` and `SUPPORT.md` written from the code, `docs/EULA-NOTES.md` states the gap without pretending to close it | a contact address, and a lawyer before anything is sold |
| **AUD-18** Deezer terms | **half done** — every request identified by `User-Agent`, three per-service clocks, four attempts with backoff | Dylan: Deezer or MusicBrainz. The calls are polite either way |
| **AUD-3** what a supporter gets | **built** — a pin per donation at the bottom of Settings, count written in by hand from Ko-fi | A Ko-fi handle. The card does not render without one |
| **AUD-23** the front door | **moved to v2.1.0** (2026-08-24) — as onboarding, not marketing. Unchanged in scope; it is simply not what v1 waits on | Nothing |
| **AUD-21** the updater keypair | **done 2026-08-24** — `tauri.conf.json` now carries `E17781A5EB1BC8D6`, the public half of the pair rotated on 2026-08-22. No new key was needed; the private half was on disk all along | Dylan: the key file's contents into `TAURI_SIGNING_PRIVATE_KEY`, and a backup |
| **REL-001** release signing | **wired 2026-08-24** — Android `signingConfigs` in `gen/android/build.gradle.kts`, four secrets read by `release.yml` | Dylan: `keytool -genkeypair`, then the four repository secrets |
| **AUD-22** first release | **done 2026-08-25** — `v2.0.0-rc.7` published all 4 platforms (macOS, Windows, Linux, Android) with signed bundles, APK, SHA256SUMS and updater manifest | — |

## What is actually decided

- **Direct download for v1**, itch.io possible later. Not the stores.
- **Ship with the updater**, which is why the keypair matters and why the
  public key in `tauri.conf.json` currently disagreeing with the private one is
  a real problem rather than a tidy-up.
- **macOS, Android, Windows, Linux**; iOS wanted, not committed.
- **Donations carry nothing contingent.** Four desks independently rejected
  money-unlocks-features: it is consideration, which buys the whole legal and
  tax profile of a paid app while capping revenue at donation levels.
- **First tag is an `-rc`**, per `docs/RELEASE.md`, so a mistake costs a draft
  rather than the release that `releases/latest` resolves to — which is the URL
  compiled into every binary.

## What is being built first

**Superseded 2026-08-24 — see "The 2026-08-24 call" at the top.** Donation
landed; onboarding is now the content of v2.1.0 rather than a thing v1 waits
for. The two paragraphs below are kept because the *reasoning* about what each
one is still holds, and AUD-23 is written against it.

**Donation, then onboarding.** Decided 2026-08-23. Neither is a release
mechanic; both are the product, and the release steps above wait for them.

**Donation.** The logic first, the thank-you later. The rule from
`DECISIONS.md` §2 is unchanged and is what keeps it a gift: *nothing contingent
on payment*. Dylan's sketch for the thank-you is a pin at the bottom of
Settings, one added per donation — no feature behind it, nothing withheld,
just a visible record that somebody contributed. Worth being deliberate that
the pin is a receipt and not a good: it is awarded *because* money arrived,
which is the same sentence as a purchase, and what keeps it on the right side
of the line is that it does nothing and gates nothing.

**Onboarding.** Not a landing page and no theatrics. A small onboarding window
that appears as a person moves around the app, and **no opening "connect your
music" modal** — send them to Settings and to the connection settings with a
help modal instead. See AUD-23 for what this replaces: first run currently
*does* open with a two-path chooser, so this is a change of an existing flow,
not a filling of a gap.

## Queued, and deliberately not now

Both surfaced 2026-08-23 while pushing. Neither is urgent, both are real, and
the order between them matters — so they are written down rather than done,
because the feature work is what stands between here and a release and these
are not.

### The licence gate covers half the tree

`ci.yml` runs `cargo deny ... check licenses advisories` on `vapor-core`, and
only `check advisories` on `vapor-app/src-tauri`. The app tree is where reqwest,
tauri and the overwhelming majority of the ~627 crates live, so **the gate
covers the small half and misses the big one.**

That matters because AUD-20 is recorded as closed with "cargo deny gate in CI",
and `docs/LICENSING.md` describes its inventory as being behind a gate. The
claim is currently broader than the truth. The risk it is supposed to catch is
the one that document names in as many words — a transitive dependency
reintroducing copyleft into a proprietary app — and that is exactly the half not
being checked.

Running the stricter command on the app tree today fails on one thing:
`webpki-roots` and `webpki-root-certs` v1.0.9 declare **CDLA-Permissive-2.0**,
reached through `reqwest -> hyper-rustls`. That is the Community Data License
Agreement, permissive, and it is the licence on Mozilla's CA certificate data —
so this is almost certainly a line to add to `deny.toml`'s allow list with a
note, not a problem. It has been in the lockfile since before 2026-08-23 and
nothing has ever objected, because nothing looks.

**The work:** change the one command, allow the licence with its reasoning,
re-count `docs/LICENSING.md`. Twenty minutes.

**Do it before the dependency bumps below**, not after. A bump is precisely how
a new licence arrives, and a gate that goes in afterwards has already missed the
thing it was for.

### Thirteen Dependabot pull requests, none of them urgent

Open as of 2026-08-23, and exempt from the PR closer (see AUD-17) because
watching dependencies is deliberate.

**Nothing is on a clock.** None carries a `security` label — all are plain
version bumps. `npm audit` reports 0 vulnerabilities across prod and dev, and
`cargo deny check advisories` is clean on both trees. Checked rather than
assumed, because "there are thirteen open PRs" reads as pressure and is not.

**Not as a batch, and not before real hardware.** The risk sits exactly where
the product's quality does:

* **cpal 0.15 -> 0.18** (`vapor-core`, PR 1) and **0.15 -> 0.17**
  (`vapor-app/src-tauri`, PR 6). Note the two trees are being sent to
  *different versions of the same crate* — legal, since the lockfiles are
  separate, but a version skew between the engine and the shell in the audio
  device layer is a bad place to have one. TD-24 already records cpal as the
  least battle-tested part of the stack and unvalidated on iOS and Android, so
  a regression here has nothing to catch it.
* **symphonia 0.5 -> 0.6** (PR 2). The decoder, and the reason 58% of the
  library opens at all.
* **typescript 5.9 -> 7.0** (PR 8) and **vite 6.4 -> 8.2** (PR 11). Toolchain
  majors; cheap to try, noisy to debug mid-feature.

The four GitHub Actions bumps (PRs 3, 5, 7, 9) are the low-risk ones and CI
proves them on the spot. One exception inside that: **`tauri-action`
0.6 -> 1.0 (PR 3) should wait until after the first tag.** Changing the release
action and running the release pipeline for the first time in the same change
means a failure has two possible causes.

**When:** after the feature work, and after the app has run on real hardware —
which is the same condition TD-55 and TD-24 are already waiting on.

## What nobody has decided

1. **A paid Apple Developer account**, or no macOS build anybody else can
   open. See pipeline step 1. The only signing question still open.
2. **Governing law and a legal entity** for `docs/EULA-NOTES.md`. Parked here
   deliberately on 2026-08-23 rather than guessed at — nothing is sold, so
   nothing turns on it yet, and a lawyer will ask both questions first. The
   contact address that sat beside it is answered: `SUPPORT.md` names
   dylangrowcoot@gmail.com.
3. **Where the metadata comes from — parked as a risk, not a decision.**
   Dylan, 2026-08-23: the source does not matter as long as the data arrives
   *lazily*, per track, when it is needed. The switch in Settings is a
   philosophical question — do you want the app to look things up online at all
   — and not a command to go and fetch everything now.

   The risk, recorded rather than acted on: Deezer is called with no API key
   and no registered application, against an API that documents no quota. At
   one user that is a non-event. **It becomes real at distribution**, when many
   copies of one unregistered client hit the same API — the failure mode is
   being blocked, not being sued. If it ever needs to move, what Deezer
   supplies is an artist portrait, album art, and a genre for files that carry
   none; MusicBrainz plus the Cover Art Archive covers the last two and hosts
   **no artist images at all**, so portraits would need a third source. The
   `User-Agent` already sent is the one MusicBrainz asks for, so the move costs
   no new compliance work — it costs the portraits.

**Settled since this list was written**

* **What is in v1** — asked and answered 2026-08-23: *whatever exists when the
  work is finished.* It was never written down because it is not finished, and
  a feature list invented now would be fiction. Donation and onboarding are the
  last two things going in.

  **Amended 2026-08-24.** The answer held and the date moved: v1 is whatever
  exists on 2026-08-24, which includes donation and does not include
  onboarding. The definition did not change — "when the work is finished" was
  always going to be settled by Dylan saying so, and he has. Onboarding is
  v1.1.
* **Pull requests** — closed 2026-08-23. GitHub has no switch for this, so
  `.github/workflows/no-outside-prs.yml` closes any pull request whose author is
  not the owner, with a comment pointing at the issue tracker, and
  `CONTRIBUTING.md` says why. This was the one item that was *accruing* rather
  than waiting: every day the repository sat public with contributions
  unsettled was a day somebody could hand over copyright nobody could give
  back.
* **Sync between two real devices (TD-55)** — Dylan will test it once the
  feature work is done. Worth knowing it got harder rather than easier:
  AUD-7 landed a key exchange, so **every existing pairing is now invalid** and
  the first two-device test is also a first-pairing test.

## Found on the way, and not in any ticket

**The updater is a third network call nobody had counted.** `docs/RELEASE.md`
§3 says the app talks to two strangers, both off by default. There are three,
and the third is on by default with no switch: every desktop launch fetches
`releases/latest/download/latest.json` from GitHub and, if a signed newer
release exists, downloads and installs it silently. Verified at `lib.rs:6246`,
and no `updater_enabled` setting exists anywhere. That is defensible — it is
how the app stays patched — but it belongs in the privacy document, which it
now is, and `RELEASE.md` §3 is wrong until someone corrects it.

**A doc comment claims a privacy guarantee the code does not make.** *Fixed
2026-08-23 in `741cf34`; kept here because the finding is what the epic is
for.* `lib.rs:711` said the beacon "only runs on private addresses
([`peers::is_local`])". `is_local` is called in exactly two places, both in
`commands/sync.rs`, and both gate *outbound* pair and sync connections. The
beacon at `peers.rs:450` broadcasts unconditionally on whatever network the
device has joined — which is the case the comment was reassuring the reader
about. The behaviour is fine and documented honestly in `PRIVACY.md`; the
comment is what is wrong.

**`data_breakdown` itemises 8 of the ~17 things stored.** `plays.json` and
`skips.json` — what you played and what you skipped, which is how the DJ learns
— are among the ones it does not show, so the Your Data screen understates
what is kept.

## The measurement that is missing

No release has ever run. `release.yml`'s logic is verified as far as it can be
locally — the version check runs against the real files, the YAML parses, a
local `tauri build` produces exactly the artefacts it expects including the
`.sig` — and none of that is the same as having run. Two of its pinned action
SHAs pointed at commits that do not exist and nothing noticed, because nothing
had ever executed the file. Expect more of that shape on the first `-rc`.

**Two more of that exact shape, found 2026-08-24 while wiring the build.**
Both were sitting in the file, both would have failed the first tag, and
neither was findable by reading the workflow on its own:

1. **The macOS job would have died at the bundle step.**
   `bundle.macOS.signingIdentity` was pinned to `"Vapor Dev"` on 2026-08-23 —
   a self-signed certificate in one login keychain. `app.yml` was given
   `APPLE_SIGNING_IDENTITY: "-"` in the same commit and `release.yml` was not,
   so the workflow that has run proved the override and the workflow that has
   never run did not have it. The commit message for `ee4df98` says "CI passes
   `APPLE_SIGNING_IDENTITY: -`", which was true of the CI that ran.
2. **Nothing built Android at all.** `tauri-action` wraps `tauri build`; the
   mobile bundle is a different CLI verb with a Gradle project under it, so the
   matrix could never have produced an APK no matter what was in it. The epic
   listed Android as a target and the pipeline had no job for it.

The pattern is the same one this section is about, and it is worth naming: a
file that has never executed accumulates faults at the rate the things around
it change, and none of them are visible to review, because review reads the
file and the fault is in the gap between the file and everything else.
