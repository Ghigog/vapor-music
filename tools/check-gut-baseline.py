#!/usr/bin/env python3
"""Gate the GUT suite on a pinned baseline instead of on a green board.

The suite has pre-existing failures. Waiting for zero before turning CI on would
mean no regression protection during the whole migration, so instead the current
counts are pinned and CI fails when they get *worse*.

Reads EXPECTED_PASS / EXPECTED_FAIL from the environment so the numbers live in
the workflow next to the explanation of where they came from.

Exit codes:
  0  at or better than baseline
  1  regression, or the output could not be parsed
"""

import os
import re
import sys


def parse_totals(text):
    """Pull the totals out of GUT's summary block.

    GUT colourises its output, so the numbers are surrounded by ANSI escapes.
    Matching on the label and taking the trailing integer avoids depending on
    exactly how the line is decorated.
    """
    totals = {}
    for label, key in (
        ("Passing Tests", "passing"),
        ("Failing Tests", "failing"),
        ("Tests", "total"),
        ("Scripts", "scripts"),
    ):
        m = re.search(rf"{label}\s+\D*?(\d+)", text)
        if m:
            totals[key] = int(m.group(1))
    return totals


def main():
    if len(sys.argv) < 2:
        print("usage: check-gut-baseline.py <gut-output.txt>", file=sys.stderr)
        return 1

    try:
        with open(sys.argv[1], "r", errors="replace") as fh:
            text = fh.read()
    except OSError as e:
        print(f"cannot read GUT output: {e}", file=sys.stderr)
        return 1

    totals = parse_totals(text)
    if "passing" not in totals or "failing" not in totals:
        print("could not parse GUT totals — did the suite run at all?", file=sys.stderr)
        print("--- tail of output ---", file=sys.stderr)
        print("\n".join(text.splitlines()[-40:]), file=sys.stderr)
        return 1

    expected_pass = int(os.environ.get("EXPECTED_PASS", "0"))
    expected_fail = int(os.environ.get("EXPECTED_FAIL", "0"))

    passing = totals["passing"]
    failing = totals["failing"]

    print(
        f"scripts={totals.get('scripts', '?')} tests={totals.get('total', '?')} "
        f"passing={passing} failing={failing}"
    )
    print(f"baseline: passing>={expected_pass} failing<={expected_fail}")

    problems = []
    if failing > expected_fail:
        problems.append(
            f"failures rose from {expected_fail} to {failing} (+{failing - expected_fail})"
        )
    if passing < expected_pass:
        problems.append(
            f"passing fell from {expected_pass} to {passing} (-{expected_pass - passing})"
        )

    if problems:
        print("\nREGRESSION:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    if failing < expected_fail or passing > expected_pass:
        print(
            f"\nBaseline improved (passing {expected_pass}->{passing}, "
            f"failing {expected_fail}->{failing}). "
            "Tighten EXPECTED_PASS / EXPECTED_FAIL in .github/workflows/ci.yml "
            "so the gain is locked in."
        )

    print("\nOK — at or better than baseline.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
