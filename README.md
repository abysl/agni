# Agni

Agni is a Rust framework for building digital card games. It keeps track of
cards, players, and actions, and lets several devices follow the same game.
It does not draw a table: an application supplies the interface.

Its central rule is reproducibility. Starting with the same game state and
applying the same ordered actions must produce the same result on every device.
That makes replay and multiplayer agreement possible.

## What is included?

- Shared card-table state and an action log that can be replayed.
- Host and client sessions for multiplayer applications.
- An engine that can run as a WebAssembly module.
- Tools for preparing game plugins with explicit execution limits.
- Deck types and optional importers for supported games.

A **plugin** decides what a particular game's rules allow. **WebAssembly**
is a portable executable format used here to run the engine and plugins
behind a defined interface.

Agni is under active development; its APIs and protocols are not stable.
It is not a standalone game, an official game client, or a complete anti-cheat
system.

## Start here

If you want a graphical card table, see [Kai](https://github.com/abysl/kai).
If you want to contribute to the framework, begin with
[Contributing](CONTRIBUTING.md) and the [development guide](wiki/development.md).

With a current Rust toolchain, a first focused test is:

```sh
git clone https://github.com/abysl/agni.git
cd agni
cargo test --locked -p agni-plugin-sdk
```

The development guide explains the remaining checks and optional tools.

## Related projects and current boundaries

[Spirit Library](https://github.com/abysl/spirit-library) provides content
storage and peer communication.
[agni-rfb](https://github.com/abysl/agni-rfb) is the separately maintained
Riftbound plugin repository.

The separation is not complete: this workspace still contains Riftbound
crates and references used by existing clients and tests. Do not assume a
build of this repository contains only game-neutral code. The
[architecture guide](wiki/design/architecture.md) explains the current layout.

## Documentation and license

The [documentation index](wiki/README.md) distinguishes introductory guides,
implementation references, and historical design records.

Project code is licensed under [GNU GPL version 3](LICENSE). That license
does not grant rights to third-party games, card images, rules publications,
or dependencies. Do not include downloaded game content in release artifacts
without reviewing its provenance and redistribution terms.
