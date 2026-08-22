/**
 * The focus trap, tested through a real dialog rather than in isolation.
 *
 * Four overlays declared `aria-modal="true"` and none of them held focus, so
 * Tab walked out of the dialog and into the library behind it — which is still
 * rendered and still clickable. Nothing catches that except a test that
 * actually presses Tab: the attribute is right there in the markup, so every
 * accessibility check that looks at attributes passes.
 *
 * `Confirm` stands in for all four. They share `useDialog`, and this is the one
 * with a decision behind its initial focus worth asserting.
 */
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Confirm } from "./Confirm";

function open(overrides: Partial<Parameters<typeof Confirm>[0]> = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <Confirm
      title="Delete everything?"
      body="This cannot be undone."
      confirmLabel="Delete"
      destructive
      onConfirm={onConfirm}
      onCancel={onCancel}
      {...overrides}
    />,
  );
  return { onConfirm, onCancel };
}

describe("a modal dialog holds focus", () => {
  it("opens on Cancel, not on the destructive button", async () => {
    open();
    // A dialog that appears under someone's hands must not have Return wired
    // to the irreversible option.
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("wraps forward from the last control to the first", async () => {
    open();
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const confirm = screen.getByRole("button", { name: "Delete" });

    await userEvent.tab();
    expect(confirm).toHaveFocus();

    // Without the trap this lands somewhere in the page behind the dialog.
    await userEvent.tab();
    expect(cancel).toHaveFocus();
  });

  it("wraps backward from the first control to the last", async () => {
    open();
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const confirm = screen.getByRole("button", { name: "Delete" });

    expect(cancel).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(confirm).toHaveFocus();
  });

  it("closes on Escape", async () => {
    const { onCancel } = open();
    await userEvent.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalled();
  });

  it("brings focus back inside if it has escaped", async () => {
    open();
    // Simulates focus having ended up outside — a click on the page behind, or
    // a control that removed itself while focused.
    (document.activeElement as HTMLElement)?.blur();
    expect(document.activeElement).toBe(document.body);

    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });
});
