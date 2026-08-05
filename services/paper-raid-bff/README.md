# Paper Raid BFF

`paper-raid-bff` is the narrow Consumer Edge for the Paper Raid alpha. It maps
three fixed external alpha identities, holds browser sessions, renders the
dark bilingual player shell, signs only Consumer Edge assertions, and
aggregates typed Hepta, Nakama archive and content-addressed object reads.

Authority stays outside this process:

- Hepta owns teams, papers, revisions, consent and research facts.
- Nakama owns ordered session events and replay roots.
- the object store owns immutable artifact bytes under
  `objects/sha256/<digest>`.
- the BFF owns only sessions, revocation generations, CSRF uses, assertion
  audit IDs and the exact idempotent Hepta response cache.

The alpha has two explicit edge scopes. `loopback_process` requires a
loopback bind. `container_loopback_publish` requires an unspecified container
bind and must be published by the host only on `127.0.0.1`, then reached over
an authenticated SSH tunnel. The public origin remains loopback HTTP in both
profiles, so the alpha cookie deliberately has `Secure=false`; it remains
encrypted/authenticated, `HttpOnly` and `SameSite=Strict`.

`/ready` requires BFF PostgreSQL, Hepta `/ready`, Nakama `/healthcheck`, a
successful read of the configured immutable CAS canary, and valid
trust/config. It always reports `finality=pending_only`; settlement is not an
availability dependency for writing a paper.

P1, P3 and P5 command routes are exact-whitelisted. Contribution ledgers,
evaluations, tolerance-aware reproductions, Appeals and independent
resolutions are submitted through their committed typed routes and read back
from the P5 review model. Missing sections render as `unavailable`; the BFF
never fabricates research facts or persists a Paper Room read model.

The browser alpha is a same-origin, external-script flow:

1. `/login` exchanges one fixed alpha key for an encrypted HttpOnly session;
   the page immediately clears the key and never uses browser storage.
2. `/league/start` first self-reads `/v2/hepta/players/me` and
   `/v2/hepta/agent-bindings`. A missing human enters secure key onboarding; a
   missing active binding enters external Agent onboarding; only both facts
   permit entry to the Lobby.
3. Human onboarding separates key generation/export from registration. The
   Ed25519 private key is generated in WebCrypto, exported once only inside an
   AES-256-GCM/PBKDF2-SHA-256 encrypted recovery bundle, and re-imported as a
   non-extractable in-memory signer. The BFF receives only the public key and
   proof of possession. An uncertain registration response is recovered via
   the self-read; a retry reuses the original imported key and never silently
   generates a replacement.
4. Agent onboarding accepts only the exact public proof signed by the external
   Agent under `hepta.paper_raid.agent_binding_proof.v2`. The Lobby also
   exposes the exact dual-signed `hepta.paper_raid.agent_binding_key_rotation.v2`
   request: both
   the currently bound and replacement Agent keys must sign the same scoped
   rotation. Agent private keys, seeds, mnemonics, runtime tokens and provider
   credentials never enter this browser or BFF.
5. `/league` reads Hepta Challenges, player-scoped matchmaking tickets and
   player-scoped three-person Team proposals. Queue and proposal decisions use
   the exact P3 typed routes.
6. `/league/formation/<proposal-or-team-id>` renders either the proposal or
   formal Team and exposes exact JSON editors for Team materialization,
   human-signed readiness, locking and Paper creation.
7. `/league/papers/<paper-id>` reads only the P3 Paper Room, event stream and
   P5 review state.
   It exposes typed proposal, decision, lease, revision, review, merge,
   evidence, experiment, claim, consent, finalization, evaluation,
   reproduction and Appeal actions. Acceptance, independent section review
   authorship consent and author Appeal signing frames are constructed from
   current Hepta facts and signed by the imported human key locally. External
   Agent and independent panel signatures remain external. The BFF signs only
   the Consumer user assertion.
8. `/api/papers/<paper-id>/timeline` combines query-bound Hepta room events
   with Nakama archives selected only from Hepta's player-scoped
   `member_research_sessions`. The Nakama HTTP key never reaches the browser.

CAS upload is streamed and capped independently at 32 MiB. The BFF recomputes
the requested digest and returns its configured content-addressed URI; it
does not create a research fact. A download is allowed only when the
authenticated player's P3 Paper Room contains an exact ArtifactManifest
object and storage-location binding for the digest, media type, URI and ACL.
JSON commands remain capped at 2 MiB.

If a mutation response is lost after CSRF rotation commits, the encrypted
session cookie remains valid. `POST /session/refresh` requires the exact
configured Origin and atomically returns a fresh synchronizer token, allowing
the same business idempotency key and body to be retried safely.

For a container deployment, use `deploy/alpha.env.example` only as a schema.
Generate every key independently and supply it through the X230 deployment
secret mechanism. Do not commit populated env files.

The runtime image is pinned to immutable builder and distroless base digests,
runs as UID/GID 65532 without a shell, carries exact OCI revision/creation and
Cargo-lock provenance labels, and includes a deterministic CycloneDX 1.5 SBOM
at `/usr/share/doc/paper-raid-bff/sbom.cdx.json`. `scripts/check-image.sh`
requires a clean commit, regenerates and compares the SBOM, independently
rebuilds the image with `--no-cache`, compares image IDs, scans the exported
root filesystem, and proves the credential scanner rejects an injected
sentinel fixture. Both builds explicitly disable Buildx-generated provenance
and SBOM attestations (`--provenance=false --sbom=false`) because provenance is
carried by immutable OCI labels and the independently generated, checked-in
SBOM; this keeps the loaded single-platform image identity deterministic.

The image gate never installs a host plugin. It downloads Docker Buildx
`v0.36.1` from
`https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64`,
requires SHA-256
`48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778`,
loads it only through a temporary `DOCKER_CONFIG`, and deletes that directory
on exit.

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
as command-line arguments, browser storage, or output. Three isolated browser
contexts then prove distinct human identities and HttpOnly sessions, empty
local/session/IndexedDB storage, original-key recovery after a committed-but-
lost registration response, external Agent binding, Lobby entry, and optional
P5 Paper Room rendering. The only stdout result is a secret-free,
machine-verifiable `hepta.paper_raid.browser_e2e.result.v1` JSON object.
