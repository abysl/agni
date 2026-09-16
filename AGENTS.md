# Agni implementation rules

Audience: coding assistants and contributors who have read
[Contributing](CONTRIBUTING.md) and [architecture](wiki/design/architecture.md).

## Determinism

The same initial state and ordered inputs must produce the same result.
In core/simulation decisions, do not use unordered map iteration, floats,
wall-clock or environment reads, unseeded randomness, or allocation order.

Core and simulation must never depend on a renderer. Networking and content
fetching remain outside the fold. Clients submit requests and render views;
plugins decide game-specific behavior.

## Current repository boundaries

This workspace includes a real rules implementation, not just placeholders.
The Riftbound crates are also present in the separate agni-rfb repository.
Existing clients and tests still consume the copies here; do not assume
the extraction is complete.

Spirit dependencies are public Git dependencies pinned by Cargo.lock.
Spirit must never depend on Agni. Register application protocols through the
node's caller hooks instead of putting game knowledge in Spirit.

## Compatibility and hidden information

Read the relevant log, session, ABI, and plugin tests before changing those
contracts. Golden vectors are compatibility evidence. Update them with
`AGNI_GOLDEN_WRITE=1` only for an intentional versioned format change.

A refused request must neither mutate state nor reveal a private face.
Module hashes refer to hardened bytes and are pinned for a session.
Review execution-limit changes as compatibility changes, not just tuning.

## Work and verification

Code carries no comment lines. Use small functions and precise names; put
API/design explanations in the wiki and rationale in commit messages.

Run `bash ci/check.sh` and the repository's treefmt check. Full networking
and WebAssembly integration tests are additional checks described in
[development](wiki/development.md).

Do not commit downloaded card artwork, credentials, private stores,
internal deployment configuration, or generated modules.
Rules-pool Markdown is parsed as data; do not reflow it as prose.
