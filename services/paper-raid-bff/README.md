# Paper Raid BFF

`paper-raid-bff` is the narrow Consumer Edge for the Paper Raid alpha. It maps
either a bounded fixed-alpha allowlist or the bounded database-backed
invite-alpha directory, holds browser sessions,
renders the dark bilingual player shell, signs only Consumer Edge assertions,
and aggregates typed Hepta, Nakama archive and content-addressed object reads.

Authority stays outside this process:

- Hepta owns teams, papers, revisions, consent and research facts.
- Nakama owns ordered session events and replay roots.
- the object store owns immutable artifact bytes under
  `objects/sha256/<digest>`.
- the BFF owns only sessions, revocation generations, CSRF uses, assertion
  audit IDs, one-time Agent pairing digests, and signed-request exact-replay
  bytes whose replay validity is bounded to at most 60 seconds; expired rows
  are deleted on the next signed admission and by retention maintenance.

## Provider-neutral OIDC foundation

`src/oidc.rs` freezes the provider-neutral OIDC security contract before any
public login route is enabled. It validates HTTPS provider endpoints, binds the
callback to the exact public origin, requires `openid`, enforces issuer,
audience/authorized-party, nonce, expiry, issued-at and maximum-age checks, and
maps the provider subject to a stable hashed local subject identifier without
embedding provider PII.

The claims API deliberately accepts only claims whose JWT signature and JOSE
header have already been verified against the pinned provider JWKS. Token
exchange, JWKS rotation/cache policy, PostgreSQL account provisioning and the
browser callback route remain disabled until a real IdP is selected and tested.
The fixed alpha login route remains the only active login path in this
candidate, so this foundation is not evidence of a public Beta deployment.

The alpha has two explicit edge scopes. `loopback_process` requires a
loopback bind. `container_loopback_publish` requires an unspecified container
bind and must be published by the host only on `127.0.0.1`, then reached over
an authenticated SSH tunnel. The public origin remains loopback HTTP in both
profiles, so the alpha cookie deliberately has `Secure=false`; it remains
encrypted/authenticated, `HttpOnly` and `SameSite=Strict`.

## Fixed alpha identity topology

`PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON` accepts at most 64 pairwise distinct
fixed identities and fails closed unless one simultaneously usable topology is
present: at least three `author` identities whose capabilities cover Captain,
Evidence and Experiment, plus at least four non-author identities covering one
evaluator, two reviewers and one reproducer. Those four review slots must have
a pairwise-distinct identity assignment even when an identity declares several
scopes; a multi-scope identity cannot satisfy two seats in the same Paper
round. Captain, Evidence and Experiment must likewise be assignable to three
different authors; one broadly capable identity cannot stand in for a full
team. This makes the minimum deployable Alpha topology seven humans; no BFF
restart or allowlist swap is part of the player flow.

Each identity object accepts these optional fields:

```json
{
  "login_key": "at-least-32-random-bytes",
  "subject_id": "alpha-reviewer-1",
  "display_name": "Reviewer One",
  "nakama_user_id": "00000000-0000-0000-0000-000000000004",
  "player_id": "00000000-0000-0000-0000-000000000014",
  "scopes": ["reviewer"],
  "author_roles": []
}
```

`scopes` is a non-empty, duplicate-free subset of `author`, `evaluator`,
`reviewer` and `reproducer`. `author_roles` is a duplicate-free subset of
`captain`, `evidence` and `experiment`; it is allowed only with the `author`
scope and must then contain at least one role. Omitting both fields still parses
an identity as an author supporting all three roles, but the complete
configuration is rejected unless the independent review topology above is also
present. A non-author identity that omits `author_roles` receives no author
role.

Unknown fields and values, empty or duplicate sets, incomplete author-role or
independent-review coverage, more than 64 total identities, short login keys,
and any repeated login key, subject, player or Nakama user ID fail
configuration loading. The scopes record
which participation surfaces an identity is eligible for; they do not replace
Hepta's signed membership, non-author and reviewer/reproducer independence
checks, which remain the authorization authority for each Paper.

## Invite-Key SSH Alpha

`PAPER_RAID_BFF_IDENTITY_MODE` is an explicit, deny-unknown authority switch.
It defaults to `fixed_alpha`. The optional `invite_alpha` mode replaces the
environment allowlist with a PostgreSQL-backed BFF access directory so an
operator can provision a 20–50 person SSH-tunnel cohort without restarting the
BFF or editing its environment. It does not enable OIDC, public registration,
HTTPS termination, Internet exposure, ranking, rewards, or any legacy League
authority. The same loopback/SSH edge checks above remain mandatory.

The two identity sources can never be active together. `fixed_alpha` requires
`PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON`; `invite_alpha` rejects that variable
and requires all of these explicit settings:

```text
PAPER_RAID_BFF_IDENTITY_MODE=invite_alpha
PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID=<approved-opaque-policy-id>
PAPER_RAID_BFF_LOGIN_QUOTA_WINDOW_SECONDS=<explicit-seconds>
PAPER_RAID_BFF_LOGIN_QUOTA_GLOBAL_LIMIT=<explicit-count>
PAPER_RAID_BFF_LOGIN_QUOTA_BUCKET_LIMIT=<explicit-count>
PAPER_RAID_BFF_MUTATION_QUOTA_WINDOW_SECONDS=<explicit-seconds>
PAPER_RAID_BFF_MUTATION_QUOTA_ACCOUNT_LIMIT=<explicit-count>
```

The database activation authority treats SQL `NULL` as hostile unless the
field is explicitly nullable. Its v3 row validator is declared `CALLED ON
NULL INPUT`, the table parity `CHECK` wraps the validator in
`COALESCE(..., FALSE)`, and runtime schema readiness requires the installed
validator to remain non-`STRICT`. Active rows may therefore use `NULL` only
for both revocation fields; a required approval field encoded as JSON `null`,
or a tampered `STRICT` validator, fails closed. V3 stores and revalidates the
exact root-provisioned local approval and runtime-ACL evidence bytes. It pins
the activation UUID, deployment, positive monotonic sequence, nonce digest,
60–86400 second TTL, database name/OID and PostgreSQL cluster system
identifier. Unique receipt/nonce and deployment+cluster+sequence keys plus
immutable revocation tombstones reject remint, downgrade and cross-cluster
replay. BFF startup, `/ready`, and product middleware execute one PostgreSQL
statement that compares the exhaustive v3 table/constraint/index/trigger and
function-attribute catalog, the exact migration-owned function bodies, every
configured pin, the revalidated authority row and raw-byte digests, the live
cluster identity, revocation/expiry state, and the canonical runtime ACL under
one statement snapshot. The separate schema status helper is diagnostic only
and can never authorize a product request.
The root-owned local receipt is the authority; no remote/public issuer is
implemented. Real `pg_control_system()` privilege/readback and trigger
behavior remain mandatory dynamic activation evidence.

In invite mode `/ready` separates database/schema reachability from cohort
provisioning. It reports `access_directory_reachable`,
`access_directory_within_capacity`, `access_audit_append_only`,
`access_topology_ready`, provisioned/active account counts, Captain/Evidence/
Experiment-capable author counts, and evaluator, reviewer, and reproducer
counts. Provisioned means an `invited`
account with a currently redeemable invitation in an active batch, or an
`active` account with a non-expired active credential; paused/expired batches,
expired invitations/credentials, and suspended or closed accounts cannot
satisfy topology. Overall
readiness remains `503` until three distinct provisioned authors cover Captain,
Evidence, and Experiment and four further distinct non-author accounts cover
one evaluator, two reviewers, and one reproducer. A zero-account directory is
therefore reachable but not game-ready. Operators can run the database-only
access CLI before starting/routing the BFF; the process does not need to claim
player readiness in order for provisioning to work.

Invite and credential values are 256-bit random, displayed by the operator CLI
exactly once, and stored only as SHA-256 digests. The first successful
`/alpha/login` atomically marks the one-time invitation redeemed, activates its
account, and promotes the same presented value to an active login credential.
Every later request re-reads the account directory; suspension or closure
therefore denies an existing cookie immediately, and the operator action also
increments the session generation and revokes stored sessions. Login failures
share one `401 authentication_required` response. PostgreSQL-backed global and
256 deterministic pre-authentication-bucket login windows plus a per-account
authenticated-mutation window fail closed and return `429` with `Retry-After`
when exhausted.

`paper-raid-accessctl` is a host/operator binary, not an Axum route. Run it only
from a root/operator shell with a mode-0600 environment or secret file. It
requires `PAPER_RAID_BFF_IDENTITY_MODE=invite_alpha`, the dedicated
`PAPER_RAID_ACCESS_DATABASE_URL`, and `PAPER_RAID_ACCESS_OPERATOR_SUBJECT`.
Only `prune` additionally requires the configured retention policy ID. It
deliberately has no fallback to
`PAPER_RAID_BFF_DATABASE_URL`, and the resident BFF never reads the operator
DSN. Routine commands never run migrations. Initial schema activation and
explicit idempotent upgrades use only the closed `schema-migrate` command with
a schema-owner DSN; routine commands should use a narrower operator data role.
Neither credential belongs in the BFF runtime environment.

The separately pinned operator image is built with `Dockerfile.accessctl` and
contains exactly `/paper-raid-accessctl`; it does not reuse the resident BFF
image. The committed accessctl SBOM is deliberately unbound while b5 protects
the active candidate. Packaging must first build the current binary and
export it independently twice from the pinned `runtime-binary-export` target,
then run
`scripts/bind-accessctl-runtime-sbom.sh services/paper-raid-bff/docker/accessctl.sbom.cdx.json FIRST_BINARY SECOND_BINARY`
from a clean committed tree. The binder requires byte-identical binaries,
replaces the zero runtime digest with their exact SHA-256, and changes the
release state to `bound-release-binary`. The Dockerfile and
release-provenance verifier both reject an unregenerated or mismatched SBOM,
so this static slice cannot be activated or described as a built image.

In `invite_alpha`, BFF startup performs only a read-only schema-presence check
and fails closed until the operator CLI has provisioned it. Its separate
`PAPER_RAID_BFF_DATABASE_URL` role should receive only the SELECT/INSERT/UPDATE/
DELETE grants required by sessions, one-time redemption, access-directory
reads, append-only audit insertion, telemetry, and durable quota counters; it
does not need schema-owner or trigger-changing privileges. The existing fixed
Alpha startup migration behavior remains unchanged for compatibility.
Representative invocations are:

```bash
paper-raid-accessctl schema-migrate
paper-raid-accessctl batch-create --label cohort-a --max-issued 50 \
  --expires-at 2026-09-01T00:00:00Z
paper-raid-accessctl invite-issue --batch-id <uuid> --subject alpha-author-08 \
  --display-name 'Author Eight' --nakama-user-id <uuid> --player-id <uuid> \
  --scopes author --author-roles evidence --expires-at 2026-08-20T00:00:00Z \
  --credential-expires-at 2026-09-20T00:00:00Z
paper-raid-accessctl invite-reissue --subject alpha-author-08
paper-raid-accessctl invite-revoke --invite-id <uuid>
paper-raid-accessctl batch-pause --batch-id <uuid>
paper-raid-accessctl batch-resume --batch-id <uuid>
paper-raid-accessctl credential-rotate --subject alpha-author-08 \
  --expires-at 2026-10-20T00:00:00Z
paper-raid-accessctl account-suspend --subject alpha-author-08
paper-raid-accessctl account-reactivate --subject alpha-author-08
paper-raid-accessctl account-close --subject alpha-author-08
paper-raid-accessctl account-export --subject alpha-author-08
paper-raid-accessctl prune --policy-id <approved-opaque-policy-id> \
  --before 2026-08-01T00:00:00Z
```

`invite-issue` accepts already allocated `player-id` and `nakama-user-id`
values; it does not create an upstream Nakama account or a Hepta research fact.
The operator must provision and verify those external identifiers through the
existing private control planes before delivering the one-time credential.
This keeps the access CLI from becoming a second game or match authority.
`invite-reissue` is the bounded recovery path when an issued invitation's
one-time value was not delivered: it accepts only a known subject whose
account is still `invited` and invitation is still `issued`, atomically
replaces the stored digest, and does not increment the batch issue count or
consume capacity again. Operator-subject provenance is still an Alpha blocker:
the current opaque subject is supplied by a trusted host environment rather
than an independently attested operator identity provider.

### Bounded operator metrics

The BFF exposes Prometheus text at `GET /metrics` only when the immediate TCP
peer is loopback. The production entrypoint supplies the peer address to Axum;
non-loopback callers receive `404`. In the X230 Alpha profile Docker port-NAT
does not preserve a loopback peer address inside the container, so a host curl
to the consumer publish remains denied. A future pinned operator collector
must share the BFF container's network namespace and scrape
`http://127.0.0.1:7020/metrics`; this sidecar topology is not activated by the
current release. `/metrics` has no credential and must never be proxied through
the consumer edge.

Metrics are process-local operational signals, not research authority. They
use fixed route templates, method/status classes, fixed latency buckets, and
small enumerated outcome labels. Player, subject, Paper, team, challenge,
Agent, binding, invitation, credential, digest, IP, user-agent, and free-form
reason values are never labels or sample values. The exported families cover
HTTP count/latency, aggregate Hepta/finality failures, queue depth and ETA
availability, Bridge pairing/submission/recovery, stale-authority UI reloads,
invite authentication/redemption outcomes, and aggregate login/first-action
funnel events. Counters reset on BFF restart; Prometheus provides durable
retention.

Metrics health is deliberately absent from `/ready`: a broken scraper must
alert, but it cannot change the truth of PostgreSQL, Hepta, Nakama, CAS,
identity, or Agent Bridge readiness. The static
`scripts/check-observability-boundary.sh` gate rejects raw URL-path labels,
identifier labels, non-loopback exposure, and readiness coupling.

Every batch and invitation expiry is explicit; `never` must be written when
the operator deliberately wants no expiry. Credential expiry is independently
explicit both at invitation issue and rotation. Each batch has an immutable
operator-selected issue ceiling and the global non-closed directory is capped
at 64 accounts. There is no guessed retention period: `prune` requires both an
exact RFC3339 cutoff and an exact match to the
configured policy ID, then records non-secret per-table deletion counts. The
access audit is append-only under a database trigger and is not pruned by this
command; any future archival/deletion procedure requires a separately reviewed
policy and privileged path. Batch pause/revoke applies to outstanding
invitations; already redeemed accounts are managed explicitly with
suspend/close. `account-export` keeps the same command and preserves its
directory/lifecycle fields, but `paper-raid-bff.account-export.v2` is only a
BFF-local access-directory projection. Its machine-readable `export_boundary`
sets `global_account_export_complete=false`, enumerates the included account,
scope, author-role, credential-lifecycle, invite and access-audit records, and
continues to exclude credential/invite hashes and all secret material.
The v2 boundary states that included rows come from one read-only,
repeatable-read PostgreSQL snapshot. The audit row for the current export is
written only after that snapshot and is explicitly not included in the output.
Access-audit rows expose their audit ID, a closed projected action, and an
operator-provenance status. The raw host-self-declared operator subject is
omitted and represented as `unverified_host_assertion_redacted`; unknown
actions are replaced with `redacted_unverified`. Metadata is reconstructed
only from a closed action/schema whitelist. Unknown or legacy metadata is
omitted and marked `redacted_unverified`; raw JSON is never passed through.
`contains_secret_material=false` applies only to these reconstructed export
fields, not to omitted authorities or an unbounded global subject access
request.

The same boundary explicitly closes every omitted component instead of
silently implying a global export. BFF sessions/request-security state, Agent
Bridge rows, product telemetry, full invite-batch records, quota/retention
metadata, schema metadata and global operator-command audit are `not_queried`;
they are resident BFF data but outside this account-scoped access-directory
command. Hepta and Nakama are `not_supported`
because they are separate authorities and accessctl performs no
cross-authority or network read. CAS reachability, CAS bytes and backups are
also `not_supported`; they require separately reviewed inventory paths that do
not exist here. These statuses describe command capability, not data absence,
retention, deletion, anonymization or legal completeness. The access
credential survives suspension but never has its expiry extended; rotate it
explicitly after reactivation when it has expired. The access schema is
BFF-local and contains no Paper facts, Agent secrets, signatures, scores,
rankings, rewards, or legacy state.

### Host-local command audit

Every supported or rejected routine accessctl invocation records a host-local
command attempt after connecting to an already activated schema. Identity-mode
rejection and bounded argument rejection are therefore auditable. Routine
commands never auto-migrate; a missing capability returns
`schema_activation_required`. `schema-migrate` is the only activation
ceremony. On first activation its attempt audit is truthfully `unavailable`
because the table does not yet exist, while its result is recorded after the
schema becomes ready. Readiness verifies the exact three-value outcome
constraint, validator function bytes/properties, typed attempt/event linkage,
the unique attempt/event index, and the exact append-only trigger/function;
the capability marker alone never proves this boundary.

For every successful business command, its per-object audit rows and
`operator_command_result` row are inserted in the same database transaction as
the business mutation. A one-time invitation or credential is not printed
until that transaction is confirmed committed. Attempt and result audit IDs
are deterministically derived from `attempt_id + event`; inserts are followed
by exact readback, so retrying after an acknowledgement loss cannot create a
second command result. A failed command writes its bounded result in a
separate transaction after the failed business transaction is gone.

The two append-only actions are exactly `operator_command_attempt` and
`operator_command_result`. Their metadata schema contains only a generated
attempt UUID and closed `command_code`/`command_state`/`reason_code` values; it contains no
arguments, subjects, account/batch/invitation identifiers, credentials,
secrets, paths, free text or exception details. The database outcome is the
closed set `succeeded | denied | indeterminate`: `succeeded` means the attempt
record or committed command result succeeded; `denied` means the transaction
is known not committed; `indeterminate` is reserved for a commit
acknowledgement whose exact result row cannot be proved by exact readback.
An absent, mismatched, or unavailable post-COMMIT read is not evidence of
rollback and therefore remains `unknown`. The same rule applies to the
attempt/result audit transaction itself and is exposed as
`audit_status=unknown` with a bounded `*_audit_commit_unknown` code. A denied result distinguishes `command_rejected`,
`unsupported_command`, and `database_operation_failed` through its bounded
reason code. Other statement/transaction errors report
`command_state=not_committed`; only the commit-acknowledgement case reports
`command_state=unknown`.

If the attempt audit cannot commit, the business command is not run. A
successful business transaction cannot exist without its exact success result
row. When the database cannot prove the commit either way, accessctl emits no
normal or one-time-secret result; operators use `invite-reissue` for a lost
issued-invitation delivery and explicit credential rotation for an active
account. Missing access-database configuration, connection, or activated
schema means the audit table is not trusted or reachable; accessctl truthfully
returns `audit_status=unavailable` without claiming an audit row. All failure
output is bounded JSON and omits the underlying exception. No operator network
route is added.

## Agent Bridge v2

An authenticated author creates one five-minute pairing grant in the browser.
The cleartext `prg1.*` code is returned exactly once, copied explicitly, and
never recoverable from the BFF: PostgreSQL stores only its SHA-256 digest and a
fail-closed `issued -> pinned -> consumed` lifecycle. One subject can have only
one live grant. An expired issued or pinned grant is revocable; a fresh code and
fresh V3 proof can recover the same Agent binding that committed upstream
before a lost response.

The local dependency-free `tools/paper-raid-agent-bridge` accepts the code only
from a hidden interactive TTY or intentional stdin. There is no code/login key
flag, environment variable or config field. It first calls the rate-limited
`POST /api/agent-bridge/pairing-context`, then reuses the same in-memory code and
exact AgentBinding V3 request at `POST /api/agent-bridge/pair`. The public
context includes the grant's authoritative issue/expiry bounds; stable UUIDs
derived from the player, Agent and grant let a restarted process reconstruct
the same binding/proof request without persisting the code, context, signature
or a pending transaction. The V3 proof
binds a bounded, sorted capability/resource disclosure whose assurance is
always `self_declared_unverified`; it is not a measured benchmark or ranking
fact. Pair response bytes are pinned so an exact retry after a lost response is
stable. The CLI proves the owner-only state destination is writable before it
consumes a code. A later one-time grant may atomically refresh the mapping for
the same subject/player/Agent/binding after Hepta's dual-signed key rotation;
different owners or Agent IDs remain conflicts. The old key fails signed calls
until that re-pair, and only Hepta's currently active key can restore service.

After pairing, the Agent has eight dedicated endpoints: `GET` binding,
review-objects and challenge-objects, plus `POST` health, inbox,
delivery-drafts, proposals and review-receipts. Every request
uses the nine frozen `x-paper-raid-agent-*` headers and an Ed25519
`hepta.paper_raid.agent_bridge_request_proof.v1` over the exact method, path,
canonical query, raw HTTP body SHA-256, binding/key identity, nonce and at-most
60-second lifetime. Unknown bindings hit a global and one of 256 fixed buckets
before lookup; only a valid mapped binding consumes the per-binding quota or
writes a nonce/replay row. The row and any response bytes expire with the
request proof (at most 60 seconds); every signed-request admission and the
retention tool transactionally delete expired rows. If the BFF crashes after a
fixed route commits but before completing the cache row, an exact hash+nonce
retry waits for an in-flight request and then re-enters only that same route:
binding/inbox are reads, health is an idempotent upsert, and proposal recovery
reuses Hepta's original idempotency key. Completion still compare-checks exact
response bytes. The BFF reconstructs the owner Consumer assertion and
self-reads Hepta's active binding/key/capability on every call, so key rotation,
revocation, account suspension or disclosure drift fails closed. Health remains
self-declared, inbox reads only owner-visible Papers and tasks assigned to that
player/binding, and proposal forwarding is fixed to `submit_agent_proposal`.
No Agent request can select an upstream route or assertion operation.

The proposal transport is Bridge-driven rather than command-copy assisted. An
empty Bridge `paper_ids` list asks the BFF to discover the paired player's
Papers from Hepta's authoritative raid state. The inbox supplies assigned work
and prior proposals, plus a versioned `delivery_candidates` projection. The
client consumes only exact candidate items projected by the BFF; it never joins
tasks, leases, section heads, or manifests locally.

For `planned` or `in_progress` Author work, the same inbox projects one immutable four-object
Challenge bundle—playable brief, dataset, baseline and frozen evaluator—from
the Paper's activation-derived snapshot. Accepted, rejected and cancelled
history cannot suppress or replace a current task. Before Agent work starts,
the Bridge performs four separately proof-signed GETs and verifies exact role,
path, media type, byte length and digest. It publishes an owner-only material
directory only after every object passes, using fsync plus an atomic no-clobber
rename; partial downloads and ambiguous bundles never become consumable. This
removes manual selection of authoritative Challenge files without allowing the
Agent or player to choose scientific truth.

Any projected `pending`, `submitting` or `consumed` delivery recovery suppresses
all new Author starts until it is handled. Otherwise, after atomic material
publication, the Bridge runs its prevalidated local Author executor with the
unique material directory as its working directory. The executor must echo the
exact Paper/player/binding/work/version/bundle/authority start binding and may
return only section, registered artifact-manifest ID and payload digest as new
draft input. The Bridge then declares one short-lived delivery intent through
the signed `delivery-drafts` endpoint, reads a fresh inbox, requires exactly one
field-for-field matching candidate, and only then signs a proposal. A recovery
candidate is resubmitted without rerunning materials or the executor; there is
no endpoint or public CLI that accepts a caller-selected draft, lease, manifest
hash or proposal authority.

The request names only Paper, assigned work item, section, manifest and payload
digest; the BFF derives the current work version, parent, active holder lease
and positive fencing token, and authoritative manifest digest from Hepta. It
persists the resulting control binding for at most 15 minutes and revalidates
every field against Hepta before inbox projection and again before proposal
forwarding. A changed work version, head, lease, assignment, phase, manifest or
existing submitted delivery makes the draft unavailable. Bridge forwarding is
delivery-only and signs Agent Proposal V2 over the exact lease/fence/work epoch
and artifact manifest ID/hash; historical V1 verification is not a BFF mutation
path. Before the first forward, the BFF requires the proposal ID, Hepta
idempotency key and signed timestamp to equal the frozen deterministic
derivation from the binding, draft and declaration time, then atomically pins
the exact canonical body hash while moving `pending -> submitting`. The claim
holds the draft row lock and rechecks the complete Paper/work version/section/
lease fence/head/manifest/payload snapshot plus all deterministic proposal
derivations, so a concurrent draft refresh cannot poison the newer row with an
older request. If the
BFF dies after Hepta commits but before local consumption/response completion,
an exact authoritative proposal readback permits the still-live `submitting`
(or already `consumed`) draft to be reprojected. A restarted Bridge recreates
the same body and Hepta idempotently returns the same proposal; only an exact
field-for-field upstream response can move the draft to `consumed`. Changed
proposal bytes or an unrelated 2xx response fail closed. No
task/lease/head/manifest Cartesian product is inferred. `work --auto` remains
restricted to exactly one explicit current or recovery item.

Agent Bridge quota defaults can be overridden explicitly with:

```text
PAPER_RAID_BFF_AGENT_BRIDGE_QUOTA_WINDOW_SECONDS=60
PAPER_RAID_BFF_AGENT_PAIR_QUOTA_GLOBAL_LIMIT=120
PAPER_RAID_BFF_AGENT_PAIR_QUOTA_BUCKET_LIMIT=8
PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_GLOBAL_LIMIT=2000
PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_BUCKET_LIMIT=120
PAPER_RAID_BFF_AGENT_REQUEST_QUOTA_BINDING_LIMIT=240
```

`/ready` requires BFF PostgreSQL, Hepta `/ready`, Nakama `/healthcheck`, a
successful read of the configured immutable CAS canary, and valid
trust/config. It also reports `agent_bridge_schema_ready` and
`agent_bridge_integrity_ok`; readiness never requires an already-paired row.
It reports `finality=paper_scoped_projection`. Hepta may authoritatively expose
`pending_finality` or, after a verified Receipt V2, `verified_finality`. When
the review aggregate is absent, conflicted, or unavailable, both the initial
Paper Room and ancillary live timeline preserve that fact as
`unknown_finality`, `unavailable_finality`, or `error_finality` under the non-authoritative
`hepta.paper_raid.bff_finality_availability.v1` schema; it never reuses the
authoritative consumer-finality schema or rewrites uncertainty as pending. The
normal UI names these states distinctly, displays the bounded reason code, and
keeps every eligibility bit locked.
Successful Hepta projections are strictly parsed as
`hepta.paper_raid.consumer_finality.v2`; malformed status/schema/timestamps and
missing, noncanonical, or mismatched effective evaluation/reproduction/
Appeal-resolution bindings are rejected fail-closed. Pending projections must
carry three null bindings; verified projections must match the AAR's exact
effective causal chain field by field. Ranking, score,
reward and economic eligibility remain independently fail-closed and are never
inferred from finality alone.

P1, P3 and P5 command routes are exact-whitelisted. Contribution ledgers,
evaluations, tolerance-aware reproductions, Appeals and independent
resolutions are submitted through their committed typed routes and read back
from the P5 review model. Missing sections render as `unavailable`; the BFF
never fabricates research facts or persists a Paper Room read model.

Appeal is also a zero-JSON player path. An author types plain-language grounds
and chooses one Paper-scoped ArtifactManifest from the authoritative Paper
Room; the browser hashes the grounds and the BFF derives the current
evaluation, frozen release hash and selected manifest hash before returning a
local human-signing frame. An actively assigned independent reproducer sees a
resolver form only for one unresolved Appeal bound to that exact evaluation.
`denied` always binds a null superseding evaluation; `upheld` is enabled only
when the review model contains one exact same-release, next-round,
independent-panel superseding evaluation. The signing-frame response carries
the server-derived Paper/child route locators, so neither normal form accepts
an evaluation UUID, Appeal UUID, release digest, signature or protocol JSON.
Reload and lost-response recovery come from `review-state`, not browser cache.
The legacy exact-JSON editors remain collapsed under Developer Tools.

The four Nakama lifecycle controls are also fixed BrowserCommands. Create,
resume, roster replacement and completion map only to the committed static
`/v2/hepta/nakama/research-session-controls/{create,resume,replace-roster,complete}`
POST routes and their exact `*_v2` Consumer assertion operations. Their
BrowserCommand `resource_id`, `child_id` and `session_id` locators must all be
null; session/roster/set locators come only from Hepta's deny-unknown typed
payload. The caller can never supply an upstream path or assertion operation.

The browser alpha is a same-origin, external-script flow:

1. `/login` exchanges either one fixed-alpha allowlisted key or one
   invite-alpha invitation/credential for an encrypted HttpOnly session; the
   page immediately clears the key and never uses browser storage. Invite mode
   is a bounded cohort directory, not a three-person mode or public signup.
2. `/league/start` first self-reads `/v2/hepta/players/me`. Authors and
   reproducers additionally require an active external Agent binding;
   evaluators and reviewers do not. Scope then routes authors to the Research
   Lobby and independent identities to `/league/review`.
3. Human onboarding separates key generation/export from registration. The
   Ed25519 private key is generated in WebCrypto, exported once only inside an
   AES-256-GCM/PBKDF2-SHA-256 encrypted recovery bundle, and re-imported as a
   non-extractable in-memory signer. The BFF receives only the public key and
   proof of possession. An uncertain registration response is recovered via
   the self-read; a retry reuses the original imported key and never silently
   generates a replacement.
4. Agent onboarding normally generates a browser-authenticated five-minute
   one-time code and accepts no Agent-side player credential. The dependency-
   free local `tools/paper-raid-agent-bridge` reads that code only from TTY/stdin,
   creates the exact `hepta.paper_raid.agent_binding_proof.v3` plus bounded
   self-declared capability profile, recovers a lost response through
   authoritative self-read, reports signed health, polls assigned work, and
   submits independently signed Agent proposals. Pasting public V3 proof JSON
   remains inside a collapsed recovery/developer fallback, not the intended
   player flow. The
   Lobby also exposes the exact dual-signed
   `hepta.paper_raid.agent_binding_key_rotation.v2` request: both
   the currently bound and replacement Agent keys must sign the same scoped
   rotation. Agent private keys, seeds, mnemonics, runtime tokens and provider
   credentials never enter this browser or BFF.
5. `/league` reads Hepta Challenges, player-scoped matchmaking tickets and
   player-scoped author-Team proposals. Queue and proposal decisions use
   the exact P3 typed routes. A queued ticket has a server-derived 30-minute
   deadline, can be cancelled, and shows queue position, compatible pool,
   missing players/roles and an honest ETA (`0` only when a compatible team is
   ready; otherwise unknown). Matching uses the exact availability bucket and
   a deterministic Captain/Evidence/Experiment role solver rather than plain
   FIFO-three selection. An optional premade trio uses a browser-generated
   lowercase `PR1-<UUIDv4>` code: the browser hashes it before submission and
   clears the raw input, while Hepta accepts only the canonical digest. Public
   tickets never mix with private-party tickets, different party digests never
   mix, two party members wait for the exact third, and a fourth live ticket is
   rejected. A joining member is also rejected before insertion if the party's
   exact availability window drifts or its partial role preferences can no
   longer cover all three distinct roles; any queued member can cancel and
   rejoin with corrected preferences. Ticket responses reveal only
   `private_party: true|false`; the
   party digest is absent from player read models, Room/outbox events, logs and
   metrics. It is affinity only and grants no identity or gameplay authority.
6. `/league/formation/<proposal-or-team-id>` renders either the proposal or
   formal Team and exposes typed materialization, human-signed readiness,
   locking and Paper creation controls. Protocol JSON is confined to the
   closed Developer Tools fallback.
7. `/league/papers/<paper-id>` reads only the P3 Paper Room, event stream and
   P5 review state. Its normal surface is driven by Hepta's derived objective,
   blockers and next actions. It currently exposes typed phase, work-item,
   artifact, evidence, experiment, claim, paper-revision, release, consent and
   finalization actions, including the complete section
   lease/proposal/decision/review/merge chain and deterministic contribution
   ledger preparation. Evidence verification, human decisions, section
   reviews and authorship consent frames are constructed from current Hepta
   facts and signed by the imported human key locally. Protocol editors remain
   available only under closed Developer Tools.
   The Room uses Hepta's typed `player_phase`; the stored V1 compatibility
   value `reproducing` is displayed and evidenced only as
   `reproduction_readiness`. Independent reproduction remains a Review Raid
   action and is never presented as Author work.
   The Experiment path includes browser-native run and figure wizards: exact
   stdout/stderr plus successful output/metrics bytes become verified
   CAS-backed manifests and one exact RunRecord; failed and cancelled runs
   retain their logs and plain-language failure fact without inventing output.
   An SVG plus its transform description becomes a verified figure manifest
   and FigureLineage bound only to selected authoritative room runs. Manifest
   IDs, hashes, and record IDs are generated and checked by the browser
   workflow rather than entered by a player.
   Release promotion derives each author's same-Paper accepted Agent artifacts
   and approving section reviews from the authoritative Room, applies the
   capped 100/150 provisional milestone budget, and freezes that exact ledger
   hash into the candidate. Duplicate records cannot multiply either
   milestone; malformed or drifting Room facts lock promotion/repair instead
   of silently budgeting zero. These points do not unlock ranking, rewards or
   economic eligibility.
   The Room also renders the immutable ChallengeRuleset snapshot as player
   rules: template, product difficulty, version, duration/grace, every victory
   requirement and every phase gate. The authoritative deadline drives a live
   deadline/overtime/grace countdown, while `outcome`, `outcome_reason` and
   terminal time are read back as game facts. Captain-only failed/abandoned
   controls post typed reason codes through `/api/papers/<paper-id>/outcome`;
   the BFF re-reads Hepta and derives the Paper locator and aggregate version.
   Expiry is not a Captain action. Once the snapshotted grace deadline has
   elapsed, Hepta materializes the canonical terminal fact idempotently on the
   next authorized Room/state/event read; the normal UI offers no manual
   expired form. Players never enter JSON, UUIDs, versions or hashes for these
   actions.
   When the frozen ruleset allocates role resources, the same normal Room
   renders the actor-scoped remaining focus, shared run budget and explicit
   Evidence/Captain actions. Evidence chooses an unassessed authoritative
   EvidenceCard; Captain can checkpoint only after fresh Evidence and
   Experiment progress. Experiment RunRecord creation consumes its role focus
   and the shared run budget inside Hepta's authoritative transaction. A
   disclosed retained failure may refund focus but never run budget. These
   resources affect pacing only: ranking, score, reward and economic
   eligibility remain false.
   A canonical three-role roster is enforced end to end: Captain owns phase
   checkpoints and cross-player work orchestration; Evidence owns evidence,
   citation and claim mutations; Experiment owns preregistration and retained
   runs. Artifact, section, revision and consent actions stay collaborative.
   The normal UI renders only actions authorized for the current role; legacy
   noncanonical teams retain their original permissive behavior.
   Terminal Author Raids also render an `After Action Report v1` from the
   existing Paper Room, review and finality authorities. It explains retained
   success/failure/cancellation facts, recorded role-resource actions, frozen
   CRediT/milestone entries and any provisional RaidScore while keeping
   ranking, reward, score and economic eligibility visibly locked. Missing,
   unavailable, cross-Paper, duplicated or malformed Room/review authority
   makes the entire report render one safe unavailable card; it never mixes a
   malformed child card with contribution rows or provisional XP. Production
   AAR input is limited to private sealed wrappers constructed only by the
   operation/path/body-bound Hepta `/room` and `/review-state` reads; raw
   `Value` fixture sealing is test-only. The projection binds the
   single ledger to the Paper's unique current release revision, candidate hash,
   candidate-bound ledger hash and frozen CRediT roster. It recomputes the
   complete canonically ordered scientific reference vectors without imposing
   an artificial 256-reference ceiling; rendering remains count-only and the
   milestone XP budget remains capped. It binds the current submission and
   canonical frozen PaperBundle material, recomputes tolerance-policy,
   reference-metric, hard-gate, PaperScore and evaluation-signing hashes,
   derives evaluation status from the exact two-reviewer quorum, and accepts
   RaidScore XP only after those facts agree. The sealed review boundary also
   parses exact typed assignment and reproduction records, enforces canonical
   UUID/timestamp/status lifecycles and rejects duplicate live round/slot or
   round/player leases. Hepta `/review-state` includes only open and expired
   evaluation-draft authority: pinned and released evaluator/reviewer leases
   must bind that exact draft, while finalized drafts remain omitted and each
   finalized evaluation still requires its consumed evaluator/two-reviewer
   assignment set. Every reproduction binds its exact evaluation, tolerance
   rules and active reproducer assignment.
   Historical superseded records remain explainable through a unique monotonic
   lineage; duplicates, cross-Paper records and unlinked records close the whole
   AAR. Open and denied Appeals keep the
   appealed evaluation effective; an upheld resolution selects only its exact
   same-submission/same-release superseding evaluation and score. A child
   evaluation—and any child Appeal or resolution—cannot become authoritative
   before the parent's upheld resolution. Verified finality additionally
   requires the unique latest reproduction for the effective evaluation to be
   reproduced and temporally prior to verification. Role-resource
   balances and exact failed-run refunds are replayed from the frozen
   allocation; every complete typed RunRecord must bind one
   `action_id == subject_id == run_record_id` action by the unique Experiment
   actor. Evaluation, Appeal, resolution and verified-finality timestamps must
   be canonical and monotonic; verified finality must follow the effective
   evaluation and, when present, its effective resolution. The verified
   Consumer-finality V2 evaluation, reproduction, and resolution UUIDs must
   equal those derived AAR authorities exactly; timestamp ordering alone is
   insufficient. Successful, failed
   and abandoned outcomes must precede the grace boundary, while expiry
   requires both `terminal_at == grace_expires_at` and the immutable
   `challenge_grace_deadline_elapsed` reason. The BFF rebuilds review signing
   frames and hashes and checks canonical signature/key encodings, but does not
   claim to reverify evaluator, reviewer, Appeal or resolution Ed25519
   signatures locally. Active-key ownership and signature verification remain
   Hepta write-time responsibilities before its authenticated read model
   crosses the sealed BFF boundary. This is a
   read-only projection: durable role mastery, challenge unlocks, immutable
   gameplay replay and automatic rematch remain unavailable. The timeline `Sync now`
   control is only live/archive catch-up and is no longer labelled as replay
   telemetry.
8. `/league/review` is the assignment-scoped independent Review Raid surface.
   It renders typed evaluator draft, two-reviewer attestation, quorum/finalize
   and reproduction actions from a frozen review bundle. The ordinary Review
   page also exposes the exact assignment-scoped paper source, bibliography,
   claim/evidence graph, dataset, evaluator and candidate result as a verified
   file library with explicit open/download actions. Browser reviewers may read
   the complete six-or-seven-object authority; Agent Bridge receives only the
   exact three-or-four executable-object projection. Route Paper, submission,
   assignment, player, round, slot, version, release/PaperBundle seals,
   ArtifactManifest seal, bundle hash, object key, digest, media type and byte
   length are re-bound before any CAS read. Every signature frame is rebuilt
   after a fresh scoped read; players never enter actor IDs, submission hashes,
   signatures or protocol JSON. Author scope alone grants no Review access. A
   mixed Author/Review identity may use this surface only for a separately
   assigned Paper where Hepta proves that player is not an Author; Author Room
   membership never grants Review authority.
9. `/api/papers/<paper-id>/timeline` combines query-bound Hepta room events
   with Nakama archives selected only from Hepta's player-scoped
   `member_research_sessions`. The response also repeats the current
   player-scoped Paper Room read model so the browser can update the phase,
   teammate connected/ready state and durable event list without reloading.
   Paper Room clients poll the incremental Hepta/Nakama cursors every 1.25
   seconds, retain only the non-secret Hepta integer cursor in session storage,
   and use a capped five-second reconnect backoff. Nakama positions and roster
   epochs stay in memory, independently for each already-visible logical
   research session, so one session's sequence cannot skip another's durable
   events. A reload safely replays every Nakama archive from zero. Catch-up
   pages run immediately while `has_more` is true and the UI does not claim
   `Live` until all pages are current. Nakama realtime delivery remains a hint;
   durable archive catch-up is the recovery authority. The Nakama HTTP key
   never reaches the browser.

CAS upload is streamed and capped independently at 32 MiB. The BFF recomputes
the requested `sha256:<64-lowercase-hex>` API digest and returns both that
prefixed BFF/CAS digest, the raw `artifact_sha256` used by Hepta's neutral
manifest, and the canonical `cas://sha256/<64-lowercase-hex>` ArtifactReference
URI; it does not create a research fact. The backing S3-compatible provider
remains an implementation detail under
`/<bucket>/objects/sha256/<64-lowercase-hex>` and is never exposed as a
research URI. A download is allowed only when the
authenticated player's P3 Paper Room contains an exact ArtifactManifest
object and storage-location binding for the digest, media type, URI and ACL.
The exact Paper Collaboration Kernel media allowlist is
`application/x-bibtex`, `text/csv; charset=utf-8`, `application/json`,
`text/markdown; charset=utf-8`, `application/pdf`,
`text/x-python; charset=utf-8`, `image/svg+xml`,
`text/plain; charset=utf-8` and `application/octet-stream`; exact
`application/zip` and `application/gzip` remain available for bundles. No
wildcard or parameter normalization is accepted. JSON commands remain capped
at 2 MiB.

If a mutation response is lost after CSRF rotation commits, the encrypted
session cookie remains valid. `POST /session/refresh` requires the exact
configured Origin and atomically returns a fresh synchronizer token, allowing
the same business idempotency key and body to be retried safely.

For a container deployment, use `deploy/alpha.env.example` only as a schema.
Generate every key independently and supply it through the X230 deployment
secret mechanism. Do not commit populated env files.

The runtime image is pinned to immutable builder and distroless base digests,
runs as UID/GID 65532 without a shell, carries exact OCI revision/creation and
Git source-tree/Cargo-lock provenance labels, and includes a deterministic CycloneDX 1.5 SBOM
at `/usr/share/doc/paper-raid-bff/sbom.cdx.json`. `scripts/check-image.sh`
requires a clean commit, regenerates and compares the SBOM, independently
rebuilds the image with `--no-cache`, compares image IDs, OCI index digests,
IID files and config digests independently, scans the exported
root filesystem, requires exactly one CycloneDX `type=file` component for
`/paper-raid-bff`, and proves its SHA-256 equals both independently built image
binaries. The host release build remains a compile/code gate, while the
runtime byte authority comes only from the immutable Debian Rust builder; this
avoids treating the Ubuntu host linker as equivalent to the pinned image
linker. The Docker build checks the SBOM-bound runtime SHA-256 before creating
the release tree, and the final OCI config repeats it in
`org.trillionnium.runtime-binary.sha256`. The gate also proves the credential
scanner rejects an injected sentinel fixture. Both builds explicitly disable Buildx-generated provenance
and SBOM attestations (`--provenance=false --sbom=false`) because provenance is
carried by immutable OCI labels and the independently generated, checked-in
SBOM; this keeps the loaded single-platform image identity deterministic.

To retain a verified local image for the eight-image release staging chain,
use an explicit, previously unused tag:

```bash
scripts/build-paper-raid-bff-image.sh \
  --image-ref trnm/paper-raid-bff:<release-id>
```

The staging builder runs the same two-build immutable image gate, requires the
result to be `linux/amd64`, and creates the requested local tag only after every
SBOM, rootfs, provenance, reproducibility, sentinel, and runtime check passes.
It refuses to replace an existing tag and never pushes. Direct
`scripts/check-image.sh` use remains disposable and removes its internal gate
tags on exit.

Regenerate the tracked runtime SBOM only from a clean commit with
`scripts/generate-runtime-sbom.sh services/paper-raid-bff/docker/sbom.cdx.json`.
That path runs the pinned builder twice with `--no-cache`, requires byte-equal
runtime binaries, rejects revision/tree/SBOM/self-hash embedding, and generates
the CycloneDX document twice before replacing the tracked file.

The image gate never installs a host plugin. It downloads Docker Buildx
`v0.36.1` from
`https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64`,
requires SHA-256
`48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778`,
loads it only through a temporary `DOCKER_CONFIG`, and deletes that directory
on exit. The shared downloader keeps partial bytes, resumes with HTTP ranges,
and fails after eight bounded attempts; every successful transfer is still
accepted only after the fixed SHA-256 check.
For an offline/repeated local gate, `PAPER_RAID_BUILDX_BIN` may name a cached
regular non-symlink file. The gate copies it into the same disposable
`DOCKER_CONFIG` and still enforces the pinned SHA-256 and Buildx version before
use; the cache path is never mounted or copied into the product image.

The release browser gate is also disposable. `scripts/check-browser-e2e.sh`
builds its runner from Playwright `1.49.1` at the immutable amd64 base digest
`sha256:ad57c625d284e8d287abcd40d18434582b2e354de71c8428cb080112b0c45960`
and installs the exact npm graph from `browser-e2e/package-lock.json`; it uses
the same checksum-locked temporary Buildx above and never installs a host
browser or npm package. By default it reaches the X230 deployment through an
existing host SSH tunnel at `http://127.0.0.1:17020` with `--network host`.

The gate accepts either a mode-0600 credentials JSON through
`PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE`, or the root-only alpha login keys
from `/etc/trillionnium-paper-raid/runtime.env` plus a mode-0600 public Agent
proof array selected by `PAPER_RAID_BFF_BROWSER_AGENT_BINDINGS_FILE`. Secret
values are mounted read-only into the disposable runner and are never passed
as command-line arguments, browser storage, or output. Three isolated author
browser contexts then prove distinct human identities and HttpOnly sessions,
empty local/session/IndexedDB storage, original-key recovery after a
committed-but-lost registration response, external Agent binding, Lobby entry,
and optional P5 Paper Room rendering. The only stdout result is a secret-free,
machine-verifiable `hepta.paper_raid.browser_e2e.result.v1` JSON object.

`scripts/check-browser-mobile-a11y.sh` is a separate source-candidate gate for
the ordinary login and `practice_unranked` browser path at exact 390px and
430px widths. It builds an exact reduced Cargo workspace offline with host
`rustc`/Cargo versions required to equal the pinned Docker builder, then runs
that current binary inside the immutable Rust container with an isolated tmpfs
PostgreSQL database and a typed read-only Hepta boundary double. Candidate and
runner inputs come from one read-only source snapshot whose SHA-256 is sealed
in the result; clean evidence uses an exact `git archive` of the reported
revision. The double verifies the complete Ed25519 Consumer assertion,
request-bound claims, lifetime, and replay boundary before answering. Pinned Chromium
is driven through keyboard input plus the CDP accessibility tree. The fixture
prequalifies one exact active owner binding in
both authorities so this gate can inspect the pre-Agent Practice stages; its
result therefore always records `production_bridge_pairing_proved=false`,
`production_bridge_execution_proved=false`,
`post_agent_focus_transition_real_e2e_proved=false`,
`developer_json_fallback_used=false`, and
`runtime_kind=pinned_host_toolchain_container_test_only`.
It is not production-image/SBOM evidence, a normal installed-Bridge pairing
proof, a deployment check, a post-Agent Practice completion proof, or a manual
screen-reader certification. Clean source is mandatory for releasable evidence;
`PAPER_RAID_BFF_BROWSER_ALLOW_DIRTY=1` exists only for explicitly labelled
development runs.
