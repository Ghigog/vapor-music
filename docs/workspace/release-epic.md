# REL — the release epic

Everything between here and a build a stranger can download and run. Opened
2026-08-23 because these items block each other in an order that is not
visible when they sit in `tickets.md` as eight unrelated rows.

`docs/RELEASE.md` is the reference — signing, licensing, third-party services,
accepted limitations, mechanics. This file is the *state*: what is done, what
blocks what, and which items are waiting on Dylan rather than on work.

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

*macOS.* `tauri build` produces an ad-hoc signature, which is a hash of the
binary — so every release build is a new identity, a keychain grant does not
survive one, and Gatekeeper refuses it on anybody else's machine. The dev loop
is already fixed by `src-tauri/.cargo/config.toml`, which signs with a stable
self-signed identity and pins the identifier; setting that same identity as
`signingIdentity` under `bundle.macOS` does the same for release builds and
costs nothing. **Handing the app to another person is a separate and paid
decision**: a Developer ID account plus notarisation.

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

*The keypair that already exists is wrong.* On 2026-08-22 a session printed the
updater private key into a transcript and rotated it. `~/.tauri/` holds a new
keypair and the old one beside it, suffixed `.COMPROMISED-2026-08-22`.
**`tauri.conf.json` still carries the old public key**, so the config trusts a
key whose private half is the compromised file, and a build signed with the new
key would produce a signature the app refuses. Nothing depends on it until a
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
| **AUD-3** what a supporter gets | **being built** — donation logic first | Nothing. The thank-you is a pin in Settings, one per donation |
| **AUD-23** the front door | **being built** — as onboarding, not marketing | Nothing. Donation goes first |
| **AUD-21** the updater keypair | **not a ticket any more** — pipeline step 1 above | Nothing, until the feature work is done |
| **REL-001** release signing | **not a ticket any more** — pipeline step 2 above | Step 1 |
| **AUD-22** first release | blocked — pipeline step 3 above | Steps 1 and 2, and what v1 contains |

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

## What nobody has decided

1. **Deezer or MusicBrainz.** The volume and the anonymity are dealt with —
   AUD-18 landed a `User-Agent` and a 200 ms floor between requests, down from
   ~7 a second sustained across a 563-track pass. What is left is the part that
   was always a decision: Deezer still has no API key and no registered
   application, and Deezer documents no quota to be compliant with.
   **What actually changes if it moves:** Deezer supplies three things — an
   artist portrait, album art, and a genre for files that carry none.
   MusicBrainz plus the Cover Art Archive covers the last two and **does not
   host artist images at all**, so artist portraits would need a third source
   or would go. The header already sent is the one MusicBrainz asks for, so the
   move costs no new compliance work — it costs the portraits.
2. **A contact address** for the support route. There is no telemetry and no
   crash reporting, both deliberate, so a person describing a fault is the only
   channel that exists. `SUPPORT.md` is written and its first line is a
   placeholder waiting on this. `docs/EULA-NOTES.md` wants a jurisdiction and a
   legal entity too.
3. **What is in v1.** Asked 2026-08-23 and worth recording that **nobody has
   ever written it down.** "v1" appears in `DECISIONS.md` §3 as a distribution
   choice (direct download) and in §2 as a thing to revisit at v1.1, and in
   `RELEASE.md` as a cost note. No document names a feature set, so "is v1
   worth cutting" cannot be answered as asked — the prior question is what it
   contains. Donation and onboarding are now the first two answers to that.
4. **Pull requests, and this one is overdue** — see AUD-17. The repository is
   already public with contributions unsettled, and one accepted outside pull
   request freezes the licence choice in a way that cannot be undone
   unilaterally. Disable PRs or add a CLA; minutes either way. It is on this
   list rather than the ticket board's quiet middle because it is the only open
   item that is *accruing* rather than waiting.

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
