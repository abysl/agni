# Copy and Empower review

## Changed paths

- `games/riftbound-turns/src/cards/{keeper_of_masks,leblanc_deceiver,mirror_image,mournful_witness}.rs`
- `games/riftbound-turns/src/cards/mod.rs`
- `games/riftbound-turns/src/engine/{cleanup,ctx,triggers}.rs`
- `plugins/sdk/src/{decide,table}.rs`
- `sim/src/log.rs`
- `plugins/riftbound/tests/projection.rs`

## Root cause

Reflection effects only narrated an unimplemented copy. No effect could replace an existing card face, so a Reflection remained a blank 0-Might token and never acquired the target's script. Mournful Witness declared its Empowered static but omitted its combat-ended trigger; cleanup had no event that identified all combatants.

## Implementation

`Transform` persists a replacement public face through the SDK, simulation fold, table projection, and reload. It preserves token identity and refuses hidden source or destination faces. The simulation variant is appended so prior variant indices remain stable. Reflection cards now transform into their target's current face, so a copy of a copy follows that copied face. The transient Ctx resolves a token's changed face immediately; subsequent requests resolve it from the persisted table face.

Combat cleanup now emits `CombatEnded` with its deterministic sorted combatants. Mournful Witness triggers once when it was among them and becomes Empowered through its ordinary resolver. Steel Paws' existing request/reload test continues to cover paid Empower, persisted Empowered state, and Might counter projection.

## Tests

- Added serialized `Request` coverage for Mirror Image: move, target pick, two passes, effect application, blob reload, copied face, copied script, Might, Temporary, and token identity.
- Enabled the prior ignored LeBlanc, Mirror Image, Keeper of Masks, and Mournful Witness engine-gap tests.
- Added SDK/simulation projection and persisted-state coverage for `Transform`, minting the Reflection through the real spawn-effect path before transforming it.
- `cargo test --locked -p agni-riftbound-turns serialized_play_request` — 1 passed.
- `cargo test --locked -p agni-riftbound-turns reflection` — 16 passed.
- `cargo test --locked -p agni-riftbound-turns empower` — 189 passed, 6 ignored.
- `cargo test --locked -p agni-sim a_transform_effect` — 1 passed.
- `cargo test --locked -p agni-riftbound-plugin --test projection an_entry_level_spawn_takes_the_id_before_the_effect_spawn_that_follows` — 1 passed.

## Remaining limitations

The Empower-filtered run leaves six ignored tests: Escaped Grayback's kill payment, Gangplank Naval's stun/minus-Might/bounce replacement, Mel Newly Awakened's amplified negative-Might effect, Punching Poro's discard payment, Profiteer's disempower payment, and Renekton Brute's Might-threshold trigger. Rune payment-choice work remains coordinated with its owner.

No Kai renderer change is required: it receives the transformed public face. A wire change is required before shipping this effect. Clients replay a `Game` log entry through the host-pinned plugin but also fold its returned verdict through their local `agni_sim::log::Effect`; an older client cannot deserialize `Transform`, despite receiving the new plugin module. Coordinate the requested maintenance/main wire bump and fixture regeneration. No version or lockfile changes were made here.

Official rules evidence: the [Riftbound Core Rules](https://riftbound.gg/wp-content/uploads/sites/67/2025/12/Riftbound-Core-Rules-March-30-2026.pdf) state that copied traits become the copy's traits and that tokenness is intrinsic, so a copied Reflection remains a token.
