# Economy parity corpus review

This bounded patch changes only
`orgs/andrea/projects/agni/agni/net/tests/riftbound_turns.rs`. It extends the
native/hardened replay corpus with a public-action economy path at source
revision `7ca6bfb7aeea8995d0f88cac9e94bc7870c70c37` (`7ca6bfb7`).

The scenario uses `open_m9_game` with seven explicitly dealt Mind runes, Jhin
- Murderous Artist, Temporal Portal, and the registered Rally the Troops spell.
`StartGame` channels two more runes, so the test records the actual nine ready
runes immediately before Rally is played. Moving Jhin to a battlefield and
resolving priority runs its public Add trigger, banking one energy and one
rainbow in the pool. Activating Temporal Portal uses that banked rainbow,
exhausts the Portal, and records its Repeat promise.

Moving Rally through the public game action opens the `OptionalCost` prompt; the
test checks the promised Repeat slot specifically with `SLOT_PROMISED_REPEAT`.
It saves the admitted log and state at that suspended prompt, answers `yes`,
and verifies `repeats() == 1`, an empty promise slot, an empty pool, four ready
runes consumed, and exactly four runes recycled to the rune deck. It captures
the five-card hand before play and requires the final hand to be
`before - 1 + 2`, with two Rally narration effects and one repeat marker. The
suspended state and complete result are both passed through the existing
native/hardened replay parity helper.

Validation from the exact checkout and parity cache:

* `devenv shell -- cargo fmt --all -- --check` passed.
* `git diff --check` passed.
* `--no-run` compilation passed.
* The focused native/hardened corpus command passed: `1 passed; 0 failed; 27
  filtered out; finished in 174.96s`.

```
AGNI_ENGINE_WASM=/build/agni-takeover/parity/wasm32-unknown-unknown/release/agni_engine_wasm.wasm \
AGNI_RIFTBOUND_WASM=/build/agni-takeover/parity/wasm32-unknown-unknown/release/riftbound_plugin.wasm \
CARGO_TARGET_DIR=/build/agni-takeover/parity CARGO_BUILD_JOBS=12 \
devenv shell -- cargo test -p agni-net --test riftbound_turns \
native_and_hardened_riftbound_replays_match_at_every_entry -- --nocapture

test native_and_hardened_riftbound_replays_match_at_every_entry ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 174.96s
```

Raw artifact hashes used by that run:

```
f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3  /build/agni-takeover/parity/wasm32-unknown-unknown/release/agni_engine_wasm.wasm
75e00e5152983e935761757d13a5a8ed7d41a1bb79d10e86ba5845cd72875517  /build/agni-takeover/parity/wasm32-unknown-unknown/release/riftbound_plugin.wasm
```

After the economy payment, execution-view, and reload repairs, the same
expanded corpus was rerun from source revision `746ef6e1` with fresh raw
artifacts. It passed in 181.93 seconds (1 passed, 27 filtered), including the
persisted pool and promised-Repeat continuation.

Fresh raw artifact paths and hashes:

```
f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3  /tmp/claude-1000/-home-rae-atlas-orgs-andrea-projects-apps-desktop-kai/74772ad4-41d3-41fb-98a2-4dfdb97c0558/scratchpad/lanes/target-rules/wasm32-unknown-unknown/release/agni_engine_wasm.wasm
42596cd379fb7c723a0686bcf43e3536018c83b8ce976e45bf9101b0a8d7d54f  /tmp/claude-1000/-home-rae-atlas-orgs-andrea-projects-apps-desktop-kai/74772ad4-41d3-41fb-98a2-4dfdb97c0558/scratchpad/lanes/target-rules/wasm32-unknown-unknown/release/riftbound_plugin.wasm
```

The corresponding hardened Kai artifacts are under
`/build/agni-takeover/economy/`:

```
4dcefc139aa16d05d1538b8743717e2fa72979e3a576f5727485c9f5a20eabbe  engine.wasm
256a0b64809aae7a2db467b3a0192eaac97a432cfc8b9175cd713eb33cd99c1e  riftbound.wasm
56350ed1cededb15a59f0c2b31d7623b22c361e2dfc0fb5c2b9aacefe25e5f3c  mtg.wasm
```
