# Contributing to Agni

Audience: programmers familiar with Git and basic programming, new to Agni.
You do not need to understand every game plugin to work on the framework.

## First steps

Read the [architecture guide](wiki/design/architecture.md), then follow the
[development guide](wiki/development.md). Choose one component and run its tests
before making a change.

Open a GitHub issue before changing a public protocol or the simulation model.
For a small fix, create a branch, reproduce the bug with a test, make the fix,
and open a pull request containing the problem, approach, and test results.

## The important constraint

The same starting state and ordered actions must produce the same result.
Simulation decisions must not depend on wall-clock time, environment variables,
unseeded randomness, floating-point arithmetic, or unordered map iteration.

Rendering, network I/O, and content downloads belong outside the deterministic
state transition. The core and simulation crates must not acquire a renderer
dependency.

## Testing a change

Use the focused commands in the development guide while iterating. Before
submitting, run formatting and the fast checks used by GitHub, then any
additional tests needed for your change.

A serialization change needs compatibility tests. A hidden-information change
needs tests for what each seat can observe. Update golden fixtures only when
the format change is intentional and its versioning has been reviewed; do not
regenerate them just to make a failing test pass.

## Code and documentation

Keep functions small and names precise. Code has no comment lines; explain API
usage in the wiki, design decisions in design documents, and the reason for
a change in the commit message.

Use synthetic fixtures where possible. Never commit private stores, signing
keys, downloaded card art, internal deployment configuration, or credentials.
Third-party content requires its own provenance and redistribution review.

Keep GitHub PRs focused. Include protocol, snapshot, or dependency compatibility
effects and explicitly list any tests you could not run.
[AGENTS.md](AGENTS.md) contains implementation-specific constraints.
