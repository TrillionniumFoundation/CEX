# consumer-entry-api module contract

Status: active module contract  
Workspace member: `services/consumer-entry-api`  
Package: `consumer-entry-api`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `consumer-edge`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines one exact
repository boundary; it is not release evidence or production authorization.

## Purpose and non-goals

Consumer Entry translates consumer- and Matrix-facing actions into bounded
CEX/Hepta calls and projects backend state into player/task views. It also offers
an authenticated read-only lookup of an already durable Matrix task result for
the response-loss reconciliation path.

It may not become the unique writer for research facts, Nakama state, Ledger
balances, provider outcomes, World/Game authority, or Chain finality. The Matrix
lookup never repeats the original task and never changes Matrix delivery state.

## Authority and owned state

The service owns consumer sessions, identity-binding governance, edge-local rate
limits, CSRF/idempotency material, replay-store entries, and product read models
explicitly assigned to this package. Its Matrix replay result is authoritative
only for the previously admitted Consumer Entry task projection.

A cache row, projection, HTTP response, transport acknowledgement, delivery ID,
or request fingerprint does not transfer authority from Ledger, research,
provider, World/Game, Nakama, or Chain components.

## Source layout and entry points

Catalog and security-relevant entry points include:

- `services/consumer-entry-api/src/main.rs`;
- `services/consumer-entry-api/src/lib.rs`;
- `services/consumer-entry-api/src/consumer_ingress.rs`;
- `services/consumer-entry-api/src/identity_admin_routes.rs`;
- `services/consumer-entry-api/src/task_routes.rs`;
- `services/consumer-entry-api/src/matrix_result_lookup.rs` — exact-delivery
  read-only lookup;
- `services/consumer-entry-api/src/replay_store_snapshot.rs` — bounded stable
  snapshot reader;
- `services/consumer-entry-api/src/term_exchange_backend.rs`;
- `services/consumer-entry-api/src/world_routes.rs`;
- `services/consumer-entry-api/src/health_metrics.rs`;
- `services/consumer-entry-api/src/tests.rs`.

Any new binary, route, durable owner, public source boundary, or removed path must
update the catalog, this contract, traceability, and executable verification in
the same change.

## Interfaces and contracts

Consumer routes cover chat/task admission and projection, browser/session
surfaces, identity administration, bounded World/League projections, health, and
metrics. Raw internal Ledger, Audit, provider, research, and finality APIs remain
behind their owning boundaries.

`POST /v1/matrix/messages/result` accepts canonical `delivery_id`,
`payload_sha256`, `event_id`, `matrix_user_id`, `room_id`, and the
`request_fingerprint` defined by `docs/matrix-result-reconciliation-v2.md`. The
service independently recomputes the fingerprint before reading replay state and
requires the signed `matrix_result_lookup` assertion to bind the same fingerprint,
subject, room, issuer, audience, issuance, expiry, and approved key.

A successful response echoes all delivery-binding fields and returns only an
existing replay result whose source kind, event, user, room, identity scope, task
ID, and optional invocation ID are internally consistent. It does not infer
success from transport status or create a task on lookup.

## Persistence, concurrency, and recovery

PostgreSQL or explicitly configured bounded files provide durable edge state.
Replay snapshots are read as stable regular files with a fixed maximum byte size;
a changed, unavailable, malformed, future-dated, expired, identity-mismatched, or
outcome-less entry fails closed.

The lookup remains keyed by the original Matrix event for Consumer Entry task
idempotency, while the signed and echoed delivery fingerprint binds the recovery
request to a particular Matrix outbox delivery. The downstream Adapter and
PostgreSQL function perform independent fingerprint checks before delivery-state
repair. Consumer Entry itself never writes that repair.

Remote effects occur only after durable intent/claim commit and before a separate
outcome transaction. Possible-side-effect timeouts remain pending or enter a
reviewed reconciliation path; they never authorize a fresh operation identity.

## Configuration and secrets

Runtime profile, ingress token, session-auth issuer/key/secret registry, expected
audience, clock skew and TTL limits, replay-store path/window/size, identity
binding paths, approval sources, actor allowlists, rate-limit storage, downstream
URLs, and product rollout switches are explicit configuration.

Production-like startup must fail before listening when required durable storage,
credentials, trust anchors, approved revisions, or modes are absent. Session
secrets, raw replay bytes, database URLs, authority hashes, and private identities
must not appear in logs, metrics, errors, traces, or repository fixtures.

## Security and trust boundaries

Outside local development, browser mutations require signed sessions and CSRF.
Matrix result lookup additionally requires the adapter ingress credential and a
bounded HMAC assertion. Parseable Matrix identifiers are not proof of identity.

Unknown JSON fields, noncanonical delivery IDs, invalid hashes, changed delivery
fingerprints, cross-user/room/event results, expired assertions, unapproved
issuers/keys, oversized bodies, unstable replay files, and inconsistent task or
invocation IDs fail closed. HTTP 200 alone never proves a business result.

The service returns `production_authorization=not_granted`; this is a boundary
statement, not a runtime switch and not evidence that external authority accepted
the result.

## Verification

Required commands are:

```text
cargo fmt -p consumer-entry-api -- --check
cargo test --locked -p consumer-entry-api --all-targets
cargo clippy --locked -p consumer-entry-api --all-targets -- -D warnings
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
```

Behavioral verification covers signed ingress/session/CSRF, identity governance,
bounded replay snapshots, exact delivery fingerprint changes for every component,
source/scope/task matching, downstream failure translation, and prevention of
World/League projections becoming CEX authority.

The exact source SHA must also pass the non-empty hosted workflow and appear in
the generated immutable candidate manifest. A source checker or zero-step job is
not execution evidence.

## Deployment and operations

Deploy as a supporting edge behind authenticated transport. Separate operator
and administrative access from consumer routes. Record image/source identity,
runtime profile, issuer-registry revision, credential identifiers, replay-store
identity and retention, dependency identities, readiness dimensions, rollback
boundary, alerts, and owner escalation.

Monitor lookup authentication failures, replay snapshot instability, unresolved
and expired results, identity mismatches, downstream reachability, rate limits,
and projection age. Repository CI does not replace representative-volume restore,
sustained load, real credential custody, independent approval, or human go/no-go.

## Compatibility and change protocol

Consumer terms remain projections of backend contracts. New product surfaces must
declare their owner and may not silently expand the edge into another authority.
The Matrix delivery fingerprint algorithm and response fields are versioned
security contracts; changing their component order, byte encoding, assertion
binding, replay semantics, or retirement conditions is breaking.

Changes to authority, public routes/types, persistence, configuration, retry
semantics, or topology require this contract, the module catalog, relevant ADRs
and protocols, executable tests, hosted gate wiring, and a new exact candidate
trigger. No module document may declare repository closure or production
authorization.
