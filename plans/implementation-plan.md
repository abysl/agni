# agni — Implementation Plan

## Phase 0 — Scaffold ✅

- [x] Workspace, `spirit-sdk` path dependency with a test that fails if removed
- [x] devenv shell, Bevy 0.19, mold linker, dynamic-linking dev build

## Phase 1 — Renderer ← current

- [x] 3D table, camera framing, lighting
- [x] Hand layout as a fan; cards ease to their slots
- [x] Hover to raise; drag to pick up; drop on the table to play
- [x] Card faces: real art via spirit — [hobbit-hand.md](hobbit-hand.md), all three phases done
- [x] Board placement respects where you dropped; reorder within and across zones
- [ ] Snap-back animation when a move is rejected

## Phase 2 — Decide the shape before building it

Cheaper to decide now than to migrate later. See `wiki/design/architecture.md`.

- [ ] **Zones.** `Zone` is Hand and Board. Games need their own (exile,
      battlefields, rune decks) — generic parameter, trait, or an open enum?
- [ ] **Plugins: data or wasm?** Declarative effects are sandboxed and portable
      to firmware but limited; wasm is expressive but hard to make deterministic
      and small. The e-ink target is the constrained consumer; let it decide.
- [ ] **Replay format.** Action log, plus periodic state hashes for desync
      detection? What a rules-version bump does to old replays.

## Phase 3 — Core simulation

- [ ] State, action, and the `apply(state, action) -> state` step
- [ ] Turn structure and a resolution stack
- [ ] Replace `apply_drops_directly` with real legality checking
- [ ] Determinism test: replay a corpus, assert final state hashes match,
      cross-platform

## Phase 4 — Replay

- [ ] Log format, serialization, versioning
- [ ] Replay to any point; divergence detection

## Phase 5 — Content over spirit

- [ ] Card sets addressed by spirit content identity
- [ ] Plugin loading per the Phase 2 decision
- [ ] Rules identity as part of the replay compatibility key

## Phase 6 — Peer-to-peer

- [ ] Session establishment over spirit/iroh
- [ ] Input exchange and ordering; desync detection

## Phase 3b — Rules enforcement (Riftbound first)

The rules engine has its own plan: the milestones in
[`wiki/design/rules-engine.md`](../wiki/design/rules-engine.md) and their
state on the docs site's agni status page. All eight milestones, M0
through M8, are landed.

## Phase 7 — Consumers

- [ ] `abyss-walker` boundary (FFI or IPC)
- [ ] mechanist e-ink boundary
- [ ] First real game's rules
