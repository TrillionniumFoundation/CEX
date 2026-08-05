# ADR-005: Hepta Paper Raid v2 authority and evidence boundaries

- Status: accepted for Alpha implementation
- Date: 2026-08-05
- Owners: Hepta Research League
- Protocol: `hepta.paper_raid.v2`

## Decision

Paper Raid is the primary Hepta Research League cooperative mode. A Research
Cell contains three through five human players. Every player binds one external,
independently keyed Agent, and the Cell produces one jointly approved,
submission-ready paper plus its reproducibility evidence. The first playable
golden path fixes three humans, three external Agents, one public-data baseline
reproduction, one small extension or ablation, and a five-to-eight-page short
paper. The persisted contracts support three, four, and five members from the
start.

`PaperBundleV2`, not a PDF alone, is the jointly signed research product. It binds the
paper source and buildable PDF, bibliography, Claim–Evidence graph, code/data/
environment manifests, all experiment and figure lineage, negative results,
reproduction reports, CRediT contribution ledger, ethics/COI/license/AI
disclosures, and explicit human authorship signatures. It deliberately does
not contain Nakama roots, evaluation, appeal, or Chain finality: putting those
later facts into the artifact that the terminal Nakama event already names
would create a hash cycle. The terminal event binds the immutable
`paper_bundle_hash`; after the session, Hepta signs an outer
`PaperRaidEvidenceEnvelopeV1` that binds that hash to the Nakama receipt and
roots, frozen ruleset/challenge, evaluation, reproduction, optional appeal,
and optional verified finality. External submission is never automatic; it
requires a separate `PublicationReleaseV1` signed by every human author.

## Human and Agent authority

Humans retain authority over research scope, ethics, licenses, authorship,
author order, factual responsibility, and external publication. An Agent may
search, propose, draft, run tools, review, and submit signed artifacts; it is
not a default author and cannot accept those human responsibilities.

Consumer-facing writes require a short-lived Ed25519 Consumer Edge assertion.
The assertion binds issuer, audience, OIDC subject, separate Nakama UUID,
Hepta player ID, operation, HTTP method, canonical path, canonical body hash,
idempotency nonce, issue time, and expiry. Caller-provided identity fields do
not override it.

Paper Raid Agent onboarding is independent of the legacy v1 Agent registry.
`CreateAgentBindingRequest` carries the Agent Ed25519 public key and a proof
over `hepta.paper_raid.agent_binding_proof.v2`: binding UUID, Agent ID,
key ID, raw public key and hash, Consumer-asserted subject, Hepta player UUID,
nonce, issue time and expiry. The key ID equals the SHA-256 public-key digest,
the proof nonce equals the operation idempotency key, and an accepted
`(agent_id, nonce)` is durably single-use. A legacy self-claimed `owner_id` or
legacy public key is never an authority for a v2 binding or Nakama admission.
The Paper Raid authorization path reads the immutable key snapshot from the
v2 binding only.

Agent key rotation is a separate v2 command. Its canonical frame binds the
rotation and binding IDs, expected binding version, player, asserted subject,
Agent ID, both key IDs/raw public keys/hashes, nonce, issue time and expiry.
The current and replacement Agent keys both sign those identical bytes. A
no-op, signature substitution, stale version, cross-binding scope, or reused
nonce fails closed. PostgreSQL atomically updates the live binding while
retaining the signed rotation record, outbox event and idempotent response;
authorization epochs already issued with the old key remain immutable and
auditable. A later Nakama replacement epoch reads only the rotated binding.

The only onboarding discovery reads are assertion-scoped:
`GET /v2/hepta/players/me` returns the exact stored player whose player,
subject and Nakama UUID all match the assertion, and
`GET /v2/hepta/agent-bindings` returns only that player's bindings. There is
no caller-selected player filter.

Onboarding idempotency is deliberately response-loss safe. Hepta always
revalidates canonical assertion/proof bytes, signatures, operation, path,
nonce and request hash. If the same operation/idempotency key/request hash was
already committed, Hepta returns the original status and response even after
the assertion or proof validity window has elapsed. Time-window checks are
skipped only after finding that exact applied ledger record; an unapplied
expired request still fails. The player or binding mutation, Agent proof
nonce, outbox event and idempotent response are committed atomically.

A team creator only proposes a roster. Every proposed member must separately
sign `hepta.paper_raid.team_member_acceptance.v2`, binding the exact team,
challenge, roster version, participant slot, human player, Agent binding,
Agent ID, research role, and frozen Collaboration Compact hash. The service
stores an immutable human key ID/public-key snapshot and verifies every
signature again before locking. Missing, duplicate, stale, or tampered
acceptances prevent team lock and therefore prevent Nakama authorization.
Author order is an explicit governance decision and is never derived from a
Nakama participant slot or contribution score.

Every human signing key is registered with proof-of-possession. Rotation is
dual-signed by the old and new keys; compromise revocation is separately
signed and retained in immutable key history. Rotation or revocation
supersedes any still-forming team acceptance so the member must accept the
exact roster again with the current key. A compromise revocation discovered
after authorship signing places the paper in `integrity_hold`; it never
silently converts a historical signature to a new key.

## Independent state machines

The three lifecycles are deliberately separate:

1. Research: `forming → preregistering → researching → experimenting →
   drafting → integrity_review → reproducing → author_approval →
   submission_ready`, with controlled returns to drafting and an explicit
   `integrity_hold` branch for compromised author evidence.
2. Nakama collaboration: `lobby → ready → active → checkpointed → completed`
   (or abandoned), with authoritative ordered events, reconnect and roster
   replacement.
3. Settlement: `uncommitted → pending_finality → finalized`, with challenged
   and resolved branches supplied by the Chain integration.

A completed Nakama room does not imply a completed paper, and a completed paper
does not imply economic finality. A paper remains downloadable and manually
submittable when Chain is unavailable, while ranking and economic rewards stay
at `pending_finality`.

## Storage and authority boundaries

- Consumer Entry `/league` is a BFF and renderer. It maps user identity and
  builds read models; it is not a scientific fact writer.
- Hepta owns durable, versioned research facts: people, Agent bindings, team
  consent, PaperProject, work, revisions, evidence manifests, authorship,
  reviews, reproduction, appeals, and finality projections.
- Nakama is the sole real-time session authority for 3–5-member admission,
  presence, action ordering, reconnect, replay, roster epochs, and completion.
- Paper/code/data/log bytes live in content-addressed Git or object storage.
  Hepta stores canonical manifests, hashes, URIs and ACLs; Nakama events carry
  typed actions and hashes, not document bodies.
- Chain stores typed evidence/workload/research/license/dispute commitments and
  verified finality receipts. It is neither a paper store nor an arbiter of
  scientific truth.

All v2 durable aggregates use normalized PostgreSQL tables, an explicit
positive version, expected-version writes, operation-scoped idempotency, and a
transactional outbox. No v2 write uses the legacy singleton JSON state or the
legacy service-wide advisory lock.

## Nakama admission and completion

Hepta issues one complete ordered authorization set for exactly one 3–5-member
session roster epoch. The immutable Hepta `team_roster_version` and the
Nakama `session_roster_version` are distinct: the former governs membership
consent; the latter starts at one for the logical room and increments on each
valid reconnect replacement. The set binds Hepta/Nakama identities, Agent DID/key snapshot,
role, challenge, paper, ruleset, challenge snapshot and roster root. Nakama
must consume the complete ordered set atomically. Partial, mixed-root,
mixed-version, expired, repeated, or concurrently duplicated live epochs are
rejected.

A replacement supersedes the previous complete epoch and may change exactly
one declared disconnected slot's Agent key while preserving the human,
binding, Agent ID, role, team, paper, challenge, ruleset and challenge
snapshot. All authorization IDs are fresh and every member receives a fresh
authorization. Nakama consumption receives a signed
`AuthorizationSetConsumptionReceiptV1`; an unsigned authorization-set echo is
not an acknowledgement.

Nakama completion ingestion requires the inline signed
`trnm.research-session.completed.v1` object and the full ordered authoritative
event archive. Hepta looks up `authority_key_id` only in its locally pinned
`TRNM_NAKAMA_AUTHORITY_KEY_ID` / `TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64`
trust configuration; a key returned by Nakama is never accepted as its own
trust anchor. Hepta independently verifies the signature, every event hash and
canonical event ID, event Merkle root, archive hash, event count, commitment
ID, terminal event and terminal facts. It then binds them to the exact consumed
authorization epoch and finalized PaperBundle before storing a normalized
receipt and outbox event.

Hepta returns a separately signed `NakamaCompletionReceiptV1` only after those
checks. Its canonical bytes bind the exact session epoch, event/roster/archive
roots, frozen ruleset/challenge snapshot, Nakama authority key, terminal
facts, and verification time. This signed receipt is the sole Nakama-to-Hepta
completion acknowledgement used by the outer evidence envelope.

The canonical cross-language research-session fixture is generated by Nakama
and vendored byte-for-byte at
`docs/sdk-fixtures/trnm-nakama-research-session-golden-vectors-v1.json`.
Hepta independently recomputes it and, when the sibling canonical checkout is
present, also requires byte identity. Hepta owns a separate single canonical
fixture for Consumer assertion, Agent binding proof, release candidate, authorship consent,
PaperBundle, signed authorization consumption, signed completion, outer
evidence envelope and publication release contracts at
`docs/sdk-fixtures/hepta-paper-raid-v2.json`, verified by Rust and an
independent Node.js implementation with tamper negatives.

## Finality mode

`HEPTA_FINALITY_MODE` is explicit:

- `pending_only`: paper work and signed PaperBundle completion continue, but
  every finality ingest/verifier path fails closed and no ranking/economic
  reward is released.
- `verified`: at least one pinned Chain validator set is mandatory and only a
  canonical typed ingress plus independently verified finality receipt can
  advance settlement.

HTTP success, a bearer token, or a locally fabricated receipt is never Chain
finality. Appeals or evidence disputes keep settlement on hold.

## Deferred work

Season, Guild, Loot, PvP, character-level CRDT editing, automatic external
submission, and World expansion are deferred until this vertical slice can
produce a reproducible three-author PaperBundle from a clean deployment.
