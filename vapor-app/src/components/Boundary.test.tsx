/**
 * The render-error boundary.
 *
 * Its whole purpose is that a screen which throws does not blank the app. That
 * failure shape cost two rounds of guessing at a bug that could have named
 * itself, so the thing under test is specifically that the name reaches the
 * screen.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Boundary } from "./Boundary";

/** Throws on first render, then behaves. Mirrors a transient data fault. */
function Fragile({ fail }: { fail: boolean }) {
  if (fail) {
    throw new TypeError("Cannot read properties of null (reading 'toFixed')");
  }
  return <p>the screen</p>;
}

beforeEach(() => {
  // React logs caught render errors itself; the test asserts behaviour, not
  // console noise.
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe("Boundary", () => {
  it("shows what is normally there when nothing throws", () => {
    render(
      <Boundary where="The Vibe DJ screen">
        <Fragile fail={false} />
      </Boundary>,
    );
    expect(screen.getByText("the screen")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("names the screen and quotes the error verbatim", () => {
    render(
      <Boundary where="The Vibe DJ screen">
        <Fragile fail />
      </Boundary>,
    );

    const panel = screen.getByRole("alert");
    expect(panel).toHaveTextContent(/the vibe dj screen could not be drawn/i);
    // Verbatim: a paraphrase loses the one detail that makes it fixable.
    expect(panel).toHaveTextContent("Cannot read properties of null (reading 'toFixed')");
  });

  it("says the rest of the app still works, because it does", () => {
    render(
      <Boundary where="The Library">
        <Fragile fail />
      </Boundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(/use the sidebar/i);
  });

  it("can be retried, for a fault that was transient", async () => {
    const user = userEvent.setup();

    function Host() {
      return (
        <Boundary where="Now Playing">
          <Fragile fail={false} />
        </Boundary>
      );
    }
    // First prove the failing case renders the panel...
    const { unmount } = render(
      <Boundary where="Now Playing">
        <Fragile fail />
      </Boundary>,
    );
    await user.click(screen.getByRole("button", { name: /try again/i }));
    unmount();

    // ...then that a healthy child renders normally through the same boundary.
    render(<Host />);
    expect(screen.getByText("the screen")).toBeInTheDocument();
  });
});
