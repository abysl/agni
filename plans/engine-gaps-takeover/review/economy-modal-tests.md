# Economy modal and target regressions

The final economy follow-up adds the missing persisted interaction coverage:

- `engine::prompts::tests::a_reloaded_owned_repeat_is_offered_paid_and_runs_the_card_twice`
  decodes an owned `CostedGrant` Repeat, pays it after reload, and proves both
  executions through two concrete draw effects.
- `engine::play::tests::additional_cost_uses_the_full_item_with_pool_or_excess_discount`
  covers a printed one plus additional two cost from a three-energy pool and
  under a three-energy discount, including the resulting resource state.
- `engine::play::tests::accelerate_affordability_uses_an_item_qualified_add_source`
  proves that an item-qualified Add source is visible to Accelerate affordability
  and that the selected payment emits the expected rune effects.
- `cards::here_to_help::tests::repeated_here_to_help_keeps_each_saved_pick_across_reload_and_uses_both_held_fields`
  reloads at the post-reveal location prompt for both executions, refuses an
  unheld battlefield, and applies distinct remembered destinations.
- `cards::rocket_barrage::tests::printed_and_promised_repeats_keep_three_mode_groups_across_reconstruction`
  chooses three independently selected mode groups through pending prompts and
  reconstruction, exercising the promised Repeat path.
- `engine::targets::tests::a_third_repeated_group_reads_its_own_anchor_after_reconstruction`
  checks a third execution's relative target anchor and rejects the second
  execution's target.

The final Riftbound run passed 4,419 library tests, 7 blob compatibility
tests, and 10 match-state tests, with 256 ignored. Native/hardened replay,
warnings-denied clippy, scoped formatting, wasm builds, inventory audit, and
the Kai workspace `--all-targets` test and clippy gates passed as recorded in
the economy review notes.
