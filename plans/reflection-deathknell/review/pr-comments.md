# PR review: shared test infrastructure

Addressed abueide's three review comments on Agni #1 without changing runtime
rules or the wire/blob schema.

## Legacy encoding

Removed the three copies of `legacy_v11`, including their `item` and `row`
implementations, from Here to Help, Promising Future, and Rek'Sai. Each now
calls `fixtures::legacy::encode_v11`.

The reviewed `15` was the current chain-row width (13 v11 fields plus v12's
limited-play metadata and v16's saved ability script), not a game constant.
The shared helper reads current widths from CBOR and retains named historical
v11 fields. Appending fields does not require changing each card. The helper
also projects nested last-known details and has direct tests for that behavior
and future trailing fields. Independent historical wire fixtures are unchanged.

## Serialized actions

Removed Reflection's local `step`, `choose`, and `resolve` lifecycle helpers.
`Fixture::act_and_reload`, `choose_and_reload`, and `resolve_and_reload` now own
that reusable test behavior. A reload commits the context's table, round-trips
the blob, and rebuilds scripts; it deliberately cannot retain the departed
source in request-local caches. Resolution takes an explicit prompt-choice
callback instead of imposing Reflection's first-choice/decline policy on all
card tests. Dedicated fixture tests cover cache disposal and resumed choices.

The wiki explains the helpers, their engine-level scope, and the meaning of
the legacy row counts. Gameplay decisions still run through the existing engine.

## Verification

- Rules library: 4,577 passed; 172 pre-existing ignored tests.
- Includes all ten Reflection regressions, the three existing legacy-resume
  cases, and four new shared-fixture tests.
- Blob compatibility: 13 passed; match-state integration: 10 passed.
- `bash ci/check.sh`: passed.
- Repository `treefmt --ci`: passed.

Kai's pin is unchanged: this follow-up modifies only test code and documentation.
The existing release instructions still require repinning if the Agni PR is
squash-merged.
