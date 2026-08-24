/**
 * The supporter wall.
 *
 * What is worth testing here is not that pins render — it is the three rules
 * that keep this a gift rather than a purchase, and the one that keeps a
 * half-configured build from shipping a dead link:
 *
 * 1. Everybody sees the same wall. There is no "your" pin and no path by which
 *    this component could learn who paid, because it takes a count and nothing
 *    else.
 * 2. Nothing is gated on it.
 * 3. It says nothing it cannot support — the count is hand-written before a
 *    release, so it is always at least slightly behind.
 */
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SupporterPins } from "./SupporterPins";
import { kofiUrl } from "../lib/supporters";

/** The pins themselves, which are decorative and therefore hidden from a11y. */
function pins() {
  return document.querySelectorAll(".pins__pin");
}

describe("The supporter wall", () => {
  it("draws one pin per supporter", () => {
    render(<SupporterPins count={7} handle="someone" />);
    expect(pins()).toHaveLength(7);
  });

  it("says how many there are in words, not only in pictures", () => {
    render(<SupporterPins count={7} handle="someone" />);
    // The pins are `aria-hidden`, so a screen reader hears the sentence or it
    // hears nothing at all.
    expect(screen.getByText(/7 people have chipped in/i)).toBeInTheDocument();
  });

  it("counts one person in the singular", () => {
    render(<SupporterPins count={1} handle="someone" />);
    expect(screen.getByText(/one person has chipped in/i)).toBeInTheDocument();
    expect(pins()).toHaveLength(1);
  });

  /*
   * The rule from DECISIONS.md §2, asserted rather than trusted to the copy.
   *
   * The wall is the same for a person who has donated and a person who has
   * not, because this component is handed a project-wide number and has no
   * notion of the viewer at all. A test that could tell the two apart would
   * mean an identity had been introduced.
   */
  it("says plainly that a donation changes nothing about the app", () => {
    render(<SupporterPins count={3} handle="someone" />);
    expect(
      screen.getByText(/nothing here is bought, unlocked or withheld/i),
    ).toBeInTheDocument();
  });

  it("stops drawing discs long before the wall becomes a texture", () => {
    render(<SupporterPins count={500} handle="someone" />);
    // Capped, and the remainder is stated rather than dropped.
    expect(pins().length).toBeLessThan(500);
    expect(screen.getByText(/and 440 more/i)).toBeInTheDocument();
    expect(screen.getByText(/500 people have chipped in/i)).toBeInTheDocument();
  });

  /*
   * A build where the handle has not been filled in yet.
   *
   * Rendering the card anyway would put a link to `ko-fi.com/` in front of
   * somebody. Half-configured has to look like absent, not like broken.
   */
  it("renders nothing at all without a Ko-fi handle and without supporters", () => {
    const { container } = render(<SupporterPins count={0} handle="" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("never links anywhere when the handle is blank", () => {
    // Supporters but no handle — the wall is worth showing, the dead link is
    // not.
    render(<SupporterPins count={4} handle="   " />);
    expect(pins()).toHaveLength(4);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("points at the right page when there is one", () => {
    render(<SupporterPins count={0} handle="vapormusic" />);
    expect(screen.getByRole("link", { name: /ko-fi/i })).toHaveAttribute(
      "href",
      "https://ko-fi.com/vapormusic",
    );
    // And says so honestly while the wall is empty.
    expect(screen.getByText(/nobody has chipped in yet/i)).toBeInTheDocument();
  });

  /** A negative count is nonsense arriving from a typo in a release edit. */
  it("draws nothing rather than crashing on a nonsense count", () => {
    render(<SupporterPins count={-3} handle="someone" />);
    expect(pins()).toHaveLength(0);
  });
});

describe("kofiUrl", () => {
  it("is null for a handle nobody has filled in", () => {
    expect(kofiUrl("")).toBeNull();
    expect(kofiUrl("   ")).toBeNull();
  });

  it("builds the page URL from a handle", () => {
    expect(kofiUrl("vapormusic")).toBe("https://ko-fi.com/vapormusic");
  });
});
