#!/usr/bin/env python3
"""Fast fail-closed static wiring checks for the active CEX P0 v12 candidate."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []
ACTIVE_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
ACTIVE_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
DOC_CHECKER = "scripts/check-development-docs.py"
SHARED_TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
AUTHORITATIVE_WORKFLOWS = (
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
)
RELEASE_WORKFLOW = ".github/workflows/p0-release-candidate-gate.yml"
AUTHORITATIVE_POSTGRES_SCRIPTS = (
    "scripts/check-p0-migrations-postgres.sh",
    "scripts/check-audit-source-baseline-postgres.sh",
    "scripts/check-ledger-operation-identity-postgres.sh",
    "scripts/check-invocation-ledger-contract-postgres.sh",
    "scripts/check-invocation-ledger-terminal-postgres.sh",
    "scripts/check-gateway-exact-reserve-postgres.sh",
    "scripts/check-execution-settlement-commands-postgres.sh",
    "scripts/check-provider-reconciliation-postgres.sh",
    "scripts/check-p0-exact-ledger-soak-postgres.sh",
    "scripts/check-trnm-settlement-receipt-lookup-http.sh",
    "scripts/backfill-audit-source-baselines.sh",
)


def read_text(relative_path: str) -> str:
    path = ROOT / relative_path
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative_path}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative_path}: {error}")
        return ""


def require_text(relative_path: str, *needles: str) -> None:
    content = read_text(relative_path)
    for needle in needles:
        if needle not in content:
            PROBLEMS.append(f"{relative_path} lacks required marker: {needle}")


def forbid_text(relative_path: str, *needles: str) -> None:
    content = read_text(relative_path)
    for needle in needles:
        if needle in content:
            PROBLEMS.append(f"{relative_path} contains forbidden marker: {needle}")


def forbid_regex(relative_path: str, *patterns: tuple[str, str]) -> None:
    content = read_text(relative_path)
    for pattern, label in patterns:
        if re.search(pattern, content, flags=re.MULTILINE):
            PROBLEMS.append(f"{relative_path} contains forbidden pattern: {label}")


def forbid_path(relative_path: str) -> None:
    if (ROOT / relative_path).exists():
        PROBLEMS.append(f"obsolete/conflicting path must not exist: {relative_path}")


def latest_migration() -> tuple[str, str]:
    migrations = sorted(
        path
        for path in (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql")
        if path.is_file()
    )
    if not migrations:
        PROBLEMS.append("no numbered SQL migrations found")
        return "", ""
    numbers: dict[str, list[str]] = {}
    for path in migrations:
        match = re.match(r"^(?P<number>\d{4})_", path.name)
        if match is None:
            PROBLEMS.append(f"invalid numbered migration filename: {path.name}")
            continue
        numbers.setdefault(match.group("number"), []).append(path.name)
    for number, names in sorted(numbers.items()):
        if len(names) > 1:
            PROBLEMS.append(
                f"duplicate migration number {number}: {', '.join(sorted(names))}"
            )
    latest = migrations[-1]
    match = re.match(r"^(?P<number>\d{4})_", latest.name)
    return (match.group("number") if match else "", latest.name)


def verify_release_template(expected_filename: str) -> None:
    raw = read_text("docs/templates/cex-release-baseline-manifest-v1.json")
    if not raw:
        return
    try:
        document = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid release manifest template JSON: {error}")
        return
    recorded = document.get("database", {}).get("migration_head")
    if recorded != expected_filename:
        PROBLEMS.append(
            f"release manifest database.migration_head={recorded!r}, expected {expected_filename!r}"
        )


def verify_candidate_trigger() -> None:
    raw = read_text(SHARED_TRIGGER)
    if not raw:
        return
    try:
        trigger = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid P0 candidate trigger JSON: {error}")
        return
    if trigger.get("schema") != "cex.p0-candidate-trigger.v1":
        PROBLEMS.append("candidate trigger schema is not cex.p0-candidate-trigger.v1")
    if trigger.get("plan") != Path(ACTIVE_PLAN).name:
        PROBLEMS.append("candidate trigger is not bound to the active v12 plan")
    if not isinstance(trigger.get("sequence"), int) or trigger["sequence"] < 1:
        PROBLEMS.append("candidate trigger sequence must be a positive integer")
    if trigger.get("production_authorization") != "not_granted":
        PROBLEMS.append("candidate trigger must explicitly deny production authorization")


def verify_core() -> None:
    for relative_path in (
        "services/gateway-service/src/main.rs",
        "services/identity-service/src/main.rs",
        "services/ledger-service/src/main.rs",
        "services/execution-service/src/main.rs",
        "services/audit-service/src/main.rs",
    ):
        require_text(relative_path, "runtime_guard::enforce")

    for obsolete in (
        "migrations/0060_add_audit_outbox_delivery_schema.sql",
        "migrations/0061_add_execution_transactional_audit_outbox.sql",
        "migrations/0062_add_identity_transactional_audit_outbox.sql",
    ):
        forbid_path(obsolete)

    require_text(
        "migrations/0066_add_invocation_ledger_contract.sql",
        "cex_invocation_ledger_contracts_v1",
        "cex_register_invocation_ledger_contract_v1",
        "cex_invocation_ledger_effect_request_v1",
        "cex_bind_invocation_ledger_effect_v1",
        "missing_effect_evidence",
    )

    require_text(
        "services/gateway-service/src/infrastructure/state.rs",
        '"trnm-economy"',
        '"trnm_economy"',
        "legacy reserve remains fail-closed",
    )

    trnm_launcher = "scripts/run-trnm-economy-service.sh"
    require_text(
        trnm_launcher,
        "TRNM_SECRET_ENV_NAMES=(",
        "require_distinct_secrets",
        "TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH is required",
        "must be an absolute mounted path",
        "LEDGER_ADMIN_TOKEN",
        "TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET",
        "TRNM_GAME_AUTHORITY_TOKEN",
        "TRNM_PLAYER_SESSION_SIGNING_SECRET",
        "CONSUMER_ENTRY_INGRESS_TOKEN",
        "CONSUMER_ENTRY_SESSION_AUTH_SECRET",
        "CEX_GATEWAY_API_KEY",
        "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET",
    )
    forbid_text(
        trnm_launcher,
        "trnm-economy-local-production-key",
        ":-$IDENTITY_ADMIN_TOKEN",
        "trnm-entitlement-signing-v1:$IDENTITY_ADMIN_TOKEN",
        "../trillionnium-world/run/online-authority/issuer-registry.json",
    )

    require_text(
        "deploy/systemd/cex-trnm-economy-maintenance.service",
        "WorkingDirectory=%h/.openclaw/workspace/CEX",
        "ExecStart=%h/.openclaw/workspace/CEX/scripts/run-trnm-economy-maintenance.sh",
    )
    forbid_text(
        "deploy/systemd/cex-trnm-economy-maintenance.service",
        "/home/alex/",
    )


def verify_exact_contracts() -> None:
    require_text(
        "scripts/_dev-helpers.sh",
        "cex_sync_postgres_env_from_database_url",
        "CEX_POSTGRES_PASSWORD",
        "dangerous_query_keys",
        "cex_docker_exec_with_password",
        "cex_docker_exec_with_password_stdin",
        "cex_docker_run_with_password",
        "cex_docker_run_with_password_stdin",
        "cex_postgres_docker_socket_is_target",
        "--env-file",
        # Passwords are now carried through the generic decoded-password
        # variable or a Docker env-file; never require a literal secret-bearing
        # command argument in this static contract.
        'PGPASSWORD="$psql_password"',
        'PGPASSWORD="$readiness_password"',
    )
    # The normalized runtime probe must honor an explicitly supplied
    # DATABASE_URL even though it loads the repository's .env for the rest of
    # its local defaults.  Keep this contract visible to the static gate so a
    # future refactor cannot silently redirect a CI/operator run to cex_ai.
    require_text(
        "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
        "NORMALIZED_RUNTIME_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "NORMALIZED_RUNTIME_CALLER_DATABASE_URL_SET=0",
        "cex_load_env",
        'if [[ \"$NORMALIZED_RUNTIME_CALLER_DATABASE_URL_SET\" == \"1\" ]]; then',
        'export DATABASE_URL=\"$NORMALIZED_RUNTIME_CALLER_DATABASE_URL\"',
        "cex_sync_postgres_env_from_database_url",
        "cex_database_url_for_database",
    )
    # The native TRNM evidence scripts also load the repository `.env`, so
    # their explicit database target must survive that convenience import.
    # Keep the preservation contract machine-checked alongside the normalized
    # runtime probe; otherwise a local `.env` can silently redirect a gate.
    require_text(
        "scripts/check-trnm-economy-disaster-recovery.sh",
        "TRNM_DR_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "TRNM_DR_CALLER_DATABASE_URL_SET=0",
        "cex_load_env",
        'if [[ \"$TRNM_DR_CALLER_DATABASE_URL_SET\" == \"1\" ]]; then',
        'export DATABASE_URL=\"$TRNM_DR_CALLER_DATABASE_URL\"',
        "cex_sync_postgres_env_from_database_url",
    )
    require_text(
        "scripts/check-trnm-native-economy-cross-process.sh",
        "TRNM_CROSS_PROCESS_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "TRNM_CROSS_PROCESS_CALLER_DATABASE_URL_SET=0",
        "cex_load_env",
        'if [[ \"$TRNM_CROSS_PROCESS_CALLER_DATABASE_URL_SET\" == \"1\" ]]; then',
        'export DATABASE_URL=\"$TRNM_CROSS_PROCESS_CALLER_DATABASE_URL\"',
        "cex_sync_postgres_env_from_database_url",
    )
    require_text(
        "scripts/check-trillionnium-league-sql-snapshot-db.sh",
        "SNAPSHOT_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "SNAPSHOT_CALLER_DATABASE_URL_SET=0",
        'export DATABASE_URL=\"$SNAPSHOT_CALLER_DATABASE_URL\"',
        "cex_sync_postgres_env_from_database_url",
        "cex_database_url_for_database",
    )
    # The partial-upgrade fixture is also invoked with hosted/CI URLs.  It
    # must preserve that URL while importing container defaults, retain URI
    # query options, and honor embedded credentials in the Docker fallback.
    require_text(
        "scripts/check-term-exchange-receipt-partial-upgrade-postgres.sh",
        "RECEIPT_CALLER_DATABASE_URL=\"${DATABASE_URL-}\"",
        "RECEIPT_CALLER_DATABASE_URL_SET=0",
        'export DATABASE_URL=\"$RECEIPT_CALLER_DATABASE_URL\"',
        "RECEIPT_DB_URI_SCHEME",
        "RECEIPT_DB_URI_SAFE_NETLOC",
        "RECEIPT_DB_URI_QUERY",
        "parse_qsl",
        "dangerous_query_keys",
        "RECEIPT_DOCKER_PSQL_PASSWORD",
        "docker_psql_stdin",
        "cex_docker_exec_with_password_stdin",
        "cex_postgres_docker_socket_is_target",
    )
    require_text(
        "scripts/check-p0-backup-restore-postgres.sh",
        "cex_database_url_for_database",
        "run_with_postgres_password",
        "trap cleanup EXIT",
    )
    require_text(
        "crates/shared-types/src/ledger_v2.rs",
        "LedgerEffectRequestV1",
        "LedgerOperationKind",
        "i64_string",
        "ExplicitTraceRequired",
    )
    require_text(
        "services/ledger-service/src/ledger_effects.rs",
        "shared_types::ledger_v2",
        "ledger_currency_mismatch",
        "cex_apply_ledger_effect_v1",
    )
    require_text(
        "services/gateway-service/src/infrastructure/ledger_v2_client.rs",
        "CEX_GATEWAY_LEDGER_MODE",
        "apply_ledger_effect_v2",
        "MoneyAmount",
    )
    require_text(
        "services/execution-service/src/ledger_settlement.rs",
        "ExecutionLedgerMode",
        "cex_invocation_ledger_effect_request_v1",
        "RetryableExactReplay",
        "ReconcileRequired",
        "validate_success_receipt",
    )

    for script in (
        "scripts/check-ledger-caller-cutover.py",
        "scripts/check-invocation-ledger-contract-static.py",
        "scripts/check-execution-ledger-settlement.py",
        "scripts/check-consumer-exact-money.py",
    ):
        try:
            result = subprocess.run(
                [sys.executable, str(ROOT / script)],
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        except OSError as error:
            PROBLEMS.append(f"cannot execute {script}: {error}")
        else:
            if result.returncode != 0:
                PROBLEMS.append(f"{script} failed: {result.stdout.strip()}")

    runtime_profile_test = ROOT / "scripts/test-runtime-profile-wiring.sh"
    try:
        result = subprocess.run(
            ["bash", str(runtime_profile_test)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
    except OSError as error:
        PROBLEMS.append(f"cannot execute scripts/test-runtime-profile-wiring.sh: {error}")
    else:
        if result.returncode != 0:
            PROBLEMS.append(
                "scripts/test-runtime-profile-wiring.sh failed: "
                + result.stdout.strip()
            )


def verify_database_url_isolation_contract() -> None:
    """Exercise URL rewriting/loader precedence without touching PostgreSQL."""

    probe = r'''
set -euo pipefail
source scripts/_dev-helpers.sh
safe="$(cex_database_url_for_database tmp_probe \
  'postgres://foo__CEX_DATABASE__bar:pw@127.0.0.1:55432/base?sslmode=disable&connect_timeout=5&application_name=sentinel__CEX_DATABASE__')"
[[ "$safe" == 'postgres://foo__CEX_DATABASE__bar:pw@127.0.0.1:55432/tmp_probe?sslmode=disable&connect_timeout=5&application_name=sentinel__CEX_DATABASE__' ]]
if cex_database_url_for_database tmp_probe \
  'postgres://u:p@127.0.0.1:55432/base?dbname=production' >/dev/null 2>&1; then
  exit 11
fi
if cex_database_url_without_password \
  'postgres://@127.0.0.1:55432/base' >/dev/null 2>&1; then
  exit 13
fi
if cex_database_url_for_database tmp_probe \
  'postgres://127.0.0.1:55432/base' >/dev/null 2>&1; then
  exit 14
fi
if cex_sync_postgres_env_from_database_url \
  'postgres://u:p@127.0.0.0:55432/base?host=production' >/dev/null 2>&1; then
  exit 12
fi
export CEX_POSTGRES_CONTAINER_NAME=caller-container
export CEX_POSTGRES_CONTAINER_NAME_PRESET=1
export DATABASE_URL='postgres://caller:pw@127.0.0.1:55432/caller_db'
export LEDGER_BASE_URL='http://caller.example.test'
cex_load_env
[[ "$CEX_POSTGRES_CONTAINER_NAME" == caller-container ]]
[[ "$DATABASE_URL" == 'postgres://caller:pw@127.0.0.1:55432/caller_db' ]]
[[ "$LEDGER_BASE_URL" == 'http://caller.example.test' ]]

# A repository `.env` may contain a stale PGPASSWORD.  It must not shadow a
# caller-selected URI password, while an explicitly exported caller password
# remains the conventional libpq override.
env_file="$(mktemp)"
trap 'rm -f -- "$env_file"' EXIT
printf '%s\n' 'PGPASSWORD=from-env' >"$env_file"
(
  # With no DATABASE_URL and no env file, the helper's built-in local
  # postgres:postgres endpoint still needs its password supplied separately
  # after the URI is stripped from argv.
  unset DATABASE_URL PGPASSWORD CEX_POSTGRES_PASSWORD CEX_POSTGRES_USER CEX_POSTGRES_DB
  export CEX_ENV_FILE=/dev/null
  source scripts/_dev-helpers.sh
  cex_load_env
  [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "0" ]]
  cex_postgres_password_is_set
  [[ "$(cex_postgres_password_value)" == postgres ]]
)
(
  # A synchronized passwordless URI must remain passwordless; the helper's
  # convenience postgres value is not allowed to leak into PGPASSWORD.
  unset PGPASSWORD CEX_POSTGRES_PASSWORD CEX_POSTGRES_USER CEX_POSTGRES_DB
  export CEX_ENV_FILE=/dev/null
  export DATABASE_URL='postgres://uri-user@127.0.0.1:55432/uri_db'
  source scripts/_dev-helpers.sh
  cex_load_env
  [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]]
  [[ "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "0" ]]
  [[ "${CEX_POSTGRES_PASSWORD}" == postgres ]]
  if cex_postgres_password_is_set; then
    exit 21
  fi
)
(
  # PostgreSQL defaults an omitted URI port to 5432.  Synchronization must
  # expose that effective port so Docker/socket and TCP target checks do not
  # reject a valid default-port URL as an unknown server.
  unset PGPASSWORD CEX_POSTGRES_PASSWORD CEX_POSTGRES_USER CEX_POSTGRES_DB
  export CEX_ENV_FILE=/dev/null
  export DATABASE_URL='postgres://uri-user:uri-password@127.0.0.1/uri_db'
  source scripts/_dev-helpers.sh
  cex_load_env
  [[ "$CEX_POSTGRES_HOST" == 127.0.0.1 ]]
  [[ "$CEX_POSTGRES_PORT" == 5432 ]]
)
(
  # An explicitly empty CEX_POSTGRES_PASSWORD is a caller-selected credential
  # and must not be replaced by the helper's convenience `postgres` default.
  # This matters for a passwordless URI where an implicit fallback would make
  # libpq authenticate with the wrong secret (or defeat peer/.pgpass auth).
  unset DATABASE_URL PGPASSWORD CEX_POSTGRES_PASSWORD CEX_POSTGRES_USER CEX_POSTGRES_DB
  unset CEX_DATABASE_URL_SYNCED CEX_DATABASE_URL_PASSWORD_PRESENT CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC
  export CEX_ENV_FILE=/dev/null
  export CEX_POSTGRES_PASSWORD=''
  export DATABASE_URL='postgres://uri-user@127.0.0.1/uri_db'
  source scripts/_dev-helpers.sh
  cex_load_env
  [[ "${CEX_POSTGRES_PASSWORD+x}" == x ]]
  [[ "$CEX_POSTGRES_PASSWORD" == "" ]]
  [[ "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == 1 ]]
  cex_postgres_password_is_set
  [[ "$(cex_postgres_password_value)" == "" ]]
)
(
  unset PGPASSWORD
  export CEX_ENV_FILE="$env_file"
  export DATABASE_URL='postgres://uri-user:uri-password@127.0.0.1:55432/uri_db'
  source scripts/_dev-helpers.sh
  cex_load_env
  cex_sync_postgres_env_from_database_url "$DATABASE_URL"
  [[ ! ${PGPASSWORD+x} ]]
  [[ "$CEX_POSTGRES_PASSWORD" == uri-password ]]
)
(
  export PGPASSWORD=caller-password
  export CEX_ENV_FILE="$env_file"
  export DATABASE_URL='postgres://uri-user:uri-password@127.0.0.1:55432/uri_db'
  source scripts/_dev-helpers.sh
  cex_load_env
  cex_sync_postgres_env_from_database_url "$DATABASE_URL"
  [[ "$PGPASSWORD" == caller-password ]]
)
'''
    try:
        result = subprocess.run(
            ["bash", "-c", probe],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
    except OSError as error:
        PROBLEMS.append(f"database URL isolation probe could not run: {error}")
    else:
        if result.returncode != 0:
            PROBLEMS.append(
                "database URL isolation probe failed: " + result.stdout.strip()
            )


def verify_postgres_argv_contract() -> None:
    """Keep hosted P0 PostgreSQL probes from exposing URI credentials in argv."""

    for script in AUTHORITATIVE_POSTGRES_SCRIPTS:
        require_text(
            script,
            "source \"$root/scripts/_dev-helpers.sh\"",
            "cex_load_env",
            "cex_sync_postgres_env_from_database_url",
            "cex_psql_stdin",
        )
        # A URI may contain credentials, so it must never be handed directly
        # to psql or to Docker's `-e PGPASSWORD=...` argv.  cex_psql_stdin
        # performs credential stripping and carries the password separately.
        forbid_regex(
            script,
            (r"\bpsql\s+['\"]?\$\{?DATABASE_URL", 'psql "$DATABASE_URL"'),
            (r"\b(?:docker|cex_docker)\b[^\n]*-e\s+PGPASSWORD=", "Docker PGPASSWORD argv"),
        )

    # The backup drill uses pg_dump/pg_restore as well as psql; all connection
    # URLs must be derived from the password-free BASE_URL_SAFE value.
    require_text(
        "scripts/check-p0-backup-restore-postgres.sh",
        "BASE_URL_SAFE=",
        "run_with_postgres_password",
        "cex_database_url_without_password",
    )
    forbid_regex(
        "scripts/check-p0-backup-restore-postgres.sh",
        (r"\b(?:psql|pg_dump|pg_restore)\s+['\"]?\$\{?BASE_URL(?:\W|$)",
         "database client receives unsanitized BASE_URL"),
        (r"\b(?:psql|pg_dump|pg_restore)\s+['\"]?\$\{?DATABASE_URL",
         'database client receives DATABASE_URL'),
    )

    # The provider workflow has its own fresh-migration loop.  Keep it on the
    # same helper path so a future edit cannot reintroduce a credential-bearing
    # `psql "$DATABASE_URL"` invocation outside the shell-script inventory.
    require_text(
        ".github/workflows/p0-provider-reconciliation-gate.yml",
        "source scripts/_dev-helpers.sh",
        "cex_load_env",
        "cex_sync_postgres_env_from_database_url",
        "cex_psql_stdin -X -v ON_ERROR_STOP=1 -f - < \"$migration\"",
    )
    forbid_regex(
        ".github/workflows/p0-provider-reconciliation-gate.yml",
        (r"\bpsql\s+['\"]?\$\{?DATABASE_URL", 'workflow psql receives DATABASE_URL'),
        (r"\b(?:docker|cex_docker)\b[^\n]*-e\s+PGPASSWORD=", "workflow Docker PGPASSWORD argv"),
    )


def verify_release_evidence_self_test() -> None:
    """Run the in-process exact-SHA/rerun binding regression fixture."""

    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts/p0-release-evidence-core.py"), "self-test"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        PROBLEMS.append(
            "p0-release-evidence-core self-test failed: " + result.stdout.strip()
        )


def verify_development_documents() -> None:
    result = subprocess.run(
        [sys.executable, str(ROOT / DOC_CHECKER)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        PROBLEMS.append(f"development-document contract failed: {result.stdout.strip()}")


def verify_gates_and_plan() -> None:
    require_text(
        "scripts/check-repository-integrity.py",
        '"commit_sha": commit_sha',
        '"tree_sha": tree_sha',
    )
    require_text(
        "scripts/bind-p0-local-evidence.py",
        "producer_tree = payload.get(\"tree_sha\")",
        "missing a valid exact tree_sha",
    )
    require_text(
        "scripts/p0-release-evidence-core.py",
        "tree_sha = payload.get(\"tree_sha\")",
        "lacks a valid exact tree_sha",
        "def revalidate_gate_runs(",
        "release-evidence core self-test failed",
    )
    for producer in (
        "scripts/check-hepta-postgres-integration.sh",
        "scripts/check-p0-exact-ledger-soak-postgres.sh",
        "scripts/check-p0-backup-restore-postgres.sh",
    ):
        require_text(producer, '"tree_sha"')
    require_text(
        "scripts/observe-repository-governance.py",
        "--candidate-branch",
        "candidate_branch",
        "candidate_ruleset_count",
        "ruleset_applies_to_branch",
        "request_rulesets",
        "ruleset_bypass_state",
        "pagination exceeded 1000 pages",
    )
    require_text(
        "scripts/check-release-baseline-manifest.py",
        "CANONICAL_BRANCH_PATTERN",
        "HOSTED_EVIDENCE_URI_PATTERN",
        "PAYLOAD_ARTIFACT_NAME_PATTERN",
        "PAYLOAD_ARTIFACT_URI_PATTERN",
        "SBOM_URI_PATTERN",
        "PROVENANCE_URI_PATTERN",
        "LOCAL_EVIDENCE_URI_PATTERNS",
        "JSON Schema source.branch pattern",
        "JSON Schema candidate payload artifact URI pattern",
        "JSON Schema candidate evidence URI pattern",
    )
    require_text(
        "docs/schemas/cex-release-baseline-manifest-v1.schema.json",
        "cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*",
        "gh://TrillionniumFoundation/CEX/actions/runs/[1-9][0-9]*/attempts/[1-9][0-9]*",
    )
    require_text(
        ".github/workflows/rust-service-gate.yml",
        "scripts/check-p0-wiring.py",
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "scripts/check-hepta-postgres-integration.sh",
        "scripts/test-runtime-profile-wiring.sh",
        "repository-integrity:",
        "hepta-postgres-integration:",
        "cargo fmt --all --check",
        "cargo check --locked --workspace --all-targets",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-migration-gate.yml",
        "scripts/check-invocation-ledger-terminal-postgres.sh",
        "scripts/check-ledger-operation-identity-postgres.sh",
        "scripts/check-trnm-economy-settlement-contract.py",
        "scripts/test-trnm-economy-settlement-status-negative.py",
        "scripts/test-runtime-profile-wiring.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-gateway-exact-reserve-gate.yml",
        "scripts/check-gateway-exact-reserve-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-execution-settlement-gate.yml",
        "scripts/check-execution-settlement-commands-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ".github/workflows/p0-provider-reconciliation-gate.yml",
        "scripts/check-provider-reconciliation-postgres.sh",
        SHARED_TRIGGER,
    )
    require_text(
        RELEASE_WORKFLOW,
        "scripts/p0-release-evidence.py collect",
        "scripts/check-p0-exact-ledger-soak-postgres.sh",
        "scripts/check-p0-backup-restore-postgres.sh",
        "scripts/check-term-exchange-receipt-partial-upgrade-postgres.sh",
        "term-exchange-receipt-partial-upgrade-regression",
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "scripts/observe-repository-governance.py",
        "--candidate-branch",
        "CANDIDATE_BRANCH",
        "candidate branch/ruleset state",
        '"tree_sha": sys.argv[3]',
        "CANDIDATE_TREE",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
        "scripts/check-trnm-economy-settlement-contract.py",
        "scripts/test-trnm-economy-settlement-status-negative.py",
        "scripts/test-runtime-profile-wiring.sh",
        SHARED_TRIGGER,
    )
    require_text(
        ACTIVE_PLAN,
        "Status: active all-blocker closure candidate; **not production-ready**.",
        f"Candidate migration head: `{MIGRATION_HEAD}`.",
        "External gates that repository edits cannot self-certify",
        "Definition of repository closure",
    )
    require_text(
        ACTIVE_ADDENDUM,
        "Block H",
        "Block I",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production gates remain upstream blockers",
    )
    require_text(
        "services/hepta-research-league/tests/postgres_recovery.rs",
        "HEPTA_REQUIRE_POSTGRES_TESTS",
        "strict PostgreSQL integration test",
    )

    for workflow in (*AUTHORITATIVE_WORKFLOWS, RELEASE_WORKFLOW):
        require_text(workflow, SHARED_TRIGGER)


def main() -> int:
    migration_number, migration_filename = latest_migration()
    if migration_filename and migration_filename != MIGRATION_HEAD:
        PROBLEMS.append(
            f"active v12 migration head {MIGRATION_HEAD!r} != repository head {migration_filename!r}"
        )
    verify_release_template(migration_filename)
    verify_candidate_trigger()
    verify_core()
    verify_exact_contracts()
    verify_database_url_isolation_contract()
    verify_postgres_argv_contract()
    verify_release_evidence_self_test()
    verify_development_documents()
    verify_gates_and_plan()
    result = {
        "status": "failed" if PROBLEMS else "ok",
        "plan": Path(ACTIVE_PLAN).name,
        "addendum": Path(ACTIVE_ADDENDUM).name,
        "migration_number": migration_number,
        "migration_head": migration_filename,
        "checks": 7,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())
