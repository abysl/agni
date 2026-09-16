# Costs recovery

This checkout ports the three independent Costs clusters onto the Play v12,
Economy, statics and A9 baseline at `432852f4`:

- `2712d6e6` carries legend-zone Empower eligibility. Counter writes use the
  current in-play predicate, including the v12 capped Empower path, and the
  saved Akali, Jayce, Profiteer and context regressions are enabled.
- `c92d252f` and `a08b6e9a` carry `SelfCost::Disempower`, its
  `Reason::NotEmpowered` refusal, prompt text and activation payment. The
  source is validated before it is exhausted and disempowered; the saved
  Ambessa, Glowstone, Hextech Disc, Mel, Questionable Tome, Kennen and Zed
  cases pass. `a08b6e9a` updates the matching design row.
- `4d661781` carries actor and token provenance. `Event::Empowered` has `by`,
  `Event::Banished` has `by` and `token`, and `empower_by`/`banish_by` plus
  `YouEmpower`/`YouBanish` preserve controller, owner, actor and token
  distinctions through triggers and card scripts.

The additional-stage settlement cluster `d0639152` remains unported. It requires
the separately reviewed A5/D1 continuation and engine-owned undo seam; its
saved early-payment loop also violates declaration/payment ordering.

Final source candidate `95327414` includes the final Play tests/Kai vocabulary,
all item-owned self Empower/Banish actor fixes, actual Legend-zone validation
(including a missing zone), and Disempower preconditions. A prohibition on
readying does not block exhaustion as payment. Saved Questionable Tome and
Time Warp chain items retain their captured actor after control changes and
blob reload with reconstructed tables. These fixtures do not establish the
unimplemented general payment transaction or A1 incarnation semantics.

Validation from the agni project directory, using
`CARGO_TARGET_DIR=/build/agni-takeover/costs` and `CARGO_BUILD_JOBS=8`:

- `cargo test -p agni-riftbound-turns`: 4479 passed, 222
  ignored; compatibility fixtures 10/10; MatchState fixtures 10/10.
- `cargo clippy -p agni-riftbound-turns --all-targets -- -D warnings`: pass.
- Scoped edition-2021 formatting and diff check: pass.
- Fresh release engine, Riftbound and MTG wasm builds: pass.
- Fresh native/hardened replay: pass in 237.91 seconds; log
  `/tmp/agni-costs-parity-root.log`.
- Kai workspace/all-targets: 602 passed, one ignored in 50.89 seconds;
  `/tmp/agni-costs-kai-root.log`. Kai warnings-denied clippy also passes.

Root full package log: `/tmp/agni-costs-final-root.log`. The inventory phase
check passes; this is three recovered clusters, not the final completion audit.
Root SHA-256 artifact identities:

| Artifact | SHA-256 |
| --- | --- |
| raw engine | `f604ab252fd3e419d1cae68cbc5961986c3ef3f8e2a4b6d6488bc3d648236ff3` |
| raw Riftbound | `4b49ed7e038dda5a8ceecc0296a56bf5d65fe78aa4125b585705be5fcd5b567e` |
| hardened engine | `4dcefc139aa16d05d1538b8743717e2fa72979e3a576f5727485c9f5a20eabbe` |
| hardened Riftbound | `3e3ed0c1717ef29a9779854a2c27e4d3f743b18417805fbf9cbd020f1291a28a` |
| hardened MTG | `56350ed1cededb15a59f0c2b31d7623b22c361e2dfc0fb5c2b9aacefe25e5f3c` |

Hardened artifacts are under `/build/agni-takeover/costs-artifacts/`; raw
modules were independently built under `/build/agni-takeover/parity/`.
