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
AUD-21 (keys exist)  ->  REL-001 (builds are signed)  ->  AUD-22 (pipeline runs on a tag)
```

`release.yml`'s `verify` job fails without the signing secret, so AUD-22 cannot
be exercised at all until AUD-21 is answered. Nothing else in this epic is
blocked by anything else in it — the rest are independent, and several are
already done.

## Status

| Item | State | Waiting on |
|---|---|---|
| **AUD-20** supply chain | **done** — 23 actions SHA-pinned, Dependabot, `cargo deny` gate in CI | — |
| **AUD-19** Windows CI | **done** — all eight jobs green, NSIS installer builds | — |
| `release.yml` dead pins | **done** — two of five pinned SHAs did not exist upstream and would have failed on the first tag push | — |
| **AUD-16** privacy / EULA / support | **partly done** — `PRIVACY.md` and `SUPPORT.md` written from the code, `docs/EULA-NOTES.md` states the gap without pretending to close it | a contact address, and a lawyer before anything is sold |
| **AUD-18** Deezer terms | **half done** — every request identified by `User-Agent`, three per-service clocks, four attempts with backoff | Dylan: Deezer or MusicBrainz. The calls are polite either way |
| **AUD-3** what a supporter gets | open | Dylan to pick. Recommendation recorded 2026-08-23: a supporter credit in About, opt-in, and nothing contingent on payment |
| **AUD-23** the front door | open | Dylan, and something worth pointing at |
| **AUD-21** the updater keypair | **deliberately parked** | Dylan. Standing instruction: no private key is kept until distribution is real |
| **REL-001** release signing | blocked | AUD-21 |
| **AUD-22** first release | blocked | AUD-21, and a decision that v1 is worth cutting |

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

## What nobody has decided

1. **Where the signing keys live**, on both platforms. Half an hour of work and
   the thing every other release item waits behind.
2. **Deezer or MusicBrainz.** The volume and the anonymity are dealt with —
   AUD-18 landed a `User-Agent` and a 200 ms floor between requests, down from
   ~7 a second sustained across a 563-track pass. What is left is the part that
   was always a decision: Deezer still has no API key and no registered
   application, and Deezer documents no quota to be compliant with. Moving to
   MusicBrainz plus the Cover Art Archive needs no new header — the one that
   went in is the one MusicBrainz asks for.
3. **A contact address** for the support route. There is no telemetry and no
   crash reporting, both deliberate, so a person describing a fault is the only
   channel that exists. `SUPPORT.md` is written and its first line is a
   placeholder waiting on this. `docs/EULA-NOTES.md` wants a jurisdiction and a
   legal entity too.
4. **Whether v1 is worth cutting yet.**

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
