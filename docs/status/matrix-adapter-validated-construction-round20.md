# Matrix adapter validated-construction remediation — round 20

Status: source implemented; exact-head Rust/startup qualification pending  
Owner: `matrix-integration`  
Production authorization: `not_granted`

## Blocker addressed

The prior package exported `MatrixAdapterConfig::from_env`, `AppState::new` and
`AppState::from_env` from the same crate root used by the deployable binary. The
binary performed strict shared profile parsing before Tokio, but an embedded
caller could name those legacy constructors and bypass that preparation. This
was a real source/API gap even though the binary startup path was stricter.

## Implemented boundary

The former crate root is now retained unchanged as private
`services/matrix-entry-adapter/src/implementation.rs`, Git blob
`2897b042eb60671bf6631857ea45840807360037`. A new small public facade at
`src/lib.rs`:

- loads the implementation through a private module and does not re-export it;
- exposes synchronous `validate_process_environment` using the one shared-config
  `resolve_profiles` implementation;
- returns a `ValidatedMatrixAdapterEnvironment` token with a private field and no
  caller constructor/default/clone/copy;
- consumes that token in `AppState::from_validated_env`;
- exposes only the validated bind address and consuming router builder needed by
  the service binary;
- does not expose raw `MatrixAdapterConfig`, legacy `new`, or facade `from_env`.

`main.rs` obtains the token before constructing Tokio, passes it through the
async boundary, constructs validated state, captures the bind address, then
consumes state in the router. The exact profile compatibility re-export remains
unchanged for the three-package shared-parser contract. `Cargo.toml` and
`Cargo.lock` are unchanged.

## Fail-closed source gates

`scripts/check-matrix-adapter-api-boundary.py` checks the exact preserved
implementation blob, private module visibility, non-inventible token, shared
parser call, non-Unicode/conflict failure markers, pre-runtime ordering, absence
of legacy facade constructors, workflow wiring and documentation. Its mutation
suite covers implementation drift, public-module exposure, legacy constructor
return, token invention, parser bypass, non-Unicode fallback, startup reordering,
token dropping, wrapper policy growth and gate/document removal.

The checker runs in the Matrix source job and the aggregate development-document
gate. Neither checker can grant production authorization.

## Acceptance still required

The source change must pass on the exact commit:

```text
python3 scripts/test-matrix-adapter-api-boundary.py
python3 scripts/check-matrix-adapter-api-boundary.py
cargo fmt -p matrix-entry-adapter -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
```

A black-box process test must demonstrate invalid, empty, non-Unicode and
conflicting profile sources fail before a listener exists, while accepted
profiles reach the validated constructor. No queued or absent workflow step is a
pass.

## Remaining repository and external blockers

This round closes the identified embedded-constructor source/API bypass only.
Durable adapter result lookup and principal-bound response-loss reconciliation,
large-gap/membership/redaction/poison operational qualification, SQL deployment
roles and real role-negative tests, credential rotation/retention/SLOs, complete
semantic inventory, real Cargo/PostgreSQL/homeserver execution, cross-repository
first-playable and final exact-candidate gates remain open. V12-X1–X8 and the
accountable human decision remain independent evidence/approval gates.

`all_repository_gaps_closed=false`; `all_plan_gaps_closed=false`;
`production_authorization=not_granted`.
