# Economy final bounded review

**Disposition: no remaining production blocker identified in the reviewed corrections.** Approve the bounded economy recovery once the helper test followup is included and the final combined tip passes the in-progress gates below. This is not an approval of the separately documented A10/A6/A5/D1/A11 rules debt.

Reviewed main economy HEAD `746ef6e1`, especially residual fixes `30a3b25c`; reviewed helper `c56accae` and its current uncommitted target-item persistence followup in `/tmp/agni-takeover-economy-docs`. Read-only inspection, no builds or tests run by this reviewer. The earlier five source corrections remain accepted from the previous note.

## Residual production corrections

- `chain::execution_bounds` walks the finite supported execution range, uses each execution's actual mode/spec/count layout, and returns both current bounds and the end of all declared targets. `execution_view` now appends only the trailing remembered suffix to the current declared group, while keeping current-group spec counts. Existing `remembered_cards` therefore reads the same current memory in the sliced callback view and the full pending/resolving item.
- On a completed execution, the runner truncates the original item's suffix at the global declared-target end and clears awaiting before incrementing execution. It retains later declared target groups and all mode slots. Suspension does not truncate memory. No new schema or hidden continuation storage was introduced.
- Accelerate retains its hypothetical item with the slot enabled and passes `Paying::Item(&accelerated)` into affordability. Item-qualified Add sources now see the same request identity as the cost quote. Additional-cost affordability continues to quote the whole enabled-slot item before discounts/pool consumption.
- A9 ownership, v7 discount preservation, explicit rejection of unpublished economy-v10 seat layout, normal statics mode-slot migration, actual third mode enumeration, and bounded relative-target group offsets remain intact.

## Test adequacy

- Here to Help now exercises real owned-Repeat admission and two resolving executions, committing table state and round-tripping the actual blob/item across both hidden-face arrival and LOCATED choices. The first chosen unit lands at battlefield 3, then the second independently lands at battlefield 1; the source completes without fault. This directly exercises the earlier missing suffix. Its use of `remembered_cards(...).last()` means it is not an isolated mutation test for clearing every old suffix entry, but the production clear is explicit and correct by inspection; no additional blocking test requested.
- The owned-Repeat test reloads the grant, obtains the normal optional prompt, pays one energy, and observes two draw executions. The additional-cost test exercises both banked energy 3 against printed 1 plus optional 2 and an excess discount of 3 with no ready runes. Accelerate's synthetic Add source requires `Paying::Item`, so that regression exercises the actual repaired context seam and completes the accelerated play.
- Helper `c56accae` plays Rocket Barrage with printed plus promised Repeat, reconstructs between mode/target requests, chooses three modes/targets, consumes the promise and observes all three real resolution results. The relative-target helper verifies that execution 3 uses its own anchor. Its followup explicitly inserts the tested item into the blob, encodes/decodes it, and retrieves it before checking candidates; include that followup, since the original helper only round-tripped a blob without the test item.
- The new real-plugin replay uses registered Jhin to bank resources, Temporal Portal to establish a Repeat promise, and registered Rally the Troops to spend banked energy and pay Repeat. It checks the suspended optional prompt before consumption, final promise/pool consumption, four recycled power runes and two actual draws, and adds both the suspended and completed logs to native/hardened parity. The corrected rune accounting reflects `open_m9_game`'s setup/channel behavior; the earlier 5-versus-3 expectation was a fixture error, not evidence of a production payment defect.

## Final conditions and limits

`git diff --check 1b517611..746ef6e1` passed during this inspection. Root reports the corrected parity scenario passed against its earlier tested artifact, but main must finish the fresh artifact build/hardening and rerun expanded parity on the final production tip. Include the helper test commit and its item-persistence followup, run their focused tests, and complete the already-running full turns/blob fixtures, clippy/format, fresh wasm/parity and Kai gates on the final combined checkout. No broad additional rules audit or test expansion is requested by this note.

The previously documented arbitrary-instance, replacement-continuation, pointer-identity, and finite-limit work stays assigned to its planned batches. This note only closes the bounded recovery regressions and their targeted validation requirements.

Coordinator followup: the target-item persistence repair is committed as
`33a8f1a5`, following `c56accae`; both were reviewed and handed to the main
worker for the combined gate. The clean helper tip is preserved as
`review/economy-modal-tests`.
