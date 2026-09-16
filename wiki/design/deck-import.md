# Deck import — resolution, retries and the unresolved report

> Scope: `importers/` — the Riftbound deck-link path (`riftbound::query`,
> `riftbound::riftcodex`, `riftbound::catalog`) and the game-neutral resolver
> underneath it (`deck::resolve`, `deck::catalog`, `transport`). Written
> alongside the fix for a Piltover Archive import that lost its legend on the
> second fetch of the same URL.

## The path a deck link takes

1. `query::resolve_query` classifies the URL against the deck-site allowlist,
   fetches the page through `Fetch` (`UreqFetch` in production, a fixture in
   tests) and hands the body to `link::extract`.
2. Piltover Archive pages carry a deck code in their `/deckbuilder?code=` link;
   the code decodes to `Identifier::Code(CardCode)` entries — set, collector
   number and printing variant. Riftdecks pages carry `Identifier::Id` values
   from image stems. Pasted text lists carry `Identifier::Name`, except that a
   line which parses as a card code or a riftbound id (`UNL-230`,
   `unl-189-219`) becomes an `Identifier::Code` so it gets the exact lookup.
3. `deck::resolve` asks a `CardLookup` for each entry. kai and the
   `resolve-riftbound` bin use the ingested `StaticCatalog` from the spirit
   store when `refs/riftbound` exists and the live `Riftcodex` client
   otherwise; the gateway resolver only ever uses the store catalog.

Codes are the most exact identifier Piltover Archive offers, so legends and
champions from a deck link never go through fuzzy name search. Names only
arise from pasted text.

## What went wrong

The same URL resolved 31 faces once and 30 the next time, with the legend
`Lillia - Bashful Bloom` unresolved. Two different backends answered the two
fetches: kai starts its background set ingest after the first import, so the
first fetch resolved live against Riftcodex and the second against the store
catalog. The two disagreed on one code.

The deck carried the legend as `UNL-230`. The set holds two printings with
that number: `unl-230-219` (the overnumbered base) and `unl-230*-219` (the
signature). `deck::catalog::unique_prefix` looked codes up by walking the
`BTreeMap` range from the fragment and stopping at the first key that did
not start with `unl-230-`. Because `*` (0x2a) sorts before `-` (0x2d), the
walk met the signature first, stopped, and never reached the base printing —
a deterministic miss on the store side, while the live client took the first
prefix hit and found it. Riftcodex also had no retry, so a single timeout or
`503` on any card turned the whole import into a `502` that named nothing.

## What the fix does

**Prefix lookups scan every key sharing the fragment.** `unique_prefix` now
walks while keys start with the bare fragment and filters for the separator
inside that run, so a variant that sorts before the separator no longer hides
the base printing. `Riftcodex::pick_by_fragment` applies the same rule: an
exact id wins, then a unique prefix match, and two distinct ids under one
prefix are refused rather than picked by API order. The two backends now
answer identically for a code.

**Name ties break by id.** When fuzzy search returns several exact-name
matches (`Lillia - Protector of Dreams` exists in both `UNL` and `OPP`),
`pick_by_name` takes the lowest riftbound id, which is also the printing the
store catalog's `NameIndex` keeps (the manifest is sorted by id). Same answer
regardless of the order the API returns.

**Transient failures retry.** `transport::Retry` runs a request up to three
times with a doubling backoff from 250 ms (250, 500) when the failure is
transient: a transport error (timeout, reset, DNS), `408`, `425`, `429` or
any `5xx`. A `404` is a miss, not a failure, and other `4xx` answers are not
retried. Both the Riftcodex client and the deck-page fetch (`UreqFetch`) sit
on the same `Transport`/`Retry` pair, with a 20 s timeout and a 4 MiB body
cap. `Riftcodex::with_transport` takes any `Transport`, and `retrying` /
`throttled` set the policy, which is how the tests script responses without
the network.

**Failures are reported per card, not as a dead import.** `deck::resolve` no
longer aborts on a lookup error. The card lands in `unresolved` with the
identifier and the full reason — the URL, the last transport error and the
attempt count, e.g.
`card lookup failed: https://api.riftcodex.com/cards/riftbound/unl-230: the fetch failed: timeout (after 3 attempts)`.
Cards the catalog simply does not know keep the reason
`no card in the catalog matches` (`deck::resolve::NO_MATCH`;
`Unresolved::lookup_failed` tells the two apart). After three lookup failures
in a row the resolver stops asking and marks the remaining entries
`not looked up: 3 lookups in a row failed, the last with: …`, so a dead
network costs at most three rounds of retries rather than one per card; a
success resets the count. `resolve` keeps its `Result` signature for callers,
but only the lookup can fail now, and it no longer does so through that
`Result`.

The JSON reply is unchanged in shape: `unresolved` is still
`[{identifier, reason}]`, which kai already prints as
`unresolved: {identifier} — {reason}`. The one status change: a deck in which
nothing resolved *because lookups failed* answers `502` with the first
failure's identifier and reason, instead of the `422` "ingest the catalog"
hint that only fits a deck the catalog genuinely lacks.

## Determinism

Same input plus the same responses now yields the same deck: no pick depends
on API ordering, on `BTreeMap` adjacency, or on which backend answered.
`riftbound::query::tests::the_same_input_resolves_the_same_deck_every_time`
and the `Riftcodex` scripted-transport tests pin this; the store-side case is
`deck::catalog::tests::a_variant_sorting_before_the_separator_does_not_hide_the_base_printing`
and `riftbound::catalog::tests::a_legend_resolves_beside_its_signature_printing`.
