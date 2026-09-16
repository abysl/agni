# Rule references and pool data

Audience: contributors changing Riftbound rules or the data used by rule tests.
This directory is not a player-facing rulebook.

Consult the publisher's [rules hub](https://playriftbound.com/en-us/rules-hub/)
for official rules. Cite the edition/date alongside a rule number: numbering
can change between editions. A checked-in historical reference is not proof
of the current official ruling.

The existing extracted rule-text files are reference material, not project
authored GPL code. Their presence does not establish redistribution permission.
Do not add new copies of entire publications to documentation or release bundles.

## Pool files are machine input

Markdown under `pool/` is parsed by tests and consumers. It contains deck
lists, card rows, and optional coaching sections. It is not ordinary prose
that can be freely reorganized by a documentation or formatting pass.

A card row uses the existing `- **Name** (...): text` grammar. The parsers
depend on its delimiters. Coaching bullets must not accidentally match that
grammar. Inspect the consumers and run their tests before changing it.

Use synthetic examples for new tests when possible. Card text, artwork, and
catalog dumps need separate provenance and rights review; the repository's
source-code license does not automatically apply to them.
