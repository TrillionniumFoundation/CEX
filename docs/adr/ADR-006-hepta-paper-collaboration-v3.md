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
work item, section, parent revision, artifact root and the active human-owned
`AgentBinding`. The binding is the sole Agent identity and key authority for
Paper Raid proposals: Agent ID, key ID, canonical public key and public-key
hash must agree. The legacy v1 Agent registry is neither read nor written, and
Agent proposals do not rely on a Consumer user assertion.

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

Creating a whole-Paper revision is the materialization barrier. Hepta rejects
the operation while a section lease, proposal or revision is still in flight,
then atomically captures a canonical descriptor for every advanced section
head. Each entry binds the section key, whole-Paper base, section head, signed
merge id, patch manifest and patch digest. The descriptor root is stored on the
Paper revision, chained to its parent's root, and included in every newly
promoted release-candidate hash. In the same transaction, section heads are
rebased to the new whole-Paper revision so an author-approval rollback can
continue drafting without an explicit out-of-band rebase. Historical records
without the optional root retain their exact V2 serialization and hash.

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

Every newly created roster is fail-closed to one `captain`, one `evidence`, and
one `experiment` seat; optional fourth and fifth seats are `support`. This is
validated again at the Hepta write boundary, so a Consumer cannot bypass the
matcher with arbitrary role strings and regain the former permissive duty
path. Historical noncanonical rosters remain readable for audit but cannot use
role-gated author mutations; they must form a new canonical Team.

The Paper Room is a player-scoped aggregate projection. PostgreSQL builds it in
one `REPEATABLE READ` transaction and exposes only the asserted member's
research-session access. Cursor catch-up reads persisted typed room events.

### 7. Review Raid draft leases and crash recovery

An evaluation draft is not an unbounded lock. New
`hepta.paper_raid.evaluation_draft.v2` records receive an immutable,
server-owned 24-hour `expires_at`, included in the draft hash. The evaluator
assignment is pinned when the draft is created; each reviewer assignment is
pinned only with its immutable attestation. Every pinned assignment stores the
exact `pinned_evaluation_id`; this identity is retained after consumption or
expiry, so a prior expired draft can never authorize release of a replacement
panel. Attestation and finalization writes fail closed at the deadline.

There is no player or reviewer force-release command. A later assignment claim
holds the Paper lock, locks every stale open draft, verifies that its pinned
seats exactly match the evaluator and stored attestations, then atomically:

1. transitions the draft `open -> expired` at its fixed deadline;
2. transitions only that exact pinned panel to `expired`;
3. appends `hepta.paper_raid.evaluation_draft.expired.v1` with the immutable
   draft hash, deadline and released assignment IDs; and
4. evaluates the new claim against the resulting vacancies.

Expired drafts and attestations remain append-only, but no longer reserve the
round; a newly claimed evaluator must use a fresh evaluation identity and
draft hash. Old attestations cannot satisfy the new quorum. A finalized draft
and its consumed assignments never enter the expiry path. PostgreSQL enforces
the same ownership relation in triggers, so a direct pinned-to-expired update
without the matching expired draft (and reviewer attestation, where required)
fails. Migrated v1 alpha drafts retain their original scientific hash and
receive the deterministic compatibility deadline `created_at + 24 hours`.

Matchmaking proposals have a server-owned five-minute response/materialization
deadline. Both `proposed` and unanimously `accepted` formations remain subject
to it until a team exists. Timeout, decline, or a matched-player withdrawal
atomically releases eligible peers and immediately runs the deterministic
role/availability matcher. Proposal identity binds source ticket IDs **and
their versions**, so a requeued trio cannot collide with its expired attempt.
Queue admission, selection, timeout requeue, decision, and materialization all
fail closed unless every affected human is active and has exactly one active
external Agent binding; ineligible historical tickets remain auditable but do
not influence compatible-pool or ETA projections.
Successful materialization transitions all exact source tickets from `matched`
to terminal `consumed` and its proposal from `accepted` to terminal
`materialized`; only `queued` and `matched` participate in the one-live ticket
uniqueness boundary. Direct Team creation is a disjoint UUIDv4 namespace;
matcher proposal/Team IDs are deterministic UUIDv5 values. Shared challenge
and Team-ID locks plus explicit materialization events prevent generic Team
creation from racing or impersonating matcher provenance. Queue hints derive
their shortage from an actual three-role bipartite assignment, not merely a
role-name union.

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
Rust/Node signing vectors, memory/PostgreSQL parity, repeatable migrations,
concurrent matchmaking decisions, version-aware requeue/rematch, accepted
proposal expiry, materialization ticket consumption, 3-player unanimity,
URI/DOI tamper rejection, cross-scope/stale/cycle/fence rejection, phase
freeze, Paper Room consistency and live PostgreSQL tests.
