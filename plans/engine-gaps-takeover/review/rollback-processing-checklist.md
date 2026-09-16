# A5/A6 implementation checklist

Read-only follow-up against recovered statics (`871e7e45` plus the active A9
cost changes). Recheck after all recovery lanes land, because they also edit
chain and cleanup processing.

## Rollback

`engine/chain.rs::Checkpoint` currently saves the blob, effect/event lengths,
spawn counter and whether a fault already existed. Restore replays the table
from `origin`, truncates effects/events, clamps `collected`, clears `awaiting`
and clears the fault. It does not save `Ctx::deaths`.

`engine/kill.rs::die` appends captured deathknells to `Ctx::deaths`, then writes
the persisted death record and raises `Event::Died`. Restoring the blob and
event length rolls back the latter two, while the captured deathknell remains.
`triggers::collect` subsequently takes that queue. Extend the existing broken
script regression with a real deathknell kill before an invalid effect, and
verify no ghost trigger, death record, move, draw, or changed state survives.
Also seed a preexisting captured occurrence and verify rollback preserves it.

Review every mutable transient field, including `won`, exact `collected`,
`picked`, `awaiting`, and `remembered`. `run` clears/takes some of these after
calling the script but before restoring its checkpoint; explicitly preserve
the continuation contract rather than automatically clearing every queue.
The table index can be invalidated and rebuilt. `origin`, `actor`, `seat`,
zone metadata, options, entry metadata and script registry are currently
unchanged by this path. A1/A3 may introduce further mutable fields that must
join the checkpoint.

## Mandatory processing limits

- `chain::proceed` runs 64 iterations, then narrates that the chain stalled
  and returns normally. Its caller cannot distinguish exhaustion from a
  genuine priority/prompt boundary.
- `engine::settle` runs its auto-answer loop up to `SETTLE_LIMIT`, then returns
  `Ok(())` even if mandatory answers remain.
- `cleanup::run` performs eight lethal passes, then continues to attachment,
  control and showdown processing even if lethal work remains.

Use explicit completion/suspension/exhaustion results or a deterministic
propagated failure. A player prompt or legitimate priority window is a valid
boundary; exhausting a safety limit is not. Whole-request refusal must discard
the projected blob and effects, including any narration and trigger queues.
An accepted continuation must serialize all required work and provide a
deterministic resumption path. Do not merely raise the limits.

Tests should cover one less than, exactly, and more than each limit, including
auto-selected choices and repeated lethal cascades. Assert whether the request
is accepted or refused and what authoritative state remains, rather than only
checking narration or a loop counter. Add a real native/hardened replay case
for the chosen exhaustion policy, and exercise a normal prompt suspension to
ensure it remains accepted.
