# SDK resumed review at e4f330c6

Verified HEAD `e4f330c672b6ff3d76c2c78fd7484962f559bc37` before reading. Reviewed committed SDK/game changes since4d710e35, especially89a862de,3598148e,de62ea8d,e4f330c6, against the previous six-item checklist and approved engine plus76335cb7. Read-only: no builds, source edits, Claude workflow/CLI, or quota failure.

**Approval remains withheld for the preallocation issue below and missing discriminating boundary evidence. The prior substantive rollback/Peek/Join/counter repairs are now largely closed.**

## Concrete remaining preallocation blocker

`plugins/sdk/src/decide.rs:preflight_state` tracks duplicate outer `table` but not duplicate nested `table.cards`. It overwrites `cards` for each occurrence and checks only the final value. Thus a complete active request with an oversized first cards array followed by `cards:[]` passes preflight; the main table parser allocates the oversized first array before reaching/rejecting the duplicate. This is precisely the previously reported oversized-first/small-duplicate bypass, at its nested field rather than the outer state field.

The same function does not track duplicate `seats` (the ninth seen slot is unused), and uses `seats.unwrap_or(256)`. A complete state ordered as seats=[0], oversized-for-one-seat Peeks, then duplicate seats=all256 IDs obtains the larger bound in preflight. The allocating parser reads the Peek collection under the first roster before rejecting the later duplicate. Missing roster also defaults to256 rather than refusing an active request with no authoritative roster. The capsule then inherits that untrusted preflight count.

Narrow fix: reject missing/duplicate active roster before allocating any projection; detect nested duplicate cards and check its advertised cap on the first occurrence, rather than overwrite-and-check-last. Use the validated count for both current and capsule Peek bounds. Keep the current path borrowed/nonallocating and leave nongroup valid collection sizes unrestricted. No production SDK→sim dependency is needed.

Required failing tests before repair:

- Complete active ABI1 request with 4097 first card rows plus duplicate empty cards; assert borrowed preflight rejects, then parse rejects. Pair with one exact4096 cards field that parses successfully.
- Current one-seat roster,4097 Peek rows, later duplicate256-seat roster; preflight must reject. Pair with valid one-seat exact-cap and256-seat capacity controls. A deliberate extra duplicate Peek is acceptable specifically to prove raw count rejection, provided its purpose is stated and the valid control independently passes.
- Missing seats with active group and nonempty capsule/current Peek collections rejects before allocation; duplicate seats rejects regardless of ordering.
- Independently exceed current versus capsule cards/counters/aggregate annotations/outer annotation rows/tokens/revealed/owed/shown/actual-roster Peeks, leaving the other half valid; test both top-level field orders. Direct preflight assertions plus complete parse outcomes prevent rejection for the wrong reason. Distribute aggregate annotations over small rows. Retain valid nongroup over-feature-cap controls.

**No such preflight tests were added in the three preflight commits.** The previous advertised-array tests and suite count cannot establish these bounds.

## Incoming Move nuance: fix the helper contract without overstating admission reachability

89a862de adds a shared `relocated` condition to `apply_action_disclosures`: it returns early for an incoming Move when the CardInfo vector is unchanged. The engine's incoming action application clears shown and performs hidden/forget/shed bookkeeping unconditionally; only ordinary Effect::Move has the no-op early return. Keep those two paths distinct. In particular a same-placement hidden incoming Move on an already hidden card can leave cards unchanged while disclosure still needs clearing in the action projection.

The authoritative engine ordinarily rejects a positional no-op Move during validation before producing a DecideRequest. Therefore this source mismatch is **not demonstrated as an accepted-log divergence** by feeding an artificial no-op Request to the SDK. Correct the SDK action helper to mirror the action application contract, or explicitly document/refuse unsupported preconditions; do not use the Effect no-op test as evidence for incoming actions. Add separate tests for Effect no-op preserving shown, incoming action bookkeeping, and actual native admission refusing positional no-op. The new test only exercises the first and built-in Hand token disappearance.

## Repairs credited

- Real Effect::Move now preserves disclosure on an unchanged card vector, and only forgets token disclosure if the token actually disappears. Built-in Hand token cleanup is covered by a focused new test. Real admitted relocations produce a changed card vector, so that distinction is sound for the reviewed effect path.
- from_decide now validates ordered pending Request/Public debt against current live/owed state, then Outstanding checks require revealability. Rollback rejects a still-live owed transient absent from the capsule. The formerly invalid latest-request test now supplies current owed state, blanks the current face and clears revealed; its positive fixture is corrected, and an absent-current-debt negative was added.
- The rollback absent-card branch still conditionally ignores impossible missing-current pending debt; valid from_decide inputs plus guarded lifecycle operations exclude that state. Matching engine's unconditional pending-absent rejection would be simpler, but no additional admitted failure is demonstrated here.
- Final Peek pruning now accounts for shown and removes all redundant other-viewer pairs after same-Owner public reconciliation. The focused shown test is discriminating. Pending handling precedes pruning; needed Owner-other-viewer pairs remain intact.
- Playing-state Join no longer bypasses next-dense-seat/255 checks, and the overly strict CounterBounds.start restriction is removed. Required direct game-entry and counter-start/clamping regression vectors were not added with these production fixes; carry them into the next focused test commit.
- Current preflight now covers tokens/revealed/outer annotations and outer duplicate fields. The new capsule pass checks field lengths and aggregate annotations before projection allocation and uses a supplied roster count. These improvements stand; the missing/duplicate roster and nested-cards flaws above prevent claiming the feature complete.

## Narrow next steps

1. One bounded repair commit: add the failing nested-cards/roster preflight vectors, fix the borrowed preflight, and show those exact tests pass. Add independent complete current/capsule cap controls, not only truncated payload rejection.
2. One helper/test commit: separate incoming action disclosure semantics from Effect no-op, retain Hand token test, and add direct active-game Join plus engine-valid out-of-range counter-start controls. Report each pre-fix failure or, for already-fixed source, the exact assertion that discriminates the old code.
3. Resume the reusable native differential-driver checkpoint in phase-two review: actual fixture verdict, incremental versus project_verdict, full normalized projection including capsule/information, and independent mechanical expectations. The subsequent cold mechanical, roster/adversarial, wasm/session checkpoints remain separate. No total test count substitutes for that acceptance.
