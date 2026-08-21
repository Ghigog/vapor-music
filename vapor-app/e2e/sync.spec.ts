/**
 * Pairing two devices, through the real UI.
 *
 * ## The pairing field has to hold all six digits of a code
 *
 * It used to be `9ch` wide, which sounds like plenty and was not: `ch` is the
 * advance of "0" and knows nothing about the 0.18em of `letter-spacing`, and
 * `border-box` takes the padding out of the declared width too. Six digits
 * needed ~91px and had 75, so the code scrolled inside its own field and you
 * could see four of what you had typed.
 *
 * This pins the fix by measurement rather than by arithmetic, and pins the
 * headroom as well: the width is stated in `ch`, which is read off whatever
 * font is loaded, so the number that matters is how wide a digit can get
 * before six of them stop fitting. JetBrains Mono sits at 0.6em and the
 * ceiling is 0.707em, which clears every stock monospace the app could fall
 * back to while the webfont is still in flight.
 */
import { expect, test } from "@playwright/test";
import { boot } from "./harness";

test("the pairing code fits six digits, with room for a wider fallback", async ({ page }) => {
  await boot(page, {
    syncEnabled: true,
    peers: [{ id: "px", name: "Pixel 9 · Vapor", address: "192.168.100.9:7040", kind: "phone", lastSeen: 0 }],
  });
  await page.getByRole("button", { name: /settings/i }).first().click();
  await page.getByRole("button", { name: /enter a code/i }).first().click();

  const field = page.locator(".sync__code");
  await field.fill("997215");
  await expect(field).toHaveValue("997215");

  const fit = await field.evaluate((el: HTMLInputElement) => {
    const cs = getComputedStyle(el);
    const inner = el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const ls = parseFloat(cs.letterSpacing);
    const digit = (family: string) => {
      const q = document.createElement("span");
      q.style.cssText = `position:absolute;visibility:hidden;white-space:pre;font-size:${cs.fontSize};font-family:${family};letter-spacing:0`;
      q.textContent = "0000000000";
      document.body.appendChild(q);
      const w = q.getBoundingClientRect().width / 10;
      q.remove();
      return w;
    };
    return {
      overflowing: el.scrollWidth > el.clientWidth,
      // Six advances plus the five gaps between them. The sixth gap trails the
      // last digit and is dead space, so it does not have to fit.
      maxAdvanceEm: (inner - 5 * ls) / 6 / parseFloat(cs.fontSize),
      usedAdvanceEm: digit(cs.fontFamily) / parseFloat(cs.fontSize),
    };
  });

  expect(fit.overflowing, "the code must not scroll inside its own field").toBe(false);
  expect(fit.usedAdvanceEm).toBeLessThan(fit.maxAdvanceEm);
  // Stock monospace faces are 0.6em; hold the ceiling above that so a fallback
  // during the first paint cannot clip the code.
  expect(fit.maxAdvanceEm, "room for a 0.65em fallback digit").toBeGreaterThan(0.65);
});
