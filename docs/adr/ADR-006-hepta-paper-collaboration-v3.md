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
proposals, decisions, revisions, reviews, merges and Paper Room events.
Repeatable migration `0044` upgrades new Agent-proposal storage to the signed
V2 lease/work epoch and stores the authoritative manifest hash relationally.
Its validated constraints bind the lease to the same Paper and keep the full
signed proposal identity null-safely identical to `record_json`; startup also
verifies exact column, constraint and ordered-index catalog shape. It refuses
to fabricate these authenticated fields when legacy proposal rows exist
because V1 signatures never covered them. Each mutation uses the existing
business-row plus outbox transaction, optimistic version checks and
idempotency ledger. The memory implementation executes the same command
against a cloned state and swaps it into authority only after all validation
succeeds, so errors and duplicate IDs cannot advance state, cursor or outbox.

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

External Agents submit Ed25519-signed `AgentProposal` records. New mutations
accept V2 only: its canonical frame binds Paper, work item, expected work
version, section, parent revision, exact lease UUID and fencing token, artifact
manifest UUID and authoritative root, and the active human-owned
`AgentBinding`. Under the final memory lock or PostgreSQL transaction, Hepta
requires active assigned work at that exact version, one unexpired live lease
matching the signed epoch and current section head, and an exact Team roster
member whose player, binding and Agent ID agree. V1 signing helpers and frozen
vectors remain byte-compatible for SDK readers, but V1 signatures are not
accepted by the proposal mutation endpoint. The binding is the sole Agent
identity and key authority: Agent ID, key ID, canonical public key and
public-key hash must agree. The legacy v1 Agent registry is neither read nor
written, and Agent proposals do not rely on a Consumer user assertion.

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

Proposal V2 makes a lease replacement or work reopen a cryptographic epoch
change: replaying the exact previously signed body fails even when the same
binding reacquires the section. Parents are resolved through a Paper-scoped revision namespace containing both
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

`reproducing` in the stored PaperPhase V1 column/JSON is a compatibility wire
value. The player-scoped Author Raid projection MUST expose that checkpoint as
`player_phase=reproduction_readiness`; it means bundle readiness only.
Independent reproduction belongs exclusively to Review Raid. A future stored
phase rename requires a versioned data migration and may not reinterpret old
snapshot bytes in place.

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
role/availability matcher. Matcher V2 proposal identity binds the solver
version, source ticket IDs and versions, player IDs, exact ordered role
preferences, availability/private-party partition, and the final
player-to-role assignment. Reordered preferences, reassignment, or a requeued
trio therefore cannot reuse its earlier attempt. Active legacy proposals
lacking that frozen contract are expired and their safe tickets are requeued;
there is no public-hash compatibility fallback.
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

### 8. Contribution authority and liveness

Contribution ledgers freeze the complete authoritative fact set; there is no
256-reference ceiling. Every accepted artifact-manifest ID and every approving
section-review ID is retained in canonical sorted order. The explanatory XP
function alone is capped: an author receives 100 points for one-or-more accepted
artifacts, 150 for one-or-more approving reviews, and at most 250 total. The
257th fact therefore changes the ledger hash and remains auditable but cannot
mint more points. This avoids an irreversible liveness trap without discarding
scientific history.

One artifact manifest has exactly zero or one accepted Agent proposal globally,
and consequently zero or one credited human author. Memory checks this before
recording the human acceptance. PostgreSQL additionally enforces a global
partial unique index on accepted proposals, so two concurrent acceptances have
one transaction winner and the losing decision rolls back atomically.

The caller-supplied contribution-ledger UUID must be non-nil. Promotion reserves
it for the exact Paper and release-candidate hash under the Paper memory lock or
in the same PostgreSQL transaction as promotion. Ledger creation must consume
that ownership; it cannot claim an unreserved or differently owned ID. The
PostgreSQL primary key serializes cross-Paper promotion races, and a composite
foreign key binds the ledger to the exact ID, Paper and release-candidate hash.
Because pre-0047 promoted candidates did not retain their requested ledger ID,
0047 rejects an in-flight candidate without a frozen ledger instead of silently
stranding it or fabricating ownership.

Frozen PostgreSQL ledgers retain their complete `entries_json` alongside the
JSON envelope and enforce scalar, timestamp and entries parity. Before any
RaidScore calculation, the loader checks relational parity, canonical author
and reference ordering/uniqueness, milestone points, and recomputes the ledger
hash from the full set. Drift can therefore fail availability but cannot alter
RaidScore. Every load also re-reads the exact reservation triple instead of
depending on startup or the foreign key alone. The reservation and ledger rows
are append-only: enabled-always row guards reject update/delete and statement
guards reject TRUNCATE. Readiness compares the complete table-scoped constraint,
column, storage, index, trigger and guard-function catalog exactly; a decoy
constraint name or permissive `OR TRUE` definition cannot satisfy it.

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
