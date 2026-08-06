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

Hepta must compare the verified `command_id` with the queued
`TrnmCommand.idempotency_key`. A local database UUID is never a Chain command
identity.
