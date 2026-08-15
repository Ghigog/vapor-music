# Design — Daylight

The redesign that phase 4 builds against. Produced in Claude Design; this
directory is the exported source, kept in the repo so the UI work has a
reference that does not require a login.

## What is here

| Path | What it is |
|---|---|
| `Vapor Music v2 - Daylight.dc.html` | **The design.** 12 screens, light theme. Source of truth. |
| `vapor-mark.js` | The generative logo, as a working custom element. Ships in the app. |
| `tokens.css` | Design tokens extracted from the above, for the app to import. |
| `assets/` | Icons, logo, background. Ship in the app. |
| `_ref/` | Earlier iterations, kept for context. Not built against. |

Deliberately **not** here: `support.js`, the Claude Design canvas runtime. It
renders `.dc.html` files inside the design tool and is not application code.
The screenshots and uploads from the export were dropped for the same reason.

## The idea

> Every dark music app says the same thing: *stay in here with us*. Daylight
> says the opposite.

Warm paper, sky at the horizon, glass with real light in it. One green —
`--sov` — that means only one thing: **this is on your device, and it is
yours.** It is reserved for that. Using it for ordinary emphasis would spend
the one piece of colour the product has an argument attached to.

## Screens

Onboarding · Library · Songs · Search · Now Playing · Queue · Vibe DJ ·
Liner Notes · Your Data · Settings · Loading · Empty

`Your Data` is new in v2 and follows directly from the sovereignty claim: the
place the promise gets proved rather than asserted.

Mobile layouts for Library, Vibe and Settings are in
`_ref/Vapor Music - Today (Mobile).dc.html`.

## The mark

`vapor-mark.js` is not a picture of a logo — it draws one, per frame, from a
signed distance field over a chain of ~64 nodes. Arms that are close in space
but far apart along the curve are unioned with a smooth minimum, so where the
W folds back it fuses with a fillet instead of creasing. Lighting is analytic
from the surface normal, so nothing is stamped and nothing can disagree.

It is a plain custom element with **no dependencies**, so it drops into the
app unchanged:

```html
<script src="vapor-mark.js"></script>
<vapor-mark size="128" theme="light" state="idle"></vapor-mark>
```

| Attribute | Values |
|---|---|
| `size` | px, square. Default 160 |
| `theme` | `light` \| `dark` |
| `state` | `idle` \| `playing` \| `blending` \| `thinking` |
| `energy` | 0–1, amplitude drive while playing |
| `speed` | motion multiplier, default 1 |
| `hue` | hue offset in degrees |
| `static` | present = render one frame, no rAF |

The states are the point: it breathes when idle, tightens while the DJ is
choosing, and swirls through a blend. Those map onto real engine state —
`state="thinking"` during pathfinding, `state="blending"` during a transition,
`energy` from the deck's level — so the logo is a readout, not decoration.

> [!NOTE]
> The mark is still being worked on. Treat its shape as unsettled; the
> attribute surface above is what the app should code against, since that is
> unlikely to change even if the rendering does.

## Using the tokens

`tokens.css` is transcribed from the design, not invented. When the design
changes, re-extract rather than editing both by hand.

It defines the palette, glass treatment, radii, type scale and elevation, plus
a few primitives the design repeats everywhere: `.glass`, `.label`,
`.numeric`, `.sovereign`. Dark theme overrides surfaces and ink only — brand
hues are unchanged, so the sovereignty green means the same thing in both.

Shadows are blue-tinted rather than neutral: on warm paper a grey shadow reads
as dirt.
