#!/usr/bin/env python3
"""Idempotent, fail-closed Sequence 54 known-blocker repair for an external worktree.

This helper lives only on the temporary staging branch. It never pushes, merges,
approves, changes governance, or grants production authority. The caller must run
all qualification checks and perform a compare-and-swap fast-forward publication.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


class RepairError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RepairError(message)


def write_if_changed(path: Path, value: str, changed: list[str], root: Path) -> None:
    current = path.read_text(encoding="utf-8")
    if current != value:
        path.write_text(value, encoding="utf-8")
        changed.append(path.relative_to(root).as_posix())


def repair_merge_checkout(root: Path, changed: list[str]) -> None:
    path = root / ".github/workflows/p0-rust-toolchain-convergence.yml"
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(
        r"(?ms)(^\s+- name: Checkout GitHub prospective merge object\n"
        r".*?^\s+path: \.candidate-identity/merge\n"
        r"^\s+fetch-depth: )1(\s*$)"
    )
    updated, count = pattern.subn(r"\g<1>2\2", text, count=1)
    if count == 0:
        require(
            re.search(
                r"(?ms)^\s+- name: Checkout GitHub prospective merge object\n"
                r".*?^\s+path: \.candidate-identity/merge\n"
                r"^\s+fetch-depth: 2\s*$",
                text,
            )
            is not None,
            "prospective-merge checkout depth cannot be located",
        )
    write_if_changed(path, updated, changed, root)


def repair_toolchain_checker(root: Path, changed: list[str]) -> None:
    path = root / "scripts/rust_toolchain_convergence_v4.py"
    text = path.read_text(encoding="utf-8")

    if "VERSION_IDENTITY_SELECTOR_KINDS = frozenset(" not in text:
        anchor = (
            "@dataclass(frozen=True)\n"
            "class Selector:\n"
            "    kind: str\n"
            "    identity: str | None\n"
            "    line: str\n\n\n"
            "def require(condition: bool, message: str) -> None:\n"
        )
        helper = (
            "@dataclass(frozen=True)\n"
            "class Selector:\n"
            "    kind: str\n"
            "    identity: str | None\n"
            "    line: str\n\n\n"
            "VERSION_IDENTITY_SELECTOR_KINDS = frozenset({\n"
            "    \"rustup_toolchain\",\n"
            "    \"rustup_default\",\n"
            "    \"cargo_plus\",\n"
            "    \"toolchain_assignment\",\n"
            "    \"json_toolchain_assignment\",\n"
            "    \"yaml_toolchain\",\n"
            "    \"toml_toolchain\",\n"
            "    \"rust_version_environment\",\n"
            "})\n"
            "CONTAINER_IDENTITY_SELECTOR_KINDS = frozenset({\n"
            "    \"container_base\",\n"
            "    \"container_heredoc\",\n"
            "})\n\n\n"
            "def selector_binds_expected(selector: Selector, expected: str) -> bool:\n"
            "    identity = (selector.identity or \"\").strip()\n"
            "    if selector.kind in VERSION_IDENTITY_SELECTOR_KINDS:\n"
            "        return identity == expected\n"
            "    if selector.kind in CONTAINER_IDENTITY_SELECTOR_KINDS:\n"
            "        return re.search(\n"
            "            rf\"(?i)(?<![A-Za-z0-9_.-])rust:{re.escape(expected)}(?:[-@\\\\s]|$)\",\n"
            "            identity,\n"
            "        ) is not None\n"
            "    return False\n\n\n"
            "def active_source_contains(text: str, needle: str) -> bool:\n"
            "    \"\"\"Return true only when a marker occurs outside a source comment.\"\"\"\n"
            "    for raw_line in text.splitlines():\n"
            "        stripped = raw_line.lstrip()\n"
            "        if not stripped or stripped.startswith((\"#\", \"//\", \"/*\", \"*\", \"<!--\")):\n"
            "            continue\n"
            "        code = raw_line\n"
            "        for marker in (\" #\", \"\\\\t#\", \" //\"):\n"
            "            offset = code.find(marker)\n"
            "            if offset >= 0:\n"
            "                code = code[:offset]\n"
            "        if needle in code:\n"
            "            return True\n"
            "    return False\n\n\n"
            "def require(condition: bool, message: str) -> None:\n"
        )
        require(text.count(anchor) == 1, "Selector helper anchor drifted")
        text = text.replace(anchor, helper, 1)

    old = (
        "    if selectors and expected not in text:\n"
        "        problems.append(f\"Rust-selecting carrier does not bind {expected}: {path}\")\n"
    )
    new = (
        "    version_selectors = [\n"
        "        selector\n"
        "        for selector in selectors\n"
        "        if selector.kind in VERSION_IDENTITY_SELECTOR_KINDS\n"
        "        or selector.kind in CONTAINER_IDENTITY_SELECTOR_KINDS\n"
        "    ]\n"
        "    for selector in version_selectors:\n"
        "        if not selector_binds_expected(selector, expected):\n"
        "            problems.append(\n"
        "                f\"unapproved fixed Rust identity {selector.identity!r}: {path}\"\n"
        "            )\n"
        "    if selectors and not any(\n"
        "        selector_binds_expected(selector, expected)\n"
        "        for selector in version_selectors\n"
        "    ):\n"
        "        problems.append(f\"Rust-selecting carrier does not bind {expected}: {path}\")\n"
    )
    if old in text:
        text = text.replace(old, new, 1)
    require("unapproved fixed Rust identity" in text, "selector identity hardening absent")

    old = (
        "    if required_binding:\n"
        "        if expected not in text:\n"
        "            problems.append(f\"required toolchain binding omits Rust {expected}: {path}\")\n"
        "        if path not in {\"rust-toolchain\", \"rust-toolchain.toml\"} and release_commit not in text:\n"
        "            problems.append(\n"
        "                f\"required toolchain binding omits release commit {release_commit}: {path}\"\n"
        "            )\n"
        "    if observer_present(path, classification, text) and release_commit not in text:\n"
    )
    new = (
        "    release_commit_bound = active_source_contains(text, release_commit)\n"
        "    if required_binding:\n"
        "        if not any(\n"
        "            selector_binds_expected(selector, expected)\n"
        "            for selector in version_selectors\n"
        "        ):\n"
        "            problems.append(f\"required toolchain binding omits Rust {expected}: {path}\")\n"
        "        if (\n"
        "            path not in {\"rust-toolchain\", \"rust-toolchain.toml\"}\n"
        "            and not release_commit_bound\n"
        "        ):\n"
        "            problems.append(\n"
        "                f\"required toolchain binding omits release commit {release_commit}: {path}\"\n"
        "            )\n"
        "    if observer_present(path, classification, text) and not release_commit_bound:\n"
    )
    if old in text:
        text = text.replace(old, new, 1)
    require(
        "release_commit_bound = active_source_contains" in text,
        "release commit comment hardening absent",
    )

    labels = [
        "wrong_fixed_version_with_expected_comment",
        "conflicting_fixed_selectors",
        "required_binding_release_comment_only",
    ]
    if labels[0] not in text:
        anchor = '    unregistered_path = "services/example/toolchain.carrier"\n'
        require(text.count(anchor) == 1, "hostile fixture insertion anchor drifted")
        fixtures = '''    label = "wrong_fixed_version_with_expected_comment"\n    problems, selectors = analyse_text(\n        "scripts/wrong-fixed.sh",\n        f"# expected Rust {expected}\\nexport RUSTUP_TOOLCHAIN=1.97.0\\n",\n        classification="content_discovered",\n        expected=expected,\n        release_commit=release_commit,\n        obsolete=obsolete,\n        floating_channels=floating,\n        policy_definition=False,\n        required_binding=False,\n    )\n    require(bool(selectors), f"hostile fixture had no selector: {label}")\n    assert_rejected(label, problems)\n    executed.append(label)\n\n    label = "conflicting_fixed_selectors"\n    problems, selectors = analyse_text(\n        "scripts/conflicting-fixed.sh",\n        f"export RUSTUP_TOOLCHAIN={expected}\\ncargo +1.97.0 test\\n",\n        classification="content_discovered",\n        expected=expected,\n        release_commit=release_commit,\n        obsolete=obsolete,\n        floating_channels=floating,\n        policy_definition=False,\n        required_binding=False,\n    )\n    require(len(selectors) >= 2, f"hostile fixture lost a selector: {label}")\n    assert_rejected(label, problems)\n    executed.append(label)\n\n    label = "required_binding_release_comment_only"\n    problems, selectors = analyse_text(\n        "scripts/comment-only-release.sh",\n        f"export RUSTUP_TOOLCHAIN={expected}\\n# release commit {release_commit}\\n",\n        classification="content_discovered",\n        expected=expected,\n        release_commit=release_commit,\n        obsolete=obsolete,\n        floating_channels=floating,\n        policy_definition=False,\n        required_binding=True,\n    )\n    require(bool(selectors), f"hostile fixture had no selector: {label}")\n    assert_rejected(label, problems)\n    executed.append(label)\n\n'''
        text = text.replace(anchor, fixtures + anchor, 1)

    if "active_expected_selector_and_release_binding" not in text:
        anchor = "    return executed, accepted\n"
        require(text.count(anchor) == 1, "positive fixture insertion anchor drifted")
        fixture = '''    label = "active_expected_selector_and_release_binding"\n    problems, selectors = analyse_text(\n        "scripts/active-binding.sh",\n        (\n            f"export RUSTUP_TOOLCHAIN={expected}\\n"\n            f"export RUST_RELEASE_COMMIT={release_commit}\\n"\n            "rustc --version --verbose\\n"\n        ),\n        classification="content_discovered",\n        expected=expected,\n        release_commit=release_commit,\n        obsolete=obsolete,\n        floating_channels=floating,\n        policy_definition=False,\n        required_binding=True,\n    )\n    require(bool(selectors), f"positive fixture had no selector: {label}")\n    require(not problems, f"positive fixture was rejected: {label}: {problems}")\n    accepted.append(label)\n\n'''
        text = text.replace(anchor, fixture + anchor, 1)

    write_if_changed(path, text, changed, root)

    policy_path = root / "docs/security/rust-toolchain-surfaces-v1.json"
    policy = json.loads(policy_path.read_text(encoding="utf-8"))
    hostile = policy.get("hostile_fixtures")
    require(isinstance(hostile, list), "hostile_fixtures is not a list")
    hostile = [value for value in hostile if value not in labels]
    insertion = hostile.index("unregistered_binary_carrier") if "unregistered_binary_carrier" in hostile else len(hostile)
    hostile[insertion:insertion] = labels
    policy["hostile_fixtures"] = hostile
    write_if_changed(
        policy_path,
        json.dumps(policy, ensure_ascii=False, indent=2) + "\n",
        changed,
        root,
    )


def repair_projection_checker(root: Path, changed: list[str]) -> None:
    path = root / "scripts/check-consumer-projection-boundary.py"
    text = path.read_text(encoding="utf-8")
    old_import = "from rust_route_contract import RouteSyntaxError, extract_routes\n"
    new_import = (
        "from rust_route_contract import (\n"
        "    RouteSyntaxError,\n"
        "    decode_string,\n"
        "    extract_routes,\n"
        "    tokenize,\n"
        ")\n"
    )
    if old_import in text:
        text = text.replace(old_import, new_import, 1)
    require("decode_string" in text and "tokenize" in text, "projection lexical imports absent")

    if "def source_string_literals(" not in text:
        anchor = "\n\ndef projection_files() -> list[Path]:\n"
        require(text.count(anchor) == 1, "projection helper anchor drifted")
        helpers = '''\nSQL_LITERAL_PREFIX_RE = re.compile(\n    r"(?is)^\\s*(?:(?:--[^\\n]*\\n|/\\*.*?\\*/)*\\s*)"\n    r"(?:with|select|insert|update|delete|truncate|create|alter|drop)\\b"\n)\nSQL_CALL_CONTEXT_RE = re.compile(\n    r"(?is)(?:sqlx::)?(?:query(?:_as|_scalar)?!?|execute|prepare)\\s*\\(\\s*$"\n)\nNETWORK_CALL_CONTEXT_RE = re.compile(\n    r"(?is)(?:"\n    r"\\.\\s*(?:get|post|put|patch|delete|request)\\s*\\(\\s*$|"\n    r"(?:Request|Uri|Url)::(?:get|post|put|patch|delete|parse)\\s*\\(\\s*$|"\n    r"\\b(?:url|uri|endpoint|base_url)\\s*[:=][^;\\n]{0,160}$|"\n    r"\\bformat!\\s*\\([^)]{0,160}$"\n    r")"\n)\n\n\ndef source_string_literals(text: str) -> list[tuple[str, int]]:\n    literals: list[tuple[str, int]] = []\n    for token in tokenize(text):\n        if token.kind in {"string", "raw_string"}:\n            literals.append((decode_string(token), token.offset))\n    return literals\n\n\ndef is_sql_literal(text: str, offset: int, value: str) -> bool:\n    context = text[max(0, offset - 256):offset]\n    return (\n        SQL_LITERAL_PREFIX_RE.search(value) is not None\n        or SQL_CALL_CONTEXT_RE.search(context) is not None\n    )\n\n\ndef is_network_endpoint_literal(text: str, offset: int) -> bool:\n    context = text[max(0, offset - 256):offset]\n    return NETWORK_CALL_CONTEXT_RE.search(context) is not None\n'''
        text = text.replace(anchor, helpers + anchor, 1)

    old_scan = '''    for match in DDL_RE.finditer(text):\n        problems.append(\n            f"{relative}:{source_line(text, match.start())}: runtime DDL is forbidden in projection code"\n        )\n\n    for match in MUTATION_RE.finditer(text):\n        table = match.group(1).lower()\n        if table.startswith(FORBIDDEN_AUTHORITY_PREFIXES):\n            problems.append(\n                f"{relative}:{source_line(text, match.start())}: direct mutation of authoritative table {table!r}"\n            )\n        elif not table.startswith(ALLOWED_MUTATION_PREFIXES):\n            problems.append(\n                f"{relative}:{source_line(text, match.start())}: mutation target {table!r} lacks an approved projection namespace"\n            )\n\n    try:\n        declarations = extract_routes(text)\n    except RouteSyntaxError as error:\n        raise AssertionError(f"projection route parsing failed in {relative}: {error}") from error\n'''
    new_scan = '''    try:\n        literals = source_string_literals(text)\n        declarations = extract_routes(text)\n    except RouteSyntaxError as error:\n        raise AssertionError(f"projection route parsing failed in {relative}: {error}") from error\n\n    for value, offset in literals:\n        if not is_sql_literal(text, offset, value):\n            continue\n        for match in DDL_RE.finditer(value):\n            problems.append(\n                f"{relative}:{source_line(text, offset)}: runtime DDL is forbidden in projection code"\n            )\n        for match in MUTATION_RE.finditer(value):\n            table = match.group(1).lower()\n            if table.startswith(FORBIDDEN_AUTHORITY_PREFIXES):\n                problems.append(\n                    f"{relative}:{source_line(text, offset)}: direct mutation of authoritative table {table!r}"\n                )\n            elif not table.startswith(ALLOWED_MUTATION_PREFIXES):\n                problems.append(\n                    f"{relative}:{source_line(text, offset)}: mutation target {table!r} lacks an approved projection namespace"\n                )\n'''
    if old_scan in text:
        text = text.replace(old_scan, new_scan, 1)
    require("literals = source_string_literals(text)" in text, "projection SQL hardening absent")

    old_endpoints = '''    for expression in FORBIDDEN_INTERNAL_ENDPOINTS:\n        for match in expression.finditer(text):\n            problems.append(\n                f"{relative}:{source_line(text, match.start())}: projection code calls an internal authoritative endpoint directly"\n            )\n'''
    new_endpoints = '''    for value, offset in literals:\n        if not is_network_endpoint_literal(text, offset):\n            continue\n        for expression in FORBIDDEN_INTERNAL_ENDPOINTS:\n            if expression.search(value):\n                problems.append(\n                    f"{relative}:{source_line(text, offset)}: projection code calls an internal authoritative endpoint directly"\n                )\n'''
    if old_endpoints in text:
        text = text.replace(old_endpoints, new_endpoints, 1)
    require("is_network_endpoint_literal(text, offset)" in text, "projection endpoint hardening absent")
    write_if_changed(path, text, changed, root)

    test_path = root / "scripts/test-consumer-route-contract.py"
    tests = test_path.read_text(encoding="utf-8")
    if "test_non_sql_prose_does_not_trigger_mutation_guard" not in tests:
        anchor = "    def test_invalid_rust_does_not_silently_skip(self):\n"
        require(tests.count(anchor) == 1, "projection test anchor drifted")
        additions = '''    def test_non_sql_prose_does_not_trigger_mutation_guard(self):\n        self.assertEqual(\n            self.check('let note = "update set that should remain prose";'),\n            [],\n        )\n\n    def test_narrative_internal_path_does_not_trigger_network_guard(self):\n        self.assertEqual(\n            self.check('let note = "never call /v1/ledger/settle directly";'),\n            [],\n        )\n\n    def test_direct_internal_endpoint_call_is_rejected(self):\n        self.assertTrue(\n            self.check('client.post("/v1/ledger/settle").send().await;')\n        )\n\n    def test_projection_sql_namespace_remains_allowed(self):\n        self.assertEqual(\n            self.check('sqlx::query("update league_matches set status = 1");'),\n            [],\n        )\n\n'''
        tests = tests.replace(anchor, additions + anchor, 1)
    write_if_changed(test_path, tests, changed, root)

    doc = root / "docs/modules/consumer-entry-api.md"
    value = doc.read_text(encoding="utf-8")
    marker = "must not silently expand the edge into a second authoritative monolith"
    if marker not in value:
        value = value.rstrip() + (
            "\n\nWorld/League projections must not silently expand the edge "
            "into a second authoritative monolith.\n"
        )
    write_if_changed(doc, value, changed, root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    require((root / ".git").exists(), "target root is not a Git worktree")
    changed: list[str] = []
    repair_merge_checkout(root, changed)
    repair_toolchain_checker(root, changed)
    repair_projection_checker(root, changed)
    report = {
        "schema": "cex.sequence54-known-blocker-repair.v4",
        "changed_files": sorted(changed),
        "production_authorization": "not_granted",
        "merge_authorization": "not_granted",
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RepairError, ValueError, json.JSONDecodeError) as error:
        print(f"Sequence 54 fallback repair FAILED: {error}")
        raise SystemExit(1)
