/**
 * The list of tracks analysis could not describe.
 *
 * The case worth pinning is the one that prompted it: a count stuck short of
 * the total with nothing on screen saying which tracks or why. A permanent
 * failure counts as done and is *not* the cause; a stalled one is.
 */
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useBackend } from "../test/setup";
import { AnalysisFailures } from "./AnalysisFailures";
import type { AnalysisFailure } from "../lib/generated/AnalysisFailure";

const STALLED: AnalysisFailure = {
  href: "/dav/Music/one.flac",
  title: "Undertow",
  artist: "Hollow Coast",
  reason: "not downloaded yet",
  permanent: false,
  attempts: 4,
};

const BROKEN: AnalysisFailure = {
  href: "/dav/Music/two.flac",
  title: "Second Sleep",
  artist: "Marrowfield",
  reason: "decodes to zero samples",
  permanent: true,
  attempts: 0,
};

describe("listing what analysis could not do", () => {
  it("names each track, its reason and its file", async () => {
    useBackend({ analysisFailures: [BROKEN, STALLED] });
    render(<AnalysisFailures onClose={() => {}} />);

    expect(await screen.findByText("Second Sleep")).toBeInTheDocument();
    expect(screen.getByText("decodes to zero samples")).toBeInTheDocument();
    // Two tracks can share a title; only the path says which file to look at.
    expect(screen.getByText("/dav/Music/two.flac")).toBeInTheDocument();
  });

  /* The distinction the whole feature turns on: "will retry" is ordinary and
     resolves itself, "stuck" is what holds the count short for ever. */
  it("tells a track that will retry apart from one that keeps failing", async () => {
    useBackend({ analysisFailures: [STALLED] });
    render(<AnalysisFailures onClose={() => {}} />);

    expect(await screen.findByText(/stuck · 4 tries/)).toBeInTheDocument();
  });

  it("calls a first-pass miss ordinary rather than failed", async () => {
    useBackend({ analysisFailures: [{ ...STALLED, attempts: 1 }] });
    render(<AnalysisFailures onClose={() => {}} />);

    expect(await screen.findByText("will retry")).toBeInTheDocument();
    expect(screen.queryByText(/stuck/)).not.toBeInTheDocument();
  });

  it("says so plainly when there is nothing wrong", async () => {
    useBackend({ analysisFailures: [] });
    render(<AnalysisFailures onClose={() => {}} />);

    expect(
      await screen.findByText(/every track in your library has been described/i),
    ).toBeInTheDocument();
  });

  it("shows the reason it could not ask, rather than an empty list", async () => {
    const backend = useBackend();
    backend.fail("analysis_failures", "The library is not readable.");
    render(<AnalysisFailures onClose={() => {}} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/not readable/i);
  });

  it("closes on Escape", async () => {
    useBackend({ analysisFailures: [BROKEN] });
    let closed = false;
    render(<AnalysisFailures onClose={() => (closed = true)} />);
    await screen.findByText("Second Sleep");

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(closed).toBe(true));
  });
});
