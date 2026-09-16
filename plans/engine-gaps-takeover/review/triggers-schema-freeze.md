Trigger port freeze

Baseline: /tmp/agni-takeover-triggers, takeover/triggers, 68ce096f (Damage v13). No source edits at freeze.

Schema: reserve plugin BLOB_VERSION 14. Preserve v13 SeatState(16), CardState(15), ChainItem(14), Limited and Economy/A9 fields. Noted becomes five fields with buffed; v13 and older four-field Noted rows decode as buffed=false. Granted/Lent use ItemKind tags 4/5 and four fields [holder,lender,index]; tags 0..3 remain three fields and old versions reject tags 4/5. Add xd rows [seat,zone,amount], preserving None versus Some(0), canonical replacement and expiration. ChainItem remains 14 fields; Pending retains every nested Noted and current awaited/remembered fields. Independent v12/v13 fixtures are required; no version-byte-only migration.

Production touchpoints: state.rs version/readers/writers; engine/ctx.rs Event::Moved and arrived; engine/triggers.rs matching and source/departed data; engine/kill.rs death snapshots; engine/combat.rs explicit assigner/side excess; engine/activate.rs/attach.rs/statics.rs/targets.rs and affected card scripts from the five saved commits. Moved.by uses resolving item controller, never request/pass actor. Excess after showdown.take uses explicit local assigner/side. A3/A4/A4a/D1/A11/A6 remain deferred.

Immutable saved mapping:
- e980b53a: altar_of_memories, shard_of_undoing, spectral_centaur, vanguard_helm, vicious_snapjaws, viktor_leader, wraith_of_echoes; UnitDies watcher/source-self/friendly gear/enemy negatives; Noted/departed snapshots.
- 3fa5ba42: blighted_battleaxe, boneshiver, cull, dorans_ring, eye_of_the_herald, forgefire_cape, last_rites, pendulum_blade, recurve_bow, sacred_shears, skyfall_of_areion, svellsongur, trinity_force, warmogs_armor, world_atlas; attachment/wearer Grant::Ability and copied/mirror coverage.
- 57f1f8ac: forge_of_the_fluft, gardens_of_becoming, heimerdinger_inventor; holder payment/exhaust and queued pending activation after control/zone changes.
- 786200d1: hextech_gauntlets, sivir_ambitious, trapping_grounds, tryndamere_barbarian, vi_piltover_enforcer, yeti_brawler; excess records plus Granted/Lent holder/lender detach/dead-lender/copy-self tests; the_zero_drive remains ignored.
- 7dfaef5f: ahri_nine_tailed_fox, back_alley_bar, blast_cone, pirates_haven, the_dreaming_tree, volibear_imposing plus saved Mask/Lillia/Smoke-and-Mirrors continuation evidence; Who::Any/Enemy, Readied/ChosenFriendly and mover actor.

Mandatory commit-4 pending tests: forge pending activation after hand change; gardens pending activation after unit leaves; heimerdinger pending lender departure and holder-only exhaust; svellsongur copied text resolves as wearer; warmogs detach/dead wearer outcomes. Hextech equipment-cost ignore remains separate.
