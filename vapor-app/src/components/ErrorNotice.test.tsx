/**
 * The error surface, and the one thing it must never do.
 *
 * A notice is a claim. A red bar claims something failed, a green one claims
 * something worked, and both are worthless the moment either can appear
 * without the thing it describes having happened.
 *
 * `ErrorNotice` used to render whatever it was handed, including `null` — so
 * an unguarded `<ErrorNotice error={maybeNull} />` sat on the screen
 * permanently, reading "Something went wrong, and the reason did not survive
 * the trip" because nothing had gone wrong and there was no reason to survive.
 * Every call site guarded it by hand until one did not.
 */
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorNotice, messageOf } from "./ErrorNotice";

describe("ErrorNotice", () => {
  it("shows nothing at all when there is no error", () => {
    const { container } = render(<ErrorNotice error={null} />);

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });

  it("shows nothing when the error is undefined", () => {
    render(<ErrorNotice error={undefined} />);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  /** A caller holding `string | null` clears it to `""` as often as to null. */
  it("shows nothing for an empty message", () => {
    render(<ErrorNotice error="" />);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the message when there is one", () => {
    render(<ErrorNotice error="the index would not open" />);
    expect(screen.getByRole("alert")).toHaveTextContent(/would not open/i);
  });

  /*
   * The defect that reached the first outside user: an error bar with no way
   * out. It sat there, and the only thing left to do about it was restart.
   *
   * There was no test for it. The monkey can now produce error bars — a random
   * walk with the backend failing under it (AUD-9) — but notices are rare in a
   * random walk, and it is luck whether any given run sees one. Luck is not a
   * regression test. This is deterministic, and it is the one that would have
   * caught it.
   */
  it("offers a way out when the caller can dismiss it", async () => {
    const user = userEvent.setup();
    let dismissed = false;
    render(
      <ErrorNotice error="the server closed the connection" onDismiss={() => (dismissed = true)} />,
    );

    const out = screen.getByRole("button", { name: "Dismiss" });
    await user.click(out);
    expect(dismissed).toBe(true);
  });

  /*
   * And the other half: a notice nobody can dismiss must not pretend otherwise.
   * A dead × is worse than none — it reads as a way out and is not one.
   */
  it("shows no dismiss control when there is nothing to dismiss to", () => {
    render(<ErrorNotice error="the server closed the connection" />);
    expect(
      screen.queryByRole("button", { name: "Dismiss" }),
    ).not.toBeInTheDocument();
  });

  it("unwraps a thrown Error", () => {
    render(<ErrorNotice error={new Error("no audio device")} />);
    expect(screen.getByRole("alert")).toHaveTextContent(/no audio device/i);
  });

  /**
   * The fallback exists for shapes nobody planned for, and it is the sentence
   * that gave the empty-notice bug away. It must stay reachable — but only
   * from something that really was thrown.
   */
  it("still explains an unreadable failure rather than showing an empty bar", () => {
    render(<ErrorNotice error={{ unexpected: true }} />);
    expect(screen.getByRole("alert")).not.toBeEmptyDOMElement();
  });

  it("does not invent a message for nothing", () => {
    expect(messageOf(null)).toBe(
      "Something went wrong, and the reason did not survive the trip.",
    );
  });
});
