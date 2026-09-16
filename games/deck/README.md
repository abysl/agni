# agni-deck

The deck shapes every game crate shares: `CardName`, the generic
`DeckEntry<C>` (a card plus its copy count) and the helpers that operate on a
zone of entries — `push` merges a card into a zone by equality, `total` sums
the copies, `expand` repeats each card by its count, `flatten` turns a zone
back into names.

`agni-riftbound` and `agni-mtg` alias `DeckEntry` to their own `ResolvedCard`
and keep their genuinely different pieces (deck anatomy, zone table, deal
plan) to themselves. The importers build on the same helpers so a third game
adds a vocabulary, not a copy of the deck machinery.
