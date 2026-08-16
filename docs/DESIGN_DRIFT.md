# Design drift: what the rewrite changed, and where it came from

Written 2026-08-16, in answer to a direct question: *"The vibe is nothing like I
left it. What happened to all my original theory and philosophy? What is the
source of what you put here?"*

This traces every element of the current React app back to a source, or records
that it has none. It is an audit, not a proposal.

## The three sources

| # | Source | What it is |
|---|---|---|
| **S1** | `scripts/` (Godot) + `docs/ai_dj_workflow.md` | The original app and its written spec. Yours. |
| **S2** | `design/Vapor Music v2 - Daylight.dc.html` | The Daylight redesign. `design/README.md` calls it "the design. 12 screens, light theme. **Source of truth**". |
| **S3** | `vapor-app/` (React + Tauri) | The rewrite, built from S2. |

The finding, in one line: **S2 preserved S1's model almost completely. S3 dropped
large parts of both.** The drift is between the design and the rewrite, not
between the original and the design.

---

## Finding 1 — Navigation: the design specifies three tabs, the rewrite built twelve

S2 defines its navigation in code, not in a picture:

```js
{key:'library',  label:'Library'},
{key:'vibe',     label: dj ? 'Vibe' : 'Shuffle'},
{key:'settings', label:'Settings'},
```

Three destinations. The Vibe tab **renames itself to "Shuffle" when DJ mode is
off** — the design already answers the question of what that screen is when the
DJ is not conducting.

S3 has twelve sidebar entries: Library, Songs, Search, Now Playing, Queue, Vibe
DJ, Your Data, Settings, plus Liner Notes and Onboarding as routes.

**How it happened.** S2 contains twelve *mockups*, listed in `design/README.md`
as "Onboarding · Library · Songs · Search · Now Playing · Queue · Vibe DJ ·
Liner Notes · Settings · Your Data · Empty · Loading". Ten of those are screens
and two are states. The rewrite read twelve mockups as twelve destinations and
built a sidebar entry for each. `TECH_DEBT.md` TD-30 then recorded that reading
as the goal — *"Two screens of twelve"* → *"Done — all twelve exist"* — which
locked it in and marked it complete.

Two of the twelve labels are states, not screens, and the rewrite says so
itself: "plus Loading and Empty as shared components rather than routes, since
they are states every screen falls into." The same reasoning applies to Queue,
Now Playing and Search, and was not applied.

## Finding 2 — Songs and Search are tabs inside Library in the design

S2's Library screen carries a search field reading **"Search 1,284 tracks"**, and
its tab row is defined as:

```js
libTabs: this._chips(['Albums', 'Songs', 'Artists', 'Playlists'], 'Albums')
```

**Songs is a tab within Library.** S3 made Albums/Artists/Genres/All the tabs and
promoted Songs to a separate screen with its own table, then added a third
screen for Search whose function the Library field already covers.

S2's Search mockup is a *modal overlay* — it has a "Cancel" button and shows
"top result", the shape of an invoked search rather than a place you navigate to.

## Finding 3 — Queue is a bottom sheet, not a screen

S2 labels its Queue mockup **"06 Queue — bottom sheet"**. Its contents:

> Coastal Drift · Aeriform · **Up next** · **Conducted by Vibe · 47 min** ·
> **Re-conduct**

The queue is presented as belonging to Vibe — "Conducted by Vibe", with a
"Re-conduct" action on it. S2's Now Playing mockup opens with a **"⌄"** chevron,
the dismiss control of a sheet pushed up from the player bar.

S3 made both of them permanent sidebar destinations with no relationship to
Vibe.

## Finding 4 — Vibe DJ: the model survived the design and was dropped in the rewrite

This is the substantial one. S1's `docs/ai_dj_workflow.md` defines three match
classifications, a repeating four-step choice sequence, an AI Choice badge with
manual override, a Vibe Limit, and six named transitions.

**S2 kept them.** Its Vibe mockup shows a `PERFECT MATCH` badge, a `DJ` toggle,
`8 bars`, `phase +1.2ms`, `or blend into`, and `See path` — and its data defines
the alternates explicitly:

```js
alternates: [
  {title:'Vermilion Hours', bpm:'138', key:'11A', fx:'Filter Sweep',   tag:'FRESH'},
  {title:'Longwater',       bpm:'118', key:'6A',  fx:'Bass Swap',      tag:'MATCH'},
  {title:'Pale Machine',    bpm:'96',  key:'2A',  fx:'Reverb Freeze',  tag:'SWITCH'},
]
```

That is S1's `perfect` / `interesting` / `creative` triple, under your names,
each carrying one of your six transitions, colour-coded green / accent / amber.

**S3 has none of it.** What the screen shows instead: four curve buttons and one
"Conduct from here" button.

| From `ai_dj_workflow.md` | In the Daylight design | In the React app |
|---|---|---|
| Match / Fresh / Switch candidates | ✅ `alternates`, tagged and colour-coded | ❌ absent |
| 4-step mood path cycle | ✅ "See path" | ❌ absent |
| AI Choice badge + user override | ✅ `PERFECT MATCH` badge | ❌ absent |
| Smart Mixing on/off | ✅ `DJ` toggle, tab relabels to "Shuffle" | ❌ absent |
| Vibe Limit / Mix Tuner | ❌ not in the mockup | ❌ absent |
| 6 named transitions | ✅ shown per candidate | ⚠️ engine has all six; screen shows only the chosen one |
| Phrase-aligned blend length | ✅ "8 bars", "phase +1.2ms" | ❌ absent |
| Queue as "Conducted by Vibe" | ✅ | ❌ separate screen |

*(The "In the React app" column describes the app as it stood when this was
written. See "Closing the rest" at the foot of this file for what changed the
same day — every ❌ above is now built.)*

---

## What did *not* drift

The engine is a faithful port, and it is credited in the source.

* `vapor-core/crates/vapor-library/src/pathfinder.rs` is `dj_pathfinder.gd`
  ported — same A\* search, same weights, same Camelot graph. Its header cites
  your file by name and comments reference "the constants at the top of
  `dj_pathfinder.gd`".
* The four curves are yours, with identical maths:

  | Curve | `dj_pathfinder.gd` | `pathfinder.rs` |
  |---|---|---|
  | Build | `+0.4` energy, `+15` BPM | `+0.4` energy, `+15` BPM |
  | Chill | `−0.4` energy, `−15` BPM | `−0.4` energy, `−15` BPM |
  | Wave | `0.3·sin(2πt)`, `10·sin(2πt)` | `0.3·sin(2πt)`, `10·sin(2πt)` |
  | Flat | start energy, start BPM | start energy, start BPM |

* The six transitions and the ±6% stretch refusal are in the engine and match
  the spec.
* `lib.rs:915` still reasons in your vocabulary: *"The original's 'creative'
  match type: a genre jump is steered the same…"*

So the logic was carried across carefully. It is the **interface to that logic**
that was reduced, which is why the app can still plan a set but can no longer
show you the choice it made or let you override it.

## What was renamed with no source

Three things in `Vibe.tsx` came from nowhere but the rewrite:

| In the app | Should be | Source of the app's version |
|---|---|---|
| "Wind down" | **"Chill Down"** (`chill`) | none — a rename |
| "Steady" | the `_` fallback case, which you never named | none — an invention |
| "Starts easy, ends hard", "Lets the energy fall away", "Rises and falls across the set", "Holds one mood throughout" | — | none — prose written during the rewrite |

The curve *ids* in the React code are still `build` / `chill` / `wave` / `flat`,
so only the display strings drifted.

---

## Bearing on the nine items raised on 2026-08-16

Items 6 and 9 are not new requests. They are the design being asked for again.

| Item | Status against the design |
|---|---|
| **6** — fold Songs and Search into Library | S2 already specifies this (`libTabs`, Library search field, Search as a modal) |
| **9** — Queue and Now Playing off the sidebar; queue lives with Vibe; Vibe is Shuffle when DJ is off | S2 already specifies all of it, including the tab relabel |
| **5** — use the full window width | S2's mockups are 390px phone frames; the desktop layout was extrapolated, and the extrapolation kept phone proportions |
| **8** — the Vibe screen | Engine faithful; interface reduced; two labels invented. Detailed above. |

Items 1, 2, 3, 4 and 7 concern behaviour the design does not specify and are
open product decisions.

## What was done about it, same day

* **6, 9** — restored to the design. Library carries the search field and a
  Songs tab holding the table; the Songs and Search screens are gone. Now
  Playing opens from the player-bar title, the queue lives on Vibe and names
  who ordered it, and the tab relabels itself "Shuffle" when the DJ is off.
* **8** — `chill` is "Chill Down" again and the invented blurbs are replaced by
  each curve's actual arithmetic. The `_` fallback is shown as "Hold Steady",
  which remains the one label with no origin, because the original never gave
  it one. The Godot help modal is ported: it renders this repo's
  `ai_dj_workflow.md` verbatim, so the help cannot drift from the spec.
* **1, 2, 3, 4, 5, 7** — done; see the commit.

## Closing the rest, 2026-08-16

The three items left open above are now built, and the table in Finding 4 no
longer has a ❌ in it.

**Match / Fresh / Switch.** `_get_match_type_between` ported with its
thresholds — a genre jump is a Switch whatever else is true, then 8 BPM or 0.2
of energy separates Fresh from Match. One candidate per kind, each scored on
what its kind is *for* rather than on one shared notion of "closest", since a
Switch scored like a Match would just be a Match. The cards are transcribed
from the design's `alternates`, tag colours included.

**The four-step cycle and the override.** Match, Fresh, Match, then a change.
The fourth step **alternates rather than flipping the original's coin**: a set
that cannot be reproduced cannot be reasoned about, and the screen has to be
able to say in advance what it will do. Over a full period the proportions are
the same, which is what the coin was for. `selected` is a separate field from
`aiChoice` so §4's behaviour holds — the badge stays where the DJ put it and
the ring moves, and an override reads as an override.

**The Vibe Limit.** `transition_cost` has taken the energy threshold as a
parameter since the port and all three callers passed the same constant, so
this was a control the engine was already built for. It is a slider under the
curves, because the curve says where the set is going and this says how far it
may jump getting there.

### Three more of the same kind, found while looking

The pattern in Finding 4 — logic ported faithfully, interface never built — was
not confined to the Vibe screen.

| What was ported | What was missing |
|---|---|
| `FolderStore` and `Playlist::folder_id`, both tested, from `playlist_folder_service.gd` | No command, no UI. `folderId` reached the frontend as a field nothing could set. |
| `DEFAULT_ENERGY_THRESHOLD` as a parameter of `transition_cost` | Every caller passed the constant. |
| Nothing — `metadata_service.gd`'s network half was never ported at all | No lyrics, no artist image, no album art, no genre lookup. |

The first two are now exposed. The third is ported as `metadata.rs`, with one
deliberate change: **it asks first.** Everything else the app knows about a
track it works out on the device from the audio, which is what the sovereignty
green in the palette means and what Liner Notes says on screen. A lookup sends
the artist and title of what someone is listening to to a server they have no
relationship with. The Godot build did that unconditionally and mentioned it
nowhere. It is off by default, per track rather than per library, and what
comes back is drawn in its own panel naming where it came from.

Two bugs did not survive that port: `parse_lrc` divided whatever the fraction
matched by 100, so LRCLIB's three-digit milliseconds landed ten times late, and
it kept only the first timestamp on a line — which is how LRC writes a repeated
chorus, so the words stopped moving the second time round.
