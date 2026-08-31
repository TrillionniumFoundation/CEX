# Gateway Exact Registration and Reserve v1

- Status: P0 implementation candidate
- Migration: `0073_add_gateway_exact_reserve_commands.sql`
- Ingress binary: `gateway-exact-reserve-api`
- Worker binary: `gateway-exact-reserve-worker`
- Canonical Ledger target: `POST /v2/ledger/effects`
- Contract authority: `cex_invocation_ledger_contracts_v1`
- Default rollout posture: shadow; active execution disabled
- Canonical legacy-reserve posture: fail-closed; non-production break-glass only

Machine-readable rollout posture: `legacy_reserve_fail_closed=true`.

## 1. Objective

The legacy Gateway request model carries value-bearing major-unit numbers. Those values cannot be
rounded, truncated or cast into canonical money without changing the historical contract. P0-N6
therefore introduces a separate exact-money attachment for an existing Invocation:

```text
existing non-monetary Invocation skeleton
  + account ID
  + organization ID
  + explicit trace ID
  + currency unit and scale
  + string minor units
        ↓ one source transaction
0066 immutable Invocation Ledger contract
  + 0073 immutable reserve command
  + Audit v2 intent
commit
        ↓ separate claim transaction
active reserve command lease
commit
        ↓ no business SQL transaction open
POST /v2/ledger/effects
        ↓ separate outcome transaction
verified receipt / retry / reconcile_required / dead_letter
commit
```

This expand slice does not reinterpret a nonzero legacy `requested_amount`,
`requested_reserve_amount` or `reserve_amount`. Registration fails closed unless legacy monetary
intent is zero or absent.

The canonical `POST /v1/invocations` route now rejects a supplied legacy `reserve_amount` before
authentication, persistence or an upstream call. Clients must first create a non-monetary
Invocation skeleton (omit `reserve_amount`) and then call the exact-reserve ingress below. The
serialized skeleton omits the absent compatibility key, which is required by the 0066/0073
dual-money guard. `CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS` is an emergency non-production rollback
switch and is ignored for production-like profiles.

## 2. Exact ingress contract

`POST /v2/invocations/:invocation_id/exact-reserve` accepts only:

```json
{
  "account_id": "uuid",
  "org_id": "uuid",
  "trace_id": "uuid",
  "currency_unit": "credit",
  "currency_scale": 6,
  "amount_minor": "1000000",
  "execution_mode": "shadow",
  "max_attempts": 5
}
```

The request type uses `deny_unknown_fields`. Major-unit aliases, floating-point values, signs,
decimals, whitespace and noncanonical leading zeros are rejected before database work.
`amount_minor` is a positive string that must fit signed 64-bit storage. Currency unit must already
be lowercase and match `^[a-z][a-z0-9._-]{0,31}$`; scale is 0–6.

The route is an internal expand-phase API. It requires a bearer token whose authenticated source is
bound to the fixed `gateway-exact-ingress` principal. An omitted `x-cex-service-id` header resolves
to that principal; a supplied header must match it exactly or authentication fails. A caller that
possesses the ingress token therefore cannot relabel the immutable contract or Audit actor as
another service. Production-like startup rejects short credentials and known development
placeholders. Request bodies are bounded. Active command creation requires the explicit
`CEX_GATEWAY_EXACT_RESERVE_ALLOW_ACTIVE=true` rollout switch.

The API and worker resolve `CEX_RUNTIME_PROFILE`/`APP_ENV` through the shared runtime-profile
authority. They therefore accept the same canonical aliases, including the production-like
`trnm-economy` lane, reject conflicting sources, and honor the same explicit implicit-development
escape hatch. Private profile enums in either money binary are forbidden.

## 3. Atomic source transaction

`cex_prepare_gateway_exact_reserve_v1` obtains an Invocation-scoped advisory transaction lock and
then:

1. verifies Invocation/organization binding;
2. verifies legacy monetary intent is zero or absent;
3. rejects dual exact/legacy money keys in the stored request payload;
4. calls `cex_register_invocation_ledger_contract_v1`;
5. derives the canonical reserve request through `cex_invocation_ledger_effect_request_v1`;
6. inserts or exactly replays one deterministic reserve command;
7. inserts the authenticated Audit v2 intent.

All seven operations commit or roll back together. A response loss after commit can be retried with
the same immutable input. Identical content returns the original contract and command; different
content under the same Invocation identity raises an immutable collision.

## 4. Durable reserve command

`cex_gateway_ledger_reserve_commands_v1` binds:

- deterministic command ID;
- Invocation and organization IDs;
- deterministic reserve operation ID;
- 0066 contract hash;
- exact Ledger v2 request snapshot;
- canonical request fingerprint;
- source principal and schema version;
- shadow/active mode;
- bounded claim, attempt, receipt and operator-recovery evidence.

A `BEFORE INSERT` trigger re-resolves the Invocation and 0066 contract. Direct rows that disagree
on tenant, operation identity, request projection, fingerprint or contract hash are rejected.
Immutable fields cannot change and commands cannot be deleted. Transition evidence is append-only.

## 5. Worker boundary

The worker calls `cex_claim_gateway_exact_reserves_v1` through an autocommit query. The claim
transaction commits before any HTTP request; in other words, the claim transaction commits before
the remote Ledger side effect begins. It then sends the stored request to
`POST /v2/ledger/effects` with redirects disabled, a bounded timeout and a bounded response body.
The final state is persisted through a separate autocommit function.

The serial batch configuration must satisfy:

```text
batch × (HTTP timeout + database margins) + safety margin < lease
```

Defaults are batch 2, request timeout 20 seconds and lease 90 seconds.

## 6. Outcome semantics

| Result | Durable state | Meaning |
|---|---|---|
| Verified first apply or exact replay | `succeeded` | Receipt and 0066 reserved evidence match |
| Connect failure or explicit retryable HTTP response | `retry_wait` | Same operation may be replayed exactly |
| Ambiguous transport, redirect, oversized body or invalid success receipt | `reconcile_required` | Remote result cannot be asserted |
| Permanent validation/auth/tenant/collision rejection | `dead_letter` | Operator review required |

A retryable result on the final automatic attempt becomes `reconcile_required`, not assumed
failure. An expired final claim lease follows the same rule.

## 7. Verified receipt

Before success, PostgreSQL verifies:

- account and trace IDs;
- operation ID and operation kind;
- scoped idempotency key;
- amount minor units;
- currency unit and scale;
- non-nil Ledger entry ID;
- 0066 contract hash;
- 0066 status is `reserved`;
- 0066 last operation and entry IDs match the receipt.

The receipt receives a SHA-256 digest. Exact completion replay must provide the same receipt and
replayed flag.

## 8. Operator recovery

`cex_acknowledge_gateway_exact_reserve_v1` records actor, reason and time for
`reconcile_required`/`dead_letter` rows. `cex_requeue_gateway_exact_reserve_v1` requires that
acknowledgement, adds a bounded attempt budget and clears the live acknowledgement. A later incident
therefore requires a fresh acknowledgement. Exact requeue replay is idempotent; different content
collides.

## 9. Shadow qualification and cutover

1. Deploy migration 0073, binaries and metrics with active creation disabled.
2. Verify the canonical legacy-reserve rejection and create representative non-monetary skeletons;
   register exact contracts in `shadow` and compare the canonical request with the intended
   Invocation.
3. Prove collision, tenant, token rotation, response loss and lease-expiry matrices.
4. Promote a bounded canary cohort with `cex_promote_gateway_exact_reserve_v1`.
5. Observe queue age, unknown outcomes and contract/receipt reconciliation.
6. The matching legacy reserve side effect is already blocked on the production request path. Keep
   it disabled; only an explicitly reviewed non-production break-glass may invoke the compatibility
   implementation while the exact-field caller cutover is qualified.

Rollback stops both binaries and leaves command/receipt evidence intact. The migration is an
append-only expand migration; rollback never drops evidence or fabricates legacy money.

## 10. Known boundary after P0-N6

The existing Gateway lifecycle creates the non-monetary Invocation skeleton through its
legacy-compatible route. The canonical route now fails closed when a legacy floating-point reserve
is supplied rather than silently calling `/v1/ledger`. A later reviewed caller cutover must add an
exact reserve field to the public schema and make the exact ingress the sole monetary source; until
then the explicit two-step exact ingress is the only supported value-bearing path.

The repository remains not production-ready until hosted checks execute on the exact commit/tree,
least-privilege roles are applied, restore/rollback evidence is bound and the remaining P0 money
work is closed.
