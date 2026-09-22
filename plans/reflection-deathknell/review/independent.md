# Independent review and follow-up

The independent reviewer verified the nine original Reflection regressions and
13 blob-compatibility tests on d2e94ba. No introduced regression or serialization
incompatibility was found.

One P2 gap was reproduced: Reckoner's Arena directly queued a Reflection's
Conquer ability without saving its script. A copy of Kai'Sa - Survivor killed
before that queued ability resolved drew zero cards instead of one.

The follow-up adds that tenth serialized regression, confirms it fails with
zero draws, and routes all production pending-item producers through
`Ctx::enqueue`. Enqueue captures script identity once, preserving it if an item
already has one. This covers Arena, ordinary triggers, activations, played
items, and delayed items as they enter the pending queue, instead of requiring
each producer to remember the snapshot step.

The rules library passes with 4,573 tests and 172 pre-existing ignores after
the follow-up. The original combat and Temporary regressions remain green.
