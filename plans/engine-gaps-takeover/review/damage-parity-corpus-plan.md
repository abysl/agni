# Small real Damage v13 replay corpus

Design only, using `net/tests/riftbound_turns.rs` helpers and registered `Lotus Trap` / `Alpha Strike` scripts. No production edits or builds. Implement after the reviewed Damage corrections are green; use that exact fresh plugin artifact.

## Scenario and exact fixture

Add `native_lotus_alpha_strike_entries() -> (Vec<LogEntry>, Vec<u8>)` beside the existing native corpus builders. Build through `open_native_table`, `roll_for_first`, `open_m9_game`, `moved`, `pick`, `turn_event`, `option_index` and `card_option`. All gameplay must enter through HostSession intents and relay to the client; do not mutate GameBlob/table state to install the trap or damage.

Use M9Deal:

- seat 0 pool: two Body runes; hand: `spell("Alpha Strike", 3, 1, "Body")`; base: `unit("Vi", 3, 1, "Fury", 5)` as in the existing Alpha Strike net test;
- seat 1 pool: two Fury runes; hand: `spell("Lotus Trap", 2, 0, "Fury")` (the existing Lotus card fixture's actual cost);
- seat 1 garrison at battlefield 1: `unit("Wisp", 1, 0, "Calm", 1)` and a deliberately vanilla fixture unit `unit("Training Giant", 1, 0, "Calm", 12)`. The latter uses generic unit behavior to survive eight damage without unrelated card abilities; do not give a real named card an incorrect printed Might;
- default main decks and rune decks from open_m9_game; no extra grants, counters or direct state injection.

`open_m9_game` starts seat 0's turn and channels TWO Mind runes in addition to its explicit pool. Assert initial ready counts 4 for seat 0 and 2 for seat 1. Do not budget from the two explicit Body runes alone. Both garrison units are physically in the shared battlefield zone with their owners/controllers seat 1; use returned IDs, never hard-coded card numbers. No march or showdown is needed.

## Admitted sequence and assertions

1. Seat 0 moves Alpha Strike from Hand to Chain via `moved`, chooses Vi using the current numbered target option. Assert one finalized spell and its captured controller 0. Payment is three energy plus one Body power: exactly three runes exhaust, one Body rune recycles (it may be one of those exhausted), and one ready rune remains. Verify zone/count/domain movements and no Gold use; do not pin an irrelevant rune ID choice if the deterministic planner picks an equivalent source.
2. Seat 0 passes priority. Seat 1 reacts with Lotus Trap from Hand to Chain, chooses Training Giant, and pays two energy, zero power. Both Fury runes exhaust and none recycle. Pass seat 1 then seat 0 to resolve Trap. Assert Trap is in seat-1 Trash, Alpha Strike remains on the chain, and the Giant's CardState multiplier is 2 in a v13 blob.
3. Pass seat 0 then seat 1 to start Alpha Strike's real resolution prompt. Assert five points to allocate, controlled by seat 0. Choose Wisp once, then Giant once using fresh `option_index` lookups. The latter receives two actual damage from the one assigned point. At this exact boundary save `encode_state(host.state())` and the current log length/next sequence; assert a Resume prompt remains, the Alpha item is Resolving, Giant damage is 2, multiplier is 2, and `damage_marks == [(0,2)]`. Do not require Wisp to have left before the resolving spell completes: it has lethal damage, but normal cleanup occurs at the appropriate completion boundary.
4. Choose Giant for each of the remaining three points. Resolve any remaining automatic/reflexive chain work through `drain_priority` rather than hard-coded extra passes. Assert Wisp reaches its owner's Trash through normal cleanup, Giant stays at battlefield 1 with eight damage and `damage_marks == [(0,8)]`, Alpha Strike reaches seat-0 Trash, and the chain/prompt close. Alpha Strike's ordinary one-kill XP reflexive should yield exactly one XP to seat 0; this is a useful additional assertion, but it must not be achieved by injecting a death or calling cleanup directly.
5. Seat 0 ends the turn through TurnEvent::EndTurn. Assert turn player 1 and normal turn advancement; the same surviving Giant has zero damage, empty marks and multiplier default 0 (allow its otherwise-default CardState row to be removed). This proves expiration on the actual trapped survivor, unlike hitting a different untrapped unit after expiry. The battlefield stays held by seat 1; any normal Hold score/Channel/Draw effects are real log entries and replayed normally.
6. Use `same_bytes`/`blob_of` at Trap resolution, saved allocation prompt, finished damage and post-Expiration. Return the complete originating log and `encode_state(host.state())`.

## Harness wiring and actual cold continuation

Call the new builder from `native_and_hardened_riftbound_replays_match_at_every_entry`, then `assert_plugin_replay_parity(&entries,&expected,"Lotus Trap / Alpha Strike v13")`. The existing harness compares decide request bytes, verdicts, admitted fold outcomes/deltas, snapshots, both engine views and both plugin views after every entry, restoring each engine snapshot between entries. This supplies the full normal log path with new nonzero v13 data.

Also replay the suffix from the explicit saved mid-allocation checkpoint with **new** NativeEngine, new WasmEngine, and new native/hardened plugin instances. Restore both engines from that same checkpoint and continue with entries starting at the captured next sequence. Reuse the existing comparison loop on this suffix and assert the final snapshot equals the originating host. Merely calling restore on the same plugin instance is not this cold continuation check. Compare the two freshly restored engines' deltas/views to each other; their view-cache initialization need not equal a warm host's incremental delta history. Do not replay the last entry against the final state, and do not construct the saved checkpoint from plugin blob bytes without the table.

Keep the explicit nonzero multiplier/marks/prompt assertions in the builder and at cold restore. End-only snapshot equality could pass a corpus that never exercised the new Damage state. Preserve the original corpus builders and their labels. Use the final Damage wasm paths with the existing ENGINE_BUDGET/PLUGIN_BUDGET; no budget increase or allocator-gas-equivalence claim is implied.

This corpus exercises ordinary damage, per-seat marker persistence, multiplier persistence, saved resolution, cleanup and Expiration. It intentionally has no prevention/multiplier ordering choice, kill replacement, copied ability or borrowed source, so passing it does not close A11, D1, A3 or A4.
