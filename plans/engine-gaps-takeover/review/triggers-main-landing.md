# Five-cluster Triggers recovery on main

Local main merged candidate `1e2951df` at `345b1646`. The merge tree exactly matches the tested candidate. It includes previous main `217ff8ab` and the recovered baseline `915e8cb5`; no neutral engine/SDK protocol changes are included.

The recovery carries saved trigger subjects, attached/projected grants, Granted/Lent holder and lender identities, conservative ordering, assigner-aware combat excess, movement attribution and blob v14 migration. It is the five saved clusters, not the remaining trigger roadmap.

Final acceptance:

- Riftbound library: 4,536 passed, 177 ignored; 11 compatibility and 10 match-state tests passed.
- Full network/Riftbound plugin corpus: 32 passed, including native/hardened replay.
- Corrected independent v12/v13 migration test passed. The original authored fixture had inconsistent target/spec-count data; production readers were not relaxed. Exact target, mode, Repeat, Pending and default-field assertions are retained.
- Registered Warmog/Gardens replay passed in 73.61 seconds, including the holder's buff counter and fresh native/hardened continuations from both Granted and Lent checkpoints.
- Kai workspace/all-targets: 602 passed, one ignored. The gate exposed missing AI descriptions for Altar of Memories and Last Rites; adding those two descriptions fixed the derived prompt test. Explicit fresh module paths resolved the separate missing-fixture failures.
- Engine/net and Kai clippy passed with warnings denied. Scoped Rust formatting, Git whitespace and original inventory checks passed.

Fresh raw engine SHA-256 is `f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3`; Riftbound is `aec8da193be858ed8cc873c79cecf780ab5bfbc7d02e3c2f20cf531b9c02fb74`. The replay harness hardens these using its existing budgets/configurations. Triggers changes the plugin blob to v14; the neutral engine/SDK ABI remains unchanged.

Astra's final review found no new production blocker and conditioned landing on the corrected migration, buff assertion, cold checkpoints and final gates. Root checked those corrections and their logs before merging. The design document's committed conflict markers were removed by preserving the reviewed main document and adding a bounded v14 recovery section.

Logs are retained locally under the `agni-triggers-` prefix. The recovery pass is complete; neutral protocol work, replacement Costs and the broader roadmap remain parked. No remote push or deployment is claimed.
