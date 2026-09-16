# agni + kai — Port to spirit's primitives

> The consumer half of
> [`../../spirit/plans/records-collections-transforms.md`](../../spirit/plans/records-collections-transforms.md).
> That plan builds records, collections, attestations and transforms in spirit;
> this one moves agni and kai onto them and deletes the substitutes they grew
> while those primitives were still design docs.

## What gets deleted, and what replaces it

| Today | Lines | Replaced by |
|---|---|---|
| `importers/scryfall.rs` `Manifest`, `mtg/ingest.rs` `Manifest`, `riftbound/ingest.rs` `RiftboundManifest` | 3 schemas | card + printing CIRs, rules records, one catalog collection |
| hand-written `std::fs::write(store.root().join("refs")/…)` in all three ingesters | 3 sites | `collection.append(...)` — spirit has no card-ref API today, which is *why* agni reaches past it |
| `art.rs` `Journal` + `<store>/mtg-images` + `<store>/riftbound-images` | ~90 | content attestations `(ci:<printing>, td:<image-fetch>) → blob:<jpeg>`, folded by spirit-index |
| `image_url` field on every manifest card | 3 fields | the `http-get` TDR that fetched it |
| `sim/pins.rs` hex + `blob:` parsing, `hash_hex`, `blob_ref`; `engine-host` re-exports; kai `short_hex` | ~60 | `spirit_core::{BlobRef, BlobHash}` typed addresses |
| kai `<store>/modules-seeded` + `seed_one` precedence rules | ~80 | a module version collection; "mesh install beats bundle" becomes trust ordering between signers |
| kai `pinned_bytes_from` + `request_fetch` + `PinState` | ~50 | `spirit_sdk::resolve(ci \| blob, policy)` with mesh fetch behind it |
| `engine-host` `ModuleSource` / `StoreSource` | ~40 | resolution against the module collection |
| `importers/mtg/catalog.rs` + `riftbound/catalog.rs` name indexes | partial | spirit-index's `external id → ci`; the *naming* rules stay in agni |

What explicitly stays in agni: every parser (deck codes, text lists, card codes,
Scryfall/Riftcodex JSON shapes), naming normalisation, game data shapes, the
wasm host, and the harden pass. Those are game knowledge. The port moves
*storage, identity, replication and provenance* down into spirit and leaves
*interpretation* up here.

> **A4 landed 2026-09-04** with the spirit side (S0–S4) and both leaks. A1, A2,
> A3, A5 and A6 are still ahead; the ship order below is unchanged for them.

## A1 — typed addresses

- [ ] Depend on S0. Replace `pin_hash`/`blob_ref`/`hash_hex` with
      `spirit_core::BlobRef`; `sim/pins.rs` shrinks to the genesis-log
      accessors and the `PinError` policy, which are agni's business.
- [ ] kai's `short_hex` becomes a display helper on the typed address.

No behaviour change; genesis pins keep their `blob:<hex>` spelling, so no wire
or golden-fixture churn. **1 day.**

## A2 — cards become records

Implements identity.md's migration section.

- [ ] Ingest v2 for both games: query with `unique=prints`, capture
      `oracle_id`/`id`/`set`/`collector_number` (Riftcodex equivalents:
      `riftbound_id`, `set_id`, `collector_number`), mint card CIR, printing
      CIR, rules record, and the two content attestations per printing.
- [ ] Write `refs/cards/<game>` pointing at the catalog; leave `refs/hob`,
      `refs/mtg`, `refs/riftbound` untouched.
- [ ] kai reads the catalog when present and falls back to the manifest, so a
      half-migrated mesh keeps dealing.
- [ ] `art_from_store` / `art_by_names` / `load_catalog` resolve
      identity → printing → art through spirit-index instead of scanning a
      manifest's card list.

Done when: two fresh nodes ingesting `hob` converge on the same catalog hash and
kai deals a hand where every face resolved through a card CIR. **3 days.**

## A3 — importers become transformation documents

The shape the whole port is for: an importer stops being a binary that fetches,
parses and writes, and becomes a document that names a source URL and a pinned
transform.

Each importer splits along the purity line it already almost has:

- **Impure, generic, spirit's:** every HTTP GET, with its rate limit
  (Scryfall 100 ms, Riftcodex 1 req/s) as a TDR field. No agni code at all —
  agni only supplies the `Fetcher` capability (`ureq` on desktop, the gateway's
  on the browser path).
- **Pure, game-specific, agni's:** the JSON → records transform, compiled to
  `wasm32-unknown-unknown` and hardened by `agni-harden` exactly like a game
  plugin. `scryfall.rs`'s field extraction, `riftbound/json.rs`,
  `card_code.rs`, `deck_code.rs` and `naming.rs` are already pure and move over
  nearly unchanged.

```
td:http-get{url: scryfall search, snapshot, rate_ms: 100}     → blob:<page json>
td:wasm-transform{module: blob:<scryfall-cards.wasm>,
                  inputs: {pages: [blob:…]}}                   → NeedInputs([image urls])
td:http-get{each image url}                                    → blob:<jpeg>
td:wasm-transform{… inputs pinned with the images}             → blob:<catalog>
(ci:<mtg card set>, td:<locked>) → blob:<catalog>              # content attestation
```

- [ ] New crates `importers/transforms/scryfall-cards` and
      `.../riftcodex-cards`: zero-import wasm guests over the existing parsers,
      answering the fetch-plan loop (`Output` | `NeedInputs`).
- [ ] `ingest-scryfall` / `ingest-riftbound` become thin drivers that author the
      unlocked TDR, hand spirit a `Fetcher` and the wasm `TransformRunner`
      (`agni-engine-host`, already gas-metered), and print the locked
      `td:<hash>`.
- [ ] Golden test: a locked TDR plus fixture response blobs reproduces a
      byte-identical catalog offline, in CI, with no network.
- [ ] Delete the journals — resumability now comes from the store already
      holding the pinned input blobs.
- [ ] `/gateway/resolve-deck` keeps its shape but resolves through the same
      transform, so the browser path and the desktop path stop being two
      implementations.

Done when: `hob` re-ingests from a locked TDR offline and produces the catalog
hash A2 produced online. **4 days.**

## A4 — plugins become version collections ✅ 2026-09-04

- [x] `agni-harden --store --name` publishes: a `wasm-module` CIR
      (`name`, `role`, `version`, `abi_version`), a content attestation
      `(ci:<module@version>, td:<harden config>) → blob:<wasm>`, and an `add` op
      on `col:modules/<name>`. The harden config in the TDR is the
      reproducibility claim: same input wasm plus same config yields the same
      output bytes.
- [x] kai resolves a module by: genesis pin if the log has one (unchanged —
      a pin is a `blob:` and resolution ends at bytes, so **no session-protocol
      change**), else the newest collection item whose attestation is trusted
      and whose `abi_version` matches the host.
- [x] `seed_bundled` appends the bundled build as an item signed by the local
      key instead of overwriting a ref; `seed_one`'s "stays at … (installed from
      the mesh)" heuristic and `<store>/modules-seeded` both delete — a mesh
      install is simply an item from another signer, and precedence is trust
      order.
- [x] The settings module list shows versions, signer and held-ness from the
      collection; `ModuleRow.source` stops being a hard-coded `"store"`.

Done: two versions of `riftbound` coexist in the store, kai runs the newest
trusted one, an untrusted signer's version is listed but never loaded, and a
genesis pin still joins a table under the exact bytes it names — no
session-protocol change. Covered by
`kai::engine::modules::platform::tests` and `spirit_schema::modules::tests`.

The browser peer has no store and no trust registry, so the resolving moved to
the node that has both: `/gateway/modules` serves the folded versions with
signer and trust, and kai's wasm path renders those rows instead of decoding
manifests itself.

## A5 — trust, and closing the ref hole

Today `Mesh::wanted_names` accepts **every ref name every peer advertises**, and
`converge` pulls from whichever peer claims the highest `held`, overwriting
`refs/<name>`. Complete module refs are safe (equal `held` loses), but a ref
name you do not yet hold is accepted from anyone, and a card set is replaced by
whoever claims more art. The only thing standing between that and executing a
stranger's wasm is kai's genesis pin check.

- [ ] Depend on S3's follow-set and S4b's trust registry: pull only collections
      you follow, from signers you trust at cache level or above.
- [ ] kai settings: peers list gains a trust level, defaulting to contact for a
      scanned QR and mesh for your own devices.
- [ ] `wiki/design/multiplayer.md` and kai's `peers.md` updated in the same
      change — "scanning a QR grants full mesh visibility" stops being true.

Done when: a peer advertising `modules/riftbound` you have never seen is
ignored until you trust its signer. **2 days.**

## A6 — identity on the wire

- [ ] `CardFace` carries `(card ci, printing ci)`; JPEG bytes stop travelling
      inside hands (multiplayer.md's known limitation — hundreds of KB per
      deal). Each peer resolves art from its own replicated store.
- [ ] The rules record's blob hash joins the replay compatibility key, so errata
      cannot silently rewrite a replay.
- [ ] `agni-plugins`' `ScriptRegistry` keys on card CIR, as its AGENTS entry
      already anticipates.

Wire change with session-protocol fallout and golden-fixture regeneration —
the largest single step. **4 days.**

## Order

A1 first (cheap, deletes code, unblocks nothing else). A2 and A4 are
independent; A4 is the smaller and gives the more visible win. A3 needs A2's
records and spirit's S5. A5 needs S3+S4b. A6 last — it is the only step that
breaks the wire, and it wants identities to be uncontroversial first.

Ship order that keeps the app working throughout:
**A1 → A4 → A2 → A3 → A5 → A6.**
