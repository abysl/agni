# GameBlob legacy compatibility

`games/riftbound-turns/tests/blob_compatibility.rs` exercises the public
`GameBlob::decode` boundary with independently written CBOR for the three
persisted layouts that are still accepted:

- v7 covers both the original boolean spell lock (eight-field seats) and the
  numeric lock plus chosen champion (nine-field seats). Its twelve-field card
  carries an ordinary grant, attachment, hidden-zone metadata, control, a
  named card, a might modifier, and the preserved winner/death fields.
- v9 covers the ten-field readiness seat and twelve-field card with ordinary
  and costed grants. The costed grant bytes include Own, Chaos, and Rainbow
  powers, while the v9 card deliberately has no name field.
- v10 covers the current eleven-field seat and thirteen-field card, including
  readiness, named state, ordinary and costed grants, attachment, and hidden
  metadata.

Each decoded fixture is re-encoded as a separately authored canonical v10
document. The expected bytes are produced directly through the SDK
`MapWriter`/`Writer`, so the test does not turn the current encoder into its
own oracle. The checks also verify representative public fields and preserve
the costed-grant bytes without depending on the Rust representation of that
field; this keeps the test usable while the grant representation changes.

The negative case covers an unsupported version and each cross-version seat or
card length. These tests establish parsing and canonical serialization
compatibility. They do not prove that a decoded game state is legal or that a
particular card keyword has the intended gameplay semantics.
