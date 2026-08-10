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
`PAPER_RAID_ACCESS_DATABASE_URL`, `PAPER_RAID_ACCESS_OPERATOR_SUBJECT`, and the
configured retention policy ID. It deliberately has no fallback to
`PAPER_RAID_BFF_DATABASE_URL`, and the resident BFF never reads the operator
DSN. The operator DSN is a schema-owner/migration credential because the CLI
runs the idempotent BFF migrations before each command; it must not be placed
in the BFF runtime environment.

In `invite_alpha`, BFF startup performs only a read-only schema-presence check
and fails closed until the operator CLI has provisioned it. Its separate
`PAPER_RAID_BFF_DATABASE_URL` role should receive only the SELECT/INSERT/UPDATE/
DELETE grants required by sessions, one-time redemption, access-directory
reads, append-only audit insertion, telemetry, and durable quota counters; it
does not need schema-owner or trigger-changing privileges. The existing fixed
Alpha startup migration behavior remains unchanged for compatibility.
Representative invocations are:

```bash
paper-raid-accessctl batch-create --label cohort-a --max-issued 50 \
  --expires-at 2026-09-01T00:00:00Z
paper-raid-accessctl invite-issue --batch-id <uuid> --subject alpha-author-08 \
  --display-name 'Author Eight' --nakama-user-id <uuid> --player-id <uuid> \
  --scopes author --author-roles evidence --expires-at 2026-08-20T00:00:00Z \
  --credential-expires-at 2026-09-20T00:00:00Z
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
suspend/close. Account export contains directory and
lifecycle metadata but never credential hashes or secret material. The access
credential survives suspension but never has its expiry extended; rotate it
explicitly after reactivation when it has expired. The access schema is
BFF-local and contains no Paper facts, Agent secrets, signatures,
scores, rankings, rewards, or legacy state.

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

After pairing, the Agent has only four dedicated endpoints: `GET
/api/agent-bridge/binding` and `POST` health, inbox and proposals. Every request
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

This is a Bridge-assisted proposal workflow, not autonomous play: the browser
selects an assigned work item and registered manifest, while the local Agent
signs and submits the bounded proposal. The copied command includes both the
manifest UUID and Hepta's authoritative `artifact_manifest_hash`.

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
It reports `finality=paper_scoped_projection`: a fresh Paper is
`pending_finality`, while a verified Receipt V2 is projected as
`verified_finality`. Ranking, score, reward and economic eligibility remain
independently fail-closed and are never inferred from finality alone.

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
   FIFO-three selection.
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
   The Room also renders the immutable ChallengeRuleset snapshot as player
   rules: template, product difficulty, version, duration/grace, every victory
   requirement and every phase gate. The authoritative deadline drives a live
   deadline/overtime/grace countdown, while `outcome`, `outcome_reason` and
   terminal time are read back as game facts. Captain-only failed/abandoned
   controls post typed reason codes through `/api/papers/<paper-id>/outcome`;
   the BFF re-reads Hepta and derives the Paper locator and aggregate version.
   Expired remains hidden and is rejected server-side until the snapshotted
   grace deadline has elapsed. Players never enter JSON, UUIDs, versions or
   hashes for these actions.
   A canonical three-role roster is enforced end to end: Captain owns phase
   checkpoints and cross-player work orchestration; Evidence owns evidence,
   citation and claim mutations; Experiment owns preregistration and retained
   runs. Artifact, section, revision and consent actions stay collaborative.
   The normal UI renders only actions authorized for the current role; legacy
   noncanonical teams retain their original permissive behavior.
8. `/league/review` is the assignment-scoped independent Review Raid surface.
   It renders typed evaluator draft, two-reviewer attestation, quorum/finalize
   and reproduction actions from a frozen review bundle. Every signature frame
   is rebuilt after a fresh scoped read; players never enter actor IDs,
   submission hashes, signatures or protocol JSON. Author-scoped identities
   are denied this surface even if they also declare a review scope.
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
