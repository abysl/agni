# Deck import and resolution

Audience: contributors implementing import formats or debugging an unresolved
deck. Read [architecture](architecture.md) first.

Importing has two distinct steps. Parsing converts text, a code, or a supported
link into requested card entries. Resolution maps those entries to known
cards. A successful parse is not proof that every card was found.

The importer layer owns the game-neutral parser/resolver structure.
Game-specific implementations provide vocabulary, identifiers, and card/deck
types. Reuse that structure instead of copying the entire pipeline for a new
format.

Keep unresolved entries visible, with reasons. Retry only the work that can
change, and preserve the user's list so a transient network error does not
silently produce a shorter deck.

Native fetchers and browser-compatible parsing are different feature sets.
Pure parsing must not acquire a network requirement. URL fetching needs an
explicit allowlist and policy; never let an arbitrary submitted URL become
an unrestricted server-side fetch.

Tests should cover malformed input, duplicate counts, ambiguous names, missing
metadata, partial resolution, and supported round trips. Use synthetic
responses rather than making unit tests depend on a third-party service.
