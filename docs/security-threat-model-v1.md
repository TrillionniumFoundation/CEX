# CEX and Hepta security threat model

Status: active threat model

## 1. Protected assets

- exact Ledger balances, reservations, entries and operation identity;
- Invocation, Execution, provider and Audit provenance;
- human and Agent signing keys and immutable key snapshots;
- PaperBundle, claim/evidence, code/data/environment and publication-release hashes;
- Nakama authorization sets, ordered event archives and completion receipts;
- Chain commands, trust anchors and verified finality receipts;
- database migration, runtime and finality role credentials;
- candidate manifests, SBOM, provenance and hosted run identity.

## 2. Trust boundaries

| Boundary | Trusted input | Untrusted input | Required control |
|---|---|---|---|
| Consumer Edge → Hepta | pinned issuer/key and canonical assertion | caller identity fields, body and replayed nonce | Ed25519 verification, method/path/body hash, expiry and operation-scoped idempotency |
| External Agent → Hepta | proof from the bound Agent key | claimed owner, Agent ID or replacement key | proof of possession, dual-sign rotation, nonce uniqueness and immutable key snapshot |
| Hepta → Nakama | signed authorization/control command | response-provided keys or partial roster sets | pinned authority, complete ordered set, strict JSON, roster-root recomputation and signed receipt |
| Runtime → Ledger | exact contract and service identity | legacy amount, dual monetary intent or invented receipt | exact minor units, scoped idempotency, authenticated receipt validation and fail-closed mode |
| Execution → provider | durable claimed command | timeout, ambiguous transport, mutable provider response | commit-before-I/O, bounded lease, `reconcile_required`, immutable artifact URI/SHA-256 |
| Hepta → Chain/finality | pinned validator set or trust anchor | HTTP success, bearer token or response-supplied key | independent receipt verification and explicit finality mode |
| CI → release evidence | exact SHA/tree and pinned Actions | mutable status prose, template hashes or another SHA's run | exact-run polling, immutable artifact digest, SBOM/provenance and manifest validation |

## 3. Principal threats and controls

| Threat | Failure mode | Repository control | Residual/external requirement |
|---|---|---|---|
| forged or replayed Consumer assertion | cross-player write | canonical assertion frame, expiry, nonce/idempotency and subject/player/Nakama binding | issuer-key custody and rotation review |
| stolen Agent key | malicious research action | proof-of-possession, dual rotation, compromise revocation, immutable epochs | incident response and owner verification |
| author-key compromise | invalid authorship/publication | proof-of-possession, immutable signature snapshot and `integrity_hold` | legal/authorship review |
| partial or mixed Nakama roster | unauthorized session member | atomic complete-set consumption, roster/version/root checks | live Nakama topology rehearsal |
| forged completion | false research completion | pinned completion authority, archive/event/root recomputation | independent game-server review |
| duplicate remote side effect | double charge, dispatch or reward | durable command identity, claim commit before I/O, exact replay | real provider definite/non-definite outcome artifacts |
| timeout treated as failure | unsafe retry | `reconcile_required`, no automatic retry after possible dispatch | operator reconciliation procedure |
| float or legacy money path | rounding or authority split | integer minor units, v1 route retirement, static exclusion gates | independent financial-control review |
| outbox loss or double delivery | missing/duplicate audit or research event | transactional outbox, lease ownership, exact replay and append-only evidence | sustained queue-age/SLO qualification |
| database-role escalation | finality or schema corruption | one-shot migrator, ordinary runtime role, isolated finality role and minimal definer surface | real credential issuance and custody review |
| evidence-file substitution | forged receipt or key material | content hashes, bounded reads, canonical files and exact-tree provenance | hardened runner/storage topology review |
| CI status substitution | qualify wrong tree | exact SHA/tree polling and candidate manifest binding | branch/ruleset enforcement and independent approval |
| content-addressed bytes unavailable | unreproducible paper | manifest hash/URI and fail-closed retrieval | retention, backup and object-store DR qualification |

## 4. Abuse and privacy considerations

Research inputs may contain personal, licensed, confidential or export-controlled data. Challenge packs must declare data provenance, allowed use, retention and deletion policy. Logs and Audit events must not store model API keys, raw private keys, unrestricted paper bodies or unnecessary personal data. The platform does not host model credentials or platform-owned competitor Agents.

Agent output is untrusted research assistance. Human authors retain factual, ethical, licensing and publication responsibility. No score or reward may be interpreted as scientific truth.

## 5. Security acceptance

Repository acceptance requires canonical-frame tests, tamper negatives, auth failures, replay/collision tests, role-bound migration tests, exact-tree integrity and strict PostgreSQL recovery. It does not replace penetration testing, key-custody review, financial-control review, legal review, provider approval or final human go/no-go. Those remain `blocked_upstream` until independently evidenced.
