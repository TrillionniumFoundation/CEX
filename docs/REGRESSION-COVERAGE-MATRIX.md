# Regression Coverage Matrix

This file tracks how the old PowerShell regression scripts map onto the newer Rust-based test layers.

Status meanings:

- **Covered** = Rust tests now exercise the same primary runtime behavior; PowerShell script is a retirement candidate.
- **Partial** = Rust tests cover the main HTTP/runtime behavior, but the PowerShell script still adds unique value.
- **Legacy-only** = no equivalent Rust coverage yet; keep the PowerShell script.

## Matrix

| Legacy script | Primary scenario | Current Rust coverage | Status | Notes |
|---|---|---|---|---|
| `scripts/smoke-test-local-runtime.ps1` | Basic detached runtime smoke: create account, create invocation, verify reserve landed | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_happy_path_matches_api_regression` | **Covered** | Rust black-box check is a strict superset because it also verifies execution + audit state. |
| `scripts/api-regression-check.ps1` | Happy-path end-to-end API flow across gateway/execution/ledger/audit | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_happy_path_matches_api_regression` | **Covered** | Same primary runtime path now lives in Rust black-box form, and both the Rust probe plus the retained PowerShell compatibility script now check that gateway invocation reads expose the cached/live execution snapshot. |
| `scripts/approval-regression-check.ps1` | Approval-required invocation + approve path | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_approval_flow_matches_regression` | **Covered** | Rust black-box validates the approval hold + approve transition over live HTTP, and the retained PowerShell compatibility script now also checks invocation execution-snapshot parity with execution-service reads. |
| `scripts/reject-refund-regression-check.ps1` | Approval rejection triggers refund and survives restart, including approval-row DB assertions | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_reject_refund_flow_matches_regression` plus `gateway-service/tests/runtime_approval_probe.rs::runtime_approval_probe_reject_refund_matches_db_state` | **Covered** | HTTP/runtime-visible behavior is covered by black-box tests; DB-level approval assertions are covered by the approval probe target. |
| `scripts/settlement-regression-check.ps1` | Settlement success consumes reserve; settlement failure refunds reserve; both survive restart | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_settlement_flows_match_regression` | **Covered** | Main runtime behavior now covered in Rust black-box form. |
| `scripts/lifecycle-regression-check.ps1` | Dispatch/start/cancel and dispatch/start/timeout lifecycle behavior with restart persistence | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_lifecycle_cancel_and_timeout_match_regression` | **Covered** | Rust black-box also verifies invocation mirror states during dispatching/running; the retained PowerShell script now also checks invocation execution-snapshot parity across dispatch/start/final/restart reads. |
| `scripts/state-machine-regression-check.ps1` | Replay semantics and invalid-transition `409` conflicts across success/cancel/timeout branches | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_state_machine_replay_and_conflict_match_regression` | **Covered** | Rust black-box now locks replay/conflict semantics plus restart persistence. |
| `scripts/workflow-persistence-regression-check.ps1` | Auto-path + approved-path persistence after restart, plus approval DB row inspection | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_workflow_persistence_matches_regression` plus `gateway-service/tests/runtime_approval_probe.rs::runtime_approval_probe_workflow_persistence_matches_db_state` | **Covered** | Runtime-facing persistence plus DB-level approval-row assertions are now both covered in Rust, and the retained PowerShell script now also checks invocation execution-snapshot parity after restart. |
| `scripts/audit-persistence-regression-check.ps1` | Direct audit-event persistence across detached runtime restart | `gateway-service/tests/runtime_blackbox.rs::blackbox_runtime_audit_persistence_matches_regression` | **Covered** | Rust black-box now creates an audit event, restarts runtime, and verifies the event survives via HTTP only. |

## Retirement candidates now

All scripts listed above are now practical retirement candidates from a *coverage* perspective. Their original names under `scripts\` now remain mainly as compatibility shims forwarding to `scripts\legacy\`.

A reasonable retirement order is:

1. `scripts/smoke-test-local-runtime.ps1`
2. `scripts/api-regression-check.ps1`
3. `scripts/approval-regression-check.ps1`
4. `scripts/settlement-regression-check.ps1`
5. `scripts/lifecycle-regression-check.ps1`
6. `scripts/state-machine-regression-check.ps1`
7. `scripts/audit-persistence-regression-check.ps1`
8. `scripts/reject-refund-regression-check.ps1`
9. `scripts/workflow-persistence-regression-check.ps1`

## Optional manual probes

Even when covered, PowerShell scripts may still be convenient for:

- one-off manual debugging from a shell
- demonstrating a single scenario step-by-step
- quick operator checks without compiling tests

But they are no longer required to preserve regression coverage.

## Practical policy

### Preferred day-to-day gate

Use:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
```

or CI-safe mode:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1 -ServiceLocalOnly
```

### When to still run legacy PowerShell scripts

Run the legacy PowerShell scripts only when you specifically want:

- a manual shell-first walkthrough of a scenario
- a targeted operator/debug probe outside the Rust test flow

These compatibility/manual probes now also honor the scoped auth model when they hit protected surfaces:

- gateway invocation create/read probes send `x-api-key` via the local gateway API-key helper (defaulting to `local-dev-key` unless overridden)
- audit trace reads prefer `AUDIT_ADMIN_TOKENS_JSON`, then shared identity env, then legacy single-token env vars, then the dev fallback
- direct execution admin/operator probes prefer `EXECUTION_ADMIN_TOKENS_JSON`, then shared identity env, then legacy single-token env vars, then the dev fallback
- direct ledger account/credit probes prefer `LEDGER_ADMIN_TOKENS_JSON`, then shared identity env, then legacy single-token env vars, then the dev fallback

So if you rehearse split-admin locally, the legacy scripts and runtime approval probes should follow the same scoped auth path instead of assuming one global `local-dev-admin-token`, anonymous trace access, or unauthenticated gateway/ledger reads.

