# EULA — what is missing, and what needs a lawyer

**Status:** notes only. There is no EULA.
**Written:** 2026-08-23, for AUD-16.

> This is an engineer reading `LICENSE` and the shipped code and writing down
> the gap. It is not legal advice, it is not a draft, and nothing in it should
> be relied on as either. Its whole purpose is to make the next conversation
> with a lawyer short.

---

## What `LICENSE` currently does

`LICENSE` is a reservation of rights over the **repository**. In substance:

- All rights reserved to Dylan Growcoot, 2026.
- The source, assets and documentation are proprietary and confidential.
- **No licence is granted** to use, copy, modify, merge, publish, distribute,
  sublicense or sell any part of it, by implication, estoppel or otherwise —
  only by a written agreement signed by the copyright holder.
- Third-party components are carved out and left under their own licences,
  itemised in `THIRD_PARTY_NOTICES.md` with texts in `licenses/`. That carve-out
  is correct and specifically names the MPL-2.0 set (symphonia, the
  Servo-derived CSS crates via Tauri).

`docs/LICENSING.md` is the inventory this rests on: 620 of 624 packages
resolved, no GPL/AGPL/LGPL-only dependency, 18 MPL-2.0 packages.

## What it does not do

**It grants a person who downloads a build no right to run it.** "Use" is in
the list of things not granted, and running a program is use. Taken literally,
somebody who downloads a release today holds a copy of a work whose author has
told them they may not use it.

That is the right starting position for source code and the wrong one for a
binary handed to a listener. It is also silent on everything an end-user
agreement normally carries:

| Absent | Why it matters here |
|---|---|
| A grant of use | The whole point. Without it there is no permission to install or run |
| Warranty disclaimer | None anywhere in the repository |
| Limitation of liability | None anywhere in the repository |
| Termination | No stated conditions, no stated effect |
| Governing law and jurisdiction | Not named |
| Anything about automatic updates | The desktop build replaces itself — see below |
| Anything about the user's own files | The app reads, tags, caches and uploads music the user owns, and holds a credential for a server the user owns |

`vapor-app/src-tauri/tauri.conf.json` carries the matching copyright string
(`Copyright (c) 2026 Dylan Growcoot. All rights reserved.`), so the binary is
stamped consistently with the repository. It is stamped with the same gap.

## What a binary distribution needs

Roughly, and in the order a template will have them:

1. **A grant.** A non-exclusive, non-transferable, revocable licence to install
   and run the software for personal, non-commercial use. Scope — per device,
   per person, per household — is a decision, not a default.
2. **Restrictions** consistent with `LICENSE`: no redistribution, no resale, no
   modification. **A blanket no-reverse-engineering clause is the one to be
   careful with**, because MPL-2.0 files really do ship inside this binary and
   carry their own terms, and because some jurisdictions do not permit that
   restriction to be excluded by contract.
3. **Third-party notices**, incorporated by reference. `THIRD_PARTY_NOTICES.md`
   and `licenses/` already exist and are the exhibit — this is the one item on
   the list that is already done.
4. **Warranty disclaimer** and **limitation of liability**. For a one-person
   project these are the clauses that actually earn the document.
5. **Automatic updates.** Not optional boilerplate here: every desktop launch
   fetches `latest.json` from GitHub and, if a signed newer release exists,
   downloads and installs it silently with no in-app way to decline
   (`vapor-app/src-tauri/src/lib.rs`, the `#[cfg(desktop)]` updater block, and
   the `plugins.updater` endpoint in `tauri.conf.json`). An agreement that does
   not mention this describes a program that does not exist.
6. **Termination**, and what survives it.
7. **Governing law and jurisdiction.** `<JURISDICTION — Dylan to confirm>`.
8. **The contracting party.** A person or a company, with an address.
   `<LEGAL ENTITY AND ADDRESS — Dylan to fill in>`.
9. **A pointer to `PRIVACY.md`**, and no clause that contradicts it.

## What specifically needs a lawyer's eye

Not "get a lawyer to write it" — these are the six questions worth paying for.

1. **MPL-2.0 inside a proprietary binary.** The obligation is file-level and
   the app does not modify those files, so `docs/LICENSING.md` reads this as
   satisfied by notice alone. That reading is an engineer's. It is also the
   claim with the most downside if it is wrong, because 18 packages including
   the entire audio decoding stack sit under it.
2. **Whether a warranty disclaimer survives consumer law** in the places this
   is sold or given away. In the UK and the EU, statutory rights are not
   excludable by a term in a licence, so "no warranty of any kind" is not fully
   effective against a consumer however it is worded. What the clause should
   say instead is jurisdiction-specific.
3. **The silent auto-update.** Whether informed consent to a program replacing
   itself has to be obtained rather than recited, and whether the answer changes
   once money is involved.
4. **The two app stores.** Apple supplies a standard Licensed Application End
   User Licence Agreement to developers who do not provide their own, and a
   custom one has to meet its minimum terms; Google Play expects the developer's
   own terms. Both have their own required disclosures about data. **Read the
   current text of both developer agreements** rather than relying on this
   paragraph, which is a summary written from memory and is the sort of thing
   that changes.
5. **Liability where the price is zero, and where it is not.** These are
   different documents. AUD-16 says the EULA wants a lawyer *before anything is
   sold*, and that is the correct trigger.
6. **The user's own content and credentials.** The app holds a WebDAV password
   in the OS credential store and moves the user's own files to and from a
   server the user chose. An agreement needs to say plainly that none of that
   content becomes the developer's, and that the developer is not the operator
   of that server.

## What does not need an EULA

Keeping the source proprietary. `LICENSE` already does that, and it does it
correctly. The gap is entirely about the person running the binary, not the
person reading the repository — worth stating because the two get conflated,
and because "we already have a LICENSE" is the reason a EULA does not get
written.

## Related

- `PRIVACY.md` — factual, written from the code, and done.
- `SUPPORT.md` — the reporting route, awaiting a contact address.
- `docs/LICENSING.md` — the dependency inventory, v2.3, reviewed 2026-08-20.
- `docs/RELEASE.md` §3 — the other outstanding pre-release items.
- Ticket AUD-17 — contributions on a public repository, which is a different
  licensing hole and is not closed by any of the above.
