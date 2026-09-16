# Correction: Ignored costs retain optional choices

Read-only source/rule confirmation against Play WIP based on e9235605. This supersedes earlier review/checklist language requiring `Price::Ignored` to auto-decline optional costs. That recommendation was incorrect.

Pinned Core 355.1.a chooses whether to pay optional additional costs. 356.2.b.1 adds accepted optional costs; 356.4.f.1 explicitly defines an optional cost as paid if the player decided to pay it, regardless of the actual amount. 356.5.a sets the total to zero, including non-standard costs, when an instruction ignores “any and all costs.” No rule in this sequence removes optional choices. The separate base-cost-only rule 356.1.b.3 even explicitly preserves choosing Accelerate; the distinction is the final amount due, not whether the player may choose.

Consequently, in `engine/play.rs`, `begin_limited` must not pass `OPTIONAL_COST_SLOTS` as forced declines solely because the price is Ignored. Offer applicable Accelerate, printed/granted Repeat and optional additional choices; evaluate affordability with the complete item and final Ignored override. Keep accepted slots for effect semantics, pay zero physical costs, then finalize normally. 805.1.a/805.2 and 820.1.d still apply the accepted Accelerate/Repeat effects. A card-specific instruction forbidding an optional choice would be separate semantics; the supplied Bone Skewer text has no such restriction.

`cost::origin_cost` excludes Ignored from its accelerated monetary component. With the final Ignored override this does not by itself change the numeric total, but it must not suppress ability eligibility or discard the chosen flag. `can_accelerate_item` currently has no Ignored exclusion. The old `begin_ignoring_any_and_all_costs` helper also force-declines; audit any reachable caller or explicitly legacy-only use rather than leave a second incompatible entry route.

Corrected acceptance:

1. Admit an Ignored Limited play of an Accelerate unit with no resources, save table and blob at its optional prompt, reconstruct, accept, and assert zero resource changes plus ready entry. Decline in a separate run and assert normal exhausted entry. A Bone Skewer fixture must also assert its Stun behavior separately rather than let Stun obscure entry readiness.
2. Ignored Repeat spell: accept, choose the required targets/modes for both executions, save/reload, finish twice with no costs consumed. Decline produces one execution. Keep A10's already recorded multiple-instance limitation separate.
3. Ignored optional-rune card such as Clockwork Keeper: accept for zero and obtain its paid-choice effect; decline omits that effect. The accepted slot survives reconstruction.
4. When Costs adds non-standard costs, Ignored waives mandatory physical sacrifice/discard too; accepted optional costs retain their rules-facing choice without performing the waived operation. Distinguish that choice from settlement receipts, as specified in the Costs port design.

Do not preserve an old test whose assertion is simply “Ignored declines every option.” Replace that assertion with the rule-supported accept/decline outcomes above. No builds performed.
