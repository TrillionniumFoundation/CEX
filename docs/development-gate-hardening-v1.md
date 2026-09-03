# Development gate hardening and residual work

Status: implemented changes awaiting exact-head hosted qualification  
Parent plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`, Blocks H and K in the active implementation addendum  
Architecture authority: `decisions/adr-004-three-module-external-agent-battle-platform.md`  
Production authorization: `not_granted`

## Scope and candidate identity

This repair preserves the accepted external-only architecture and migration head
`0088_enforce_provider_terminal_evidence_binding.sql`. It does not re-enable model
execution, a retired process route, compatibility feature, or production mode.

The Sequence-52 architecture contract is unchanged. The shared trigger's
`sequence-52-freeze-2` identifies a new candidate generation, NOT qualification.
The commit and tree are recorded in the PR and generated evidence, not written
back into this source file. Freeze-1 and pre-repair evidence cannot qualify the
new commit. Any subsequent edit invalidates its exact-head evidence again.

## Implemented repairs and acceptance tests

| ID | Defect | Required behavior | Verification |
|---|---|---|---|
| DG-1 | Child/core failure with empty diagnostics could be normalized to success | Exit code, status, schema, diagnostic shape, authority and denial flag are checked independently; all must be valid | `GateResultTests` |
| DG-2 | Child output was unbounded and stderr could impersonate a stdout result | Separate disk-spooled streams; combined 1 MiB acceptance limit; 120-second deadline; reject malformed UTF-8, duplicate JSON keys, non-finite literals and non-object roots without echoing raw bytes | `ChildTransportTests` |
| DG-3 | Empty required sections and example/comment headings could satisfy documentation checks | Exactly one real level-two heading and non-empty visible body per required section; retain useful fenced examples but do not count example headings | `ModuleSectionTests` |
| DG-4 | Six real library entrypoints were omitted from the module catalog | Ledger, TRNM economy, Gateway, Execution, Audit and Paper Raid BFF each document and catalog their `src/lib.rs` | `CatalogRepairTests` |
| DG-5 | Manual probe input was expanded directly into shell source | Transfer `inputs.reason` through an environment variable; quote/limit its diagnostic rendering; keep manual-main-only/no-checkout/no-permission probes | `DeploymentInputTests` |
| DG-6 | Example registry lost JSON quoting under shell sourcing and its comment matched a forbidden activation marker | Preserve exact JSON using shell quotes; omit executable opt-in examples; retain the strict architecture scan | `DeploymentInputTests` |

Canonical regression command:

```text
python3 scripts/test-development-gates.py
```

The command is wired before documentation validation in the `repository-integrity`
job of `rust-service-gate`. No previous check or test is removed or weakened.
The child time/output bounds are checker-process containment, not an OS sandbox
or a substitute for hosted runner isolation and resource quotas.

## Evidence and limits

Local unit fixtures prove checker behavior, not runtime qualification, issuer
independence, production readiness, or full-workspace test success. The repair
changes no Rust or SQL implementation. Linux/Windows Cargo suites, PostgreSQL
migration/recovery/settlement tests, the whole documentation catalog gate and the
aggregate candidate manifest still require execution on the final repository tree.

The publication must list the actual test command and outcome separately from
hosted results. A missing tool, 403 integration response, queued/unallocated job,
missing artifact or unread protection state is NOT success.

## Residual repository work: do not relabel as external approval

The canonical component-status document still lists these implementation and
validation gaps. This change does not claim to close them:

| Workstream | Required implementation/acceptance |
|---|---|
| Consumer Entry | Bounded-context decomposition, route/schema inventory and World/Game ownership enforcement |
| Matrix adapter/poller | Durable cursor, compare-and-set/fencing, restart and multi-instance regression; poison-event behavior |
| Matrix relay | Durable delivery/outbox/dead-letter behavior and response-loss recovery |
| Module/API coverage | Complete code-derived API/configuration/data inventory and Cargo target coverage beyond explicitly listed/conventional entrypoints |
| Capability | Real registry distribution/issuer governance and exact-head Rust regression |
| Execution coverage | Review and restore/port applicable lifecycle/authorization/settlement coverage removed during the external-only transition; architecture negatives alone are not the full API suite |

The remaining runner allocation, protected-main controls, independent review,
downstream immutable binding and X1-X8 evidence retain their separate owners.
Repository access described as administrator-level does not override the actual
GitHub App permissions or establish that any external activity occurred.

## Release rule

Remain Draft / do-not-merge until all required exact-head checks and governance
conditions are proven. No documentation checker, unit fixture, administrator
assertion or repository-owned artifact may supply independent production approval.
