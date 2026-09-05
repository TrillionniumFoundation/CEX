# Qualification source integrity and closed build packets

Status: supporting v12 implementation contract; unqualified working change  
Owners: platform-foundations and trnm-economy  
Parent: v12 implementation addendum, Blocks H and K  
Production authorization: `not_granted`

## Immutable source during qualification

A qualification run consumes an already committed dependency lock and generated
contracts. It must not regenerate `Cargo.lock`, repair a semantic inventory,
commit files or push a branch and then attach success to the earlier source SHA.
Dependency resolution and generated-contract changes belong in a reviewed source
commit before qualification. An inconsistent lock or absent generated inventory
must remain a failure; disabling `--locked` is not a repair.

The obsolete `seq53-matrix-lock-and-gate.yml` and `seq53-semantic-catalog.yml`
workflows are removed. They combined generation with source commits and pushes.
Their removal does not remove the Matrix package tests, the semantic contract
checks, any of the five v12 authority workflows, or the aggregate gate. The
Matrix/semantic supporting jobs and their strict commands remain. The build
packet regression rejects reintroduction of those two workflow files.

The root LICENSE records the existing MIT package license. SECURITY.md,
CONTRIBUTING.md, CHANGELOG.md and explicit CODEOWNERS routes document source
policy, contribution and security reporting. Their presence is not enforcement
of protected branches, proof of an independent review or legal/production approval.
Architecture remains Sequence 52 / ADR-004; these changes do not freeze Sequence 54.

## TRNM build prerequisites

The TRNM settlement workflow's build job depends on its static-contracts job.
It keeps the PostgreSQL requirement flag, owner-contract and byte-immutability
tests, strict Clippy and release build. Qualification uses the committed lock.
The collector receives the static dependency result and the actual `outcome` of
lock verification, formatting, tests, lint, build and source-unchanged steps.
Every value must be `success`. A skipped, failed, cancelled or missing step is
not eligible. Step `outcome`, rather than an error-tolerant `conclusion`, is bound.

These observations are passed by the workflow. This local collector does not
independently authenticate GitHub job execution; the existing exact-head hosted
record and final candidate collector still perform that separate qualification.
No workflow-generated list can prove another job ran without its dependency and
actual execution record. The source status must remain the pending, non-promoted
TRNM source contract rather than self-embedding a verified revision.

## Closed packet and provenance fields

`scripts/trnm_build_evidence.py` creates a new private directory under a supplied
existing parent outside the checkout. The hosting workflow uses RUNNER_TEMP.
No old destination is reused and the repository-owned `evidence/` directory is
never copied wholesale. The only permitted files are:

| File | Source / purpose |
|---|---|
| `Cargo.lock` | Exact committed lock blob |
| `settlement_v1.sql` | Exact committed TRNM service migration blob |
| `source-status.json` | Exact committed pending source status blob |
| `trnm-economy-service` | Bounded x86-64 ELF-shaped release output |
| `manifest.json` | Source identity, supplied step observations and payload digests |
| `SHA256SUMS` | Exact sorted checksums of the other five files |

A whole-checkout Git diff check, untracked-source check and explicit rejection of
assume-unchanged/skip-worktree flags precede collection. The three source inputs
are compared with `git cat-file` for the expected HEAD, not merely trusted because
the worktree reports clean. Inputs are read as bounded regular, single-link files
without symlink traversal, and parsing/hashing/copying use those acquired bytes.
Text inputs are limited to 16 MiB each and the binary to 256 MiB. Read-time changes,
missing or linked files, unexpected packet files, changed source/status, wrong
SHA, changed packet bytes and checksum/manifest disagreement fail closed.

The new private directory uses mode 0700; copied text uses 0600 and the binary
0700 before upload. Packet readback verification checks the exact closed file set
and hashes. Before returning the directory, the collector rereads the source and
binary inputs and rechecks Git identity. A failure removes only its newly created
packet, not the repository or a pre-existing directory. Run IDs and attempts are
canonical positive integers. Paths containing controls, parent traversal or
symlinks are rejected and error output does not expose payloads or credentials.

This is not an adversarial-filesystem sandbox: use a private immutable checkout,
trusted Git tooling and isolated build runner. Concurrent ancestor replacement,
host compromise, ignored build inputs, compiler trust and a reproducibility proof
are outside this collector. The ELF check establishes a header shape, not a valid
executable program, completed compiler invocation or running service. Source tree
and lock commitments identify the reviewed inputs, not an independent build proof.

## Manifest compatibility and artifact handling

The manifest keeps `trnm_cex_settlement_build_evidence_v1`, repository, commit,
tree, workflow run/attempt, contract, declared toolchain/image, checks and SHA-256
fields. It adds explicit step outcomes, event, static result, collection time and
`production_authorization=not_granted`. The declaration
`build_environment_identity=workflow_declared_not_image_digest_attestation` makes
clear that a configured Rust version or PostgreSQL tag is not a resolved immutable
container digest or proof of actual execution. The hash commitment covers the
canonical manifest without its own `payload_sha256` field.

The verifier is for this strengthened v1 packet profile; it does not silently
upgrade an older packet lacking these fields. Consumers must verify the full
packet and use the new artifact name containing both SHA and run attempt. Retain
historical evidence as historical rather than rewriting it into the new profile.
Artifact upload may not preserve local Unix permissions; permissions recorded at
collection do not constitute deployed file-permission evidence.

The collector's readback is local packet integrity, not an extracted-download
verification or remote artifact attestation. It no longer labels a local checksum
command as a downloaded-artifact test. Independently downloaded artifact validation
and final exact-head evidence binding remain separate requirements.

## Verification and remaining closure work

```text
python3 scripts/test-trnm-build-evidence.py
python3 scripts/trnm_build_evidence.py --verify /approved/private/packet
```

Collection is performed by the workflow only after its prerequisites and with
its explicit context. Do not set fake workflow success variables to claim a
production build. The test suite uses real disposable Git repositories and files,
synthetic ELF bytes and hostile mutations. It also checks workflow dependency,
locked commands, outcome binding, closed upload path and retirement of the two
source-writing workflows. It does not run Rust or PostgreSQL.

The existing repository-integrity job runs this suite, as does the TRNM static
job. All original authoritative checks remain. Complete CEX checkout, committed
lock resolution, current generated semantic inventory, compiler/database/runtime
tests, clean exact-head hosted execution, remaining module development and the
final candidate manifest are still required. Independent source review, protected
main, external deployment/recovery/custody/soak and V12-X1-X8 are not satisfied by
this document, the root policy files, a test count or a checksum packet.
