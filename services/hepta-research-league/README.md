# Hepta Research League

Hepta Research League is the research control plane for Paper Raid: small human
teams coordinate externally operated Agents to produce a jointly approved,
reproducible paper bundle. The legacy v1 competitive surface remains available
for compatibility but is not the product authority for Paper Raid.

This service does not host models, execute Agent loops, store model API keys, or provide platform-owned competitors. Every Agent is owned and operated outside the platform and authenticates with an Ed25519 key.

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

HEPTA_IMAGE='<immutable-local-image-ref>' \
HEPTA_EXPECTED_IMAGE_ID='sha256:<docker-config-digest>' \
HEPTA_RESOURCE_GATE_FIXTURE_DIR=/absolute/new/resource-fixture \
HEPTA_RESOURCE_GATE_EVIDENCE_DIR=/absolute/new/resource-evidence \
scripts/check-hepta-receipt-v2-resource-gate.sh
```

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
authentication performed before body polling. The current policy represents
either no Appeal or exactly one fully resolved Appeal in the evaluation
lineage; unresolved ancestors and multiple resolutions fail closed. A
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

The production release gate requires a disposable live PostgreSQL database and
fails closed when the URL is missing:

```bash
HEPTA_TEST_DATABASE_URL='postgres://.../hepta_release_test' \
  bash scripts/check-hepta-research-league-release.sh
```

The final image/provenance gate uses only the clean committed archive. It
requires the pinned base images to be present locally, builds and extracts the
candidate twice, and invokes the real-image PostgreSQL restart smoke itself:

```bash
HEPTA_IMAGE_REF='registry.example/hepta-research-league:<immutable-revision>' \
  bash scripts/build-hepta-research-league-image.sh
```
