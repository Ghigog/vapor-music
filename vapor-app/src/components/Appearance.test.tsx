/**
 * The appearance control.
 *
 * The three things worth pinning down are the three that would each be an
 * invisible regression: that the theme changes on the press rather than on the
 * round trip, that `auto` leaves the root alone so the OS query keeps binding,
 * and that a failed save puts the choice back instead of leaving the screen
 * claiming something the next launch will contradict.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useBackend, setPrefersDark } from "../test/setup";
import { AppearanceControl } from "./Appearance";
import { adoptAppearance } from "../lib/theme";

function theme(): string | null {
  return document.documentElement.getAttribute("data-theme");
}

describe("choosing a theme", () => {
  it("shows the three choices with the stored one selected", () => {
    useBackend();
    adoptAppearance("lamplight");
    render(<AppearanceControl />);

    expect(screen.getByRole("radio", { name: /daylight/i })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(screen.getByRole("radio", { name: /lamplight/i })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("radio", { name: /auto/i })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    adoptAppearance("auto");
  });

  it("puts the theme on the root and saves it", async () => {
    const backend = useBackend();
    render(<AppearanceControl />);

    await userEvent.click(screen.getByRole("radio", { name: /lamplight/i }));

    expect(theme()).toBe("lamplight");
    await waitFor(() => expect(backend.called("set_appearance")).toBe(true));
    await waitFor(async () => {
      const saved = await backend.invoke<{ theme: string }>("settings");
      expect(saved.theme).toBe("lamplight");
    });
    adoptAppearance("auto");
  });

  /* Auto is the absence of the attribute, not a third value written into it —
     that is what lets the media query in tokens.css keep working. */
  it("clears the root attribute when told to follow the machine", async () => {
    useBackend({ appearance: "lamplight" });
    adoptAppearance("lamplight");
    render(<AppearanceControl />);

    await userEvent.click(screen.getByRole("radio", { name: /auto/i }));

    expect(theme()).toBeNull();
  });

  it("still reports the machine's theme to the mark under auto", async () => {
    useBackend();
    render(<AppearanceControl />);
    setPrefersDark(true);

    await waitFor(() =>
      expect(screen.getByRole("radio", { name: /auto/i })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(theme()).toBeNull();
  });

  it("puts the choice back when the save fails, and says why", async () => {
    const backend = useBackend();
    backend.fail("set_appearance", "Settings are read-only.");
    render(<AppearanceControl />);

    await userEvent.click(screen.getByRole("radio", { name: /lamplight/i }));

    await waitFor(() => expect(theme()).toBeNull());
    expect(screen.getByRole("radio", { name: /auto/i })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByText(/read-only/i)).toBeInTheDocument();
  });
});
