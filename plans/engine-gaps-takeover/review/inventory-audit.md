# Immutable test inventory audit

The audit CLI reads the immutable `inventory.json`, indexes the Rust card test
sources under a caller-supplied agni root, and emits JSON for all 100 clusters
and 381 original entries. It records the requested and actual source lines,
whether the function is enabled or ignored, the ignore reason, cluster counts,
and explicit replacement resolution. It never edits inventory or Rust sources.

An enabled entry means that the named function has a complete, contiguous and
unambiguous attribute block containing `#[test]` and no `#[ignore]` attribute.
It does not mean that the test passes or that its assertions verify the
intended rules semantics. Non-test functions, duplicate names, and missing,
blank-separated or multiline attribute blocks are `unknown`, never enabled.
Only `test`, `ignore`, and `should_panic` metadata is accepted; conditional or
other attributes such as `cfg_attr` are `unknown`.
The parser classifies an ignore beginning with `engine gap` as
`ignored_engine_gap`, a `rules question` ignore with an explicit mapping as
`ignored_rules_question_with_replacement`, and any other ignore wording as
`unknown`. Missing functions are `absent_unmapped` unless every explicit
replacement is a valid enabled test. `--check` always fails for an
`absent_unmapped` entry; lane labels only add checks for expected replacements
when the original function is still present. Replacement mappings declare the
lane where they are expected, so a source root from another lane does not
create a false missing report.

The current tree audit reported:

| status | entries |
| --- | ---: |
| enabled | 43 |
| ignored_engine_gap | 326 |
| ignored_rules_question_with_replacement | 1 |
| unknown ignore reason | 11 |
| absent_unmapped | 0 |

All 381 entries were found and no unexplained missing entry was reported. The
four explicit mappings are Towering Pairofant to two statics tests, Rift
Herald to two play tests, Herald of Scales to one economy test, and Fallen
Feline to one integration replacement. The mapping file records the
contradictory Herald fixture and the rules-question reason for Fallen Feline.

Validation uses Node only:

```text
AGNI_ROOT=/path/to/agni
SAVED_LANES=/path/to/saved/lanes

node plans/engine-gaps-takeover/inventory-audit.js --agni-root "$AGNI_ROOT" --label current --check
node plans/engine-gaps-takeover/inventory-audit.js --agni-root "$SAVED_LANES/statics/orgs/andrea/projects/agni/agni" --label statics --check
node plans/engine-gaps-takeover/inventory-audit.js --agni-root "$SAVED_LANES/play/orgs/andrea/projects/agni/agni" --label play --check
node plans/engine-gaps-takeover/inventory-audit.js --agni-root "$SAVED_LANES/economy/orgs/andrea/projects/agni/agni" --label economy --check
node plans/engine-gaps-takeover/inventory-audit.js --agni-root "$SAVED_LANES/integration/orgs/andrea/projects/agni/agni" --label integration --check
```

The saved statics, play, economy, and integration roots all completed with zero
unexplained missing entries. They resolved the expected replacement sets:
Towering Pairofant 2/2, Rift Herald 2/2, Herald of Scales 1/1, and Fallen
Feline 1/1. The emitted JSON remains the accountable artifact; the review
counts are a compact reading of those outputs.

| root label | enabled | ignored engine gap | rules question with replacement | replaced engine gap | unknown | absent/unmapped |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| current | 43 | 326 | 1 | 0 | 11 | 0 |
| statics | 43 | 324 | 0 | 1 | 13 | 0 |
| play | 20 | 347 | 0 | 1 | 13 | 0 |
| economy | 38 | 328 | 1 | 0 | 14 | 0 |
| integration | 23 | 344 | 1 | 0 | 13 | 0 |

The audit is syntactic. It does not run tests, inspect assertion quality,
prove that an enabled test passes, compare behavior across branches, or decide
whether an ignored rule question has the correct rules interpretation. Eleven
current-tree ignores remain `unknown` because their wording does not begin
with either recognized prefix; they are visible per entry in the JSON for
manual disposition. Temporary fixture checks also confirmed that non-test
functions, duplicate names, blank-separated attributes, and multiline
attributes remain `unknown`, while an absent mapped original with no valid
replacement makes `--check` fail even without a lane label.

`--check` remains the partial lane check: it always rejects an absent or
unmapped original, and a label additionally enables that label's expected
replacement checks. `--complete` is the strict accountable report. It fails
for every unresolved engine-gap ignore, unknown or conditional source shape,
absent/unmapped original, or incomplete explicit replacement mapping regardless
of label. It passes an original only when its source test is enabled, or when
its reviewed mapping has at least one replacement and every replacement is an
enabled test; a mapped historical rules-question original uses the same
complete replacement rule. The JSON contains `complete_unresolved`, an
`unresolved_reasons` count, and an `unresolved` array with each original test
id, status, and reasons. This is still inventory evidence: enabled means the
source shape is enabled, not that the test passes or proves the rule.

The current integration is expected to keep the partial statics check green
while strict completion fails on the remaining engine-gap and unknown entries:

```text
node plans/engine-gaps-takeover/inventory-audit.js \
  --agni-root "$AGNI_ROOT" --label statics --check
node plans/engine-gaps-takeover/inventory-audit.js \
  --agni-root "$AGNI_ROOT" --complete
```

Temporary Node-only fixtures also cover the strict cases: all enabled tests
pass, a complete one-to-many mapping passes, an unknown/conditional attribute
fails, an ignored engine-gap original fails without a complete mapping, and an
incomplete mapping fails without a label.
