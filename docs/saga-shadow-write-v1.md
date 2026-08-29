# Saga Shadow Write v1

## Purpose

The durable saga schema is introduced before it becomes authoritative. Gateway can now mirror each completed synchronous invocation into deterministic shadow commands so the team can validate operation keys, payload shape, migration readiness and backlog without executing a second side effect.

Enable with:

```env
CEX_SAGA_SHADOW_WRITE=1
```

## Mode isolation

Migration `0058_add_saga_shadow_mode_v1.sql` adds:

- `execution_mode=shadow|active`;
- a claim function that selects only `execution_mode=active`;
- a mode-aware queue summary and claim index.

Shadow commands remain `pending` for reconciliation but can never be claimed by the v1 dispatcher function.

## Modeled commands

For every invocation, Gateway writes an `execution_create` shadow command.

When the request has a positive reserve amount and account id, it also writes `ledger_reserve`.

Operation keys are deterministic:

```text
invocation:<invocation-id>:ledger_reserve:initial
invocation:<invocation-id>:execution_create:initial
```

The unique `operation_key` constraint makes shadow writing idempotent.

## Payload safety

Shadow payloads include:

- invocation and trace id;
- account/capability/provider identifiers;
- legacy reserve amount formatted to six decimal places;
- observed authoritative status/result flags.

Prompt text is not duplicated. Only its byte length is recorded until a reviewed request-fingerprint contract is available.

## Failure semantics

Shadow persistence is observational:

- failure does not change the synchronous invocation response;
- production operators receive an explicit structured stderr line;
- no shadow command is dispatched;
- the next slice must add counters, backlog-age metrics and a reconciliation endpoint.

This non-blocking behavior is deliberate only for shadow mode. Active saga commands will be part of the authoritative transaction and cannot be best-effort.

## Reconciliation queries

```sql
select *
from cex_saga_queue_summary_v1
where execution_mode = 'shadow';
```

```sql
select operation_key, command_kind, payload, created_at
from cex_saga_commands_v1
where execution_mode = 'shadow'
order by created_at, command_id;
```

Compare observed payload fields with `invocations`, `executions` and ledger entries. A mismatch does not trigger automated repair in v1.

## Promotion gates

Before any command becomes active:

1. migrations 0057/0058 pass fresh and upgrade tests;
2. shadow writes show no duplicate/missing operation keys;
3. payloads contain enough information to reconstruct the intended action;
4. backlog and mismatch metrics are live;
5. active worker requires explicit command-kind allow-list;
6. crash-after-side-effect-before-receipt tests pass;
7. operator dead-letter/reconcile runbook is approved.
