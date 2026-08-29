# CometBFT Receipt V2 cross-repository fixtures

These fixtures freeze the owned JSON contracts consumed by Integration and
Hepta. Each JSON file contains one compact JSON value followed by exactly one
repository text-file newline. Tests remove that one transport newline and then
require the remaining bytes to pass the strict canonical decoder byte for
byte.

The positive receipt was assembled from the single-validator local CometBFT
diagnostic evidence captured on 2026-08-07. It proves wire interoperability and
tamper classification only; it is not a release, multi-validator, or production
finality claim.

The legacy generic Research V1 lane compares the verified `command_id` with
the queued `TrnmCommand.idempotency_key`; a local database UUID is never a
Research V1 command identity. Paper Raid V4 deliberately has a different
contract: its command ID is
`ExternalKey::from_uuid("hepta.paper-raid.finality-preparation", preparation_id)`.
The preparation idempotency key is validated and echoed for audit only and is
never used as V4 Chain identity or looked up through the legacy Research V1
queue rule.

`hepta-paper-raid-v4-projection-golden-v1.json` is the shared, checked-in
Hepta-to-Chain projection corpus. Its original, rework, denied, and upheld
cases each contain a complete strict preparation plus the exact projected V4
field JSON, canonical 35-item commitment CBOR, command ID, binding commitment
and fingerprint, and domain payload hash. A fresh Hepta vendor must consume
that literal file and reproduce every value; a test-only reconstruction is not
a substitute for the shared fixture.
