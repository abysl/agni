# Engine-only review: 8e1ebe62

Read committed `9c24b873..8e1ebe6245586578ea98c38254537c92a697440f` only, against the final repair review and Peek authorization addendum. SDK/game changes and subsequent baseline merge are excluded. No builds or production edits.

**Disposition: changes required before the shared integration gate.** Four bounded source corrections remain; prior fixes and remaining acceptance are separated below.

## Remaining source blockers

1. **Peek redundancy pruning still occurs too early.** rollback_effect_group computes retained_peeks before the pending-Reveal loop clears shown/revealed. The addendum explicitly requires final visibility after pending handling. Concrete sequence: baseline Owner c with Peek(c,1); Begin; same-placement PublicReveal creates shown; a hidden round trip accepts a new Peek(c,1) and RequestReveal(c); Rollback. Pruning sees old shown and removes Peek1, then pending handling clears shown/revealed, so viewer1 loses its accepted grant. Move pruning after pending handling. Test both final sets and the other viewer's TableView; the added baseline-only Peek test cannot catch this combination.

2. **Annotation preallocation remains unbounded within the feature scan.** raw_group_limits runs before cbor_value now, which is a substantial improvement for card/information/counter arrays. However, map_spans allocates a Vec of every map entry and copies every key into a BTreeSet before the annotation aggregate is checked. A single annotation row map with a million keys allocates that metadata/key payload before rejecting its >16384 count; the outer annotation-card map is also unbounded despite at most4096 valid referenced cards. Read each advertised map count and check the remaining aggregate/outer-card bound BEFORE map_spans allocates/collects it. The bounds-only pass can read row headers and borrowed byte spans; it does not need copied annotation keys. Keep semantic duplicate checks before typed serde normalization, using bounded deterministic key structures for the new journal paths.

3. **Active current-state limits are not part of the preallocation gate.** The repair newly applies4096/16384/seat-derived limits to current active state as well as the capsule and checks ordinary effects against them. But prescan_group_limits only bounds group.before/information; malformed active current table/annotations/counters can still allocate their full Value tree before validate_state rejects. Reuse a raw mechanical-field bounds routine on the active current state before Value construction (allow optional/default current-state fields). Bound the actual roster to at most256 IDs before using its advertised count for Peek capacity. Preserve nongroup compatibility by invoking these feature limits only when a group is active.

4. **Destination/owner membership is not enforced at mutation.** Confirmed the SDK worker's differential finding: engine Effect::Move/Spawn can address a visible PerSeat zone to an absent player, while restore's validate_table and SDK reject the resulting Card. Resolve effective_seat first, then reject an unseated effective destination before mutation; for Shared zones supplied seat is normalized to0 and must remain accepted when0 is seated. Spawn separately validates its explicit/default owner before allocation. The common effective_seat seam plus Spawn's owner check is the bounded engine fix. Test roster[0,7]: PerSeat7 accepted, absent1 rejected; Shared supplied1 accepted at0; explicit owner1 rejected even for Shared. Failed effects must consume no allocator or mechanical changes.

No new game rule, player cap or private-face authority is needed for these corrections.

## Confirmed improvements

- Current token references are validated again; the absent999 token fixture isolates that missing check.
- RequestPeek no longer skips solely on candidate.revealed; PublicReveal no longer blindly clears all Peek pairs. The added baseline-Peek test checks both actual TableViews, no inappropriate shown flag, and restored view equality. Revealed+required Owner Peek is allowed by decoder shape validation. Only final ordering remains wrong in the combined pending case above.
- Global16MiB/million-collection limits and nongroup annotation/Peek feature limits have been removed. span0/step0 are again accepted; tests include these controls and a real-card annotation aggregate above16384 outside a group. Source no longer imposes the previous size-based rejection on ordinary large byte/string-vector payloads.
- Hidden-face PublicReveal records are rejected on the tentative group path before committing the entry's action/effects; the journal no longer accepts that particular unrestorable state. Ordinary nongroup Reveal behavior remains unchanged.
- Raw group count checks now precede cbor_value. Card/information/counter/set array lengths and before annotation totals are checked before full Value decoding. The remaining metadata/current-state holes above prevent full preallocation approval, but this is materially better than the earlier generic-only scanner.
- Active ordinary mutations enforce current feature limits atomically on their candidate. Post-terminal ordinary effects run without those group limits. Existing allocator high-water restoration, coalesced/later Reveal debt, transient history and first-marker handling are retained.

## Acceptance still required, separate from source blockers

The diff adds one substantive baseline Peek/audience test and expands existing decoder controls. It does not add the complete later-Peek/pending/same-placement/All/None matrix, complete valid-shape raw limit controls, or a hidden-face group test. Reported suite passes therefore do not prove those individual cases.

Before integration, add focused cases for the four blockers, including direct assertions against the feature-aware byte gate on complete over-limit objects with valid at-limit controls. Keep large nongroup compatibility controls native-only; a >16MiB annotation, >million-element ordinary Face.domain and >4096 ordinary Peek baseline are inexpensive in conceptual scope but need not be multiplied across the wasm corpus. Ensure the chosen oversized fixture is otherwise valid, rather than wrong-shaped/truncated or dangling.

The planned shared gate supplies exact full mechanics/order/face/counter restoration, failed-clone and accepted-rollback allocator behavior, admitted disclosure refusal/success waits, both viewer audiences, actual SDK projection equality, fresh native/hardened engine+plugin continuation, ABI0 guards and independent old snapshot migration. That gate is still necessary after source repairs. Existing versioned empty-group goldens remain format coverage. No gate was rerun here and no fourth-Costs/A1/D1 completion is claimed.
