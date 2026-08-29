#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration="$root/migrations/0003_invite_alpha_access.sql"
operator_audit_migration="$root/migrations/0006_accessctl_operator_audit.sql"
activation_migration="$root/migrations/0007_invite_activation_authority.sql"
operator_audit_upgrade_fixture="$root/migrations/fixtures/0006_accessctl_operator_audit_upgrade.sql"
operator_audit_tamper_fixture="$root/migrations/fixtures/0006_accessctl_operator_audit_tampered_guard.sql"
operator_audit_trigger_tamper_fixture="$root/migrations/fixtures/0006_accessctl_operator_audit_tampered_trigger.sql"
operator_audit_rollback_fixture="$root/migrations/fixtures/0006_accessctl_operator_audit_rollback_after_drop.sql"
operator_audit_rollback_verify_fixture="$root/migrations/fixtures/0006_accessctl_operator_audit_rollback_verify.sql"
activation_nullable_fixture="$root/migrations/fixtures/0007_invite_activation_nullable_and_json_null.sql"
activation_strict_tamper_fixture="$root/migrations/fixtures/0007_invite_activation_strict_tamper.sql"
postgres_gate="$root/scripts/check-postgres.sh"
access="$root/src/access.rs"
app="$root/src/app.rs"
config="$root/src/config.rs"
db="$root/src/db.rs"
atomic_activation_sql="$root/src/invite_activation_v3_atomic.sql"
ctl="$root/src/bin/paper-raid-accessctl.rs"
env_example="$root/deploy/alpha.env.example"
readme="$root/README.md"

for file in "$migration" "$operator_audit_migration" "$activation_migration" \
  "$operator_audit_upgrade_fixture" "$operator_audit_tamper_fixture" \
  "$operator_audit_trigger_tamper_fixture" \
  "$operator_audit_rollback_fixture" "$operator_audit_rollback_verify_fixture" \
  "$activation_nullable_fixture" "$activation_strict_tamper_fixture" \
  "$postgres_gate" \
  "$access" "$app" "$config" "$db" "$atomic_activation_sql" "$ctl" "$readme"; do
  test -s "$file"
done

for table in schema_capabilities accounts account_scopes account_author_roles invite_batches invites login_credentials access_audit quota_windows retention_runs; do
  grep -Fq "paper_raid_bff_${table}" "$migration"
done

grep -Fq 'None | Some("fixed_alpha") => Ok(IdentityMode::FixedAlpha)' "$config"
grep -Fq 'invite_alpha forbids PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON' "$config"
grep -Fq 'identity_for_subject(&subject_id)' "$app"
grep -Fq 'authenticate_or_redeem(&request.login_key)' "$app"
grep -Fq 'issue_at_generation(&identity, generation)' "$app"
grep -Fq 'session_generation_tx(&mut tx, &identity.subject_id)' "$access"
grep -Fq '"access_directory_reachable"' "$app"
grep -Fq '"access_directory_within_capacity"' "$app"
grep -Fq '"access_audit_append_only"' "$app"
grep -Fq '"access_topology_ready"' "$app"
grep -Fq 'author_topology_ready' "$access"
grep -Fq 'review_topology_ready' "$access"
grep -Fq 'PAPER_RAID_BFF_IDENTITY_MODE=fixed_alpha' "$env_example"
grep -Fq '# PAPER_RAID_BFF_LOGIN_QUOTA_BUCKET_LIMIT=' "$env_example"
grep -Fq 'enforce_mutation_quota' "$root/src/auth.rs"
grep -Fq 'StatusCode::TOO_MANY_REQUESTS' "$root/src/error.rs"
grep -Fq 'header::RETRY_AFTER' "$root/src/error.rs"
for boundary in \
  'paper_raid_bff_runtime_acl_state_v2()' \
  'sequence_privileges AS (' \
  "'MAINTAIN'" \
  'column_acl_entries AS (' \
  'default_acl_entries AS (' \
  'database_acl_entries AS (' \
  'schema_acl_entries AS (' \
  'parameter_acl_entries AS (' \
  'database_role_settings AS (' \
  'activation_function_acl_entries AS (' \
  'pg_parameter_acl parameter' \
  'pg_db_role_setting setting' \
  "to_regprocedure('pg_catalog.pg_control_system()')" \
  "privilege_name || ' WITH GRANT OPTION'" \
  "'database_connect_grantable'" \
  "'schema_create_grantable'" \
  'acl.is_grantable' \
  "approval_receipt_v1_sha256_value = 'sha256:' || encode(" \
  "trnm.paper-raid.invite-alpha-activation-receipt.v1" \
  'revocation_reason_value IS NOT NULL' \
  'CALLED ON NULL INPUT' \
  'CONSTRAINT paper_raid_bff_invite_activation_parity_ck CHECK (' \
  'DROP CONSTRAINT IF EXISTS paper_raid_bff_invite_activation_parity_ck' \
  'ON paper_raid_bff_invite_activations ((TRUE))'; do
  grep -Fq "$boundary" "$activation_migration"
done
python3 - "$activation_migration" <<'PY'
from pathlib import Path
import sys


class RuntimeAclError(ValueError):
    pass


def assert_runtime_acl_arrays(source: str) -> None:
    runtime_acl = source.split(
        "CREATE OR REPLACE FUNCTION paper_raid_bff_runtime_acl_state_v2()", 1
    )[1].split("$function$;", 1)[0]
    for catalog_acl in ("a.attacl", "parameter.paracl"):
        if runtime_acl.count(f"aclexplode({catalog_acl})") != 1:
            raise RuntimeAclError(
                f"runtime ACL state must pass nullable {catalog_acl} directly to aclexplode"
            )
    if "'{}'::aclitem[]" in runtime_acl:
        raise RuntimeAclError(
            "runtime ACL state converts NULL ACLs to a zero-dimensional empty array"
        )
    for default_acl in (
        "database_row.datacl, acldefault('d', database_row.datdba)",
        "namespace.nspacl, acldefault('n', namespace.nspowner)",
        "procedure.proacl, acldefault('f', procedure.proowner)",
    ):
        if runtime_acl.count(default_acl) != 1:
            raise RuntimeAclError(f"runtime ACL default path drifted: {default_acl}")


source = Path(sys.argv[1]).read_text(encoding="utf-8")
assert_runtime_acl_arrays(source)
hostile_mutants = {
    "column NULL coerced to empty ACL": source.replace(
        "aclexplode(a.attacl)",
        "aclexplode(COALESCE(a.attacl, '{}'::aclitem[]))",
        1,
    ),
    "parameter NULL coerced to empty ACL": source.replace(
        "aclexplode(parameter.paracl)",
        "aclexplode(COALESCE(parameter.paracl, '{}'::aclitem[]))",
        1,
    ),
    "database default ACL removed": source.replace(
        "database_row.datacl, acldefault('d', database_row.datdba)",
        "database_row.datacl, '{}'::aclitem[]",
        1,
    ),
    "schema default ACL removed": source.replace(
        "namespace.nspacl, acldefault('n', namespace.nspowner)",
        "namespace.nspacl, '{}'::aclitem[]",
        1,
    ),
    "function default ACL removed": source.replace(
        "procedure.proacl, acldefault('f', procedure.proowner)",
        "procedure.proacl, '{}'::aclitem[]",
        1,
    ),
}
for name, mutant in hostile_mutants.items():
    if mutant == source:
        raise SystemExit(f"runtime ACL hostile mutant was not constructed: {name}")
    try:
        assert_runtime_acl_arrays(mutant)
    except RuntimeAclError:
        continue
    raise SystemExit(f"runtime ACL static gate accepted hostile mutant: {name}")
PY
for boundary in \
  'paper_raid_bff_cluster_identity_v3()' \
  'SECURITY DEFINER' \
  'pg_control_system()' \
  'trnm.paper-raid.invite-alpha-local-approval.v3' \
  'root_provisioned_local_only' \
  'trnm.paper-raid.invite-alpha-runtime-acl-evidence.v2' \
  'local_approval_sha256_value' \
  'runtime_acl_evidence_sha256_value' \
  'approval_sequence_value BETWEEN 1 AND 9007199254740991' \
  'cluster_system_identifier_value' \
  "interval '60 seconds' AND interval '86400 seconds'" \
  'paper_raid_bff_invite_activation_authorities_v3' \
  'paper_raid_bff_invite_activation_monotonic_v3' \
  'paper_raid_bff_invite_activation_immutable_v3' \
  'paper_raid_bff_invite_activation_truncate_v3' \
  'paper_raid_bff_invite_v3_local_approval_key' \
  'paper_raid_bff_invite_v3_nonce_key' \
  '"paper_raid_bff_invite_activation_authorities_v3_local_approval_"' \
  '"paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_ke"' \
  'paper_raid_bff_invite_activation_v3_deployment_sequence_key' \
  'paper_raid_bff_invite_activation_v3_parity_ck' \
  "SELECT count(*) = 13 AND bool_and(key = ANY(ARRAY[" \
  "SELECT count(*) = 8 AND bool_and(key = ANY(ARRAY[" \
  "'release_id','runtime_acl_sha256','schema'" \
  "'accessctl','bff','hepta','nakama','object_store'" \
  "'object_store_client','ops','postgres'" \
  'LOCK TABLE paper_raid_bff_invite_activation_authorities_v3' \
  'VALIDATE CONSTRAINT paper_raid_bff_invite_activation_v3_parity_ck' \
  'BEFORE UPDATE OR DELETE ON paper_raid_bff_invite_activation_authorities_v3' \
  'BEFORE TRUNCATE ON paper_raid_bff_invite_activation_authorities_v3' \
  'invite activation tombstones cannot be deleted' \
  'invite activation tombstones cannot be truncated' \
  'existing.approval_sequence >= NEW.approval_sequence' \
  'paper_raid_bff_one_active_invite_activation_v3' \
  "VALUES ('invite_activation_authority_v3')"; do
  grep -Fq "$boundary" "$activation_migration"
done
python3 - "$activation_migration" <<'PY'
from pathlib import Path
import re
import sys


class TransactionBoundaryError(ValueError):
    pass


outer_begin = re.compile(r"(?m)^BEGIN;$")
outer_commit = re.compile(r"(?m)^COMMIT;$")
outer_rollback = re.compile(r"(?m)^ROLLBACK;$")
lock_statement = (
    "LOCK TABLE paper_raid_bff_invite_activation_authorities_v3\n"
    "    IN ACCESS EXCLUSIVE MODE;"
)


def assert_atomic_migration(source: str) -> None:
    begins = list(outer_begin.finditer(source))
    commits = list(outer_commit.finditer(source))
    if len(begins) != 1 or len(commits) != 1:
        raise TransactionBoundaryError(
            "0007 must have exactly one outer BEGIN and one outer COMMIT"
        )
    if outer_rollback.search(source):
        raise TransactionBoundaryError("0007 must not expose an outer ROLLBACK")

    begin = begins[0]
    commit = commits[0]
    prefix_lines = source[: begin.start()].splitlines()
    if any(line.strip() and not line.lstrip().startswith("--") for line in prefix_lines):
        raise TransactionBoundaryError("0007 outer BEGIN is not the first SQL command")
    if source[commit.end() :].strip():
        raise TransactionBoundaryError("0007 outer COMMIT is not the last SQL command")
    if source.count(lock_statement) != 1:
        raise TransactionBoundaryError(
            "0007 must retain exactly one canonical ACCESS EXCLUSIVE lock"
        )
    lock_position = source.index(lock_statement)
    if not begin.end() < lock_position < commit.start():
        raise TransactionBoundaryError(
            "0007 ACCESS EXCLUSIVE lock escaped the outer transaction"
        )


source = Path(sys.argv[1]).read_text(encoding="utf-8")
assert_atomic_migration(source)

without_lock = source.replace(lock_statement, "", 1)
hostile_mutants = {
    "missing BEGIN": source.replace("BEGIN;\n", "", 1),
    "duplicate BEGIN": source.replace("BEGIN;\n", "BEGIN;\nBEGIN;\n", 1),
    "LOCK before BEGIN": lock_statement + "\n\n" + without_lock,
    "missing COMMIT": source.replace("\nCOMMIT;\n", "\n", 1),
    "ROLLBACK terminator": source.replace("\nCOMMIT;\n", "\nROLLBACK;\n", 1),
}
for name, mutant in hostile_mutants.items():
    try:
        assert_atomic_migration(mutant)
    except TransactionBoundaryError:
        continue
    raise SystemExit(f"0007 transaction boundary accepted hostile mutant: {name}")
PY
python3 - "$activation_migration" "$db" "$atomic_activation_sql" <<'PY'
from pathlib import Path
import re
import sys


class ManagedCatalogError(ValueError):
    pass


canonical = {
    "paper_raid_bff_invite_v3_local_approval_key": "local_approval_sha256",
    "paper_raid_bff_invite_v3_nonce_key": "nonce_sha256",
}
legacy = {
    "paper_raid_bff_invite_activation_authorities_v3_local_approval_sha256_key":
        "paper_raid_bff_invite_activation_authorities_v3_local_approval_",
    "paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_key":
        "paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_ke",
}


def require_once(source: str, fragment: str, description: str) -> int:
    if source.count(fragment) != 1:
        raise ManagedCatalogError(f"0007 must contain one {description}")
    return source.index(fragment)


def assert_managed_catalog(migration: str, readiness_sources: tuple[str, ...]) -> None:
    managed_creates = re.findall(
        r"(?:ADD CONSTRAINT|CREATE(?: UNIQUE)? INDEX)\s+([a-z0-9_]+)",
        migration,
    )
    if any(len(name.encode("utf-8")) > 63 for name in managed_creates):
        raise ManagedCatalogError("0007 creates an identifier longer than 63 bytes")

    add_positions = []
    cleanup_positions = []
    for name, column in canonical.items():
        if len(name.encode("utf-8")) > 63:
            raise ManagedCatalogError("canonical v3 unique name exceeds 63 bytes")
        cleanup_positions.append(require_once(
            migration,
            f"DROP CONSTRAINT IF EXISTS {name},",
            f"canonical constraint cleanup for {name}",
        ))
        cleanup_positions.append(require_once(
            migration,
            f"DROP INDEX IF EXISTS {name};",
            f"canonical index cleanup for {name}",
        ))
        add_positions.append(require_once(
            migration,
            f"ADD CONSTRAINT {name}\n        UNIQUE ({column}),",
            f"canonical UNIQUE rebuild for {name}",
        ))
        if migration.count(f"'{name}'") != 2:
            raise ManagedCatalogError(
                f"0007 expected catalogs do not name only canonical {name}"
            )
        for readiness in readiness_sources:
            if readiness.count(f"WHEN '{name}' THEN") != 2:
                raise ManagedCatalogError(
                    f"resident readiness does not name canonical {name} exactly twice"
                )

    for long_name, truncated_name in legacy.items():
        if len(long_name.encode("utf-8")) <= 63:
            raise ManagedCatalogError("legacy source spelling no longer exercises truncation")
        if len(truncated_name.encode("utf-8")) != 63:
            raise ManagedCatalogError("legacy PostgreSQL identifier is not 63 bytes")
        cleanup_positions.append(require_once(
            migration,
            f"DROP CONSTRAINT IF EXISTS\n        {long_name},",
            f"long legacy constraint cleanup for {long_name}",
        ))
        cleanup_positions.append(require_once(
            migration,
            f'DROP CONSTRAINT IF EXISTS\n        "{truncated_name}",',
            f"truncated legacy constraint cleanup for {truncated_name}",
        ))
        cleanup_positions.append(require_once(
            migration,
            f"DROP INDEX IF EXISTS\n    {long_name};",
            f"long legacy index cleanup for {long_name}",
        ))
        cleanup_positions.append(require_once(
            migration,
            f'DROP INDEX IF EXISTS\n    "{truncated_name}";',
            f"truncated legacy index cleanup for {truncated_name}",
        ))
        verify_catalog = migration.split("DO $verify_v3_managed_catalog$", 1)[1]
        if long_name in verify_catalog or truncated_name in verify_catalog:
            raise ManagedCatalogError("0007 expected catalog accepts a legacy name")
        if any(
            long_name in readiness or truncated_name in readiness
            for readiness in readiness_sources
        ):
            raise ManagedCatalogError("resident readiness accepts a legacy name")

    if max(cleanup_positions) >= min(add_positions):
        raise ManagedCatalogError("0007 rebuilds canonical uniqueness before legacy cleanup")


migration = Path(sys.argv[1]).read_text(encoding="utf-8")
readiness = tuple(
    Path(path).read_text(encoding="utf-8") for path in sys.argv[2:]
)
assert_managed_catalog(migration, readiness)

local_name = "paper_raid_bff_invite_v3_local_approval_key"
local_long = (
    "paper_raid_bff_invite_activation_authorities_v3_"
    "local_approval_sha256_key"
)
local_truncated = "paper_raid_bff_invite_activation_authorities_v3_local_approval_"
overlong = local_name + "_identifier_limit_regression_padding"
hostile_mutants = {
    "overlong managed identifier": (
        migration.replace(
            f"ADD CONSTRAINT {local_name}", f"ADD CONSTRAINT {overlong}", 1
        ),
        readiness,
    ),
    "truncated expected catalog name": (
        migration.replace(f"'{local_name}'", f"'{local_truncated}'", 1),
        readiness,
    ),
    "missing truncated constraint cleanup": (
        migration.replace(
            f'DROP CONSTRAINT IF EXISTS\n        "{local_truncated}",\n',
            "",
            1,
        ),
        readiness,
    ),
    "missing long index cleanup": (
        migration.replace(f"DROP INDEX IF EXISTS\n    {local_long};\n", "", 1),
        readiness,
    ),
    "truncated resident readiness name": (
        migration,
        (
            readiness[0].replace(
                f"WHEN '{local_name}' THEN",
                f"WHEN '{local_truncated}' THEN",
                1,
            ),
            readiness[1],
        ),
    ),
}
for name, (migration_mutant, readiness_mutants) in hostile_mutants.items():
    try:
        assert_managed_catalog(migration_mutant, readiness_mutants)
    except ManagedCatalogError:
        continue
    raise SystemExit(f"0007 managed catalog accepted hostile mutant: {name}")
PY
for boundary in \
  'pg_get_viewdef(c.oid, true)' \
  'expected_runtime_acl_source' \
  'expected_invite_activation_validator_source' \
  'acl.trim() == expected_acl.trim()' \
  "c.relname = 'paper_raid_bff_invite_activations'" \
  "pg_get_userbyid(c.relowner) = 'paper_raid_bff'" \
  "pg_get_expr(i.indexprs, i.indrelid)" \
  'SELECT count(a.attnum) = 26' \
  'SELECT count(*) = 2 AND bool_and(' \
  'SELECT count(*) = 3 FROM pg_index' \
  'AND NOT p.proisstrict'; do
  grep -Fq "$boundary" "$db"
done
for boundary in \
  'invite_activation_v3_schema_ready' \
  'expected_invite_activation_v3_sources' \
  "'paper_raid_bff_invite_activation_authorities_v3'" \
  "capability = 'invite_activation_authority_v3'" \
  'SELECT count(*) = 5 AND bool_and(' \
  'SELECT count(*) = 6 AND bool_and(' \
  'trigger_row.tgtype = 34' \
  "procedure.proconfig = expected.config" ; do
  grep -Fq "$boundary" "$db"
done
for boundary in \
  'WITH catalog_exact AS MATERIALIZED (' \
  'installed_sources AS MATERIALIZED (' \
  'selected_authority AS MATERIALIZED (' \
  'FOR SHARE' \
  'live_identity AS MATERIALIZED (' \
  'paper_raid_bff_cluster_identity_v3()).*' \
  'live_acl AS MATERIALIZED (' \
  'THEN EXISTS (' \
  'SELECT count(*) = 5 AND bool_and(' \
  'SELECT count(*) = 6 AND bool_and(' \
  'trigger_row.tgtype = 34' \
  'procedure.proconfig = expected.config' \
  "'paper_raid_bff_invite_activation_row_valid_v3(jsonb,text,text,text,text,uuid,text,text,oid,text,bigint,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure) = \$30" \
  "'paper_raid_bff_cluster_identity_v3()'::regprocedure) = \$31" \
  "'paper_raid_bff_invite_activation_monotonic_v3()'::regprocedure) = \$32" \
  "'paper_raid_bff_invite_activation_immutable_v3()'::regprocedure) = \$33" \
  "'paper_raid_bff_invite_activation_truncate_v3()'::regprocedure) = \$34" \
  "'paper_raid_bff_runtime_acl_state_v2()'::regprocedure) = \$35" \
  "relation.relname =" \
  "'paper_raid_bff_runtime_acl_state_v2'" \
  "ARRAY['security_barrier=true']::text[]" \
  'pg_get_viewdef(relation.oid, true)' \
  'authority.local_approval_sha256 = $2' \
  'authority.approval_sequence = $3' \
  'authority.nonce_sha256 = $4' \
  'identity.database_oid = authority.database_oid' \
  'identity.cluster_system_identifier =' \
  "sha256(convert_to(authority.local_approval, 'UTF8'))" \
  "sha256(convert_to(" \
  "authority.approval_record ->> 'release_id' = \$18" \
  "authority.approval_record #>> '{images,postgres}' = \$19" \
  "authority.approval_record #>> '{images,object_store}' = \$20" \
  "authority.approval_record #>> '{images,object_store_client}' = \$21" \
  "authority.approval_record #>> '{images,ops}' = \$22" \
  "authority.approval_record #>> '{images,nakama}' = \$23" \
  "authority.approval_record #>> '{images,hepta}' = \$24" \
  "authority.approval_record #>> '{images,bff}' = \$25" \
  "authority.approval_record #>> '{images,accessctl}' = \$26" \
  "authority.approval_record #>> '{hepta,revision}' = \$27" \
  "authority.approval_record #>> '{hepta,source_tree}' = \$28" \
  "authority.approval_record #>> '{hepta,fileset_sha256}' = \$29" \
  'authority.revoked_at IS NULL' \
  'authority.expires_at > statement_timestamp()' \
  'authority.economy_eligibility = FALSE'; do
  grep -Fq "$boundary" "$atomic_activation_sql"
done
for boundary in \
  'PAPER_RAID_BFF_INVITE_APPROVAL_SEQUENCE' \
  'PAPER_RAID_BFF_INVITE_APPROVAL_NONCE_SHA256' \
  'PAPER_RAID_BFF_INVITE_DATABASE_NAME' \
  'PAPER_RAID_BFF_INVITE_DATABASE_OID' \
  'PAPER_RAID_BFF_INVITE_CLUSTER_SYSTEM_IDENTIFIER' \
  'PAPER_RAID_BFF_INVITE_RELEASE_ID' \
  'PAPER_RAID_BFF_INVITE_POSTGRES_IMAGE' \
  'PAPER_RAID_BFF_INVITE_OBJECT_STORE_IMAGE' \
  'PAPER_RAID_BFF_INVITE_OBJECT_STORE_CLIENT_IMAGE' \
  'PAPER_RAID_BFF_INVITE_OPS_IMAGE' \
  'PAPER_RAID_BFF_INVITE_NAKAMA_IMAGE' \
  'PAPER_RAID_BFF_INVITE_HEPTA_IMAGE' \
  'PAPER_RAID_BFF_INVITE_BFF_IMAGE' \
  'PAPER_RAID_BFF_INVITE_ACCESSCTL_IMAGE' \
  'PAPER_RAID_BFF_INVITE_HEPTA_REVISION' \
  'PAPER_RAID_BFF_INVITE_HEPTA_SOURCE_TREE' \
  'PAPER_RAID_BFF_INVITE_HEPTA_FILESET_SHA256'; do
  grep -Fq "$boundary" "$config"
done
ready_source=$(sed -n '/^pub async fn invite_activation_ready/,/^}/p' "$db")
grep -Fq 'include_str!("invite_activation_v3_atomic.sql")' <<<"$ready_source"
if grep -Fq 'invite_activation_schema_ready(pool).await' <<<"$ready_source"; then
  echo 'resident readiness still splits catalog and authority checks across statements' >&2
  exit 1
fi
if grep -Fq 'FROM paper_raid_bff_invite_activations ' <<<"$ready_source"; then
  echo 'resident readiness still accepts the legacy v2 activation table' >&2
  exit 1
fi
python3 - "$activation_migration" <<'PY'
from pathlib import Path
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
constraint = source.split(
    "CONSTRAINT paper_raid_bff_invite_activation_parity_ck CHECK (", 1
)[1].split("\n    )\n);", 1)[0]
if "COALESCE(" not in constraint or ",\n            FALSE" not in constraint:
    raise SystemExit("invite activation CHECK does not fail closed on SQL NULL")
PY
grep -Fq 'enforce_invite_activation' "$app"
grep -Fq 'invite_activation_ready(&state.pool, invite).await' "$app"
python3 - "$atomic_activation_sql" "$db" "$activation_migration" <<'PY'
from pathlib import Path
import re
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
db_source = Path(sys.argv[2]).read_text(encoding="utf-8")
migration_source = Path(sys.argv[3]).read_text(encoding="utf-8")
if source.count(";") != 1 or not source.rstrip().endswith(";"):
    raise SystemExit("Invite activation authority must be one PostgreSQL statement")
binds = {int(value) for value in re.findall(r"\$(\d+)", source)}
if binds != set(range(1, 36)):
    raise SystemExit("Invite activation atomic statement bind set drifted")
ordered = [
    "WITH catalog_exact AS MATERIALIZED (",
    "installed_sources AS MATERIALIZED (",
    "selected_authority AS MATERIALIZED (",
    "live_identity AS MATERIALIZED (",
    "live_acl AS MATERIALIZED (",
    "SELECT CASE",
    "WHEN COALESCE((SELECT * FROM catalog_exact), FALSE)",
]
positions = [source.index(fragment) for fragment in ordered]
if positions != sorted(positions):
    raise SystemExit("Invite activation atomic statement boundary ordering drifted")


class CatalogShapeError(ValueError):
    pass


def compact(candidate: str) -> str:
    return re.sub(r"\s+", " ", candidate.replace("\\", " ")).strip()


def assert_catalog_shape(candidate: str) -> None:
    normalized = compact(candidate)
    required = {
        "type-exact connoinherit":
            "constraint_row.connoinherit = (constraint_row.contype IN ('p','u'))",
        "superuser system-function owner":
            "JOIN pg_roles owner_role ON owner_role.oid = procedure.proowner",
        "superuser owner predicate": "AND owner_role.rolsuper",
        "same-owner ACL cardinality":
            "SELECT count(*) = CASE WHEN owner_role.rolname = 'paper_raid_bff' "
            "THEN 1 ELSE 2 END AND bool_and(",
        "runtime system-function denial":
            "AND NOT has_function_privilege( 'paper_raid_bff_runtime', "
            "procedure.oid, 'EXECUTE' )",
    }
    for description, fragment in required.items():
        if normalized.count(fragment) != 1:
            raise CatalogShapeError(f"Invite activation catalog lost {description}")
    owner_acl = "(acl.grantee = procedure.proowner AND NOT acl.is_grantable)"
    if normalized.count(owner_acl) != 2:
        raise CatalogShapeError(
            "Invite activation catalog must model both owner ACL rows as non-grantable"
        )
    if "pg_get_userbyid(procedure.proowner) = 'postgres'" in normalized:
        raise CatalogShapeError("Invite activation hard-codes a bootstrap owner name")


def assert_pg_control_ceremony(candidate: str) -> None:
    test = candidate.split(
        "real_postgres_v3_atomic_catalog_authority_and_revocation_gate", 1
    )[1]
    for command in (
        "REVOKE ALL ON FUNCTION pg_catalog.pg_control_system() FROM PUBLIC;",
        "GRANT EXECUTE ON FUNCTION pg_catalog.pg_control_system() \\",
        "TO paper_raid_bff;",
    ):
        if test.count(command) != 1:
            raise CatalogShapeError(
                f"Invite activation PG test lacks exact pg_control ceremony: {command}"
            )


validator_marker = (
    "CREATE OR REPLACE FUNCTION "
    "paper_raid_bff_invite_activation_row_valid_v3("
)
expected_validator_declaration = ", ".join((
    "approval_record_value JSONB",
    "local_approval_value TEXT",
    "local_approval_sha256_value TEXT",
    "runtime_acl_evidence_value TEXT",
    "runtime_acl_evidence_sha256_value TEXT",
    "activation_id_value UUID",
    "deployment_identity_value TEXT",
    "database_name_value TEXT",
    "database_oid_value OID",
    "cluster_system_identifier_value TEXT",
    "approval_sequence_value BIGINT",
    "nonce_sha256_value TEXT",
    "issued_at_value TIMESTAMPTZ",
    "expires_at_value TIMESTAMPTZ",
    "revoked_at_value TIMESTAMPTZ",
    "revocation_reason_value TEXT",
    "economy_eligibility_value BOOLEAN",
))


def normalized_declaration(source_text: str, test_only: bool) -> str:
    if test_only:
        source_text = source_text.split(
            "real_postgres_v3_atomic_catalog_authority_and_revocation_gate", 1
        )[1]
    declaration = source_text.split(validator_marker, 1)[1].split(")", 1)[0]
    declaration = compact(declaration)
    return re.sub(r"\s*,\s*", ", ", declaration)


def assert_drift_signature(candidate: str) -> None:
    migration_declaration = normalized_declaration(migration_source, False)
    drift_declaration = normalized_declaration(candidate, True)
    if migration_declaration != expected_validator_declaration:
        raise CatalogShapeError("0007 validator parameter declaration drifted")
    if drift_declaration != migration_declaration:
        raise CatalogShapeError(
            "Invite activation drift DDL must preserve all validator parameter names"
        )


def replace_drift_declaration(candidate: str, replacement: str) -> str:
    test_start = candidate.index(
        "real_postgres_v3_atomic_catalog_authority_and_revocation_gate"
    )
    declaration_start = candidate.index(validator_marker, test_start) + len(validator_marker)
    declaration_end = candidate.index(") RETURNS BOOLEAN", declaration_start)
    return candidate[:declaration_start] + replacement + candidate[declaration_end:]


assert_catalog_shape(source)
assert_catalog_shape(db_source)
assert_pg_control_ceremony(db_source)
assert_drift_signature(db_source)

hostile_catalog_mutants = {
    "all constraints forced inheritable": source.replace(
        "constraint_row.connoinherit =\n"
        "                        (constraint_row.contype IN ('p','u'))",
        "NOT constraint_row.connoinherit",
        1,
    ),
    "owner ACL marked grantable": source.replace(
        "acl.grantee = procedure.proowner\n"
        "                                       AND NOT acl.is_grantable",
        "acl.grantee = procedure.proowner\n"
        "                                       AND acl.is_grantable",
        1,
    ),
    "bootstrap owner hard-coded": source.replace(
        "AND owner_role.rolsuper",
        "AND pg_get_userbyid(procedure.proowner) = 'postgres'",
        1,
    ),
    "distinct-owner ACL count forced": source.replace(
        "SELECT count(*) = CASE\n"
        "                                      WHEN owner_role.rolname = 'paper_raid_bff'\n"
        "                                      THEN 1 ELSE 2\n"
        "                                  END",
        "SELECT count(*) = 2",
        1,
    ),
    "runtime pg_control execution allowed": source.replace(
        "AND NOT has_function_privilege(",
        "AND has_function_privilege(",
        1,
    ),
}
for name, mutant in hostile_catalog_mutants.items():
    if mutant == source:
        raise SystemExit(f"Invite activation catalog mutant was not constructed: {name}")
    try:
        assert_catalog_shape(mutant)
    except CatalogShapeError:
        continue
    raise SystemExit(f"Invite activation catalog accepted hostile mutant: {name}")

hostile_ceremony_mutants = {
    "PUBLIC pg_control execution retained": db_source.replace(
        "REVOKE ALL ON FUNCTION pg_catalog.pg_control_system() FROM PUBLIC; \\\n",
        "",
        1,
    ),
    "schema owner pg_control grant omitted": db_source.replace(
        "GRANT EXECUTE ON FUNCTION pg_catalog.pg_control_system() \\\n"
        "             TO paper_raid_bff; \\\n",
        "",
        1,
    ),
}
for name, mutant in hostile_ceremony_mutants.items():
    if mutant == db_source:
        raise SystemExit(f"Invite activation ceremony mutant was not constructed: {name}")
    try:
        assert_pg_control_ceremony(mutant)
    except CatalogShapeError:
        continue
    raise SystemExit(f"Invite activation ceremony accepted hostile mutant: {name}")

hostile_drift_mutants = {
    "anonymous validator parameters": replace_drift_declaration(
        db_source,
        " JSONB, TEXT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, OID, TEXT, "
        "BIGINT, TEXT, TIMESTAMPTZ, TIMESTAMPTZ, TIMESTAMPTZ, TEXT, BOOLEAN ",
    ),
    "renamed validator parameter": replace_drift_declaration(
        db_source,
        " " + expected_validator_declaration.replace(
            "approval_record_value", "approval_record_drift", 1
        ) + " ",
    ),
}
for name, mutant in hostile_drift_mutants.items():
    try:
        assert_drift_signature(mutant)
    except CatalogShapeError:
        continue
    raise SystemExit(f"Invite activation drift DDL accepted hostile mutant: {name}")
PY
if grep -Fq 'paper_raid_bff_runtime_acl_state_v1' "$db" || \
   grep -Eq 'CREATE( OR REPLACE)? VIEW paper_raid_bff_runtime_acl_state_v1' "$activation_migration"; then
  echo "legacy spoofable runtime ACL view remains in the activation path" >&2
  exit 1
fi
grep -Fq 'LOGIN_BUCKET_DOMAIN' "$access"
grep -Fq 'login_bucket_principal(&candidate_hash)' "$access"
if grep -Fq 'known_candidate' "$access"; then
  echo "quota bucket depends on credential existence" >&2
  exit 1
fi
grep -Fq 'paper_raid_bff_access_audit is append-only' "$migration"
grep -Fq 'BEFORE UPDATE OR DELETE ON paper_raid_bff_access_audit' "$migration"
grep -Fq "outcome IN ('succeeded', 'denied', 'indeterminate')" "$operator_audit_migration"
grep -Fq "'accessctl_operator_audit_v1'" "$operator_audit_migration"
grep -Fq "'accessctl_operator_audit_v1'" "$db"
grep -Fq 'paper_raid_bff_operator_audit_row_valid' "$operator_audit_migration"
grep -Fq 'paper_raid_bff_one_operator_event_per_attempt' "$operator_audit_migration"
grep -Fq 'SELECT COALESCE((CASE' "$operator_audit_migration"
grep -Fq 'operator audit JSON-null negative test failed' "$operator_audit_migration"
for field in command_code command_state reason_code; do
  grep -Fq "'${field}'" "$operator_audit_migration"
done
grep -Fq 'paper_raid_bff_object_audit_attempt_idx' "$operator_audit_migration"
grep -Fq 'paper_raid_bff_object_audit_attempt_parent' "$operator_audit_migration"
grep -Fq 'paper_raid_bff_require_object_audit_attempt' "$operator_audit_migration"
grep -Fq "result_row.action = 'operator_command_result'" "$operator_audit_migration"
grep -Fq 'parent.operator_subject = result_row.operator_subject' "$operator_audit_migration"
grep -Fq "NEW.operator_event = 'operator_command_result'" "$operator_audit_migration"
grep -Fq 'new accessctl audit cannot claim legacy-unavailable lineage' "$operator_audit_migration"
grep -Fq 'access audit append-only function definition is missing or drifted' "$operator_audit_migration"
grep -Fq 'access audit append-only trigger definition is missing or drifted' "$operator_audit_migration"
grep -Fq 'DROP TRIGGER paper_raid_bff_access_audit_append_only' "$operator_audit_migration"
grep -Fq 'CREATE TRIGGER paper_raid_bff_access_audit_append_only' "$operator_audit_migration"
python3 - "$operator_audit_migration" <<'PY'
from pathlib import Path
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
needles = [
    "access audit append-only function definition is missing or drifted",
    "access audit append-only trigger definition is missing or drifted",
    "DROP TRIGGER paper_raid_bff_access_audit_append_only",
    "UPDATE paper_raid_bff_access_audit",
    "CREATE TRIGGER paper_raid_bff_access_audit_append_only",
    "VALUES ('accessctl_operator_audit_v1')",
]
positions = [source.index(needle) for needle in needles]
if positions != sorted(positions):
    raise SystemExit("accessctl audit migration guard/backfill/restore order drifted")
if "-- no-transaction" in source.lower() or "\nCOMMIT" in source.upper():
    raise SystemExit("accessctl audit migration escaped transactional execution")
PY
grep -Fq '\ir ../0003_invite_alpha_access.sql' "$operator_audit_upgrade_fixture"
grep -Fq '\ir ../0006_accessctl_operator_audit.sql' "$operator_audit_upgrade_fixture"
for lineage in linked legacy_unavailable not_applicable; do
  grep -Fq "${lineage}" "$operator_audit_upgrade_fixture"
done
grep -Fq 'canonical append-only trigger was not restored' "$operator_audit_upgrade_fixture"
grep -Fq 'new legacy_unavailable row unexpectedly succeeded' "$operator_audit_upgrade_fixture"
grep -Fq 'orphan result history was not downgraded to legacy_unavailable' "$operator_audit_upgrade_fixture"
grep -Fq 'subject-mismatched result history was not downgraded to legacy_unavailable' "$operator_audit_upgrade_fixture"
grep -Fq 'new orphan operator result unexpectedly succeeded' "$operator_audit_upgrade_fixture"
grep -Fq 'new subject-mismatched operator result unexpectedly succeeded' "$operator_audit_upgrade_fixture"
grep -Fq "RAISE EXCEPTION 'tampered append-only guard'" "$operator_audit_tamper_fixture"
grep -Fq '\ir ../0006_accessctl_operator_audit.sql' "$operator_audit_tamper_fixture"
grep -Fq 'BEFORE UPDATE ON paper_raid_bff_access_audit' "$operator_audit_trigger_tamper_fixture"
grep -Fq '\ir ../0006_accessctl_operator_audit.sql' "$operator_audit_trigger_tamper_fixture"
grep -Fxq 'BEGIN;' "$operator_audit_rollback_fixture"
grep -Fq 'DROP TRIGGER paper_raid_bff_access_audit_append_only' "$operator_audit_rollback_fixture"
grep -Fq '0006 fixture deliberate failure after DROP' "$operator_audit_rollback_fixture"
grep -Fq 'rollback did not restore the canonical append-only guard' "$operator_audit_rollback_verify_fixture"
grep -Fq 'rollback did not restore the pre-migration audit row' "$operator_audit_rollback_verify_fixture"
grep -Fq 'rollback did not restore the pre-0006 outcome constraint' "$operator_audit_rollback_verify_fixture"
grep -Fq 'rollback left the 0006 capability exposed' "$operator_audit_rollback_verify_fixture"
for fixture in \
  0006_accessctl_operator_audit_upgrade.sql \
  0006_accessctl_operator_audit_tampered_guard.sql \
  0006_accessctl_operator_audit_tampered_trigger.sql \
  0006_accessctl_operator_audit_rollback_after_drop.sql \
  0006_accessctl_operator_audit_rollback_verify.sql; do
  grep -Fq "$fixture" "$postgres_gate"
done
for fixture in \
  0007_invite_activation_nullable_and_json_null.sql \
  0007_invite_activation_strict_tamper.sql; do
  grep -Fq "$fixture" "$postgres_gate"
done
grep -Fq 'valid active activation with nullable revocation fields failed' \
  "$activation_nullable_fixture"
grep -Fq 'activation validator accepted a required JSON null' \
  "$activation_nullable_fixture"
grep -Fq 'activation CHECK accepted a required JSON null' \
  "$activation_nullable_fixture"
grep -Fq ') STRICT;' "$activation_strict_tamper_fixture"
grep -Fq 'STRICT validator passed the non-strict readiness predicate' \
  "$activation_strict_tamper_fixture"
grep -Fq 'outer activation CHECK accepted a STRICT NULL result' \
  "$activation_strict_tamper_fixture"
grep -Fq 'idempotent 0007 did not restore CALLED ON NULL INPUT' \
  "$activation_strict_tamper_fixture"
grep -Fq 'idempotent 0007 did not restore fail-closed CHECK' \
  "$activation_strict_tamper_fixture"
grep -Fq 'run_fixture_failure' "$postgres_gate"
grep -Fq 'new psql connection' "$postgres_gate"
grep -Fq 'real_postgres_v3_atomic_catalog_authority_and_revocation_gate' \
  "$postgres_gate"
for lineage in linked legacy_unavailable not_applicable; do
  grep -Fq "'${lineage}'" "$operator_audit_migration"
done
grep -Fq "NEW.operator_lineage_status = 'legacy_unavailable'" "$operator_audit_migration"
grep -Fq "idx.relname = 'paper_raid_bff_one_operator_event_per_attempt'" "$db"
grep -Fq "idx.relname = 'paper_raid_bff_object_audit_attempt_idx'" "$db"
grep -Fq "t.tgname = 'paper_raid_bff_object_audit_attempt_parent'" "$db"
grep -Fq "'paper_raid_bff_operator_audit_shape_ck'" "$db"
grep -Fq "'paper_raid_bff_access_audit_outcome_check'" "$db"
grep -Fq 'expected_operator_audit_validator_source' "$db"
grep -Fq 'expected_object_audit_parent_source' "$db"
grep -Fq 'normalize_sql(&installed) == normalize_sql(expected)' "$db"
grep -Fq "t.tgtype = 27" "$db"
if grep -Fq "position('''indeterminate'''" "$db"; then
  echo "accessctl readiness uses substring outcome verification" >&2
  exit 1
fi

if grep -Eq 'route\([^)]*(accessctl|invite|batch|account-(suspend|close)|credential-rotate)' "$app"; then
  echo "operator control leaked into the network router" >&2
  exit 1
fi

if grep -Eqi 'hepta_league_state|consumer-entry-api|LeagueState::|paper_score|league_reward' "$migration" "$access" "$ctl"; then
  echo "invite alpha crossed the Paper Raid authority boundary" >&2
  exit 1
fi

if grep -Eq '(invite_secret|login_credential|login_key)[[:space:]]+(TEXT|BYTEA)' "$migration"; then
  echo "cleartext credential column detected" >&2
  exit 1
fi

grep -Fq 'secret_hash BYTEA' "$migration"
grep -Fq 'displayed_once_not_stored' "$ctl"
grep -Fq 'env::var("PAPER_RAID_BFF_IDENTITY_MODE").as_deref() != Ok("invite_alpha")' "$ctl"
grep -Fq 'CommandReason::IdentityModeRejected' "$ctl"
grep -Fq 'PAPER_RAID_ACCESS_DATABASE_URL' "$ctl"
if grep -Fq 'required_env("PAPER_RAID_BFF_DATABASE_URL")' "$ctl"; then
  echo "operator CLI reads the resident BFF database credential" >&2
  exit 1
fi
grep -Fq 'invite_schema_ready(&pool)' "$app"
grep -Fq 'revoke_sessions(&mut tx, subject)' "$ctl"
object_audit_source=$(sed -n '/^async fn audit(/,/^enum OperatorAuditReadback/p' "$ctl")
grep -Fq 'operator_attempt_id, operator_event' <<<"$object_audit_source"
grep -Fq '.bind(attempt_id)' <<<"$object_audit_source"
grep -Fq "VALUES (\$1, \$2, \$3, \$4, 'succeeded', \$5, \$6, NULL, 'linked')" <<<"$object_audit_source"

grep -Fq '"schema":"paper-raid-bff.account-export.v2"' "$ctl"
grep -Fq '"scope":"bff_local_access_directory"' "$ctl"
grep -Fq '"global_account_export_complete":false' "$ctl"
grep -Fq '"consistency":"repeatable_read_read_only"' "$ctl"
grep -Fq '"current_account_export_audit_included":false' "$ctl"
grep -Fq '"current_account_export_audit_write":"separate_atomic_audit_transaction_after_export_snapshot"' "$ctl"
grep -Fq '"included_bff_records"' "$ctl"
for record in accounts account_scopes account_author_roles login_credentials_lifecycle invites access_audit; do
  grep -Fq "\"paper_raid_bff_${record}\"" "$ctl"
done
for component in bff_sessions bff_request_security_state bff_agent_bridge bff_product_telemetry bff_invite_batches bff_quota_and_retention_metadata bff_schema_metadata bff_operator_command_audit hepta nakama cas_reachability cas_bytes backups; do
  grep -Fq "\"${component}\"" "$ctl"
done
grep -Fq '"status":"not_queried"' "$ctl"
grep -Fq '"status":"not_supported"' "$ctl"
grep -Fq '"secret_hashes_included":false' "$ctl"
grep -Fq '"contains_secret_material":false' "$ctl"
grep -Fq '"contains_secret_material_scope":"reconstructed_export_fields_only"' "$ctl"
grep -Fq '"field_projection":"closed_action_and_whitelisted_metadata_reconstruction"' "$ctl"
grep -Fq '"unknown_action_or_metadata":"omitted_and_marked_redacted_unverified"' "$ctl"
grep -Fq '"operator_subject_projection":"raw_value_omitted_provenance_unverified"' "$ctl"
if grep -Fq 'paper-raid-bff.account-export.v1' "$ctl" || \
   grep -Fq '"global_account_export_complete":true' "$ctl"; then
  echo "account export claims a legacy or global-complete scope" >&2
  exit 1
fi

account_export_source=$(sed -n '/async fn account_export/,/^async fn prune/p' "$ctl")
grep -Fq 'args.only(&["subject"])?' <<<"$account_export_source"
for field in account_id subject_id display_name nakama_user_id player_id state created_at activated_at suspended_at closed_at updated_at scopes author_roles credential_count credentials invites access_audit contains_secret_material contains_secret_material_scope; do
  grep -Fq "\"${field}\"" <<<"$account_export_source"
done
if grep -Eq 'reqwest::|https?://|HeptaClient|NakamaClient|CasClient' \
  <<<"$account_export_source"; then
  echo "BFF-local account export performs a cross-authority or network read" >&2
  exit 1
fi
grep -Fq 'SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY' <<<"$account_export_source"
grep -Fq 'operator_subject IS NOT NULL AS operator_subject_present' <<<"$account_export_source"
grep -Fq 'operator_lineage_status' <<<"$account_export_source"
grep -Fq 'let credential_count = credentials.len();' <<<"$account_export_source"
grep -Fq 'project_access_audit_row(&row)' <<<"$account_export_source"

audit_projection_source=$(sed -n '/fn project_access_audit_row/,/^fn account_export_boundary/p' "$ctl")
grep -Fq '"metadata_status".to_string(), json!("verified_projection")' <<<"$audit_projection_source"
grep -Fq '"metadata_status".to_string(), json!("redacted_unverified")' <<<"$audit_projection_source"
grep -Fq 'paper-raid-bff.accessctl.object-audit.v1' <<<"$audit_projection_source"
grep -Fq '"invite_issue" | "invite_reissue"' <<<"$audit_projection_source"
grep -Fq 'fn project_access_audit_action' <<<"$audit_projection_source"
grep -Fq 'json!(action.unwrap_or("redacted_unverified"))' <<<"$audit_projection_source"
grep -Fq '"operator_subject".to_string(), Value::Null' <<<"$audit_projection_source"
grep -Fq '"unverified_host_assertion_redacted"' <<<"$audit_projection_source"
if grep -Fq 'projected.insert("metadata".to_string(), raw_metadata)' <<<"$audit_projection_source"; then
  echo "account export passes raw audit metadata" >&2
  exit 1
fi

account_export_boundary=$(sed -n '/fn account_export_boundary/,/^async fn prune/p' "$ctl")
boundary_statuses=$(grep -oE '"status":"[^"]+"' <<<"$account_export_boundary" | LC_ALL=C sort -u)
if [[ "$boundary_statuses" != $'"status":"included"\n"status":"not_queried"\n"status":"not_supported"' ]]; then
  echo "account export boundary status enum drifted" >&2
  exit 1
fi
for component in bff_sessions bff_request_security_state bff_agent_bridge bff_product_telemetry bff_invite_batches bff_quota_and_retention_metadata bff_schema_metadata bff_operator_command_audit; do
  grep -A2 -F "\"${component}\":{" <<<"$account_export_boundary" | \
    grep -Fq '"status":"not_queried"'
done
for component in hepta nakama cas_reachability cas_bytes backups; do
  grep -A2 -F "\"${component}\":{" <<<"$account_export_boundary" | \
    grep -Fq '"status":"not_supported"'
done

grep -Fq 'BFF-local access-directory projection' "$readme"
grep -Fq 'global_account_export_complete=false' "$readme"
grep -Fq 'one statement snapshot' "$readme"
grep -Fq 'schema status helper is diagnostic only' "$readme"

grep -Fq 'paper-raid-bff.operator-command-audit.v1' "$ctl"
grep -Fq 'operator_command_attempt' "$ctl"
grep -Fq 'operator_command_result' "$ctl"
grep -Fq 'paper-raid-bff.accessctl.failure.v1' "$ctl"
for command_code in schema_migrate batch_create batch_pause batch_resume batch_revoke invite_issue invite_reissue invite_revoke credential_rotate account_suspend account_reactivate account_close account_export prune unsupported; do
  grep -Fq "\"${command_code}\"" "$ctl"
done
for reason_code in attempt_recorded command_committed command_rejected unsupported_command database_operation_failed identity_mode_rejected database_configuration_missing operator_configuration_missing operator_configuration_rejected retention_configuration_missing retention_configuration_rejected database_connect_unavailable database_migration_unavailable schema_activation_required command_not_run; do
  grep -Fq "\"${reason_code}\"" "$ctl"
done
for state in not_run not_committed committed unknown; do
  grep -Fq "\"${state}\"" "$ctl"
done
for audit_status in unavailable recorded failed unknown; do
  grep -Fq "\"${audit_status}\"" "$ctl"
done
grep -Fq 'AttemptWriteFailed' "$ctl"
grep -Fq 'ResultWriteFailed' "$ctl"
grep -Fq 'AttemptCommitUnknown' "$ctl"
grep -Fq 'ResultCommitUnknown' "$ctl"
grep -Fq 'CommandState::Unknown' "$ctl"
grep -Fq 'CommandReason::DatabaseOperationFailed' "$ctl"
grep -Fq 'AuditStatus::Unavailable' "$ctl"
grep -Fq 'AuditStatus::Failed' "$ctl"
grep -Fq 'OperatorAuditOutcome::Denied' "$ctl"
grep -Fq 'OperatorAuditOutcome::Indeterminate' "$ctl"
grep -Fq 'async fn audited_command_failure' "$ctl"
grep -Fq 'fn error_chain_contains_sqlx' "$ctl"
grep -Fq 'CommitAckUnknown' "$ctl"
grep -Fq 'DatabaseOperationNotCommitted' "$ctl"
grep -Fq 'env::args_os().skip(1)' "$ctl"
grep -Fq 'MAX_ARGUMENT_TOKENS' "$ctl"
grep -Fq 'MAX_OPTION_NAME_BYTES' "$ctl"
grep -Fq 'MAX_OPTION_VALUE_BYTES' "$ctl"
grep -Fq 'operator_command_codes_are_closed_and_unknown_input_is_not_echoed' "$ctl"
grep -Fq 'operator_command_audit_metadata_contains_only_bounded_fields' "$ctl"
grep -Fq 'operator_audit_ids_are_stable_and_event_scoped' "$ctl"
grep -Fq 'commit_ack_loss_preserves_unknown_audit_truth' "$ctl"
grep -Fq 'bounded_os_parser_prioritizes_unsupported_without_reading_parameters' "$ctl"
grep -Fq 'bounded_os_parser_rejects_oversized_known_command_inputs' "$ctl"
grep -Fq 'access_audit_metadata_is_reconstructed_only_for_known_schema_and_action' "$ctl"
grep -Fq 'audit_failure_disclosure_never_claims_command_success' "$ctl"
if grep -Fq 'paper-raid-accessctl: {error:#}' "$ctl"; then
  echo "accessctl prints unbounded exception details" >&2
  exit 1
fi

operator_audit_source=$(sed -n '/fn operator_audit_id/,/^async fn audited_command_failure/p' "$ctl")
grep -Fq 'paper-raid-bff.operator-command-audit-id.v1' <<<"$operator_audit_source"
grep -Fq 'ON CONFLICT (audit_id) DO NOTHING' <<<"$operator_audit_source"
grep -Fq 'OperatorAuditReadback::Exact' <<<"$operator_audit_source"
grep -Fq 'OperatorAuditReadback::Absent' <<<"$operator_audit_source"
grep -Fq 'OperatorAuditReadback::Mismatch' <<<"$operator_audit_source"
grep -Fq 'insert_operator_audit_tx(&mut tx, operator, result_record)' <<<"$operator_audit_source"
grep -Fq 'commit_with_operator_audit_readback(tx, pool, operator, result_record)' <<<"$operator_audit_source"
commit_truth_source=$(sed -n '/^async fn commit_with_operator_audit_readback/,/^async fn write_operator_audit/p' "$ctl")
grep -Fq 'Ok(OperatorAuditReadback::Exact) => Ok(())' <<<"$commit_truth_source"
grep -Fq 'Err(anyhow!(CommitAckUnknown))' <<<"$commit_truth_source"
if grep -Fq 'DatabaseOperationNotCommitted' <<<"$commit_truth_source"; then
  echo "post-COMMIT non-exact readback is mislabeled not_committed" >&2
  exit 1
fi
operator_metadata_source=$(awk '
  /^fn operator_command_audit_metadata/ { capture=1 }
  /^async fn audited_command_failure/ { exit }
  capture { print }
' "$ctl")
for key in schema attempt_id command_code command_state reason_code; do
  grep -Fq "\"${key}\"" <<<"$operator_metadata_source"
done
operator_metadata_keys=$(grep -oE '"[a-z_]+"[[:space:]]*:' <<<"$operator_metadata_source" | \
  sed -E 's/["[:space:]:]//g' | LC_ALL=C sort -u)
if [[ "$operator_metadata_keys" != $'attempt_id\ncommand_code\ncommand_state\nreason_code\nschema' ]]; then
  echo "operator command audit metadata key set drifted" >&2
  exit 1
fi
if grep -Eq 'args|raw|secret|login_credential|invite_id|batch_id|account_id|subject_id|path|error|details' \
  <<<"$operator_metadata_source"; then
  echo "operator command audit metadata captures parameters or unbounded details" >&2
  exit 1
fi

run_source=$(sed -n '/async fn run()/,/^async fn schema_migrate/p' "$ctl")
if grep -Fq 'db::migrate' <<<"$run_source"; then
  echo "regular accessctl command path performs automatic migration" >&2
  exit 1
fi
run_entry_source=$(sed -n '/async fn run()/,/^async fn reject_unsupported_command/p' "$ctl")
unsupported_line=$(grep -n 'return Err(reject_unsupported_command().await)' <<<"$run_entry_source" | cut -d: -f1)
database_config_line=$(grep -n 'required_env("PAPER_RAID_ACCESS_DATABASE_URL")' <<<"$run_entry_source" | head -1 | cut -d: -f1)
if [[ -z "$unsupported_line" || -z "$database_config_line" ]] || (( unsupported_line >= database_config_line )); then
  echo "unsupported command is not fixed before configuration/schema disclosure" >&2
  exit 1
fi
unsupported_source=$(sed -n '/^async fn reject_unsupported_command/,/^async fn schema_migrate/p' "$ctl")
grep -Fq 'CommandReason::UnsupportedCommand' <<<"$unsupported_source"
if grep -Fq 'CommandReason::DatabaseConfigurationMissing' <<<"$unsupported_source"; then
  echo "unsupported command leaks configuration reason" >&2
  exit 1
fi
schema_migrate_source=$(sed -n '/^async fn schema_migrate/,/^async fn batch_create/p' "$ctl")
grep -Fq 'db::migrate(pool)' <<<"$schema_migrate_source"
grep -Fq 'schema_ready_before' <<<"$schema_migrate_source"
grep -Fq 'attempt_audit_status' <<<"$schema_migrate_source"
grep -Fq 'result_audit_status' <<<"$schema_migrate_source"

business_source=$(sed -n '/^async fn batch_create/,/^async fn revoke_sessions/p' "$ctl")
if grep -Fq 'tx.commit().await' <<<"$business_source"; then
  echo "business handler bypasses atomic success-result finalizer" >&2
  exit 1
fi
finalizer_count=$(grep -Fc 'finalize_business_transaction(tx, pool, operator, context).await?' <<<"$business_source")
if (( finalizer_count != 9 )); then
  echo "not every accessctl business handler uses the atomic success-result finalizer" >&2
  exit 1
fi
grep -Fq '"invite_reissue"' <<<"$business_source"
grep -Fq "a.state = 'invited' AND i.state = 'issued'" <<<"$business_source"
grep -Fq '"batch_capacity_consumed":false' <<<"$business_source"

grep -Fq '### Host-local command audit' "$readme"
grep -Fq 'same database transaction as' "$readme"
grep -Fq 'database_operation_failed' "$readme"
grep -Fq 'audit_status=unavailable' "$readme"
grep -Fq 'audit_status=unknown' "$readme"
grep -Fq 'unverified_host_assertion_redacted' "$readme"
grep -Fq '`schema-migrate` is the only activation' "$readme"
grep -Fq '`invite-reissue` is the bounded recovery path' "$readme"
grep -Fq 'Operator-subject provenance is still an Alpha blocker' "$readme"

echo "invite-alpha static boundary: PASS"
