# Frozen rollback Peek reconciliation clarification

This supersedes “skip RequestPeek when candidate.revealed” and “clear all peeks on a visible PublicReveal.” It changes only reconciliation of already accepted baseline/journal authorization; ordinary unrelated Effect::Peek behavior is unchanged. Engine and SDK must implement the same deterministic rule.

1. Restore the capsule's exact Peek set. For each accepted RequestPeek record naming a card present after restoration and a valid seated viewer, retain that exact pair regardless of candidate.revealed. Discard absent transient targets; never add another viewer. PublicReveal still reconciles face/owed/revealed/shown according to the existing contract, but must not immediately erase Peek pairs.
2. After processing all records and final outstanding-request handling, consider only cards with a retained PublicReveal record. For each such card, remove a Peek pair ONLY if that viewer already has visibility without the Peek: restored zone is All; or restored zone is Owner and card.seat equals that viewer; or final shown contains that card. Do not use revealed as a visibility grant. Keep every other accepted pair. This is canonical: redundant pairs on those publicly reconciled cards are removed, not optionally kept. Peek pairs on cards with no PublicReveal record retain their exact baseline/journal behavior; do not normalize unrelated baseline state.
3. Required retained Peek pairs may coexist with revealed in a restored Owner zone. Decoder/projection validation must permit this valid result. None restoration does not invent revealed/shown; an existing exact Peek may still authorize its single viewer there. Accepted face knowledge remains in log/caches. This is not a global reveal of the restored hidden card.

Decisive two-seat assertions for c held by seat0, with All/Owner/None Aux zones:

- Baseline c hidden in Owner with Peek(c,1); Begin→Move(All)→normal PublicReveal→Rollback: c returns Owner, !shown(c), revealed(c), Peek(c,1) retained; table_view(0) and table_view(1) both report c.face_visible. Reload must preserve those results.
- Baseline same but no Peek; Begin→Move(All)→PublicReveal→Move(None)→accepted Peek(c,1)→Rollback: same final authorization. The later request is real, because the hidden round trip removed current revealed before Peek was issued.
- Control with neither baseline nor recorded Peek: PublicReveal in All→Rollback Owner leaves view0 visible and view1 hidden; !shown(c), no invented Peek(c,1).
- PublicReveal at the SAME Owner zone/seat establishes shown; after rollback both viewers are visible and that card's redundant Peek pairs are removed. Restored All also removes its redundant Peek pairs. This prevents a now-redundant grant from later surviving a hidden move merely as leftover journal metadata.
- Restored None keeps no global visibility: with no accepted Peek, both viewers remain hidden despite public history; with an accepted Peek(c,1), only viewer1 is authorized. Do not clear a legitimate single-viewer grant to achieve a blanket “both hidden” assertion.

Assert both final sets and table_view audiences, plus fresh snapshot/SDK reconstruction. This adds no field/tag/version and does not relax OutstandingDisclosure or private-face sourcing rules.
