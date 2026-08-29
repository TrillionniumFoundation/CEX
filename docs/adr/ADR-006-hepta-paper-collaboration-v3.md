# ADR-006: Hepta Paper Collaboration Kernel v3

- Status: Accepted for Paper Raid alpha
- Date: 2026-08-05
- Supersedes: none; this extends ADR-005 without widening legacy `/v1/hepta`

## Context

Paper Raid is a cooperative research workflow, not a text-length game. Three to
five human authors bring independently operated external Agents and produce one
auditable paper plus its evidence and reproduction artifacts. ADR-005 created
the human, Agent-binding, Team, Paper, revision, author-consent and joint-
submission aggregates. It did not define matchmaking, immutable research
artifacts, signed Agent/human collaboration, section concurrency or a Paper
Room read model.

The Integration repository already defines the neutral
`paper-raid.artifact-bundle.v1` manifest. Hepta must consume that exact contract
without importing Integration as a runtime dependency or accepting caller-
reported paper hashes. The existing singleton `hepta_league_state` JSON and
global advisory lock are not suitable authorities for concurrent Paper work.

## Decision

### 1. Authority and storage boundaries

Hepta is authoritative for long-lived research facts and human decisions.
Nakama remains authoritative for multiplayer admission, presence, ordering,
reconnection and replay. Artifact bytes live in content-addressed object/Git
storage; Hepta stores verified manifests, immutable hashes, URIs and ACLs.
Chain commitments and finality remain outside this kernel.

Migration `0033` adds normalized PostgreSQL tables for matchmaking, artifact
and evidence lineage, experiments/runs/figures/claims, section leases,
proposals, decisions, revisions, reviews, merges and Paper Room events. Each
mutation uses the existing business-row plus outbox transaction, optimistic
version checks and idempotency ledger. The memory implementation executes the
same command against a cloned state and swaps it into authority only after all
validation succeeds, so errors and duplicate IDs cannot advance state, cursor
or outbox.

### 2. Exact neutral artifact adapter

Hepta vendors the exact Integration schema and golden fixture from immutable
revision `61c9ffd0b410faed023b68a604e4d0906c3006f8`. The schema digest is
`sha256:aba8fd6d1059c59f63cdb258a2e507de1bed3ff74f935b7dd0214e4640ad9bb6`.
The adapter independently parses with unknown-field rejection, validates safe
logical paths and immutable CAS/Git URIs, recomputes canonical JSON plus newline
SHA-256, and checks every storage location against the source object digest.
Integration is not a runtime trust oracle.

`PaperRevision`'s four hashes are no longer self-attested. A revision must bind
one verified artifact root from the same Paper and exactly one object for each
required role:

- `paper_source` -> `source_manifest_hash`
- `bibliography` -> `bibliography_hash`
- `claim_evidence_graph` -> `claim_evidence_graph_hash`

The logical path and digest bindings are persisted in
`hepta_paper_revision_artifact_bindings`. Unknown, cross-Paper, ambiguous and
tampered descriptors are rejected before revision state changes.

### 3. Human and Agent actions

External Agents submit Ed25519-signed `AgentProposal` records bound to Paper,
work item, section, parent revision, artifact root, Agent registration and the
frozen human-owned binding. Agent proposals do not rely on a Consumer user
assertion.

Human evidence verification, proposal decisions, section reviews and merges
use the current human signing key and a Consumer Edge assertion that binds the
player, HTTP method/path, body hash and idempotency nonce. Citation signatures
cover DOI and canonical URL; changing either invalidates the signature. Agent
identity does not grant authorship, ethics approval, license acceptance or
publication authority.

### 4. Section concurrency

The alpha does not use character-level CRDTs. A section has one head, one
bounded lease and a monotonically increasing fencing token. The flow is:

`lease -> signed Agent proposal -> signed human decision -> section revision -> independent signed review -> signed merge`

Parents are resolved through a Paper-scoped revision namespace containing both
whole-Paper and section revisions. Cross-Paper, cross-section, stale and cyclic
parents fail. A merge atomically advances the section head and consumes the
exact lease; older fencing tokens can never write after a newer lease exists.

### 5. Phase gates

The permitted mutation matrix is explicit:

| Mutation | Allowed Paper phases |
| --- | --- |
| Artifact manifest | preregistering, researching, experimenting, drafting, integrity_review, reproducing |
| Evidence, citation, claim | researching, experimenting, drafting, integrity_review, reproducing |
| Experiment plan | preregistering, researching |
| Run, figure lineage | experimenting, drafting, integrity_review, reproducing |
| Section lease/proposal/decision/revision/merge | drafting |
| Section review | drafting, integrity_review |

`author_approval`, `integrity_hold` and `submission_ready` freeze every v3
collaboration mutation. This separates a finished Nakama room, a mutable
research project and an immutable release candidate.

### 6. Matchmaking and reads

Protocol and Team aggregates support 3-5 authors. The first playable matcher is
deliberately fixed to exactly three distinct human players. Queue selection and
replacement are deterministic; all three must accept, and any decline requeues
the tickets without inventing a Team.

The Paper Room is a player-scoped aggregate projection. PostgreSQL builds it in
one `REPEATABLE READ` transaction and exposes only the asserted member's
research-session access. Cursor catch-up reads persisted typed room events.

## Consequences

- Paper source, bibliography and claim graph now have provenance that can be
  independently rebuilt and audited.
- Failed commands are atomic in both memory and PostgreSQL implementations.
- Collaboration is coarser than a live editor but has deterministic conflict,
  review and signature semantics suitable for an alpha.
- The old `/league` `LeagueState`, CEX `/battle`, keyword scoring and immediate
  reward path remain non-authoritative and must not write these aggregates.
- Evaluation, reproduction, Contribution/Raid scoring, Appeal and Chain
  finality are subsequent layers; they may read this kernel but may not rewrite
  its immutable records.

## Verification

Release gates require exact Integration fixture parsing and hash equality,
Rust/Node signing vectors, memory/PostgreSQL parity, repeatable application of
`0033`, concurrent matchmaking decisions, 3-player unanimity and decline
requeue, URI/DOI tamper rejection, cross-scope/stale/cycle/fence rejection,
phase freeze, Paper Room consistency and live PostgreSQL tests.
