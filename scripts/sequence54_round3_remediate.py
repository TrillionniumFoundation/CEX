#!/usr/bin/env python3
"""Race-safe repository-internal Sequence 54 remediation applied by a temporary workflow."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path.cwd().resolve()


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def write(relative: str, content: str) -> None:
    (ROOT / relative).write_text(content, encoding="utf-8")


def replace_once(relative: str, old: str, new: str) -> None:
    content = read(relative)
    count = content.count(old)
    if count != 1:
        raise SystemExit(f"{relative}: expected one replacement, found {count}")
    write(relative, content.replace(old, new, 1))


# 1. Prospective merge parent inspection requires the merge commit and both parents.
workflow = ".github/workflows/p0-rust-toolchain-convergence.yml"
content = read(workflow)
merge_block = re.compile(
    r"(?P<prefix>      - name: Checkout GitHub prospective merge object\n"
    r"(?:.*\n){0,14}?"
    r"          path: \.candidate-identity/merge\n"
    r"          fetch-depth: )1(?P<suffix>\n)",
)
content, count = merge_block.subn(r"\g<prefix>2\g<suffix>", content, count=1)
if count != 1:
    raise SystemExit(f"{workflow}: prospective-merge fetch-depth repair did not match exactly once")
write(workflow, content)


# 2. Bind toolchain policy to parsed selector identities, not arbitrary file text.
checker = "scripts/rust_toolchain_convergence_v4.py"
content = read(checker)
helper_anchor = "\ndef analyse_text(\n"
if content.count(helper_anchor) != 1:
    raise SystemExit(f"{checker}: analyse_text anchor drift")
helpers = r'''

def strip_hash_comments(text: str) -> str:
    """Remove shell/YAML/TOML-style comments while preserving line numbers."""
    output: list[str] = []
    for line in text.splitlines(keepends=True):
        single = False
        double = False
        escaped = False
        cut = len(line)
        for index, character in enumerate(line):
            if escaped:
                escaped = False
                continue
            if character == "\\" and double:
                escaped = True
                continue
            if character == "'" and not double:
                single = not single
                continue
            if character == '"' and not single:
                double = not double
                continue
            if character == "#" and not single and not double:
                cut = index
                break
        body = line[:cut]
        if line.endswith("\n") and not body.endswith("\n"):
            body += "\n"
        output.append(body)
    return "".join(output)


def selector_rust_identity(selector: Selector) -> str | None:
    """Return the Rust identity selected by a parsed carrier."""
    if selector.kind == "toolchain_action":
        return None
    if selector.identity is None:
        return None
    value = selector.identity.strip().strip("'\"")
    if selector.kind in {"container_base", "container_heredoc"}:
        match = re.search(
            r"(?i)(?:^|[/[:space:]])rust:(\d+\.\d+\.\d+)(?:[-@/[:space:]]|$)",
            value,
        )
        if match is None:
            # Python's regular-expression engine does not implement POSIX
            # character classes. Keep an explicit fallback for ordinary image
            # references and fail closed for dynamic tags.
            match = re.search(
                r"(?i)(?:^|[/\s])rust:(\d+\.\d+\.\d+)(?:[-@/\s]|$)",
                value,
            )
        return match.group(1) if match else None
    return value


def selector_binding_self_tests(policy: dict[str, Any]) -> tuple[list[str], list[str]]:
    expected = str(policy["expected_rust_toolchain"])
    release_commit = str(policy["expected_rust_release_commit"])
    obsolete = list(policy["forbidden_active_versions"])
    floating = set(policy["floating_channels_forbidden"])
    rejected: list[str] = []
    accepted: list[str] = []

    hostile = [
        (
            "wrong_fixed_version_with_correct_comment",
            f"# expected Rust {expected}\nexport RUSTUP_TOOLCHAIN=1.97.0\ncargo test\n",
            False,
        ),
        (
            "conflicting_fixed_selectors",
            f"export RUSTUP_TOOLCHAIN={expected}\ncargo +1.97.0 test\n",
            False,
        ),
        (
            "required_release_commit_comment_only",
            f"export RUSTUP_TOOLCHAIN={expected}\n# release commit {release_commit}\nrustc --version --verbose\n",
            True,
        ),
    ]
    for label, source, required in hostile:
        problems, _selectors = analyse_text(
            "scripts/selector-binding-fixture.sh",
            source,
            classification="content_discovered",
            expected=expected,
            release_commit=release_commit,
            obsolete=obsolete,
            floating_channels=floating,
            policy_definition=False,
            required_binding=required,
        )
        require(bool(problems), f"selector-binding hostile fixture was accepted: {label}")
        rejected.append(label)

    positive_source = (
        f"export RUSTUP_TOOLCHAIN={expected}\n"
        f"RUST_RELEASE_COMMIT={release_commit}\n"
        "rustc --version --verbose\n"
    )
    problems, selectors = analyse_text(
        "scripts/selector-binding-positive.sh",
        positive_source,
        classification="content_discovered",
        expected=expected,
        release_commit=release_commit,
        obsolete=obsolete,
        floating_channels=floating,
        policy_definition=False,
        required_binding=True,
    )
    require(not problems and selectors, "selector-binding positive fixture was rejected")
    accepted.append("exact_selector_and_active_release_binding")
    return rejected, accepted
'''
content = content.replace(helper_anchor, helpers + helper_anchor, 1)
old_analysis = '''    problems: list[str] = []
    selectors = selectors_for(path, classification, text)
    selector_lines = [selector.line for selector in selectors]
    for version in obsolete:
        if any(version in line for line in selector_lines):
            problems.append(f"obsolete active Rust identity {version}: {path}")
    for selector in selectors:
        if selector.identity in floating_channels:
            problems.append(f"floating Rust channel in active carrier: {path}")
    if selectors and expected not in text:
        problems.append(f"Rust-selecting carrier does not bind {expected}: {path}")
    for match in ACTION_RE.finditer(text):
'''
new_analysis = '''    problems: list[str] = []
    active_text = strip_hash_comments(text)
    selectors = selectors_for(path, classification, active_text)
    version_selectors: list[tuple[Selector, str | None]] = []
    for selector in selectors:
        if selector.kind == "toolchain_action":
            continue
        identity = selector_rust_identity(selector)
        version_selectors.append((selector, identity))
        if identity is None:
            problems.append(f"unresolved Rust selector identity: {path}: {selector.kind}")
            continue
        if identity in obsolete:
            problems.append(f"obsolete active Rust identity {identity}: {path}")
        if identity in floating_channels:
            problems.append(f"floating Rust channel in active carrier: {path}: {identity}")
        elif identity != expected:
            problems.append(f"unapproved fixed Rust identity {identity}; expected {expected}: {path}")
    if selectors and not version_selectors:
        problems.append(f"Rust-selecting carrier has no parsed toolchain identity: {path}")
    for match in ACTION_RE.finditer(active_text):
'''
if content.count(old_analysis) != 1:
    raise SystemExit(f"{checker}: selector analysis block drift")
content = content.replace(old_analysis, new_analysis, 1)
old_required = '''    if required_binding:
        if expected not in text:
            problems.append(f"required toolchain binding omits Rust {expected}: {path}")
        if path not in {"rust-toolchain", "rust-toolchain.toml"} and release_commit not in text:
            problems.append(
                f"required toolchain binding omits release commit {release_commit}: {path}"
            )
    if observer_present(path, classification, text) and release_commit not in text:
'''
new_required = '''    if required_binding:
        if not any(identity == expected for _selector, identity in version_selectors):
            problems.append(f"required toolchain binding omits parsed Rust {expected}: {path}")
        if (
            path not in {"rust-toolchain", "rust-toolchain.toml"}
            and release_commit not in active_text
        ):
            problems.append(
                f"required toolchain binding omits active release commit {release_commit}: {path}"
            )
    if (
        observer_present(path, classification, active_text)
        and release_commit not in active_text
    ):
'''
if content.count(old_required) != 1:
    raise SystemExit(f"{checker}: required binding block drift")
content = content.replace(old_required, new_required, 1)
old_self_test = "        hostile, positive = hostile_self_tests(policy)\n"
new_self_test = (
    "        hostile, positive = hostile_self_tests(policy)\n"
    "        selector_hostile, selector_positive = selector_binding_self_tests(policy)\n"
    "        hostile.extend(selector_hostile)\n"
    "        positive.extend(selector_positive)\n"
)
if content.count(old_self_test) != 1:
    raise SystemExit(f"{checker}: hostile self-test call drift")
content = content.replace(old_self_test, new_self_test, 1)
write(checker, content)

policy_path = "docs/security/rust-toolchain-surfaces-v1.json"
policy = json.loads(read(policy_path))
for fixture in (
    "wrong_fixed_version_with_correct_comment",
    "conflicting_fixed_selectors",
    "required_release_commit_comment_only",
):
    if fixture not in policy["hostile_fixtures"]:
        policy["hostile_fixtures"].append(fixture)
write(policy_path, json.dumps(policy, indent=2, ensure_ascii=False) + "\n")


# 3. Parse Rust string literals before applying SQL/endpoint boundary checks.
projection = "scripts/check-consumer-projection-boundary.py"
content = read(projection)
content = content.replace(
    "from rust_route_contract import RouteSyntaxError, extract_routes\n",
    "from rust_route_contract import RouteSyntaxError, decode_string, extract_routes, tokenize\n",
    1,
)
old_regex = '''MUTATION_RE = re.compile(
    r"\\b(?:insert\\s+into|update|delete\\s+from|truncate(?:\\s+table)?)\\s+"
    r"(?:(?:public|[a-zA-Z_][a-zA-Z0-9_]*)\\.)?[\\\"`]?([a-zA-Z_][a-zA-Z0-9_]*)",
    re.IGNORECASE,
)
'''
new_regex = '''SQL_TABLE = (
    r'(?:(?:public|[a-zA-Z_][a-zA-Z0-9_]*)\\.)?'
    r'[\\\"`]?[a-zA-Z_][a-zA-Z0-9_]*[\\\"`]?'
)
MUTATION_RE = re.compile(
    rf"\\binsert\\s+into\\s+(?P<insert>{SQL_TABLE})"
    rf"|\\bupdate\\s+(?P<update>{SQL_TABLE})\\s+set\\b"
    rf"|\\bdelete\\s+from\\s+(?P<delete>{SQL_TABLE})"
    rf"|\\btruncate(?:\\s+table)?\\s+(?P<truncate>{SQL_TABLE})",
    re.IGNORECASE,
)
'''
if content.count(old_regex) != 1:
    raise SystemExit(f"{projection}: mutation regex drift")
content = content.replace(old_regex, new_regex, 1)
content = content.replace(
    "re.compile(r'/(?:v[0-9]+/)?(?:ledger|audit|executions?|providers?)(?:/|\\\")', re.IGNORECASE),\n"
    "re.compile(r'/(?:v[0-9]+/)?(?:ledger|audit|executions?|providers?)(?:/|$|[?#])', re.IGNORECASE),\n",
    1,
)
content = content.replace(
    "re.compile(r'/(?:v[0-9]+/)?(?:chain[-_]?finality|finality)(?:/|\\\")', re.IGNORECASE),\n"
    "re.compile(r'/(?:v[0-9]+/)?(?:chain[-_]?finality|finality)(?:/|$|[?#])', re.IGNORECASE),\n",
    1,
)
start = content.find("def check_file(path: Path) -> list[str]:\n")
end = content.find("\ndef check_contract_files() -> list[str]:\n")
if start < 0 or end < 0 or end <= start:
    raise SystemExit(f"{projection}: check_file function anchors drift")
new_check_file = r'''def check_file(path: Path) -> list[str]:
    text = regular_bytes(ROOT, path).decode("utf-8")
    relative = path.relative_to(ROOT).as_posix()
    problems: list[str] = []

    try:
        tokens = tokenize(text)
    except RouteSyntaxError as error:
        raise AssertionError(f"projection source parsing failed in {relative}: {error}") from error

    literals: list[tuple[str, int]] = []
    for token in tokens:
        if token.kind not in {"string", "raw_string"}:
            continue
        try:
            literals.append((decode_string(token), token.offset))
        except RouteSyntaxError as error:
            raise AssertionError(
                f"projection string parsing failed in {relative}: {error}"
            ) from error

    for literal, literal_offset in literals:
        def literal_line(match_offset: int) -> int:
            return source_line(text, literal_offset) + literal.count("\n", 0, match_offset)

        for match in DDL_RE.finditer(literal):
            problems.append(
                f"{relative}:{literal_line(match.start())}: runtime DDL is forbidden in projection code"
            )

        for match in MUTATION_RE.finditer(literal):
            raw_table = next(
                value
                for value in (
                    match.group("insert"),
                    match.group("update"),
                    match.group("delete"),
                    match.group("truncate"),
                )
                if value is not None
            )
            table = raw_table.rsplit(".", 1)[-1].strip('"`').lower()
            if table.startswith(FORBIDDEN_AUTHORITY_PREFIXES):
                problems.append(
                    f"{relative}:{literal_line(match.start())}: direct mutation of authoritative table {table!r}"
                )
            elif not table.startswith(ALLOWED_MUTATION_PREFIXES):
                problems.append(
                    f"{relative}:{literal_line(match.start())}: mutation target {table!r} lacks an approved projection namespace"
                )

        for expression in FORBIDDEN_LITERAL_PATTERNS:
            for match in expression.finditer(literal):
                problems.append(
                    f"{relative}:{literal_line(match.start())}: projection code declares an authoritative outcome"
                )

        for expression in FORBIDDEN_INTERNAL_ENDPOINTS:
            for match in expression.finditer(literal):
                prefix = literal[: match.start()]
                # A direct path, URL, or format-placeholder concatenation has no
                # prose before the forbidden path. Descriptive diagnostics and
                # documentation literals are not network calls.
                direct_reference = (
                    not prefix
                    or (
                        not any(character.isspace() for character in prefix)
                        and prefix.startswith(("/", "{", "http://", "https://"))
                    )
                )
                if direct_reference:
                    problems.append(
                        f"{relative}:{literal_line(match.start())}: projection code calls an internal authoritative endpoint directly"
                    )

    try:
        declarations = extract_routes(text)
    except RouteSyntaxError as error:
        raise AssertionError(f"projection route parsing failed in {relative}: {error}") from error
    for declaration in declarations:
        line = source_line(text, declaration.offset)
        if declaration.value is None:
            problems.append(f"{relative}:{line}: dynamic projection route path requires explicit reviewed resolution")
            continue
        route = declaration.value
        segments = {part.lower() for part in route.split("/") if part and not part.startswith(":")}
        forbidden = sorted(segments & FORBIDDEN_ROUTE_SEGMENTS)
        if forbidden:
            problems.append(f"{relative}:{line}: projection route {route!r} contains authoritative segment(s) {forbidden}")

    return problems
'''
content = content[:start] + new_check_file + content[end:]
write(projection, content)

projection_tests = "scripts/test-consumer-route-contract.py"
content = read(projection_tests)
test_anchor = "\n\nif __name__=='__main__':\n"
if content.count(test_anchor) != 1:
    raise SystemExit(f"{projection_tests}: test anchor drift")
extra_tests = r'''

    def test_rust_update_set_call_is_not_sql(self):
        self.assertEqual(self.check('let _ = state.update(set);'), [])

    def test_comment_and_descriptive_endpoint_are_not_calls(self):
        source = '// /v1/ledger/settle\nlet note = "documentation mentions /v1/ledger/ only";'
        self.assertEqual(self.check(source), [])

    def test_internal_endpoint_literal_is_rejected(self):
        self.assertTrue(self.check('let endpoint = "/v1/ledger/settle";'))

    def test_projection_namespace_mutation_remains_allowed(self):
        self.assertEqual(
            self.check('sqlx::query("update world_items set status = 1");'),
            [],
        )
'''
content = content.replace(test_anchor, extra_tests + test_anchor, 1)
write(projection_tests, content)

module_doc = "docs/modules/consumer-entry-api.md"
content = read(module_doc)
marker = "must not silently expand the edge into a second authoritative monolith"
if marker not in content:
    content = content.rstrip() + (
        "\n\n## Projection anti-monolith invariant\n\n"
        "World/League projection code must not silently expand the edge into a second "
        "authoritative monolith. It may maintain explicitly named projection namespaces, "
        "but authoritative Ledger, execution, provider, identity, audit, and finality state "
        "must remain behind their owning services and reviewed interfaces.\n"
    )
write(module_doc, content)

print("Sequence 54 round-3 repository-internal remediation applied")
