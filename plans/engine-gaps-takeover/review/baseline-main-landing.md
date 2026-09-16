# Recovery baseline on main

Local main merged the reviewed recovery at `915e8cb5`, preserving previous main `20af74aa` and integration `4d22159c`. The merge tree exactly matches the reviewed integration tree. Production is unchanged from `3973e4bf`; subsequent commits are documentation only.

Included: Rules, Statics and owned cost grants, Economy, Play, the first three Costs clusters, Damage, the pure match-state foundation, Kai destination controls, preview scaling, trash/banishment viewer, activity-based end-turn confirmation, stacked counter display and AI prompt compatibility.

Excluded: pending Triggers v14 recovery, neutral effect-group protocol and SDK changes, rejected fourth Costs implementation, full rematch integration and unresolved exact playtest cases. The end-turn guard tracks attempted local actions, not authoritative accepted card plays; that distinction remains a named UI refinement.

Fresh validation on the identical production tree:

- Riftbound package: 4,494 library tests passed, 214 ignored; 10 blob compatibility and 10 match-state tests passed.
- Kai workspace/all-targets: 602 passed, one ignored.
- Riftbound/agni-net and Kai workspace/all-targets clippy passed with warnings denied.
- Scoped edition-2021 Rust formatting and Git whitespace checks passed.
- Fresh release engine, Riftbound and MTG Wasm builds passed.
- Native/hardened full-game replay passed at every entry, including saved damage and fresh-instance suffixes, in 298.16 seconds. The harness hardened the supplied fresh engine/plugin bytes using its existing configurations and budgets.

Raw artifact SHA-256:

| Module | Hash |
|---|---|
| Engine | `f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3` |
| Riftbound | `fe04eea88fe278bda08561af509f127c9124bcda1a5a6a55c40310075e3ad970` |
| MTG | `dcaf9cd4e1ef2339ca5483bfbbe4c3984726f0a83b258efbee631ab45826506f` |

Commands used the project's devenv, explicit build caches and eight Cargo jobs. Validation logs are retained locally with the `agni-landing-baseline-` prefix. No remote push or deployment is claimed.
