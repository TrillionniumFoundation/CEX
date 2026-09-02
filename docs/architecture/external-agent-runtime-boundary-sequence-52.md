# Sequence 52 external-Agent runtime boundary closure

Status: active architecture closure contract  
Candidate sequence: `52`  
Runtime policy: `external_only`  
Supersedes: `docs/architecture/external-agent-runtime-boundary-sequence-51.md`  
Production authorization: `not_granted`

## Authority and scope

This contract closes the repository-actionable architecture gaps under accepted ADR-004. It governs the default CEX workspace build, its HTTP routing surface, Capability registry semantics, compatibility-only provider code, CI activation surfaces, and machine-readable traceability. The accepted decision remains that Hepta, Nakama, and TRNM are the three top-level domains and that participating Agent runtimes, model selection, provider credentials, prompts, and inference execution remain external.

The parent v12 plan remains authoritative for exact Ledger, Gateway, Execution lifecycle, Audit, migration, recovery, and evidence controls. Any provider-specific Ollama/OpenClaw execution wording retained in that historical plan is superseded within the ADR-004 scope by this contract and the active implementation addendum. It may describe historical migration-0088 rows and negative compatibility verification only; it cannot authorize a current runtime path.

## Block M — final repository architecture closure

Block M is complete only when every condition below is true on one unchanged repository candidate:

1. The default `execution-service` build does not compile a local provider worker target.
2. The default Execution router does not expose `/v1/executions/:id/process` or any equivalent in-process model/provider execution endpoint.
3. `services/execution-service/src/providers.rs` contains identity helpers and a fail-closed compatibility function only. It performs no HTTP inference call, subprocess execution, filesystem model discovery, or prompt/body logging.
4. The compatibility worker is available only behind the non-default `legacy-local-provider-dispatch` feature and rejects every production-like profile before work begins.
5. Capability Service consumes bounded external Agent capability declarations from `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON`; local model discovery, executable paths, commands, secrets, provider credentials, and implicit production defaults are forbidden.
6. Authoritative workflows, deployment manifests, configuration files, and example environments do not activate the legacy feature or its opt-in flag.
7. World, Game/Nakama, Chain, Matrix, content-addressed storage, and external providers/Agents remain explicit external authorities rather than implicit Cargo workspace members.
8. The architecture decision, closure contract, traceability file, module contracts, shared candidate trigger, and checker all bind candidate sequence 52.
9. The bounded self-hosted runner probes remain `workflow_dispatch`-only, have no repository-token permissions, perform no checkout, and require `refs/heads/main`. Their presence is not runner-execution evidence.
10. Every checker and document continues to emit or state `production_authorization: not_granted`; repository validation cannot issue the final human go/no-go.

## Execution boundary

The normal Execution API owns durable lifecycle state, claims, lease renewal, reconciliation, dead-letter acknowledgement, and signed external evidence ingestion. It does not own an Agent runtime. The retired `/process` route is absent from the default router, so a request to that path returns `404 Not Found` rather than attempting local execution or manufacturing an external-Agent result.

`dispatch_via_provider` is retained solely as a bounded fail-closed compatibility symbol for historical code and negative tests. For every syntactically valid provider target it returns `external_agent_runtime_required` without reading the prompt, connecting to a provider, spawning a child process, or echoing target/prompt material. Historical provider-command tables and migration-0088 terminal-evidence constraints remain auditable and may be reconciled, but they confer no runtime authority.

The feature-gated `execution-provider-dispatch-worker` is not a supported production runtime. It exists only so old rows and rollback evidence remain inspectable. Production-like startup rejection is mandatory and no workflow or deployment surface may enable the feature.

## Capability boundary

Capability Service is a validated projection of declarations issued by external Agent operators. It is not a model catalog, model router, tool executor, secret registry, or availability authority. Registry parsing is bounded by bytes and record count, rejects duplicate identity/revision collisions, disallows executable/model/path/URL/command/environment/secret metadata, and fails closed in production-like profiles when required authority is absent or malformed.

A capability record indicates discovery and protocol compatibility only. Execution eligibility, assignment acceptance, key possession, evidence validity, consent, settlement, and finality remain governed by their owning protocols and services.

## Active-plan correction

Within ADR-004 scope, the following parent-plan concepts are historical compatibility language, not active runtime requirements:

- successful local Ollama `/api/generate` execution;
- OpenClaw CLI model invocation from CEX;
- a provider/model terminal envelope generated by a CEX-hosted inference adapter;
- filesystem-discovered model catalogs;
- provider credentials or Agent private keys entering CEX.

The active replacement is signed, versioned external-Agent assignment and evidence exchange under `docs/hepta-agent-protocol-v1.md`. Migration-0088 provider terminal-evidence checks remain regression requirements for already persisted historical records and for rejecting fabricated or mismatched evidence.

## Runner and evidence boundary

The desktop, ROG, Pocket4, and Mac probe workflows are connectivity definitions only. They intentionally use manual dispatch, no checkout, and `permissions: {}`. A file existing in `.github/workflows/`, a queued run, `runner_id=0`, an empty runner identity, `steps=[]`, a missing log, or an artifact from another SHA provides no qualification credit.

Repository qualification still requires non-empty execution of every required hosted context on the exact candidate SHA, retained artifacts, a generated immutable candidate manifest, protected-main governance, and independent review. These requirements are external to source-tree self-certification.

## Verification

Repository checks:

```text
python3 scripts/check-external-agent-runtime-boundary.py
python3 scripts/check-development-docs.py
python3 scripts/check-module-documentation.py
cargo test -p execution-service --test external_agent_boundary
cargo test -p capability-service --test http_flow
cargo check -p execution-service --all-targets --no-default-features
cargo clippy -p execution-service --all-targets --no-default-features -- -D warnings
```

Required negative cases include the retired `/process` route returning 404, attempted Ollama/OpenClaw dispatch returning a stable external-Agent-required error without prompt echo, malformed capability declarations failing closed, and activation scans rejecting any workflow/deploy/config enablement of the legacy feature.

## Independent blockers preserved

Sequence 52 does not self-certify or close:

- runner allocation and non-empty exact-SHA hosted execution;
- enforceable protected-main rules and no administrator bypass;
- independent non-final-pusher approval;
- downstream World/Game immutable revision binding and zero unexplained receipt divergence;
- representative-volume recovery, cutover, rollback, and sustained soak;
- real external Agent/provider reconciliation evidence;
- credential custody and rotation review;
- independent security, operations, and financial review;
- legal/commercial/provider approval;
- final human go/no-go.

## Change protocol

Any reintroduction of a process route, local inference client, executable provider adapter, local model discovery, implicit capability default, legacy feature activation, new external authority, or changed Agent protocol requires a new accepted or superseding ADR, updated module/catalog/protocol documentation, executable negative tests, machine traceability, hosted gate wiring, and a new shared candidate sequence. No such change may inherit qualification evidence from sequence 52.
