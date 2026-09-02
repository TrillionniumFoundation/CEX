# Trillionnium World deterministic fixture boundary

Status: active external-component contract  
Catalog ID: `trillionnium-world-fixture`  
Repository: `TrillionniumFoundation/Trillionnium-World`  
Workspace member: no  
Production evidence: external  
Production authorization: `not_granted`

## Purpose and authority

The pinned Trillionnium-World revision supplies deterministic World-transition fixtures used to verify CEX/Game contract compatibility. It is test input and a source-identity dependency, not an authoritative runtime inside the CEX Cargo workspace.

The fixture does not own CEX identity, Ledger, research, Agent, settlement, Nakama match, Game storage or Chain-finality state. Successful fixture execution proves only that the exact pinned source and consumer contract behave as asserted by the external-evidence workflow.

## Exact source binding

The authoritative pin lives in `evidence/world-authority-external-lock.json`. `.github/workflows/world-authority-external-evidence.yml` checks out the declared repository at the exact commit, verifies its exact tree, rejects a dirty checkout and runs the repository-provided positive and negative transition-fixture checks.

A branch name, moving tag, local sibling checkout, Cargo path dependency or unverified archive is not an acceptable binding. Updating the commit or tree creates a new integration candidate and requires a new CEX shared trigger plus exact-source evidence.

## Consumed contract

CEX consumes only the bounded transition fixture and its declared HTTPS/request-response contract. The fixture must remain deterministic, canonical and free of hidden mutable dependencies. Test scripts must cover positive transition behavior, tamper/identity negatives and formatting/test/vet checks appropriate to the fixture implementation.

World payloads remain untrusted inputs at every CEX or Game boundary. Fixture success cannot authorize World commands, public online play, a player market, value settlement or production cutover.

## Failure and recovery

Failure to fetch the exact commit/tree, dirty source, a fixture contract mismatch, nondeterministic output, formatting/test failure or missing evidence fails the external workflow closed. CEX must not silently substitute a local checkout, newer revision or synthetic fixture.

Rollback restores the prior lock record and its exact compatible consumer revision only through a new reviewed candidate. Historical evidence remains immutable and must not be relabelled as evidence for another source identity.

## Security and trust boundary

The World fixture must not receive CEX production credentials, Agent private keys, Ledger administrator tokens or Chain signing material. It executes as an untrusted build/test dependency with read-only source access and bounded outputs. Any network or filesystem capability beyond the documented fixture contract requires separate review.

## Verification and evidence limits

Repository wiring is checked through:

```text
python3 scripts/check-module-documentation.py
python3 scripts/check-development-docs.py
```

The external exact-source workflow may retain commit/tree identity, check results and artifact digests. That evidence is not owner-repository branch protection, representative-volume recovery, real Game/Nakama deployment, downstream cutover approval or production authorization.

## Change protocol

Changes to repository identity, fixture role, consumed contract, workflow, lock schema or production-evidence scope require this document, `docs/module-catalog-v1.json`, `docs/modules/index.md`, compatibility tests and the shared candidate trigger in the same CEX change. Production authorization remains `not_granted` until all independent external gates and final human go/no-go are complete.
