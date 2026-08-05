# ADR-007: Hepta Paper Review, Reproduction and Appeal v4

- Status: Accepted for Paper Raid alpha
- Date: 2026-08-05
- Supersedes: none; extends ADR-005/006 and does not widen legacy `/v1/hepta`

## Context

Paper Raid produces a paper and reproducible research bundle, not a single game
score. The legacy Research League workflow stores generic reports inside one
`LeagueState`, accepts an operator identity, compares metric maps for exact
equality, and mutates an evaluation while handling an appeal. Those semantics
cannot establish independent peer review, accommodate stochastic experiments,
or preserve the record that was actually challenged.

The collaboration kernel already supplies frozen release-candidate hashes,
signed human/Agent provenance and artifact lineage. This layer must evaluate
those immutable facts without allowing gameplay activity, message volume,
token consumption or speed to change scientific quality or authorship.

## Decision

### 1. New Paper-scoped authority

Migration `0034` adds normalized tables for Contribution Ledgers, evaluations,
two reviewer attestations, PaperScore, RaidScore, reproductions, Appeals and
Appeal resolutions. This v4 authority does not read or write legacy
`workflows.rs`, `hepta_league_state`, generic evaluator reports, CEX `/battle`,
keyword scoring or immediate ledger rewards.

Every command is idempotent. PostgreSQL writes the immutable business row and
outbox event in one transaction. The memory implementation validates and
mutates a clone, then swaps it into authority only after success. Evaluation
and reproduction corrections append explicit `supersedes_*` rows; prior rows
are never updated.

### 2. Contribution is not scientific quality

The Contribution Ledger has exactly one entry per frozen human author. CRediT
roles must equal the release candidate. Artifact credit requires a same-paper
Agent proposal and that player's signed accept decision; review credit requires
that player's approving same-paper section review. XP is deterministically
derived (not caller supplied), and the ledger hash is frozen into the release
candidate before author consent.

`PaperScore`, `Contribution Ledger`, and `RaidScore` remain separate:

- PaperScore measures the paper using the frozen 10,000-bps rubric.
- Contribution records signed, accepted work and review provenance.
- RaidScore awards collaboration/milestone XP and declares
  `paper_score_excluded=true`.

Contribution never determines author eligibility/order and does not enter
PaperScore.

### 3. Hard gates and independent panel

An evaluation binds one `submission_ready` PaperBundle, its release hash, a
versioned tolerance policy, fixed-point reference metrics, PaperScore and hard
gates. The asserted evaluator and exactly two signed reviewers must be three
distinct active players, none of whom is an author. Each records an immutable
COI attestation hash and signs the same evaluation signing hash.

The rubric maxima are frozen at method rigor 2,500; experiment/statistics
1,500; reproducibility 1,500; evidence/citations 1,500; value/originality
1,500; argument/expression 1,000; and ethics/transparency 500 bps. Fabricated
citation/data, hidden failed runs, missing author consent, unsupported core
claims, broken lineage, or incomplete license/ethics/COI makes the release
`not_eligible` regardless of score. Eligible acceptance additionally requires
at least 6,000 bps and both reviewer approvals.

### 4. Versioned stochastic reproduction

Tolerance policy v1 supports four typed deterministic rules:

- absolute fixed-point delta;
- relative delta in basis points with a zero-safe denominator;
- statistical interval-overlap, effect-delta and p-value thresholds;
- exact frozen seed-set hash.

The reproducer is a signed active player distinct from every author and panel
member. All rule results and their detail hashes are retained, including failed
reproductions. A correction may supersede only that reproducer's latest report
for the same evaluation.

### 5. Appeal is append-only

Only an asserted frozen human author may open the one Appeal for an evaluation.
The signature binds evaluation, paper, release, grounds and evidence manifest.
The old evaluation remains byte-for-byte intact; the read model derives its
settlement projection as `challenged`.

During an open Appeal a replacement evaluation may be appended only for the
same submission/release and must use a panel disjoint from the original one.
An Appeal resolver must be distinct from the authors, appellant, original
panel, and replacement panel. `upheld` requires the exact superseding
evaluation; `denied` cannot reference one. Resolution is a separate signed row
and changes only the read projection to `resolved`.

### 6. Finality boundary

Every evaluation starts at `pending_finality`. Opening an Appeal holds the
settlement; resolving it does not itself create Chain finality. Paper bytes may
still be downloaded and manually submitted while Chain is unavailable, but
rankings and economic rewards remain withheld until a separately verified
canonical Chain receipt exists. Chain is not a scientific-truth oracle.

## Consequences

- Peer review, reproduction failure and successful corrections remain
  auditable instead of being overwritten.
- Random experiments can be compared without unsafe exact-map equality.
- Consumer assertions prevent caller-supplied evaluator, appellant, reproducer
  or resolver identities.
- Scientific quality cannot be inflated with gameplay telemetry or Agent token
  volume.
- The UI can render `pending_finality`, `challenged` and `resolved` while all
  signed source records remain immutable.

## Verification

The release gate requires memory/live-PostgreSQL parity, repeatable migration
`0034`, frozen signature vectors, exact two-reviewer and COI independence,
PaperScore hard gates, all four tolerance modes, seed/statistical failures,
append-only evaluation/reproduction supersession, appellant assertion binding,
resolver independence, outbox atomicity and derived settlement projections.
