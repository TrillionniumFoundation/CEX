#!/usr/bin/env python3
"""Static governance checks for numbered CEX SQL migrations."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

MIGRATION_RE = re.compile(r"^(?P<number>\d{4})_[a-z0-9][a-z0-9._-]*\.sql$")
FUNCTION_RE = re.compile(
    r"create\s+or\s+replace\s+function\s+.*?(?=create\s+or\s+replace\s+function|\Z)",
    re.IGNORECASE | re.DOTALL,
)
DESTRUCTIVE_PATTERNS = {
    "drop table": re.compile(r"\bdrop\s+table\b", re.IGNORECASE),
    "truncate": re.compile(r"\btruncate\b", re.IGNORECASE),
    "delete": re.compile(r"\bdelete\s+from\b", re.IGNORECASE),
    "drop column": re.compile(r"\balter\s+table\b.*?\bdrop\s+column\b", re.IGNORECASE | re.DOTALL),
}


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    migration_dir = root / "migrations"
    errors: list[str] = []
    migrations: list[tuple[int, Path]] = []

    for path in sorted(migration_dir.glob("*.sql")):
        match = MIGRATION_RE.fullmatch(path.name)
        if not match:
            errors.append(f"unrecognized numbered migration filename: {path.name}")
            continue
        migrations.append((int(match.group("number")), path))

    if not migrations:
        errors.append("no numbered SQL migrations found")
    else:
        numbers = [number for number, _ in migrations]
        if numbers[0] != 1:
            errors.append(f"migration sequence must start at 0001, found {numbers[0]:04d}")
        duplicates = sorted({number for number in numbers if numbers.count(number) > 1})
        if duplicates:
            errors.append(
                "duplicate migration numbers: " + ", ".join(f"{number:04d}" for number in duplicates)
            )
        missing = sorted(set(range(numbers[0], numbers[-1] + 1)) - set(numbers))
        if missing:
            errors.append(
                "missing migration numbers: " + ", ".join(f"{number:04d}" for number in missing)
            )

    for number, path in migrations:
        content = path.read_text(encoding="utf-8")
        lowered = content.lower()
        stripped = content.strip()
        if not stripped:
            errors.append(f"{path.name}: migration is empty")
            continue
        if path.name != path.name.lower():
            errors.append(f"{path.name}: filename must be lowercase")
        if "\x00" in content:
            errors.append(f"{path.name}: contains a NUL byte")

        if number >= 55:
            if not stripped.lower().startswith("begin;"):
                errors.append(f"{path.name}: P0 migration must start with begin;")
            if not stripped.lower().endswith("commit;"):
                errors.append(f"{path.name}: P0 migration must end with commit;")

            allow_destructive = "migration-check: allow-destructive" in lowered
            if not allow_destructive:
                for label, pattern in DESTRUCTIVE_PATTERNS.items():
                    if pattern.search(content):
                        errors.append(
                            f"{path.name}: destructive operation '{label}' requires an explicit "
                            "-- migration-check: allow-destructive marker and reviewed rollback plan"
                        )

            for function in FUNCTION_RE.findall(content):
                if "set search_path" not in function.lower():
                    first_line = function.strip().splitlines()[0]
                    errors.append(
                        f"{path.name}: function lacks explicit SET search_path: {first_line}"
                    )

            if "security definer" in lowered and "set search_path" not in lowered:
                errors.append(
                    f"{path.name}: SECURITY DEFINER migration lacks explicit SET search_path"
                )

    if migrations:
        head = migrations[-1][1].name
        template_path = root / "docs/templates/cex-release-baseline-manifest-v1.json"
        try:
            template = json.loads(template_path.read_text(encoding="utf-8"))
            template_head = template["database"]["migration_head"]
            if template_head != head:
                errors.append(
                    f"release manifest template migration_head={template_head}, expected {head}"
                )
        except (OSError, json.JSONDecodeError, KeyError, TypeError) as error:
            errors.append(f"cannot validate release manifest migration head: {error}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(
        f"migration contract check passed: count={len(migrations)} "
        f"head={migrations[-1][1].name}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
