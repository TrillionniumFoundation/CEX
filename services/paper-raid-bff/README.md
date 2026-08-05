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
  audit IDs, idempotent response cache and bounded read cache.

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

The committed P1 command routes are exact-whitelisted. Matchmaking,
collaboration artifact registration, review, reproduction and appeal are
typed fail-closed placeholders until their authoritative Hepta contracts are
rebased into this branch. Missing sections render as `unavailable`; the BFF
never fabricates research facts.

There is deliberately no browser artifact upload/download route on this base
revision. P1 does not expose the P3 neutral bundle binding and separate
storage-location mapping, so accepting a member-supplied URI, media type or
digest would create a second artifact authority. The CAS adapter is complete
and its append/read semantics are tested, but it becomes browser-reachable
only after the P3 room aggregate and artifact ACL contract are rebased. That
future upload route must use its own streamed 32 MiB cap; JSON commands remain
globally capped at 2 MiB.

If a mutation response is lost after CSRF rotation commits, the encrypted
session cookie remains valid. `POST /session/refresh` requires the exact
configured Origin and atomically returns a fresh synchronizer token, allowing
the same business idempotency key and body to be retried safely.

For a container deployment, use `deploy/alpha.env.example` only as a schema.
Generate every key independently and supply it through the X230 deployment
secret mechanism. Do not commit populated env files.
