#!/usr/bin/env python3
"""Fail closed on mutable or parser-ambiguous GitHub Actions trust inputs."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import Iterable

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

    target = (ROOT / relative_raw).resolve()
    try:
        target.relative_to(ROOT.resolve())
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

    descriptors = [
        candidate
        for candidate in (target / "action.yml", target / "action.yaml")
        if candidate.is_file()
    ]
    if len(descriptors) != 1:
        raise ValueError(
            "local action directory must contain exactly one action.yml or action.yaml"
        )
    return descriptors[0].resolve()


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
    if not location or not separator or not PINNED_SHA_RE.fullmatch(ref):
        return "external actions must use a 40-character lowercase commit SHA", None
    if not REMOTE_ACTION_RE.fullmatch(location):
        return "external action location has an invalid owner/repository/path shape", None
    if any(segment in {"", ".", ".."} for segment in location.split("/")):
        return "external action location contains an invalid path segment", None
    return None, None


def uses_entries(text: str, label: str) -> list[tuple[int, str]]:
    entries: list[tuple[int, str]] = []
    for number, physical in enumerate(text.splitlines(), start=1):
        line = strip_yaml_comment(physical).rstrip()
        if not line.strip():
            continue
        if FLOW_USES_RE.search(line):
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
            PROBLEMS.append(
                f"{label}:{number}: parser-ambiguous uses key is forbidden"
            )
    return entries


def scan_action_document(path: Path) -> None:
    path = path.resolve()
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


def unquoted_lines(text: str) -> Iterable[tuple[int, str]]:
    for number, physical in enumerate(text.splitlines(), start=1):
        yield number, strip_yaml_comment(physical)


def validate_authoritative_events(path: Path, text: str) -> None:
    label = relative(path)
    for number, line in unquoted_lines(text):
        if FORBIDDEN_EVENT_RE.search(line):
            PROBLEMS.append(
                f"{label}:{number}: pull_request/pull_request_target is forbidden for exact-tree evidence"
            )


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
        "dynamic": "steps:\n  - uses: ${{ matrix.action }}\n",
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

    for sample in (
        "on:\n  pull_request:\n",
        "on: [push, pull_request]\n",
        "on:\n  \"pull_request_target\": {}\n",
    ):
        if not any(FORBIDDEN_EVENT_RE.search(line) for _, line in unquoted_lines(sample)):
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
