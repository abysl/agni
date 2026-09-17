# TCG Arena Riftbound import

TCG Arena's Riftbound deckbuilder has two deck formats that the importer accepts.

The standard export is a counted text list. It emits the deckbuilder's categories in order, followed by a `Sideboard:` section. The importer accepts that text through the existing Riftbound text vocabulary. Untagged cards remain uncommitted until catalog resolution, where card kinds place runes and battlefields and the existing champion inference handles the chosen champion.

The starter-deck export is JSON. Its `deckList.categoriesOrder` names category arrays containing `{ "count", "id" }` entries. The importer maps `Legend`, `Chosen_Champion`, `Battlefields`, `Runes`, and `Sideboard`; all other categories are main deck entries. IDs are passed to the local catalog resolver and no card database is bundled. The JSON card ID shape was verified against the public TCG Arena Riftbound asset [`Riftbound-CardList.json`](https://raw.githubusercontent.com/Russeus/RB-TCG-Arena/main/Riftbound-CardList.json), whose first record is keyed by `OGN-001` and repeats that value in its `id` field. Synthetic IDs are used in tests.

TCG Arena's public deck import link is `/import?game=Riftbound&name=...&deck=...` on `https://tcg-arena.fr`. The generator passes `encodeURIComponent(btoa(decklist))` into `URLSearchParams`, which applies another encoding layer: base64 padding can appear as `%253D`. The importer decodes query form values, then the inner deck parameter, before base64 decoding. It also accepts the single-encoded links already supported. It decodes this embedded text locally. It does not follow `/load/...`, which is TCG Arena's game-file sharing route, and it makes no request for a deck import URL.

## Public source verification, 2026-09-16

The HTML served by TCG Arena references the exact JavaScript asset
[`index-VzYe3nvQ.js`](https://tcg-arena.fr/assets/index-VzYe3nvQ.js).
The following symbol names identify the inspected functions in that build;
they may change on a future deployment. No source bundle or card data is
stored in this repository.

- `Y$e`, nested `E`: standard text export emits counted names in category
  order, then a separate sideboard section. It does not emit category headers
  for the chosen champion, so standard text cannot always preserve that choice
  unambiguously; the existing catalog-based inference remains the fallback.
- `Y$e`, nested `k`: starter JSON carries title, ID, counts, timestamps,
  game, format and `deckList`. It copies `categoriesOrder` and each listed
  category's count/ID entries. It assigns the root game from the input deck
  without validating its presence. A missing/undefined input game is therefore
  omitted by JSON serialization. Agni accepts absent game metadata within its
  Riftbound import context, but rejects a present non-Riftbound or non-string
  value. This is a compatibility allowance; the current public
  [Riftbound starter asset](https://russeus.github.io/RB-TCG-Arena/Riftbound-Decks.json)
  was checked and its first deck explicitly declares Riftbound.
- That JSON exporter copies only categories named in `categoriesOrder`; it
  does not separately append `Sideboard`. Agni preserves a sideboard when the
  payload contains it, but cannot recover a sideboard omitted by the exporter.
- `F0e`: generates the import URL with both encoding layers described above.
  `L0e`: reads query parameters, requires game and deck, applies an additional
  URI decode to the deck parameter, then base64 decodes it. Unlike starter
  JSON, a URL still requires its Riftbound game parameter.

Synthetic tests exercise absent-game JSON through `parse_any` and text-query
resolution, and double-encoded links through both URL and text queries.
The standard-text chosen-champion limitation and upstream JSON sideboard
omission are properties of the exported payload, not catalog downloads.

Native gateway use recognizes only the exact HTTPS host `tcg-arena.fr` and keeps the existing bounded transport policy for other supported site links. The gateway deployment must ship the `riftbound-gateway` feature with the updated importer and have a current Riftbound catalog in its store. Browser clients can parse pasted text, JSON, or TCG Arena import links locally; they do not need a CORS request to TCG Arena.

Kai should recognize `tcg-arena.fr/import` as a Riftbound deck link and show an import hint such as “TCG Arena deck link — paste or import”. It should label `/load/...` as a game-file link rather than a deck link. Those UI changes belong in Kai and are intentionally outside this change.

TCG Arena source references:

- [Verified TCGA import/export JavaScript asset](https://tcg-arena.fr/assets/index-VzYe3nvQ.js)
- [TCGA game-file documentation](https://documentation.tcg-arena.fr/content-files)
