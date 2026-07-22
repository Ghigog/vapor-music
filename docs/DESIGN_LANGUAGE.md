# Vapor Music — Design Language

**Version:** 1.0  
**Status:** Living Document  
**Maintained by:** Design & Frontend

> This document is the single source of truth for all visual, typographic, motion, and interaction decisions in Vapor Music. Every UI element, screen, and animation should be traceable back to a principle defined here. When in doubt, ask: *does this feel like Vapor?*

---

## 1. Design Philosophy

### The Aesthetic: Apple Glassmorphism × Modern Minimalism

Vapor Music's primary design lineage is **Apple's evolving material language** — from macOS Big Sur through to visionOS. This means: frosted glass, layered depth, ambient light bleeding through surfaces, and a ruthless commitment to letting the content (the music) be the hero. The UI recedes; the music leads.

**Frutiger Aero** (ca. 2004–2013) remains a cherished secondary inspiration — the era of software that felt *alive*, with warmth, organic curves, and physical presence. But we filter those feelings through Apple's modern restraint, not reproduce them.

The result is a UI that feels:

- **Alive** — surfaces breathe, elements respond, light moves
- **Tactile** — glass has weight, buttons have depth, sliders have resistance
- **Intimate** — like holding a beautiful object, not staring at a dashboard
- **Focused** — the music is always the hero; the UI recedes when you're listening

### The Single Guiding Question

> *Does this feel like touching frosted glass while standing next to a window?*

If an element feels flat, harsh, corporate, or aggressive — it's wrong.

---

## 2. Color System

### 2.1 The Philosophy of Color in Vapor

Color in Vapor is *atmospheric*, not decorative. We do not use color to signal brand ownership or to draw attention to UI chrome. Color exists to:

1. Communicate **depth and materiality** (darker = deeper behind glass)
2. Reflect **the current album's mood** (Dynamic Palette — see §2.4)
3. Indicate **interactive state** without aggression
4. Create **ambient warmth** in an otherwise cool glass material

### 2.2 Base Palette — Dark Mode ("Vapor Dark")

Dark mode is the primary and default experience. Built on deep charcoal with pure-white text and a single Apple system-blue accent — clean, professional, and at home on any desktop.

#### Background Layers

| Token | Hex | Opacity | Usage |
|---|---|---|---|
| `BG_VOID` | `#141414` | 100% | True background. Behind everything. |
| `BG_BASE` | `#1E1E1E` | 100% | Primary app surface. Navigation bars, sidebars. |
| `BG_ELEVATED` | `#282828` | 100% | Card backgrounds. Slightly elevated surfaces. |
| `BG_FLOAT` | `#333333` | 100% | Modals, drawers, overlay panels. |
| `BG_GLASS` | `#1E1E1E` | 55% | Frosted glass base. Applied with backdrop blur. |

#### Glass Surface Tints

| Token | Value | Usage |
|---|---|---|
| `GLASS_TINT` | `rgba(255, 255, 255, 0.03)` | Barely perceptible white wash over frosted surfaces |
| `GLASS_BORDER` | `rgba(255, 255, 255, 0.15)` | Top/left edge highlight (simulates light hitting glass) |
| `GLASS_BORDER_SUBTLE` | `rgba(255, 255, 255, 0.06)` | Bottom/right edge shadow |
| `GLASS_SHIMMER` | `rgba(255, 255, 255, 0.05)` | Subtle gradient sheen across glass panels |
| `GLASS_BLUR` | `24px` | Standard backdrop blur radius |
| `GLASS_BLUR_HEAVY` | `40px` | For primary modals and Now Playing full-screen |

#### Text Colors

| Token | Hex | Opacity | Usage |
|---|---|---|---|
| `TEXT_PRIMARY` | `#FFFFFF` | 100% | Headings, track titles, primary labels. |
| `TEXT_SECONDARY` | `#FFFFFF` | 60% — `#999999` | Artist names, subtitles, secondary metadata. |
| `TEXT_TERTIARY` | `#FFFFFF` | 35% | Timestamps, inactive tabs, hints. |
| `TEXT_DISABLED` | `#FFFFFF` | 18% | Disabled states. |
| `TEXT_INVERSE` | `#141414` | 100% | Text on light/accent surfaces. |

#### Accent — "Apple System Blue"

A single, professional cool blue. No secondary accent competes for attention.

| Token | Hex | Usage |
|---|---|---|
| `ACCENT_CORE` | `#0A84FF` | Primary interactive elements, active states, progress. |
| `ACCENT_BRIGHT` | `#42A4FF` | Hover states, focus rings, highlights. |
| `ACCENT_DIM` | `#0066DE` | Pressed states, deep accents. |
| `ACCENT_GLOW` | `rgba(10, 132, 255, 0.35)` | Box shadows, ambient glow effects. |
| `ACCENT_SURFACE` | `rgba(10, 132, 255, 0.12)` | Tinted glass panels, selected states. |

#### Secondary Accent

The `AQUA_*` token group is retained for API compatibility but aliased to the blue accent family in all default themes. Custom themes may override it.

#### Semantic Colors

| Token | Hex | Usage |
|---|---|---|
| `SEMANTIC_SUCCESS` | `#34D399` | Sync complete, track loaded, analysis done. |
| `SEMANTIC_WARNING` | `#FBBF24` | Offline mode indicator, sync conflict. |
| `SEMANTIC_ERROR` | `#F87171` | Playback failure, auth error. |
| `SEMANTIC_INFO` | `#60A5FA` | General informational toasts. |

All semantic colors are used as glows and tinted surfaces — never as flat, saturated blocks.

---

### 2.3 Light Mode — "Vapor Light"

Light mode is available and fully supported. Built on Apple's `#F5F5F7` system grey with pure-white frosted panels and a single `#007AFF` accent. It adapts seamlessly to any wallpaper and feels native on macOS.

| Token | Hex | Opacity | Usage |
|---|---|---|---|
| `BG_VOID` | `#F5F5F7` | 100% | Background. Apple system light grey. |
| `BG_BASE` | `#F9F9F9` | 100% | Primary surface. |
| `BG_ELEVATED` | `#FFFFFF` | 100% | Cards. Pure white glass. |
| `BG_GLASS` | `#FFFFFF` | 50% | Frosted glass, light mode variant. |
| `GLASS_BORDER` | `#FFFFFF` | 80% | Glass edge highlight — crisp top/left edge. |
| `GLASS_BORDER_SUBTLE` | `#000000` | 6% | Soft lower/right edge for depth. |
| `TEXT_PRIMARY` | `#262626` | 100% | Near-black. Crisp and readable. |
| `TEXT_SECONDARY` | `#8E8E93` | 100% | Apple cool medium grey. Soft but legible. |
| `TEXT_TERTIARY` | `#8E8E93` | 60% | Tertiary text. |
| `TEXT_DISABLED` | `#8E8E93` | 30% | Disabled states. |
| `ACCENT_CORE` | `#007AFF` | 100% | Apple Light System Blue. |

Accent family and semantic colors follow the same pattern as dark mode.

---

### 2.4 Dynamic Palette — Album Art Extraction

This is one of Vapor's signature touches. When a track is playing, the app extracts the **dominant color family** from the album art and **subtly shifts the ambient atmosphere** of the interface.

**Rules:**
- Extract 3 dominant colors from the album art using a palette extraction algorithm (e.g., k-means clustering or median cut)
- Desaturate extracted colors by 40%, reduce lightness by 20%, shift them toward the cool end of their hue
- Apply as a **radial gradient bloom** behind the Now Playing panel only — it does not affect navigation or library views
- Transition duration: `1200ms` ease-in-out when track changes
- Cap saturation at 60% HSL to prevent garish results
- Never override semantic colors or the primary accent with extracted colors
- The dynamic accent always blends within the blue family — it does not override `ACCENT_CORE`

```
Album Art → Color Extraction → Desaturate + Cool Shift → Ambient Bloom
```

This effect is subtle. If a user notices it consciously, it's probably too strong.

---

## 3. Typography

### 3.1 Font Stack

#### Primary Typeface — **Inter**

Inter is the backbone of the Vapor type system. It was designed for screen legibility at small sizes and has exceptional OpenType feature support. It reads cleanly through a frosted glass surface.

```
font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
```

- Source: [Google Fonts — Inter](https://fonts.google.com/specimen/Inter)
- Variable font axes used: `wght` (100–900), `slnt` (0 to -10 for subtle italics)
- Enable OpenType features: `font-feature-settings: 'cv02', 'cv03', 'cv04', 'tnum';`
  - `cv02–04`: Alternate letterforms for cleaner numerals
  - `tnum`: Tabular numerals for timestamps, BPM, and progress times

#### Display / Title Typeface — **Outfit**

Used exclusively for large display text: the Now Playing track title (full-screen mode), onboarding headlines, and the app wordmark. Outfit has a slightly rounded, humanist character that adds warmth to large sizes.

```
font-family: 'Outfit', sans-serif;
```

- Weights used: 300 (Light), 400 (Regular), 600 (SemiBold), 700 (Bold)
- Source: [Google Fonts — Outfit](https://fonts.google.com/specimen/Outfit)

#### Monospace — **JetBrains Mono**

Used for: BPM values, musical key labels, file size display, technical metadata, and the waveform timestamp ruler.

```
font-family: 'JetBrains Mono', 'Fira Code', monospace;
```

---

### 3.2 Type Scale

All sizes are in pixels. The scale is based on a modular ratio of **1.25 (Major Third)** starting from a base of 14px.

| Token | Size | Line Height | Weight | Typeface | Usage |
|---|---|---|---|---|---|
| `--type-2xs` | 11px | 1.4 | 400 | Inter | Micro-labels, badge text |
| `--type-xs` | 13px | 1.45 | 400 | Inter | Timestamps, hint text, secondary captions |
| `--type-sm` | 14px | 1.5 | 400 | Inter | Body text, list item labels |
| `--type-base` | 16px | 1.55 | 400 | Inter | Default body, settings descriptions |
| `--type-md` | 18px | 1.4 | 500 | Inter | Card titles, section headers |
| `--type-lg` | 22px | 1.3 | 600 | Inter | Page titles, album titles |
| `--type-xl` | 28px | 1.2 | 700 | Outfit | Screen headings |
| `--type-2xl` | 36px | 1.15 | 700 | Outfit | Onboarding headlines |
| `--type-display` | 48–72px | 1.0 | 300 | Outfit | Now Playing track title (full-screen) |

**Display sizes** (Now Playing full-screen track title) are set in Outfit Light (300) at large sizes because thinness creates elegance at scale. Heavy weights at 72px feel like a billboard, not a music player.

---

### 3.3 Typographic Rules

1. **Never use pure white text** — Use `--text-primary` (`#F0F0FF`). Pure white is harsh through glass.
2. **Track titles** are set in Inter SemiBold (600), never bold. Bold at small sizes reads as aggressive.
3. **Artist names** are always `--text-secondary`, never the same weight or color as the track title.
4. **Metadata values** (BPM, key, duration) use JetBrains Mono in `--text-tertiary`.
5. **Letter spacing:** Large display text (≥28px) gets `letter-spacing: -0.02em`. Small text (≤13px) gets `letter-spacing: 0.01em` for legibility.
6. **No fully uppercase strings** in the primary UI. Uppercase is reserved for subtle section dividers only, set at `--type-2xs` with `letter-spacing: 0.12em`.

### 3.4 Font & Layout Consistency for New Features

When introducing new UI components, sidebar extensions, lists, or custom navigators, developers must enforce the following rules:
- **Button Typeface Matching**: All navigation list headers, buttons, toggle elements, and items must strictly use `font_ui` (Inter). Do not use `font_display` (Outfit) for small UI actions or list elements.
- **Header Text Brightness**: Custom headers or toggle buttons (e.g., "Playlists") must match standard navigation elements. Use `--text-tertiary` when idle and `--text-secondary` on hover/pressed. Do not use `--text-secondary` as a baseline color for custom controls.
- **Scroll Container Wrapping**: Sidebar navigation elements and dynamic lists must be wrapped inside a `ScrollContainer` with horizontal scroll disabled. The parent panel must never expand vertically beyond the window boundaries; vertical overflow must be handled strictly by scrolling within the container.

---

## 4. Depth & Layering

Vapor's UI exists in **z-space**. Every element has a defined layer, and layers are communicated through blur, brightness, and shadow — not borders or flat color.

### 4.1 The Layer Model

```
Layer 6 — Toasts / Tooltips          (always top, brief)
Layer 5 — Full-screen Modals          (Now Playing expanded, Settings)
Layer 4 — Drawers / Sheets            (Queue panel, EQ drawer)
Layer 3 — Popovers / Context Menus    (right-click, dropdowns)
Layer 2 — Floating Elements           (Mini Player, sticky header)
Layer 1 — Primary Content             (Library, Now Playing card)
Layer 0 — Background / Backdrop       (album art bloom, void)
```

### 4.2 Shadow System

Shadows in Vapor are **not** the default dark boxes you see in Material Design. They are cool-blue tinted, diffuse, and multi-layered to simulate real light physics.

| Token | Value | Layer |
|---|---|---|
| `--shadow-sm` | `0 2px 8px rgba(6, 6, 10, 0.40)` | Subtle card lift |
| `--shadow-md` | `0 4px 16px rgba(6, 6, 10, 0.50), 0 1px 4px rgba(6, 6, 10, 0.30)` | Floating panels |
| `--shadow-lg` | `0 8px 32px rgba(6, 6, 10, 0.60), 0 2px 8px rgba(6, 6, 10, 0.40)` | Modals, drawers |
| `--shadow-glow-accent` | `0 0 20px rgba(123, 110, 246, 0.40), 0 0 60px rgba(123, 110, 246, 0.15)` | Active elements, now playing |
| `--shadow-glow-aqua` | `0 0 20px rgba(61, 214, 200, 0.35), 0 0 50px rgba(61, 214, 200, 0.12)` | Waveform, AI DJ mode |

### 4.3 Glass Panel Construction

Every frosted glass panel is built from the same stack of layers:

```
1. backdrop-filter: blur(var(--glass-blur))          ← the frosting
2. background: var(--bg-glass)                        ← dark tinted base
3. background + var(--glass-tint)                     ← cool blue atmosphere
4. border: 1px solid var(--glass-border)              ← top/left light edge
5. box-shadow: var(--shadow-md)                        ← depth
6. ::before pseudo — subtle shimmer gradient           ← gloss
```

**The shimmer pseudo-element:**
```
::before {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(
	135deg,
	rgba(255, 255, 255, 0.06) 0%,
	transparent 50%,
	rgba(255, 255, 255, 0.02) 100%
  );
  border-radius: inherit;
  pointer-events: none;
}
```

This is what makes a panel feel like glass rather than a translucent rectangle.

---

## 5. Shape & Geometry

### 5.1 Border Radius Scale

Vapor uses generous, consistent rounding. Sharp corners are not part of this language.

| Token | Value | Usage |
|---|---|---|
| `--radius-xs` | `6px` | Badges, small chips, tooltips |
| `--radius-sm` | `10px` | Input fields, small buttons |
| `--radius-md` | `16px` | Cards, panels, standard surfaces |
| `--radius-lg` | `24px` | Large cards, drawer sheets |
| `--radius-xl` | `32px` | Modal overlays, full-panel components |
| `--radius-2xl` | `48px` | Now Playing card (full-screen) |
| `--radius-pill` | `9999px` | Progress bars, toggle switches, tags |
| `--radius-circle` | `50%` | Album art, avatar icons |

### 5.2 Geometry Rules

- **Album art is always circular** in the mini-player and queue list. It is **square with `--radius-lg`** in the library grid and **square with `--radius-2xl`** in the full-screen Now Playing view.
- **No square buttons.** All icon-only buttons are pill or circle shaped.
- **No hairline borders** (0.5px) — they disappear or look broken on non-retina displays. Minimum visible border is `1px`.
- Waveform visualizer uses **rounded bar tops** (`border-radius: 2px` on each bar) — never sharp bar charts.

---

## 6. Motion & Animation

### 6.1 The Motion Philosophy

Animation in Vapor serves one purpose: **communicating physicality**. A card lifting. Glass sliding. A glow breathing. Motion should feel like something with mass and inertia — not a CSS transition that a framework applied automatically.

**Do not animate for decoration.** Every motion must answer: *what physical property does this simulate?*

### 6.2 Easing Curves

| Token | Curve | CSS Value | Usage |
|---|---|---|---|
| `--ease-glass` | Custom spring | `cubic-bezier(0.34, 1.56, 0.64, 1)` | Elements entering the scene (slight overshoot = spring) |
| `--ease-settle` | Decelerate | `cubic-bezier(0.0, 0.0, 0.2, 1)` | Elements leaving the scene, drawers closing |
| `--ease-lift` | Standard | `cubic-bezier(0.4, 0.0, 0.2, 1)` | Hover state transitions, opacity changes |
| `--ease-pulse` | Sinusoidal | `ease-in-out` | Breathing animations, ambient pulse |

### 6.3 Duration Scale

| Token | Value | Usage |
|---|---|---|
| `--duration-instant` | `80ms` | Micro-interactions (press state, icon swap) |
| `--duration-fast` | `160ms` | Hover states, badge changes, color shifts |
| `--duration-normal` | `240ms` | Panel transitions, card reveals |
| `--duration-slow` | `400ms` | Drawer open/close, modal entrance |
| `--duration-atmospheric` | `800–1200ms` | Album art transition, dynamic palette shift, screen changes |

### 6.4 Signature Animations

#### Glass Slide-In (Drawers, Sheets)
Panels enter by sliding in from their natural edge AND fading from `opacity: 0` AND blurring in from a slightly lower `backdrop-blur`. All three simultaneously. This sells the "the glass panel materialized" feel.

```
enter: translateY(24px) → translateY(0) + opacity 0 → 1 + blur 0px → 24px
duration: 400ms, ease: --ease-glass
```

#### Glow Pulse (AI DJ Active, Sync in Progress)
The active state glow breathes using a CSS animation, not a static `box-shadow`. The glow scales between 60% and 100% opacity on a 2.4-second loop.

```
@keyframes glow-pulse {
  0%, 100% { box-shadow: 0 0 20px rgba(61, 214, 200, 0.20); }
  50%       { box-shadow: 0 0 40px rgba(61, 214, 200, 0.55), 0 0 80px rgba(61, 214, 200, 0.20); }
}
duration: 2.4s, ease: ease-in-out, iteration: infinite
```

#### Track Transition Crossfade (Visual)
When the track changes in Now Playing, the album art doesn't instantly swap. It fades to a slightly blurred, desaturated ghost of the previous art while the new art scales in from `scale(0.94)` to `scale(1.00)`. The dynamic palette bloom shifts simultaneously at `1200ms`.

#### Button Press Feedback
Buttons depress on press — not just a color change. Use `transform: scale(0.96)` at `80ms` `--ease-settle`, releasing back to `scale(1.00)` at `160ms` `--ease-glass`. This simulates physical depression.

#### Waveform Bars
The waveform visualizer bars animate using a staggered CSS animation with a `calc(var(--bar-index) * 30ms)` delay per bar, creating a left-to-right wave ripple rather than all bars moving in unison.

---

## 7. Iconography

### 7.1 Icon Style

- Source library: **Phosphor Icons** (preferred) — they have a consistent rounded weight that matches Inter and the Vapor aesthetic. Use the `Regular` weight for standard icons, `Fill` weight for active/selected states.
- Backup: **Lucide** icons for any gaps in Phosphor
- **Never mix icon libraries** within the same screen
- All icons in the navigation and controls use `24px` × `24px` artboard; micro-icons in metadata use `16px` × `16px`

### 7.2 Icon Color Rules

- Inactive icons: `--text-tertiary`
- Hover: `--text-secondary`
- Active / Selected: `--accent-core` (with `--accent-glow` applied as `filter: drop-shadow`)
- Destructive (delete): `--semantic-error`
- Never use icon-only buttons without a tooltip or visible label at first use

### 7.3 The Play/Pause Button

The central play/pause button in the Now Playing panel is a signature element. It receives special treatment:

- **Size:** 64px × 64px (mini-player), 80px × 80px (full Now Playing)
- **Background:** Circular glass panel with `--accent-surface` tint
- **Active glow:** `--shadow-glow-accent` applied as `box-shadow`
- **Icon:** Phosphor `Play` fill / `Pause` fill, 28px, `--accent-bright`
- **Press animation:** `scale(0.90)` → `scale(1.00)` with spring easing

---

## 8. Spacing System

### 8.1 Base Unit

All spacing is based on a **4px grid**. Every margin, padding, and gap value must be a multiple of 4.

| Token | Value | Usage |
|---|---|---|
| `--space-1` | `4px` | Micro gaps, icon-to-label spacing |
| `--space-2` | `8px` | Tight internal padding (badges, chips) |
| `--space-3` | `12px` | Standard component internal padding |
| `--space-4` | `16px` | Default card padding (compact) |
| `--space-5` | `20px` | Standard element gap |
| `--space-6` | `24px` | Card padding (comfortable) |
| `--space-8` | `32px` | Section gaps, large panel padding |
| `--space-10` | `40px` | Screen edge margins (mobile) |
| `--space-12` | `48px` | Large section separators |
| `--space-16` | `64px` | Major layout divisions |

### 8.2 Component-Specific Spacing

- **Glass card padding:** `--space-6` (24px) standard, `--space-4` (16px) compact list mode
- **Navigation bar height:** 64px desktop, 72px mobile (accounts for safe area)
- **Now Playing mini-player height:** 72px
- **Bottom sheet handle to content gap:** `--space-4` (16px)
- **Touch target minimum:** 44px × 44px (Apple HIG / WCAG compliance)

---

## 9. Component Patterns

### 9.1 Glass Card

The foundational building block of the Vapor UI.

**Anatomy:**
```
┌─────────────────────────────────────┐  ← glass-border (top)
│  ::before shimmer gradient           │
│                                     │
│   [content]                         │
│                                     │
└─────────────────────────────────────┘
		 ↑ box-shadow: --shadow-md
		 ↑ backdrop-filter: blur(24px)
```

**States:**
- Default: `--bg-glass` + `--glass-tint`
- Hover: brightness increases by ~5%, border shifts to `rgba(255,255,255,0.18)`
- Selected: background shifts to `--accent-surface`, border shifts to `rgba(123, 110, 246, 0.30)`
- Pressed: `scale(0.98)`, `80ms`

### 9.2 Progress / Scrubber Bar

The playback scrubber is not a browser default `<input type="range">`. It is a custom component:

- **Track:** Full width, height `4px`, background `rgba(255,255,255,0.12)`, `border-radius: --radius-pill`
- **Filled:** `--accent-core` gradient: `linear-gradient(90deg, --accent-dim, --accent-bright)`
- **Thumb:** `14px` circle, `--accent-bright`, `--shadow-glow-accent`, appears on hover only (invisible at rest)
- **Buffered region:** `rgba(255,255,255,0.08)` behind the fill track
- **Hover interaction:** Track height expands from `4px` to `6px` over `160ms` `--ease-lift`

### 9.3 Now Playing — Full Screen

The crown jewel screen. This is where all design language elements converge.

**Layout zones (top to bottom):**
1. **Background** — Album art blurred to `80px`, scaled to cover, dimmed to 15% opacity. Dynamic palette bloom radial gradient layered on top.
2. **Glass panel** — Centered, `--radius-2xl`, `--glass-blur-heavy` (40px), contains all controls
3. **Album art** — Square with `--radius-xl`, `--shadow-lg` + `--shadow-glow-accent` subtle
4. **Track title** — Outfit Light, `--type-display`, `--text-primary`, `-0.02em` letter-spacing
5. **Artist / Album** — Inter Regular, `--type-lg`, `--text-secondary`
6. **Waveform visualizer** — Full width, 48px height, aqua bars with `--aqua-glow`
7. **Scrubber** — custom component (see §9.2)
8. **Transport controls** — Previous, Play/Pause, Next, with secondary row (shuffle, repeat, queue, love)
9. **Ambient metadata strip** — BPM, Key, Energy level in JetBrains Mono, `--text-tertiary`

### 9.4 Navigation — Sidebar (Desktop) / Tab Bar (Mobile)

**Desktop Sidebar:**
- Width: `240px` collapsed to icon-only `68px` on hover out
- Background: `--bg-base` (not glass — it is the ground plane)
- Active item: `--accent-surface` background, `--accent-core` icon + label
- App wordmark at top in Outfit SemiBold

**Mobile Tab Bar:**
- Floating glass pill, not a full-width bar. Sits 16px above the bottom safe area inset.
- Contains: Library, Search, Now Playing, Queue, Settings
- Blur: `--glass-blur-heavy` — it hovers over content

### 9.5 Mini Player (Persistent)

Always visible when a track is loaded, regardless of what screen the user is on. Slides up from the bottom (desktop: from the sidebar footer; mobile: above the tab bar).

- Height: `72px`
- Glass panel, `--radius-lg`
- Left: Album art (40px circle) + Track title + Artist
- Center: Play/Pause only
- Right: Like button + queue indicator

---

## 10. States & Feedback

### 10.1 Loading States

No spinners. Vapor uses **skeleton screens** — ghost placeholders with a subtle shimmer animation:

```
background: linear-gradient(
  90deg,
  rgba(255,255,255,0.04) 0%,
  rgba(255,255,255,0.10) 50%,
  rgba(255,255,255,0.04) 100%
);
background-size: 200% 100%;
animation: shimmer 1.6s ease-in-out infinite;
```

Skeleton shapes match exactly the layout of the content they replace.

### 10.2 Empty States

Empty states are not just "no results found" text. They use:
- A large (80px) Phosphor icon in `--text-tertiary`
- A two-line message: heading in `--type-lg` Inter SemiBold, body in `--type-sm` `--text-secondary`
- An optional CTA button (glass card style, `--accent-core` border)
- Subtle background illustration: a very low-opacity (4%) geometric waveform or music note

### 10.3 Toast Notifications

- Position: Top-center, 16px below the safe area / navigation bar
- Width: fit-content, max 380px
- Style: Glass card, `--radius-md`, with a left accent bar in the semantic color
- Animation: Slide down from `translateY(-16px)` + `opacity: 0` → rest position, `--ease-glass`, `240ms`
- Auto-dismiss: `4000ms` with a progress bar across the bottom of the toast depleting in real time

---

## 11. Accessibility

Glassmorphism can conflict with accessibility requirements if not carefully managed. Vapor is committed to meeting **WCAG 2.1 AA** as a minimum.

### Rules

1. **Text contrast**: All `--text-primary` on `--bg-glass` panels must meet 4.5:1 contrast ratio. Use the `--text-primary` token only — do not use alpha-based text colors on glass for body text.
2. **Focus indicators**: Custom focus rings use `--accent-bright` at `2px` solid, with `3px` offset. Never suppress the focus ring.
3. **Touch targets**: Minimum 44×44px for all interactive elements, including the waveform scrubber thumb.
4. **Reduce Motion**: All animations respect `prefers-reduced-motion`. When active, transitions drop to `opacity` changes only (no transforms, no scale, no blur transitions).
5. **Screen reader support**: All icon-only buttons have `aria-label`. The Now Playing region is a live region (`aria-live: polite`) so track changes are announced.
6. **Blur performance**: `backdrop-filter` is expensive. Provide a reduced-quality mode (controlled via settings) that replaces `backdrop-filter: blur()` with a flat semi-transparent background for lower-end devices.

---

## 12. Platform Adaptations

### Desktop (Windows / macOS / Linux)

- Uses native window chrome where available (macOS: native traffic lights, Windows: custom titlebar matching `--bg-base`)
- Sidebar navigation model
- Hover states are primary — the design assumes a cursor
- Right-click context menus follow the Popover pattern (§9.3 — glass card with `--radius-md`)
- Keyboard shortcuts displayed in tooltips using `--type-xs` JetBrains Mono

### Mobile (iOS / Android)

- Tab bar navigation model (floating glass pill)
- No hover states — press states replace them entirely
- Bottom sheet drawers for all secondary panels (Queue, EQ, Track Info)
- Swipe-to-dismiss on sheets, swipe-left on queue items to remove
- The Now Playing screen is triggered by tapping the mini-player; it expands with a `hero` animation — the mini-player card morphs into the full-screen panel (`400ms`, `--ease-glass`)
- Safe area insets respected via Godot's `DisplayServer` safe area API

### Responsive Breakpoints

| Breakpoint | Width | Layout |
|---|---|---|
| `xs` | < 480px | Single-column mobile |
| `sm` | 480–768px | Mobile landscape / tablet portrait |
| `md` | 768–1080px | Tablet landscape / small desktop |
| `lg` | 1080–1440px | Standard desktop |
| `xl` | > 1440px | Wide desktop — sidebar widens, album grid gains columns |

---

## 13. Godot-Specific Implementation Notes

### Rendering

- Use **Compatibility renderer** for mobile (GLES3) and **Forward+** for desktop for best glass shader support
- Frosted glass panels are implemented as Godot `SubViewport` nodes with a blur `ShaderMaterial` applied — not CSS `backdrop-filter` (this is a native app)
- Album art color extraction runs in a background `Thread` at track load time — never on the main thread
- Waveform visualizer uses a custom `Control` node drawing `RoundedRect` shapes per bar via `_draw()`

### Theme System

- All color tokens map to Godot `Theme` resource properties
- Dynamic palette shift is implemented as `Tween` animations on `CanvasModulate` or `ShaderMaterial` uniform values
- Font resources: embed Inter Variable TTF and Outfit TTF as `DynamicFont` resources in the project

### Performance Budget

| Effect | Budget |
|---|---|
| Frosted glass panels (simultaneous) | Max 4 active `SubViewport` blur passes |
| Waveform bars | Max 128 bars, 60fps target |
| Album art blur (background) | Pre-rendered at 1/4 resolution, cached |
| Dynamic palette tween | Max 3 simultaneous color property tweens |

---

## Appendix A — Quick Reference Card

```
TYPEFACES
  Display:    Outfit (300, 400, 600, 700)
  UI:         Inter (400, 500, 600)
  Technical:  JetBrains Mono (400, 500)

ACCENTS (single-accent system)
  Primary (Dark):   #0A84FF  (Apple Dark System Blue)
  Primary (Light):  #007AFF  (Apple Light System Blue)

TEXT — DARK MODE
  Primary:    #FFFFFF  (full opacity)
  Secondary:  #FFFFFF  at 60%  → #999999
  Tertiary:   #FFFFFF  at 35%

TEXT — LIGHT MODE
  Primary:    #262626  (near-black, 85% black)
  Secondary:  #8E8E93  (Apple cool medium grey)
  Tertiary:   #8E8E93  at 60%

GLASS — DARK MODE
  Base:       rgba(30, 30, 30, 0.55)  blur(24px)
  Border:     rgba(255, 255, 255, 0.15)
  Shimmer:    135deg gradient, 0%→5% white

GLASS — LIGHT MODE
  Base:       rgba(255, 255, 255, 0.50)  blur(24px)
  Border:     rgba(255, 255, 255, 0.80)
  Lower edge: rgba(0, 0, 0, 0.06)

RADIUS
  Standard card:  16px
  Large panel:    24–32px
  Now Playing:    48px
  Progress bars:  9999px (pill)

MOTION
  Fast:    160ms  cubic-bezier(0.4,0.0,0.2,1)
  Normal:  240ms  cubic-bezier(0.4,0.0,0.2,1)
  Spring:  240ms  cubic-bezier(0.34,1.56,0.64,1)
  Slow:    400ms  cubic-bezier(0.34,1.56,0.64,1)
```

---

*This document is a living specification. Any deviation from these tokens or patterns requires a documented design decision logged in the changelog below.*

## 14. Regression Guards

Rules born from real regressions. Each was violated once; do not violate them again.

### 14.1 The sidebar's right edge is square. Always.

The seek bar runs flush along the sidebar's right edge, top to bottom. Rounded right corners break that continuous line — the bar visibly detaches at the curves. Only the sidebar's **left** corners follow the window-frame radius (`RADIUS_LG`).

- StyleBox: `corner_radius_top_right = 0`, `corner_radius_bottom_right = 0` (enforced in `sidebar._apply_panel_style`).
- Glass shader: `premium_glass.gdshader` takes per-corner `corner_radii = (tl, tr, br, bl)`; the sidebar sets `(RADIUS_LG, 0, 0, RADIUS_LG)`. Never set a uniform radius on the sidebar's material.

### 14.2 Hover controls live INSIDE the row highlight

A hover-revealed control (✕ delete, drag handle, etc.) must:
- span the **full row height** (anchor top 0 → bottom 1) so its glyph centers on the row's text baseline, and
- sit **inset from the row's rounded edge** (≥ 6 px), never outside or clipped by the highlight stylebox.

Anchoring a fixed-size child with a center preset before its size is known produces the misaligned floating glyph this rule exists to prevent.

**Glyphs must exist in the app font.** A character the UI font lacks (the header's old "↕") silently falls back to an OS font with different metrics and renders on a shifted baseline, visibly misaligned beside its own text. Before using any symbol, confirm the app font has it; otherwise compose it from characters that do (the sortable indicator is two stacked in-font ▲▼ minis) or draw it. A symbol and its adjacent text share a baseline — always.

### 14.3 Inline edits keep the row's metrics

Renaming or creating an item inline replaces the row's *content*, never its *shape*. The `LineEdit` gets the row's exact height and compact content margins (see `sidebar._style_inline_edit`). If starting an edit makes the surrounding menu move, the edit is wrong.

### 14.4 Modals are glass, not native — and dims live inside the frame

Confirmations and dialogs use the in-app pattern: dim (`black @ 45%`), centered glass panel (`make_glass_panel(RADIUS_MD, 0.92)`), display-font title, `TEXT_SECONDARY` body, right-aligned actions (quiet Cancel, CTA-styled destructive/confirm). Godot's `ConfirmationDialog`/`AcceptDialog` render OS-default chrome and must not appear in the app. Backdrop click cancels.

The dim overlay parents to **`AppWindowFrame`** with the frame's corner radius (`RADIUS_LG`) — parented to the scene root it paints the window's square bounds onto the desktop, revealing the transparent margins around the rounded frame.

**Exception — file browsing is the OS's job.** `FileDialog` always sets `use_native_dialog = true`. We don't restyle file pickers; the platform's own browser is the correct UI.

### 14.5 The sidebar preview square is the output monitor

It always shows the image of whatever was focused last — artist, album, track (with lyrics overlay), or playlist. Anything that gains focus feeds it; nothing else hides it. Its minimum height is set in code (`SIDEBAR_WIDTH − 32`) because the texture's ignore-size expand mode reports a zero minimum — removing that minimum collapses the slot invisibly.

### 14.6a Contextual pickers are glass, no dim, positioned at the trigger

A CONTEXTUAL popup (right-click / long-press "Add to Playlist" or "Add to Group") is not a modal decision (§14.4) — it doesn't dim the screen, and it opens next to the thing you clicked/pressed, not centered. Reuse `add_to_picker.gd`'s pattern for anything in this category: plain undecorated `Control` backdrop (no `StyleBoxFlat` dim, mirrors `playlist_popup.tscn`'s own `Backdrop`) purely to catch outside-clicks, a glass `PanelContainer` positioned near `at_position` and clamped inside the viewport after one layout pass (`await get_tree().process_frame` — `panel.size` is a zero-rect before that). If it needs to darken the world and force a choice, it's a `GlassModal`, not this.

### 14.6b GDScript closures cannot resolve a self-reference — use a bound method instead

A "rebuild a list, where clicking a row triggers another rebuild" pattern is common in this app (selection checkboxes, the entity browser, "Add to…" pickers). The natural GDScript expression — `var rebuild: Callable; rebuild = func(): ...; row.pressed.connect(func(): ...; rebuild.call())` — **compiles and runs, but silently does nothing on the second click.** A lambda captures its free variables' values at the lambda's OWN creation moment; for a self-reference, that moment is *before* the assignment finishes, so the captured `rebuild` is an empty `Callable()`, not an error. Aliasing it into a shallower-nested local does not fix this — the underlying issue is the self-reference, not the nesting depth.

The fix: give the rebuildable list a tiny inner `class` (`extends RefCounted`) with `rebuild()` as a real method, and have rows call a bound method (`row.pressed.connect(_on_row_pressed.bind(item))`) instead of a captured Callable. `self` inside a method is never subject to this timing problem. Keep the controller instance alive with `backdrop.set_meta("controller", instance)` — nothing else references it, so it would otherwise be freed the moment the building function returns. See `add_to_picker.gd`'s `_Picker` and `dynamic_group_screen.gd`'s `_EntityBrowser` for the pattern. This one is expensive to debug (no parse error, no exception until the *second* interaction, and the failure is silent) — reach for the inner-class pattern from the start for any rebuild-on-click list.

### 14.6 Every affordance works narrow and on touch — check BOTH axes

`PlatformManager` defines two orthogonal axes and its header says not to conflate them; §14.6 exists because we did anyway:

- **Layout** follows *width* (`is_mobile_layout()` / `should_show_sidebar()`), never the device badge. A narrowed desktop window stacks rows; a landscape phone gets columns. Components re-render on `layout_changed`.
- **Affordances** follow *input hardware* (`is_touch_primary()`). Hover-revealed controls (row ✕, sidebar ✕, preview-slot chip) are always visible on touch.

Every control removed from or hidden in one layout needs a home in the other. Current mappings: column-header sorting ⇄ toolbar sort chip + direction toggle (narrow); sidebar rename/delete/title ⇄ playlist/dynamic-group compact strip (narrow, `should_show_sidebar()` false); glass confirm modal = shared `GlassModal` (both); **sidebar Playlists + Dynamic Groups lists ⇄ the single mobile popup (`playlist_popup.gd`)** — the popup renders both sections (a shared inline-creation `LineEdit` distinguishes target via a `"creating"` meta key: `"playlist"` or `"group"`), since mobile has no sidebar at all and nothing else opens a playlist or dynamic group screen on that layout. Verify narrow layouts on desktop by shrinking the window; add `VAPOR_FORCE_TOUCH=1` for touch affordances.

### 14.7 Honest metadata rendering

Verified metadata renders normal; filename/folder guesses render dimmed; unknowns render as "—". The literal strings "Unknown Artist"/"Unknown Album" never appear in a table cell, and percent-encoded text never reaches the screen.

## Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-21 | 1.0 | Initial design language established |
| 2026-06-02 | 1.1 | Migrated to Apple Glassmorphism palette. Single blue accent (#007AFF / #0A84FF). Retired dual-accent (Aurora + Aqua) in default themes. Updated all colour tables, Appendix A, and §2.3 light mode. |
| 2026-07-20 | 1.2 | Added §14 Regression Guards: square sidebar right edge, hover controls inside row highlight, inline-edit metrics, glass modals, output-monitor preview slot, honest metadata rendering. |
| 2026-07-20 | 1.3 | §14.4: modal dim parents to AppWindowFrame; native OS file browser exception. New §14.6: two-axis mobile rule (layout by width, affordances by input) with the narrow/touch equivalence mappings. |
| 2026-07-22 | 1.4 | §14.6: Dynamic Groups' mobile path added to the mapping table — the mobile popup now renders both Playlists and Dynamic Groups sections rather than leaving the new sidebar feature unreachable on narrow layouts. |
| 2026-07-22 | 1.5 | New §14.6a (contextual pickers: glass, no dim, positioned at trigger — distinct from §14.4 modals) and §14.6b (GDScript self-referencing closures silently no-op; use a bound method on a small inner class instead). Both codify bugs caught by the "Add to Playlist/Group" picker's own verification. |
