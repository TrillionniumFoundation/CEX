#!/usr/bin/env python3
"""Static, fixture-backed gate for the ordinary seven-identity player path.

This gate intentionally does not call a live BFF, Hepta, Nakama, CAS, or
Chain authority.  It proves the repo-local contracts that must be true before
those black-box checks are attempted and emits explicit ``not_run`` markers
for the live phases so a source gate cannot be mistaken for player evidence.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA = "hepta.paper_raid.player_path_gate.v1"
IDENTITY_PREFIX = "PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON="
EXPECTED_IDENTITY_KEYS = {
    "login_key",
    "subject_id",
    "display_name",
    "nakama_user_id",
    "player_id",
    "scopes",
    "author_roles",
}
SCOPES = {"author", "evaluator", "reviewer", "reproducer"}
AUTHOR_ROLES = {"captain", "evidence", "experiment"}
UUID_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)


class GateError(ValueError):
    """A bounded, operator-readable gate failure."""


def _distinct_assignment(items: list[dict[str, Any]], requirements: list[str], field: str) -> bool:
    """Return whether each requirement can use a different item."""

    def visit(position: int, used: set[int]) -> bool:
        if position == len(requirements):
            return True
        requirement = requirements[position]
        for index, item in enumerate(items):
            if index in used or requirement not in item[field]:
                continue
            if visit(position + 1, used | {index}):
                return True
        return False

    return visit(0, set())


def validate_identity_fixture(raw: Any, *, exact_seven: bool = True) -> dict[str, Any]:
    """Validate the fixed-alpha topology and return a redacted summary."""

    if not isinstance(raw, list):
        raise GateError("identity fixture must be a JSON array")
    if exact_seven and len(raw) != 7:
        raise GateError("identity fixture must contain exactly seven identities")
    if len(raw) < 7 or len(raw) > 64:
        raise GateError("identity fixture must contain 7..64 identities")

    for index, identity in enumerate(raw):
        if not isinstance(identity, dict) or set(identity) != EXPECTED_IDENTITY_KEYS:
            raise GateError(f"identity {index} does not use the explicit production field set")
        login_key = identity["login_key"]
        if not isinstance(login_key, str) or len(login_key) < 32:
            raise GateError(f"identity {index} login key is too short")
        subject_id = identity["subject_id"]
        if not isinstance(subject_id, str) or not re.fullmatch(r"[A-Za-z0-9._:-]{1,128}", subject_id):
            raise GateError(f"identity {index} subject id is invalid")
        display_name = identity["display_name"]
        if not isinstance(display_name, str) or not display_name.strip() or len(display_name) > 80:
            raise GateError(f"identity {index} display name is invalid")
        for field in ("nakama_user_id", "player_id"):
            if not isinstance(identity[field], str) or not UUID_RE.fullmatch(identity[field]):
                raise GateError(f"identity {index} {field} is not a canonical UUID")

        scopes = identity["scopes"]
        roles = identity["author_roles"]
        if (
            not isinstance(scopes, list)
            or not scopes
            or not all(isinstance(value, str) for value in scopes)
            or len(scopes) != len(set(scopes))
            or not set(scopes) <= SCOPES
        ):
            raise GateError(f"identity {index} scopes are invalid")
        if (
            not isinstance(roles, list)
            or not all(isinstance(value, str) for value in roles)
            or len(roles) != len(set(roles))
            or not set(roles) <= AUTHOR_ROLES
            or (("author" in scopes) != bool(roles))
        ):
            raise GateError(f"identity {index} author roles are invalid")

    for field in ("login_key", "subject_id", "nakama_user_id", "player_id"):
        values = [identity[field] for identity in raw]
        if len(values) != len(set(values)):
            raise GateError(f"identity fixture has duplicate {field}")

    authors = [identity for identity in raw if "author" in identity["scopes"]]
    independent = [identity for identity in raw if "author" not in identity["scopes"]]
    if len(authors) != 3 or len(independent) != 4:
        raise GateError("strict alpha fixture must separate three authors and four non-authors")
    if not _distinct_assignment(authors, ["captain", "evidence", "experiment"], "author_roles"):
        raise GateError("authors cannot fill Captain/Evidence/Experiment as distinct seats")
    if not _distinct_assignment(
        independent,
        ["evaluator", "reviewer", "reviewer", "reproducer"],
        "scopes",
    ):
        raise GateError("non-authors cannot fill evaluator/reviewer/reviewer/reproducer seats")

    return {
        "count": len(raw),
        "authors": len(authors),
        "independent_review_identities": len(independent),
        "author_roles": ["captain", "evidence", "experiment"],
        "review_slots": ["evaluator", "reviewer_1", "reviewer_2", "reproducer"],
    }


def load_alpha_fixture(repo: Path) -> tuple[Any, Path]:
    path = repo / "services/paper-raid-bff/deploy/alpha.env.example"
    lines = [line[len(IDENTITY_PREFIX) :] for line in path.read_text().splitlines() if line.startswith(IDENTITY_PREFIX)]
    if len(lines) != 1:
        raise GateError("alpha.env.example must contain exactly one identity JSON assignment")
    try:
        return json.loads(lines[0]), path
    except json.JSONDecodeError as error:
        raise GateError(f"alpha identity JSON is invalid: {error}") from error


def validate_normal_bridge_contract(repo: Path) -> dict[str, Any]:
    """Prove that the ordinary Bridge path starts with authority discovery."""

    config_path = repo / "tools/paper-raid-agent-bridge/example.config.json"
    config = json.loads(config_path.read_text())
    if config.get("paper_ids") != []:
        raise GateError("example Bridge config must leave paper_ids empty for the player path")

    lifecycle = (repo / "tools/paper-raid-agent-bridge/src/lifecycle.mjs").read_text()
    if "paper_ids: []," not in lifecycle:
        raise GateError("Bridge install defaults must leave paper_ids empty")

    operations = (repo / "tools/paper-raid-agent-bridge/src/operations.mjs").read_text()
    if "paper_ids: config.paper_ids" not in operations:
        raise GateError("Bridge inbox request must use the validated config paper_ids field")

    readme = (repo / "tools/paper-raid-agent-bridge/README.md").read_text()
    bff_readme = (repo / "services/paper-raid-bff/README.md").read_text()
    required_readme = (
        "Leave `paper_ids` empty for the normal player path.",
        "discovers the paired player's current Papers",
    )
    for marker in required_readme:
        if marker not in readme:
            raise GateError(f"Bridge README is missing normal-player contract: {marker}")
    if "Pasting public V3 proof JSON" not in bff_readme or "collapsed recovery/developer fallback" not in bff_readme:
        raise GateError("BFF README must keep public V3 JSON inside the collapsed recovery fallback")

    # The exact Paper-ID override remains an operator diagnostic escape hatch,
    # but it must not be mistaken for the ordinary path.  Reject a future
    # top-level CLI flag that would make manual selection look like onboarding.
    cli = (repo / "tools/paper-raid-agent-bridge/src/cli.mjs").read_text()
    forbidden_flags = ("--paper-id", "--paper_id", "--paper-ids", "--paper_ids")
    leaked = [flag for flag in forbidden_flags if flag in cli]
    if leaked:
        raise GateError(f"Bridge CLI exposes manual Paper selection flags: {', '.join(leaked)}")

    return {
        "paper_ids_default": [],
        "authority_discovery": True,
        "manual_paper_cli_flags": False,
        "developer_json_fallback": "collapsed_recovery_only",
    }


def validate_browser_primary_contract(repo: Path) -> dict[str, Any]:
    """Ensure JSON/developer controls are not the primary browser path."""

    html = (repo / "services/paper-raid-bff/src/html.rs").read_text()
    browser = (repo / "services/paper-raid-bff/src/browser.js").read_text()
    details_marker = "<details class=\\\"panel developer-tools\\\">"
    if details_marker not in html and '<details class="panel developer-tools">' not in html:
        raise GateError("legacy protocol controls must stay inside a collapsed Developer Tools details")
    if "known playability gap" not in html:
        raise GateError("Developer Tools fallback must be explicitly labelled as non-primary")
    if "agent-pairing-grant-form" not in html or "/api/agent-bridge/pairing-grants" not in browser:
        raise GateError("normal Bridge onboarding is missing browser pairing flow")
    if "clipboard_unavailable_copy_the_rendered_command_manually" in browser:
        raise GateError("browser still exposes command-copy fallback in the normal path")
    return {
        "primary_onboarding": "browser_pairing_grant",
        "developer_tools": "collapsed_non_primary",
        "command_copy_fallback": False,
    }


def run_node_tests(repo: Path) -> dict[str, Any]:
    command = ["npm", "--prefix", "tools/paper-raid-agent-bridge", "test"]
    result = subprocess.run(command, cwd=repo, text=True, capture_output=True)
    if result.returncode:
        # Keep the error bounded; the complete log remains available to the
        # caller's CI process if it wants to rerun the command verbosely.
        detail = (result.stderr or result.stdout).strip().splitlines()[-8:]
        raise GateError("Bridge Node tests failed: " + " | ".join(detail))
    return {"command": " ".join(command), "status": "pass"}


def run_self_tests(repo: Path) -> dict[str, Any]:
    """Exercise the validator against the common topology regressions."""

    fixture, _ = load_alpha_fixture(repo)
    validate_identity_fixture(fixture)
    mutants: list[tuple[str, Any]] = []

    mutants.append(("six_identities", fixture[:-1]))
    duplicate = copy.deepcopy(fixture)
    duplicate[1]["player_id"] = duplicate[0]["player_id"]
    mutants.append(("duplicate_player", duplicate))
    missing_reviewer = copy.deepcopy(fixture)
    missing_reviewer[5]["scopes"] = ["evaluator"]
    mutants.append(("missing_second_reviewer", missing_reviewer))
    overlapping_author = copy.deepcopy(fixture)
    overlapping_author[1]["author_roles"] = ["captain"]
    overlapping_author[2]["author_roles"] = ["captain"]
    mutants.append(("overlapping_author_roles", overlapping_author))
    non_string_scope = copy.deepcopy(fixture)
    non_string_scope[3]["scopes"] = [42]
    mutants.append(("non_string_scope", non_string_scope))

    caught: list[str] = []
    for name, mutant in mutants:
        try:
            validate_identity_fixture(mutant)
        except GateError:
            caught.append(name)
        else:
            raise GateError(f"self-test mutant unexpectedly passed: {name}")
    validate_normal_bridge_contract(repo)
    validate_browser_primary_contract(repo)
    return {"mutants_rejected": caught, "status": "pass"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-node", action="store_true", help="do not run the offline Bridge Node suite")
    parser.add_argument("--summary-file", type=Path, help="write the bounded JSON summary here")
    args = parser.parse_args()
    repo = args.repo.resolve()

    fixture, fixture_path = load_alpha_fixture(repo)
    summary: dict[str, Any] = {
        "schema": SCHEMA,
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "repo": str(repo),
        "fixture": str(fixture_path),
        "live_evidence": {
            "strict_author": "not_run",
            "strict_review": "not_run",
            "three_bridge_runs": "not_run",
            "receipt_v4": "not_run",
            "twenty_four_hour_soak": "not_run",
            "deployment_or_release_authority": "not_run",
        },
    }
    summary["identity_topology"] = validate_identity_fixture(fixture)
    summary["normal_bridge"] = validate_normal_bridge_contract(repo)
    summary["browser_primary_path"] = validate_browser_primary_contract(repo)
    summary["validator_self_test"] = run_self_tests(repo)
    if not args.skip_node:
        summary["bridge_node_tests"] = run_node_tests(repo)
    else:
        summary["bridge_node_tests"] = {"status": "not_run"}

    if args.summary_file:
        args.summary_file.parent.mkdir(parents=True, exist_ok=True)
        args.summary_file.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (GateError, OSError, json.JSONDecodeError) as error:
        print(f"paper-raid player-path gate: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
