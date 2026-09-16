# Mask regression

Added the cross-card regression in `games/riftbound-turns/src/cards/mask_of_foresight.rs`.
The test drives Smoke and Mirrors through entry, target picks, trigger handling,
and chain passes while Lillia is already defending. It covers the cleanup
designation of the swapped-in Temporary Sprite, Mask resolving before Lillia's
Move trigger, and Lillia's recorded origin for the later Sprite. The arriving
Sprite ends at 4 Might, the later Sprite remains at 3, both Sprites are at BF1,
their battlefield total is 7, and Lillia keeps her earlier +1 at base. The test
round-trips the table and blob through reconstruction between each prompt or
pass so the result also exercises persisted state.

The current engine exposes an `OrderTriggers` prompt for these non-simultaneous
triggers. That prompt is retained as an A3 compatibility path so this outcome
regression can run against the current engine; it does not represent legal
player freedom under rule 383.2.c. A3 still needs to capture trigger timing and
resolve the ordering without offering that choice.

Validation:

- `CARGO_TARGET_DIR=<lane target> CARGO_BUILD_JOBS=8 cargo test -p agni-riftbound-turns cards::mask_of_foresight::tests`: 5 passed
- `CARGO_TARGET_DIR=<lane target> CARGO_BUILD_JOBS=8 cargo test -p agni-riftbound-turns`: passed
- `CARGO_TARGET_DIR=<lane target> CARGO_BUILD_JOBS=8 cargo clippy -p agni-riftbound-turns --all-targets -- -D warnings`: passed
- `cargo fmt -p agni-riftbound-turns --check`: passed

No engine change was made. The scenario covers the existing trigger and cleanup
paths and records the A3 trigger-ordering debt described above.
