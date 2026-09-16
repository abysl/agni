# Documentation guide

Audience: readers choosing a starting point and contributors editing documentation.

Start with the [project introduction](../README.md). You should not need to
know the project's other libraries, development history, or maintainer's
environment to understand it.

## Choose a document

| Document | Intended reader | What it provides |
|---|---|---|
| [Agni implementation rules](../AGENTS.md) | Coding assistants and implementation contributors | Constraints to preserve while editing |
| [Contributing to Agni](../CONTRIBUTING.md) | New contributors with basic programming knowledge | Prepare, test, and submit a change |
| [Agni](../README.md) | First-time visitors | What the project does, limitations, and where to start |
| [Shared deck types](../games/deck/README.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Rule references and pool data](../games/riftbound/rules/README.md) | Rule and test-data contributors | Reference provenance and machine-readable pool constraints |
| [Agni architecture](design/architecture.md) | Developers new to the codebase | Responsibilities, vocabulary, and code navigation |
| [Game counters](design/counters.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Deck import and resolution](design/deck-import.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Building and integrating game plugins](design/plugins.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Riftbound rules implementation](design/rules-engine.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Developing Agni](development.md) | Programmers new to the project | Install tools, build, test, and troubleshoot |

## Writing for the reader

Introductions explain the problem, capabilities, limitations, and next step.
They expand project-specific names before using them and do not double as
reference manuals.

Contributor guides assume basic programming and Git, not knowledge of this
codebase. Setup instructions name prerequisites, the working directory, the
command, the expected result, and common failures.

Subsystem references can assume the linked introductory material, but should
state that prerequisite. Explain why a boundary exists before listing internal
symbols. Distinguish implemented behavior from a proposal.

Specifications serve compatibility work: preserve precise contracts and
explicit status markers. A prose rewrite must not silently change a protocol.

Machine-readable fixtures are data even when their extension is Markdown.
Do not paraphrase or reflow data files as part of a documentation edit.
Keep credentials, personal paths, and internal deployment details out of
public examples. Use local or example addresses when showing configuration.
