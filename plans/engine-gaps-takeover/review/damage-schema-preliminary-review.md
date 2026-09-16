# Preliminary Damage v13 migration review

Read-only inspection of the moving `/tmp/agni-takeover-damage` candidate. This is not final approval or a damage-pipeline review; no builds or edits. Findings were sent directly to `luna_ui_resume` and root before handback.

The inspected schema preserves the v12 SeatState's 14-field prefix, appending next-spell bonus and `(item, bonus)` as fields 15/16. Existing promises, readiness flags, champion, gear counters and pool remain in order. CardState preserves its 13 fields, including A9 owned costed grants and named card, then appends multiplier and damage marks. ChainItem remains 14 fields in v12/v13; Limited, execution, awaiting, targets, and optional/mode slots are not shifted. Legacy v11 remains 13 fields. Prevention correctly uses a versioned 3→4 field layout with the new unit field read only in v13.

Three concrete decoder corrections were requested:

1. SeatState's length guard checked v12==14 but accidentally omitted v11 after replacing the old `version >= 11` check. Require v11 and v12 length 14; malformed v11 lengths must refuse rather than consume a guessed body. This is a new malformed-input acceptance, not proof of damage to valid v11 documents.
2. `marks_of` accepts unsorted or duplicate seat keys, while `mark_damage` assumes sorted unique keys for binary search. Reject non-strictly-increasing keys on decode; test duplicate and descending rows. This is specific to the newly introduced field, not an expansion of the general A6 limits audit.
3. Prevention's `Amount::Next` boolean arm must require v13. Otherwise an invalid legacy bool acquires a new meaning when labeled v12 or earlier. Preserve old All/null and N/unsigned semantics.

The Here to Help, Promising Future and Swarm Queen `legacy_v11` helpers now lower SeatState 16→14 and CardState 15→13 as well as ChainItem 14→13 in chain and pending rows. They therefore do not merely relabel the version or remove Limited. Their generic fallback still copies `pv` unchanged: constrain these helpers to absent/empty preventions, or explicitly lower only old-representable prevention rows. Never produce a v11 document containing a v13 unit/Next prevention. These helpers derive historical bodies from current writers and supplement, rather than replace, independent old-byte fixtures.

`tests/blob_compatibility.rs` retains independently written old layouts, including v11 staged items and v12 Limited items. Keep those old input writers intact. The current expected canonical writers still emit v12; expected output must be separately authored for v13's version and appended defaults, not obtained by calling the new production encoder. Existing v11/v12 ChainItem bytes need no layout change beyond the surrounding canonical version.

Before final review, add an independent nonzero v12 migration vector covering SeatState promises/pool, gear counts, chosen champion/readiness; CardState named/control/owned grants; and old three-field prevention. Assert all retained values and zero/empty defaults for all new Damage fields, then v13 roundtrip with nonzero bonus/multiplier/multiple damage marks and unit/Next prevention. The initial new v12 test uses empty/default legacy payloads, so alone it cannot catch dropped or reordered legacy values. Keep the independent v7/v9/v10 compatibility cases as schema fixtures.

No dropped valid v12 field or changed ChainItem slot meaning was found in this bounded read. Recheck the worker's fixes and final fixtures against a stable candidate; this note does not approve unfinished damage arithmetic, prevention use, or deferred A11/D1 behavior.
