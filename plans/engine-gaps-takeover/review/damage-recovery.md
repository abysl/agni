# Damage recovery

The five saved Damage clusters are recovered onto the integrated Play, Economy
and first-three Costs baseline. The blob advances to v13 with explicit appended fields for
per-seat spell bonus binding, per-card damage multipliers and ordered
per-controller damage marks. Version 12 rows decode with zero Damage state;
the test fixture writes v12 CBOR independently and checks a v13 round trip.

The damage pipeline is ordered as no-damage, bonus damage, prevention,
multiplier, then damage marks and the event. Unit-scoped `Next` prevention and
the existing `All`/numeric entries are preserved. Combat assignment reads
the multiplier and lethal-damage statics, while cleanup reads the controller
that marked damage. Resolving chain items remain available while callbacks run
so item-owned damage keeps its actor.

The recovered tests include saved and reconstructed v11 location continuations
for Here to Help, Promising Future and Rek'Sai, the Elder Dragon distinct
location and lethal-marker cases, and Lotus Trap damage multiplication.
Controller-choice ordering (A11) and continuation transaction rollback (D1)
remain deferred seams; this lane does not claim either as complete.

Final source corrections restore empty-chain lethal cleanup, distinguish finite
prevention from unlimited immunity during combat assignment, and use the live
resolving item for Hierophant's enemy-source check. Astra's source review and
four discriminating fixture repairs are recorded in
`damage-corrections-review.md`; duplicate and descending damage-mark rejection
are both retained.

Root validation uses explicit build caches and at most eight Cargo jobs:

- Full Riftbound package: 4494 library tests passed, 214 ignored; 10 independent
  blob compatibility and 10 MatchState tests passed. Log:
  `/tmp/agni-damage-accepted-root.log`.
- Riftbound and agni-net all-target warnings-denied clippy passed:
  `/tmp/agni-damage-accepted-clippy-root.log`.
- Fresh release engine/Riftbound/MTG Wasm builds and hardening passed.
- Kai workspace/all-targets: 602 passed, one ignored, 55.18 seconds;
  `/tmp/agni-damage-kai-root.log`. Kai warnings-denied clippy also passed.
- The Damage inventory phase check passes with no unexplained missing tests.
  This is not the final completion audit.
- Scoped edition-2021 formatting passes. Native cold-checkpoint discovery passes
  after repair 68ce096f, and final net clippy passes.
- Expanded native/hardened replay, including the real Damage corpus and fresh
  engine/plugin cold suffix, passes in 296.43 seconds:
  `/tmp/agni-damage-parity-corrected-root.log`. The earlier helper-only genesis
  decoding failure is corrected and independently covered by the native
  checkpoint-discovery test.

The new real-session Lotus Trap/Alpha Strike corpus checks exact resource
payment, a saved Resume with multiplier 2 and marks `[(0, 2)]`, ordinary death
cleanup and XP, and actual EndTurn expiration. The cold suffix gate uses fresh
native and Wasm engine/plugin instances and the full engine snapshot.

Artifact identities (SHA-256):

| Artifact | SHA-256 |
| --- | --- |
| raw engine | `f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3` |
| raw Riftbound | `fe04eea88fe278bda08561af509f127c9124bcda1a5a6a55c40310075e3ad970` |
| hardened engine | `4dcefc139aa16d05d1538b8743717e2fa72979e3a576f5727485c9f5a20eabbe` |
| hardened Riftbound | `75543cc2ba39083b08b5ee882c4db0e9dd5295355163093e70b0f23db0eda527` |
| hardened MTG | `56350ed1cededb15a59f0c2b31d7623b22c361e2dfc0fb5c2b9aacefe25e5f3c` |

Hardened artifacts are under `/build/agni-takeover/damage-artifacts/`; raw
modules were built under `/build/agni-takeover/parity/`.

Final reviewed candidate is `68ce096f`. Integration preserves original saved
tip `ba225839` as an ancestor, and `review/damage-final` retains the port's
development history. Root verified every non-plan file matches that candidate;
conflicts were resolved using the reviewed v13 source while preserving the
integration branch's current plans and prior gate records.
