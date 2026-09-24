# Sealed: the copy changes

Answer to "does Sealed actually ship today": **yes, in 0.6.4**, for all four
things on that list. Checked against the code, not recalled:

| Claim | Where it lives |
|---|---|
| Connections-off build | `hub::sealed`, one `web::outbound` gate every outbound path passes |
| Group Policy lockdown | Windows `HKLM\Software\Policies\Excalibur\View`; macOS managed preferences; Linux env var |
| Offline signed updates | `POST /admin/releases` takes a hand-carried manifest + installer; a sealed server's feed is `None` and no setting changes it |
| Audit log | `crates/server/src/audit.rs` |

So: **option 2, rewrite the copy to match.** Below is the wording.

## The rule the whole page has to keep

Three things must never appear, whatever else changes:

1. **Never "compliant."** The existing FAQ answer is right and should be
   carried onto the Sealed page verbatim: *"Does this make us compliant? No.
   Nothing does on its own."*
2. **Never "audited" or "certified."** Nobody outside has looked at it.
3. **Never imply the connections-off claim was checked by a third party.** It
   was checked by reading the code and by tests. Say that — it is a real
   thing to say and it is not the same claim.

## What changes

**Meta description** — drop "is planned":

> Defense and security-sensitive contractors: Excalibur View keeps drawings
> inside your own network. Sealed switches off every outbound connection, on
> each seat and on the server.

**Nav entry** — drop "Planned.":

> **View Sealed** — For defense work. No outbound connections.

**The CTA** — replace "Talk to me" with the buy card that is already built.
Keep a "Talk to me" link underneath it, smaller: a shop with a contract
clause will want to ask before they buy, and that is worth encouraging.

**Section heading** — "Planned / What Sealed adds" becomes:

> **What Sealed adds**

**The feature list** — present tense throughout. Suggested wording:

> - Every outbound connection switched off, on each seat and on the server
> - Locked on by your administrator through Group Policy or a managed
>   preference, and not switchable from inside the program
> - Updates arrive as signed files your administrator carries in. Each seat
>   checks the signature against a key built into the program before
>   installing, so a tampered file is refused at every seat
> - An audit log of who did what, exportable as CSV

**Drop entirely** — any "pricing goes up when it's ready" line. It is $500 a
seat and $200 a year now.

## One sentence worth adding

Somewhere near the buy button:

> A sealed office never fetches its own license renewal, because it never
> reaches out — that is the edition working as intended. Buy seats and the
> new license comes to you as a file.

It is written up in full in `docs/sealed.md` under "How do we add seats, or
renew?". Better a customer reads it before buying than finds out in a
renewal week.

## And `/pricing/`

Yes, add the Sealed card with its own stepper — **after** the copy above is
live, not before. A price card on a page that still says "planned" makes the
site argue with itself.

## Not for the page

Nobody has yet set that Group Policy key on a real machine and watched it
refuse. The tests cover the logic; no human has run a sealed seat end to
end. That is Creede's half hour before the first defence contractor pays,
and it does not belong in the copy either way.
