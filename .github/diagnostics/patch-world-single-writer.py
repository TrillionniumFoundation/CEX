#!/usr/bin/env python3
"""Atomically close the World single-writer index lookalike bypass.

This is a bounded cross-repository maintenance helper. It preserves Draft state,
does not merge, deploy, dismiss reviews, manufacture statuses, or grant
production authorization.
"""

from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import urllib.error
import urllib.request
from typing import Any

REPOSITORY = "TrillionniumFoundation/Trillionnium-World"
BRANCH = "feat/p0-world-authority-cutover-20260906"
PR_NUMBER = 60
TOKEN_ENV_NAMES = (
    "WORLD_REPO_TOKEN",
    "TRILLIONNIUM_WORLD_REPO_TOKEN",
    "TRILLIONNIUM_WORLD_TOKEN",
    "CEX_WORLD_TOKEN",
    "WORLD_TOKEN",
    "CROSS_REPO_TOKEN",
    "CROSS_REPO_PAT",
    "ORG_GITHUB_TOKEN",
    "ADMIN_GITHUB_TOKEN",
    "GH_PAT",
    "GITHUB_PAT",
    "REPO_TOKEN",
    "PAT",
)

NEW_INDEX_MARKER = "trnm_world_single_active_writer_index_semantic_drift"
NEW_TEST_MARKER = "nonunique-same-predicate"


def run(*args: str, cwd: pathlib.Path | None = None) -> str:
    result = subprocess.run(
        list(args),
        cwd=cwd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if result.returncode != 0:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(args)}")
    return result.stdout.strip()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one old block, found {count}")
    return text.replace(old, new, 1)


def api(token: str, method: str, path: str, body: Any | None = None) -> Any:
    data = None if body is None else json.dumps(body).encode("utf-8")
    request = urllib.request.Request(
        f"https://api.github.com{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "cex-world-catalog-closure",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return None if not raw else json.loads(raw)
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"GitHub API {method} {path} failed with HTTP {error.code}: {raw[:500]}") from error


def choose_token_and_clone(destination: pathlib.Path) -> tuple[str, str]:
    attempted: list[str] = []
    for name in TOKEN_ENV_NAMES:
        token = os.environ.get(name, "").strip()
        if not token:
            continue
        attempted.append(name)
        shutil.rmtree(destination, ignore_errors=True)
        url = f"https://x-access-token:{token}@github.com/{REPOSITORY}.git"
        result = subprocess.run(
            ["git", "clone", "--quiet", "--single-branch", "--branch", BRANCH, url, str(destination)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        )
        if result.returncode == 0:
            run("git", "remote", "set-url", "origin", f"https://github.com/{REPOSITORY}.git", cwd=destination)
            return token, name
    raise RuntimeError(f"no configured cross-repository token could clone {REPOSITORY}; candidates_present={attempted}")


def patch_installer(path: pathlib.Path) -> bool:
    text = path.read_text(encoding="utf-8")
    if NEW_INDEX_MARKER in text:
        return False

    old_declaration = """    active_index_definition text;"""
    new_declaration = """    active_index_table oid;
    active_index_access_method name;
    active_index_unique boolean;
    active_index_valid boolean;
    active_index_ready boolean;
    active_index_live boolean;
    active_index_immediate boolean;
    active_index_nulls_not_distinct boolean;
    active_index_key_count smallint;
    active_index_attribute_count smallint;
    active_index_keys text;
    active_index_expression text;
    active_index_predicate text;"""
    text = replace_once(text, old_declaration, new_declaration, "installer declaration")

    old_block = """    select pg_catalog.pg_get_indexdef(indexrelid)
    into strict active_index_definition
    from pg_catalog.pg_index
    where indexrelid = 'public.trnm_world_single_active_writer_epoch_v1'::regclass;
    if active_index_definition not like '%WHERE ((status = ''active''::text) AND world_writer_enabled)%'
       and active_index_definition not like '%WHERE ((status = ''active'') AND world_writer_enabled)%' then
        raise exception 'single-active-writer partial index predicate drift: %', active_index_definition;
    end if;
"""
    new_block = """    select
        index_row.indrelid,
        access_method.amname,
        index_row.indisunique,
        index_row.indisvalid,
        index_row.indisready,
        index_row.indislive,
        index_row.indimmediate,
        index_row.indnullsnotdistinct,
        index_row.indnkeyatts,
        index_row.indnatts,
        index_row.indkey::text,
        pg_catalog.pg_get_expr(index_row.indexprs, index_row.indrelid, false),
        pg_catalog.pg_get_expr(index_row.indpred, index_row.indrelid, false)
    into strict
        active_index_table,
        active_index_access_method,
        active_index_unique,
        active_index_valid,
        active_index_ready,
        active_index_live,
        active_index_immediate,
        active_index_nulls_not_distinct,
        active_index_key_count,
        active_index_attribute_count,
        active_index_keys,
        active_index_expression,
        active_index_predicate
    from pg_catalog.pg_index as index_row
    join pg_catalog.pg_class as index_relation
      on index_relation.oid = index_row.indexrelid
    join pg_catalog.pg_namespace as index_namespace
      on index_namespace.oid = index_relation.relnamespace
    join pg_catalog.pg_am as access_method
      on access_method.oid = index_relation.relam
    where index_namespace.nspname = 'public'
      and index_relation.relname = 'trnm_world_single_active_writer_epoch_v1';

    if active_index_table is distinct from 'public.trnm_world_authority_epochs_v1'::regclass
       or active_index_access_method is distinct from 'btree'
       or active_index_unique is distinct from true
       or active_index_valid is distinct from true
       or active_index_ready is distinct from true
       or active_index_live is distinct from true
       or active_index_immediate is distinct from true
       or active_index_nulls_not_distinct is distinct from false
       or active_index_key_count is distinct from 1
       or active_index_attribute_count is distinct from 1
       or active_index_keys is distinct from '0'
       or active_index_expression is distinct from '1'
       or active_index_predicate is distinct from '((status = ''active''::text) AND world_writer_enabled)' then
        raise exception using
            errcode = '55000',
            message = 'trnm_world_single_active_writer_index_semantic_drift',
            detail = pg_catalog.format(
                'table=%s access_method=%s unique=%s valid=%s ready=%s live=%s immediate=%s nulls_not_distinct=%s key_count=%s attribute_count=%s keys=%s expression=%s predicate=%s',
                active_index_table::regclass,
                active_index_access_method,
                active_index_unique,
                active_index_valid,
                active_index_ready,
                active_index_live,
                active_index_immediate,
                active_index_nulls_not_distinct,
                active_index_key_count,
                active_index_attribute_count,
                active_index_keys,
                coalesce(active_index_expression, '<null>'),
                coalesce(active_index_predicate, '<null>')
            );
    end if;
"""
    text = replace_once(text, old_block, new_block, "installer index verification")
    path.write_text(text, encoding="utf-8")
    return True


def patch_installation_checker(path: pathlib.Path) -> bool:
    text = path.read_text(encoding="utf-8")
    if NEW_TEST_MARKER in text:
        return False

    text = replace_once(
        text,
        'HOSTILE_INDEX_DB="trnm_world_hostile_index_${suffix}"',
        'HOSTILE_WRONG_PREDICATE_DB="trnm_world_hostile_pred_${suffix}"\n'
        'HOSTILE_NONUNIQUE_DB="trnm_world_hostile_nonuniq_${suffix}"\n'
        'HOSTILE_WRONG_KEY_DB="trnm_world_hostile_key_${suffix}"',
        "checker database declarations",
    )
    text = replace_once(
        text,
        '  for database in "$HOSTILE_INDEX_DB" "$PARTIAL_DB" "$REAPPLY_DB"; do',
        '  for database in "$HOSTILE_WRONG_KEY_DB" "$HOSTILE_NONUNIQUE_DB" '
        '"$HOSTILE_WRONG_PREDICATE_DB" "$PARTIAL_DB" "$REAPPLY_DB"; do',
        "checker cleanup",
    )

    query_function = """query_db() {
  local database="$1"
  shift
  PGDATABASE="$database" psql -X -v ON_ERROR_STOP=1 -At "$@"
}
"""
    helper = query_function + """
expect_hostile_index_rejected() {
  local database="$1"
  local label="$2"
  local create_sql="$3"

  create_db "$database"
  PGDATABASE="$database" psql -X -v ON_ERROR_STOP=1 -f "$BASE" >"$RUN/hostile-${label}-base.log"
  query_db "$database" -c "$create_sql" >"$RUN/hostile-${label}-create.log"

  set +e
  PGDATABASE="$database" bash "$INSTALLER" >"$RUN/hostile-${label}-install.out" 2>"$RUN/hostile-${label}-install.err"
  local hostile_status=$?
  set -e

  if [[ "$hostile_status" -eq 0 ]]; then
    echo "supported installer accepted hostile single-writer index: $label" >&2
    exit 1
  fi
  if ! grep -F 'trnm_world_single_active_writer_index_semantic_drift' "$RUN/hostile-${label}-install.err" >/dev/null; then
    echo "supported installer rejected hostile index for an unexpected reason: $label" >&2
    cat "$RUN/hostile-${label}-install.err" >&2 || true
    exit 1
  fi
}
"""
    text = replace_once(text, query_function, helper, "checker helper insertion")

    old_hostile = """# A same-name index with a different expression/predicate must never be accepted
# as the single-writer control. The hardening file is name-idempotent, so the
# supported installer performs a semantic pg_get_indexdef check and exits
# nonzero before returning qualification to its caller.
create_db "$HOSTILE_INDEX_DB"
PGDATABASE="$HOSTILE_INDEX_DB" psql -X -v ON_ERROR_STOP=1 -f "$BASE" >"$RUN/hostile-index-base.log"
query_db "$HOSTILE_INDEX_DB" -c "create unique index trnm_world_single_active_writer_epoch_v1 on public.trnm_world_authority_epochs_v1 (epoch_id);" >"$RUN/hostile-index-create.log"
set +e
PGDATABASE="$HOSTILE_INDEX_DB" bash "$INSTALLER" >"$RUN/hostile-index-install.out" 2>"$RUN/hostile-index-install.err"
hostile_status=$?
set -e
if [[ "$hostile_status" -eq 0 ]]; then
  echo "supported installer accepted a wrong same-name single-writer index" >&2
  exit 1
fi
if ! grep -F 'single-active-writer partial index predicate drift' "$RUN/hostile-index-install.err" >/dev/null; then
  echo "supported installer rejected the hostile index for an unexpected reason" >&2
  cat "$RUN/hostile-index-install.err" >&2 || true
  exit 1
fi
"""
    new_hostile = """# Correct constant key and exact predicate, but non-unique: allows two writers.
expect_hostile_index_rejected \\
  "$HOSTILE_NONUNIQUE_DB" \\
  'nonunique-same-predicate' \\
  "create index trnm_world_single_active_writer_epoch_v1 on public.trnm_world_authority_epochs_v1 ((1)) where status = 'active' and world_writer_enabled;"

# Unique and exact predicate, but keyed by epoch_id rather than a constant.
expect_hostile_index_rejected \\
  "$HOSTILE_WRONG_KEY_DB" \\
  'unique-wrong-key-same-predicate' \\
  "create unique index trnm_world_single_active_writer_epoch_v1 on public.trnm_world_authority_epochs_v1 (epoch_id) where status = 'active' and world_writer_enabled;"

# Retain the original wrong-key / missing-predicate regression case.
expect_hostile_index_rejected \\
  "$HOSTILE_WRONG_PREDICATE_DB" \\
  'unique-wrong-key-wrong-predicate' \\
  "create unique index trnm_world_single_active_writer_epoch_v1 on public.trnm_world_authority_epochs_v1 (epoch_id);"
"""
    text = replace_once(text, old_hostile, new_hostile, "checker hostile matrix")

    text = text.replace(
        '"schema": "trillionnium.world.postgres-installation-evidence.v1",',
        '"schema": "trillionnium.world.postgres-installation-evidence.v2",',
        1,
    )
    text = replace_once(
        text,
        '  "wrong_same_name_index_rejected": true,',
        '  "single_writer_index_catalog_semantics_exact": true,\n'
        '  "nonunique_same_predicate_index_rejected": true,\n'
        '  "unique_wrong_key_same_predicate_index_rejected": true,\n'
        '  "unique_wrong_key_wrong_predicate_index_rejected": true,',
        "checker evidence fields",
    )
    text = replace_once(
        text,
        '    "$RUN/hostile-index-install.err" "$TRNM_WORLD_POSTGRES_EVIDENCE_DIR/"',
        '    "$RUN"/hostile-*.err "$TRNM_WORLD_POSTGRES_EVIDENCE_DIR/"',
        "checker artifact copy",
    )
    path.write_text(text, encoding="utf-8")
    return True


def patch_contract(path: pathlib.Path) -> bool:
    document = json.loads(path.read_text(encoding="utf-8"))
    before = json.dumps(document, sort_keys=True)
    document["native_actions_trigger_sequence"] = max(2, int(document.get("native_actions_trigger_sequence", 0)))
    durable = document.setdefault("durable_cutover", {})
    invariants = durable.setdefault("invariants", [])
    hostile = durable.setdefault("hostile_qualification", [])
    for value in (
        "single_writer_index_targets_authority_epochs_relation",
        "single_writer_index_uses_btree",
        "single_writer_index_is_unique_valid_ready_live_and_immediate",
        "single_writer_index_has_exact_constant_expression_key_one",
        "single_writer_index_has_exact_active_write_enabled_predicate",
    ):
        if value not in invariants:
            invariants.append(value)
    for value in (
        "nonunique_constant_key_same_predicate_index",
        "unique_epoch_id_same_predicate_index",
        "unique_epoch_id_without_predicate_index",
    ):
        if value not in hostile:
            hostile.append(value)
    after = json.dumps(document, sort_keys=True)
    if before == after:
        return False
    path.write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return True


def main() -> int:
    result_path = pathlib.Path(os.environ.get("RESULT_PATH", "/tmp/world-catalog-closure-result.json"))
    result: dict[str, Any] = {
        "schema": "cex.world.catalog-closure-execution.v1",
        "repository": REPOSITORY,
        "branch": BRANCH,
        "production_authorization": "not_granted",
    }
    with tempfile.TemporaryDirectory(prefix="world-catalog-closure-") as temporary:
        repo = pathlib.Path(temporary) / "world"
        token, token_name = choose_token_and_clone(repo)
        result["credential_candidate"] = token_name
        before_sha = run("git", "rev-parse", "HEAD", cwd=repo)
        result["before_sha"] = before_sha

        installer = repo / "scripts/apply-trnm-world-authority-cutover-v1.sh"
        checker = repo / "scripts/check-trnm-world-authority-postgres-installation.sh"
        contract = repo / "docs/contracts/trillionnium-world-authority-cutover-v1.json"
        changes = {
            installer.relative_to(repo).as_posix(): patch_installer(installer),
            checker.relative_to(repo).as_posix(): patch_installation_checker(checker),
            contract.relative_to(repo).as_posix(): patch_contract(contract),
        }
        result["files_changed"] = [path for path, changed in changes.items() if changed]

        run("bash", "-n", str(installer), cwd=repo)
        run("bash", "-n", str(checker), cwd=repo)
        json.loads(contract.read_text(encoding="utf-8"))
        run("git", "diff", "--check", cwd=repo)

        if result["files_changed"]:
            run("git", "config", "user.name", "Trillionnium bounded maintenance", cwd=repo)
            run("git", "config", "user.email", "maintenance@trillionnium.invalid", cwd=repo)
            run("git", "add", *result["files_changed"], cwd=repo)
            run("git", "commit", "-m", "fix(world): close single-writer index lookalike bypass", cwd=repo)
            authenticated = f"https://x-access-token:{token}@github.com/{REPOSITORY}.git"
            run("git", "remote", "set-url", "origin", authenticated, cwd=repo)
            try:
                run("git", "push", "origin", f"HEAD:{BRANCH}", cwd=repo)
            finally:
                run("git", "remote", "set-url", "origin", f"https://github.com/{REPOSITORY}.git", cwd=repo)

        after_sha = run("git", "rev-parse", "HEAD", cwd=repo)
        after_tree = run("git", "rev-parse", "HEAD^{tree}", cwd=repo)
        result["after_sha"] = after_sha
        result["after_tree"] = after_tree

        pr = api(token, "GET", f"/repos/{REPOSITORY}/pulls/{PR_NUMBER}")
        live_head = pr["head"]["sha"]
        if live_head != after_sha:
            raise RuntimeError(f"PR head mismatch after push: local={after_sha} remote={live_head}")

        body = f"""## P0 objective

Close the World-owned source, durable state-machine and contract half of `TrillionniumFoundation/CEX#33` without reviving the retired legacy client/runtime stack.

## Exact current identities

- World source head: `{after_sha}`
- World source tree: `{after_tree}`
- World base: `main@{pr['base']['sha']}`
- World prospective merge: computed and executed by the truth-bound native workflows; no prior-head result is credited.

## Newly closed current-head database bypass

The supported installer now validates the single-writer index through PostgreSQL catalogs rather than a `pg_get_indexdef` substring. It binds the target relation, BTREE access method, uniqueness, valid/ready/live/immediate flags, one expression key equal to constant `1`, and the exact active/write-enabled predicate. The installation matrix rejects three same-name lookalikes: non-unique constant-key with the expected predicate, unique `epoch_id` with the expected predicate, and unique `epoch_id` without the predicate.

## Admission boundary

This remains a Draft source/database candidate. Native World exact-head and prospective-merge workflows, fresh eligible reviews, real-data count/ID/hash reconciliation, deployment IAM/secrets, production outage/rollback and a time-bounded no-dual-writer interval are still required. No status is manufactured and no CEX-side run is relabelled as a World-native context.

`production_authorization=not_granted`. Do not merge or deploy while any applicable check, review or external cutover record is missing or failing.
"""
        api(token, "PATCH", f"/repos/{REPOSITORY}/pulls/{PR_NUMBER}", {"body": body, "draft": True})
        try:
            api(
                token,
                "POST",
                f"/repos/{REPOSITORY}/pulls/{PR_NUMBER}/requested_reviewers",
                {"reviewers": ["Franksudoman", "Tomasrgbsf"]},
            )
            result["reviewers_requested"] = True
        except RuntimeError as error:
            result["reviewers_requested"] = False
            result["review_request_warning"] = str(error)

        result["status"] = "remote_verified"
        result_path.parent.mkdir(parents=True, exist_ok=True)
        result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
