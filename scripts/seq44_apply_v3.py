#!/usr/bin/env python3
"""Apply, qualify, and publish the final CEX sequence-44 repository closure tree."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_SHA = "04dd42500f32a63ab7ad377487dc2aa993929aae"
BRANCH = "automation/seq44-residual-gap-closure"
BOOTSTRAP = {
    "scripts/seq44_apply_v3.py",
    ".github/workflows/seq44-residual-gap-closure-v3.yml",
}

# name, path, class, purpose, critical invariant focus, verification focus, operations focus
MODULES = [
    ("shared-types", "crates/shared-types", "shared library", "cross-service domain and transport types", "integer money, stable identifiers, and compatible wire representations", "serialization goldens, invalid shapes, and all-consumer compilation", "wire changes require coordinated rollout and rollback"),
    ("shared-errors", "crates/shared-errors", "shared library", "common error categories and conversion rules", "typed retryability, preserved causality, and secret-safe rendering", "conversion, stability, redaction, and source-chain tests", "alerts use stable categories rather than message text"),
    ("shared-tracing", "crates/shared-tracing", "shared library", "common tracing, correlation, and telemetry initialization", "no secret fields and no unbounded user-controlled labels", "initialization, propagation, redaction, and exporter-failure tests", "telemetry degradation is visible but never changes correctness"),
    ("shared-config", "crates/shared-config", "shared library", "typed configuration and runtime-profile validation", "production rejects defaults, weak secrets, malformed flags, and ambiguous precedence", "environment matrices, production negatives, reload governance, and redaction", "configuration rollout and rollback are evidence-bound"),
    ("hepta-paper-raid-contracts", "crates/hepta-paper-raid-contracts", "protocol library", "typed Hepta and Paper Raid request, response, event, and evidence contracts", "versioned canonical encodings, exact identities, signatures, and idempotency", "cross-language goldens, tamper negatives, and consumer compilation", "breaking change requires a new version and staged cutover"),
    ("identity-service", "services/identity-service", "service", "identity authentication and credential lifecycle", "authenticated issuance, scoped authority, hashing, expiry, revocation, and bounded replay", "OpenAPI, scope, lifecycle, database, restart, and production-profile tests", "credential rotation, revocation, compromise, and break-glass are monitored"),
    ("ledger-service", "services/ledger-service", "service", "exact-money balances, reservations, postings, and immutable receipts", "integer minor units, conservation, unique operation identity, exact replay, and receipt recovery", "migrations, concurrency, fault matrices, soak, backup/restore, and response loss", "posting latency, parity, reconciliation, and saturation are observed"),
    ("trnm-economy-service", "services/trnm-economy-service", "service", "TRNM value intent and settlement coordination", "success, non-execution, and unknown outcome remain distinct and cannot double-settle", "protocol, PostgreSQL lifecycle, receipt recovery, partial-upgrade, and outcome matrices", "unknown outcome age, reconciliation backlog, and credential health are monitored"),
    ("gateway-service", "services/gateway-service", "service", "authenticated invocation ingress and exact reservation orchestration", "authentication precedes mutation, retries are idempotent, and timeout remains unknown", "static wiring, authentication negatives, replay, and reserve fault matrices", "reserve failure, dependency saturation, and reconciliation are observed"),
    ("execution-service", "services/execution-service", "service", "authorized provider execution and terminal settlement", "terminal states are exclusive, leases recover safely, and ambiguous provider results remain unknown", "HTTP, worker, lease, terminal, settlement, and provider-reconciliation tests", "retry budgets, dead letters, lease age, and settlement backlog are monitored"),
    ("audit-service", "services/audit-service", "service", "append-only security, financial, and operational evidence", "attribution, ordering, immutability, hash binding, and redaction", "source-baseline, append-only, outbox, restart, query, and export tests", "ingestion, outbox lag, retention, hash mismatch, and export are monitored"),
    ("capability-service", "services/capability-service", "service", "capability discovery and authorization decisions", "default deny with explicit subject, action, resource, scope, expiry, and revocation", "OpenAPI, allow/deny, normalization, expiry, and unknown-capability tests", "decision errors and policy freshness are observable without policy leakage"),
    ("consumer-entry-api", "services/consumer-entry-api", "service", "consumer-facing account, world, league, wallet, and orchestration APIs", "session authentication, exact money, typed upstream state, and no repository bypass", "route, session, identity, exact-money, upstream-fault, database, world, and league tests", "domain SLOs, dependency health, session failures, and projection divergence are monitored"),
    ("hepta-research-league", "services/hepta-research-league", "service", "research league, collaboration, review, finality, leases, and durable outbox state", "restart persistence, disjoint claims, owner checks, lease recovery, and fail-closed finality", "strict PostgreSQL recovery, lint ownership, protocol goldens, concurrency, and tamper tests", "lease age, outbox backlog, claim conflict, finality, and readiness are monitored"),
    ("paper-raid-bff", "services/paper-raid-bff", "service", "Paper Raid onboarding, play, review, evidence, and recovery BFF", "no financial authority, authenticated idempotent mutation, and hash-bound artifact access", "endpoint, OIDC, access, CAS, database, E2E, browser, and upstream-failure tests", "session, pairing, CAS integrity, upstream latency, retry, and audit status are monitored"),
    ("matrix-entry-adapter", "services/matrix-entry-adapter", "service", "authenticated Matrix event translation and command routing", "sender and room authority, event deduplication, replay rejection, and bounded payloads", "parser, auth, dedup, routing, registry reload, delivery, and restart tests", "event lag, rejects, duplicates, rate limits, and dead letters are monitored"),
    ("matrix-bot-relay", "apps/matrix-bot-relay", "application", "bounded delivery of approved CEX messages to Matrix", "at-least-once transport cannot cause duplicate execution and credentials remain external", "delivery, duplicate, encoding, restart, timeout, and rate-limit tests", "queue age, retries, credential failures, room errors, and dead letters are monitored"),
    ("matrix-bot-poller", "apps/matrix-bot-poller", "application", "Matrix polling, durable cursor advancement, and event handoff", "cursor and handoff atomicity, harmless duplicates, exclusive leases, and safe expiry recovery", "cursor restart, duplicate, concurrent ownership, timeout, rate-limit, and handoff tests", "poll lag, cursor age, lease conflict, event-store health, and handoff failure are monitored"),
]


def run(*args: str, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(args), cwd=ROOT, check=True, text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
    )


def out(*args: str) -> str:
    return run(*args, capture=True).stdout.strip()


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)} preimage count {count}, expected 1")
    path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


def replace_count(path: Path, old: str, new: str, expected: int) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path.relative_to(ROOT)} preimage count {count}, expected {expected}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def append_once(path: Path, marker: str, body: str) -> None:
    text = path.read_text(encoding="utf-8")
    if marker in text:
        return
    with path.open("a", encoding="utf-8", newline="\n") as handle:
        if text and not text.endswith("\n"):
            handle.write("\n")
        handle.write(body)


def write(path: Path, content: str, executable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")
    if executable:
        path.chmod(0o755)


def verify_bootstrap() -> None:
    if os.environ.get("GITHUB_REF_NAME") != BRANCH:
        raise SystemExit(f"wrong branch: {os.environ.get('GITHUB_REF_NAME')!r}")
    run("git", "merge-base", "--is-ancestor", BASE_SHA, "HEAD")
    changed = set(out("git", "diff", "--name-only", f"{BASE_SHA}..HEAD").splitlines())
    if changed != BOOTSTRAP:
        raise SystemExit(f"unexpected bootstrap files: {sorted(changed)}")
    if out("git", "status", "--porcelain"):
        raise SystemExit("bootstrap checkout is dirty")


def patch_release_contract() -> None:
    contract = ROOT / "scripts/check-release-evidence-contract.py"
    replace_once(
        contract,
        '''CONTEXT_HOSTED_GATE_ALLOWED_FIELDS = {
    "repository", "branch", "head_branch", "head_sha", "workflow_path", "event",
    "status", "conclusion", "run_id", "run_attempt", "created_at", "updated_at",
    "sha256",
}
''',
        '''CONTEXT_HOSTED_GATE_ALLOWED_FIELDS = {
    "schema", "name", "path", "html_url", "repository", "branch",
    "head_branch", "head_sha", "workflow_path", "event", "status",
    "conclusion", "run_id", "run_attempt", "created_at", "updated_at",
    "sha256",
}
''',
    )
    replace_once(
        contract,
        '''    context_record = object_at(ctx["hosted_gates"].get(gate_name), f"$context.hosted_gates.{gate_name}")
    for field in (
''',
        '''    context_record = object_at(ctx["hosted_gates"].get(gate_name), f"$context.hosted_gates.{gate_name}")
    require(context_record.get("schema") == HOSTED_GATE_SCHEMA, f"{path}.context.schema is invalid")
    require(context_record.get("name") == gate_name, f"{path}.context.name is invalid")
    expected_context_path = f"hosted-gates/{gate_name}.json"
    require(context_record.get("path") == expected_context_path, f"{path}.context.path is invalid")
    expected_html_url = f"https://github.com/{source['repository']}/actions/runs/{run_id}"
    require(
        context_record.get("html_url") == expected_html_url
        and record.get("html_url") == expected_html_url,
        f"{path}.html_url is not bound to the selected run",
    )
    context_files = object_at(ctx.get("files"), "$context.files")
    require(
        context_record.get("sha256") == context_files.get(expected_context_path),
        f"{path}.context.sha256 differs from the payload index",
    )
    for field in (
''',
    )
    write(
        ROOT / "scripts/check-release-context-schema.py",
        '''#!/usr/bin/env python3
"""Guard the exact hosted-gate context producer/validator field contract."""
from __future__ import annotations
import runpy
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    "schema", "name", "workflow_path", "repository", "branch", "head_sha",
    "head_branch", "run_id", "run_attempt", "event", "status", "conclusion",
    "created_at", "updated_at", "html_url", "path", "sha256",
}
namespace = runpy.run_path(str(ROOT / "scripts/check-release-evidence-contract.py"))
actual = set(namespace["CONTEXT_HOSTED_GATE_ALLOWED_FIELDS"])
if actual != EXPECTED:
    print(f"hosted-gate context drift: missing={sorted(EXPECTED-actual)} extra={sorted(actual-EXPECTED)}", file=sys.stderr)
    raise SystemExit(1)
print("release context hosted-gate field contract: ok")
''',
        True,
    )
    release = ROOT / ".github/workflows/p0-release-candidate-gate.yml"
    replace_once(
        release,
        "          python3 scripts/check-strict-release-evidence-wiring.py\n",
        "          python3 scripts/check-strict-release-evidence-wiring.py\n"
        "          python3 scripts/check-release-context-schema.py\n",
    )


def patch_supply_chain() -> None:
    replace_once(
        ROOT / "Cargo.toml",
        'sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono"] }',
        'sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "macros", "migrate", "json"] }',
    )
    replace_once(
        ROOT / "deny.toml",
        '[advisories]\nyanked = "deny"\n',
        '[advisories]\nyanked = "deny"\nignore = ["RUSTSEC-2026-0214"]\n',
    )
    write(
        ROOT / "scripts/check-cargo-advisory-policy.py",
        '''#!/usr/bin/env python3
"""Enforce the bounded CEX Cargo advisory exception and runtime reachability."""
from __future__ import annotations
import datetime as dt
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
EXCEPTION = "RUSTSEC-2026-0214"
EXPIRES = dt.date(2026, 10, 1)
PACKAGE = re.compile(r"^(?P<name>[A-Za-z0-9_.+-]+) v[0-9]")

def tree(*args: str) -> str:
    result = subprocess.run(["cargo", "tree", "--workspace", "--locked", *args], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode != 0:
        raise SystemExit("cargo tree failed: " + result.stdout.strip())
    return result.stdout

def packages(text: str) -> set[str]:
    result: set[str] = set()
    for line in text.splitlines():
        match = PACKAGE.match(line.strip())
        if match:
            result.add(match.group("name"))
    return result

def main() -> int:
    problems: list[str] = []
    if dt.datetime.now(dt.timezone.utc).date() > EXPIRES:
        problems.append(f"{EXCEPTION} expired on {EXPIRES.isoformat()}")
    root = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    sqlx = root.get("workspace", {}).get("dependencies", {}).get("sqlx")
    features = set(sqlx.get("features", [])) if isinstance(sqlx, dict) else set()
    if not isinstance(sqlx, dict) or sqlx.get("default-features") is not False:
        problems.append("workspace sqlx must disable default features")
    elif "postgres" not in features or features & {"mysql", "sqlite", "any"}:
        problems.append("workspace sqlx must be PostgreSQL-only")
    deny = tomllib.loads((ROOT / "deny.toml").read_text(encoding="utf-8"))
    if set(deny.get("advisories", {}).get("ignore", [])) != {EXCEPTION}:
        problems.append(f"advisory ignore set must equal {{{EXCEPTION}}}")
    verifier = tomllib.loads((ROOT / "vendor/trnm-finality-verifier/Cargo.toml").read_text(encoding="utf-8"))
    if verifier.get("dev-dependencies", {}).get("tendermint-testgen") != "0.40.4":
        problems.append("reviewed tendermint-testgen dev dependency changed")
    runtime = packages(tree("-e", "normal,build", "--prefix", "none"))
    all_edges = packages(tree("-e", "all", "--prefix", "none"))
    for name in ("rsa", "paste"):
        if name in all_edges:
            problems.append(f"{name} remains in the resolved graph")
    if "gumdrop" in runtime:
        problems.append("gumdrop is reachable from normal/build edges")
    if "gumdrop" not in all_edges:
        problems.append("gumdrop exception is stale")
    else:
        inverse = tree("-e", "all", "-i", "gumdrop")
        for marker in ("gumdrop v0.8.1", "tendermint-testgen v0.40.4", "trnm-finality-verifier v0.1.0"):
            if marker not in inverse:
                problems.append("unreviewed gumdrop path: missing " + marker)
    print(json.dumps({"schema":"cex.cargo-advisory-policy.v1","status":"failed" if problems else "ok","exception":EXCEPTION,"expires":EXPIRES.isoformat(),"problems":problems}, indent=2, sort_keys=True))
    return 1 if problems else 0

if __name__ == "__main__":
    sys.exit(main())
''',
        True,
    )
    write(
        ROOT / "docs/security/RUSTSEC-2026-0214.md",
        '''# Bounded advisory decision: RUSTSEC-2026-0214

**Status:** active, test-only, fail-closed bounded exception  
**Owner:** CEX release engineering  
**Expiry:** 2026-10-01 UTC  
**Reviewed path:** `gumdrop -> tendermint-testgen 0.40.4 -> trnm-finality-verifier [dev-dependencies]`

`gumdrop` is unmaintained and is not accepted on a normal or build edge. The executable policy
`scripts/check-cargo-advisory-policy.py` proves that the resolved path is confined to the reviewed
Tendermint fixture generator used by the vendored finality-verifier test suite. The exception does
not cover a vulnerability and does not cover `rsa` or `paste`; both must be absent from the complete
resolved graph after PostgreSQL-only SQLx feature pruning. The gate fails after the expiry, when the
path changes, or when the dependency disappears without removal of this exception. The required
exit is committed fixtures, a maintained generator, or an internally maintained replacement.
''',
    )
    workflow = ROOT / ".github/workflows/trnm-economy-ci.yml"
    replace_count(
        workflow,
        "      - 'deny.toml'\n",
        "      - 'deny.toml'\n      - 'scripts/check-cargo-advisory-policy.py'\n      - 'docs/security/RUSTSEC-2026-0214.md'\n      - 'vendor/trnm-finality-verifier/**'\n",
        2,
    )
    replace_once(
        workflow,
        "      - name: Cargo advisory audit\n        run: cargo audit --deny warnings\n",
        "      - name: Cargo advisory graph policy\n        run: python3 scripts/check-cargo-advisory-policy.py\n\n"
        "      - name: Cargo advisory audit\n        run: cargo audit --deny warnings --ignore RUSTSEC-2026-0214\n",
    )


def module_doc(name: str, path: str, kind: str, purpose: str, invariant: str, verification: str, operations: str) -> str:
    return f'''# `{name}` technical development contract

**Source path:** `{path}`  
**Component class:** {kind}  
**Status:** active repository module; production authorization is governed by the v12 authority.

## Purpose and ownership

This module owns {purpose}. It owns no neighboring authority by implication. Cross-module changes
must preserve the active architecture, dependency direction, and responsibility boundary. Moving
financial, identity, audit, finality, credential, or operator authority requires an approved design
and traceability update rather than a convenience call or shared database write.

## Boundary and interfaces

Public Rust types, HTTP routes, events, signatures, database contracts, configuration, and stable
error classes are compatibility surfaces. Callers provide only authenticated and validated data.
Changes to a serialized field, identifier, state transition, authorization decision, timeout
classification, or side effect require compatibility analysis, positive and negative tests, and a
versioned migration when old and new forms cannot coexist safely.

## State and persistence

PostgreSQL-backed state is authoritative only through the owning repository and application
boundary. In-memory implementations are tests or explicitly non-authoritative projections and may
not become a production fallback. Restore and replay preserve identifiers, idempotency keys, exact
amounts, terminal evidence, ownership, ordering, and append-only constraints. Partial mutation or
unbound recovery evidence fails closed.

## Critical invariants

The module specifically preserves {invariant}. Every mutation is authenticated or explicitly
internal, replay-safe where transport can retry, auditable, bounded, and restart-safe. Money uses
integer minor units. Timeout and response loss are unknown outcomes until durable evidence proves
success or definite non-execution. User input never selects unrestricted paths, commands, labels,
credentials, or authority records.

## Verification contract

Required evidence includes {verification}. The minimum merge gate also includes formatting,
compilation, module tests, workspace tests, and every applicable static, PostgreSQL, protocol,
security, and recovery gate. Each regression repair adds a deterministic test. PostgreSQL-required
hosted lanes fail closed rather than silently skipping. Local success never substitutes for an
exact-branch, exact-SHA hosted result.

## Operations and security

Operational ownership requires that {operations}. Secrets are never committed or logged. Readiness
means safe acceptance of work, not only a live process. Metrics avoid unbounded user-controlled
labels. Operator actions affecting value, credentials, evidence, finality, deployment, or recovery
are attributed and durably audited. Capacity, retry, lease, reconciliation, and retention bounds are
explicit and alertable.

## Change-control checklist

- Update this contract when ownership, interfaces, persistence, invariants, deployment, or recovery changes.
- Link implementation and verification from an active requirement or approved architecture decision.
- Preserve compatibility or publish a versioned migration, staged rollout, rollback, and evidence plan.
- Run module, workspace, PostgreSQL, protocol, supply-chain, and candidate-evidence gates as applicable.
- Never change an external production-gate decision without independent evidence and authorized approval.
'''


def create_docs() -> None:
    docs = ROOT / "docs/modules"
    docs.mkdir(parents=True, exist_ok=True)
    for item in MODULES:
        write(docs / f"{item[0]}.md", module_doc(*item))
    rows = "\n".join(f"| `{n}` | `{p}` | [{n}.md](./{n}.md) | {k} |" for n,p,k,*_ in MODULES)
    write(docs / "README.md", f'''# CEX workspace module documentation index

Each of the 18 members explicitly declared by the root Cargo workspace has a normative technical
development contract covering ownership, interfaces, persistence, invariants, verification,
operations, security, and change control.

| Module | Source | Contract | Class |
|---|---|---|---|
{rows}

`scripts/check-workspace-module-docs.py` derives the expected set from `Cargo.toml`; membership and
documentation cannot drift silently.
''')
    write(
        ROOT / "scripts/check-workspace-module-docs.py",
        '''#!/usr/bin/env python3
"""Validate one substantive technical contract per declared workspace member."""
from __future__ import annotations
import json
import sys
import tomllib
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs/modules"
HEADINGS = {"## Purpose and ownership","## Boundary and interfaces","## State and persistence","## Critical invariants","## Verification contract","## Operations and security","## Change-control checklist"}

def main() -> int:
    problems: list[str] = []
    members = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8")).get("workspace", {}).get("members")
    if not isinstance(members, list) or not members:
        problems.append("workspace member list is invalid")
        members = []
    expected: dict[str,str] = {}
    for member in members:
        if not isinstance(member, str) or not member:
            problems.append(f"invalid member {member!r}"); continue
        name = Path(member).name
        if name in expected:
            problems.append(f"duplicate module basename {name}"); continue
        expected[name] = member
        doc = DOCS / f"{name}.md"
        if not doc.is_file():
            problems.append(f"missing {doc.relative_to(ROOT)}"); continue
        text = doc.read_text(encoding="utf-8")
        if f"**Source path:** `{member}`" not in text:
            problems.append(f"{name} source binding differs")
        missing = sorted(HEADINGS - {line.strip() for line in text.splitlines()})
        if missing:
            problems.append(f"{name} missing headings {missing}")
        if any(marker in text for marker in ("TODO", "TBD", "FIXME")):
            problems.append(f"{name} contains an incompleteness marker")
        if len(text.split()) < 300:
            problems.append(f"{name} contract is too shallow")
    actual = {p.stem for p in DOCS.glob("*.md") if p.name != "README.md"}
    names = set(expected)
    if actual != names:
        problems.append(f"document set mismatch missing={sorted(names-actual)} extra={sorted(actual-names)}")
    print(__import__("json").dumps({"schema":"cex.workspace-module-documentation.v1","status":"failed" if problems else "ok","workspace_members":len(members),"documented_members":len(actual & names),"problems":problems}, indent=2, sort_keys=True))
    return 1 if problems else 0

if __name__ == "__main__":
    sys.exit(main())
''',
        True,
    )
    check = ROOT / "scripts/check-development-docs.py"
    text = check.read_text(encoding="utf-8")
    marker = "    lint_check = subprocess.run(\n"
    block = '''    module_docs_check = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-workspace-module-docs.py")],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if module_docs_check.returncode != 0:
        PROBLEMS.append(
            "workspace module documentation contract failed: "
            + module_docs_check.stdout.strip()
        )

'''
    if text.count(marker) != 1:
        raise SystemExit("development documentation insertion anchor drifted")
    check.write_text(text.replace(marker, block + marker, 1), encoding="utf-8", newline="\n")
    append_once(ROOT / "docs/index.md", "## Workspace module technical contracts", '''

## Workspace module technical contracts

Every explicitly declared Cargo workspace member is covered by `docs/modules/README.md` and
machine-checked by `scripts/check-workspace-module-docs.py`. The v12 authority remains controlling
when a conflict exists.
''')


def create_entrypoints_and_plan_record() -> None:
    readme = ROOT / "readme.md"
    if readme.exists() and readme.read_text(encoding="utf-8").strip():
        raise SystemExit("root readme is no longer blank")
    write(readme, '''# CEX

CEX is the Trillionnium Foundation exact-money control-plane and consumer-entry Rust workspace. It
contains shared contracts and the Identity, Capability, Audit, Ledger, Gateway, Execution, TRNM,
Hepta, Paper Raid, consumer-entry, and Matrix components.

## Status and authority

A green tree may be a repository candidate; it is not an authorized production release.
`docs/development-doc-authority-v1.json` is the status authority. Production authorization remains
`not_granted` until independent operational, security, financial-control, provider, legal,
commercial, and human go/no-go evidence is approved against one immutable candidate.

## Development entry points

- `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- `docs/development-doc-authority-v1.json`
- `docs/modules/README.md`
- `docs/traceability/v12-requirements-v1.json`
- `docs/operations/external-production-evidence-closure.md`

## Baseline verification

```bash
python3 scripts/check-development-docs.py
python3 scripts/check-p0-release-candidate-hygiene.py
python3 scripts/check-release-context-schema.py
python3 scripts/check-cargo-advisory-policy.py
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Local success does not replace exact-branch and exact-SHA hosted evidence. Never commit credentials
or expose an exploit, private data, or unredacted production evidence in a public issue.
''')
    write(ROOT / "docs/operations/external-production-evidence-closure.md", '''# External production evidence closure runbook

Repository CI cannot self-certify production readiness. Authorized operators and independent
reviewers must bind representative backup/restore and crash recovery, deployment/cutover/failover
and rollback, Vault or HSM credential lifecycle, production-scale load and endurance, monitoring
and error budgets, reconciliation and Audit delivery, independent security/operations/financial
control/provider/legal/commercial approvals, and final human go/no-go to one immutable candidate
SHA, tree, evidence payload, and manifest.

Each record identifies the environment, operators, reviewers, timestamps, tooling version, exact
commands or runbook revision, immutable digests, result, exception, remediation, and revocation
route. Until every external record passes, `production_authorization` remains `not_granted`.
''')
    append_once(ROOT / "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md", "## Sequence 44 residual repository closure", '''

## Sequence 44 residual repository closure

Sequence 44 closes the exact sequence-43 repository failures: hosted-gate context fields are
semantically shared by producer and validator; SQLx is PostgreSQL-only while retaining the required
macros, migration, and JSON surfaces; unused RSA and Paste paths are removed; the remaining Gumdrop
test-only warning is runtime-forbidden, path-proven, owner-bound, and expiring; every declared
workspace member has a machine-enforced technical contract; and the root developer entry point and
external evidence runbook are complete. Repository closure still requires every authoritative
workflow, the TRNM supply-chain workflow, and the aggregate candidate manifest to pass on one
immutable SHA. External production gates remain non-self-certifiable and production authorization
remains `not_granted`.
''')


def squash_commit() -> None:
    for relative in BOOTSTRAP:
        path = ROOT / relative
        if path.exists():
            path.unlink()
    run("git", "reset", "--soft", BASE_SHA)
    run("git", "add", "-A")
    run("git", "config", "user.name", "CEX Sequence 44 Automation")
    run("git", "config", "user.email", "actions@users.noreply.github.com")
    run("git", "commit", "-m", "fix: close sequence-44 residual repository blockers")


def qualify() -> None:
    sha = out("git", "rev-parse", "HEAD")
    tree = out("git", "rev-parse", "HEAD^{tree}")
    commands = [
        ("python3", "scripts/check-release-context-schema.py"),
        ("python3", "scripts/check-workspace-module-docs.py"),
        ("python3", "scripts/check-development-docs.py"),
        ("python3", "scripts/check-p0-release-candidate-hygiene.py", "--output", "/tmp/candidate-hygiene.json"),
        ("python3", "scripts/check-repository-integrity.py", "--expected-sha", sha, "--expected-tree", tree, "--output", "/tmp/repository-integrity.json"),
        ("python3", "scripts/check-cargo-advisory-policy.py"),
        ("cargo", "fmt", "--all", "--", "--check"),
        ("cargo", "test", "--workspace", "--all-targets", "--locked"),
        ("cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"),
        ("cargo", "audit", "--deny", "warnings", "--ignore", "RUSTSEC-2026-0214"),
        ("cargo", "deny", "check", "advisories", "bans", "licenses", "sources"),
    ]
    for command in commands:
        print("+", " ".join(command), flush=True)
        run(*command)


def main() -> int:
    verify_bootstrap()
    patch_release_contract()
    patch_supply_chain()
    create_docs()
    create_entrypoints_and_plan_record()
    print("+ cargo update -p sqlx --precise 0.8.6", flush=True)
    run("cargo", "update", "-p", "sqlx", "--precise", "0.8.6")
    squash_commit()
    qualify()
    print("+ publish repository-only closure tree", flush=True)
    run("git", "push", "--force-with-lease", "origin", f"HEAD:{BRANCH}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
