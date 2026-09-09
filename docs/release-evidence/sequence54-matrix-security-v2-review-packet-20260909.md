# Sequence 54 Matrix reconciliation security-v2 review packet

This packet maps the three blocking Matrix reconciliation findings to the current repository implementation and regressions. It is repository review evidence only. It does not grant production authorization and does not substitute for GitHub-hosted exact-head and prospective-merge execution.

## 1. Stable retry identity across fresh observations

The authority-bearing reconciliation identity is the immutable delivery/result/request binding. Per-observation values such as lookup envelope generation time, operator observation time, transport receipt details, and raw lookup-response digest are observation evidence; they must not redefine the business-result identity or turn a legitimate retry into a collision.

Implementation and regression anchors:

- `services/matrix-entry-adapter/operator-migrations/0005_adapter_result_causal_binding.sql`
- `services/matrix-entry-adapter/src/result_reconciliation.rs`
- `scripts/reconcile-matrix-adapter-result.py`
- `scripts/test-matrix-result-causal-binding-postgres.sql`
- `scripts/check-matrix-result-reconciliation-security-v2.py`

Acceptance behavior:

1. first valid reconciliation performs one terminal transition;
2. retry with the same immutable result/delivery/request binding but fresh observation metadata returns replay/idempotent disposition and performs no second transition;
3. changed result content, task authority, delivery payload, or request fingerprint fails closed as collision;
4. observations remain append-only and auditable.

## 2. Closed PostgreSQL transport and subprocess environment

The operator database path must use either a protected local transport or certificate- and hostname-verified TLS for non-loopback TCP. The subprocess environment must be constructed from an allowlist and must not inherit ambient libpq service, password, option, or trust configuration.

Implementation and negative-test anchors:

- `scripts/reconcile-matrix-adapter-result.py`
- `scripts/check-matrix-result-reconciliation-security-v2.py`

Required fail-closed cases include empty/ambiguous credentials, plaintext or downgrade-capable remote transport, missing trust root, hostname-verification absence, hostile `PGSERVICE`, `PGOPTIONS`, `PGPASSFILE`, and unrelated inherited `PG*` state.

## 3. Durable causal binding to the exact delivery payload

The lookup and returned result are bound to the immutable outbox delivery through:

- `delivery_id`;
- exact `payload_sha256`;
- domain-separated `request_fingerprint`;
- Matrix `event_id`, `room_id`, and `matrix_user_id`;
- task/result authority checked under the row lock before terminal transition.

Implementation and hostile-test anchors:

- `services/consumer-entry-api/src/matrix_result_lookup.rs`
- `services/matrix-entry-adapter/src/result_reconciliation.rs`
- `services/matrix-entry-adapter/operator-migrations/0005_adapter_result_causal_binding.sql`
- `scripts/test-matrix-result-causal-binding-postgres.sql`
- `scripts/check-matrix-result-reconciliation-security-v2.py`

A same-event cached result cannot be reassociated with a changed delivery, payload, task, result, room, or principal without failing closed.

## Required executable qualification

The following repository entry points must execute on the unchanged final source head and its actual prospective merge object:

```text
python3 scripts/check-matrix-result-reconciliation-security-v2.py
python3 scripts/check-matrix-result-reconciliation.py
python3 scripts/check-module-documentation.py
python3 scripts/check-development-documentation.py
cargo test -p matrix-entry-adapter result_reconciliation --locked -- --nocapture
cargo test -p consumer-entry-api matrix_result_lookup --locked -- --nocapture
python3 scripts/matrix_operator_postgres_regression.py
```

Qualification requires a real governed runner identity, non-empty executed steps, terminal success, retained logs and artifacts, and exact source/tree/base/merge binding. Queueing, skipping, startup failure, zero-step jobs, local author execution, or a different SHA receive no admission credit.

## Governance and production boundary

This packet does not resolve or waive live protected-main enforcement, independent approvals, unresolved review conversations, cross-repository component acceptance, representative restore, deployment/cutover/rollback, external-Agent or Matrix production outcomes, credential custody, World authority/no-dual-writer proof, sustained SLO, or final security/operations/financial/legal/commercial go/no-go.

```text
production_authorization=not_granted
merge_authorized=false
all_plan_gaps_closed=false
```
