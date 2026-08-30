#!/usr/bin/env python3
"""Fail closed on mutable or parser-ambiguous GitHub Actions trust inputs."""

from __future__ import annotations

import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Iterable
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_GLOBS = (".github/workflows/*.yml", ".github/workflows/*.yaml")
AUTHORITATIVE_WORKFLOWS = {
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
    ".github/workflows/p0-release-candidate-gate.yml",
}
WRAPPER_MARKERS = {
    "scripts/check-p0-release-candidate-hygiene.py": (
        "check-p0-release-candidate-hygiene-core.py",
        "check-workflow-trust.py",
    ),
    "scripts/check-release-baseline-manifest.py": (
        "check-release-baseline-manifest-core.py",
        "check-release-baseline-manifest-contract.py",
    ),
    "scripts/p0-release-evidence.py": (
        "p0-release-evidence-core.py",
        "bind-p0-local-evidence.py",
        "check-hosted-gate-execution.py",
    ),
    "scripts/p0-release-evidence-strict.py": (
        "p0-release-evidence-core.py",
        "verify-hosted-run-execution.py",
        "repository-governance.json",
        "hosted-run-execution.json",
    ),
}
CORE_PATHS = {
    "scripts/check-p0-release-candidate-hygiene-core.py",
    "scripts/check-release-baseline-manifest-core.py",
    "scripts/p0-release-evidence-core.py",
}
PINNED_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
PINNED_DOCKER_RE = re.compile(r"^docker://[^\s@]+(?:[:][^\s@]+)?@sha256:[0-9a-f]{64}$")
REMOTE_ACTION_RE = re.compile(
    r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$"
)
BLOCK_USES_RE = re.compile(
    r'''^\s*(?:-\s*)?(?:uses|"uses"|'uses')\s*:\s*(?P<value>.*?)\s*$'''
)
FLOW_USES_RE = re.compile(
    # A block mapping starts with the `uses:` key at the beginning of a
    # physical line (possibly after a list dash); that is valid YAML and is
    # handled by BLOCK_USES_RE below.  Only a `{`/`,` delimiter proves that
    # the key is embedded in a flow-style mapping.  Including `^` here would
    # classify every ordinary block-form action as flow syntax.
    r'''[{,]\s*(?:uses|"uses"|'uses')\s*:'''
)
ANY_USES_KEY_RE = re.compile(r'''(?:uses|"uses"|'uses')\s*:''')
DOUBLE_QUOTED_SCALAR_RE = re.compile(r'"(?P<body>(?:\\.|[^"\\])*)"')
# This checker deliberately has no PyYAML dependency.  Any YAML syntax which
# can change a mapping key without being represented by the small lexical
# parser below is therefore rejected.  In particular, quoted keys may carry
# YAML escapes (``"us\\x65s"``), and tags/anchors/aliases or flow collections
# can hide an action/event key across physical lines.
EXPLICIT_KEY_RE = re.compile(r"^\s*(?:-\s*)?\?(?:\s|$)")
TAG_RE = re.compile(
    r"(?<![A-Za-z0-9_$])!(?:![A-Za-z0-9_.:/-]+|<[^\n>]+>|[A-Za-z0-9_.:/-]+)?"
    r"(?=$|[\s,}\]])"
)
ANCHOR_ALIAS_RE = re.compile(
    r"(?<![A-Za-z0-9_$])[&*][A-Za-z0-9_.-]*(?=$|[\s,}\]])"
)
FLOW_DELIMITER_RE = re.compile(r"[{}\[\]]")
BLOCK_SCALAR_RE = re.compile(
    r":\s*[|>](?:(?:[1-9][+-]?)|(?:[+-][1-9]?))?\s*$"
)
# A block scalar used directly as the workflow ``on`` value is equivalent to
# an event string after YAML folding.  It is outside the small structural
# parser's trusted subset and can hide a forbidden event across physical
# lines, so reject the construct rather than trying to infer its folded value.
ON_BLOCK_SCALAR_RE = re.compile(
    r"^\s*(?:on|[\"']on[\"'])\s*:\s*[|>]"
    r"(?:(?:[1-9][+-]?)|(?:[+-][1-9]?))?\s*$"
)
ON_QUOTED_VALUE_RE = re.compile(
    r"^\s*(?:on|[\"']on[\"'])\s*:\s*(?P<quote>[\"'])"
)
FORBIDDEN_EVENT_RE = re.compile(
    r"(?<![A-Za-z0-9_])(?:pull_request|pull_request_target)(?![A-Za-z0-9_])"
)
PROBLEMS: list[str] = []
VISITED_ACTIONS: set[Path] = set()


def relative(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT.resolve()).as_posix()
    except ValueError:
        return str(path)


def strip_yaml_comment(line: str) -> str:
    """Strip an unquoted YAML comment marker from one physical line."""

    in_single = False
    in_double = False
    escaped = False
    index = 0
    while index < len(line):
        char = line[index]
        if in_double:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_double = False
        elif in_single:
            if char == "'":
                if index + 1 < len(line) and line[index + 1] == "'":
                    index += 1
                else:
                    in_single = False
        else:
            if char == '"':
                in_double = True
            elif char == "'":
                in_single = True
            elif char == "#":
                return line[:index]
        index += 1
    return line


def unquoted_lines(text: str) -> Iterable[tuple[int, str]]:
    """Yield YAML lines which are structural rather than block-scalar data.

    Workflow ``run``/``shell`` blocks routinely contain shell, Python, and
    PowerShell punctuation that looks like YAML flow syntax.  The trust
    parser must inspect those blocks for neither action nor trigger keys.  A
    small indentation state machine is sufficient for the block-scalar forms
    used by GitHub workflows; malformed indentation is left visible and thus
    fails closed when it resembles ambiguous YAML.
    """

    scalar_indent: int | None = None
    for number, physical in enumerate(text.splitlines(), start=1):
        indentation = len(physical) - len(physical.lstrip(" "))
        if scalar_indent is not None:
            if not physical.strip() or indentation > scalar_indent:
                continue
            scalar_indent = None

        line = strip_yaml_comment(physical)
        yield number, line
        if BLOCK_SCALAR_RE.search(line.rstrip()):
            scalar_indent = indentation


def quoted_mapping_keys(line: str) -> Iterable[tuple[str, str, bool, bool]]:
    """Return quoted mapping-key tokens found on one structural line.

    The tuple is ``(quote, body, has_backslash, closed)``.  ``closed=False``
    is retained for a likely multiline key beginning at a mapping boundary;
    rejecting it prevents a continuation line from bypassing the lexical
    checks.  This is deliberately broader than YAML's exact grammar and may
    reject unusual but valid scalar text, which is the safe tradeoff here.
    """

    text = strip_yaml_comment(line)
    index = 0
    while index < len(text):
        quote = text[index]
        if quote not in {"'", '"'}:
            index += 1
            continue

        start = index
        index += 1
        body: list[str] = []
        has_backslash = False
        closed = False
        while index < len(text):
            char = text[index]
            if quote == '"':
                if char == "\\":
                    has_backslash = True
                    body.append(char)
                    index += 1
                    if index < len(text):
                        body.append(text[index])
                        index += 1
                    continue
                if char == '"':
                    index += 1
                    closed = True
                    break
                body.append(char)
                index += 1
                continue

            # YAML single-quoted scalars escape a quote by doubling it.
            if char == "'":
                if index + 1 < len(text) and text[index + 1] == "'":
                    body.extend(("'", "'"))
                    index += 2
                    continue
                index += 1
                closed = True
                break
            if char == "\\":
                has_backslash = True
            body.append(char)
            index += 1

        if closed:
            end = index
            while end < len(text) and text[end] in " \t":
                end += 1
            if end < len(text) and text[end] == ":":
                yield quote, "".join(body), has_backslash, True
            # Continue after the token; another quoted key may occur later in
            # a flow mapping on the same line.
            index = end
            continue

        # A quoted key may legally continue on the next physical line.  Only
        # treat an unterminated token as a key when it starts at a mapping
        # boundary, avoiding needless rejection of ordinary quoted values.
        prefix = text[:start].rstrip()
        if not prefix or prefix.endswith(("-", "?", "{", ",")):
            yield quote, "".join(body), has_backslash, False
        break


def decode_yaml_double_quoted(body: str) -> str | None:
    """Decode the YAML escapes needed to identify a forbidden event key."""

    simple = {
        "0": "\0",
        "a": "\a",
        "b": "\b",
        "t": "\t",
        "n": "\n",
        "v": "\v",
        "f": "\f",
        "r": "\r",
        "e": "\x1b",
        " ": " ",
        '"': '"',
        "/": "/",
        "\\": "\\",
        "N": "\u0085",
        "_": "\u00a0",
        "L": "\u2028",
        "P": "\u2029",
    }
    decoded: list[str] = []
    index = 0
    while index < len(body):
        char = body[index]
        if char != "\\":
            decoded.append(char)
            index += 1
            continue
        index += 1
        if index >= len(body):
            return None
        escaped = body[index]
        if escaped in simple:
            decoded.append(simple[escaped])
            index += 1
            continue
        width = {"x": 2, "u": 4, "U": 8}.get(escaped)
        if width is None or index + width >= len(body):
            return None
        digits = body[index + 1 : index + 1 + width]
        if not re.fullmatch(rf"[0-9A-Fa-f]{{{width}}}", digits):
            return None
        try:
            decoded.append(chr(int(digits, 16)))
        except ValueError:
            # Invalid Unicode scalar values are still parser-ambiguous; the
            # caller has already recorded the escaped-key rejection.
            return None
        index += width + 1
    return "".join(decoded)


def decode_quoted_key(quote: str, body: str) -> str | None:
    if quote == '"':
        return decode_yaml_double_quoted(body)
    return body.replace("''", "'")


def mask_yaml_quoted_and_expressions(line: str) -> str:
    """Mask quoted scalars and GitHub expressions before syntax checks."""

    text = strip_yaml_comment(line)
    masked = list(text)
    index = 0
    while index < len(text):
        if text.startswith("${{", index):
            end = text.find("}}", index + 3)
            stop = len(text) if end < 0 else end + 2
            for position in range(index, stop):
                masked[position] = " "
            index = stop
            continue
        if text[index] not in {"'", '"'}:
            index += 1
            continue

        quote = text[index]
        masked[index] = " "
        index += 1
        while index < len(text):
            char = text[index]
            masked[index] = " "
            if quote == '"' and char == "\\":
                index += 1
                if index < len(text):
                    masked[index] = " "
                    index += 1
                continue
            if char == quote:
                index += 1
                break
            if quote == "'" and index + 1 < len(text) and text[index + 1] == "'":
                masked[index + 1] = " "
                index += 2
                continue
            index += 1
    return "".join(masked)


def parser_ambiguities(line: str) -> list[str]:
    """Return conservative parser-ambiguity reasons for a YAML line."""

    issues: list[str] = []
    for quote, body, has_backslash, closed in quoted_mapping_keys(line):
        if has_backslash:
            issues.append(
                "escaped quoted mapping keys are forbidden; use plain YAML keys"
            )
            # One issue per line is enough even if a flow map has multiple
            # escaped keys.
            break
        if not closed:
            issues.append(
                "multiline quoted mapping keys are parser-ambiguous and forbidden"
            )
            break

    masked = mask_yaml_quoted_and_expressions(line)
    if EXPLICIT_KEY_RE.match(masked):
        issues.append("explicit YAML mapping keys are parser-ambiguous and forbidden")
    if TAG_RE.search(masked) or ANCHOR_ALIAS_RE.search(masked):
        issues.append("YAML tags, anchors, and aliases are parser-ambiguous and forbidden")
    if FLOW_DELIMITER_RE.search(masked):
        issues.append("flow-style YAML collections are parser-ambiguous and forbidden")
    return issues


def parse_scalar(raw: str) -> str:
    value = raw.strip()
    if not value:
        raise ValueError("uses value is empty")
    if value[0] in "|>&*!":
        raise ValueError("aliases, anchors and block/dynamic uses values are forbidden")
    if value.startswith("${{") or "${{" in value:
        raise ValueError("expression-derived uses values are forbidden")
    if value[0] == '"':
        try:
            decoded = json.loads(value)
        except json.JSONDecodeError as error:
            raise ValueError(f"invalid double-quoted uses value: {error}") from error
        if not isinstance(decoded, str):
            raise ValueError("double-quoted uses value must decode to a string")
        return decoded
    if value[0] == "'":
        if len(value) < 2 or value[-1] != "'":
            raise ValueError("unterminated single-quoted uses value")
        return value[1:-1].replace("''", "'")
    if any(char.isspace() for char in value):
        raise ValueError("unquoted uses value contains whitespace")
    return value


def resolve_local_use(use: str, *, require_exists: bool) -> Path | None:
    if not use.startswith("./"):
        return None
    if "\\" in use or "//" in use or "${{" in use:
        raise ValueError("local uses path contains a forbidden escape or expression")
    relative_raw = use[2:]
    if not relative_raw:
        raise ValueError("local uses path is empty")
    segments = relative_raw.split("/")
    if any(segment in {"", ".", ".."} for segment in segments):
        raise ValueError("local uses path contains an empty, dot or parent segment")
    if not re.fullmatch(r"[A-Za-z0-9._/-]+", relative_raw):
        raise ValueError("local uses path contains unsupported characters")

    repository_root = ROOT.resolve()
    target = (ROOT / relative_raw).resolve()
    try:
        target.relative_to(repository_root)
    except ValueError as error:
        raise ValueError("local uses path escapes the repository root") from error

    if not require_exists:
        return target

    if target.is_file():
        if (
            target.suffix in {".yml", ".yaml"}
            and target.parent == (ROOT / ".github/workflows").resolve()
        ):
            return None
        raise ValueError("local uses file must be a top-level reusable workflow")

    if not target.is_dir():
        raise ValueError(f"local uses target does not exist: {relative_raw}")

    descriptor_candidates = (target / "action.yml", target / "action.yaml")
    descriptors = [candidate for candidate in descriptor_candidates if candidate.is_file()]
    if len(descriptors) != 1:
        raise ValueError(
            "local action directory must contain exactly one action.yml or action.yaml"
        )

    return validate_local_action_descriptor(descriptors[0], repository_root)


def validate_local_action_descriptor(descriptor: Path, repository_root: Path) -> Path:
    """Validate and resolve a local action descriptor inside the checkout."""

    # A descriptor symlink is not part of the immutable workflow tree.  It can
    # point outside the checkout (or be swapped after this check), so fail
    # closed instead of recursively trusting its contents.
    if descriptor.is_symlink():
        raise ValueError("local action descriptor must not be a symlink")
    try:
        resolved_descriptor = descriptor.resolve()
    except (OSError, RuntimeError) as error:
        raise ValueError(f"cannot resolve local action descriptor: {error}") from error
    try:
        resolved_descriptor.relative_to(repository_root)
    except ValueError as error:
        raise ValueError("local action descriptor escapes the repository root") from error
    if not resolved_descriptor.is_file():
        raise ValueError("local action descriptor does not resolve to a regular file")
    return resolved_descriptor


def action_pin_problem(use: str, *, require_local_exists: bool = True) -> tuple[str | None, Path | None]:
    if use.startswith("./"):
        try:
            descriptor = resolve_local_use(use, require_exists=require_local_exists)
        except ValueError as error:
            return str(error), None
        return None, descriptor

    if use.startswith("docker://"):
        if PINNED_DOCKER_RE.fullmatch(use):
            return None, None
        return (
            "docker actions must use docker://<image>@sha256:<64 lowercase hex>",
            None,
        )

    location, separator, ref = use.rpartition("@")
    if (
        not location
        or not separator
        or not PINNED_SHA_RE.fullmatch(ref)
        or ref == "0" * 40
    ):
        return "external actions must use a 40-character lowercase commit SHA", None
    if not REMOTE_ACTION_RE.fullmatch(location):
        return "external action location has an invalid owner/repository/path shape", None
    if any(segment in {"", ".", ".."} for segment in location.split("/")):
        return "external action location contains an invalid path segment", None
    return None, None


def uses_entries(text: str, label: str) -> list[tuple[int, str]]:
    entries: list[tuple[int, str]] = []
    ambiguous = False
    for number, structural in unquoted_lines(text):
        line = structural.rstrip()
        if not line.strip():
            continue
        syntax_issues = parser_ambiguities(line)
        if syntax_issues:
            ambiguous = True
            for issue in syntax_issues:
                PROBLEMS.append(f"{label}:{number}: {issue}")
            continue
        if FLOW_USES_RE.search(line):
            ambiguous = True
            PROBLEMS.append(
                f"{label}:{number}: flow-style uses mappings are forbidden; use block syntax"
            )
            continue
        match = BLOCK_USES_RE.fullmatch(line)
        if match:
            try:
                value = parse_scalar(match.group("value"))
            except ValueError as error:
                PROBLEMS.append(f"{label}:{number}: {error}")
            else:
                entries.append((number, value))
            continue
        if ANY_USES_KEY_RE.search(line):
            ambiguous = True
            PROBLEMS.append(
                f"{label}:{number}: parser-ambiguous uses key is forbidden"
            )
    # Do not interpret any action reference from a document whose YAML shape
    # was parser-ambiguous.  The caller still receives all diagnostics above,
    # while local-action traversal cannot accidentally trust a partial parse.
    return [] if ambiguous else entries


def scan_action_document(path: Path) -> None:
    # Re-check at traversal time as well as during ``resolve_local_use``.  A
    # local descriptor can be replaced between those calls; following a newly
    # introduced symlink would make the trust result depend on an untracked
    # path outside this checkout.
    if path.is_symlink():
        PROBLEMS.append(f"{relative(path)}: local action descriptor must not be a symlink")
        return
    try:
        path = validate_local_action_descriptor(path, ROOT.resolve())
    except ValueError as error:
        PROBLEMS.append(f"{relative(path)}: {error}")
        return
    if path in VISITED_ACTIONS:
        return
    VISITED_ACTIONS.add(path)
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        PROBLEMS.append(f"{relative(path)}: cannot read local action descriptor: {error}")
        return
    scan_uses(text, relative(path))


def scan_uses(text: str, label: str) -> None:
    for number, use in uses_entries(text, label):
        problem, descriptor = action_pin_problem(use)
        if problem:
            PROBLEMS.append(f"{label}:{number}: mutable/unsafe action reference {use!r}: {problem}")
        elif descriptor is not None:
            scan_action_document(descriptor)


def validate_authoritative_events(path: Path, text: str) -> None:
    label = relative(path)
    for number, line in unquoted_lines(text):
        if ON_BLOCK_SCALAR_RE.search(line.rstrip()):
            PROBLEMS.append(
                f"{label}:{number}: block-scalar workflow event values are forbidden"
            )
        quoted_value = ON_QUOTED_VALUE_RE.match(line)
        if quoted_value:
            quote = quoted_value.group("quote")
            value = line[quoted_value.end() :]
            # A quote that is continued on the next physical line can decode
            # to a forbidden event while evading the one-line scalar decoder.
            # Reject all unterminated ``on`` values; multiline event scalars
            # are outside the trusted workflow subset anyway.
            closed = False
            index = 0
            while index < len(value):
                if quote == '"' and value[index] == "\\":
                    index += 2
                    continue
                if value[index] == quote:
                    if quote == "'" and index + 1 < len(value) and value[index + 1] == "'":
                        index += 2
                        continue
                    closed = True
                    break
                index += 1
            if not closed:
                PROBLEMS.append(
                    f"{label}:{number}: multiline quoted workflow event values are forbidden"
                )
        for issue in parser_ambiguities(line):
            PROBLEMS.append(f"{label}:{number}: parser-ambiguous event YAML: {issue}")
        for quote, body, has_backslash, closed in quoted_mapping_keys(line):
            if not has_backslash or not closed:
                continue
            decoded = decode_quoted_key(quote, body)
            if decoded in {"pull_request", "pull_request_target"}:
                PROBLEMS.append(
                    f"{label}:{number}: escaped forbidden event key {decoded!r} is not allowed"
                )
        literal_forbidden = FORBIDDEN_EVENT_RE.search(line)
        if literal_forbidden:
            PROBLEMS.append(
                f"{label}:{number}: pull_request/pull_request_target is forbidden for exact-tree evidence"
            )
        # YAML double-quoted scalars decode hexadecimal/unicode escapes before
        # GitHub evaluates the ``on`` trigger.  A raw-text search alone would
        # therefore miss values such as ``"pull_\\x72equest"`` (including a
        # list-form trigger item).  Decode every complete quoted scalar and
        # reject an escaped event name when its raw text did not already make
        # the literal check fail.
        if not literal_forbidden:
            for match in DOUBLE_QUOTED_SCALAR_RE.finditer(line):
                decoded = decode_yaml_double_quoted(match.group("body"))
                if decoded is not None and FORBIDDEN_EVENT_RE.search(decoded):
                    PROBLEMS.append(
                        f"{label}:{number}: escaped forbidden event value is not allowed"
                    )
                    break


def validate_wrappers(workflow_texts: dict[str, str]) -> None:
    for path, markers in WRAPPER_MARKERS.items():
        full = ROOT / path
        if not full.is_file():
            PROBLEMS.append(f"missing fail-closed wrapper: {path}")
            continue
        try:
            text = full.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            PROBLEMS.append(f"cannot read fail-closed wrapper {path}: {error}")
            continue
        for marker in markers:
            if marker not in text:
                PROBLEMS.append(f"{path} lacks required delegation marker: {marker}")

    for path in CORE_PATHS:
        if not (ROOT / path).is_file():
            PROBLEMS.append(f"missing immutable core implementation: {path}")

    for workflow_path, text in workflow_texts.items():
        for core_path in CORE_PATHS:
            if core_path in text:
                PROBLEMS.append(
                    f"{workflow_path} bypasses the fail-closed wrapper via {core_path}"
                )


def self_test() -> None:
    pinned = "actions/checkout@" + "a" * 40
    valid_sample = (
        "steps:\n"
        f"  - uses: {pinned}\n"
        f"  - \"uses\" : \"{pinned}\"\n"
        f"  - 'uses': '{pinned}' # quoted key/value\n"
    )
    before = len(PROBLEMS)
    parsed = uses_entries(valid_sample, "<self-test-valid>")
    if [value for _, value in parsed] != [pinned, pinned, pinned]:
        PROBLEMS.append("workflow trust parser self-test did not parse all block-key variants")
    for _, value in parsed:
        problem, _ = action_pin_problem(value, require_local_exists=False)
        if problem:
            PROBLEMS.append(f"workflow trust parser rejected pinned self-test value: {problem}")
    if len(PROBLEMS) != before:
        PROBLEMS.append("workflow trust valid-form self-test failed")

    invalid_samples = {
        "mutable-tag": "steps:\n  - uses: actions/checkout@v4\n",
        "flow-map": f"steps:\n  - {{ uses: {pinned} }}\n",
        "flow-map-multiline": f"steps:\n  - {{\n      uses: {pinned}\n    }}\n",
        "dynamic": "steps:\n  - uses: ${{ matrix.action }}\n",
        "escaped-key": f"steps:\n  - \"us\\x65s\": {pinned}\n",
        "single-quoted-backslash-key": f"steps:\n  - 'us\\x65s': {pinned}\n",
        "tagged-key": f"steps:\n  - !!str \"uses\": {pinned}\n",
        "anchored-mapping": f"steps:\n  - &action\n    uses: {pinned}\n",
        "multiline-quoted-key": f"steps:\n  - \"us\n    es\": {pinned}\n",
    }
    for name, sample in invalid_samples.items():
        local: list[str] = []
        original = len(PROBLEMS)
        parsed = uses_entries(sample, f"<self-test-{name}>")
        for _, value in parsed:
            problem, _ = action_pin_problem(value, require_local_exists=False)
            if problem:
                local.append(problem)
        parser_problems = PROBLEMS[original:]
        del PROBLEMS[original:]
        if not parser_problems and not local:
            PROBLEMS.append(f"workflow trust negative self-test failed: {name}")

    problem, _ = action_pin_problem("./../escape", require_local_exists=False)
    if not problem:
        PROBLEMS.append("workflow trust local path traversal self-test failed")
    problem, _ = action_pin_problem("actions/checkout@" + "0" * 40, require_local_exists=False)
    if not problem:
        PROBLEMS.append("workflow trust all-zero action SHA self-test failed")

    # Exercise both local-descriptor containment guards without depending on
    # platform-specific permission to create symlinks (the hygiene check also
    # runs on the hosted Windows lane).  The mocked lstat-style result models
    # a descriptor candidate that is replaced by a symlink in the checkout.
    with tempfile.TemporaryDirectory(prefix="cex-workflow-trust-") as directory:
        test_root = Path(directory) / "repo"
        action_dir = test_root / "local-action"
        action_dir.mkdir(parents=True)
        (action_dir / "action.yml").write_text(
            "name: self-test\nruns:\n  using: composite\n  steps: []\n",
            encoding="utf-8",
        )
        original_root = ROOT
        try:
            globals()["ROOT"] = test_root
            with mock.patch.object(Path, "is_symlink", return_value=True):
                symlink_problem, _ = action_pin_problem("./local-action")
        finally:
            globals()["ROOT"] = original_root
        if symlink_problem != "local action descriptor must not be a symlink":
            PROBLEMS.append(
                "workflow trust local descriptor symlink self-test failed"
            )

        outside = Path(directory) / "outside-action.yml"
        outside.write_text("name: outside\n", encoding="utf-8")
        try:
            validate_local_action_descriptor(outside, test_root.resolve())
        except ValueError as error:
            containment_problem = str(error)
        else:
            containment_problem = ""
        if "escapes the repository root" not in containment_problem:
            PROBLEMS.append(
                "workflow trust local descriptor containment self-test failed"
            )

    for sample in (
        "on:\n  pull_request:\n",
        "on: [push, pull_request]\n",
        "on:\n  \"pull_request_target\": {}\n",
        "on:\n  \"pull_\\x72equest\": {}\n",
        "on: \"pull_\\x72equest\"\n",
        "on:\n  - \"pull_\\x72equest\"\n",
        "on:\n  !!str \"pull_request\": {}\n",
        "on:\n  &event pull_request_target: {}\n",
        "on: |\n  pull_request\n",
        "on: >-\n  pull_request_target\n",
    ):
        before = len(PROBLEMS)
        validate_authoritative_events(ROOT / "<self-test-events>", sample)
        event_problems = PROBLEMS[before:]
        del PROBLEMS[before:]
        if not event_problems:
            PROBLEMS.append("workflow trust forbidden-event self-test failed")


def git_identity() -> tuple[str | None, str | None]:
    values: list[str | None] = []
    for revision in ("HEAD", "HEAD^{tree}"):
        try:
            value = subprocess.run(
                ["git", "-C", str(ROOT), "rev-parse", revision],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            value = None
        values.append(value or None)
    return values[0], values[1]


def main() -> int:
    self_test()

    workflow_paths: set[Path] = set()
    for pattern in WORKFLOW_GLOBS:
        workflow_paths.update(path for path in ROOT.glob(pattern) if path.is_file())

    workflow_texts: dict[str, str] = {}
    for path in sorted(workflow_paths):
        label = relative(path)
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            PROBLEMS.append(f"{label}: cannot read workflow: {error}")
            continue
        workflow_texts[label] = text
        scan_uses(text, label)
        if label in AUTHORITATIVE_WORKFLOWS:
            validate_authoritative_events(path, text)

    missing = sorted(AUTHORITATIVE_WORKFLOWS - set(workflow_texts))
    for path in missing:
        PROBLEMS.append(f"missing authoritative workflow: {path}")

    validate_wrappers(workflow_texts)
    commit_sha, tree_sha = git_identity()
    if commit_sha is None or tree_sha is None:
        PROBLEMS.append("cannot resolve exact git commit/tree identity")

    result = {
        "schema": "cex.workflow-trust-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "ok": not PROBLEMS,
        "commit_sha": commit_sha,
        "tree_sha": tree_sha,
        "workflow_count": len(workflow_texts),
        "local_action_descriptor_count": len(VISITED_ACTIONS),
        "authoritative_workflows": sorted(AUTHORITATIVE_WORKFLOWS),
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    raise SystemExit(main())
