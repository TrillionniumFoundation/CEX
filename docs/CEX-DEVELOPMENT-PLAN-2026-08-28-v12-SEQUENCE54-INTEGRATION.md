# CEX v12 Sequence 54 integration and residual-gap closure

Status: active integration authority  
Production authorization: `not_granted`

## 1. Authority being converged

Sequence 54 is a non-regressive integration of two independently evolved v12 lines:

- functional base: `fix/cex-v12-audit-remediation-20260905@acc798ad07c1005dd8b94b54c4a71eff6288f4c7`, including migration head `0088_enforce_provider_terminal_evidence_binding.sql`, 23 workspace members, Matrix durability, external-Agent-only runtime boundaries and terminal provider-evidence binding;
- security/governance donor: `fix/cex-v12-final-closure-v4-20260908@9e1426c09cb19d062504b4a6732bf83867f675ad`, including Rust 1.98.1 convergence, recursive active-surface detection, fail-closed Ruleset tooling and allocation-only runner diagnostics.

Neither parent may be treated as the final candidate after this integration. Evidence transfers only when an invariant is re-executed on one unchanged Sequence 54 exact commit and prospective merge tree.

## 2. Non-regression invariants

The integrated tree must retain all of the following:

1. the exact 23-member Cargo workspace and matching module catalog;
2. global migration head `0088_enforce_provider_terminal_evidence_binding.sql`;
3. the external-Agent-only Capability and Execution boundary, including retired local-process execution routes;
4. durable Matrix relay/poller state, replay and queue semantics;
5. exact Ledger/Gateway/Execution/Audit/TRNM money and receipt-recovery contracts;
6. Paper Raid authenticated room/review envelopes, v2 Nakama controls, runtime/SBOM custody and real mobile accessibility assertions;
7. Rust `1.98.1` and release commit `48a229ceaefd4985c50990b14116b6d856af0985` across active host and container selectors;
8. no source-writing or automatic convergence workflow;
9. no active `1.95.0`, `1.98.0`, `stable`, `latest`, `beta` or `nightly` Rust selector;
10. no repository-external Cargo path dependency;
11. the bounded RustSec policy, all-feature dependency closure, complete release-surface policy, independent CODEOWNERS ownership and pinned supply-chain workflows remain present and mutually consistent.

`scripts/check-sequence54-integration.py` enforces the integration-specific invariants. `scripts/check-sequence54-rustsec-admission.py` prevents the Sequence 54 RustSec policy and workflow surface from being silently deleted or weakened. `scripts/check-rust-advisory-exceptions.py` performs dynamic default/all-feature graph, release-surface and hostile-fixture validation. `scripts/check-rust-toolchain-convergence.py` performs the recursive Git-tree and blob-content toolchain scan. Existing v12 documentation, migration, exact-money, PostgreSQL, Gateway, Execution, provider, Hepta, Matrix and candidate gates remain required rather than being replaced by this document.

## 3. Bounded RustSec admission

Sequence 54 currently permits only these time-bounded exceptions:

- `RUSTSEC-2023-0071` / `rsa 0.9.10`: lockfile metadata only; activated and workspace execution reachability must remain empty;
- `RUSTSEC-2026-0214` / `gumdrop 0.8.1`: vendored finality test-generator development path only; normal/build release execution must remain empty;
- `RUSTSEC-2024-0436` / `paste 1.0.15`: compile-time reachability restricted to the exact `hepta-research-league -> trnm-finality-verifier -> tendermint -> flex-error -> paste` path.

The policy is bound to PR #53, risk issue #35 and independent security approver `Tomasrgbsf`; its current expiry ceiling is `2026-10-08`. A repository-authored record cannot substitute for a fresh exact-head GitHub approval. Any new advisory, package version, source, target, path, workspace reachability, feature reachability, expiry extension or policy omission fails closed. Removal of the dependencies is preferable; until removal is verified, the risk issue remains open.

## 4. Governance and required checks

`docs/repository-ruleset-required-contexts-v1.json` is the complete desired Ruleset object for `main`. Application is compare-and-swap against an exact expected main SHA and policy digest. The live object must require pull requests, two approvals, CODEOWNERS, latest-push approval, stale-review dismissal, resolved review threads, non-fast-forward rejection, deletion rejection and all current Sequence 54 status contexts.

Allocation-only runner diagnostics are not admission checks. They may prove that a runner label can accept a job, but cannot substitute for a source checkout, test, PostgreSQL, supply-chain or prospective-merge result.

## 5. Removed obsolete automation

The integrated tree removes the old Sequence 44 and World settlement auto-convergence workflows. Repository workflows are evidence producers; they do not push source, update `main`, self-approve or manufacture external evidence.

## 6. Exact-candidate qualification

A candidate is repository-qualified only when, on one unchanged source SHA and its prospective merge tree:

- every required job starts, contains non-empty steps and succeeds;
- the generated candidate manifest binds the exact source/tree/base/merge identities and retained artifact digests;
- the live Ruleset is read back and matches the complete policy object;
- direct update, non-fast-forward update and deletion negative probes are rejected by GitHub while a same-actor positive control succeeds on a disposable canary;
- at least two eligible approvals, including the required independent security approval, bind the final head;
- all review conversations are resolved.

A queued, skipped, startup-failed or zero-step job is not evidence of source success.

## 7. External production blockers

Repository closure cannot self-certify production. Representative-volume restore, real deployment/cutover/rollback, provider outcomes, credential custody, sustained SLO qualification, World authority transfer and no-dual-writer proof, independent security/operations/financial/legal/commercial review and final human go/no-go remain externally evidenced blockers.

The only truthful result before those records exist is `production_authorization=not_granted`.
