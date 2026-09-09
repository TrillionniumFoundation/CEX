# Matrix result reconciliation security contract v3

Status: active source contract  
Sequence: CEX v12 Sequence 54  
Production authorization: `not_granted`

Security contract v3 closes the remaining causal-association ambiguity in the
response-loss recovery path. Contract v2 bound the operator request and the
row-locked outbox delivery, but the durable Consumer Entry replay result was
still selected by Matrix `event_id`. V3 requires the replayed result itself to
carry the exact relay delivery commitment that existed when the task was first
forwarded and requires the projected `task_id` to equal the raw gateway
`invocation_id` at every downstream trust boundary.

This is a repository-source contract. It does not claim that GitHub-hosted jobs
executed, that the live `main` Ruleset is installed, that independent reviewers
approved the final head, or that production release authority was granted.

## End-to-end authority chain

The binding is created only at the durable relay-to-adapter boundary:

1. `matrix-bot-relay` reads `delivery_id`, `payload_sha256`, and the immutable
   event payload from the locked outbox claim.
2. The relay sends the payload with `x-cex-delivery-id`,
   `x-cex-payload-sha256`, `x-idempotency-key`, and `idempotency-key`.
3. `delivery_binding.rs` runs before the legacy adapter event handler. It
   verifies canonical payload bytes against `payload_sha256`, validates the
   Matrix event/room/sender identities, requires both idempotency headers in
   beta and production, rejects any caller-supplied reserved binding field, and
   injects `metadata.cex_delivery_binding`.
4. The existing adapter forwarding path nests that metadata in the Consumer
   Entry request. Consumer Entry persists the complete task response, including
   `source.metadata.metadata.cex_delivery_binding`, in its durable replay
   snapshot.
5. `matrix_result_lookup.rs` may still locate a candidate cache entry by event
   ID, but it returns the result only when the persisted eight-field binding
   exactly equals the authenticated lookup request. A legacy or unbound cache
   entry fails closed.
6. `matrix_result_response_binding.rs` independently buffers every successful
   Consumer result lookup and rejects the 2xx boundary unless the returned
   top-level and nested binding copies are identical and exact, the Matrix
   source/scope matches, and non-empty `task_id == raw.invocation_id`.
7. The canonical Adapter reconciliation handler validates the authenticated
   lookup envelope and source identity. The facade-level
   `reconciliation_response_binding.rs` independently buffers every successful
   reconciliation response and rejects the 2xx boundary unless the forwarded
   durable result contains the exact eight-field embedded binding, matching
   task/invocation identity, Matrix identity scope, delivery ID, payload digest,
   complete read-only response contract, and independently recomputed request
   fingerprint.
8. The v3 operator command validates the same embedded binding and exact
   task-to-invocation identity before issuing SQL.
9. `cex_matrix_reconcile_adapter_result_v3` independently validates the
   task-to-invocation identity, embedded binding and fingerprint, then delegates
   to the v2 core, which locks the exact outbox row, recomputes the fingerprint
   from persisted authority, and performs the single allowed transition.

The repeated checks are intentional. Consumer Entry, the Adapter response
boundary, the operator command, and PostgreSQL are separate trust boundaries;
none may turn a malformed upstream success into authority for the next layer.

## Persisted binding object

The reserved object has exactly eight fields and rejects extensions:

```json
{
  "schema": "cex.matrix.delivery-binding.v1",
  "source": "matrix-bot-relay-headers-v1",
  "delivery_id": "<canonical lowercase UUID>",
  "payload_sha256": "sha256:<64 lowercase hex>",
  "event_id": "$...",
  "room_id": "!...",
  "matrix_user_id": "@...",
  "request_fingerprint": "sha256:<64 lowercase hex>"
}
```

The fingerprint algorithm remains the contract-v2 domain-separated,
length-prefixed SHA-256 over, in order:

```text
cex.matrix.adapter-result-delivery.v1
delivery_id
payload_sha256
event_id
matrix_user_id
room_id
```

This object is not accepted from the Matrix payload. The middleware rejects a
pre-existing `cex_delivery_binding` field before inserting the relay-derived
object. Production and beta requests without the complete trusted header set are
rejected. Local development requests without those headers remain usable for
non-authoritative testing, but their results cannot satisfy reconciliation.

## Replay and task identity semantics

The cache key is only a candidate locator. It is not authorization. A hit must
also satisfy all of the following:

- exact source kind, event ID, room ID, and Matrix principal;
- exact identity scope;
- non-empty task ID and the same non-empty raw gateway invocation ID;
- exact persisted delivery binding shape and source marker;
- exact delivery ID and payload commitment;
- independently recomputed request fingerprint;
- identical top-level and nested binding copies at the Consumer boundary;
- a complete read-only reconciliation envelope and a second fail-closed check
  before a successful response leaves the Adapter.

Therefore, a stale task result for the same Matrix event cannot be rebound to a
new delivery or changed payload, and a projected task cannot be separated from
its raw invocation. Honest retries with a fresh adapter envelope or observation
timestamp remain idempotent because v3 preserves the v2 split between immutable
result identity and append-only observations.

## Database least privilege

Migration
`0006_adapter_result_embedded_delivery_binding.sql` adds
`cex_matrix_reconcile_adapter_result_v3` without rewriting migrations 0001–0005.
The runtime role loses `EXECUTE` on v2 and receives only v3. V3 validates the
non-empty task/raw invocation equality and the embedded result binding, while the
v2 owner-only core retains the established row lock, unknown-outcome allowlist,
server-side result digest, collision rules, and one-transition history invariant.

The operator regression intentionally runs migrations 0001–0005 twice and the
historical regressions before applying 0006 twice. It then executes
`test-matrix-result-embedded-binding-postgres.sql` and
`test-matrix-result-task-invocation-binding-postgres.sql`, which prove:

- first v3 reconciliation and honest replay;
- one `dead_letter -> sent` transition and two append-only observations;
- v2 runtime revocation and v3 runtime grant;
- rejection of missing, changed, or extended embedded bindings;
- rejection of missing or changed raw invocation identity;
- no observation side effect from hostile inputs.

## Verification

```text
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/check-matrix-result-reconciliation-security-v3.py
python3 scripts/check-matrix-result-reconciliation-traceability-v3.py
python3 scripts/reconcile-matrix-adapter-result-v3.py --self-test
python3 scripts/test-matrix-operator-postgres-runner-v4.py
cargo fmt -p matrix-entry-adapter -p consumer-entry-api -- --check
cargo test --locked -p matrix-entry-adapter -p consumer-entry-api --all-targets
cargo clippy --locked -p matrix-entry-adapter -p consumer-entry-api --all-targets -- -D warnings
python3 scripts/matrix_operator_postgres_regression.py
```

The Consumer and Adapter package tests include hostile response-boundary cases
for missing or changed invocation identity, a missing, changed, or extended
embedded binding, different top-level/nested copies, and malformed read-only
response metadata. A successful inner lookup is not sufficient by itself; each
facade must revalidate the result before emitting 2xx.

Repository admission still requires non-empty execution on the exact final head
and prospective merge object, retained logs and artifacts, live Ruleset readback
and rejection probes, two fresh eligible approvals including independent
security approval, and normal resolution of blocking reviews. Production
authorization additionally requires real restore, deployment, rollback,
credential-custody, provider, World-authority, SLO, legal, financial, commercial,
and accountable human go/no-go evidence.

```text
all_repository_source_gaps_closed=false
all_plan_gaps_closed=false
production_ready=false
production_authorization=not_granted
```
