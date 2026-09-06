# capability-service module contract

Status: active module documentation  
Module path: `services/capability-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Read-only discovery of approved model, Agent, tool or workflow descriptors used by Gateway and operator surfaces.

## Non-goals

- Executing models or Agents.
- Treating local OpenClaw discovery as production authority.
- Enabling demo capabilities after malformed production configuration.
- Persisting provider credentials.

## Authority and state ownership

An explicitly supplied, validated registry is authoritative for production-like reads. Local static/demo/OpenClaw discovery is development-only and never authorizes production execution.

## Interfaces

- GET /health and /metrics.
- GET /v1/capabilities and /v1/capabilities/:id.
- Validated startup configuration and configurable bind address.

## Data and persistence

- Current service state is an immutable in-memory snapshot loaded at startup.
- Production-like startup requires a non-empty valid registry with unique IDs and no demo/placeholder provider.

## Security and configuration

- Explicit runtime profile.
- Reject malformed, duplicate, demo or placeholder entries in production-like profiles.
- Disable implicit local OpenClaw discovery in production-like profiles.
- Never expose provider secrets.

## Failure and recovery

- Invalid production registry is a startup failure, not an empty or demo fallback.
- Registry rollout uses immutable revision/digest and process replacement until a governed reload protocol exists.

## Observability

- Profile, registry source, record counts, enabled/disabled counts and bounded validation failure code.
- Health does not imply downstream provider availability.

## Verification

- `cargo test --locked -p capability-service`
- `cargo clippy --locked -p capability-service --all-targets -- -D warnings`
- `services/capability-service/MODULE.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Capability IDs are immutable. Descriptor changes that alter behavior require a new version or provider_ref; consumers tolerate additive optional metadata.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.
