# External production evidence intake contract v1

Status: active operational evidence contract  
Production authorization: `not_granted`

This contract defines how independently issued evidence for V12-X1 through
V12-X8 is identified, transported, validated, retained, and bound to one exact
CEX candidate. It does **not** allow repository source, local tests, GitHub
Actions, a repository administrator, or this checker to self-certify an
external gate.

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
- a mutable branch URL, latest artifact alias, local path, or evidence without a
  verified SHA-256 digest;
- evidence issued for another commit, tree, artifact set, topology, provider,
  jurisdiction, or approval scope.

## 2. Machine-readable bundle

The shape-only template is
`docs/templates/cex-external-production-evidence-bundle-v1.json`. A real bundle
uses schema `cex.external-production-evidence-bundle.v1`, sets `template=false`,
and binds all records to:

- repository `TrillionniumFoundation/CEX`;
- exact 40-character commit and tree SHA;
- exact migration head;
- immutable generated repository-candidate manifest URI and SHA-256;
- exact artifact/deployment scope;
- UTC generation time and approved retention-policy identity.

The schema is closed: unknown root, candidate, manifest, gate, evidence,
issuer, or final-decision fields fail validation rather than becoming an
unreviewed extension channel.

Real evidence bundles are retained in the approved external custody system.
They are not committed into the source tree. The repository may retain only the
shape-only template and this contract.

## 3. Evidence envelope

Every pass or fail record contains:

- immutable URI with no embedded credentials and a `sha256:<64 lowercase hex>`
  content digest;
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

Structural validation does not prove that the issuer identity, signature,
organization, independence statement, or underlying activity is genuine. The
responsible human control owner must verify those facts before accepting the
evidence.

## 4. Gate-specific acceptance

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
provider and production scope, then issue explicit approvals or denials bound to
the exact candidate and deployment scope. Repository actors may not infer that
approval is unnecessary.

### V12-X8 — final human go/no-go

The final release authority verifies repository qualification and accepted
X1-X7 evidence on the same immutable candidate/artifact set, then records `go`
or `no-go`, identity, organization, the exact
`final_human_release_authority` role, UTC time, scope, and evidence digest.
Automation may validate structure but may not emit, infer, or impersonate this
decision.

## 5. Validation modes

Run the repository contract check with:

```text
python3 scripts/check-external-production-evidence-contract.py --contract-only
```

This validates only the source-tree template, active traceability and navigation
wiring, anti-self-certification policy, and closed schema. It deliberately
reports production authorization as `not_granted`.

A responsible evidence custodian may structurally inspect an externally stored
bundle with:

```text
python3 scripts/check-external-production-evidence-contract.py \
  --bundle /secure/intake/cex-external-production-evidence.json
```

A successful bundle check means only that the supplied JSON satisfies this
shape and cross-gate consistency contract. It is not a cryptographic signature
verification, reviewer independence determination, legal conclusion, financial
approval, or production authorization.

## 6. Ordering and revocation

X8 cannot be `go` unless X1-X7 are structurally present as `pass` for the same
candidate. Any candidate, artifact, topology, provider, jurisdiction, control,
or approval-scope change requires re-evaluation by every affected issuer.
Revocation, expiry, supersession, or a later failing result blocks authorization
and must be retained rather than deleted.

## 7. Change protocol

Changing this schema, accepted URI policy, gate IDs, evidence envelope,
independence rule, issuer-role binding, closed field sets, traceability wiring,
or X8 ordering requires an updated template when its shape changes, checker,
implementation addendum, active documentation index, traceability, tests, and a
new shared candidate trigger. No change may weaken `self_certifiable=false` or
turn structural validation into automatic production authorization.
