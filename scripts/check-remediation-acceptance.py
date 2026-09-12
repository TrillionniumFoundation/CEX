#!/usr/bin/env python3
"""Lint the remediation plan and two bounded documentation semantics.

This is not a technical-completeness, runtime, review or release attestation.
No command from the input is executed. Source plans cannot close work items.
"""
from __future__ import annotations

import argparse
import ast
import copy
import io
import json
from pathlib import Path, PurePosixPath
import re
import stat
import tempfile
import tomllib
import unittest
from unittest.mock import patch
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PLAN = "docs/remediation/acceptance-plan-v1.json"
RELAY = "docs/modules/matrix-bot-relay.md"
STATES = "docs/hepta-paper-raid-state-machines-v1.md"
TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
CANONICAL_CLI = "scripts/reconcile-matrix-adapter-result.py"
V3_CLI = "scripts/reconcile-matrix-adapter-result-v3.py"
MIGRATION = "0006_adapter_result_embedded_delivery_binding.sql"
FUNCTION = "cex_matrix_reconcile_adapter_result_v3"
CONTRACT = "docs/matrix-result-reconciliation-v3.md"
SCHEMA = "cex.remediation-acceptance-check.v1"
MAX_BYTES = 2 * 1024 * 1024
WORK_IDS = {*(f"P0-{i:02d}" for i in range(1, 5)),
            *(f"P1-{i:02d}" for i in range(1, 14)),
            *(f"P2-{i:02d}" for i in range(1, 4))}


class ContractError(ValueError):
    """A bounded, non-secret source-contract diagnostic."""


def need(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        need(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def reject_constant(_: str) -> None:
    raise ContractError("non-finite JSON number")


def decode(raw: str) -> Any:
    need(len(raw.encode("utf-8")) <= MAX_BYTES, "JSON exceeds byte budget")
    return json.loads(raw, object_pairs_hook=unique_object, parse_constant=reject_constant)


def file_path(root: Path, value: Any) -> Path:
    need(isinstance(value, str) and bool(value), "missing repository path")
    pure = PurePosixPath(value)
    need(not pure.is_absolute() and ".." not in pure.parts and "\\" not in value
         and pure.as_posix() == value, "noncanonical repository path")
    path = root
    for part in pure.parts:
        path = path / part
        need(not path.is_symlink(), "symlink in repository path")
    need(path.is_file() and stat.S_ISREG(path.stat().st_mode), "missing regular source file")
    return path


def read(root: Path, value: str) -> str:
    path = file_path(root, value)
    need(path.stat().st_size <= MAX_BYTES, "source exceeds byte budget")
    with path.open("rb") as stream:
        raw = stream.read(MAX_BYTES + 1)
    need(len(raw) <= MAX_BYTES, "source exceeds byte budget")
    return raw.decode("utf-8")


def obj(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    need(isinstance(value, dict) and set(value) == keys, f"invalid {label} fields")
    return value


def text(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip()) and len(value) <= 4096


def visible_markdown(raw: str) -> str:
    """Comments and fenced examples cannot supply operative declarations."""
    raw = re.sub(r"<!--.*?(?:-->|\Z)", "", raw, flags=re.S)
    lines: list[str] = []
    char, size = "", 0
    for line in raw.splitlines():
        match = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if match:
            run, tail = match.groups()
            if not char:
                char, size = run[0], len(run)
            elif run[0] == char and len(run) >= size and not tail.strip():
                char, size = "", 0
            continue
        if not char:
            lines.append(line)
    return "\n".join(lines)


def declaration(markdown: str, name: str, expected: str) -> None:
    matches = re.findall(r"^" + re.escape(name) + r": `([^`]+)`\s*$", markdown, re.M)
    need(matches == [expected], f"wrong or ambiguous {name}")


def table_row(markdown: str, header: list[str], key: str) -> list[str]:
    """Read the operative table cells; an unrelated correct token is no proof."""
    rows: list[list[str]] = []
    active = False
    for line in markdown.splitlines():
        if not line.strip().startswith("|"):
            active = False
            continue
        cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
        if cells == header:
            active = True
            continue
        if active and cells and cells[0] == key:
            need(len(cells) == len(header), "table row shape mismatch")
            rows.append(cells)
    need(len(rows) == 1, "missing or ambiguous operative table row")
    return rows[0]


def validate_semantics(root: Path) -> None:
    trigger = decode(read(root, TRIGGER))
    need(isinstance(trigger, dict)
         and trigger.get("matrix_operator_migration_head") == MIGRATION
         and trigger.get("matrix_operator_runtime_entrypoint") == FUNCTION,
         "candidate Matrix authority requires an explicit contract update")
    directory = root / "services/matrix-entry-adapter/operator-migrations"
    files = sorted(p.name for p in directory.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    need(bool(files) and files[-1] == MIGRATION, "operator migration head drift")
    file_path(root, directory.relative_to(root).as_posix() + "/" + MIGRATION)
    file_path(root, CONTRACT)
    file_path(root, V3_CLI)
    canonical = ast.parse(read(root, CANONICAL_CLI))
    assignments = [node.value for node in canonical.body if isinstance(node, ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == "V3_PATH" for t in node.targets)]
    need(len(assignments) == 1, "canonical CLI has no unique v3 path declaration")
    value = assignments[0]
    need(isinstance(value, ast.BinOp) and isinstance(value.op, ast.Div)
         and isinstance(value.left, ast.Name) and value.left.id == "ROOT"
         and isinstance(value.right, ast.Constant) and value.right.value == V3_CLI,
         "canonical CLI v3 path drift")
    relay = visible_markdown(read(root, RELAY))
    declaration(relay, "Operator migration head", MIGRATION)
    declaration(relay, "Runtime reconciliation function", FUNCTION)
    declaration(relay, "Recovery contract", CONTRACT)
    references = re.findall(r"The response-loss recovery contract is documented in\s+`([^`]+)`", relay)
    need(references == [CONTRACT], "operative relay recovery reference is stale or ambiguous")
    ranges = re.findall(r"operator migrations `0001`\s+through `(\d{4})`", relay)
    need(ranges == [MIGRATION[:4]], "operative relay migration range is stale or ambiguous")
    states = visible_markdown(read(root, STATES))
    row = table_row(states, ["State", "Authorized command", "Preconditions", "Durable outcome",
                            "Failure/recovery rule"], "reproducing")
    need(row[1] == "freeze author reproducibility-readiness evidence",
         "author lifecycle incorrectly grants independent reproduction")
    mapping = table_row(states, ["Stored phase", "Author projection", "Independent reproduction owner"],
                        "reproducing")
    need(mapping == ["reproducing", "reproduction_readiness", "Review Raid"],
         "storage/projection/independence mapping drift")


def validate(root: Path) -> dict[str, int]:
    plan = obj(decode(read(root, PLAN)), {
        "schema", "status", "baseline_commit", "baseline_tree", "production_authorization",
        "repository_qualification", "work_items", "modules"}, "plan")
    need(plan["schema"] == "cex.remediation-acceptance.v1"
         and plan["status"] == "proposed_execution_contract", "wrong plan identity")
    need(plan["production_authorization"] == "not_granted"
         and plan["repository_qualification"] == "not_claimed", "source plan cannot grant acceptance")
    for name in ("baseline_commit", "baseline_tree"):
        need(isinstance(plan[name], str) and re.fullmatch(r"[0-9a-f]{40}", plan[name]) is not None,
             "invalid baseline identity")
    cargo = tomllib.loads(read(root, "Cargo.toml"))
    members = cargo.get("workspace", {}).get("members")
    need(isinstance(members, list) and bool(members)
         and all(text(m) for m in members) and len(set(members)) == len(members), "invalid workspace list")
    catalog = decode(read(root, "docs/module-catalog-v1.json"))
    entries = catalog.get("modules") if isinstance(catalog, dict) else None
    need(isinstance(entries, list) and all(isinstance(e, dict) for e in entries), "invalid catalog")
    by_member = {entry.get("workspace_member"): entry for entry in entries}
    need(len(by_member) == len(entries) and set(by_member) == set(members), "catalog/workspace mismatch")
    modules = plan["modules"]
    need(isinstance(modules, list) and 0 < len(modules) <= 500, "invalid module list")
    seen: set[str] = set()
    cases: set[str] = set()
    for item in modules:
        item = obj(item, {"workspace_member", "package", "owner", "kind", "documentation", "source",
                          "review_status", "acceptance_cases"}, "module")
        member = item["workspace_member"]
        need(text(member) and member in by_member and member not in seen, "duplicate or unknown module")
        seen.add(member)
        original = by_member[member]
        for name in ("package", "owner", "kind", "documentation"):
            need(text(item[name]) and item[name] == original.get(name), f"module {name} drift")
        manifest = tomllib.loads(read(root, member + "/Cargo.toml"))
        need(manifest.get("package", {}).get("name") == item["package"], "package manifest drift")
        file_path(root, item["documentation"])
        file_path(root, item["source"])
        need(item["source"] in original.get("source_entrypoints", []), "source is not catalog-bound")
        need(item["review_status"] == "not_reviewed", "source cannot self-certify independent design review")
        examples = item["acceptance_cases"]
        need(isinstance(examples, list) and 3 <= len(examples) <= 100, "missing module acceptance themes")
        for case in examples:
            case = obj(case, {"id", "expected"}, "acceptance theme")
            need(text(case["id"]) and case["id"].startswith(item["package"] + ":")
                 and case["id"] not in cases and text(case["expected"]), "invalid or duplicate acceptance theme")
            cases.add(case["id"])
    need(seen == set(members), "missing workspace module acceptance mapping")
    work = plan["work_items"]
    need(isinstance(work, list) and len(work) == len(WORK_IDS), "missing full remediation scope")
    graph: dict[str, list[str]] = {}
    for item in work:
        item = obj(item, {"id", "priority", "owner", "scope", "depends_on", "title", "acceptance", "status"},
                   "work item")
        identity = item["id"]
        need(text(identity) and identity in WORK_IDS and identity not in graph, "invalid work identity")
        need(item["priority"] == identity[:2] and text(item["owner"]) and text(item["title"]), "invalid ownership")
        need(item["scope"] in {"repository", "administrative", "external", "mixed"}, "invalid work scope")
        need(item["status"] == "open", "a source work item cannot claim evidence-backed closure")
        dependencies = item["depends_on"]
        need(isinstance(dependencies, list) and all(text(d) and d in WORK_IDS for d in dependencies)
             and len(dependencies) == len(set(dependencies)), "unknown or duplicate dependency")
        acceptance = item["acceptance"]
        need(isinstance(acceptance, list) and 2 <= len(acceptance) <= 20
             and all(text(a) for a in acceptance) and len(set(acceptance)) == len(acceptance),
             "missing acceptance criteria")
        graph[identity] = dependencies
    need(set(graph) == WORK_IDS, "remediation scope was removed")
    complete: set[str] = set()
    visiting: set[str] = set()
    def visit(key: str) -> None:
        need(key not in visiting, "work dependency cycle")
        if key in complete:
            return
        visiting.add(key)
        for child in graph[key]:
            visit(child)
        visiting.remove(key)
        complete.add(key)
    for key in graph:
        visit(key)
    validate_semantics(root)
    return {"workspace_modules": len(seen), "module_acceptance_themes": len(cases),
            "open_work_items": len(work), "unreviewed_module_designs": len(modules)}


def fixture(root: Path) -> dict[str, Any]:
    """Synthetic test inputs, never a copied release or independent approval."""
    def write(path: str, value: str) -> None:
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(value, encoding="utf-8")
    member = "crates/example"
    module = dict(workspace_member=member, package="example", owner="test-owner", kind="library",
                  documentation="docs/modules/example.md", source=member + "/src/lib.rs",
                  review_status="not_reviewed", acceptance_cases=[
                      {"id": f"example:{i}", "expected": f"synthetic expected behavior {i}"} for i in range(3)])
    tasks = [dict(id=k, priority=k[:2], owner="test-owner", scope="repository", depends_on=[],
                  title="Synthetic work item", acceptance=["Synthetic positive case", "Synthetic rejection case"],
                  status="open") for k in sorted(WORK_IDS)]
    plan = dict(schema="cex.remediation-acceptance.v1", status="proposed_execution_contract",
                baseline_commit="1" * 40, baseline_tree="2" * 40, production_authorization="not_granted",
                repository_qualification="not_claimed", modules=[module], work_items=tasks)
    write(PLAN, json.dumps(plan))
    write("Cargo.toml", '[workspace]\nmembers = ["crates/example"]\n')
    write(member + "/Cargo.toml", '[package]\nname = "example"\nversion = "0.1.0"\n')
    write(member + "/src/lib.rs", "// synthetic fixture\n")
    write(module["documentation"], "# Synthetic module\n")
    catalog_entry = {k: module[k] for k in ("workspace_member", "package", "owner", "kind", "documentation")}
    catalog_entry["source_entrypoints"] = [module["source"]]
    write("docs/module-catalog-v1.json", json.dumps({"modules": [catalog_entry]}))
    write(TRIGGER, json.dumps(dict(matrix_operator_migration_head=MIGRATION,
                                 matrix_operator_runtime_entrypoint=FUNCTION)))
    write("services/matrix-entry-adapter/operator-migrations/" + MIGRATION, "-- synthetic fixture\n")
    write(CONTRACT, "# Synthetic current contract\n")
    write(V3_CLI, "# synthetic fixture\n")
    write(CANONICAL_CLI, 'V3_PATH = ROOT / "' + V3_CLI + '"\n')
    write(RELAY, f"Operator migration head: `{MIGRATION}`\nRuntime reconciliation function: `{FUNCTION}`\n"
          f"Recovery contract: `{CONTRACT}`\nShared transport migrations `0001` through `0005` and operator migrations `0001`\n"
          f"through `0006` are owned here.\nThe response-loss recovery contract is documented in\n`{CONTRACT}`.\n")
    write(STATES, "| State | Authorized command | Preconditions | Durable outcome | Failure/recovery rule |\n"
          "|---|---|---|---|---|\n| `reproducing` | freeze author reproducibility-readiness evidence | frozen | record | hold |\n\n"
          "| Stored phase | Author projection | Independent reproduction owner |\n|---|---|---|\n"
          "| `reproducing` | `reproduction_readiness` | `Review Raid` |\n")
    return plan


def self_test() -> dict[str, int]:
    class Tests(unittest.TestCase):
        def setUp(self) -> None:
            self.tmp = tempfile.TemporaryDirectory()
            self.addCleanup(self.tmp.cleanup)
            self.root = Path(self.tmp.name)
            self.plan = fixture(self.root)

        def fails(self, mutate: Any) -> None:
            mutate(self.plan)
            (self.root / PLAN).write_text(json.dumps(self.plan), encoding="utf-8")
            with self.assertRaises((ContractError, ValueError, TypeError)):
                validate(self.root)

        def change(self, path: str, old: str, new: str) -> None:
            target = self.root / path
            target.write_text(target.read_text(encoding="utf-8").replace(old, new), encoding="utf-8")
            with self.assertRaises(ContractError):
                validate(self.root)

        def test_valid(self): self.assertEqual(validate(self.root)["open_work_items"], 20)
        def test_missing_module(self): self.fails(lambda p: p.update(modules=[]))
        def test_duplicate_module(self): self.fails(lambda p: p["modules"].append(copy.deepcopy(p["modules"][0])))
        def test_wrong_owner(self): self.fails(lambda p: p["modules"][0].update(owner="other"))
        def test_unknown_plan_field(self): self.fails(lambda p: p.update(approved=True))
        def test_production_grant(self): self.fails(lambda p: p.update(production_authorization="granted"))
        def test_qualification_claim(self): self.fails(lambda p: p.update(repository_qualification="qualified"))
        def test_source_review(self): self.fails(lambda p: p["modules"][0].update(review_status="approved"))
        def test_source_closure(self): self.fails(lambda p: p["work_items"][0].update(status="closed"))
        def test_missing_work(self): self.fails(lambda p: p["work_items"].pop())
        def test_unknown_dependency(self): self.fails(lambda p: p["work_items"][0].update(depends_on=["P3-99"]))
        def test_cycle(self): self.fails(lambda p: p["work_items"][0].update(depends_on=[p["work_items"][0]["id"]]))
        def test_duplicate_case(self): self.fails(lambda p: p["modules"][0]["acceptance_cases"].append(copy.deepcopy(p["modules"][0]["acceptance_cases"][0])))
        def test_missing_source(self): self.fails(lambda p: p["modules"][0].update(source="crates/example/src/missing.rs"))
        def test_path_escape(self):
            with self.assertRaises(ContractError): file_path(self.root, "../outside")
        def test_noncanonical_path(self):
            with self.assertRaises(ContractError): file_path(self.root, "crates//example/src/lib.rs")
        def test_symlink(self):
            # Synthetic path rejection is portable without Windows symlink privilege.
            with patch.object(Path, "is_symlink", return_value=True):
                with self.assertRaises(ContractError): validate(self.root)
        def test_duplicate_json(self):
            with self.assertRaises(ContractError): decode('{"x":1,"x":2}')
        def test_nonfinite_json(self):
            with self.assertRaises(ContractError): decode('{"x":NaN}')
        def test_json_budget(self):
            with self.assertRaises(ContractError): decode(' ' * (MAX_BYTES + 1))
        def test_stale_migration_declaration(self): self.change(RELAY, f"head: `{MIGRATION}`", "head: `0004_old.sql`")
        def test_stale_prose_with_correct_declaration(self): self.change(RELAY, f"documented in\n`{CONTRACT}`", "documented in\n`docs/matrix-result-reconciliation-v1.md`")
        def test_stale_range_with_correct_declaration(self): self.change(RELAY, "through `0006` are", "through `0004` are")
        def test_comment_cannot_supply_declaration(self): self.change(RELAY, f"Recovery contract: `{CONTRACT}`", f"<!-- Recovery contract: `{CONTRACT}` -->")
        def test_example_cannot_supply_declaration(self): self.change(RELAY, f"Recovery contract: `{CONTRACT}`", f"```text\nRecovery contract: `{CONTRACT}`\n```")
        def test_duplicate_declaration(self): self.change(RELAY, f"Recovery contract: `{CONTRACT}`", f"Recovery contract: `{CONTRACT}`\nRecovery contract: `{CONTRACT}`")
        def test_authority_row_with_correct_mapping(self): self.change(STATES, "freeze author reproducibility-readiness evidence", "submit independent reproduction report")
        def test_wrong_independence(self): self.change(STATES, "`Review Raid`", "`Author Raid`")
        def test_cli_comment_is_not_binding(self): self.change(CANONICAL_CLI, f'V3_PATH = ROOT / "{V3_CLI}"', f'# correct path: {V3_CLI}\nV3_PATH = ROOT / "scripts/legacy.py"')
        def test_package_name_drift(self): self.change("crates/example/Cargo.toml", 'name = "example"', 'name = "other"')
    result = unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
    need(result.wasSuccessful(), "remediation self-test failed")
    return {"tests_run": result.testsRun, "synthetic_fixtures_only": 1}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--contract-only", action="store_true")
    group.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    output: dict[str, Any] = dict(schema=SCHEMA, status="ok", production_authorization="not_granted",
        checker_may_grant_production_authorization=False, repository_qualification="not_claimed",
        coverage_result="structural_and_bounded_document_semantics_only", problems=[])
    try:
        output.update(self_test() if args.self_test else validate(ROOT))
    except (OSError, ValueError, TypeError, KeyError, RecursionError, SyntaxError) as error:
        output["status"] = "failed"
        output["problems"] = [str(error) if isinstance(error, ContractError) else "invalid or unavailable source contract"]
    print(json.dumps(output, ensure_ascii=False, sort_keys=True, indent=2))
    return 0 if output["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
