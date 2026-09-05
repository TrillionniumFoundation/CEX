# Source-derived route semantics: bounded extraction and snapshot contract

Status: supporting implementation contract; unqualified working change  
Owner: platform-foundations / service contract owners  
Parent: v12 implementation addendum, Blocks H and K  
Production authorization: `not_granted`

This guide deepens the existing semantic inventory implementation. It neither
changes ADR-004 nor expands CEX's business or external-Agent authority. The
existing generated JSON schema remains `cex.repository-contract-semantics.v1`.
The new fields describe extraction uncertainty and coverage, not runtime proof.

## Problem and scope

The former generator searched the next 500 characters after a path literal for
HTTP method names. Adjacent routes and nested handler calls could contaminate a
route's methods; longer expressions could lose methods. The replacement uses a
bounded lexical scanner and balanced argument lists. It does not type-check Rust,
expand macros, evaluate conditional compilation or execute an Axum router.

`scripts/rust_route_contract.py` owns tokenization, literal decoding, declaration
boundaries and explicit outer-router method discovery. The generator retains
policy-based owner, authentication, authorization, data and retirement assignments.
Those policy declarations must still be verified against real service behavior.

## Supported declarations and uncertainty

| Construct | Extracted fact |
|---|---|
| `.route("/path", get(handler).post(other))` | Explicit GET and POST on this outer router |
| Qualified `axum::routing::get`, method service constructors | Same explicit method recognition |
| `MethodRouter::new().get(handler)` | Recognized explicit constructor chain |
| `on(MethodFilter::GET \| MethodFilter::POST, handler)` | Explicit filter union |
| `any` / `any_service` | `ANY`, not an invented finite method list |
| Transparent `layer`, `route_layer`, `with_state`, `handle_error` | Preserve observed outer methods; do not inspect their callback bodies |
| `.nest`, `.nest_service`, `.route_service` | Registration retained; effective methods unresolved |
| Helper-created, merged, fallback or unknown router expressions | `UNRESOLVED`; separately preserve any explicitly observed methods |
| Dynamic path or concatenating/path-generating macro | Retain a dynamic-path declaration, do not invent a literal value |

Normal and raw Rust strings, escaped Unicode/ASCII, character literals, lifetimes,
byte/C strings, line comments and nested block comments are handled as distinct
lexical structures. Methods inside handlers, strings and comments do not affect
the outer route. Raw registration identifiers such as `.r#route` are recognized.
Generic handler constructors are retained as unresolved expressions. Unsupported
or malformed registration structure fails instead of guessing.

Rust inputs are bounded to 16 MiB, 750,000 tokens and nesting depth 256. These are
deliberate source-check limits, not runtime capacity promises. JavaScript and
TypeScript files continue to participate in existing configuration extraction,
but their routing syntax is explicitly listed as not analyzed. Rust macro bodies
may contain lexical declarations; the extractor does not claim their expansion,
activation, reachability or effective prefixed path. GET does not cause a synthetic
HEAD fact. A function named `get` is not proven to be Axum merely by its spelling.

## Fact identity and generated fields

Every declaration retains its source line, one-based character column, registration
kind, path resolution, method resolution, observed methods and full registration
expression SHA-256. Route identity includes its column and expression digest,
so two declarations on the same line are not accidentally collapsed. Non-route
fact identity rules remain unchanged. Existing policy precedence and Consumer
projection-only restrictions remain in force.

`counts.unresolved_route_methods` and `counts.dynamic_route_paths` make uncertainty
visible. `route_extractor` records the parser digest and explicit-source scope.
`input_coverage` reports workspace, code and SQL counts and the non-Rust paths
whose routing was not analyzed. `unclassified=0` means policy rules covered all
emitted facts; it does NOT mean all effective routes or methods are resolved.

The changed route identities and additional metadata require deliberate regeneration
and review of the inventory. Do not hand-edit counts, copy an older inventory from
another commit, or create an empty inventory to satisfy the gate. Unknown methods
must not be turned into authorization assumptions.

## Consistent bounded input set

`scripts/semantic_source_snapshot.py` validates the explicit Cargo member list
against the module catalog, package names and each declared source entry point.
It reads the Git index inventory and rejects absent tracked source/SQL inputs,
including non-entry-point files omitted by a sparse or partial materialization.
CEX currently has explicit workspace members; wildcard member expansion is rejected
until implemented deliberately. Root/package manifests, policy, catalog, scripts,
source and SQL bytes used by generation are cached in one bounded in-memory snapshot.

Limits are 16 MiB per input, 256 MiB total and 20,000 inputs/index entries. Paths
must remain inside the repository and must not traverse symbolic links. Nonregular
files, FIFO inputs and detected read-time changes are rejected. Parsing and hashing
consume the same snapshot bytes, followed by byte and inventory rechecks before
rendering. Source/policy/parser changes, new files, or Git inventory changes during
generation invalidate the attempt.

This is a local working-tree consistency check, not proof of a trusted Git origin,
an approved commit, Cargo dependency closure or adversarial filesystem isolation.
The source roots and SQL globs remain explicit; vendor and build directories are
excluded. A dedicated immutable checkout plus the existing exact-tree integrity
attestation and compiler gates is still required for qualification. Extraction may
include uncommitted source changes during development; it cannot qualify them.

## Output and verification commands

Use Python 3.11 or newer and Git. From a complete checkout:

```text
python3 scripts/test-rust-route-contract.py
python3 scripts/test-semantic-source-integration.py
python3 scripts/test-consumer-route-contract.py
python3 scripts/generate-repository-semantics.py --write
python3 scripts/generate-repository-semantics.py --check
python3 scripts/check-consumer-projection-boundary.py
```

`--write` is an explicit developer operation. It renders fully before replacing a
repository-local JSON file through an fsynced temporary file and atomic rename.
Absolute external paths, parent traversal and symlink outputs are rejected; failed
replacement preserves the previous file and removes its temporary file. Operate
inside a private checkout; these checks do not protect against a hostile process
continually replacing ancestor directories. `--check` never writes or repairs the
inventory. An absent or stale file remains a failure. Generated output must be
reviewed and committed before final-candidate freezing.

## Consumer projection protection

`scripts/check-consumer-projection-boundary.py` now consumes decoded declarations
from the same extractor. Forbidden authority path segments cannot be hidden with
Rust string escapes or by choosing `route_service` / `nest_service`. Dynamic paths
in these projection files fail with an explicit unresolved-boundary diagnostic;
they require a reviewed, concrete definition rather than silent omission.
All existing SQL mutation, DDL, authority-literal, endpoint and policy checks remain.
This does not replace SQL role enforcement, tenant tests or runtime authorization.

## CI integration and evidence boundary

The existing `rust-service-gate` repository-integrity job runs the three new test
suites, generated inventory `--check` and Consumer projection checks before its
existing document/integrity pipeline. Windows, Linux, Hepta and all prior steps
are retained. The semantic helper workflow runs the same regressions and checks
on the exact PR head using an ephemeral hosted runner, read-only permissions and
no persisted checkout credentials. Its path filters include parser/helper/tests.
No source self-patching, dependency refresh, forced push, test waiver or approval
step is added. The final v12 authority contexts and candidate manifest remain
separate, mandatory evidence.

Parser tests exercise actual extraction; integration tests construct disposable
synthetic Git workspaces and exercise the real generator and snapshot guards;
Consumer tests exercise the real source guard. None compile Rust, execute services,
prove PostgreSQL recovery, test a homeserver or qualify the full CEX repository.
A source subset may run these tests but must not emit a purported complete CEX
inventory. Independent review, hosted execution and production gates stay open
until their actual evidence exists.
