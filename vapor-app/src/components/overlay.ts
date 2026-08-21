/**
 * Where a viewport overlay is mounted.
 *
 * ## Why overlays cannot render where they are written
 *
 * `position: fixed` is only relative to the viewport while no ancestor
 * establishes a containing block for it — and `transform`, `filter`,
 * `perspective`, `contain: paint` and **`backdrop-filter`** all do.
 *
 * `.glass` is `backdrop-filter: blur(30px) saturate(1.7)`, and it is on almost
 * every surface in the app. So a `position: fixed; inset: 0` backdrop written
 * inside a card was not covering the window at all: measured inside the sync
 * card it came out 962×409 at (289, 109) in a 1280×720 window — the card's own
 * box. It looked like a scrim with square corners sitting on a rounded card,
 * because that is exactly what it was.
 *
 * Mounting on `<body>` puts every overlay outside all of it. The alternative —
 * being careful about which components sit inside a glass card — is a rule that
 * holds until the next time something is moved.
 */
export function overlayRoot(): HTMLElement {
  return document.body;
}
