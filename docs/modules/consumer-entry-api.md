# consumer-entry-api module contract

Status: active module contract  
Workspace member: `services/consumer-entry-api`  
Package: `consumer-entry-api`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `consumer-edge`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json` and defines the exact
Consumer Entry repository boundary. The Matrix recovery extensions are specified
by `docs/matrix-result-reconciliation-v2.md` and
`docs/matrix-result-reconciliation-v3.md`. Documentation is not hosted execution,
independent approval, or production authorization.

## Purpose and non-goals

Consumer Entry translates consumer- and Matrix-facing actions into bounded CEX
calls and projects backend state into player and task views. It also exposes an
authenticated read-only lookup of an already durable Matrix task result for the
response-loss reconciliation path. That lookup never repeats the original task
and never mutates Matrix delivery state.

Consumer Entry may not become the unique writer for research facts, Nakama state,
Ledger balances, provider outcomes, World/Game authority, or Chain finality. An
edge projection cannot promote itself into another component's authority.

## Authority and owned state

The service owns consumer sessions, identity-binding governance, edge-local rate
limits, CSRF and idempotency material, replay-store entries, and explicitly
assigned product read models. Its replay result is authoritative only for the
previously admitted Consumer Entry task projection and only when its persisted
source metadata proves the exact relay delivery binding.

A cache key, projection, HTTP status, transport acknowledgement, caller-supplied
delivery ID, or request fingerprint does not transfer authority from Ledger,
research, providers, World/Game, Nakama, or Chain. Event ID remains a candidate
locator rather than an authorization decision.

## Source layout and entry points

Catalog-bound entry points remain exact:

- `services/consumer-entry-api/src/main.rs`;
- `services/consumer-entry-api/src/lib.rs`;
- `services/consumer-entry-api/src/consumer_ingress.rs`;
- `services/consumer-entry-api/src/identity_admin_routes.rs`;
- `services/consumer-entry-api/src/task_routes.rs`;
- `services/consumer-entry-api/src/term_exchange_backend.rs`;
- `services/consumer-entry-api/src/world_routes.rs`;
- `services/consumer-entry-api/src/health_metrics.rs`;
- `services/consumer-entry-api/src/tests.rs`.

Additional security-relevant entry points are:

- `services/consumer-entry-api/src/matrix_result_lookup.rs` — canonical
  exact-delivery read-only lookup and persisted-binding validator;
- `services/consumer-entry-api/src/replay_store_snapshot.rs` — bounded stable
  snapshot reader;
- `scripts/check-matrix-result-reconciliation-security-v3.py` — source and
  self-test gate for the delivery-binding chain.

Any new binary, route, durable owner, public source boundary, or removed path must
update the catalog where applicable, this contract, traceability, and executable
verification in the same change.

## Interfaces and contracts

Consumer routes cover task admission and projection, browser and session
surfaces, identity administration, bounded World/League projections, health, and
metrics. Raw Ledger, Audit, provider, research, and finality APIs remain behind
their owning boundaries.

`POST /v1/matrix/messages/result` accepts canonical `delivery_id`,
`payload_sha256`, `event_id`, `matrix_user_id`, `room_id`, and the
length-prefixed domain-separated `request_fingerprint`. The service recomputes
that fingerprint before reading replay state and requires the signed
`matrix_result_lookup` assertion to bind the same fingerprint, subject, room,
issuer, audience, issuance, expiry, and approved key.

A successful response returns only an existing replay result whose source kind,
event, user, room, identity scope, task ID, and optional invocation ID are
consistent. It additionally requires the cached result's nested
`source.metadata.metadata.cex_delivery_binding` object to have exactly eight
fields and to match the delivery, payload, event, room, principal, fingerprint,
schema, and relay source marker. Legacy or unbound entries fail closed.

## Persistence, concurrency, and recovery

PostgreSQL or explicitly configured bounded files provide durable edge state.
Replay snapshots are read as stable regular files with a fixed maximum byte
size; changed, unavailable, malformed, future-dated, expired, identity-mismatched,
outcome-less, hard-linked, or symbolic inputs fail closed.

The replay map remains keyed by original Matrix event for task idempotency, but a
cache hit is only a candidate. The persisted binding injected before the initial
Consumer Entry admission must exactly match the later authenticated recovery
request. The Adapter and PostgreSQL v3 function perform independent checks
before any delivery-state repair, while Consumer Entry itself never writes that
repair.

Remote effects occur only after durable intent or claim commit and before a
separate outcome transaction. Possible-side-effect timeouts remain pending or
enter a reviewed reconciliation path; they never authorize a new operation
identity or fallback task creation.

## Configuration and secrets

Runtime profile, ingress token, session-auth issuer, key and secret registry,
expected audience, clock-skew and TTL limits, replay-store path, window and size,
identity-binding paths, approval sources, actor allowlists, rate-limit storage,
downstream URLs, and product rollout switches are explicit configuration.

Production-like startup fails before listening when required durable storage,
credentials, trust anchors, approved revisions, or modes are absent. Session
secrets, raw replay bytes, database URLs, private authority hashes, and private
identities must not appear in logs, metrics, errors, traces, issues, or repository
fixtures.

The result lookup receives only the adapter ingress credential and a bounded
signed session assertion. It does not accept generic browser credentials as a
replacement for the Matrix recovery principal.

## Security and trust boundaries

Outside local development, browser mutations require signed sessions and CSRF.
Matrix result lookup separately requires the adapter ingress token and bounded
HMAC assertion. Parseable Matrix identifiers, event ID, a task ID, and HTTP 200
are not proof that a cached result belongs to the requested delivery payload.

Unknown JSON fields, noncanonical delivery IDs, invalid hashes, changed request
fingerprints, cross-user, cross-room or cross-event results, expired assertions,
unapproved issuers or keys, oversized bodies, unstable replay files, inconsistent
task or invocation IDs, missing delivery binding, extra binding fields, and a
wrong relay source marker fail closed. The response retains
`production_authorization=not_granted` as an authority boundary rather than a
runtime release switch.

## Verification

Catalog compatibility commands remain exact:

```text
cargo test -p consumer-entry-api
cargo clippy -p consumer-entry-api --all-targets -- -D warnings
```

Sequence 54 qualification uses the complete set:

```text
cargo fmt -p consumer-entry-api -- --check
cargo test --locked -p consumer-entry-api --all-targets
cargo clippy --locked -p consumer-entry-api --all-targets -- -D warnings
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/check-matrix-result-reconciliation-security-v3.py
python3 scripts/reconcile-matrix-adapter-result-v3.py --self-test
```

Behavioral verification covers signed ingress and sessions, identity governance,
bounded replay snapshots, fingerprint changes for every component, exact
persisted binding shape, source, scope and task matching, downstream failure
translation, and prevention of World or League projections becoming CEX
authority. The exact source SHA must also pass non-empty hosted workflows and
appear in the immutable candidate manifest; a source checker or zero-step job is
not execution evidence.

## Deployment and operations

Deploy as a supporting edge behind authenticated transport, with operator and
administrative access separated from consumer routes. Readiness independently
reports listener state, issuer-registry validity, replay-store stability,
Consumer Entry dependencies, and recovery lookup availability. Record image and
source identity, runtime profile, approved registry revision, credential IDs,
replay-store retention, dependency identities, and rollback boundary without
retaining secrets.

Monitor lookup authentication failures, unstable snapshots, unresolved or
expired results, missing and mismatched persisted bindings, downstream
reachability, rate limits, and projection age. Rollback stops ingress and lookup
traffic, preserves durable idempotency and replay identities, and deploys only a
schema- and contract-compatible prior binary or reviewed forward repair.
Repository CI does not replace representative-volume recovery, sustained load,
credential custody, or independent approval.

## Compatibility and change protocol

Consumer terms remain projections of backend contracts. New product surfaces
must declare their owner and may not silently expand the edge into another
authority. The Matrix delivery fingerprint and persisted eight-field binding are
versioned security contracts; changing component order, byte encoding, source
marker, assertion binding, replay semantics, or retirement conditions is
breaking.

Changes to authority, routes, public types, persistence, configuration, retry
semantics, or topology require this contract, catalog metadata where applicable,
relevant ADRs and protocols, executable tests, hosted gate wiring, traceability,
and a new exact candidate trigger. No module document or local source checker may
declare repository qualification or production authorization.
