# CEX Sequence 51 external-Agent runtime boundary

Status: active architecture closure contract  
Decision authority: `decisions/adr-004-three-module-external-agent-battle-platform.md`  
Parent implementation plan: `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`  
Production authorization: `not_granted`  
Candidate identity: `sequence-51`  
production_authorization: `not_granted`

This contract closes the repository-side contradiction between the accepted three-domain architecture and legacy local provider execution. It does not erase historical provider records or migration evidence, and it cannot certify real external Agent operation, production infrastructure, independent review, or a human release decision.

## Block M — external-Agent-only runtime and truthful capability registry

### Objective

The default CEX runtime is a control plane for externally operated Agents. Hepta admits signed Agent identities, capability declarations, work authorization and evidence; Nakama owns realtime match state; TRNM owns finality. CEX does not become a fourth Agent cloud, model router, prompt host or inference service.

### Default execution boundary

The default workspace build must satisfy all of the following:

- `services/execution-service/src/lib.rs` does not compile `provider_dispatch` or `providers` unless the explicit `legacy-local-provider-dispatch` feature is selected;
- normal `/v1/executions/:id/start` and `/process` routes call the execution lifecycle API and never local provider adapters;
- the legacy local-provider worker target has `required-features = ["legacy-local-provider-dispatch"]`;
- authoritative workflows, release qualification, deploy manifests and example production configuration never enable that feature;
- the legacy worker rejects beta, staging and production before dispatch and requires `CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true` in an isolated local/test process;
- historical provider commands, evidence, reconciliation artifacts and terminal database invariants remain readable and testable for upgrade/recovery purposes without granting runtime authority.

### Capability registry boundary

`capability-service` publishes external Agent capability declarations only. Its active source is `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON`.

Each record must:

- use a stable `cap.external-agent.*` identifier;
- declare `kind=external_agent_capability` and `provider=external-agent`;
- bind a stable `did:` or `agent:` reference and explicit protocol/version;
- remain descriptive and non-authorizing;
- pass strict field, length, count, duplicate and control-character validation.

The service must not scan OpenClaw/Ollama/local model files, spawn model CLIs, invent demo model authority, or infer provider availability. A missing registry is non-ready in local development and is a startup failure in production-like profiles. `/health` reports `runtime_policy=external_only`, registry source, readiness and record count rather than returning an unconditional green string.

### Evidence and privacy boundary

External Agent outputs enter authoritative workflows only through the versioned Hepta Agent protocol, signed identity, content commitments, explicit terminal receipts and bounded evidence storage. Transport errors and operator surfaces use stable error classes and hashes; unrestricted prompt/result bodies must not be copied into ordinary logs, metrics, audit payloads or database error fields.

The retained legacy adapter source is not an approved production path. Any future reactivation requires a new ADR that explicitly supersedes ADR-004, a separate trust/cost/privacy model, bounded transport implementation, migration strategy, independent review and a new exact-tree candidate.

### Executable verification

The canonical repository command is:

```text
python3 scripts/check-external-agent-runtime-boundary.py
```

`python3 scripts/check-development-docs.py` invokes this check. The checker verifies decision markers, feature isolation, default route ownership, production rejection, capability source behavior, module contracts, candidate sequence and absence of legacy-feature activation from CI/deployment surfaces.

Required Rust verification remains:

```text
cargo test -p capability-service --all-targets
cargo clippy -p capability-service --all-targets -- -D warnings
cargo test -p execution-service --all-targets
cargo clippy -p execution-service --all-targets -- -D warnings
```

These commands intentionally exercise the default external-only build. Compiling the legacy feature is not repository qualification and cannot be used as production evidence.

### Closure rule

Block M is repository-closed only when the source checker, module documentation, default Rust build and exact-SHA hosted gates succeed on one unchanged candidate. Branch protection, runner allocation, external Agent deployment, representative-volume recovery, credential custody, downstream World/Game revision binding, V12-X1 through V12-X8, independent approval and final human go/no-go remain separate authorities.

The only truthful production posture before those records exist is `production_authorization=not_granted`.
