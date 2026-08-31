# POL — the polish epic

Opened 2026-08-28, from Dylan's list, given while `v2.0.0-rc.10` was under its
soak test. Nothing here is started.

These are **not** release blockers. rc.10 is the build being proved; this is the
version after it. Kept in one file because several items touch the same two
screens and doing them in isolation would mean three sessions editing
`Settings.tsx`.

Every item carries the file and line it lands in, so picking one up does not
start with a search.

---

## POL-1 : the lyrics attribution tag

**Remove it.** Two copies:

* `components/LyricsPanel.tsx:178` — `" Timed by whoever transcribed them, so the alignment is theirs."`
* `screens/LinerNotes.tsx:294` — `" Timed to the recording by whoever transcribed them, so the alignment is theirs, not ours."`

`screens/screens.test.tsx:962` asserts `/from lrclib/i` is present. Dylan asked
for the *alignment* sentence to go. **Open question: does "from lrclib" stay?**
It is an attribution rather than a hedge, and lrclib asks to be credited, so it
is read here as staying unless he says otherwise — the test is left alone.

Dylan, verbatim: *"You have a tendency to write these little extra bits; remove
them."* Worth treating as a standing note rather than one edit.

## POL-2 : trim the "Music on this device" empty state

`screens/Settings.tsx:120`. Reduce to **"No folders yet."** and nothing else.

Keep `Settings.tsx:157`, the "Forgetting a folder removes it from the library"
line — Dylan named it explicitly.

`screens/Settings.test.tsx:606` asserts the long copy and will need updating in
the same change.

## POL-3 : help with "Where your music lives"

`screens/Settings.tsx:494`. Three parts.

**POL-3a — suggest the real server address.** Typing `koofr` should offer
`https://app.koofr.net`.

> **Flag, and it changes the shape of this item.** Dylan's examples were
> "google drive, proton, koofr". **Google Drive and Proton Drive do not offer
> WebDAV at all** — Google never shipped it, and Proton Drive exposes its own
> API rather than WebDAV. So a provider list cannot include the two he named
> first, and a suggestion that produces a URL which cannot work is worse than
> no suggestion.
>
> This wants confirming provider by provider before any endpoint is hardcoded,
> and it wants deciding what the field does for a provider that has no WebDAV —
> silence, or an honest "this provider has no WebDAV" note. Verify each
> endpoint against the provider's own documentation; do not transcribe from
> memory, including the Koofr one above.

**POL-3b — rename "Password" to "App password".** `Settings.tsx:528` already
says so in the hint; the label should say it too.

**POL-3c — suggest the folder path from the provider.** Given a Koofr address,
offer `dav/Koofr/` so the person types only `music`. Same verification
requirement as POL-3a: the path is provider-specific and must be checked, not
guessed.

## POL-4 : the animated logo

The About section and the first-run onboarding page should use the **live,
animated** Vibe logo, not the static app icon.

## POL-5 : restructure the settings sections

Dylan's reading, which is correct: Network is the only section with its title
*inside* the section, and two different sections are called "Library".

Proposed shape — **this one needs his sign-off before anyone moves a control**,
because it is the arrangement of a screen rather than a defect:

* Fold **Analyse** into the lower Library section, beside **Hide duplicates**
  (`Settings.tsx:708`) and **Fetch lyrics and artwork** (`Settings.tsx:746`).
* Fold **Share across this network** — Network's only control — into Library
  too, and retire the Network section.
* Move the **Add a folder** button from "Music on this device" to the *top* of
  "Where your music lives", as the first thing on it, with the cloud options
  under a line reading *"alternatively, play music from the cloud"*.

## POL-6 : playlists and groups look like two different features

* Titles are different sizes.
* Playlists have a download button; groups do not.
* Playlists have Play and Delete buttons; groups do not.
* **Playlists cannot be renamed at all.**

Wanted: same title treatment, same download-for-offline button on both, and
**no** Play or Delete buttons on either. Removing a track becomes long-press
and drag to a panel at the bottom reading **Remove**. Renaming needs to exist.

## POL-7 : drag and drop onto playlists and groups

* Dragging an album onto the playlists/groups area at the bottom **should open
  the sub-menu** so there is something to drop into. It does not.
* An **entire artist** should be draggable into a playlist or a group, as
  should an album — not only individual tracks.
* The empty-playlist copy says *"drag tracks from songs onto this playlist in
  the sidebar"*. On mobile there is no sidebar: it should name the bottom bar,
  and say tracks, albums **and** artists.

## POL-8 : the DJ queued a track that will not decode — **a bug, not polish**

Vibe picked a track, playback failed with `unsupported: no decodable audio
track` (`vapor-core/crates/vapor-dsp/src/decode.rs:242`), and the music stopped.

Dylan's question is the right one: if it cannot be decoded, analysis should
have caught it and it should be in the failed list under the analysis view — so
why was it a candidate?

**Two possibilities, and they have different fixes. Establish which before
touching anything:**

1. **The candidate pool is built from the library rather than from analysed
   tracks.** Then the DJ can pick anything, and the fix is to exclude tracks
   with no analysis, or with a recorded failure.
2. **The track analysed fine and failed later, at play time.** Likely if it
   lives on WebDAV: a truncated or partial fetch, or an error page returned as
   bytes, decodes as "no decodable audio track". Then the pool is right, the
   read is wrong, and the fix is retry-and-skip rather than filtering.

What decides it, and neither needs code: **does that track appear in the failed
list now**, and **does it play when picked by hand from the library?** Playing
by hand rules possibility 2 in or out on its own.

Separately, and true under either: **the DJ stopping the music because one
track failed is its own defect.** It should skip to the next candidate. That
part is worth fixing whatever the cause turns out to be.

---

## Not in scope here

Everything in `devops-epic.md`, and the outstanding memory work in AND-7 — a
sixty-minute mix still holds ~635 MB of decoded samples before analysis starts.
That is a bigger job than any of the above and should not be bundled with
screen polish.
