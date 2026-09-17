# TCG Arena Riftbound import

TCG Arena's Riftbound deckbuilder has two deck formats that the importer accepts.

The standard export is a counted text list. It emits the deckbuilder's categories in order, followed by a `Sideboard:` section. The importer accepts that text through the existing Riftbound text vocabulary. Untagged cards remain uncommitted until catalog resolution, where card kinds place runes and battlefields and the existing champion inference handles the chosen champion.

The starter-deck export is JSON. Its `deckList.categoriesOrder` names category arrays containing `{ "count", "id" }` entries. The importer maps `Legend`, `Chosen_Champion`, `Battlefields`, `Runes`, and `Sideboard`; all other categories are main deck entries. IDs are passed to the local catalog resolver and no card database is bundled.

TCG Arena's public deck import link is `/import?game=Riftbound&name=...&deck=...` on `https://tcg-arena.fr`. The deck parameter is `encodeURIComponent(btoa(decklist))`. The importer decodes this embedded text locally. It does not follow `/load/...`, which is TCG Arena's game-file sharing route, and it makes no request for a deck import URL.

Native gateway use recognizes only the exact HTTPS host `tcg-arena.fr` and keeps the existing bounded transport policy for other supported site links. The gateway deployment must ship the `riftbound-gateway` feature with the updated importer and have a current Riftbound catalog in its store. Browser clients can parse pasted text, JSON, or TCG Arena import links locally; they do not need a CORS request to TCG Arena.

Kai should recognize `tcg-arena.fr/import` as a Riftbound deck link and show an import hint such as “TCG Arena deck link — paste or import”. It should label `/load/...` as a game-file link rather than a deck link. Those UI changes belong in Kai and are intentionally outside this change.

TCG Arena source references:

- [TCG Arena deck import implementation](https://tcg-arena.fr/)
- [TCGA game-file documentation](https://documentation.tcg-arena.fr/content-files)
