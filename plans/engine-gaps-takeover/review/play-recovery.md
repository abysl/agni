# Play recovery

This branch ports the five saved Play clusters onto the reviewed Economy/A9
baseline. The v12 writer carries `ChainItem.limited` as field 14; readers for
v7, v9, v10, and v11 continue to read the 13-field shape, with mode-slot
migration only before v11. The independent blob fixtures author old v7/v9/v10
inputs and canonical v11/v12 outputs separately. The returned compatibility
suite covers ten cases, including queued and chain rows, Limited round trips,
legacy Here to Help/Swarm Queen stages, and wrong-arity rejection.

Limited child plays are enqueued while a resolving callback runs. `proceed`
pauses queued advancement while a resolving parent is present, including after
snapshot reload; once the parent finishes, children receive their normal
location, payment, and priority flow. Current Here to Help queues every
permitted held battlefield and retains the old LOCATED handler for legacy
continuations. Promising Future uses the same current path while retaining its
old LOCATE resume stage. The repeated Here to Help regression saves after each
face arrival and location choice, rejects an unheld battlefield, checks that
resources are unchanged before child payment, and verifies both child arrivals.
The handed-back native net regression covers the same parent-before-child
boundary with exact card and resource assertions.

Deferred plays preserve effects that are established before child finalization:
pre-play stun and control survive cleanup while a pending child is in flight;
gear attachment follows the eventual unit location; and stored Limited
locations are the locations admitted when the child was enqueued. Revealed
cancellation returns a failed card to its physical owner's deck. Swarm Queen
uses the full item quote for revealed cards, keeps a legacy stage-4 location
resume, and queues the selected card once without restarting deck confirmation.

The root-verified library checkpoint is 4,458 passed, 0 failed, 236 ignored;
the package also reports 10 blob-compatibility and 10 integration tests passed.
Focused gates include cost 15 passed, Bone Skewer 9 passed, Here to Help 8
passed/1 ignored, Promising Future 7 passed, Swarm Queen 9 passed, and net
replay 29 passed on the preceding fresh source checkpoint. Clippy with denied
warnings, whole workspace format check, and diff check pass. Fresh raw modules
are `/tmp/claude-1000/-home-rae-atlas-orgs-andrea-projects-apps-desktop-kai/74772ad4-41d3-41fb-98a2-4dfdb97c0558/scratchpad/lanes/target-rules/wasm32-unknown-unknown/release/agni_engine_wasm.wasm`
(f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3) and
`/tmp/claude-1000/-home-rae-atlas-orgs-andrea-projects-apps-desktop-kai/74772ad4-41d3-41fb-98a2-4dfdb97c0558/scratchpad/lanes/target-rules/wasm32-unknown-unknown/release/riftbound_plugin.wasm`
(4b94470bd62a14ad662f6a012f0893383fcf50386ba13cabf372e6db395c4fe9).
Hardened outputs are `/build/agni-takeover/play-artifacts/engine.hardened.wasm`
(4dcefc139aa16d05d1538b8743717e2fa72979e3a576f5727485c9f5a20eabbe) and
`/build/agni-takeover/play-artifacts/riftbound.hardened.wasm`
(b5e80c52d6818feec2c2e5e756f3b3f8b5fd4a6c9e4af1fe7fda0753d485cc65).
The final root gate includes the saved Repeat/Clockwork optional-choice,
LessEnergy/Accelerate payment, and paired Limited permission/prohibition
mutation tests (`/tmp/agni-play-final2-root.log`). A captured Miss Fortune
grant survives its source leaving; a subsequently active Warden restriction
cancels before rune or Gold payment.

Fresh native/hardened replay passed in 252.29 seconds, including the repeated
Here to Help corpus (`/tmp/agni-play-parity-root.log`). The final Kai
workspace/all-targets gate passed 602 tests with one ignored in 90.51 seconds
after five AI question hints were updated for deferred plays; the derived
prompt-coverage test remains unchanged (`/tmp/agni-play-kai-final-root.log`).
Engine and Kai clippy with denied warnings and scoped edition-2021 formatting
pass. Root artifacts under `/build/agni-takeover/play/` have the hashes above.
The final changes after the artifact build are tests and Kai prompt wording;
engine production source is unchanged. No economy or Play comments were added
to production code. This accepts the five saved clusters, not the remaining
Play clusters or deferred foundations in plan.md.
