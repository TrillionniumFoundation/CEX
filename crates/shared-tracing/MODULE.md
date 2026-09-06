# shared-tracing module contract

Status: active module documentation  
Module path: `crates/shared-tracing`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Common tracing initialization and cross-service correlation conventions.

## Non-goals

- Persisting audit truth.
- Exporting credentials or research content.
- Defining business status from log presence.

## Authority and state ownership

Tracing is diagnostic only. Audit, Ledger and domain databases remain authoritative; a log line cannot prove a committed state transition.

## Interfaces

- Rust tracing subscriber initialization.
- Trace and span field conventions consumed by service binaries.

## Data and persistence

- No authoritative persistence.
- Recommended bounded fields include trace_id, operation_id, service, route, outcome class and latency bucket.

## Security and configuration

- Redact credentials, signatures, private keys, raw prompts, unrestricted paper bodies and personal data.
- Do not use tenant-controlled text as metric labels.

## Failure and recovery

- Tracing backend failure must not corrupt business state. Production policy must explicitly choose fail-open diagnostics versus fail-closed audit delivery.

## Observability

- Sampling, exporter health and dropped-event counters must be externally visible.
- Trace propagation must preserve an existing trusted trace identity rather than accept arbitrary provenance as authority.

## Verification

- `cargo test --locked -p shared-tracing`
- `cargo clippy --locked -p shared-tracing --all-targets -- -D warnings`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Field names used by dashboards and alerts require a deprecation period or dual emission. Trace format changes do not change Audit evidence semantics.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.
