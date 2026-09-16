# A9: costed-grant interner audit and bounded repair

Astra read-only audit of `/tmp/agni-takeover-reconcile/orgs/andrea/projects/agni/agni`. The recovery tree advanced during inspection to `35bdbb85` (reviewed statics recovery). No production edits or extra test/build processes were run against the worker's tree.

## Finding and evidence level

The interner introduces process/instance history into metered execution and retains heap outside serialized state. This is a concrete conflict with the plugin's pure-request contract. The dependency chain below is present in the actual implementation. I did not reproduce a current shipped card crossing the default 100,000,000-gas budget; that stronger claim is not established by this audit.

1. `cards/mod.rs:1124–1178` defines `static INTERNED_POWER: Mutex<Vec<&'static [Power]>>`. `Power::intern` does a linear first-equal search over every previously seen nonempty power vector. A miss allocates with `to_vec().into_boxed_slice()`, leaks it with `Box::leak`, and appends to the global. A hit returns early. The input vector's contents, its insertion position, and all earlier calls therefore determine work. Empty vectors are the one cache-free special case.
2. `state.rs::CardState::read_version` reconstructs each persisted costed grant with `Cost::interned`; both `lib.rs::decide` and `present.rs::present` decode GameBlob. A read of persisted game state therefore changes this global. Decode failure after an earlier grant has been read does not undo that mutation.
3. `engine/cost.rs::to_script` is the second production call site. `cards/kennen_storm_of_shuriken.rs::lend_flow` calls it when granting Flow equal to the target spell's printed cost. `Ctx::grant` stores the result in the blob, but the leaked slice and insertion history are not part of that blob. Dropping/expiring the grant or resetting the game does not reclaim them.
4. `plugins/harden/src/lib.rs::harden` injects `ConstantCostRules` gas metering after stack limiting. Search-loop branches and allocation work are in the guest and are metered; the cache is not an unmetered host optimization. The default plugin memory cap is 256 wasm pages (16 MiB), and the default gas budget is 100,000,000.
5. `engine/host/src/lib.rs::WasmModule` owns one wasmi Store, Instance and Memory. `call` resets `gas_left` to the per-call budget and resets the wasmi fuel backstop. It does not instantiate a new module, restore guest memory, clear globals, or reset the allocator. `gas_left()` allows direct observation of remaining gas.
6. `sim/src/engine.rs::AbiPlugin` retains that module between calls. Gas-exhausted or trapped `decide` returns a rejected Verdict; exhausted/trapped `view` returns an empty PluginView. A sufficiently tight budget can therefore turn the extra history-dependent work into different externally visible behavior.
7. `net/src/{host,client}.rs::plugin_view` calls `view` on the same mutable PluginModule used for decisions. View calls are not entries in the deterministic input log and differ between clients. Fresh restoration/reconstruction can also start from the same persisted blob with a cold instance, while another instance retains previous grants and games. A rejected/speculative call or failed decode can leave interner state that no committed game log describes.
8. The SDK guest wrapper retains only its last `REPLY` Vec in the expected request/reply plumbing; it replaces that Vec and frees incoming requests. It has no registry/reset hook for `INTERNED_POWER`. Existing generic engine snapshot/restore serializes engine state, not arbitrary plugin heap or the leaked slice pool.

Two histories can therefore execute the same serialized request with different search lengths or allocate on only one side. Exact same accepted log replay on fresh instances may still populate equal caches; that narrower successful case does not establish independence from view calls, failed requests, saved-state reconstruction, or previous games. The fix should remove this newly introduced semantic cache, not claim that this one change proves all allocator-level gas costs in the guest history-independent.

## Scope and all callers observed

Static printed cards intentionally use `cards::Cost { energy, power: &'static [Power] }`; `Keyword`, Card, Grant and static card literals rely on cheap Copy values and constant slices. Keep that API.

Runtime state currently uses `CardState.granted_costed: Vec<(Keyword, Expiry)>`, forcing dynamic data into the static API. The interner was added to bridge that mismatch.

The runtime readers/writers requiring edits are:

- `state.rs`: CardState field, defaultness, write/read_version, and busy-blob fixture.
- `engine/ctx.rs`: `has_keyword`, `granted_cost`, `flow_of`, `grant`, `grant_keyword_this_turn`, expiration, and grant tests. `keyword_instances` currently counts `row.granted` but omits `row.granted_costed`; include this reader in the representation audit instead of preserving an accidental omission.
- `engine/cost.rs`: `repeat_of_item`, `origin_cost`, remove `to_script`, and conversion helpers.
- `engine/legal.rs::flow_cost` and the presence check in `engine/activate.rs`.
- `cards/kennen_storm_of_shuriken.rs`: grant construction and `grants_flow` runtime accessor.

There is no `Ctx::costed_keywords` method in `35bdbb85` or the saved `f8322afb` statics tip inspected. If a later recovery edit adds one, update that iterator to yield a borrowed/owned dynamic cost rather than constructing static Keyword values. Re-run the whole-tree symbol search at implementation start.

Current readers preserve a specific precedence: a printed Flow/Repeat cost wins over an explicit granted cost, with `Static::GrantsRepeat` as the later Repeat fallback. This repair must not silently change that precedence, decide how multiple alternative keyword instances are chosen, or add unsupported granted Equip/Empower behavior. Those are separate rule decisions. Current `Ctx::grant` accepts dynamic Flow/Repeat and refuses Equip/Empower.

## Smallest representation repair

Add an owned serialized row, preferably next to CardState:

- `CostedKind`: Equip, Repeat, Empower, Flow with the existing codes 14, 16, 19, 20; only valid costed tags decode.
- `CostedGrant { kind, energy: u8, power: Vec<Power>, until: Expiry }`.

An alternative is a small reusable `OwnedCost { energy, power: Vec<Power> }` field within CostedGrant; either avoids any static lifetime. Keep `Power` a Copy enum and its existing code mapping.

Preserve the existing serialized row exactly as `[keyword_code, energy, [power_code...], expiry]`. The surrounding CardState array position/length and version handling should stay unchanged. A representation-only repair that preserves exact bytes and semantics does not itself require another blob, ABI or wire bump. The ongoing recovery's existing version bump remains necessary for its other schema changes; do not rewind it or invent new tags for this refactor.

`CostedGrant::from_keyword(keyword, until)` copies a printed/static keyword's power slice into a Vec. `Ctx::grant` routes the existing Flow/Repeat inputs through that constructor, leaving card scripts unchanged. Other keyword grants keep their existing tuple shape.

For consumption, factor the existing `cost::of_script` mapping into `of_parts(energy, &[Power], domains) -> engine::cost::Cost`. `of_script` delegates with its static slice, and `of_grant` delegates with the owned Vec. That retains the exact meaning of `Power::Own`, explicit Domain and Rainbow. Change `Ctx::granted_cost` and `Ctx::flow_of` to return an owned runtime `engine::cost::Cost`; adapt the handful of callers above to stop converting back through cards::Cost. Printed `Card::flow_cost`, `Card::empower_cost`, `Keyword::cost` and const Cost literals remain unchanged.

Update presence/instance queries to compare CostedKind with the requested keyword's kind code. Do not reconstruct a `Keyword::Flow(cards::Cost)` just to compare kinds. Expiration retains rows by `row.until` and dropping a game releases their owned Vecs.

### Kennen and the reverse-conversion trap

Remove `cost::to_script` entirely. It maps arbitrary `Need::AnyOf(_)` to `Power::Own` and cannot represent runtime xp, burn or floating fields. Its current only production caller uses `cost::printed`, where those extra fields are zero and AnyOf is derived from the same card's domains, so this audit does not establish a current incorrect Kennen price from that conversion. It is nevertheless the wrong general owned-cost constructor.

For the smallest byte-compatible repair, give Kennen a purpose-specific constructor for a granted **printed** cost, producing owned Power symbols directly from the target face with the same `printed` mapping:

- no domain -> Rainbow needs;
- one domain -> explicit Domain needs;
- one printed power per domain -> each Domain in order;
- otherwise -> repeated Own symbols, resolved against the relevant card domains when read.

Keep the existing `flow_for_its_cost`/labels in runtime Cost form if helpful; grant creation should call the owned printed-cost constructor and an explicit `grant_costed`/`grant_flow_this_turn` method, never intern. Test that its runtime conversion equals `cost::printed` for every branch.

If a subsequent cost lane genuinely needs to persist an arbitrary runtime Cost, use an explicit richer owned encoding for Need::AnyOf and all required fields with the appropriate version change. Do not pretend the current compact Power encoding can serialize every runtime Cost. That expansion is not necessary to remove this interner.

Once callers are migrated, delete `INTERNED_POWER`, `Power::intern`, `Cost::interned`, and their Mutex/PoisonError imports. Keep ordinary static costs and keyword code helpers. A global map, thread-local pool, request-reset interner, larger gas budget, or leaked allocation on every decode does not fix the representation problem.

## Concrete validation

1. Preserve/extend the existing busy-blob round trip with a costed Flow containing Own, explicit Chaos, and Rainbow. Compare exact bytes before/after the representation migration, including older supported version-9/10 rows. Unknown power/tag, truncated row, and invalid expiry decode must fail without retaining any grant or mutating a global.
2. Grant a costed Flow and Repeat through real request boundaries, serialize/reconstruct, and assert legality, displayed price, chosen runes/payment, and grant expiry match before reconstruction. Include duplicate kinds, distinct costs, and unchanged printed-before-granted precedence. Preserve unsupported Equip/Empower refusal.
3. Kennen: retain its existing engine tests and add a persisted grant followed by Flow payment in a later request. Cover empty, single-domain, multicolor with one power per domain, and multicolor AnyOf/Own branches. Ensure there are no added xp/burn/floating values and no lossy general converter remains.
4. Confirm costed grants participate in `has_keyword` and `keyword_instances`, expire at exactly their Expiry, disappear on the existing zone/object reset paths, and drop on new game. This must preserve other lanes' controller/name/death-history fields and array layouts.
5. Add a real hardened-plugin history test to the parity harness: construct one serialized request with a nonempty owned costed grant; run it on a fresh module and on a module after additional valid view calls, unrelated costed states, and reset/new-game traffic. Compare verdict acceptance, bytes, effects and view results. Reconstruct game state between requests. Equal outcomes at ample gas are the minimum acceptance; do not assert identical gas for all ordinary allocation histories without establishing that broader host guarantee.
6. Before repair, an optional isolated diagnostic can instantiate `WasmModule` directly, call `decide`/`view`, and record `gas_left` for the identical request after different interner histories. With a temporary reduced budget between observed costs, demonstrate accept/reject divergence if attainable. Label this a diagnostic, not an existing production-budget failure. This audit has not run it.
7. Use a focused native allocation-lifetime harness if needed: decoding and dropping distinct grant blobs should not retain allocations proportional to the historical count of unique power vectors. Avoid fragile exact allocator-call counts in ordinary engine tests. The production structural requirement is that no global mutable interner or Box::leak remains in the cost path.

Run full agni-riftbound-turns tests, scoped formatting, warnings-denied clippy, wasm compilation and hardened real-plugin parity after the refactor. Native round-trip success alone cannot validate this gas/guest-lifetime issue.

## Ownership

One serialized repair worker should own the small set listed above plus focused tests/review note. It intersects `state.rs`, `engine/ctx.rs`, and `engine/cost.rs`, so finish it at the reviewed statics checkpoint before recovering economy/costs onto that checkpoint. Future lane merge resolution must preserve the owned representation and reapply lane behavior through its runtime accessors; do not restore old static-lifetime return types to quiet compilation. This should be one bounded corrective commit, separate from new rules primitives.
