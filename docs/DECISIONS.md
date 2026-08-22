# Decisions

Answers to the ten-desk pre-launch audit of 2026-08-21. One line each on what was
decided and why, so the reasoning survives the conversation it came from.

The audit reports themselves are not in the repository — they were session
scratch. What they concluded is summarised here where it changed a decision.

---

## 1. Jurisdiction — Ecuador

Tax residence follows where you actually live, not where you choose. Ecuador
today; Italy would only take over if a year is genuinely spent there (the usual
test is more than 183 days, and it is a fact about the calendar rather than a
preference).

Two consequences that matter more than the label:

* Ecuador is **outside the EU**, so EU VAT on digital sales would fall under the
  non-Union OSS rules if anything were ever sold to an EU consumer.
* Decision 2 removes almost all of this anyway. A gift with nothing given in
  return is not a supply, so there is no VAT to account for and no consumer
  contract to perform.

Confirm with an Ecuadorian accountant before any money moves. Not before.

## 2. Money — a true donation, nothing unlocked

**Customisation stays free for everyone.** The donation buys nothing, unlocks
nothing, and is not a precondition for any feature.

Four independent reviewers reached the same conclusion about the original plan
(a donation that unlocks customisation): it takes on the full legal and tax
profile of a paid app — Apple's in-app-purchase requirement, EU and UK
withdrawal rights, EU VAT with no registration threshold — while capping revenue
at donation levels, and the word "donation" adds a misleading-practice exposure
on top of obligations that attach either way. It was the worst available
quadrant, not a clever middle.

The weak enforcement was never the objection. Nobody proposed DRM and nobody
wants it.

**Thank-yous are still wanted**, and the rule that keeps them safe is simple:
*nothing may be contingent on payment.* A supporter credit in the About screen,
early access to a build everyone eventually receives, or a separate cosmetic
download are all gifts. A feature that only a payer can reach is a sale wearing
a gift's label, whatever the button says. Options are still open; see
`docs/LICENSING.md` for the licence side of the same question.

Revisit at v1.1, when there is evidence about whether anyone donates at all.

## 3. Distribution — direct download for v1

itch.io is the likely host. Stores are a later decision, and deliberately so:
direct download removes Apple's payment rules entirely and keeps the app out of
both review queues while it is still being shown to friends.

## 4. Updater — ships in v1

Not reversible. Without one the first cohort can never be patched, and there is
already a known patch coming: LAN sync needs a transport fix before the app goes
to anyone else.

## 5. Platforms — desktop and Android for v1, iOS deferred

Wanted: macOS, Windows, Linux, Android, and iOS eventually.

* **Windows and Linux** already compile in CI. Adding them to a release is
  cheap.
* **Android** compiles in CI and was unproven on hardware when this was decided
  (2026-08-21): no device run, no automated coverage, and 359 lines of
  hand-transcribed JNI where a wrong signature aborts on the phone rather than
  failing to build. One manual run on real hardware before it ships to anyone.
* **iOS is out for v1**, and not on preference. There is no sideloading route,
  so it requires the Apple Developer Program and App Store review — which is
  exactly the paid channel decision 3 avoids. Revisit only if the app is ever
  worth $99/year to distribute.

## 6. The Godot tree — delete it, keep the tag

It is not shipped and has not been the shipping version for some time. Its CI
job runs a macOS runner and a Godot download on every commit, including
docs-only ones, to hold a baseline of nineteen known-stale failures steady.

`godot-final-v1.78` already exists. Deleting the working tree loses nothing —
the history retains every commit and the tag is a checkout away whenever a
legacy feature needs to be read. Three tech-debt tickets close with it.

## 7. Concurrent Claude sessions — enforce, don't ask nicely

Split `vapor-app/src-tauri/src/lib.rs` into `commands/<domain>.rs`, leaving only
`generate_handler![]` behind. At ~10,900 lines and 102 commands it is the single
door to the backend, so any two sessions doing backend work are in the same file
by construction — the structural cause of two of the three recorded incidents
where one session committed another's work.

Add a `PreToolUse` hook that blocks `git add -A`, `git add .` and `commit -a`.
The written rule cannot be followed reliably: `git status` has no author column,
so "files you did not create" is undecidable by a session that has lost its
early context. One commit obeyed the letter of the rule with 25 explicit paths
and broke its intent completely.

Stated priority: making sessions behave with each other is the top one.

**Relaxed the same day, after using it.** The original wording forbade a session
from touching another's uncommitted work at all. Dylan: the priority is that the
tree stays current and conflict-free, not that loose work sits untouched. A
session may now commit work it did not do, provided it lands in its own commit,
names whose it is, and passes whatever gate covers it first — committing leaves
everything where it was, which is what separates it from `stash`.

The hook did not move. What it denies destroys or removes work; committing does
neither.

---

## Constraint on all of the above

**v1 must cost nothing.** Nobody knows about this app and it will not have
users beyond friends, so no recurring bill is justified before there is evidence
anyone wants it.

The one that follows from this and is easy to miss: **a public repository is not
an open-source one.** GitHub Actions minutes and GitHub Releases hosting are
free for public repos regardless of licence, so a public repository with
`LICENSE` reserving all rights buys free CI and free updater hosting at no cost
to the licence position.

This repository is already public — `gh repo view` on 2026-08-21 — which was
assumed the other way round when the seven answers above were written. So
`docs/LICENSING.md`'s precondition is not something to do before flipping a
switch; it is overdue. Disable pull requests or require a CLA, because a single
accepted outside contribution freezes the licence choice.

The remaining unavoidable cost is Apple's $99/year, needed only for macOS
signing and notarisation. Unsigned macOS builds open with a documented
right-click workaround, which is acceptable for friends and not acceptable for
strangers.
