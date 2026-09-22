# Riftbound constructed ban validation

Audience: clients that import, edit, save or seat Riftbound decks.

`agni_riftbound::legality::check` applies the current constructed ban policy in
both `Mode::Standard` and `Mode::Constructed2v2`. Banned cards produce
`Rule::Banned`, `Grade::Break` and a `Verdict::Broken` result. The finding names
the card, its registered zone, all matching print IDs, the effective date and
the official source. Sideboard entries are violations, not swap advisories.
Zero-count entries are absent from the registered deck.

## September 2026 additions

[Riot's September announcement](https://playriftbound.com/en-us/news/announcements/september-ban-list-updates-effective-september-18-2026)
was published September 15, 2026. It adds these cards to **Standard and
Constructed 2v2**, effective **September 18, 2026**:

| Named card | Reference printing |
| --- | --- |
| Ekko, Recurrent | OGN-110/298 |
| Stacked Deck | OGN-183/298 |

Riot cites Ekko's low-interaction recurring combo and Stacked Deck's excessive
consistency, especially in Kennen strategies. The announcement specifies no
effective time of day or timezone and no simultaneous unbans.

The ban matches the full named card across printings and supported punctuation
variants. Another Ekko title is not banned. Reference print IDs also identify
the banned cards when supplied names are missing or inconsistent. New bans
should extend the central list, not replace earlier entries.

## Client responsibility

Importing or saving a historical list is allowed and does not silently remove
banned cards. Re-run legality validation before seating the deck. Enforced
constructed play must reject `Verdict::Broken`; an editor warning alone is not
enforcement. Kai's selection gate already rejects broken decks when rules are
enforced and requires explicit confirmation for invalid free-table decks.
Clients must update their pinned dependency to receive this policy.

This validator has no Limited or unrestricted mode. Those formats must not be
classified as Standard merely to run this ban check. Free-table policy remains
the caller's decision; card scripts are preserved so historical play and replay
continue to work. Both existing copies of the Riftbound deck crate, in Agni and
agni-rfb, carry this update while Kai still consumes the Agni Git dependency.

This is an immutable current-policy snapshot, not a date scheduler. It performs
no clock reads or network requests and adds no ban checks to the deterministic
in-game fold. No wire, plugin-state, or snapshot format changes are required.
The policy does not independently verify that a supplied identity came from a
trusted card catalog; ordinary import resolution remains responsible for that.
