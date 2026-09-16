# Engine boundary evidence after 4d292547

Test-only commits 8ac7a468 and af477d9d strengthen the final engine checkpoint before Astra review. No production behavior changed.

The limit fixture now exceeds cards, counters and aggregate annotation limits separately in current state and the rollback capsule. It calls each raw prepass directly, verifies the other half passes, and rejects the complete versioned snapshot. Exact-cap snapshots restore to equal states. Annotations are distributed across four individually small rows; separate outer-map and roster-derived Peek controls retain the card count at its valid cap. The deliberately extra Peek is a duplicate, and the extra outer annotation row references an absent ID: their purpose is to prove the raw count guards run before semantic rejection, not to claim those payloads are otherwise valid. Nongroup oversized-card compatibility remains accepted.

Incoming absent-seat Move/Spawn tests check admission validation, then compare the entire sequenced-refusal state with only the documented next_seq increment. An initial assertion expecting sequenced refusal to preserve next_seq failed; the frozen contract and existing rejecting-decider tests explicitly require consuming that slot. The test now distinguishes admission from already sequenced folding. Successful sparse/shared placement restores exactly.

Focused limit and action tests pass. The final versioned-payload limit run passes; sim all-target warnings-denied clippy passed before the last three assertion-only changes. Remaining engine source disposition belongs to Astra; SDK source and the real differential/export/session gate remain separate acceptance requirements.
