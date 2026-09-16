# Two-player matchmaking

Audience: application and transport contributors familiar with Rust async code,
new to Agni's matchmaking API. Matchmaking is transport state, not a game rule.

`agni_net::matchmaking::Matchmaker` implements `agni-matchmaking/1`. Register it
with the application's Spirit router alongside `TableProtocol`. Spirit itself
remains game-independent; its existing table adverts carry opaque names.

Call `prepare()` before opening a waiting table, then `begin(key)` once its
actual pinned configuration is known. Publish the returned `Ticket::advert()`
as the table name. The advert contains a version prefix, fresh 128-bit search
nonce and 256-bit BLAKE3 settings fingerprint. Applications define canonical
settings and include their game identity, player count, wire version and module
pins. No card lists or private faces belong in this digest or advert.

The higher endpoint identity offers to the lower identity. A claim is the
CBOR-encoded advertised ticket, over one bidirectional stream; the reply is a
CBOR boolean. Each side finishes its send stream. Reads are limited to 512
bytes; the complete exchange has a four-second deadline. Endpoint identity
comes from the authenticated connection, never from a claimed player name.

Preparing, searching and outbound offers reject table admission. Incoming
claims atomically reserve one peer and exclude outbound offers. A reservation
expires after 30 seconds unless `admit(peer)` consumes it for a table join.
Matched admission permits that same identity to reconnect, but rejects any
third player. Call admission only after wire-version validation and before
seating a guest. A matched table must not become an unrestricted table merely
because it leaves the matchmaking screen.

The application closes its empty waiting table before joining its reserved
host, then validates the host's welcome configuration against the search key.
Withdraw adverts while reserved or matched; restore the search advert after
an expired reservation. Stale gossip cannot reserve a newer search because its
nonce differs. Lost replies and disappearing guests release their reservations
by expiry. Cancelled offers check their original search identity before
publishing a result.

`bridge::cancel_join()` removes queued joins, drops the outbound link and
invalidates the active attempt generation. Late connection events cannot update
a subsequent session. Ordinary game messages and session wire version remain
unchanged; older apps cannot participate in this new matchmaking protocol.

This provides discovery and exclusive seating, not anti-cheat or reputation.
Anyone can announce a settings fingerprint or temporarily occupy a reservation.
The mesh's normal reachability and advert-expiry limits still apply.

Run `cargo test --locked -p agni-net --lib --test matchmaking --test host_bridge`.
The integration test discovers a host through a third gossip peer, reserves it,
seats a real replica over Iroh, compares logs, and rejects a competing claimant.
It uses temporary stores and direct local endpoints, without public relays.
