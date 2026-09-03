# paper-raid-bff module contract

Status: active module contract  
Workspace member: `services/paper-raid-bff`  
Package: `paper-raid-bff`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `paper-raid-edge`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Provides the narrow browser/consumer edge for Paper Raid Alpha, aggregating Hepta, Nakama archive, and content-addressed object reads without taking their authority.

**Non-goals.** It does not own teams, papers, research facts, ordered Nakama events, object bytes, Ledger value, Chain finality, model credentials, or public identity-provider authority.

## Authority and owned state

Browser sessions, CSRF/revocation, bounded invite-alpha access directory, edge-local replay bytes, and Consumer Edge assertion signing only.

Owned state: Sessions, revocation generations, one-time CSRF uses, signed-request replay bytes, invite/access directory state, quota counters, access audit, Agent pairing/delivery drafts, and edge telemetry.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `app.rs`: route assembly and product flow.
- `auth.rs`/`oidc.rs`: sessions and provider-neutral OIDC foundation.
- `access.rs` and `paper-raid-accessctl`: invite-alpha governance.
- `agent_bridge.rs`: external Agent pairing/delivery edge.
- `db.rs`: edge persistence and schema checks.
- `hepta.rs`/`nakama.rs`/`cas.rs`: downstream adapters.

- `lib.rs`: library entry point and public module exports.

Catalog-bound entry points:

- `services/paper-raid-bff/src/main.rs`
- `services/paper-raid-bff/src/lib.rs`
- `services/paper-raid-bff/src/app.rs`
- `services/paper-raid-bff/src/auth.rs`
- `services/paper-raid-bff/src/oidc.rs`
- `services/paper-raid-bff/src/access.rs`
- `services/paper-raid-bff/src/agent_bridge.rs`
- `services/paper-raid-bff/src/db.rs`
- `services/paper-raid-bff/src/hepta.rs`
- `services/paper-raid-bff/src/review_receipts.rs`
- `services/paper-raid-bff/src/bin/paper-raid-accessctl.rs`
- `services/paper-raid-bff/README.md`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Browser routes, fixed/invite alpha login, Consumer Edge assertions, access-control CLI, Hepta/Nakama/CAS clients, Quick Raid/practice projections, readiness, and loopback-only metrics.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL stores sessions/access/quota/telemetry where configured. Activation rows and schema/catalog parity are fail-closed; operator schema/data roles are separate from the resident BFF.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Explicit identity mode, loopback/container scope, public origin, session/CSRF secrets, identity cohort, access retention/quota values, downstream trust keys/URLs, database roles, OIDC foundation settings.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Cookies are encrypted/authenticated, HttpOnly and SameSite=Strict; public OIDC routes remain disabled until a real IdP/JWKS lifecycle is qualified. Invite values are stored only as SHA-256 digests.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p paper-raid-bff --all-targets
cargo clippy -p paper-raid-bff --all-targets -- -D warnings
bash services/paper-raid-bff/scripts/check-boundaries.sh
bash services/paper-raid-bff/scripts/check-browser-e2e.sh
```

Required behavioral focus:

- Session/CSRF/replay, identity topology, invite issuance/redeem/revoke/rotate, quotas, and audit append-only behavior.
- Boundary tests proving Hepta/Nakama/CAS authority remains external.
- Browser E2E, mobile accessibility, image/SBOM, and PostgreSQL migration/recovery.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Keep the consumer edge loopback/SSH-bound for Alpha. Build resident and accessctl images separately, bind SBOMs to exact binaries, run schema migration only through the operator CLI, and deny non-loopback metrics.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Fixed Alpha and invite Alpha are mutually exclusive authority modes. Presentation projections never grant account, ranking, reward, scientific, or finality authority.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.
