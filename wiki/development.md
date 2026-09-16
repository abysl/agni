# Developing Agni

Audience: programmers who know basic Git and terminal use but have not worked
on this project. Commands below start at the repository root after checkout.

## Requirements and checkout

Install Git and Rust 1.98.1, including Cargo. On Linux, a C compiler and ordinary
native build tools are needed by some dependencies. Clone the source:

```sh
git clone https://github.com/abysl/agni.git
cd agni
cargo test --locked -p agni-plugin-sdk --lib
```

If using rustup, install/select 1.98.1. Cargo downloads dependencies, including
the public Spirit Library Git revision recorded in Cargo.lock. You do not
need another repository checked out just to run the focused tests.

The optional `devenv shell` supplies a development toolchain and helper commands.

## Checks

```sh
bash ci/check.sh
```

This executes library tests for the core, simulation, plugin SDK, and shared
deck model, plus the session, undo, and wire-golden integration tests, then
compile-checks every workspace target. It deliberately does not execute all
networking and runtime-module tests.

For a broader local run, install the WebAssembly target first:

```sh
rustup target add wasm32-unknown-unknown
cargo test --locked --workspace
```

Some integration tests build and execute real WebAssembly modules. The explicit
network smoke test is ignored by default and needs a suitable network:
`cargo test --locked -p agni-net --test net_smoke -- --ignored`.
Never interpret an ignored test as evidence of network compatibility.

Optional importer binaries have feature gates. For their complete build check:

```sh
cargo check --locked --workspace --all-targets --features agni-importers/scryfall,agni-importers/riftbound-gateway,agni-importers/mtg-native
```

## Build a portable engine

```sh
cargo build --locked -p agni-engine-wasm --target wasm32-unknown-unknown --release
cargo run --locked --release -p agni-harden -- target/wasm32-unknown-unknown/release/agni_engine_wasm.wasm target/engine.wasm --engine
```

The second command validates the raw module and adds execution limits.
Use the hardened output when exercising the host. See the
[plugin guide](design/plugins.md) before changing the interface or limits.

## Formatting

treefmt runs the configured language formatters for this repository. With Nix
installed, the pinned environment supplies treefmt, rustfmt, taplo, and alejandra:

```sh
nix-shell ci/format.nix --run treefmt
nix-shell ci/format.nix --run 'treefmt --ci'
```

The first command applies formatting; the second fails if formatting changes
are needed. You can also install those tools yourself and run `treefmt`
directly. Rust, TOML, and Nix are covered; prose is reviewed for clarity.

## What GitHub checks

The `PR checks` workflow runs on pull requests targeting `main`, pushes to
`main`, and merge-queue events. Its `treefmt` and `fast-check` jobs feed the
single `pr-gate` result.

Rust dependency/build caches are reused; only pushes to `main` save shared
caches. A cold run still needs to fetch and compile dependencies.
The workflow uses read-only repository permissions and does not publish
packages or deploy applications.

Repository administrators must require `pr-gate` in the protection rule or
ruleset for `main` to prevent merging a failed check. A workflow file alone
does not enforce that rule.

## Common failures

If `--locked` refuses to proceed, a manifest and Cargo.lock disagree. Update
the lockfile intentionally, inspect the dependency changes, and commit it.
Do not remove `--locked` from CI to hide the mismatch.

A missing formatter means its executable is not on PATH; use the pinned Nix
environment. A native linker/pkg-config error usually means a required system
library or build tool is missing, not that a Rust test failed.

Run commands from the repository root unless a guide explicitly says otherwise.
