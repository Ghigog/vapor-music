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
| **AUD-16** privacy / EULA / support | in progress | a contact address, and a lawyer before anything is sold |
| **AUD-18** Deezer terms | open | reading Deezer's current API terms, then registering or moving to MusicBrainz + Cover Art Archive |
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
2. **Deezer or MusicBrainz.** Deezer is called today with no API key, no
   registered application, and no `User-Agent` naming a contact, at a request
   volume that an analysis pass over a whole library makes substantial.
3. **A contact address** for the support route. There is no telemetry and no
   crash reporting, both deliberate, so a person describing a fault is the only
   channel that exists.
4. **Whether v1 is worth cutting yet.**

## The measurement that is missing

No release has ever run. `release.yml`'s logic is verified as far as it can be
locally — the version check runs against the real files, the YAML parses, a
local `tauri build` produces exactly the artefacts it expects including the
`.sig` — and none of that is the same as having run. Two of its pinned action
SHAs pointed at commits that do not exist and nothing noticed, because nothing
had ever executed the file. Expect more of that shape on the first `-rc`.
