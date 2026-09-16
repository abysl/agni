# Damage corrections review

Reviewed `06546f4b..5af45c61` (including `eb1f7356`) in `/tmp/agni-takeover-damage`, source and tests only. Excluded the moving `net/tests/riftbound_turns.rs`. No builds or production edits.

**Disposition: bounded production approval; focused test corrections and final gates remain required. No additional production blocker found in this corrective delta.** This does not approve A11 replacement ordering or D1 callback continuations.

## Three production blockers closed

- `cards/esteemed_hierophant.rs::from_an_enemy_spell_or_ability` now uses `live_item`, finding the resolving execution removed from the ordinary chain. The new `a_resolving_enemy_spell_uses_its_live_item_for_the_immunity` plays and resolves an enemy spell against the seven-rune Hierophant; zero damage and survival distinguish the original failure. This fixture retains one Ctx; it is not a saved-request parity test.
- `engine/chain.rs::proceed` restores empty-chain lethal cleanup before opening the turn state. Its new outside-resolution regression uses ordinary `settle`, checks first-hit survival, then second-hit death, trash destination, Died event and neutral state. The current Resolving-before-child ordering remains intact.
- `engine/combat.rs::exempt` uses NoDamage or unbounded prevention, and `lethal` uses that exemption. The finite N(3) shield on a 3-Might unit is explicitly nonexempt with lethal assignment 6. This distinguishes finite protection from unlimited protection without claiming controller-selected replacement ordering.

## Restored coverage

The saved doubled-target lethal calculation, actual Elder Dragon combat assignment/deaths, lethal marker cleanup, damage-controller attribution/healing, and multiplier stacking/expiry tests are restored. The combat Elder case traverses the assignment prompt and deaths, rather than only calling the lethal calculator. The multiplier case and Lotus expiry case invoke expiration helpers directly; they do not establish real EndTurn/reload behavior.

Lotus Trap's revised test now keeps the trapped unit alive at 5 Might, deals 1→2 before expiration, and deals another 1 afterward to the same unit, reaching 3. This discriminates expiration, unlike the previous unrelated target assertion.

## Required focused fixture corrections

1. `state.rs::malformed_old_seat_and_unsorted_damage_marks_are_rejected` supplies a body under the wrong v11 seat arity, but its final Pool is `[]`. `Pool::read` requires `[energy, power-array]`, so rejection still occurs without the corrected seat-length guard. Write a valid Pool `[0, []]` and otherwise valid 14-value body under the wrong declared length. Include a correct-arity acceptance control.
2. The same test's legacy Next vector declares Prevention arity 4 while v12 requires 3. It therefore rejects before reaching the boolean Next value. Use the old three-field layout with valid source/expiry and the Next boolean; contrast accepted old numeric/All and accepted v13 Next. Preserve duplicate-key damage-mark rejection alongside the newly added descending-key case.
3. `ravenborn_tome::the_next_spell_this_turn_deals_one_more_and_the_one_after_does_not` plays `HAND_SPELL` again after it resolved to trash. `fixtures::play_from_hand` fabricates an entry whose `from` is hand, so this bypasses the actual source-zone precondition. Provide a second distinct hand spell, bind its script and supply sufficient resources; assert the first receives the bonus and the second deals ordinary damage.
4. The restored `cleanup::dying_reads_a_lethal_damage_static_of_a_card_the_marker_controls_before_might` deals two total damage to default 2-Might THEIR_UNIT. Ordinary lethal already explains its death. Raise that target to 5 Might before resolving the fixture so the same two damage isolates the marker-controller static. The actual combat test already provides independent evidence for Elder's effect; this is a narrow fixture strengthening.

These are test adequacy findings, not evidence that the now-correct decoder, bonus consumption or marker source implementation is broken. Sent to root and the Damage worker.

## Acceptance boundary

The corrective production delta does not change v13 layouts or overwrite Play/Costs/A9 behavior. The previously reviewed independent legacy fixtures remain relevant; the malformed cases above must isolate their intended guards. Fresh candidate tests, compatibility, clippy/fmt and artifact parity/Kai gates remain root/worker responsibilities. The separate native Damage corpus must exercise nonzero v13 state through saved requests and actual expiration; it was intentionally not reviewed here. No reported gate was independently rerun.

## Follow-up: corpus 8b8d5b9e and fixtures 74ccc370

Bounded source acceptance for the Damage correction/corpus work, subject to fresh integration gates. The four identified fixture faults are corrected: the malformed old seat now contains valid Pool bytes; old Next rejection uses the old three-field layout; Tome has a distinct second hand spell and four ready runes cover the two 2-energy/1-power plays; the lethal-marker target has 5 Might. Existing independent old-layout fixtures and v13 Next round-trip provide acceptance controls. The separately requested duplicate damage-mark rejection case remains absent (descending is covered); restoring that one small case is recommended, with no corresponding source defect found.

The new corpus enters through HostSession and relays actual entries to the client. It checks Alpha payment (four initial ready runes, one ready afterward, one recycled), Trap payment (two initial ready runes, zero afterward, no recycling), and actual registered spell resolution. No damage, multiplier, marker, kill, resource or expiry state is injected. The saved Resume has the resolving Alpha item, Giant multiplier 2 and marks [(0,2)]. The ordinary suffix yields eight damage, Wisp cleanup, Alpha trash and one XP; actual EndTurn clears damage, marks and multiplier on the same surviving Giant.

The cold helper reconstructs the checkpoint by a fresh native replay and selects the first Resume with a two-damage card. In this fixed corpus that uniquely identifies the asserted Giant boundary. It then creates new NativeEngine, WasmEngine, NativeRiftbound and loaded wasm plugin instances, restores the complete engine snapshot, and starts at the next entry. Requests, verdicts, fold results/deltas, snapshots and both engine/plugin views are compared throughout the suffix, then both final snapshots must equal the originating HostSession. Cached hardened module bytes are reused; runtime instances are fresh. The full corpus also enters the existing per-entry restore/parity harness.

The checkpoint selector could assert its multiplier/marks explicitly for clearer failure diagnostics, but it is meaningful for the current fixed sequence and does not replay a final entry against final state. The new real EndTurn corpus supplies the lifecycle coverage the helper-only expiry fixtures did not. No A11/D1 completion or universal gas/allocator equivalence follows from this test. No builds were run for this review.
