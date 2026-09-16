# Contributing to Agni

Agni is the neutral engine. Changes here must keep the simulation reproducible
and must not add game-specific rules or card content to neutral crates.

## Development

Enter the project environment and run the same checks used by CI:

```text
direnv allow
fmt-check
unit-test
clippy
build
```

The simulation boundary is `core/` and `sim/`. It cannot use floating point,
wall-clock time, environment reads, unseeded randomness, or unordered map
iteration in decision-making code.

Networking belongs in `net/`; plugin interfaces belong in `plugins/`; game
shape crates belong in `games/`. Riftbound implementation code is maintained
in [agni-rfb](https://github.com/abysl/agni-rfb).

Read [wiki/design/architecture.md](wiki/design/architecture.md) before
changing simulation, wire, or session behavior. Add or update deterministic
tests with every behavior change.

## Pull requests

Use a focused branch and explain the compatibility impact in the pull request.
Run formatting, tests, clippy, and the relevant build locally before opening it.
