# Triggers landing review

This branch carries the five-cluster trigger recovery on the Damage v13 / trigger blob v14 baseline. The source changes preserve captured death subjects and trigger snapshots, attached and projected grants, Granted/Lent holder and lender identities, conservative trigger ordering, assigner-aware combat excess, and effect movement attribution. A4a stable ability identities, A3 simultaneous ordering, D1 continuation rollback, and A11 controller-selected damage ordering remain deferred.

The migration fixture `v12_and_v13_saved_chain_and_pending_rows_keep_trigger_resume_fields` independently decodes both old layouts. It exercises staged targets, mode and Repeat slots, Noted, awaited references, and v12 Limited rows in chain and Pending entries; the existing independent v12 seat/pool/promise and v13 Noted tests cover the adjacent seat and card migrations. The v14 vector independently checks Granted and Lent rows, Noted `buffed`, and canonical excess ordering and duplicate rejection.

The registered corpus `native_registered_warmog_and_gardens_survive_saved_lent_continuations` deals real Warmog's Armor, Tail-Cloaked Matriarch, and Gardens of Becoming. It checks the printed Equip payment, holder attachment and Might grant, the holder's Warmog buff counter, and the queued Warmog Granted item, then a later Gardens Lent activation discovered from the actual affordance. It saves both Granted and Lent checkpoints, restores a fresh native and hardened engine/plugin pair for each suffix, and compares requests, verdicts, fold results, snapshots, and final state through the suffix. The full log replay also runs through `assert_plugin_replay_parity`.

Focused evidence:

- `cargo test -p agni-riftbound-turns --test blob_compatibility v12_and_v13_saved_chain_and_pending_rows_keep_trigger_resume_fields`: pass (`/tmp/agni-triggers-migration-green.log`)
- `cargo test -p agni-net --test riftbound_turns native_registered_warmog_and_gardens_survive_saved_lent_continuations`: pass; both Granted and Lent native/hardened cold suffixes and full replay (`/tmp/agni-triggers-registered-final.log`)
- `cargo test -p agni-riftbound-turns --all-targets`: 4536 passed, 0 failed, 177 ignored; compatibility 11 passed and MatchState 10 passed (`/tmp/agni-triggers-riftbound-full.log`)
- `cargo test -p agni-net --test riftbound_turns`: 32 passed, 0 failed, 0 ignored (`/tmp/agni-triggers-net-full.log`)
- `cargo clippy -p agni-riftbound-turns -p agni-net --tests -- -D warnings`: pass (`/tmp/agni-triggers-clippy-final.log`)
- `cargo build -p agni-engine-wasm -p agni-riftbound-plugin --target wasm32-unknown-unknown --release`: pass (`/tmp/agni-triggers-wasm.log`)

The branch was rebased by merge onto the green main baseline `32620ad3`; final full-suite and clippy reruns after that merge are recorded by the landing agent.
