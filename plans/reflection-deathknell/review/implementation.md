# Reflection abilities and Deathknell

## Reproduction

A LeBlanc-created copy of Honest Broker and the original die in a single
combat. Serializing between every choice and pass reproduced one Gold instead
of two. Temporary killing the copy reproduced zero Gold instead of one.
The multi-stage Undercover Agent Deathknell and a newly spawned copy-of-copy
also failed before the fix. Movement and bounce control cases already passed.

## Fix

- Save printed trigger/activation script identity centrally in `Ctx::enqueue`.
- Use the saved identity during targeting, ordering, payment, and resolution.
- Read departed tokens' last faces while collecting triggers in the same action.
- Refresh static-source candidates immediately when existing copies transform.
- Advance plugin to 0.9.1 and blob to 16; decode prior supported versions.
- Keep historical input fixtures and update their independent canonical output
  writers for the current schema, including the pre-existing v15 seat field.

Official Core Rules dated July 16, 2026, downloaded from the publisher's rules
hub PDF, support the behavior (477.1.b.1, 808.1.c–d, 816.1.b). Details and source
link are in `wiki/design/rules-engine.md`.

This is the Agni copy used by Kai, not the extracted agni-rfb repository.
Kai needs a companion lockfile update and rebuilt/hardened plugin. Existing
matches remain pinned to their module hash.

## Verification

- Ten Reflection regression tests pass, with state round-tripped per action.
- Rules library: 4,573 passed; 172 pre-existing ignored tests not executed.
- Blob compatibility: 13 passed, including independent v15/v16 chain rows.
- Match-state integration: 10 passed.
- Plugin library: 6 passed; projection integration: 4 passed.
- `bash ci/check.sh`: passed.
- `nix-shell ci/format.nix --run 'treefmt --ci'`: passed.
- Portable release plugin compiled and passed `agni-harden` with its default
  gas, stack, and memory limits. The shell supplied `lld`, missing on bare PATH.

No game artwork or generated plugin is committed. Framework wire encoding is
unchanged. Old states cannot reconstruct a vanished token's script identity
if they were saved before this fix.

## Additional integration check

`native_and_hardened_riftbound_replays_match_at_every_entry` is blocked by the
existing Temporal Portal / Rally the Troops fixture's assertion that Repeat
payment has finished immediately after answering yes (line 1156). It fails
identically with the unchanged 8765cdf test and a separately rebuilt baseline
0.9.0 plugin. This unrelated fixture was not changed to make the check pass.
The default gas-bench invocation contains three ignored benchmarks and is not
counted as a passing validation.
