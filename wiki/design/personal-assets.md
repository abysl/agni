# Personal presentation assets

Audience: application contributors integrating optional peer-shared pictures.

`agni_net::personal_asset` serves one in-memory payload over the separate
`agni-personal-asset/1` ALPN. It has no Spirit store, journal, index or gossip
integration. Receivers do not publish downloaded bytes.

Publish a bounded payload with `PersonalAsset::publish`, then send the returned
ticket only through the active table's private roster. The ticket names the
authenticated endpoint, a cryptographically random 256-bit capability and a
BLAKE3 digest. A holder of the capability can retrieve that picture; this is a
bearer capability, not proof of seat ownership. Never put tickets in public
discovery advertisements. A participant can retain or redistribute a received
picture, just as they can take a screenshot.

Replacing a payload or calling `clear` revokes its previous capability. Clear
on departure and when deselecting the personal picture. An already-started
transfer may finish. Payloads are limited to 4 MiB, two simultaneous transfers
and a 20-second exchange deadline. Receivers verify the fingerprint. The
application must independently decode images with pixel/allocation limits,
bound its cache, and cancel pending transfers when the user disables them.

The existing optional playmat roster string carries the versioned ticket;
this adds no session-wire variant or deterministic game action. Old clients
ignore an unknown presentation scheme.
