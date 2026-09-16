# Trigger saved-item reload regressions

The Forge, Gardens, Heimerdinger and Warmog regressions now serialize the authoritative table and `GameBlob`, decode the saved bytes, rebuild a fresh `Resolved` registry, and issue the next native request from the reconstructed context. Each test compares the saved `ChainItem`, retaining holder/lender, controller, and continuation state before resolving.

Covered paths:

- Forge of the Fluft changes battlefield control while a Lent attach request is pending.
- Gardens of Becoming loses its holder unit while the copied activation is pending.
- Heimerdinger's borrowed Lee Sin activation survives the lender leaving play.
- Heimerdinger's borrowed Gardens activation retains its exhaust payment semantics after reload.
- Warmog's Armor retains the detached-wearer trigger and the dead-wearer no-buff result across reload.

Checks used `/build/agni-takeover/triggers` with eight jobs:

```text
cargo test -p agni-riftbound-turns he_borrows_an_exhaust_ability_a_friendly_unit_itself_only_holds_by_grant --lib
cargo test -p agni-riftbound-turns --lib --no-fail-fast
```

The focused test and full native library suite passed: 4536 passed, 0 failed, 177 ignored. Full log: `/tmp/agni-triggers-reload-full.log`.
