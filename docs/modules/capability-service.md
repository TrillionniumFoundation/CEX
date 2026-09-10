# capability-service module contract

Status: active module contract  
Workspace member: `services/capability-service`  
Package: `capability-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `capability-registry`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Publishes a bounded read-only registry of external Agent capability declarations used for discovery and compatibility checks in the Hepta control plane.

**Non-goals.** It does not discover local models, execute OpenClaw/Ollama or any tool, host Agents, route inference, own provider credentials, determine entitlement, mutate Ledger state, or authorize a Paper Raid participant.

## Authority and owned state

The service is authoritative only for the validated registry snapshot currently loaded from `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON` and for the stable serialization of that read surface. Capabilities remain descriptive declarations; they do not prove availability, scientific quality, ownership, payment entitlement or execution authority.

The service owns no durable business state. External Agent registration, key ownership, enrollment and signed submissions are owned by Hepta Research League under `hepta_agent_protocol_v1`. A cache, HTTP success or self-declared capability never transfers that authority.

## Source layout and entry points

- `lib.rs`: strict registry parsing, validation, readiness, metrics and read routes.
- `main.rs`: fail-closed startup and configurable listener binding.
- `tests/http_flow.rs`: HTTP, readiness, ordering and external-only contract regression.

Catalog-bound entry points:

- `services/capability-service/src/main.rs`
- `services/capability-service/src/lib.rs`
- `services/capability-service/tests/http_flow.rs`

Any new source, public route, persistence owner or process target must update the catalog and this document in the same commit.

## Interfaces and contracts

The service exposes `/health`, `/metrics`, `/v1/capabilities` and `/v1/capabilities/:id`. `/health` returns structured readiness with `runtime_policy=external_only`, registry source, record count and `production_authorization=not_granted`; an empty development registry returns `503` rather than unconditional success.

Each registry record uses `cap.external-agent.*`, `kind=external_agent_capability`, `provider=external-agent`, a stable `did:` or `agent:` reference, explicit version and bounded display metadata. Unknown fields, duplicate IDs, empty/control-character values and over-limit input fail closed.

## Persistence, concurrency, and recovery

The current service loads one immutable in-memory snapshot at startup and exposes no mutation route. Concurrent readers share that snapshot; there is no process-local write authority, background model discovery or dynamic CLI refresh.

A production-like process must receive a non-empty validated registry. Replacement requires a new process or a future authenticated, versioned and audited reload contract. Failed replacement must leave the previous deployment artifact untouched and must never fabricate readiness.

## Configuration and secrets

`CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON` is the sole capability source. `CEX_RUNTIME_PROFILE` or its reviewed compatibility alias selects runtime posture; `CAPABILITY_BIND_ADDR` selects the listener and defaults to loopback.

Registry bytes are limited to 1 MiB, record count to 1,024, and individual text fields have explicit limits. The registry contains public capability metadata only and must not contain model/provider secrets, Agent private keys, bearer tokens or unrestricted prompts/results. Missing registry is allowed only as non-ready local development state and is rejected in production-like profiles.

## Security and trust boundaries

Local model discovery is forbidden. For avoidance of doubt, local model discovery is forbidden in every runtime profile. The implementation must not inspect `~/.openclaw`, execute a model CLI, contact Ollama, infer an allowlist from local state, or create demo model authority. External declarations are untrusted input until strict validation succeeds and still grant no execution authority.

Responses and telemetry expose only bounded public metadata and aggregate counts. High-cardinality identities, credentials, private research content and unrestricted descriptions are not valid log or metric fields. Authentication may be added for deployment privacy, but it cannot turn the registry into Agent ownership authority.

## Verification

Required commands:

```text
cargo test -p capability-service --all-targets
cargo clippy -p capability-service --all-targets -- -D warnings
python3 scripts/check-external-agent-runtime-boundary.py
```

Required behavioral focus:

- Valid external Agent declarations load deterministically and sort by stable capability ID.
- Unknown fields, local-model/provider authority, duplicates, excessive bytes/counts and malformed references fail closed.
- Missing local registry reports non-ready; missing production-like registry rejects startup.
- Health and metrics distinguish process liveness from validated-registry readiness.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy as a supporting read surface with an explicitly supplied, reviewed registry snapshot. Bind privately unless a separate exposure decision exists, monitor readiness and record count, and roll back by restoring the previous exact image plus registry digest.

Operators record image identity, registry digest/source, runtime profile, listener scope, readiness, rollback boundary and owner escalation. Repository CI cannot prove external Agent availability, key custody, production topology or final human authorization.

## Compatibility and change protocol

Capability IDs and versions are stable protocol identifiers. Additions are explicit; removal requires consumer inventory and retirement evidence. Reintroducing model discovery, provider routing or execution is a breaking architectural change and requires an ADR that supersedes ADR-004.

Changes to validation, fields, routes, configuration or ownership require this contract, the module catalog, external Agent protocol fixtures, executable tests, the architecture-boundary checker and a new shared candidate trigger. No module document may declare repository closure or production authorization.
