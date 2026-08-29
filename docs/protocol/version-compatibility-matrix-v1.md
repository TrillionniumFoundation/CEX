# Protocol and storage compatibility matrix

Status: active compatibility contract

| Capability | Read compatibility | New-write authority | Migration/translation rule | Retirement condition |
|---|---|---|---|---|
| Ledger monetary routes | legacy records may be read for audit/backfill | exact v2 account/effect functions only | never infer missing historical precision; no major-unit conversion | all value-writing v1 routes remain `410 Gone` and compatibility writers are removed or break-glass governed |
| Invocation Ledger contract | v1 non-value metadata may remain readable | immutable exact Invocation contract | one Invocation binds one reserve and terminal settlement identity | no unbound exact invocation exists |
| Provider dispatch | legacy diagnostic rows may be inspected | durable exact-authority dispatch command | ambiguous transport becomes `reconcile_required`; evidence is append-only | no automatic retry path crosses possible side-effect boundary |
| Audit | historical source rows can be bounded-backfilled | authenticated Audit v2 plus durable outbox | cursor/revision and intent creation are atomic | source baseline complete and restart is a no-op |
| TRNM economy | protocol v2 whole-credit intent remains accepted at boundary | exact Ledger minor-unit effects | whole credits convert only by checked scale multiplication; fractional exact state fails closed | no legacy balance/reserved monetary write remains |
| Hepta Agent registry | legacy v1 profiles remain readable for compatibility | Paper Raid v2 binding proof and key snapshot | v1 claimed owner/key never authorizes v2 admission | v1 onboarding clients and data dependencies reach zero |
| Paper Raid research | legacy stored phase values remain readable | versioned v2/v3/v4 command and evidence contracts according to capability | explicit projection maps compatibility terms; no implicit state collapse | all supported clients consume the canonical projection |
| Nakama authorization | diagnostic legacy material may be retained | one complete signed ordered authorization set per roster epoch | replacement changes one disconnected slot with fresh IDs | partial/mixed-root admission is impossible |
| Nakama completion | old unsigned echoes are non-authoritative | pinned signed completion plus full archive and Hepta receipt | response-provided key is diagnostic only | every live completion has pinned authority and recomputed roots |
| Chain finality | pending legacy projections may be read | independently verified typed receipt under explicit mode | `pending_only` cannot ingest finality; challenged evidence holds settlement | validator/trust-anchor lifecycle and v2 verification are fully qualified |
| Publication | draft/export artifacts may be downloaded | unanimous `PublicationReleaseV1` | PaperBundle completion never auto-submits | external submission integration has separate consent and legal approval |
| Release evidence | historical runs remain audit records | generated exact-tree candidate manifest | no run/hash is written back into source plan; a later commit requires requalification | immutable retention and independent approval policy are active |

## Compatibility invariants

1. Read compatibility never grants write authority.
2. A compatibility projection must name its source version and cannot invent precision, identity, consent or finality.
3. Replays require identical immutable input; same identity with different content is a collision.
4. Version negotiation is explicit and fail-closed for unsupported authoritative writes.
5. Retirement requires telemetry and an explicit exit criterion; elapsed time alone is insufficient.
6. Cross-repository contracts are versioned/pinned artifacts, not sibling Cargo path dependencies.
