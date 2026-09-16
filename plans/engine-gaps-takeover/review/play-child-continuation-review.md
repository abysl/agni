# Play child scheduling — independent source/rule review

Read-only WIP review of `takeover/play`, especially Flow::Child, CHILD_WAIT=255,
Here to Help and begin_limited. No production edits/builds. This note supersedes
my earlier recommendation to finish child declaration before the parent's next
Repeat execution in `/tmp/agni-play-final-baseline-review.md`.

## Decisive rule correction

Pinned Core 354.1–3: a played card moves to Chain and becomes Pending; if another
card effect/ability is resolving, continue that effect before any further steps
of playing the new card. Core 158.3/a forbids finalizing/resolving other chain
items until the resolving spell finishes. Core 820.1.d/820.3 makes Repeat another
execution within the same resolution. Thus the order is:

parent execution → enqueue child A → parent Repeat execution → enqueue child B
→ finish parent → handle pending children’s location/optional/payment/legality
choices → finalized child spells receive the normal reaction/priority process.

A permanent child's entry/finalization is likewise deferred. A child spell must
not resolve before the parent resumes. The parent does not wait for the child's
choices, finalization, successful payment, or resolution. Children can still fail
later; queueing successfully is not proof of a successful completed play.

## WIP blockers and the smaller repair

- `chain::run` inserts a parent at `chain.len().saturating_sub(1)`. A pending child
  may exist only in queue, or a permanent child may already have disappeared into
  board finalization. That index can place the parent below an unrelated older
  chain item. Explicit child ID would improve the indexing, but the wait itself
  would still violate the rules above.
- `proceed` and `resolve_top` return for ordinary Resolving items, so a CHILD_WAIT
  parent has no normal wakeup. A new auto-resume branch would fix the hang while
  retaining the wrong ordering.
- 255 is not a reserved engine stage. Dazzling Aurora already uses PLACE=255;
  other helpers pass u8::MAX as a real callback continuation. A global special
  case silently skips their callbacks. Remove CHILD_WAIT and Flow::Child.

Minimal implementation:

1. Separate play construction/enqueue from `advance`. `begin_limited`, whose
   purpose is effect-initiated play, constructs the complete Limited pending item
   and moves the card to Chain, then returns without calling advance. Ordinary
   top-level play may keep enqueue+advance. If another effect path uses the latter
   from a callback, route it through the deferred form as part of its Play port.
2. Keep normal parent Flow::Done/Repeat and Flow::Ask serialization. The parent
   stays at its actual chain position when suspended; never insert relative to an
   assumed child. The chain/queue plus ordinary stage/targets/awaiting already
   serialize this contract. No additional parent-child link or schema field is
   necessary for deferred declaration; v12 Limited remains the required addition.
3. In `proceed`, before ordinary pending-queue advancement, recognize an active
   suspended resolving parent. Only its own existing prompt/face response may
   continue it; leave queued children/tasks unfinalized. Preserve existing kill
   choice dispatch rather than treating every Resolving row as a new wait kind.
   This check matters across requests: `ctx` transient flags alone are insufficient.
4. Parent callbacks finish all executions, including their own hidden picks and
   reveals; normal finish then releases ordinary queue/task processing. Capturing
   trigger occurrences is allowed, but do not finalize them during the parent.
   Retain deterministic pending/task ordering; do not privilege a child by guessed
   physical chain index. Child spells remain Finalized until normal priority passes
   after pending work completes (359.3.b/c).
5. New Here to Help executions should enqueue the allowed battlefield list rather
   than asking their child's location through a bespoke parent LOCATED stage.
   Keep old LOCATED=3 handler for legacy saved-state compatibility only. Adapt the
   Economy repeat regression to two parent picks/reveals first, then child choices.

## Failure and cleanup contract

- Failed/refused enqueue creates no orphan item or effects. Queue construction is
  distinct from later declaration failure. Automatic child cancel removes only
  that pending child, returns its card according to origin, and clears its payment
  pins/temporary declaration state; it does not rerun or undo the completed parent.
- Choices, pool consumption and optional payments occur at child processing time,
  so two children can compete for resources and the later one may fail. A saved
  raw affordability precheck must not prematurely exclude a child based on a
  payment state that will be determined after the parent's remaining instructions.
- If parent execution faults and its existing checkpoint rolls back the callback,
  a child queued during that callback rolls back with it. Successfully committed
  children from earlier stages are not erased just because the parent later ends.
  No broad A5 framework is required, but tests must show queue/effects consistency.
- No stage reservation or extra wakeup protocol is needed. If future linked rules
  require actual child-result callbacks, design a separate typed continuation then;
  do not reinterpret today's successful enqueue boolean as that result.

## Focused acceptance

1. Repeated Here to Help with two hidden units and two allowed fields: save/reload
   after each pick/reveal; no child location prompt/entry/payment occurs until
   parent finishes. Then answer both child location/optional prompts across reload,
   checking exact identities, two arrivals and no lost/duplicated queue entries.
2. Put an unrelated older finalized spell beneath the parent: it remains below
   both new pending children and cannot resolve while parent is suspended.
3. Parent queues a child spell, then performs a visible suffix/Repeat: suffix runs
   first; child payment/finalization follows parent completion, and child effects
   wait for normal reactions/passes. This distinguishes all three milestones.
4. Two payable-looking children but resources for only one: no resource reservation
   during parent resolution; correct sequential final payment/failure return,
   unchanged completed parent, no dangling prompt or pending item.
5. Existing Dazzling Aurora stage255 and legacy Here to Help stage3 fixture still
   invoke the card callback after reconstruction. Run fresh native/hardened replay
   for the new deferred sequence; source inspection alone is not a passing gate.
