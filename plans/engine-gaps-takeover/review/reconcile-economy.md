# Economy recovery schema

The economy lane's seat and card rows cannot be applied directly to the
statics/A9 checkpoint. The current v10 seat row is eleven fields and carries
`PlayLock`, the readiness flags, `chosen_champion`, and the two-unsigned
`next_discount`. The economy row uses the same length for unrelated fields and
would reinterpret old v10 bytes.

The merged writer is v11. Its fourteen-field seat row is:

1. setup
2. draws
3. played-main
4. numeric `PlayLock`
5. facedown-look count
6. card count
7. spell count
8. promise array
9. units-enter-ready-this-turn
10. next-unit-enters-ready
11. chosen champion or null
12. gear count played
13. gear ability count activated
14. owned pool `[energy, pooled-power-codes]`

The v11 card row remains the current thirteen-field A9 row, including owned
costed grants and `named`. Card rows are dispatched by their containing blob
version and exact row schema; the decoder never guesses an ambiguous legacy
length.

The decoder dispatches only on explicit `v` 7, 9, 10, or 11. It rejects
unknown versions and wrong row lengths. The supported v10 input is the
integrated statics row with numeric `PlayLock`; v7/v9/v10 fixtures remain
independently authored old inputs and assert canonical v11 migration. A
nonzero legacy `next_discount` pair becomes one permanent Any-card discount
promise, preserving its old scope to Spell/Permanent plays rather than
abilities or triggers. It persists across turn reset, matching the old field's
behavior; zero remains no promise. Locks, readiness, champion, counters,
named cards, owned grants, winner/deaths, and other state survive
independently. The unpublished economy v10 boolean-seat row is rejected because
its slot-6 Repeat and statics mode histories cannot be distinguished safely at
the whole-blob boundary.

Legacy chain and pending items had mode picks beginning at slot 6. In v11,
promised Repeat owns slot 6 and modes begin at slot 7, so v7/v9/v10 chain and
pending rows insert `UNANSWERED` at slot 6 during version-aware decoding.
Current v11 writes one layout and never emits a competing v10 writer.

The economy port keeps A9's owned `CostedGrant` and `cost::of_grant` APIs. It
does not restore `Power::intern`, `Cost::interned`, or `cost::to_script`. The
known first-Flow/Repeat selection and coalesced promised-Repeat behavior stay
documented as C3 debt; promise-index saturation remains an A6 concern.

The six economy commits from `60fab162` through `1c790ef1` were ported into
the merged implementation at `566123d5`; original ancestry is added during
integration. Validation recorded
for the recovered implementation is 4,419 library tests passed with 256 ignored,
warnings-denied clippy, scoped formatting, engine and Riftbound plugin wasm
checks, the economy inventory audit (100 clusters and 381 entries, with no
unmapped entries), and the fresh native/hardened replay corpus. Seven blob
compatibility tests independently author v7/v9/v10 inputs and compare
canonical v11 outputs, including zero/nonzero discount migration, explicit
rejection of the ambiguous economy v10 row, and legacy chain/pending mode
insertion. The follow-up repair adds bounded coverage for the merged economy
seams. A serialized owned Repeat grant is paid after reload and produces two
concrete draw effects. Additional cost is offered and paid from a three-energy
pool, under an excess discount, and through an item-qualified Add source during
Accelerate; the tests assert the resulting resource effects. Repeated
Here-to-Help reloads at the post-reveal location prompt preserve each
execution's remembered destination, reject an unheld battlefield, and place
the two selected units separately. The chain view now computes each execution's
actual specification/target span, carries the trailing remembered suffix, and
clears only the completed execution before advancing. Rocket Barrage now drives
three independently selected mode groups through pending prompts and reload,
and a third repeated target group is checked against its own reconstructed
anchor with a wrong-target refusal. The net parity extension now covers a
persisted Temporal Portal pool and promised Repeat continuation; its review
records the native/hardened result and artifact hashes. The final engine run
reported 4,419 library tests, 7 blob compatibility tests, and 10 match-state
tests passed, with 256 ignored.
