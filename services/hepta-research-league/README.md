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
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES='67108864' \
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT='1' \
HEPTA_DATABASE_URL='postgres://hepta:...@postgres/hepta' \
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
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES='67108864' \
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT='1'
```

`HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES` is the deployment ingress cap. It
defaults to 64 MiB, must be positive, and cannot exceed Chain's frozen 128 MiB
Receipt V2 wire maximum. `HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT` defaults to one
and is restricted to 1-4 concurrent authenticated verifications. Authentication
and trust-anchor header validation run before permit acquisition or body
polling; when all permits are occupied, the endpoint returns stable HTTP 503
without reading the body.

`/ready` reports `finality_mode`, `pinned_cometbft_trust_anchor_hashes`,
`trnm_receipt_v2_max_body_bytes`, and `trnm_receipt_v2_max_in_flight`; verified
mode with a zero pin count is never ready.

The default listener is `127.0.0.1:7011`. Override it with `HEPTA_BIND_ADDR`.
Production startup requires PostgreSQL and three non-empty, pairwise-distinct
service credentials. In-memory state is available only through the test
constructor and is reported as `in_memory_test_only` by readiness.

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
| `POST` | `/v2/hepta/papers/:paper_id/chain-finality` | Verify Receipt V2 and atomically create the Paper finality projection |
| `GET` | `/ready` | Storage and architecture readiness |
| `GET` | `/metrics` | Prometheus metrics |

Challenge creation, evaluation, and TRNM command creation require
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
`f2e3da051effabc97c2d0e7c47acd1df3d0dd4aa`; their per-file provenance is in
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

After changing the minimal compile manifests, generate its lock twice from the
clean immutable revision with the pinned builder, review and commit it, then
prove that the committed bytes regenerate exactly:

```bash
bash scripts/generate-hepta-research-league-docker-lock.sh --write
# review and commit services/hepta-research-league/docker/Cargo.lock
bash scripts/generate-hepta-research-league-docker-lock.sh --check
```

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
