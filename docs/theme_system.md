# Theme System

Two palettes, one identity, and one rule about where colour is allowed to live.

## What this covers

How a theme is chosen, where the choice is stored, how it reaches the DOM, and
what to do when you need a colour that does not exist yet. The palettes
themselves are transcribed from the design files and are not restated here —
`vapor-app/public/tokens.css` is the record, and it carries the reasoning for
each value beside the value.

This replaced a Godot theme system: a `ThemeData` resource, a `ThemeManager`
autoload broadcasting `theme_changed`, `.tres` colorways, and StyleBox
factories. None of it survived the port. If you are looking for
`make_glass_panel()`, its descendant is the `.glass` class at the foot of
`tokens.css`.

## The two themes

| | Daylight | Lamplight |
|---|---|---|
| Ground | warm paper, sky at the horizon | low-chroma umber under one lamp |
| `--page` | `#eceef1` | `#1c1712` |
| `--accent` | `#007aff` | `#ec992f` |
| Source | `Vapor Music v2 - Daylight.dc.html` | `Vapor Music v3 - Lamplight.dc.html` |

Both design files live in the redesign bundle, not in the repo. Values in
`tokens.css` are transcribed from them rather than invented; when a design
changes, re-extract rather than hand-editing the CSS and the file separately.

Two departures from the Lamplight file are recorded in `tokens.css` itself,
with the measurements behind them: the accent is amber rather than the
specified cool blue, and `--amber` moved to a lemon to stay clear of it.

## Choosing a theme

Three choices, in `src/lib/theme.ts`:

```ts
type Appearance = "auto" | "daylight" | "lamplight";
```

`auto` is the default and is what shipped before there was a control at all.

### `auto` is the absence of the attribute

This is the part most likely to be broken by a well-meaning change.

`applyAppearance` writes `data-theme="daylight"` or `data-theme="lamplight"` on
the document root. Under `auto` it **removes the attribute** rather than writing
a third value into it. That is what lets the media query in `tokens.css` keep
binding:

```css
:root[data-theme="dark"],
:root[data-theme="lamplight"] { /* … */ }

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]):not([data-theme="daylight"]) { /* … */ }
}
```

With no attribute present, the OS preference decides, and it keeps deciding
while the app is open — including when the machine flips at sunset, which no
JavaScript has to notice for the *colours* to follow.

A version that wrote `data-theme="light"` for `auto` would pass any test run on
a light machine and would silently stop following the OS on every machine.
`theme.test.ts` asserts the absence directly.

`dark` and `light` are accepted as aliases in the selectors because the
placeholder dark mode used them; nothing writes them now.

### Where the choice is stored

Two places, deliberately:

- **`Settings.theme`**, over IPC — the record. `set_appearance` rejects any word
  outside the three; `Settings::sanitised` repairs anything else on load, which
  is also the migration path for the Godot-era `default_dark` this field used to
  hold. `auto` is the right landing point for those installs, since the
  placeholder dark mode only ever followed the OS.
- **`localStorage["vapor.appearance"]`** — a mirror, so the theme can be applied
  before the backend answers. The settings round trip at boot retries for up to
  fifteen seconds against a core still reading the library off disk; without the
  mirror, somebody who chose Daylight on a dark-mode machine watches the app
  open dark and then blink.

`main.tsx` calls `restoreAppearance()` before React mounts. `App.tsx` calls
`adoptAppearance(asAppearance(s.theme))` when the settings arrive and reconciles
the two — usually a no-op.

The Appearance control applies the theme on the press and persists afterwards.
If the write fails it puts the choice back: a screen that says Lamplight and
reopens in Daylight tomorrow is worse than one that admits it could not save.

## The token layer

**`tokens.css` is the only file allowed to name a colour.** Every other
stylesheet reads `var(--…)`.

This is enforced, not requested — see the tests below — because it is exactly
what the first dark mode got wrong. It swapped a dozen tokens while some sixty
rules still said `rgba(255, 255, 255, 0.6)` outright, so on a dark machine the
text went dark and the surfaces stayed light. Every one of those rules
typechecked, rendered, and passed its screen's tests.

### Tokens are named by role, not by value

The fill tokens are the clearest case. On paper the four states are all "white,
a little more white"; under the lamp they are three different materials:

| Token | Means | Daylight | Lamplight |
|---|---|---|---|
| `--fill-quiet` | hover on an otherwise transparent control | white 60% | warm wash, `rgba(255,216,170,.06)` |
| `--fill-raised` | a control that sits on the page: input, chip, button | white 75% | smoked glass, `rgba(62,47,34,.5)` |
| `--fill-raised-hover` | its hover | `#fff` | `rgba(78,60,42,.66)` |
| `--fill-on` | selected: the nav item you are on | white 85% | accent tint, `rgba(236,153,47,.16)` |
| `--ink-on` | its label | `var(--ink)` | `var(--accent-dim)` |
| `--fill-strong` | drop targets, the drag layer | white 94% | `#2b231b`, opaque |

`--fill-on` is the one that shows why the naming matters. On paper, brighter
reads as nearer, so selection is more light. On umber, more light reads as a
smudge, so selection is an accent tint and the label takes the accent too. A
rule written as `rgba(255,255,255,0.85)` can only ever be right in one theme.

The same applies to `--accent-fill` and `--on-accent`. Daylight's filled control
*is* its accent — `#007aff` pushed away from a light ground into the dark — with
a white label. Away from Lamplight's ground is the other way, so the fill is the
accent itself at `#ec992f` and the label on it is dark.

### The rest of the file

`tokens.css` is sectioned: Brand, Ink, Surfaces, Fills, Glass, Artwork
placeholders, Radii, Type, Elevation, Motion. Radii, type and motion do not move
between themes. Everything above them does.

`--sov` is reserved. It means "this is on your device and it is yours" and must
never be used for ordinary emphasis.

## What CSS cannot reach

Two things need the resolved theme handed to them.

**`VaporMark`** draws into a canvas, so it cannot read a custom property. It
takes the theme through `useTheme()` and defaults to the resolved value; every
call site in the app omits the prop. It was pinned to `"light"` at all eight
sites while a dark mode existed, so the one element that draws its own
background kept drawing a bright one.

**Native widgets** — scrollbars, `<select>` menus, form controls — are drawn by
the OS. The `color-scheme` property in each palette block reaches them;
`index.html` declares `<meta name="color-scheme" content="light dark">` so both
are available to pick from. Without it a dark Lamplight window keeps a light
scrollbar down its side.

## Tests

`src/lib/tokens.test.ts` reads the CSS as data and checks two things a person
cannot hold in their head while editing:

- **No colour literal outside `tokens.css`.** Hex, `rgb()`/`rgba()`,
  `hsl()`/`hsla()` and the bare colour keywords, across every stylesheet under
  `src/`, comments stripped. The failure names file, line and rule.
- **The two dark blocks agree.** A custom property cannot be aliased to a whole
  block and `@import` cannot take a media query, so the Lamplight values are
  written twice — once for `[data-theme]`, once for `prefers-color-scheme`. The
  test asserts the same token set, the same values, and that every dark token
  has a Daylight counterpart. Duplication is the trade; this is the cheaper half
  of it.

`src/lib/theme.test.ts` covers the choice: parsing a stored value, resolving
against the OS, the root attribute, and the `localStorage` mirror surviving
storage that throws. `src/components/Appearance.test.tsx` covers the control,
including the revert on a failed save.

`src/test/setup.ts` stubs `localStorage` and `matchMedia`, neither of which this
jsdom provides, and exports `setPrefersDark()` so a test can move the OS
preference mid-run.

## Recipes

**Adding a colour.** Put it in `tokens.css`, in both palettes, in the section it
belongs to. Name it for what it does. If you are about to add a second name for
a size or colour the scale already has, use the existing one instead.

**Changing a palette.** Edit both dark blocks or the test will fail. Re-check
contrast — see below for where the current values sit.

**Adding a theme.** The selectors and `Appearance` are written for two. A third
would want `APPEARANCES`, `APPEARANCE_LABELS`, a swatch token, the Rust
`APPEARANCES` constant, and a palette block. Nothing is structurally in the way.

## Measurements

Contrast against each theme's reference background — `--page` `#eceef1` for
Daylight, elevated `#231d17` for Lamplight, which is what cards and rows sit on:

| | Daylight | Lamplight |
|---|---|---|
| `--ink` | 15.6:1 | 13.3:1 |
| `--ink-2` | 4.5:1 | 7.1:1 |
| `--ink-3` | 2.2:1 | 4.9:1 |
| `--accent` | 3.5:1 | 7.3:1 |
| `--sov-ink` | 4.5:1 | 9.0:1 |
| filled control + label | 4.0:1 | 8.1:1 |

`--ink-3` is a hint colour and does not carry body text in either theme;
Daylight's 2.2:1 is the value the design specifies. `--accent` at 3.5:1 in
Daylight is a fill colour under a white label, not text.

Three refusals the Lamplight file makes explicitly, each measured rather than
asserted:

- **No `#000` ground.** `#17130f` at the darkest. Pure black under bright text
  is what makes glyphs bloom and smear, which is the eye strain most dark modes
  actually cause.
- **No `#fff` ink.** `#f2e4d2` is already past AAA; the remaining distance to
  21:1 buys nothing and costs comfort. Its channels are 0.949 / 0.894 / 0.824 —
  warm on purpose, and a check that dark-mode ink must be near-white in every
  channel would fail it. Contrast ratio is the thing worth asserting.
- **No thin weights under 15px.** Warm dark grounds thin strokes optically, so
  body text holds at 400 and mono labels at 500.
