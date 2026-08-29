# Saga Observability and Reconciliation v1

## Endpoints

Gateway now exposes two redacted operator surfaces:

- `GET /v1/saga/summary`
- `GET /metrics/saga`

They report only counts, timestamps, modes, command kinds and reconciliation dimensions. Command payloads, prompts, tokens and provider output are never returned.

The endpoints follow the current internal `/v1/info` posture. Before external exposure, place them behind the operator edge or add a dedicated read-only workload/operator scope.

## Shadow-write runtime counters

Gateway records:

- attempts;
- successes;
- failures.

A failure remains non-blocking only because v1 is shadow mode. Active saga command persistence will be authoritative and must fail the containing business transaction when it cannot be recorded.

## Queue dimensions

`cex_saga_queue_summary_v1` is rendered by:

- execution mode: `shadow|active`;
- command kind;
- status;
- command count;
- retry-budget exhausted count;
- oldest available time;
- oldest lease expiry.

## Reconciliation dimensions

The gateway query reports:

- shadow command count;
- shadow commands whose invocation no longer exists;
- observed invocation status mismatch;
- observed execution-id mismatch;
- reserve observed as successful but no ledger entry exists for the modeled idempotency key;
- invocations created after shadow rollout that have no execution-create shadow command;
- age of the oldest shadow command.

These are detection signals, not automated repair.

## Prometheus metrics

- `cex_gateway_saga_runtime_up`
- `cex_gateway_saga_shadow_write_total{result=...}`
- `cex_gateway_saga_queue_commands{execution_mode,command_kind,status}`
- `cex_gateway_saga_queue_retry_budget_exhausted{...}`
- `cex_gateway_saga_reconciliation_mismatches{kind=...}`
- `cex_gateway_saga_oldest_shadow_age_seconds`

## Alert recommendations

During shadow rollout:

- any `saga_runtime_up=0` for 5 minutes: high;
- `saga_shadow_write_total{result="failure"}` increase: high;
- any orphan/status/execution-id/reserve mismatch: high;
- missing execution-shadow coverage above zero: high;
- oldest shadow age is informational because shadow commands intentionally remain pending.

Before active rollout, replace absolute oldest age with kind/status-specific SLO thresholds.

## Remaining work

- authenticated read scope;
- query-duration and timeout controls;
- mismatch drill-down with pagination and redaction;
- active command dispatcher metrics;
- dead-letter acknowledgement;
- automated reconciliation commands;
- dashboard and formal alert rules.
