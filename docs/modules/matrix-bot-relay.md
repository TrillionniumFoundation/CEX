# matrix-bot-relay module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-relay`  
Package: `matrix-bot-relay`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract defines unqualified source behavior. It is not real homeserver,
PostgreSQL recovery, exact-SHA hosted or production evidence.

## Purpose and non-goals

Relay the durable Matrix outbox to the entry adapter and deliver bounded replies
to the original room. The relay does not grant user, research, financial,
World/Game or Chain finality authority and does not execute participating Agents.
Transport acknowledgement is not a proof of the downstream business result.

## Authority and owned state

Own delivery claims, bounded attempts, immutable delivery history, outgoing
Matrix idempotency-scope bindings and validated send receipts. The same delivery
UUID remains the Matrix transaction ID. External services retain their business
and messaging authority. Dead-letter can mean unresolved remote effect; it is
never a declaration that the downstream operation definitely did not execute.

## Source layout and entry points

Catalog-bound entry points:

- `apps/matrix-bot-relay/src/main.rs`: ingress, durable claims, fixed destinations,
  transaction boundaries, credential-scope binding, send receipts and recovery.
- `apps/matrix-bot-relay/src/response_contract.rs`: bounded response classification
  and original-room reply validation, with hostile response fixtures.
- `apps/matrix-bot-relay/src/runtime_profile.rs`: two-line compatibility re-export
  of the strict parser owned by shared-config; there is no local parser logic.

Use shared transport migrations 0001, 0002, 0003, 0004 and 0005 in the adapter-owned directory.
Changing destination, receipt or source identity requires catalog and protocol review.

## Interfaces and contracts

Authenticated compatibility ingress `POST /v1/inbound/matrix-event` persists
source and adapter-delivery intent before `202`. Adapter delivery calls
`/v1/matrix/events` with immutable delivery/hash/idempotency headers. Unknown
destinations remain rejected. Replies may not change the original room, even
when a response supplies a different or malformed `room_id`. Only bounded
`m.text` or `m.notice` reply objects with a nonblank body are sent.

Matrix delivery uses Client-Server v3 send with the immutable delivery UUID as
transaction ID. Only HTTP 200 plus a bounded valid `$...` event ID and no errcode
is accepted. Empty, partial, malformed or receipt-free 2xx responses are not
success. The complete accepted event identity is durably recorded before `sent`.

## Persistence, concurrency, and recovery

Claims commit before I/O. Reply enqueue and adapter-delivery completion share a
transaction. Before a Matrix send, durably bind delivery, payload hash, room,
homeserver and a SHA-256 credential fingerprint. A changed credential or endpoint
holds the operation rather than silently creating a different deduplication scope.
Do not rotate unresolved send credentials without an explicit reconciliation plan.

Record the validated send receipt and mark `sent` in one transaction. Database
triggers reject new Matrix sent transitions without a matching immutable receipt.
Repeated identical receipts replay; conflicting event IDs are rejected. Partial
body reads are distinguished from size violations. Ambiguous Matrix transport
may retry only the same bound scope, ID and bytes within the attempt budget.

Adapter durable business replay is not yet qualified. Timeouts, transport/body
loss and ambiguous error statuses therefore enter an operator hold rather than
being blindly resent. Expired adapter claims are dead-lettered with an explicit
unknown-outcome reason and prior-owner provenance. This conservative safety
control does not close adapter end-to-end availability or response-recovery gaps.
A runtime internal failure likewise holds rather than risking another effect.

## Configuration and secrets

Required settings: `MATRIX_TRANSPORT_DATABASE_URL` or `DATABASE_URL`,
`MATRIX_RELAY_WORKER_ID`, `MATRIX_RELAY_INGRESS_TOKEN`,
`MATRIX_ENTRY_ADAPTER_TOKEN` or `MATRIX_ENTRY_INGRESS_TOKEN`, and `MATRIX_ACCESS_TOKEN`.
Profile sources `MATRIX_RELAY_RUNTIME_PROFILE`, `CEX_RUNTIME_PROFILE`, `APP_ENV`
reject unknown, empty, non-Unicode or conflicting explicit values. Beta, staging
and production are production-like, with HTTPS and separated credentials.

| Key | Default / bounds |
|---|---|
| `MATRIX_BOT_RELAY_BIND` / `MATRIX_BOT_RELAY_BIND_ADDR` | `127.0.0.1:8092` |
| `MATRIX_ADAPTER_BASE_URL` | local `http://127.0.0.1:8091` |
| `MATRIX_HOMESERVER_BASE_URL` | local `http://127.0.0.1:8008` |
| `MATRIX_RELAY_CLAIM_LEASE_SECONDS` / `MATRIX_RELAY_LEASE_SECONDS` | 60; 5–3600 |
| `MATRIX_RELAY_HTTP_TIMEOUT_SECONDS` | 20; positive and more than 5 seconds below lease |
| `MATRIX_RELAY_POLL_INTERVAL_MS` | 500; effective minimum 50 |
| `MATRIX_RELAY_INGRESS_MAX_BYTES` | 1048576; maximum 1048576 |
| `MATRIX_RELAY_MAX_RESPONSE_BYTES` | 1048576; maximum 4194304 |
| `MATRIX_RELAY_DELIVERY_MAX_ATTEMPTS` | 8; 1–100 |

Claims remain single-delivery per iteration. Endpoint userinfo, query and fragment
are rejected. Secrets must not enter committed fixtures, logs, request snapshots
or receipts. The credential fingerprint is restricted operational binding data,
not a credential source or public identifier.

## Security and trust boundaries

Authenticate ingress before persistence, disable redirects and bound bodies.
Original-room binding prevents the adapter from turning the relay into a cross-room
sender. Server responses remain untrusted until the receipt shape and configured
transport authority validate. The Matrix event receipt proves a messaging response,
not Ledger value, scientific quality or Chain finality. Do not expose raw SQL
errors that may include private source payloads.

## Verification

```text
cargo test --locked -p matrix-bot-relay --all-targets
cargo clippy --locked -p matrix-bot-relay --all-targets -- -D warnings
python3 scripts/check-matrix-recovery-contract.py
python3 scripts/check-matrix-lock-coherence.py
python3 scripts/test-matrix-recovery-contract.py
```

Database regression must cover wrong owner/fence, receipt-required completion,
receipt replay/collision, binding drift, expired adapter hold, Matrix scoped retry,
poison byte immutability and history. Source checks and pure response fixtures do
not substitute for executed PostgreSQL and real homeserver failure injection.
Existing full v12 authority gates and the aggregate remain independently required.

## Deployment and operations

Apply migrations under the schema owner, remove that credential from resident
processes and use reviewed least-privilege grants. Readiness requires the new
receipt/binding functions, not simply a reachable database. Monitor pending age,
unknown-outcome holds, scoped-retry failures, receipt conflicts and dead letters.
An operator must distinguish an effect-unknown hold from confirmed rejection.

Rollback stops claims, fences workers and retains all immutable identities,
bindings and receipts. Do not resume an older worker that automatically reclaims
adapter side effects or marks sends complete from status alone. Use a reviewed
forward migration for any semantic reversal. Production authorization is not granted.

## Compatibility and change protocol

Destination strings, healthy payload hashes and delivery UUIDs remain unchanged.
Older terminal rows are preserved as historical evidence, not upgraded to verified
receipts. API/credential rotation, bounds, retry classification, schema and
quarantine changes require this contract, module catalog, protocol review,
positive/hostile tests and fresh exact-tree evidence. No document self-qualifies.

## Bound adapter response compatibility

Before any completion transaction, the relay validates `accepted`, `action`,
source event, sender and room from the actual adapter envelope. Help and ignored
events may have `accepted=false`; this is a transport disposition, not a business
success. An explicit null projected reply is valid and sends no message. The
existing status endpoint's task-ID response is accepted only for the identical
`/status` argument in the source bytes. Missing/mismatched IDs and duplicate-cache
hits are unknown-result holds, not proof that a prior effect completed. Typed
replies and original-room binding still apply. Send endpoints now retain a
configured reverse-proxy path; embedded endpoint credentials and query/fragment
values are rejected. Full durable adapter result reconciliation remains open.

## Compiler and committed lock repair

`rust-toolchain.toml` selects Rust 1.98.1. The Matrix direct-dependency lock
preflight checks the two workspace package entries against their manifests; it
does not resolve the complete dependency graph or prove compilation. Preserve
`--locked` and execute the full package gates after applying the reviewed lock
patch. See `docs/build-unblock-round6.md` for the preimage and remaining evidence.

## Plain-reply extension boundary

`response_contract::bound_reply` now rejects every reply key except `msgtype` and
`body`. An adapter cannot attach structured edits, replacement content, HTML,
mentions, relations or unknown extensions to a nominal plain-text reply. Rich
reply extensions now hold and need explicit protocol review; this does not
sanitize the plain text itself or promise client notification behavior. See
`docs/matrix-stream-scope-v1.md`; Rust/real homeserver tests remain required.

## Shared profile linkage (round 15)

This package now depends on the existing local `shared-config` crate, and the
profile compatibility file re-exports its resolver without any local policy.
The three explicit environment sources, non-Unicode rejection and startup order
are unchanged. `Cargo.lock` adds only that existing local direct edge. No registry
package/version/checksum or protocol/schema change is part of this refactor.
The original semantic tests are retained once in shared-config and must execute
there; testing only this dependent package does not run dependency unit tests.
See `docs/matrix-profile-sharing-v1.md`. Actual locked resolution, compilation,
formatting, lint and black-box startup verification remain required.
