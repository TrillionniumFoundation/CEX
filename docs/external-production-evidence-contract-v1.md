# External production evidence intake contract v1

Status: active operational evidence contract  
Production authorization: `not_granted`

This contract defines how independently issued evidence for V12-X1 through
V12-X8 is identified, transported, validated, retained, and bound to one exact
CEX candidate. It does **not** allow repository source, local tests, GitHub
Actions, a repository administrator, or the structural checker to self-certify
an external gate.

## 1. Hard boundary

Repository qualification and production authorization are different decisions.
A generated repository candidate manifest may prove source, migration, test,
workflow, SBOM, provenance, and bounded CI evidence for one exact commit/tree.
It cannot prove representative production topology, real provider outcomes,
credential custody, sustained load, independent control review, legal or
commercial approval, or the final human release decision.

The following are never external-gate evidence:

- source code, Markdown, a template, fixture, mock provider, or synthetic log;
- an empty, skipped, queued, zero-step, or pre-runner workflow record;
- an administrator assertion, waiver, repository-owner signature, or self-review;
- a mutable branch URL, latest-artifact alias, local path, or evidence without a
  verified SHA-256 digest;
- evidence issued for another commit, tree, migration head, artifact set,
  topology, provider, jurisdiction, or approval scope;
- the candidate manifest itself, candidate payload, hosted repository evidence,
  SBOM, provenance, Cargo-lock digest, migration digest, or any other
  candidate-manifest or repository evidence identity;
- repository-owned storage such as CEX Actions records or
  `artifact://cex-p0-evidence-*`.

A repository object can establish repository qualification only. Copying,
renaming, or re-addressing it does not turn it into independent external
production evidence; SHA-256 identity reuse is rejected even when the URI
changes.

## 2. Machine-readable bundle

The shape-only template is
`docs/templates/cex-external-production-evidence-bundle-v1.json`. A real bundle
uses schema `cex.external-production-evidence-bundle.v1`, sets `template=false`,
and binds all records to:

- repository `TrillionniumFoundation/CEX`;
- exact 40-character commit and tree SHA;
- active migration head
  `0088_enforce_provider_terminal_evidence_binding.sql`;
- immutable generated repository-candidate manifest URI and SHA-256;
- exact artifact/deployment scope;
- UTC generation time and approved retention-policy identity.

The schema is closed: unknown root, candidate, manifest, gate, evidence, issuer,
or final-decision fields fail validation rather than becoming an unreviewed
extension channel.

Real evidence bundles and downloaded candidate manifests are retained in the
approved external custody system. They are not committed into the source tree.
The repository may retain only the shape-only template, checker, and this
contract.

## 3. Exact-byte acquisition and candidate-manifest binding

Bundle validation requires both inputs:

```text
python3 scripts/check-external-production-evidence-contract.py \
  --bundle /secure/intake/cex-external-production-evidence.json \
  --candidate-manifest /secure/intake/cex-candidate-manifest.json
```

Each external path is accepted only when it resolves outside the source tree,
is not a final symlink, is a regular file, remains within the bounded intake
size, and has unchanged device, inode, size, modification time, and change time
across one complete read. The bytes are then written to private, read-only
single-read immutable snapshot files. All parsers, the frozen structural core,
the frozen Sequence-49 binding core, and the candidate-manifest validator
consume only those snapshots. The checker compares the snapshots with the
originally acquired bytes after every child validator returns.

The checker computes the exact candidate-manifest snapshot SHA-256 and requires
equality with `repository_candidate_manifest.sha256`. It then runs
`scripts/check-release-baseline-manifest.py` against that same snapshot. Because
that authoritative validator intentionally emits child diagnostics before its
own result, the intake checker parses only the final complete trailing JSON
object and requires:

- process exit code zero;
- schema `cex.active-v12-manifest-guard.v1`;
- `status=ok`;
- the active migration head;
- the exact snapshot path supplied by the intake checker.

The manifest bytes themselves must declare:

- schema `cex.release-baseline-manifest.v1`;
- status `candidate`;
- the same repository, commit SHA, tree SHA, migration head, and qualification
  scope as the external bundle;
- `production_ready=false`;
- `production_authorization=not_granted`.

The manifest must exist before any accepted external evidence, and both manifest
and evidence must predate or equal `bundle.generated_at`. A URI and digest alone
are insufficient when the supplied bytes do not validate or do not describe the
same candidate.

## 4. Candidate-artifact isolation

Before any bundle can become structurally eligible, the checker recursively
collects every URI and canonical SHA-256 identity present in the validated
candidate manifest, and also reserves the external bundle's candidate-manifest
URI and digest.

No V12-X1 through V12-X8 evidence record may reuse any reserved URI or digest.
This prevents candidate payloads, hosted checks, SBOM, provenance, Cargo-lock
evidence, migration evidence, repository-integrity output, or the candidate
manifest itself from being relabelled as real external activity. External
records using a repository-owned URI are also rejected even when their content
does not otherwise alias a manifest field.

This is a structural anti-confusion rule. It does not authenticate an external
storage provider or prove that an issuer is independent; the responsible human
control owner still verifies custody, identity, signatures, and the underlying
activity.

## 5. Evidence envelope

Every pass or fail record contains:

- immutable URI with no embedded credentials and a
  `sha256:<64 lowercase hex>` content digest;
- issuing actor, organization, role, and an explicit statement that the issuer
  is independent of repository automation for the asserted domain;
- an issuer role that exactly matches the gate's machine-declared
  `required_issuer_role`;
- UTC execution/decision timestamp;
- exact candidate commit/tree repeated inside the record;
- bounded scope describing environment, provider, topology, jurisdiction, or
  control domain;
- explicit `pass` or `fail` decision;
- no waiver or inferred approval.

One immutable evidence object cannot be reused across two gates. Structural
validation does not prove that the issuer identity, signature, organization,
independence statement, or underlying activity is genuine. The responsible
human control owner must verify those facts before accepting the evidence.

## 6. Gate-specific acceptance

### V12-X1 — representative-volume disaster recovery

Evidence covers real storage topology, representative data volume, backup/PITR,
restore, failure injection, exact data parity/fingerprints, measured RPO/RTO,
operators, and independent operations/recovery acceptance.

### V12-X2 — real deployment, cutover, and rollback

Evidence binds exact image/build digests, real service identities, network
policy, storage and secret mounts, cutover, rollback, partial failure, timings,
and authority/value conservation.

### V12-X3 — real provider reconciliation

Evidence contains confirmed-executed, confirmed-not-executed, and indeterminate
outcomes from the real integration, including immutable provider artifacts,
terminal-safe replay, no duplicate provider or Ledger effect, and operator
recovery.

### V12-X4 — credential custody and break glass

Evidence covers issuance, distinct custody, least privilege, rotation,
revocation, dual-control break glass, complete audit trail, and post-use
revocation accepted by an independent security/custody owner.

### V12-X5 — sustained production-like soak and SLO qualification

Evidence covers representative concurrency and duration, queue age, retry
budgets, lease recovery, provider reconciliation, Ledger parity, Audit delivery,
latency, availability, resource saturation, accepted thresholds, and retained
metrics/logs.

### V12-X6 — independent security, operations, and financial-control review

Attributable independent reviewers issue explicit decisions for authentication,
authorization, secrets, supply chain, network exposure, abuse and incident
response; deployability, capacity, observability, recovery and rollback; and
value conservation, reconciliation, segregation of duties, and exceptions.

### V12-X7 — legal, commercial, and provider approvals

Responsible authorities identify the applicable jurisdiction, contract,
provider, and production scope, then issue explicit approvals or denials bound
to the exact candidate and deployment scope. Repository actors may not infer
that approval is unnecessary.

### V12-X8 — final human go/no-go

V12-X8 `pass` requires exactly one evidence record. The
`final_human_decision` object must be the same immutable record as that V12-X8
evidence: URI, digest, actor, organization, role, scope, candidate commit/tree,
and timestamp must all agree. The record uses the exact
`final_human_release_authority` role.

The final release authority verifies repository qualification and accepted
X1-X7 evidence on the same immutable candidate/artifact set, then records `go`
or `no-go`. X8 and the final decision must occur after every accepted X1-X7
record. Automation may validate structure but may not emit, infer, or impersonate
this decision.

## 7. Validation modes

Run the repository source-contract check with:

```text
python3 scripts/check-external-production-evidence-contract.py --contract-only
python3 scripts/check-external-production-evidence-contract.py --self-test
```

This validates only the source-tree template, active traceability/navigation
wiring, anti-self-certification policy, closed schema, exact-byte acquisition,
manifest binding, mixed-output trailing JSON parsing, temporal ordering,
duplicate rejection, candidate-artifact isolation, repository-owned URI
rejection, and the X8 same-record rule. It deliberately reports:

```text
production_authorization=not_granted
checker_may_grant_production_authorization=false
```

A successful real-bundle check means only that the supplied JSON and candidate
manifest satisfy this structural and cross-gate consistency contract. It is not
cryptographic signature verification, reviewer-independence determination,
legal conclusion, financial approval, or production authorization.

## 8. Ordering and revocation

X8 cannot be `pass` unless X1-X7 are structurally present as `pass` for the same
candidate. The manifest must precede every accepted evidence record, the final
decision must not predate any accepted X1-X7 record, and bundle generation must
not predate any included evidence or decision.

Any candidate, migration, artifact, topology, provider, jurisdiction, control,
or approval-scope change requires re-evaluation by every affected issuer.
Revocation, expiry, supersession, or a later failing result blocks authorization
and must be retained rather than deleted.

## 9. Change protocol

Changing this schema, exact-byte acquisition, accepted URI policy, gate IDs,
evidence envelope, manifest binding, temporal ordering, candidate-artifact
isolation, independence rule, issuer-role binding, closed field sets,
traceability wiring, or X8 ordering requires an updated template when its shape
changes, checker, embedded regression self-tests, implementation addendum,
active documentation index, traceability, and a new shared candidate trigger.
No change may weaken `self_certifiable=false` or turn structural validation into
automatic production authorization.
