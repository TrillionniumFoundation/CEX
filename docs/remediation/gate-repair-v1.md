# Workflow trust and two-scope qualification repair

Status: source repair awaiting final exact-head execution and independent review.
Production authorization: `not_granted`.

## Failure reproduced, not waived

The full push-triggered P0 families failed at candidate hygiene before PostgreSQL
or Rust verification. The trust scanner classified `services/example/**` and
`run/evidence/**` as YAML aliases because its boundary accepted a slash before
`*`. It also rejected the repository's empty permission mapping and simple literal
branch/runner/port lists. These are source-parser incompatibilities, not evidence
that downstream tests passed and not an Actions allocation failure.

The existing byte-bound adapter retains the original implementation blob and all
prior POSIX/Windows correction points and negative tests. Its added exact-counted
corrections require an alias indicator to start a YAML token, not a glob suffix.
Only `permissions: {}` and closed one-line `branches`, `runs-on` and quoted `ports`
scalar lists are recognized. Every leaf is bounded and must contain simple literal
items: no aliases, tags, nested maps/lists, escaped strings, interpolation,
continuations, trailing tokens or arbitrary flow keys. Flow `uses`/event mappings,
mutable or dynamic action references, traversal, symlinks, and forbidden P0 PR
triggers remain rejected. Admitting a literal leaf never suppresses inspection of
any later action. This remains a deliberately restricted scanner, not a complete
YAML parser or an independent review of workflow behavior.

Six positive leaf fixtures each retain visible immutable actions and rejection of
mutable actions; sixteen hostile leaf/alias cases run in the existing self-test.
Every original self-test remains. The pinned implementation's source bytes and
identifier are not rewritten or made optional. The existing adapter composition
is retained rather than broadening a file allowlist or swallowing diagnostics.

## Separate identities, not reduced requirements

The v12 manifest schema and its fixed 13 evidence entries describe the bounded
exact-money/Hepta/document-integrity evidence contract. The Sequence 54 integration
adds further Matrix, external-Agent, toolchain and governance requirements. They
are different scopes and must not share one overloaded identifier.

The sole shared trigger therefore binds both:

| Trigger field | Required meaning |
|---|---|
| `qualification_scope` | Exact, backward-compatible v12 manifest scope, equal to the existing manifest template/schema and strict validators |
| `integration_qualification_scope` | Exact Sequence 54 non-regression scope, including 23 modules, global migration 0088, Matrix operator 0006 and four-boundary v3 controls |

The Sequence 54 checker independently requires both literal values. Missing,
empty, arbitrary or swapped values fail its embedded negative tests. The former
Sequence 54 value is retained verbatim under its unambiguous field; none of its
migration, runtime, binding, hosted, approval or external requirements is removed.
Manifest generators/validators, the manifest schema, evidence count, 18 base
requirements, migration heads and all governance checks remain unchanged. The
strict v12 evidence self-test must pass against the real checkout and shared
trigger; a label correction cannot manufacture any run or artifact.

A valid bounded v12 manifest is necessary, not sufficient, for complete Sequence
54 admission. The complete current required contexts, actual prospective merge,
Matrix and all other applicable contracts, immutable cross-repository tuple,
live protected-main readback, eligible independent approvals, external evidence
and final human authorization remain separately mandatory. This source repair
cannot declare repository closure or production activation.

## Earlier detection

The existing Sequence 54 PR integration job now runs the original candidate
hygiene and strict evidence wiring checks before source export and its unchanged
build/test/lint sequence. The five original push-authoritative workflows and
aggregate still execute their own original gates; none is replaced by this early
check. Full run discovery must include push and pull_request events, all pages
and attempts. A six-workflow PR-only view is not the complete acceptance set.
