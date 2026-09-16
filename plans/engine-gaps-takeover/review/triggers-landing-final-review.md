# Final bounded Triggers landing review

Reviewed committed candidate `5342617a`, especially test commit `8a3b958d`, against repaired source `3e00abad` and green main `32620ad3`. Read-only; no builds or edits. Neutral protocol is parked and is not a prerequisite to this landing.

**Disposition: no additional production blocker identified within the reviewed five-cluster recovery; landing remains conditional on correcting the failed migration fixture and strengthening/re-running the Granted/Lent corpus below.** This is not approval of the remaining trigger roadmap or identity redesign.

## Production and baseline preservation

The new commit adds test-only migration/corpus coverage. The merge from main carries baseline/documentation history rather than additional trigger production semantics. Prior bounded source fixes remain credited: conservative ordering for multi-item trigger batches, captured movement actor, explicit combat damage assigner for excess, complete-item Lent discounts, canonical duplicate-rejecting excess rows, preserved wearer statics and saved holder/lender continuations. The meaningful RELIC wearer regression remains restored.

The v14 reader retains the v12/v13 14-field ChainItem shape; v13 Noted remains four fields with buffed=false, while v14 Noted has five. Granted/Lent are version-gated new kinds. No source evidence in this delta supports calling the current test's failure a production migration defect before its authored body is diagnosed.

## Must close before landing

1. **Migration evidence currently fails.** `/tmp/agni-triggers-migration.log` reports FAILED at `v12_and_v13_saved_chain_and_pending_rows_keep_trigger_resume_fields`, during the v13 decode. `review/triggers-landing.md` currently calls that exact log a pass. Correct the fixture or, if diagnosis shows an actual reader defect, repair it with the independent old-format control. Then run the focused and complete compatibility suite and replace the false evidence entry with the real result.

   Keep manually authored old bodies rather than deriving an old version from the new writer. The new v13 second target is `[1,12]`, which means Seat(12); if the intended target is the battlefield used by the adjacent fixtures, encode `[2,12]` and assert Zone(12). Assert the actual stage, targets, spec_counts, Repeat/mode choices, execution, awaiting, Noted fields/default buffed, Limited and Pending Needs/identity fields that the fixture claims to preserve. A current encode/decode roundtrip cannot independently detect a legacy field dropped to its default.

2. **Assert the actual Warmog buff.** The corpus's `might()` helper reads only engine COUNTER_MIGHT. Ctx::current_might adds COUNTER_BUFFED separately, and Warmog regenerate calls buff(). Therefore unchanged `might()` after Conquer neither proves a missing buff nor proves the trigger resolved correctly. Keep the equipment Might check, but require COUNTER_BUFFED absent/0 before the Granted item resolves and exactly1 afterward. Assert the intended holder/lender and no remaining item from that activation/trigger. Do not replace this with an assertion that total Might remains unchanged.

3. **Cold-resume both item kinds.** The committed helper asserts a Lent item at its checkpoint and only runs from the later Gardens activation. It does not cold-resume the earlier Warmog Granted item. Parameterize/reuse it with an expected exact item kind/holder/lender, save immediately after Granted is queued and before its resolution, and run the fresh native/hardened suffix from that checkpoint as well as the Lent checkpoint. Both runs should end at the same independently checked final host state.

   The existing helper correctly obtains authoritative decide requests, compares native/wasm verdicts, accepted fold results, deltas and snapshots, and creates new engines and plugin instances. Preserve those properties. Retain the full-log native/hardened replay as well. Newly queued items must arise from the registered gameplay sequence, not injected blobs.

## Corpus guarantees and limits

The setup includes two explicitly supplied Body runes plus the opening channel's two runes. The actual printed Equip ability attaches Warmog and reduces ready rune count from4 to3; add/check the recycled Body identity/location where the final strengthened fixture already tracks payment. Gardens is discovered through its actual affordance and yields a Lent item with the Matriarch as holder and Gardens as lender. The final XP increment and holder exhaustion are meaningful outcome assertions.

For the Lent boundary, assert holder is ready immediately before activation and exhausted at the saved checkpoint, not only after resolution; the previously reviewed card unit tests already cover payment timing, while this corpus should demonstrate its saved-state form. The Warmog buff must be checked before the later turn progression, so Hold/expiration or another ability cannot masquerade as the trigger result.

The old helper-based reload tests still add useful lender-leaves/holder-leaves/detached/dead-wearer coverage; they are not replaced by this ordinary registered corpus. The new corpus need not implement arbitrary captured-event or stable-incarnation semantics to be accepted.

## Final gates and landing order

After these test corrections, review the small final diff and run the complete Riftbound library/compatibility/match-state suites, scoped warning-denied lint/format, the registered corpus with freshly built/hardened artifacts from the final production tree, and the affected Kai compatibility gate. Root owns gate verification and source/artifact freshness. Earlier 4,536 native passes do not certify the new failed fixture.

When those gates pass, the five-cluster candidate can merge onto the already green main as its own bounded landing, with v14 migration and actual reviewed test results recorded. Keep A3/A4 captured/stable identity work, D1/A11 replacement work, the other trigger clusters and fourth Costs settlement explicit and paused. Do not import neutral engine/SDK work or raise replay budgets as part of this recovery.
