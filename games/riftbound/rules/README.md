# Riftbound Core Rules

The official Core Rules document, downloaded from Riot's Rules Hub
(<https://playriftbound.com/en-us/rules-hub/>; the older article page
<https://playriftbound.com/en-us/news/rules-and-releases/gameplay-guide-core-rules/>
still links the 2025-12 edition). Riot does not number the editions inside the
document — each carries only a "Last Updated" date on its first page — so
the files here are named by that date, except the first one, which kept the
"v1.2" the rules page called it at the time.

- `riftbound-core-rules-v2026-07-16.pdf` — the current Core Rules, last
  updated 2026-07-16 (the Vendetta rules update, effective 2026-07-24;
  the PDF's own title is "Riftbound Core Rules RUP4 Staging"), fetched
  2026-09-10 from
  <https://cmsassets.rgpub.io/sanity/files/dsfx7636/news_live/e9ac8e3d33e0f78cef296f5945aba7bc1313b086.pdf>.
  43 MB, 120 pages, ignored by git; re-fetch it from the link above.
- `riftbound-core-rules-v2026-07-16.txt` — the same document as extracted
  text (`pdftotext -layout`, poppler 26.06 via `nix shell nixpkgs#poppler-utils`),
  committed so rule numbers can be searched and quoted. This is the edition
  the engine rules from: it adds XP (728–733), Additional Turns (734–738),
  Dependent Keywords (726–727), the Banish (427), Burn (440), Empower (441),
  Disempower (442), Skip (443) and Pay (444) actions, the combat result
  (466.3), the "alone" and "one on one" terms (740.2), and the keywords
  Repeat (820), Weaponmaster (821), Ambush (822), Hunt (823), Level (824),
  Unique (825), Backline (826), Empower (827), Empowered (828) and Flow (829).
  Numbering moved between editions (Deathknell is 808 here and 734 in v1.2;
  Combat is 459–466 here and 437–444 in v1.2; the Process of Play is 353–359
  here and 350–356 in v1.2), so quote the edition with the number.
- `riftbound-core-rules-v1.2.pdf` — Core Rules v1.2, last updated 2025-12-04
  (<https://cmsassets.rgpub.io/sanity/files/dsfx7636/news_live/572377fcaa704a05f72eb42c104079d3b3bcf740.pdf>),
  downloaded 2026-09-07. 31 MB, ignored by git.
- `riftbound-core-rules-v1.2.txt` — its extracted text, kept because the
  rulings written for M0–M8 in `wiki/design/rules-engine.md` cite v1.2
  numbers.

The patch notes for each edition are linked from the Rules Hub (Origins,
Spiritforged, Unleashed, Vendetta); the Vendetta notes are at
<https://playriftbound.com/en-us/news/announcements/core-rules-vendetta-patch-notes/>.

`pool/` holds the card pool as one Markdown file per deck (a label, a
provenance line, a `Scripted:` line, the deck as a text list and the card
lines in the grammar the engine's tests parse); the format and the wiring
are the "M9 pipeline" section of `wiki/design/rules-engine.md`. The engine's
coverage tests read the union of the six files (`POOL_FILES` in
`games/riftbound-turns/src/cards/mod.rs`), the flake's agni source filter
ships the whole directory, and kai reads the same union through
`deck/pool.rs` (the lobby, the soak and the AI seat's card reference).

Each file may end with a `## Coaching` section, one bullet per card in the
form `- **Card Name** — advice`, read by kai's `ai/cards.rs::coaching` and
appended to the AI seat's system message for the decks at the table (its
own deck by label, the opponent's by legend). It is the last section of the
file, append-only, and its bullets are prose, never card lines: no `** (`
after the name and no `): ` anywhere, so the engine's row parser and kai's
catalog parser skip them (kai's tests check both). The advice names only
cards of that file.

Rule numbers referenced by the turn machine in `games/riftbound-turns`
(v1.2 numbering, as cited in the code and the design wiki):

| Rule | What it fixes |
|---|---|
| 304–306 | the Turn Player, and when the turn passes to the next seat in turn order |
| 307–310 | the four turn states: Neutral/Showdown × Open/Closed |
| 311–313 | Priority and Focus; a player who gains Focus gains Priority; passing Priority keeps Focus |
| 314–317 | phases: Awaken, Beginning (Hold scoring), Channel (2 runes), Draw (1 card), Action, End of Turn |
| 337–345 | Showdowns: opened when a battlefield is contested, the contesting player gains Focus, players alternate play-or-pass, closed when every player passes in sequence |
| 437–444 | Combat: attacker/defender, the Showdown step, combat damage, resolution and control |
| 445–449 | Scoring by Conquer and Hold, one point per battlefield per turn, victory at the mode's score |
