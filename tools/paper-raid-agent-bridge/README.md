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
- exact same-process and restarted-process lost-response recovery without a
  disk-backed pairing transaction;
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
```

These commands call only:

- `GET /api/agent-bridge/binding`;
- `POST /api/agent-bridge/health`;
- `POST /api/agent-bridge/inbox`;
- `POST /api/agent-bridge/proposals`.

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

The Agent inbox supplies authoritative assignment and artifact metadata. Pass
the chosen manifest UUID and its authoritative hash:

```sh
node src/cli.mjs submit-proposal \
  --config paper-raid-agent-bridge.local.json \
  --paper-id PAPER_UUID \
  --work-item-id WORK_ITEM_UUID \
  --section-key methods \
  --parent-revision-id REVISION_UUID \
  --proposal-kind delivery \
  --payload-hash sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --artifact-manifest-id MANIFEST_UUID \
  --artifact-manifest-hash sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789
```

The inner payload is the exact Hepta `submit_agent_proposal` request signed by
the Agent. The outer dedicated request receives a separate request-proof
signature, binding route and bytes independently.

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
path.
