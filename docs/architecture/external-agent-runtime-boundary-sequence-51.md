# CEX Sequence 51 external-Agent runtime boundary

Status: superseded historical architecture closure record  
Superseded by: `docs/architecture/external-agent-runtime-boundary-sequence-52.md`  
Decision authority: `decisions/adr-004-three-module-external-agent-battle-platform.md`  
Parent implementation plan: `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`  
Production authorization: `not_granted`  
Candidate identity: `sequence-51`  
production_authorization: `not_granted`

This file preserves the Sequence-51 design record for audit. It is not active authority and must not be used as current route, source, qualification, or production evidence. Sequence 52 removes the retired default `/process` route, corrects the compiled compatibility boundary, and supersedes the operational details below.

## Historical Block M — external-Agent-only runtime and truthful capability registry

### Historical objective

The default CEX runtime was defined as a control plane for externally operated Agents. Hepta admits signed Agent identities, capability declarations, work authorization, and evidence; Nakama owns realtime match state; TRNM owns finality. CEX does not become a fourth Agent cloud, model router, prompt host, or inference service.

### Historical default execution boundary

Sequence 51 intended the default workspace build to satisfy all of the following:

- local provider execution source and worker targets were isolated behind explicit compatibility controls;
- normal lifecycle routes were not to invoke local provider adapters;
- the legacy local-provider worker target had `required-features = ["legacy-local-provider-dispatch"]`;
- authoritative workflows, release qualification, deploy manifests, and example production configuration did not enable that feature;
- the legacy worker rejected beta, staging, and production before dispatch and required `CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true` in an isolated local/test process;
- historical provider commands, evidence, reconciliation artifacts, and terminal database invariants remained readable and testable for upgrade/recovery purposes without granting runtime authority.

Sequence 52 tightened this boundary by requiring the retired `/v1/executions/:id/process` route to be absent from the default router and by retaining only pure identity types plus a fail-closed compatibility function in the default provider module.

### Historical capability registry boundary

`capability-service` publishes external Agent capability declarations only. Its active source is `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON`.

Each record must:

- use a stable `cap.external-agent.*` identifier;
- declare `kind=external_agent_capability` and `provider=external-agent`;
- bind a stable `did:` or `agent:` reference and explicit protocol/version;
- remain descriptive and non-authorizing;
- pass strict field, length, count, duplicate, and control-character validation.

The service must not scan OpenClaw/Ollama/local model files, spawn model CLIs, invent demo model authority, or infer provider availability. A missing registry is non-ready in local development and is a startup failure in production-like profiles. `/health` reports `runtime_policy=external_only`, registry source, readiness, and record count rather than returning an unconditional green string.

### Historical evidence and privacy boundary

External Agent outputs enter authoritative workflows only through the versioned Hepta Agent protocol, signed identity, content commitments, explicit terminal receipts, and bounded evidence storage. Transport errors and operator surfaces use stable error classes and hashes; unrestricted prompt/result bodies must not be copied into ordinary logs, metrics, audit payloads, or database error fields.

The retained legacy adapter source is not an approved production path. Any future reactivation requires a new ADR that explicitly supersedes ADR-004, a separate trust/cost/privacy model, bounded transport implementation, migration strategy, independent review, and a new exact-tree candidate.

### Historical executable verification

The checker path remained:

```text
python3 scripts/check-external-agent-runtime-boundary.py
```

The current checker now validates Sequence 52. Any Sequence-51 run, artifact, or result is stale for the active candidate.

### Historical closure rule

Sequence 51 never established production authorization. Branch protection, runner allocation, external Agent deployment, representative-volume recovery, credential custody, downstream World/Game revision binding, V12-X1 through V12-X8, independent approval, and final human go/no-go remained separate authorities.

The only truthful production posture for this historical record is `production_authorization=not_granted`.
