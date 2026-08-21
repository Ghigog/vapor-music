/**
 * Overlays escape whatever they were written inside.
 *
 * `position: fixed` stops meaning "the viewport" as soon as an ancestor
 * establishes a containing block — and `backdrop-filter` does, which `.glass`
 * carries on almost every surface in the app. A `Confirm` rendered inside the
 * sync card measured 962×409 at (289, 109) in a 1280×720 window: the card's own
 * box, with square corners, sitting on a rounded card.
 *
 * jsdom has no layout, so this asserts the thing that *causes* correct layout —
 * that the overlay is mounted on `<body>` and not inside the card — which is
 * the part a refactor would break.
 */
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Confirm } from "./Confirm";
import { Modal } from "./HelpModal";

/** A card with the property that traps fixed positioning. */
function Glass({ children }: { children: React.ReactNode }) {
  return (
    <section className="settings__card glass" data-testid="card">
      {children}
    </section>
  );
}

describe("overlays mounted inside a glass card", () => {
  it("puts a confirmation on the body, not in the card", () => {
    render(
      <Glass>
        <Confirm
          title="Delete it?"
          body="Nothing else goes."
          onConfirm={() => {}}
          onCancel={() => {}}
        />
      </Glass>,
    );

    const dialog = screen.getByRole("alertdialog");
    const backdrop = dialog.parentElement;
    expect(backdrop?.parentElement).toBe(document.body);
    expect(screen.getByTestId("card")).not.toContainElement(dialog);
  });

  it("puts a modal on the body, not in the card", () => {
    render(
      <Glass>
        <Modal title="Licences" onClose={() => {}}>
          <p>Attributions</p>
        </Modal>
      </Glass>,
    );

    const dialog = screen.getByRole("dialog");
    expect(dialog.parentElement?.parentElement).toBe(document.body);
    expect(screen.getByTestId("card")).not.toContainElement(dialog);
  });
});
