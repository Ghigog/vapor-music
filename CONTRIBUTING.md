# Contributing

**Vapor Music is not accepting code contributions.** Pull requests from anyone
other than the owner are closed automatically — see
[`.github/workflows/no-outside-prs.yml`](.github/workflows/no-outside-prs.yml).

Dependabot is exempt. Its pull requests are version numbers in a manifest and a
lockfile, which carry no authorship to assign, and dependency updates are
watched deliberately.

## Why

The licence terms for outside contributions have not been settled, and this
repository was made public before they were. A contributor holds copyright in
their own patch, so merging one fixes the project's licensing in a way that
cannot be undone later without that person's agreement. Relicensing a codebase
with contributors in it means finding all of them and getting a yes from every
one.

That is a decision the project has not made. Until it does, the honest answer
is to not take the code rather than to take it and work the licence out
afterwards, which is the order that leaves somebody unable to change their mind.

`docs/LICENSING.md` carries the full reasoning, including why the project is
proprietary today and what the alternatives would cost.

## What is welcome

**Bug reports and ideas.** The issue tracker is open and stays open. There is no
telemetry and no crash reporting in this app, both deliberately, so a person
describing what went wrong is the only way a fault is ever heard about — see
[`SUPPORT.md`](SUPPORT.md).

If you have already written a fix, **describe it in an issue**. Saying what the
problem was and how you solved it is useful and carries none of the copyright
problem above. Attaching the patch does not, so please don't.

## If this changes

It may. The blocker is a decision about a contributor licence agreement, not a
position on outside help. If that lands, this file and the workflow beside it
are what change.
