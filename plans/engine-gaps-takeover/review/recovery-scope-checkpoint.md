# Recover work close to completion

The user reiterated that this task is to recover work close to completion, finish it, and merge it to main without extending scope. This supersedes the neutral completion steps in the earlier side-branch landing plan.

- Completed: the reviewed engine/Kai recovery baseline is on local main at `915e8cb5`, with evidence recorded at `32620ad3`.
- Completed: the existing five-cluster Triggers candidate passed its migration/replay acceptance and relevant engine/Kai gates, and merged to local main at `345b1646`.
- Parked: the neutral rollback protocol, SDK preflight and synthetic acceptance harness. Their remaining work is substantial and is not a dependency of the Triggers landing.
- Parked: replacing the rejected fourth Costs implementation `d0639152`. The first three recovered Costs clusters are already on main. The rejected implementation is preserved, not merged or counted as delivered.

## Preserved neutral checkpoints

| Checkpoint | Original repository ref | Disposition |
|---|---|---|
| `4e10886c` | `codex/neutral-protocol-paused` | SDK roster/cap work and synthetic harness improvements preserved as WIP. The last focused valid-cap test passed; remaining Peek boundary and broader protocol acceptance are not claimed. No new full gate after the stop instruction. |
| `3f9f9db8` | `codex/neutral-restore-wip` | Separate staged snapshot decoder repair. A real active snapshot restored natively/raw but trapped at the standard hardened stack limit. Splitting field/group decoding passed that same cold Wasm test at the unchanged limit. A subsequent strict tagged-version regression failed before its correction; the correction passed 93 sim tests and five goldens. Final refreshed artifacts and source acceptance remain pending. |

The diagnostic larger stack setting was never retained in production. The staged decoder repair is not on main. Before resuming, read the exact review notes and rebuild artifacts from the selected source tree; local intermediate artifacts are not a release baseline.

Remaining neutral acceptance includes ordered disclosure obligations, independent actual-roster Peek controls, full mechanical rollback/counter assertions, capability and migration boundaries, and hardened session/client delivery. None of this starts automatically as part of the recovery cleanup.
