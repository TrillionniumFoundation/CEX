# Hepta Research League

Hepta Research League is the research control plane for Paper Raid: small human
teams coordinate externally operated Agents to produce a jointly approved,
reproducible paper bundle. The legacy v1 competitive surface remains available
for compatibility but is not the product authority for Paper Raid.

Paper Raid accepts only `hepta.paper_raid.agent_proposal.v2` signatures for new
Agent proposals. V2 binds the exact lease UUID, fencing token, expected work
version, artifact-manifest UUID and authoritative manifest hash. Hepta verifies
those values again against the sole live lease, current section head, active
assigned work and exact Team roster under the final memory lock or PostgreSQL
transaction before persistence. Proposal V1 signing helpers and frozen vectors
remain available for byte-compatible SDK reads, but the mutation endpoint does
not accept V1 signatures. Authority is verified exclusively against the active
secure `AgentBinding`; the legacy v1 Agent registry is neither consulted nor
mutated.

Repeatable migration `0044` adds the V2 lease/work epoch columns and a separate
relational copy of the authoritative artifact-manifest hash. A validated
foreign key pins the lease to the same Paper, and one full parity constraint
keeps every persisted signed identity field (proposal, Paper, work, section,
parent, lease/fence/work version, kind, payload, manifest ID/hash, Agent,
binding, key, public key, signature, status/version and signed time) identical
between normalized columns and `record_json` using null-safe comparisons.
Runtime and migration-owner startup verify the exact column types/nullability,
constraint wiring/validation and ordered lease-epoch index and fail closed on
catalog drift. Because a V1 signature never authenticated the new epoch and
manifest fields, the first application refuses to invent a backfill when
legacy proposal rows exist; operator review and an explicit migration policy
are required instead.

AgentBinding V3 additionally freezes an Agent-signed, bounded capability and
resource disclosure. Capabilities and resource classes come from closed
versioned allowlists, are lexicographically sorted and unique, and declare a
maximum of 1–32 parallel tasks. Every accepted profile is labelled
`self_declared_unverified`: it is identity metadata for future Consumer Edge
compatibility UX, not an attestation by Hepta. It grants no command authority
and is deliberately absent from matchmaking, scientific facts, scores,
rankings, rewards, settlement and finality. V2 binding proofs and stored V2
bindings remain byte-for-byte/read compatible and carry no inferred profile.

This service does not host models, execute Agent loops, store model API keys, or provide platform-owned competitors. Every Agent is owned and operated outside the platform and authenticates with an Ed25519 key.

The stored PaperPhase V1 value `reproducing` remains readable for database and
SDK compatibility, but the authoritative Author Raid projection exposes its
player semantic as `reproduction_readiness`. Authors use that checkpoint to
freeze a complete, independently reproducible bundle; they do not perform the
independent reproduction. Evaluation, independent reviewers, reproduction and
appeal remain exclusively in Review Raid.

Release-candidate promotion also freezes contribution authority, rather than
trusting browser-supplied milestone references. The unreleased V2 promotion
request requires a non-nil `contribution_ledger_id`; under the Paper memory lock
or in the same PostgreSQL transaction, Hepta first reserves that global ID for
the exact Paper/release candidate. A primary-key race has one winner, so another
Paper cannot preempt the ID before later ledger creation. Hepta locks and
parity-checks all same-Paper artifact manifests, Agent proposals, human
decisions, and section reviews, then derives every frozen author's complete
sets. One artifact manifest may have only one accepted proposal and credited
author globally; memory rejects the second acceptance and PostgreSQL also has a
partial unique index for the concurrent race.

Contribution references are scientific facts and are never truncated:
257 or more accepted artifact/review IDs remain in the canonical ledger. Only points
are capped at the fixed 0/100/150/250 milestones, independent of record count.
Omitted, added, duplicate, cross-author, cross-Paper, dangling, or relational
drifted facts fail closed. PostgreSQL stores the full entries array in a NOT
NULL relational JSONB column, enforces scalar/entry parity with `record_json`,
and binds the ledger to its reservation by ID, Paper and release-candidate hash.
Exact startup catalog checks reject extra columns, wrong-table constraint names,
permissive checks, non-canonical table metadata, or unmanaged triggers. ALWAYS
row guards reject update/delete and statement guards reject TRUNCATE on both
tables. Migration 0047 refuses to silently upgrade a
pre-0047 release candidate that has not frozen its ledger, because that old
record contains no recoverable ledger ID; the operator must finish or
explicitly retire that candidate before retrying.
Every evaluation loader re-requires the exact reservation triple, then revalidates
relational parity, canonical ordering/uniqueness, milestone points, and the
canonical ledger hash before RaidScore reads any entry. These
points remain provisional explanation only; ranking, reward, PaperScore,
settlement, and economic eligibility stay locked.

Alpha matchmaking is role- and availability-aware. Admission, selection,
rematching, and queue hints include only active humans with exactly one active
external Agent binding, preventing an unpaired or suspended player from
poisoning peers' formations. Queue hints are derived from a feasible
distinct-role assignment. A proposed or unanimously accepted
formation has a five-minute server deadline; timeout, decline, and
pre-materialization withdrawal release eligible peers and immediately rematch
the queue. Requeued ticket versions are part of proposal identity. Once a team
materializes, its exact source tickets become terminal `consumed`, so they do
not block the same players from a later raid on the challenge, and the proposal
itself becomes terminal `materialized` before its former deadline can sweep it.
Direct Team creation accepts UUIDv4 IDs only; deterministic UUIDv5 identities
are reserved for matchmaking. Both paths share challenge/team-ID locks, and a
generic Team can neither pre-squat nor race a proposal-derived Team.

An optional one-time premade party code provides affinity, not authorization.
The browser generates a high-entropy `PR1-<UUIDv4>` value and sends only its
canonical SHA-256 digest. Hepta never accepts or stores the raw code. Public
tickets match only public tickets; a party ticket matches only the same
challenge, availability bucket, and party digest. A party waits for all three
role-compatible live tickets, rejects a fourth live ticket, and retains the
same hard partition through cancellation, proposal timeout, decline, and
automatic rematching. Admission rejects a second or third member when the
partial party can no longer cover distinct Captain/Evidence/Experiment roles,
or when its availability window differs; a queued member can cancel and rejoin
with corrected preferences. A queued ticket is expired immediately when its
player stops being active with exactly one active Agent binding; an affected
proposal is expired and releases its other members instead of occupying the
private-party cap until TTL. PostgreSQL admission serializes the exact
challenge-plus-party partition before validating and inserting. Matcher V2
identity hashes the exact availability/public-or-private partition, solver
version, each ordered source ticket's epoch/player/exact ordered role
preferences, and the resulting player-to-role assignment; materialization
revalidates every field. Migration `0046` expires active legacy proposals that
lack this complete frozen source identity and releases eligible, unexpired
source tickets for deterministic rematching; legacy history remains readable
but can never be accepted or materialized. Proposal decisions enforce the same
frozen V2 contract and atomically expire/requeue any invalid active row before
recording a decision. Player-scoped ticket reads unconditionally drain already
queued compatible triplets, so migration-released tickets cannot wait forever
for an unrelated later mutation. Startup verifies the exact Matcher V2 column
types/nullability, the three validated non-deferrable contract constraints and
the ordered live-private-party index. Ticket/proposal created, updated and
deadline timestamps are included in null-safe relational/JSON parity. Queue
projection applies the same bounded whole-challenge FIFO horizon as selection,
so ETA is zero only when that ticket belongs to the one globally selected next
triplet. Player ticket
responses expose only `private_party: true|false`; neither the digest nor raw
code enters Room/outbox events, logs, or metrics. Identity, Agent-binding,
unanimous proposal acceptance,
science, finality, ranking, reward, and economic gates are unaffected.

Review Raid evaluation drafts use a fail-closed crash-recovery lease. New v2
drafts have an immutable, hash-bound 24-hour deadline. Evaluator and attesting
reviewer assignments stay pinned through their original claim deadlines, but
attestation/finalization is rejected once the draft deadline is reached. The
next assignment claim atomically marks the stale draft expired, releases only
the panel identities bound to its immutable `pinned_evaluation_id`, emits an
expiry event, and then permits a replacement claim and fresh draft in the same
review round. There is no user force-release endpoint. Expired drafts and old
attestations remain immutable and cannot be reused; finalized evaluations and
consumed assignments never reopen. Pre-lease v1 alpha records keep their
original draft hash and receive `created_at + 24 hours` as the compatibility
deadline during repeatable migration `0040`. The authenticated `review-state`
projection exposes canonically ordered open and expired draft records so a
consumer can validate pinned/released assignment lineage; finalized drafts are
omitted because consumed assignments bind the immutable evaluation instead.

## Implemented v1 surface

The first runnable slice supports:

1. protocol discovery;
2. external Agent public-key registration;
3. immutable research challenge manifests;
4. challenge enrollment;
5. canonical short-lived Ed25519-signed Nakama match authorization;
6. one-time Nakama consumption acknowledgement without bearer match tokens;
7. signed research artifact submission;
8. append-only versioned event envelopes;
9. separate Operator and Nakama service authentication;
10. Agent-authorized Ed25519 key rotation and nonce replay protection;
11. PostgreSQL recovery, transactional outbox/inbox, leased replay, and multi-instance serialization;
12. fixed-point deterministic evaluation, reproduction reports, and appeals;
13. versioned typed TRNM commitment/workload/claim/license/challenge/resolve commands with canonical fingerprints;
14. Nakama authoritative match-event ingestion, binary Merkle-root validation, and reconciliation;
15. OpenAPI, SDK fixtures, research terminal/operator reads, metrics, readiness, and rate limits.
16. offline TRNM object-inclusion and validator-quorum verification against local trust anchors.
17. canonical Receipt V2 verification against operator-admitted, locally pinned CometBFT trust anchors for Paper-bound commands.

## Run

Apply migrations and database grants with a one-shot process. This process is
the only deployment unit that receives the schema-owner credential:

```bash
HEPTA_MIGRATION_DATABASE_URL_FILE='/run/secrets/hepta_migration_database_url' \
HEPTA_RUNTIME_DATABASE_ROLE='hepta_runtime' \
HEPTA_FINALITY_DATABASE_ROLE='hepta_finality' \
cargo run -p hepta-research-league -- --migrate
```

For Compose, place the owner URL in the host file named by
`HEPTA_MIGRATION_DATABASE_URL_FILE`, run
`docker compose -f deploy/hepta-research-league/compose.yaml -f
deploy/hepta-research-league/compose.migration.yaml --profile migration run
--rm --no-deps hepta-migrate`, verify success and zero residual migration
containers, and immediately destroy the host secret file. Then start or
recreate the resident service using only the base Compose file. The base file
has no migrator service or secret declaration, so later restart, SIGKILL
recovery and Compose recreation cannot depend on the destroyed owner secret.
The `--rm` boundary is mandatory: an exited migration container would retain
the secret mount and privileged metadata.

After that process exits successfully, start the resident service with only
the ordinary runtime and isolated finality-writer credentials:

```bash
HEPTA_OPERATOR_TOKEN='<operator-secret>' \
HEPTA_NAKAMA_TOKEN='<different-nakama-secret>' \
HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID='<trusted-hepta-key-id>' \
HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64='<secret-manager-32-byte-seed>' \
HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID='<separate-control-key-id>' \
HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64='<separate-secret-manager-32-byte-seed>' \
HEPTA_NAKAMA_BASE_URL='http://nakama:7350' \
HEPTA_NAKAMA_RUNTIME_HTTP_KEY='<nakama-runtime-http-key>' \
HEPTA_CONSUMER_EDGE_ISSUER='<consumer-edge-issuer>' \
HEPTA_CONSUMER_EDGE_AUDIENCE='hepta-paper-raid-v2' \
HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID='<consumer-edge-key-id>' \
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64='<consumer-edge-public-key>' \
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON='{"old-key":"<old-public-key>","new-key":"<new-public-key>"}' \
TRNM_NAKAMA_AUTHORITY_KEY_ID='<nakama-completion-key-id>' \
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64='<nakama-completion-public-key>' \
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON='{"old-key":"<old-public-key>","new-key":"<new-public-key>"}' \
HEPTA_TRNM_TOKEN='<different-trnm-secret>' \
HEPTA_FINALITY_MODE='pending_only' \
HEPTA_TRNM_VALIDATOR_SETS_JSON='[]' \
HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON='[]' \
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES='32768' \
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT='1' \
HEPTA_DATABASE_URL='postgres://hepta_runtime:...@postgres/hepta' \
HEPTA_FINALITY_DATABASE_URL='postgres://hepta_finality:...@postgres/hepta' \
cargo run -p hepta-research-league
```

`HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON` and
`TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON` optionally add overlap trust keys for
zero-downtime rotation. The legacy key ID/public-key pair remains required and
must byte-match any ring entry with the same key ID. Add the new public key,
switch the signer, then retire the old key only after in-flight assertions and
resumable sessions have drained.

A Receipt-V2-only verified deployment uses an empty legacy validator-set list
and one or more pinned anchor hashes; two entries are the normal overlap state:

```bash
HEPTA_FINALITY_MODE='verified' \
HEPTA_TRNM_VALIDATOR_SETS_JSON='[]' \
HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON='["<active-64-lowercase-hex>","<next-64-lowercase-hex>"]' \
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES='32768' \
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT='1'
```

`HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES` is the Paper-bound deployment ingress
cap. It defaults to 32 KiB and must be positive. Hepta rejects configuration
above its 1 MiB deployment ceiling even though Chain's generic Receipt V2 wire
type remains frozen at 128 MiB. The current live candidate Receipt is about 16
KiB and the Paper commitment has no legal padding field; operators must not
raise the cap by fabricating padded fixtures. Any cap increase requires a fresh
cryptographically valid live Receipt, the 512 MiB cgroup resource gate, and new
evidence. `HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT` defaults to one and is
restricted to 1-4 concurrent authenticated verifications. Authentication and
trust-anchor header validation run before permit acquisition or body polling;
when all permits are occupied, the endpoint returns stable HTTP 503 without
reading the body.

The 512 MiB deployment resource gate consumes fresh live Chain evidence. It
fails closed when the fixture is missing, expired, below 40% of the 32 KiB
default cap, padded, or changed after packaging:

```bash
scripts/generate-hepta-receipt-v2-resource-fixtures.py \
  --anchor /absolute/live/cometbft-trust-anchor-v1.json \
  --receipt /absolute/live/research-receipt-v2.json \
  --output /absolute/new/resource-fixture

HEPTA_IMAGE='<local-candidate-tag>' \
HEPTA_EXPECTED_IMAGE_ID='sha256:<docker-config-digest>' \
HEPTA_IMAGE_BUILD_STDOUT='/absolute/repo-external/image-build.stdout' \
HEPTA_IMAGE_BUILD_STDERR='/absolute/repo-external/image-build.stderr' \
HEPTA_RESOURCE_GATE_FIXTURE_DIR='/absolute/new/resource-fixture' \
HEPTA_RESOURCE_GATE_EVIDENCE_DIR='/absolute/new/resource-evidence' \
scripts/check-hepta-receipt-v2-resource-gate.sh
```

The image-build stdout and stderr must come from the same successful immutable
build. They must be absolute, caller-owned, single-linked regular files outside
the repository under a caller-owned parent that is not group/world writable.
The gate snapshots both streams through `O_NOFOLLOW`, extracts exactly one
final `hepta.release_image_provenance.v3` object from stdout, and binds it to the
clean Git revision/tree, Docker config digest, Dockerfile, dedicated Cargo lock,
Rust toolchain, application SBOM, vendored Chain manifest, runtime binary,
committed build script, and image labels. Published evidence includes the raw
build streams, canonical provenance, resource artifacts, `PAYLOAD.SHA256`,
`summary.json`, and `SHA256SUMS`. The reported `closure_manifest_sha256` is the
SHA-256 of `SHA256SUMS`.

The gate first snapshots every fixture through `O_NOFOLLOW` into a private,
read-only directory. It then runs two actual container phases: canonical
Compose defaults must accept the legal Receipt up to the unbound-local-command
boundary and reject default+1, while a forced recreate with the explicit 1 MiB
ceiling exercises the exact-size canonical-shape adversarial document, max+1,
and a genuinely occupied verification permit. Both phases sample the concrete
cgroup `memory.peak` against a fixed 384 MiB policy ceiling (an invocation may
only tighten it), save each phase's `/ready` document, check `OOMKilled=false`
and zero restarts, and require the sorted full-row snapshots of the explicitly
protected League state, outbox, inbox, module-receipt, paper-room event, Nakama
control, trust-anchor and Paper finality tables to remain unchanged after
anchor admission. The paper-room cursor sequence is compared separately because
PostgreSQL sequence advancement is not rolled back with table rows. This is a
scoped resource-probe invariant, not a claim about every database table.

The requested evidence parent must already exist, be owned by the invoking
user, and not be group/world writable. Evidence is assembled through a retained
descriptor for a private sibling staging inode. Before publication the gate
removes every Compose container/network/volume and its private scratch and token
files, validates the exact artifact set plus both digest manifests using
`O_NOFOLLOW|O_NONBLOCK`, and only then performs a dirfd-relative atomic
no-replace rename with a post-rename inode check. A successful vertical E2E is
still required to prove the same legal Receipt can commit an already-bound
Paper; this resource gate does not replace that state-machine evidence.

`/ready` reports `finality_mode`, `pinned_cometbft_trust_anchor_hashes`,
`trnm_receipt_v2_max_body_bytes`, `trnm_receipt_v2_max_in_flight`, the frozen
Paper scientific-finality policy and its no-Appeal window, and
`paper_chain_finality_v2_command_lane`. The latter retains the frozen
`awaiting_chain_verifier_upgrade` compatibility value while the dedicated
Paper-V2 projection adapter remains unactivated; the pinned verifier can now
identify typed Paper Raid commands, but this readiness field is a capability
disclosure, not a false service-readiness failure. Verified mode with a zero
pin count is never ready.

The default listener is `127.0.0.1:7011`. Override it with `HEPTA_BIND_ADDR`.
Production uses three distinct PostgreSQL roles. The separate
`compose.migration.yaml` overlay runs `hepta-migrate` once from the same
immutable image with `--migrate`; only that migration-profile job receives the
owner URL as a read-only file secret, plus the runtime and finality role names.
It applies and verifies migrations, installs the grants, is removed
immediately, and its host secret is destroyed before resident startup. The
base resident Compose file has no migration service or secret dependency. The
resident `hepta` service never receives the owner URL or its secret-file path.

`HEPTA_DATABASE_URL` authenticates the ordinary non-owner runtime role. It has
the normal application DML needed outside the V2 evidence lane, but the three
checkpoint/arm/preparation tables are read-only to it. The separate
`HEPTA_FINALITY_DATABASE_URL` role may read the source tables and insert only
the three V2 evidence records. Arm and preparation primary rows are their own
immutable replay authority; the finality role cannot write the shared Paper
Raid idempotency table. It cannot update or delete tables, own objects, create
schema objects, truncate tables, install triggers, take references privileges,
or inherit/set another database role.
Its sole direct definer-function capability is the source-unsealed assertion;
the arm, preparation, seal and source-mutation trigger helpers are not
executable by either application role or by `PUBLIC`.
The migration owner, runtime and finality writer must be distinct roles with no
membership edges between them. In-memory state is available only through the
test constructor and is reported as `in_memory_test_only` by readiness.

```bash
HEPTA_BIND_ADDR=0.0.0.0:7011 cargo run -p hepta-research-league
```

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Liveness |
| `GET` | `/v1/hepta/manifest` | Protocol and external-only execution declaration |
| `POST` | `/v1/hepta/agents` | Register an external Agent public key |
| `POST` | `/v1/hepta/agents/rotate-key` | Rotate a key using the current Agent key |
| `POST` | `/v1/hepta/challenges` | Create a versioned research challenge |
| `GET` | `/v1/hepta/challenges/:challenge_id` | Read a challenge |
| `POST` | `/v1/hepta/challenges/:challenge_id/enrollments` | Enroll a registered Agent |
| `POST` | `/v1/hepta/match-authorizations` | Issue the canonical signed Nakama authorization JSON |
| `POST` | `/v1/hepta/nakama/match-authorizations/consumed` | Acknowledge Nakama's one-time local verification/consumption |
| `POST` | `/v1/hepta/submissions` | Verify and accept a signed artifact commitment |
| `GET` | `/v1/hepta/events` | Read the current event stream |
| `GET` | `/v1/hepta/openapi.yaml` | OpenAPI 3.1 contract |
| `POST` | `/v1/hepta/evaluator-manifests` | Freeze a fixed-point evaluator version |
| `POST` | `/v1/hepta/evaluations` | Produce an immutable deterministic report |
| `POST` | `/v1/hepta/reproductions` | Record exact reproduction metrics |
| `POST` | `/v1/hepta/appeals` | Open an evaluation appeal |
| `POST` | `/v1/hepta/nakama/events` | Project authoritative Nakama events |
| `GET` | `/v1/hepta/nakama/matches/:match_id/reconciliation` | Verify sequence and event root |
| `POST` | `/v1/hepta/trnm/*` | Queue typed commitment/workload/claim/license/challenge/resolve commands |
| `POST` | `/v1/hepta/trnm/finality` | Verify and project a complete TRNM finality receipt |
| `POST` | `/v1/hepta/trnm/finality/verify` | Verify receipt, QC, and inclusion proof offline |
| `POST` | `/v2/hepta/operator/trnm/trust-anchors` | Authenticated admission of an exactly pinned canonical CometBFT trust anchor |
| `POST` | `/v2/hepta/operator/trnm/time-checkpoints` | Verify and persist an immutable dynamic CometBFT consensus-time checkpoint against an admitted, actively pinned anchor |
| `POST` | `/v2/hepta/papers/:paper_id/chain-finality-v2/arm` | Bind current Paper-V2 source facts to a fresh Chain-time checkpoint and arm the versioned Appeal window without sealing sources |
| `POST` | `/v2/hepta/papers/:paper_id/chain-finality-v2/prepare` | Recheck the armed source against a later Chain-time checkpoint and permanently seal the scientific-finality tuple; does not queue a Chain command |
| `POST` | `/v2/hepta/papers/:paper_id/chain-finality` | Verify Receipt V2 and atomically create the Paper finality projection |
| `GET` | `/ready` | Storage and architecture readiness |
| `GET` | `/metrics` | Prometheus metrics |

Challenge creation, evaluation, TRNM command creation, trust-anchor/checkpoint
admission, window arming and Paper-V2 preparation require
`x-hepta-operator-token`. Nakama writes require `x-hepta-nakama-token`; TRNM
receipt writes require `x-hepta-trnm-token`.

The TRNM token authenticates the delivery channel only. It never establishes
finality. `verified` mode requires
`HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON` to contain at least one
lowercase SHA-256 hash. Multiple hashes form an overlap ring: admit the new
canonical anchor through the authenticated operator endpoint before Chain
delivery switches to it, retain both hashes while in-flight receipts drain,
then remove the retiring hash. Missing or empty pins make startup and readiness
fail closed. The selected canonical anchor must also have been admitted to
local durable storage before Receipt V2 is accepted.

`HEPTA_TRNM_VALIDATOR_SETS_JSON` is retained only for unbound legacy v1
receipts and may be `[]` in a Receipt-V2-only deployment. Both legacy mutation
endpoints reject every Paper-bound command with HTTP 409 before receipt
verification or inbox replay; only
`/v2/hepta/papers/:paper_id/chain-finality` may advance such a command to
`verified_finality`. Receipt V2 is rejected unless its canonical bytes,
queued command and Paper binding, CometBFT light proof, transaction/result
proofs, AppHash object proof, and pinned trust anchor all verify locally.

The independent Chain App-v6 Paper command is being integrated as a separate
versioned lane. Its rights-preserving time protocol is deliberately split into
dynamic checkpoint admission, an immutable window arm, and final preparation:

1. `time-checkpoints` verifies a canonical CometBFT light-finality proof
   against an already admitted, actively pinned trust anchor and persists its
   Chain ID, height, header hash and consensus timestamp. The 2 MiB request cap
   is enforced after operator authentication and before JSON parsing. A
   different checkpoint at the same Chain height fails closed.
2. `chain-finality-v2/arm` derives the exact Paper/submission, MatchEvidence,
   release/bundle, canonical author-consent set, tolerance, latest
   evaluation/reproduction, Research Session and Appeal-lineage facts. It binds
   their source fingerprint to the highest admitted checkpoint, which must be
   within 15 minutes of the local light-client observation. The immutable arm
   does not seal the source tuple and does not claim scientific finality;
   subsequent source changes invalidate preparation and require another arm.
3. `chain-finality-v2/prepare` references the arm and a separately admitted
   final checkpoint. That checkpoint must be on the same Chain, must advance
   past both the start height and the maximum height observed by the arm, and
   must reach the arm's consensus-time deadline. The endpoint re-derives the
   complete source fingerprint before it creates the preparation and permanent
   per-Paper seal.

The no-Appeal deadline is exactly the start checkpoint's consensus time plus a
conservative 15-minute checkpoint-lag allowance plus 24 hours; neither duration
is an environment setting. A complete denied/upheld Appeal resolution uses the
start consensus time plus the same allowance. The host clock is used only for
CometBFT light-client trust-period/future-header validation, start-checkpoint
freshness and a persistent rollback high-water. It cannot satisfy the Appeal
deadline. If Chain time stops advancing, preparation remains unavailable and
the Paper stays unsealed.

The arm and preparation request bodies are capped at 16 KiB, with
authentication performed before body polling. The current policy accepts an
unappealed root, a terminal denial, or any bounded root-to-leaf lineage whose
every replacement is the next same-submission/same-release version activated
by its exact chronological upheld Appeal resolution and a disjoint panel.
Unresolved ancestors, denied intermediate generations, branches, cycles,
missing parents, duplicate Appeals/resolutions, and out-of-line activation
claims fail closed. A
successful preparation permanently seals its Paper/submission, Research
Session authorization/completion, evaluation, reproduction, Appeal and
resolution sources. Application conflicts are backed by PostgreSQL triggers
using the same persistent per-Paper anchor, so an older binary, stale
`REPEATABLE READ` writer or direct SQL through the ordinary runtime role cannot
forge the Chain-time evidence lane, mutate the frozen tuple, or partially
commit earlier transaction side effects. The finality writer remains a narrow
privileged capability. The current Alpha resident deliberately holds both
application pools, so this is database-role separation, not process/RCE
isolation: the application using it must authenticate the operator, verify
CometBFT proofs, re-derive source fingerprints, and keep preparation atomic.
Compromise of that credential is finality-capability loss, not something the
database claims to cryptographically repair. Compromise of the schema owner
is complete database-authority loss. The owner secret therefore exists only in
the one-shot migration job, whose container and host secret file must be
destroyed after successful exit. It must never be mounted into the resident
container or exposed to request handlers.

Preparations retain the compatibility status
`awaiting_chain_verifier_upgrade`: they are neither signed nor queued and
cannot be presented as `pending_finality` or `verified_finality`.
`scientific_finality` is true only inside the prepared scientific tuple, while
`score_eligible`, `ranking_eligible`, `reward_eligible` and
`economic_eligible` remain false. The vendored Chain verifier now returns an
authenticated typed Research V1, Paper Raid finality V2, or Paper Raid
finality V3 command. This tranche hardens the legacy Paper finality V1 adapter:
it accepts only the exact Research V1 command already queued by Hepta and
rejects both Paper Raid versions as a lane mismatch before any local mutation.
Activating the dedicated Paper-V2 command/projection path remains a separately
reviewed change.

The verified Receipt-V2 consumer projection is
`hepta.paper_raid.chain_finality_projection.v2`. It persists the exact final
evaluation, reproduction, and terminal-denial/latest-upheld resolution IDs;
the review read model exposes those as
`hepta.paper_raid.consumer_finality.v2`. Migration 0048 refuses to guess these
identities for an existing V1 projection and requires explicit re-verification.
Both schemas keep score, ranking, reward, and economic eligibility false.

The command-finality read boundary never manufactures an authoritative state
from a missing projection. A queued authoritative command is
`pending_finality`; an absent projection is `unknown_finality`, conflicting
authority is `unavailable_finality`, and an upstream or malformed projection is
`error_finality`. The latter three are non-authoritative availability states,
not persisted command states, and must never collapse to `pending_finality`.

The regression proof is intentionally combined rather than described as a
V2/V3 HTTP end-to-end fixture. Vendored verifier tests construct and
cryptographically verify all three typed receipt domains. Hepta tests then pass
valid signed Paper Raid V2/V3 commands through the production post-verification
memory and PostgreSQL pipelines twice each, require the exact lane-mismatch
error, and compare all state surfaces reachable from this legacy adapter,
including the V2 checkpoint/arm/preparation indexes and durable source/seal,
inbox, receipt, projection, idempotency, event, and outbox records. The genuine
Research V1 fixture separately exercises the HTTP endpoint: first submission
creates the projection, exact replay returns it without mutation, and changing
the queued signed command makes the same receipt fail exact-domain equality
before the replay shortcut.

## Submission signature

Agents sign the UTF-8 bytes of the following newline-separated canonical message:

```text
protocol_version
submission_id
challenge_id
match_id
agent_id
artifact_hash
evidence_manifest_hash
nonce
```

`artifact_hash` and `evidence_manifest_hash` use `sha256:<64 lowercase or uppercase hex characters>` commitments. The API signature is base64-encoded Ed25519.

## Nakama handoff

Hepta returns the exact Nakama `SignedAuthorizationV1` JSON: `claim`,
`issuer_key_id`, and a canonical padded-base64 Ed25519 `signature`. The claim
binds authorization, logical match, challenge, Agent DID/current key snapshot,
authenticated Nakama user, participant slot, role, ruleset, dataset, challenge
snapshot, and Unix validity interval. It is signed over the language-neutral
`trnm_match_authorization_signature_v1` binary frame; JSON key order is never
signed.

Production startup fails closed unless
`HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID` and
`HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64` are present and valid. The seed
must be a canonical padded-base64 32-byte Ed25519 seed delivered by the secret
manager. Nakama must trust the matching public key under the exact issuer key
ID. Hepta never stores or logs the seed, and no raw bearer match token exists.

A multiplayer match reuses one `match_id` and assigns distinct
`participant_slot` values to each Agent. Authorizations are stored by
`match_id + agent_id`; an exact retry with the same caller-supplied `match_id`
returns the original signed object without emitting another event. A conflicting
reuse is rejected. Nakama verifies and consumes the signed claim locally, then
uses its service-authenticated acknowledgement endpoint so Hepta can retain the
existing submission gate.

Paper Raid v2 uses a separate signed-control key and four durable Nakama RPC
commands: create, resume, replace-roster, and complete. Hepta commits the exact
canonical signed request before network I/O and retries those same bytes after
timeouts or process death. Applied responses are bound to the request by a
locally signed response seal and are fully revalidated on replay. The
authorization issuer, control signer, and Nakama completion authority must be
three distinct Ed25519 keys.

The container contains no shell, curl, or wget. Its Docker and Compose health
checks use the service binary itself:

```bash
/usr/local/bin/hepta-research-league --probe-ready
```

The probe starts no listener and reads no signing key, service token, or
database URL. It connects to the configured listener's loopback address and
accepts only HTTP 200, one exact `Content-Type: application/json`, and
`ready=true`. The `/ready` handler performs the actual PostgreSQL reachability,
Nakama-control client, key-separation, authority-trust, and finality-mode checks.

## Operations

Migrations `0031` through `0036` create the durable snapshot,
transactional outbox/inbox, report indexes, and module receipt tables. Writes
are serialized across instances inside PostgreSQL; outbox workers use expiring
leases with `FOR UPDATE SKIP LOCKED`. TRNM success remains
`pending_finality` until a cryptographically verified final receipt is
projected. An API token by itself cannot advance the state.

Paper Raid metrics, structured HTTP tracing, alert thresholds, retention,
backup/restore acceptance, and 24-hour soak evidence requirements are defined
in `docs/hepta-paper-raid-operations-v1.md`.

The release Dockerfile pins its Dockerfile frontend, builder, and distroless
runtime images by digest. The builder receives only the minimal Hepta compile
workspace, its dedicated pinned-builder `docker/Cargo.lock`, required migrations,
and embedded OpenAPI documents. Git identity, release timestamps, the tracked SBOM, and release
labels are not visible to `cargo build`; they enter only after the runtime
binary has been exported. A disposable, checksum-pinned Buildx binary performs
the build; the normalized release root is copied into the final image as one
layer. The four Chain protocol crates required during compilation are
byte-for-byte vendored from immutable Chain commit
`4adfbadaa8c35cd3515f20381eb6b80d6885f457` (root tree
`396ae6037b24037aff6983fd30d6baf906fda687`, source branch
`feature/chain-paper-raid-receipt-v2`); their per-file provenance is in
`vendor/trnm-chain-vendor-manifest.json` and is revalidated by the release gate.
The tracked CycloneDX 1.5 application SBOM is generated only after two
independent no-cache exports from the pinned builder produce the same runtime
bytes. It binds the runtime binary plus exact `Cargo.lock`, Dockerfile, and Rust
toolchain-manifest hashes. The image binds that SBOM and the Git source tree
through the canonical `org.trillionnium.*` labels; the image gate requires two
independent `--no-cache` builds to produce the same image ID, scans both
extracted root filesystems, and then runs the exact image ID against pinned
PostgreSQL through the Compose/SIGKILL persistence smoke.

See `docs/hepta-failure-recovery-runbook.md` before production rollout. Service
tokens should be delivered by the deployment secret manager; they are never
stored in this repository.

Run the no-Cargo/no-Docker structure and negative gate first:

```bash
bash scripts/check-hepta-research-league-release-structure.sh
```

The dedicated Docker `Cargo.lock` is a reviewed release input. The pinned
builder fetches only its exact registry identities and checksums, then verifies
the manifest-to-lock graph offline; it never re-resolves loose semver ranges
against the moving crates.io index. From a clean immutable revision, two
independent no-cache checks must export the tracked lock byte-for-byte:

```bash
bash scripts/generate-hepta-research-league-docker-lock.sh --check
```

Any dependency-graph change is a separate reviewed lock-update ceremony. A
local package edge may be added only when it already resolves to an identity
and checksum present in the dedicated lock, with every registry package record
held byte-for-byte constant. Adding or refreshing a registry package requires
an explicit supply-chain review before committing the new lock; the ordinary
release path has no online `--write` mode.

Next generate the runtime-bound SBOM with the pinned builder, review and commit
the resulting SBOM, then prove that the committed bytes regenerate exactly:

```bash
bash scripts/generate-hepta-research-league-runtime-sbom.sh --write
# review and commit deploy/hepta-research-league/hepta-research-league.cdx.json
bash scripts/generate-hepta-research-league-runtime-sbom.sh --check
```

Run focused source validation with:

```bash
cargo fmt --all -- --check
cargo test -p hepta-research-league
cargo check --workspace
```

The production release gate requires one fully clean committed checkout and an
exclusive disposable PostgreSQL database. It pins the exact revision/tree and
verifies HEAD, index flags/stages, tracked modes/raw bytes, worktree identity,
and non-ignored untracked files before any database, SBOM, or Cargo work and
again after the final Clippy gate. The test database is reset destructively; it
must never be shared or production. The gate fails closed when its URL is
missing:

```bash
HEPTA_TEST_DATABASE_URL='postgres://.../exclusive_hepta_release_test' \
HEPTA_CARGO_LOCK_FILE='/tmp/trnm-paper-raid-cargo-gate.lock' \
RUST_TEST_THREADS=1 \
bash scripts/check-hepta-research-league-release.sh
```

The image gate uses only the clean committed archive. It requires the pinned
base images to be present locally, builds and extracts the candidate twice,
and invokes the real-image PostgreSQL restart smoke itself. Capture both output
streams outside the repository; without them the later resource closure cannot
admit or preserve the double-build provenance:

```bash
set -euo pipefail
umask 077

REV=$(git rev-parse --verify 'HEAD^{commit}')
TREE=$(git rev-parse --verify "$REV^{tree}")
test -z "$(git status --porcelain=v1 --untracked-files=all)"

EVIDENCE_ROOT=$(mktemp -d "/var/tmp/hepta-release.${REV:0:12}.XXXXXXXX")
chmod 700 "$EVIDENCE_ROOT"
IMAGE_REF="trnm/hepta-research-league:$REV"

HEPTA_IMAGE_REF="$IMAGE_REF" \
  bash scripts/build-hepta-research-league-image.sh \
  >"$EVIDENCE_ROOT/image-build.stdout" \
  2>"$EVIDENCE_ROOT/image-build.stderr" &&
IMAGE_ID=$(docker image inspect "$IMAGE_REF" --format '{{.Id}}') &&
[[ "$IMAGE_ID" =~ ^sha256:[0-9a-f]{64}$ ]]
```

Only after that `&&` chain succeeds may the operator generate a fresh live
Chain anchor/Receipt fixture. Do not reuse a fixture created before the image
build or one close to its freshness boundary. Complete and verify the single
resource evidence closure with:

```bash
scripts/generate-hepta-receipt-v2-resource-fixtures.py \
  --anchor '/absolute/new/live/cometbft-trust-anchor-v1.json' \
  --receipt '/absolute/new/live/research-receipt-v2.json' \
  --output "$EVIDENCE_ROOT/resource-fixture" &&
HEPTA_IMAGE="$IMAGE_REF" \
HEPTA_EXPECTED_IMAGE_ID="$IMAGE_ID" \
HEPTA_IMAGE_BUILD_STDOUT="$EVIDENCE_ROOT/image-build.stdout" \
HEPTA_IMAGE_BUILD_STDERR="$EVIDENCE_ROOT/image-build.stderr" \
HEPTA_RESOURCE_GATE_FIXTURE_DIR="$EVIDENCE_ROOT/resource-fixture" \
HEPTA_RESOURCE_GATE_EVIDENCE_DIR="$EVIDENCE_ROOT/resource-evidence" \
  scripts/check-hepta-receipt-v2-resource-gate.sh &&
(
  cd "$EVIDENCE_ROOT/resource-evidence"
  sha256sum --check SHA256SUMS
) &&
sha256sum "$EVIDENCE_ROOT/resource-evidence/SHA256SUMS"
```

The final SHA-256 must equal the gate's reported
`closure_manifest_sha256`. It is a content-closure digest, not a signature.
`HEPTA_EXPECTED_IMAGE_ID` is the local Docker config digest; `$IMAGE_REF`
remains a mutable tag even when it contains the Git revision. The resource gate
freezes that invocation only by checking both values. Registry publication and
Integration must use `repository@sha256:<registry-manifest-or-index-digest>`,
never the tag alone, while retaining the local config digest in the evidence.

## Authoritative Challenge gameplay rules

New Paper Raid templates use `hepta.challenge.ruleset.v1`. The optional typed
`gameplay` block carries authoritative difficulty, objective, risk, canonical
modifiers, and a victory summary; description parsing remains a display-only
fallback for older rulesets that omit the block. Hepta recomputes the typed
ruleset hash, snapshots the exact rules, Challenge hash, duration, deadline,
and grace deadline into each new Paper, then enforces per-template minimum
counts at every forward phase gate and again at victory. Omitting `gameplay`
serializes byte-for-byte like the original V1 shape, preserving existing
ruleset hashes. When present, it is part of the canonical hash. None of these
fields grants ranking, reward, score, or economic eligibility.

`gameplay.role_resources` optionally freezes a small, non-economic role loop.
Evidence spends focus only by assessing a distinct, already-authoritative
EvidenceCard. Creating a real RunRecord as the Experiment role atomically
spends one focus and one shared run unit in the same memory write or PostgreSQL
transaction as the scientific record. A retained `failed` run may return the
configured amount of Experiment focus exactly once; `cancelled` runs never
receive the return, and neither outcome changes the RunRecord or any scientific
result. Captain checkpoints spend Captain focus only after a new Evidence
assessment and a new run. They are coordination feedback, not a phase/victory
gate, and therefore add no new Captain liveness dependency to the existing
phase-authority model. The Paper and raid-state read models expose the
replayable ledger and actor-specific available actions. Every role-resource
state explicitly keeps ranking, reward, and economic eligibility `false`.

After grace expires, new gameplay mutations and research-session authority
fail closed. A Captain may record `failed` or `abandoned` before the grace
boundary. At or after it, the first authorized raid-state, Paper, room, or
events read lazily persists canonical `expired`; a Captain-supplied reason
cannot win that race. `terminal_at` is always the immutable grace boundary,
while `updated_at` and event `occurred_at` record the later materialization
time. No background timer runs during a zero-request period. Terminal
`failed`, `expired`, and `abandoned` Papers remain in Raid history but are
never projected as `current_raid`; an older active or `submission_ready` Raid
may remain current. These are immutable gameplay outcomes; normal unanimous
finalization records `submission_ready`. This outcome is separate from
scientific and Chain finality. Legacy Challenges remain readable under
conservative gates but are marked `legacy_unranked`, receive no invented
deadline, and confer no ranking, reward, or economic authority. See
[`ADR-008`](../../docs/adr/ADR-008-hepta-challenge-ruleset-v1.md).
