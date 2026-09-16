# Trigger recovery combat assignment checkpoint

The combat excess recovery now has three native regressions in `engine/combat.rs`:

- `elder_dragon_lethal_is_used_when_excess_is_recorded` runs an actual two-target assignment with a higher-Might defender and verifies the assigner-aware one-damage lethal threshold records seven excess.
- `finite_prevention_and_lotus_multiplier_are_included_in_excess_threshold` resolves a real assignment after a two-times damage multiplier and finite combat prevention, verifying the remaining three excess.
- `both_combat_assigners_keep_distinct_excess_records` resolves both assignment directions with Elder Dragon lethal statics and verifies independent four-point records for seats zero and one.

Focused commands, using `CARGO_TARGET_DIR=/build/agni-takeover/triggers` and eight build jobs, all passed:

```text
cargo test -p agni-riftbound-turns elder_dragon_lethal_is_used_when_excess_is_recorded --lib
cargo test -p agni-riftbound-turns finite_prevention_and_lotus_multiplier_are_included_in_excess_threshold --lib
cargo test -p agni-riftbound-turns both_combat_assigners_keep_distinct_excess_records --lib
```
