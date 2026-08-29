#!/usr/bin/env bash
set -euo pipefail

# Verify a Paper Raid PostgreSQL database can be restored into a genuinely new
# database. The caller owns the disposable PostgreSQL container; this helper
# never connects to a production host and never mutates the source database.

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"

container=""
database_user=""
source_database=""
restore_database=""
label="paper-raid"
summary_file=""
keep_restore_database="false"

usage() {
  cat <<'EOF'
Usage: scripts/check-paper-raid-fresh-restore.sh \
  --container <name> --user <database-user> --database <database> \
  [--label <label>] [--restore-db <database>] [--summary-file <path>] \
  [--keep-restore-db]

Creates a custom-format pg_dump from the named disposable PostgreSQL
container, restores it into a newly-created database, and requires schema and
exact per-table row-count parity. The source database is never dropped or
written. By default the fresh restore database is removed on exit.

The helper uses `sudo -n docker` by default, matching the Paper Raid gates.
Set PAPER_RAID_DOCKER_USE_SUDO=0 when the caller has direct Docker access.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --container)
      [[ $# -ge 2 ]] || { echo "--container requires a value" >&2; exit 2; }
      container="$2"
      shift 2
      ;;
    --user)
      [[ $# -ge 2 ]] || { echo "--user requires a value" >&2; exit 2; }
      database_user="$2"
      shift 2
      ;;
    --database)
      [[ $# -ge 2 ]] || { echo "--database requires a value" >&2; exit 2; }
      source_database="$2"
      shift 2
      ;;
    --restore-db)
      [[ $# -ge 2 ]] || { echo "--restore-db requires a value" >&2; exit 2; }
      restore_database="$2"
      shift 2
      ;;
    --label)
      [[ $# -ge 2 ]] || { echo "--label requires a value" >&2; exit 2; }
      label="$2"
      shift 2
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "--summary-file requires a value" >&2; exit 2; }
      summary_file="$2"
      shift 2
      ;;
    --keep-restore-db)
      keep_restore_database="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$container" ]] || { echo "--container is required" >&2; exit 2; }
[[ -n "$database_user" ]] || { echo "--user is required" >&2; exit 2; }
[[ -n "$source_database" ]] || { echo "--database is required" >&2; exit 2; }
[[ "$database_user" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "database user must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$source_database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "source database must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$label" =~ ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$ ]] || {
  echo "label must contain only bounded ASCII letters, digits, '_' or '-'" >&2
  exit 2
}

if [[ -z "$restore_database" ]]; then
  restore_database="${source_database}_restore_$(date -u +%Y%m%d%H%M%S)_$$"
fi
[[ "$restore_database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "restore database must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$restore_database" != "$source_database" ]] || {
  echo "refusing to use the source database as the restore target" >&2
  exit 2
}

if [[ -z "$summary_file" ]]; then
  summary_file="$REPO_ROOT/run/paper-raid-restore/${label}-$(date -u +%Y%m%dT%H%M%SZ)-$$.summary.json"
fi

if [[ "${PAPER_RAID_DOCKER_USE_SUDO:-1}" == "1" ]]; then
  docker_command=(sudo -n docker)
else
  docker_command=(docker)
fi

docker_exec() {
  "${docker_command[@]}" exec "$container" "$@"
}

psql_scalar() {
  local database="$1"
  local sql="$2"
  docker_exec psql \
    --username "$database_user" \
    --dbname "$database" \
    --no-psqlrc \
    --set ON_ERROR_STOP=1 \
    --tuples-only --no-align --command "$sql"
}

table_list() {
  psql_scalar "$1" \
    "SELECT quote_ident(n.nspname) || '.' || quote_ident(c.relname)
       FROM pg_class c
       JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p')
      ORDER BY 1"
}

row_count_manifest() {
  local database="$1"
  local output="$2"
  : >"$output"
  while IFS= read -r table; do
    [[ -n "$table" ]] || continue
    printf '%s=%s\n' "$table" "$(psql_scalar "$database" "SELECT count(*) FROM $table")" >>"$output"
  done < <(table_list "$database")
}

schema_dump() {
  local database="$1"
  local output="$2"
  local raw_output="${output}.raw"
  # PostgreSQL 17+ emits a random \\restrict token in plain dumps. Those two
  # guard lines carry no schema meaning and are removed before parity compare.
  docker_exec pg_dump \
    --username "$database_user" \
    --dbname "$database" \
    --schema-only \
    --no-owner \
    --no-privileges \
    --no-comments |
    sed -E '/^\\(un)?restrict[[:space:]]/d' >"$raw_output"

  # A dump/restore round trip reparses CHECK expressions. PostgreSQL may
  # flatten associative AND/OR groups introduced by BETWEEN, even though the
  # catalog expression is semantically identical. Canonicalize only boolean
  # grouping inside CHECK clauses; every other schema byte remains exact.
  python3 - "$raw_output" "$output" <<'PY'
import re
import sys
from pathlib import Path


def quoted_step(text: str, index: int) -> int | None:
    quote = text[index]
    if quote not in "'\"":
        return None
    cursor = index + 1
    while cursor < len(text):
        if text[cursor] == quote:
            if cursor + 1 < len(text) and text[cursor + 1] == quote:
                cursor += 2
                continue
            return cursor + 1
        cursor += 1
    raise ValueError("unterminated SQL quote in schema dump")


def dollar_step(text: str, index: int) -> int | None:
    if text[index] != "$":
        return None
    match = re.match(r"\$[A-Za-z_][A-Za-z0-9_]*\$|\$\$", text[index:])
    if match is None:
        return None
    delimiter = match.group(0)
    end = text.find(delimiter, index + len(delimiter))
    if end < 0:
        raise ValueError("unterminated SQL dollar quote in schema dump")
    return end + len(delimiter)


def matching_paren(text: str, start: int) -> int:
    depth = 0
    cursor = start
    while cursor < len(text):
        quoted = quoted_step(text, cursor)
        if quoted is not None:
            cursor = quoted
            continue
        dollar = dollar_step(text, cursor)
        if dollar is not None:
            cursor = dollar
            continue
        if text[cursor] == "(":
            depth += 1
        elif text[cursor] == ")":
            depth -= 1
            if depth == 0:
                return cursor
            if depth < 0:
                break
        cursor += 1
    raise ValueError("unbalanced CHECK expression in schema dump")


def strip_outer(text: str) -> str:
    text = text.strip()
    while text.startswith("(") and matching_paren(text, 0) == len(text) - 1:
        text = text[1:-1].strip()
    return text


def split_boolean(text: str, operator: str) -> list[str]:
    parts: list[str] = []
    depth = 0
    cursor = 0
    start = 0
    while cursor < len(text):
        quoted = quoted_step(text, cursor)
        if quoted is not None:
            cursor = quoted
            continue
        dollar = dollar_step(text, cursor)
        if dollar is not None:
            cursor = dollar
            continue
        if text[cursor] == "(":
            depth += 1
            cursor += 1
            continue
        if text[cursor] == ")":
            depth -= 1
            cursor += 1
            continue
        end = cursor + len(operator)
        if (
            depth == 0
            and text[cursor:end].upper() == operator
            and (cursor == 0 or not (text[cursor - 1].isalnum() or text[cursor - 1] == "_"))
            and (end == len(text) or not (text[end].isalnum() or text[end] == "_"))
        ):
            parts.append(text[start:cursor].strip())
            cursor = end
            start = cursor
            continue
        cursor += 1
    if not parts:
        return [text]
    parts.append(text[start:].strip())
    if any(not part for part in parts):
        raise ValueError("invalid boolean expression in schema dump")
    return parts


def normalize_boolean(text: str) -> str:
    text = strip_outer(text)
    for operator in ("OR", "AND"):
        parts = split_boolean(text, operator)
        if len(parts) > 1:
            normalized_parts: list[str] = []
            for part in parts:
                normalized = normalize_boolean(part)
                nested = split_boolean(strip_outer(normalized), operator)
                if len(nested) > 1:
                    normalized_parts.extend(normalize_boolean(item) for item in nested)
                else:
                    normalized_parts.append(normalized)
            return "(" + f" {operator} ".join(normalized_parts) + ")"
    return re.sub(r"[ \t\r\n]+", " ", text).strip()


def normalize_checks(source: str) -> str:
    output: list[str] = []
    cursor = 0
    matcher = re.compile(r"\bCHECK\s*\(", re.IGNORECASE)
    while True:
        match = matcher.search(source, cursor)
        if match is None:
            output.append(source[cursor:])
            break
        open_paren = source.find("(", match.start(), match.end())
        close_paren = matching_paren(source, open_paren)
        output.append(source[cursor : open_paren + 1])
        output.append(normalize_boolean(source[open_paren + 1 : close_paren]))
        cursor = close_paren
    return "".join(output)


raw_path, output_path = map(Path, sys.argv[1:])
normalized = normalize_checks(raw_path.read_text(encoding="utf-8"))
Path(output_path).write_text(normalized, encoding="utf-8")
raw_path.unlink()
PY
}

umask 077
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/paper-raid-restore.XXXXXXXX")"
dump_file="$work_dir/$label.dump"
source_counts="$work_dir/source.counts"
restore_counts="$work_dir/restore.counts"
source_schema="$work_dir/source.schema.sql"
restore_schema="$work_dir/restore.schema.sql"
restore_created="false"
started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cleanup() {
  local status=$?
  if [[ "$restore_created" == "true" && "$keep_restore_database" != "true" ]]; then
    docker_exec dropdb --username "$database_user" --if-exists "$restore_database" >/dev/null 2>&1 || true
  fi
  rm -rf "$work_dir"
  exit "$status"
}
trap cleanup EXIT

# Fail before producing a misleading summary if the caller points at a dead
# or non-PostgreSQL container.
docker_exec pg_isready --username "$database_user" --dbname "$source_database" >/dev/null

echo "==> Paper Raid fresh restore: dumping $source_database ($label)"
docker_exec pg_dump \
  --username "$database_user" \
  --dbname "$source_database" \
  --format=custom \
  --no-owner \
  --no-privileges >"$dump_file"
[[ -s "$dump_file" ]] || { echo "pg_dump produced an empty archive" >&2; exit 1; }

"${docker_command[@]}" exec -i "$container" pg_restore --list <"$dump_file" >/dev/null
row_count_manifest "$source_database" "$source_counts"
schema_dump "$source_database" "$source_schema"

echo "==> Paper Raid fresh restore: creating $restore_database"
docker_exec dropdb --username "$database_user" --if-exists "$restore_database" >/dev/null
docker_exec createdb --username "$database_user" "$restore_database"
restore_created="true"

echo "==> Paper Raid fresh restore: restoring into $restore_database"
"${docker_command[@]}" exec -i "$container" pg_restore \
  --username "$database_user" \
  --dbname "$restore_database" \
  --no-owner \
  --no-privileges \
  --exit-on-error \
  --single-transaction <"$dump_file"

row_count_manifest "$restore_database" "$restore_counts"
if ! diff -u "$source_counts" "$restore_counts"; then
  echo "fresh restore row-count parity failed" >&2
  exit 1
fi

schema_dump "$restore_database" "$restore_schema"
if ! cmp -s "$source_schema" "$restore_schema"; then
  echo "fresh restore schema parity failed" >&2
  diff -u "$source_schema" "$restore_schema" >&2 || true
  exit 1
fi

dump_sha256="$(sha256sum "$dump_file" | awk '{print $1}')"
source_schema_sha256="$(sha256sum "$source_schema" | awk '{print $1}')"
restore_schema_sha256="$(sha256sum "$restore_schema" | awk '{print $1}')"
dump_bytes="$(wc -c <"$dump_file" | tr -d '[:space:]')"
ended_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

python3 - "$summary_file" "$label" "$container" "$database_user" \
  "$source_database" "$restore_database" "$started_at" "$ended_at" \
  "$dump_file" "$dump_bytes" "$dump_sha256" "$source_schema_sha256" \
  "$restore_schema_sha256" "$keep_restore_database" "$source_counts" <<'PY'
import json
import sys
from pathlib import Path

(
    summary_path,
    label,
    container,
    database_user,
    source_database,
    restore_database,
    started_at,
    ended_at,
    dump_file,
    dump_bytes,
    dump_sha256,
    source_schema_sha256,
    restore_schema_sha256,
    keep_restore_database,
    counts_path,
) = sys.argv[1:]

counts = {}
for line in Path(counts_path).read_text(encoding="utf-8").splitlines():
    key, value = line.split("=", 1)
    counts[key] = int(value)

summary = {
    "schema": "trnm.paper-raid.fresh-restore.v1",
    "status": "passed",
    "kind": "paper_raid_postgres_fresh_restore",
    "label": label,
    "container": container,
    "database_user": database_user,
    "source_database": source_database,
    "restore_database": restore_database,
    "restore_database_kept": keep_restore_database == "true",
    "started_at": started_at,
    "ended_at": ended_at,
    "dump_file": dump_file,
    "dump_bytes": int(dump_bytes),
    "dump_sha256": f"sha256:{dump_sha256}",
    "source_schema_sha256": f"sha256:{source_schema_sha256}",
    "restore_schema_sha256": f"sha256:{restore_schema_sha256}",
    "schema_parity": source_schema_sha256 == restore_schema_sha256,
    "row_count_parity": True,
    "public_table_count": len(counts),
    "source_row_counts": counts,
    "boundary": "disposable PostgreSQL logical dump/restore; off-host backup durability is a separate release concern",
}

path = Path(summary_path)
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PAPER_RAID_FRESH_RESTORE_OK summary=$summary_file restore_db=$restore_database kept=$keep_restore_database"
