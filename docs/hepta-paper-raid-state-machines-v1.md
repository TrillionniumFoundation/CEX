# Hepta Paper Raid state-machine contract

Status: active architecture and implementation contract

Paper Raid has three independent lifecycles. No implementation may collapse them into one status or infer later authority from an earlier lifecycle.

## 1. Research lifecycle

| State | Authorized command | Preconditions | Durable outcome | Failure/recovery rule |
|---|---|---|---|---|
| `forming` | create team, bind members | Consumer assertion valid; every Agent binding has proof of possession | team proposal and immutable binding snapshots | replay returns the original response; payload collision fails |
| `preregistering` | freeze question, hypotheses, data and method | complete accepted roster and Collaboration Compact | preregistration revision and evidence hashes | stale expected version fails; no implicit overwrite |
| `researching` | collect sources and claims | preregistration accepted | claim/evidence work records | external bytes remain content-addressed; Hepta stores hashes and ACL references |
| `experimenting` | run baseline, extension or ablation | data/code/environment manifests frozen for the run | experiment, result and figure lineage | timeout does not invent completion; retry uses a new attempt identity |
| `drafting` | create paper revisions | research facts exist | immutable revision lineage | controlled return from later review states creates a new revision |
| `integrity_review` | verify claims, disclosures and lineage | draft bundle complete | review findings and hold/release decision | compromised key or evidence mismatch enters `integrity_hold` |
| `reproducing` | submit independent reproduction report | reproducible bundle frozen | reproduction decision and artifacts | failure returns to drafting or remains on hold; it does not erase evidence |
| `author_approval` | sign authorship and release scope | current human keys valid; author order explicit | immutable signatures and key snapshots | rotation/revocation invalidates forming consent; historical signatures remain auditable |
| `submission_ready` | freeze `PaperBundleV2` | all required human approvals and checks pass | immutable bundle hash | does not imply publication, Nakama completion, or finality |
| `integrity_hold` | remediate or explicitly terminate | signed operator/reviewer decision | hold reason, evidence and transition history | no automatic release; remediation produces new versioned evidence |

External publication requires a separate `PublicationReleaseV1` signed by every human author.

## 2. Nakama collaboration lifecycle

| State | Authority | Entry condition | Exit evidence |
|---|---|---|---|
| `lobby` | Nakama | one complete ordered Hepta authorization set | room identity and roster epoch |
| `ready` | Nakama | all 3–5 members and external Agents admitted against one roster root | readiness event set |
| `active` | Nakama | signed create/resume command accepted | ordered authoritative actions and reconnect evidence |
| `checkpointed` | Nakama | checkpoint command and complete event prefix | checkpoint root and archive position |
| `completed` | Nakama | terminal event names the finalized PaperBundle hash | signed completion object and full event archive |
| `abandoned` | Nakama | explicit abandonment/expiry policy | terminal reason and retained event prefix |

A roster replacement increments the session roster version, changes exactly one disconnected slot under the frozen rules, and supersedes the previous authorization epoch. It never changes the Hepta team-consent version.

## 3. Settlement and finality lifecycle

| State | Meaning | Permitted transition | Evidence requirement |
|---|---|---|---|
| `uncommitted` | no Chain command accepted | prepare or remain local | signed local preparation only |
| `pending_finality` | canonical command accepted but finality not independently verified | finalize, challenge, or remain pending | command identity and immutable receipt lookup material |
| `finalized` | pinned validator/trust-anchor verification succeeded | no ordinary rollback | verified finality receipt and content hash |
| `challenged` | evidence or appeal blocks reward/ranking release | resolve to pending/finalized/denied | dispute identity and signed decision |
| `resolved` | challenge resolution is durable | follow explicit resolution disposition | complete resolution lineage |

`HEPTA_FINALITY_MODE=pending_only` permits research completion but forbids any path from claiming verified finality. HTTP success or a bearer token is never finality.

## 4. Cross-product invariants

1. Nakama `completed` does not move Research to `submission_ready` without a valid PaperBundle.
2. Research `submission_ready` does not move Settlement to `finalized`.
3. Chain unavailability cannot make the paper unavailable; it keeps ranking/economic release pending.
4. Every remote command is persisted with canonical signed bytes before network I/O.
5. No SQL transaction remains open across a Nakama, provider, Ledger, or Chain call.
6. A timeout after a possible side effect yields pending/reconciliation state, not automatic replay.
7. Operation identity, expected version, idempotency key, and payload hash are scoped and immutable.
8. Human authority covers scope, ethics, licenses, authorship, factual responsibility and publication; Agents cannot assume it.

## 5. Concurrency and crash recovery

- Aggregate writes use expected positive versions and transaction-scoped locks.
- Outbox claims commit before delivery; lease owner and expiry are durable.
- Wrong-owner acknowledgement fails closed.
- Expired claims may be reclaimed without creating a second event identity.
- Applied idempotent responses survive response loss and process restart.
- Reconciliation never changes an immutable signed command; it appends evidence and transitions.

## 6. Required verification

Golden contract tests cover canonical bytes and tamper negatives. PostgreSQL integration must additionally execute restart persistence, concurrent disjoint claims, lease-expiry recovery, readiness and metrics with `HEPTA_REQUIRE_POSTGRES_TESTS=1`. The exact-SHA hosted result is the evidence; this document is the contract.
