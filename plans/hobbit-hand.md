# Hobbit Hand — spirit's first store, agni's first real cards

> Goal: `dev` opens the table and seven random cards from The Hobbit (`hob`,
> 321 cards, Scryfall) sit in hand with real art — every byte having come out
> of a content-addressed spirit store, hash-verified.

Three phases, each verifiable alone. Decisions inherited: canonical encoding
is deterministic CBOR (spirit `terminology.md`); code carries no comments.

## Phase 1 — spirit gets a local blob store ✅ 2026-08-28

The honest minimal slice of spirit: content addressing, nothing else. No CIRs,
no attestations, no network — those stay design docs until something needs
them.

In `spirit-core`:

- `BlobHash`: newtype over `blake3(bytes)`, 32 bytes, hex display/parse.
  Plain blake3 for now; the iroh era will switch to iroh's tree-mode hash,
  which is exactly the kind of change a local-only store can absorb —
  recorded in `spirit/wiki/design/addressing.md`.
- `BlobStore`: a directory of blobs named by hash.
  - `put(bytes) -> BlobHash` — write temp file, rename. Atomic, and
    idempotent because identical bytes land on an identical path.
  - `get(hash) -> bytes` — **re-hashes on read** and errors on mismatch, so
    corruption and truncation fail loudly instead of flowing downstream.
  - `has(hash) -> bool`.
- Tests: roundtrip, idempotent put, missing-blob error, corruption detection
  (write garbage under a hash's path, expect `get` to refuse).

Done when: `unit-test` green in spirit; CI (`spirit.yml` + `agni.yml`) green.

## Phase 2 — ingest The Hobbit from Scryfall ✅ 2026-08-28 (193 unique cards, 17MB; variants deferred to the alt-art identity work)

A `spirit-sdk` example (`cargo run --example ingest-scryfall -- hob <store>`),
not a new crate — since superseded: the ingester moved into its own
`spirit-ingest` crate so the gossip mesh could call it for Scryfall backfill,
and the command is now `cargo run -p spirit-ingest --bin ingest-scryfall --
hob <store>`.

- Page `api.scryfall.com/cards/search?q=set:hob` (~2 pages), then each card's
  `image_uris.normal` JPG, ~100ms between requests per Scryfall's etiquette
  (~40s once).
- Per card: `put` the JPG; keep `{name, mana_cost, type_line, oracle_text}`.
- Build one manifest — set code plus `[{name, mana_cost, type_line,
  oracle_text, image: <hash>}, …]` — encode with ciborium, `put` it. The
  manifest's own hash identifies the whole ingest.
- Write that hash to `<store>/refs/hob`. Naming is spirit's collections
  problem; a ref file is the honest stopgap.
- ciborium writes struct fields in declaration order, so one writer is
  deterministic; the strict RFC 8949 §4.2 profile (sorted map keys, float
  rejection) becomes necessary only when independent writers must agree, and
  is tracked in `addressing.md`, not solved here.

Done when: re-running ingest is a no-op (same manifest hash), and
`refs/hob` resolves through `get` to a manifest listing 321 cards.

## Phase 3 — agni draws seven real cards ✅ 2026-08-28

- `agni-core::CardFace` gains `art_jpeg: Option<Vec<u8>>` — bytes, no Bevy
  dependency, core stays firmware-safe.
- `agni-table`: art present → decode to a Bevy `Image`, texture the card
  material with it instead of the tint. Needs Bevy's `jpeg` feature (one
  workspace line).
- `hand` example: open `SPIRIT_STORE` (default `~/.spirit/store`), read
  `refs/hob`, `get` + decode the manifest, pick seven distinct cards with
  agni's seeded `Rng` (seed from the clock — the example is a front end
  supplying an input; the simulation stays deterministic), `get` those seven
  images only, build the table. Startup touches 8 blobs, not 642.
- No store or ref present → fall back to today's tinted placeholder cards, so
  `dev` never breaks for a fresh checkout.

Done when: `dev` shows seven random Hobbit cards in hand, drag and drop still
works, and deleting one image blob makes the run fail loudly rather than show
a wrong card.

## Deliberately out of scope

CIR minting, attestations, iroh, agni zones/rules, canonical-profile CBOR
enforcement, card backs/face-down rendering, image caching in agni.

## Seed for later: printings are spirit identities

Alt arts and reprints are a natural fit for spirit's identity mapping, noted
2026-08-28: one CIR for the *card* (name/oracle identity), attestations mapping
it to many output blobs (each printing's art), `same_as` relations merging
independently-minted duplicates — and a player's "preferred printing" is a
resolution policy, not a different card. The Phase 2 manifest keys cards by
name precisely so this upgrade replaces the manifest rather than fighting it.
