# R0 match-state review

R0 adds an isolated deterministic `MatchState` foundation for Riftbound Match
rematches. The module is intentionally not wired into `GameBlob`, the turn
dispatcher, Reset, host transport, or Kai. The standalone integration harness
includes the module with `#[path = "../src/match_state.rs"]`; R1 must integrate
the same transitions at the authoritative serialized plugin checkpoint before
this behavior is described as a production backend feature.

The state records the match generation, opening chooser, per-seat wins, the
current and previous game, the immediate terminal result, and sorted
per-seat battlefield usage. A first game exposes an `Opening` policy whose
chooser represents the already-authorized real roll winner. A won game exposes
a `Loser` policy for the next game. A draw exposes a `Fixed` policy retaining
the previous first seat and an exact, no-sideboard battlefield selection. The
opening `Unused` policy also disallows sideboarding; only a previous winning
game enables it.
There is no second roll entitlement in the later policies.

`record_result` increments a winner immediately and is idempotent for the same
result. A conflicting winner or draw is refused. A won game consumes both
players' presented battlefield names in their respective usage lists; the same
semantic name may still be presented by both seats in one game, and reuse is
checked per seat rather than globally. A draw consumes neither name. Reset
requires a recorded result, refuses an unfinished or duplicate reset, refuses
a completed match, and uses checked generation and score updates. CBOR uses
ordered map writers and sorted per-seat usage histories, with repeated names
presented again when a draw permits the same battlefield before a later win;
the name is consumed once when that later win records. Unknown fields are
skipped on decode.

The fallible constructor requires a valid opening seat; R1 must call it only
after the authoritative opening roll has produced and verified that seat, not
instantiate a default seat before the roll. `start_policy` and
`selection_policy` become unavailable while a game is active, including after
its result is recorded and before Reset. The public `summary`, `StartPolicy`,
`SelectionPolicy`, `canonical_battlefield_name`, and
`canonical_battlefield_selection` functions are the small read-only surface
intended for R1 presentation and authoritative integration. The selection
helper accepts raw names, canonicalizes them, and checks the requesting seat's
usage, so clients cannot bypass filtering with whitespace or title casing. No
hidden deck, unrevealed choice, or private selection data is stored here.

Decode validation rejects scores above two or a simultaneously complete pair,
generation/previous-game contradictions, too many wins for the recorded game
count, per-seat usage lengths that do not equal the total wins, missing winning
battlefield membership, and completed states without a current winning result.
It also checks that a current winning result is the match-completing winner,
that the known previous/current wins fit each seat's score, and that draw
rematches retain their recorded first seat and battlefield pair. The
generation bound saturates at the integer maximum so a legitimate
max-generation current game still round-trips. These are consistency checks
derived from the compact ledger; they do not reconstruct hidden history or
cryptographically verify deck registration.

Validation from the isolated harness:

```text
CARGO_TARGET_DIR=/build/agni-takeover/match-state CARGO_BUILD_JOBS=8 \
  cargo test -p agni-riftbound-turns --test match_state
10 passed

CARGO_TARGET_DIR=/build/agni-takeover/match-state CARGO_BUILD_JOBS=8 \
  cargo clippy -p agni-riftbound-turns --test match_state -- -D warnings
passed
```

The tests cover both winner seats, loser first/last choices, draw score and
fixed-first behavior, exact draw battlefield reuse, per-seat exclusions,
same-name presentation by both seats, idempotent and conflicting result
recording, reset refusal, deterministic CBOR round trips, and generation
overflow. They do not claim deck registration, simultaneous private staging,
sideboard commitment, real terminal hooks, or negotiated draw production; R1
must settle those at the integration boundary described by the rematch design.
