# Paper Raid Agent Bridge v2

This dependency-free local Bridge turns an external Ed25519 Agent into a
product-facing Paper Raid participant. The private key remains in an
owner-only local identity file. Normal operation never reads a player login
key, browser cookie, bearer token, CSRF token, or Authorization credential.

Bridge v2 supports:

- local Ed25519 identity generation/import;
- one-time browser pairing with an AgentBinding V3 proof-of-possession;
- bounded `self_declared_unverified` capability/resource disclosure;
- proof-authenticated binding, health, inbox, and proposal endpoints;
- assignment-scoped evaluator/reproducer inbox work against immutable review
  bundles, with exact-object verification and canonical signed receipts;
- byte-exact same-process transport retry plus deterministic restarted-process
  pairing recovery without a disk-backed pairing-code transaction;
- optional frozen research-session action signing.

## Configuration

Copy `example.config.json`, then adjust only its public origin, local paths,
capability declaration, and 1–64 assigned Paper IDs. Schema v2 rejects unknown
fields and all inline secrets, including `login_key_file`, login keys, tokens,
cookies, CSRF values, Authorization values, private keys, passwords, and
pairing codes. Schema v1 fails with an explicit migration hint.

```sh
mkdir -p local
chmod 700 local
cp example.config.json paper-raid-agent-bridge.local.json
chmod 600 paper-raid-agent-bridge.local.json
```

Capabilities must contain 1–16 sorted unique values from:

`artifact_analysis`, `citation_verification`, `evidence_search`,
`experiment_execution`, `experiment_planning`, `reproduction`,
`research_session_signing`, `section_drafting`.

Resource classes must contain 0–16 sorted unique values from:

`artifact_io`, `browser`, `code_execution`, `cpu`, `gpu`, `network`,
`sandbox`.

`max_parallel_tasks` is bounded to 1–32. These fields are explicitly
self-declared and unverified. They do not grant authority or affect scientific
facts, scoring, ranking, rewards, settlement, or finality.
Evaluator execution requires the declared `artifact_analysis` capability;
reproducer execution requires `reproduction`. These declarations remain an
additional local fail-closed check, never a source of assignment authority.

```sh
node src/cli.mjs identity-generate \
  --agent-id did:trnm:paper-raid:my-agent \
  --out local/agent.identity.json
```

Identity and successful binding-state files are created owner-only and never
overwritten. Symlink and unsafe-permission key files fail closed.

## Pairing

Create a one-time code in the authenticated browser, then run:

```sh
node src/cli.mjs pair --config paper-raid-agent-bridge.local.json
```

Paste the code into the hidden TTY prompt. Non-interactive callers may provide
one line on stdin or inject a `readPairingCode` callback into `pairAgent`.
There is deliberately no pairing-code argv flag, environment setting, or
config field.

The Bridge first sends `{pairing_code}` to
`POST /api/agent-bridge/pairing-context`. The returned short-lived public
context supplies the exact `subject_id` and `player_id` covered by the
AgentBinding V3 proof. It then sends `{pairing_code,binding_request}` to
`POST /api/agent-bridge/pair`. The binding request includes:

- `hepta.paper_raid.agent_binding_proof.v3`;
- the exact capability-disclosure hash;
- the local public key and Agent proof-of-possession;
- one binding/idempotency nonce with a bounded lifetime.

The code and pairing context live only inside one `pairAgent` invocation. The
Bridge checks that the owner-only state destination is writable before reading
the code. If a pair response disappears, the same already-serialized code and
exact V3 request are replayed once. The context's authoritative issue/expiry
bounds plus deterministic binding/nonce UUIDs also let a restarted process
reconstruct the identical request when the user re-enters the same live code.
No pending-code/request file is created. Only a successful public binding state
is persisted, and neither the code, context, proof, nor signed request is
returned or printed.

The binding UUID is stable for one player/Agent. After a Hepta dual-signed key
rotation, generate a fresh browser code and run `pair` with the replacement
identity. The existing owner-only state supplies the same binding UUID and is
atomically replaced only after the BFF validates Hepta's currently active key.
An old key cannot use signed endpoints, and a different player or Agent cannot
take over the mapping.

## Dedicated Agent endpoints

After pairing:

```sh
node src/cli.mjs binding --config paper-raid-agent-bridge.local.json
node src/cli.mjs health --config paper-raid-agent-bridge.local.json
node src/cli.mjs inbox --config paper-raid-agent-bridge.local.json
node src/cli.mjs inbox --config paper-raid-agent-bridge.local.json --watch
node src/cli.mjs prepare-delivery --config paper-raid-agent-bridge.local.json --input agent-output.json
node src/cli.mjs work --config paper-raid-agent-bridge.local.json
node src/cli.mjs work --config paper-raid-agent-bridge.local.json --watch --auto
```

Leave `paper_ids` empty for the normal player path. The proof-authenticated BFF
then discovers the paired player's current Papers from Hepta's authoritative
raid state, so no Paper UUID is copied into local configuration. An explicit
bounded list remains available for operator diagnostics.

These commands call only:

- `GET /api/agent-bridge/binding`;
- `POST /api/agent-bridge/health`;
- `POST /api/agent-bridge/inbox`;
- `POST /api/agent-bridge/delivery-drafts`;
- `POST /api/agent-bridge/proposals`;
- `GET /api/agent-bridge/review-objects`;
- `POST /api/agent-bridge/review-receipts`.

Every request carries `hepta.paper_raid.agent_bridge_request_proof.v1` in
`x-paper-raid-agent-*` headers. Its Ed25519 frame freezes, in order, schema,
binding ID, Agent ID, key ID, uppercase HTTP method, canonical path, canonical
query bytes, SHA-256 of the exact transmitted HTTP body bytes, nonce, issued
time, and expiry (at most 60 seconds). A lost response replays byte-identical
headers and body, enabling server-side idempotent replay without weakening
nonce protection. Cached response bytes expire with that proof and are never a
durable Paper read model. A server crash that leaves a pending row recovers
only through the same fixed route and exact nonce/hash. No cookie,
Authorization, bearer, or CSRF header is sent.

## Proposal submission

`work` consumes only versioned, explicitly bound delivery candidates returned
by the proof-authenticated BFF. It never combines an assigned task, lease,
section head, and registered manifest locally. Interactive mode can resolve
multiple explicit candidates; `--auto` submits only one exact item, and none or
multiple items never trigger an automatic proposal. One-shot and watch modes
emit the same `hepta.paper_raid.agent_bridge.work_result.v1` wrapper.

`prepare-delivery` consumes an Agent-generated local output descriptor with
exactly `paper_id`, `work_item_id`, `section_key`, `artifact_manifest_id`, and
`payload_hash`. It never consumes a browser command or pasted signature. The
signed BFF route derives and freezes the current parent revision, active
same-binding lease/fencing token, and authoritative manifest digest, stores the
control intent for at most 15 minutes, and revalidates it on every inbox and
proposal call. If assignment, phase, head, lease, manifest, or proposal state
changes, the candidate disappears. The Bridge still never infers a delivery
from a lone historical manifest or independent room collections.

The Bridge exposes no low-level proposal mutation. Every Bridge proposal is an
Agent Proposal V2 delivery bound to a server-created delivery draft and must
use `prepare-delivery` followed by `work`. Agent Proposal V1 frame support is
retained only to verify historical signed bytes; it is not a mutation path.

Once an explicit delivery item exists, `work` creates the exact Hepta
`submit_agent_proposal` request signed by the Agent. Agent Proposal V2 binds the
exact Paper/work/section/head, active lease ID and positive fencing token,
positive expected work version, payload digest, and both artifact manifest ID
and authoritative manifest digest. The outer dedicated request receives a
separate request-proof signature, binding route and bytes independently.
Proposal ID and Hepta idempotency key are deterministic from the short-lived
delivery draft, while its signed timestamp is fixed to the draft declaration
time. Reconstructing the same candidate therefore produces the same Hepta
request; the in-process HTTP retry additionally reuses byte-identical outer
proof headers and body. The BFF persists the exact proposal body hash and those
three derivation pins before forwarding. During the draft TTL, a process
restart after an ambiguous Hepta response can rediscover only a server-projected
`submitting`/`consumed` candidate whose authoritative proposal already matches,
or whose original work/head/lease tuple is still current. The Bridge then
reconstructs the same
body; caller-supplied replacement proposal or idempotency UUIDs are rejected.
The draft is consumed only after the BFF correlates every returned proposal
field to the persisted pins.

## Frozen Review execution

The same `work` command handles explicitly assigned evaluator and reproducer
tasks. Local confirmation is the default. `--auto` still acts only when the
combined Author-delivery/Review inbox contains exactly one actionable item;
zero or multiple items never select themselves. Reviewers that do not have an
execution assignment never receive a Bridge task.

Each Paper projection uses
`hepta.paper_raid.agent_bridge.review_tasks.v1`. An available item is
`hepta.paper_raid.agent_bridge.review_task.v1` and binds the exact task,
assignment, Paper, draft/finalized evaluation, role/kind, attempt, positive
assignment fencing token, state, and one
`hepta.paper_raid.resolved_frozen_review_bundle.v1`. Evaluator tasks bind the draft
evaluation authority; reproducer tasks bind the finalized evaluation. Neither
kind permits a null or caller-selected evaluation ID.

The frozen bundle binds submission/review round/slot/version/expiry, release
candidate, Paper bundle, Artifact Manifest, exact object descriptors, and one
typed execution plan. Objects are sorted and unique and each descriptor fixes:

- an authority-owned object key and safe relative logical path;
- role, media type, positive size, and `sha256:` digest;
- the literal `/api/agent-bridge/review-objects` download route.

The download query has exactly five sorted fields: `assignment_id`,
`bundle_hash`, `digest`, `object_key`, and `task_id`. It is covered by the
normal Agent request-proof signature. The BFF must re-check active assignment,
assignment version, bundle membership, expiry, and exact CAS bytes; the Bridge
then independently requires the received byte count and SHA-256 to match every
descriptor. An unavailable authority projects no task and a reason such as
`frozen_review_objects_unavailable`; the Bridge never joins Author manifests,
challenge hashes, or historical CAS objects locally.

The only execution plan accepted by this release is:

- `schema=hepta.paper_raid.review_execution_plan.v1`;
- `adapter=python3-stdlib-v1`;
- `entrypoint=evaluator/main.py`;
- a 500–30000 ms timeout and non-negative integer seed.

This is not a general Python or command runner. The executable and argv layout
are fixed in code, `shell` is always false, stdin is closed, the environment is
allowlisted, output/log bytes are bounded, and work runs in a fresh owner-only
temporary tree which is removed afterward. The descriptor cannot supply an
executable, argv, environment, absolute path, `..`, redirect, URL, or shell
fragment. More importantly, the evaluator entrypoint digest must be in the
Bridge release's compiled allowlist. This release recognizes only the three
audited seeded-pack evaluators and their exact support-object digests. Dataset,
candidate, and other input bytes are data; they are never imported as code.
Changing even one evaluator/support byte fails before execution.
Role-specific namespaces are also fixed: evaluator code is
`evaluator/main.py`, its sole optional support module is
`evaluator/baseline.py`, the candidate is `inputs/candidate.json`, datasets are
`inputs/dataset.json` or `inputs/dataset.csv`, and additional data stays below
`inputs/objects/`. An input therefore cannot masquerade as `json.py`,
`hashlib.py`, `sitecustomize.py`, or another imported module.

Successful execution produces
`hepta.paper_raid.review_execution_receipt.v1`. Its Ed25519 canonical frame
binds receipt/task/assignment/binding/Paper/submission/evaluation, kind,
attempt, fencing token, bundle hash, evaluator version, aggregate input and
output roots, metrics/statistical-evidence hash, seed-set hash, environment,
run-manifest and log seals, execution interval, Agent/key, and signing-key
hash. The POST wrapper is
`hepta.paper_raid.agent_bridge.review_receipt_request.v1`; embedded output,
environment, run manifest, metric facts, and log metadata must rehash to the
signed roots. Metrics use safe integer micros. No browser-entered metric,
environment statement, seed statement, or pasted signature can substitute for
this receipt.

Before the receipt POST, the exact signed body is fsynced to one owner-only
outbox beside `state_file`. A same-process lost response reuses byte-identical
headers/body. If both transport attempts are ambiguous, the outbox remains.
The next `work` invocation replays that exact receipt before reading a new
inbox, so a server-committed task that has already disappeared or become
`consumed` still recovers idempotently. The deterministic receipt/idempotency
UUID is fixed by binding, task, assignment, bundle, attempt, and fencing. The
outbox is removed only after an accepted response; a different task or Agent
cannot overwrite it.
Without a matching outbox, `submitting` and `consumed` projections are never
re-executed: fresh timestamps could otherwise reuse a stable receipt UUID with
different bytes. Only a pending task can start new local execution.

## Optional research-session action signing

```sh
node src/cli.mjs sign-action \
  --config paper-raid-agent-bridge.local.json \
  --input unsigned-action.json
```

This produces only a local signature for an already-authorized Nakama
transport. It does not create a login or introduce another mutation route.

## Verification

```sh
npm test
```

The Node suite freezes the V3 disclosure vector and Agent request-proof frame,
checks path/query/body tamper material, rejects config secrets/v1, proves no
login/session credentials are used, verifies byte-identical retries, confirms
pairing codes never reach disk/output, and exercises the dedicated proposal
path. It also runs a real allowlisted frozen evaluator, rejects role/assignment,
path, route, digest, byte, expiry, adapter and command-shape mutants, freezes a
language-neutral ReviewExecutionReceipt frame vector, and proves owner-only
outbox recovery after a committed/lost receipt response.
