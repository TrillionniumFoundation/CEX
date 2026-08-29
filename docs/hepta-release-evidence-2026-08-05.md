# Hepta Research League evidence — 2026-08-05

## Scope

This evidence qualifies the Hepta-owned Research League and its signed
Hepta-to-Nakama authorization boundary. It is not a canonical cross-repository
release or Chain-finality claim.

## Immutable dependencies

The workspace consumes the following packages from the canonical private Chain
repository at exact revision
`e73d1a930991f0e308bf72854b334b6191c7fcc3`:

- `trnm-research-protocol = 0.1.0`
- `trnm-finality-types = 0.1.0`
- `trnm-finality-verifier = 0.1.0`

`Cargo.lock` records the full Git source for all three packages. No dependency
reads a sibling Chain worktree.

## Nakama authorization contract

`POST /v1/hepta/match-authorizations` emits Nakama's exact signed authorization
object: `claim`, `issuer_key_id`, and a canonical padded-base64 Ed25519
signature. The claim binds the logical match, challenge, Agent DID/current
public key, authenticated Nakama user, participant slot, role, ruleset, dataset,
challenge snapshot, and Unix validity window.

The independent Rust golden test reproduces Nakama's frozen signature:

```text
ODDi1QuKNlOnykERx29Kkk7ORAlMAGn4MrS2mjLvIkstfEpdpOWp55/PJ6LZjZfUCD+BuzbJAhUWUnkerU+iCA==
```

Exact authorization issuance and consumption-acknowledgement retries return the
original immutable object. Conflicting reuse fails closed. No bearer
`match_token` is issued or persisted.

## Gates

The following passed from the canonical Hepta root:

- project preflight: zero boundary errors; two unrelated historical path
  warnings remain outside this feature;
- OpenAPI YAML and SDK fixture parsing/invariants;
- `cargo deny check sources`;
- `cargo test --locked -p hepta-research-league`;
- `cargo check --locked --workspace`;
- the PostgreSQL-backed release script against temporary
  `postgres:17.6-alpine3.22@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94`.

The live PostgreSQL gate exercised migration 0031, restart recovery, concurrent
instances, non-overlapping outbox leases, lease expiry, and replay by a recovery
worker. The temporary database container was removed after the run.

## Remaining system blockers

The historical bespoke Chain/World harness and its evidence were moved
byte-for-byte to the Integration repository's quarantine and are not an active
gate. A canonical system release still requires typed ingress through the
canonical consensus application plus AppHash/finality-bound receipts at clean,
immutable component revisions.
