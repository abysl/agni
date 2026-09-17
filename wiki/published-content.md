# Operating a curated content service

Audience: operators publishing a selected catalog of game assets.

Start `deck-gateway STORE --published-only --gateway PORT` to serve content
already staged in that store. This mode disables automatic peer replication
and queued mesh downloads before the mesh starts. The asset resolver answers
only locally published journal entries; unknown keys return 404 even when a
request supplies a URL. It does not query peer asset indexes or fetch URLs.

This is not a filter that can make an existing mixed-purpose store safe:
every blob in the serving store is public. Use a dedicated, reviewed store.
Authenticated administration and local filesystem access remain trusted and
can publish content. Keep their credentials private. Do not share this store
with applications accepting user uploads.

`curate-store SOURCE NEW_DEST --ref NAME=HASH --asset JOURNAL/NAME=HASH` copies
only explicitly approved ref closures and individually approved assets into
a new destination. It refuses an existing destination or a changed root hash,
verifies copied content addresses, regenerates the asset index, and copies
card-image journal mappings only for blobs in the selected closure. It leaves
the source untouched. Review the chosen roots: approving a root approves its
reachable content. Missing descendants remain absent rather than being fetched.

The tool does not copy identity keys, trust configuration, credentials, peer
indexes, or old playmat journals. An operator preserving an existing endpoint
must transfer its identity files securely while the old service is stopped;
never run two endpoints with the same device identity. Keep the old store
offline as a recoverable backup. Validate approved assets and rejection of
unapproved blob hashes over both HTTP and the blob protocol after migration.
