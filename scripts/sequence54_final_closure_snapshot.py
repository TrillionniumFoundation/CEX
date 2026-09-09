#!/usr/bin/env python3
"""Build a fail-closed final Sequence 54 closure snapshot from live GitHub facts."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen


class SnapshotError(RuntimeError):
    pass


class GitHub:
    def __init__(self, repository: str, token: str) -> None:
        self.repository = repository
        self.token = token
        self.api = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
        self.graphql_url = os.environ.get(
            "GITHUB_GRAPHQL_URL", "https://api.github.com/graphql"
        )

    def call(
        self,
        path: str,
        *,
        method: str = "GET",
        payload: dict[str, Any] | None = None,
        allow_error: bool = False,
        accept: str = "application/vnd.github+json",
    ) -> dict[str, Any]:
        data = None if payload is None else json.dumps(payload).encode("utf-8")
        request = Request(
            self.api + path,
            method=method,
            data=data,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": accept,
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "sequence54-final-closure-snapshot",
                "Content-Type": "application/json",
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                body = response.read()
                return {
                    "status": response.status,
                    "value": json.loads(body) if body else None,
                }
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            if allow_error:
                return {"status": error.code, "error": body[:4000]}
            raise SnapshotError(
                f"{method} {path} -> {error.code}: {body[:1200]}"
            ) from error

    def graphql(
        self,
        query: str,
        variables: dict[str, Any],
        *,
        allow_error: bool = False,
    ) -> dict[str, Any]:
        request = Request(
            self.graphql_url,
            method="POST",
            data=json.dumps({"query": query, "variables": variables}).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": "application/vnd.github+json",
                "User-Agent": "sequence54-final-closure-snapshot",
                "Content-Type": "application/json",
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                return {"status": response.status, "value": json.loads(response.read())}
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            if allow_error:
                return {"status": error.code, "error": body[:4000]}
            raise SnapshotError(
                f"GraphQL -> {error.code}: {body[:1200]}"
            ) from error


def latest_exact_approvals(
    reviews: list[dict[str, Any]],
    *,
    author: str,
    head: str,
) -> list[str]:
    latest: dict[str, dict[str, Any]] = {}
    for review in reviews:
        login = (review.get("user") or {}).get("login")
        if not login:
            continue
        if login not in latest or (review.get("submitted_at") or "") >= (
            latest[login].get("submitted_at") or ""
        ):
            latest[login] = review
    return sorted(
        login
        for login, review in latest.items()
        if login != author
        and review.get("state") == "APPROVED"
        and review.get("commit_id") == head
        and (review.get("user") or {}).get("type") != "Bot"
    )


def review_threads(
    github: GitHub,
    *,
    owner: str,
    name: str,
    number: int,
) -> tuple[list[bool], dict[str, Any] | None]:
    query = """
    query($owner:String!,$name:String!,$number:Int!,$after:String){
      repository(owner:$owner,name:$name){
        pullRequest(number:$number){
          reviewThreads(first:100,after:$after){
            nodes{isResolved}
            pageInfo{hasNextPage endCursor}
          }
        }
      }
    }
    """
    values: list[bool] = []
    after = None
    for _ in range(20):
        result = github.graphql(
            query,
            {"owner": owner, "name": name, "number": number, "after": after},
            allow_error=True,
        )
        body = result.get("value") or {}
        if result.get("status") != 200 or body.get("errors"):
            return values, result
        try:
            block = body["data"]["repository"]["pullRequest"]["reviewThreads"]
        except (KeyError, TypeError) as error:
            return values, {"parse_error": str(error), "response": body}
        values.extend(bool(node["isResolved"]) for node in block["nodes"])
        if not block["pageInfo"]["hasNextPage"]:
            return values, None
        after = block["pageInfo"]["endCursor"]
    return values, {"error": "review thread pagination limit exceeded"}


def governance(
    github: GitHub,
    *,
    repository: str,
    head: str,
    root: Path,
) -> dict[str, Any]:
    summaries = github.call(
        f"/repos/{repository}/rulesets?includes_parents=true&per_page=100",
        allow_error=True,
    )
    details: list[dict[str, Any]] = []
    shape_closed = False
    if summaries.get("status") == 200 and isinstance(summaries.get("value"), list):
        for summary in summaries["value"]:
            if summary.get("enforcement") != "active":
                continue
            detail = github.call(
                f"/repos/{repository}/rulesets/{summary['id']}",
                allow_error=True,
            )
            if detail.get("status") == 200:
                details.append(detail["value"])
        for detail in details:
            conditions = (detail.get("conditions") or {}).get("ref_name") or {}
            include = conditions.get("include") or []
            applies_main = "~DEFAULT_BRANCH" in include or "refs/heads/main" in include
            if (
                not applies_main
                or detail.get("target") != "branch"
                or detail.get("bypass_actors")
            ):
                continue
            rules = {rule.get("type"): rule for rule in detail.get("rules") or []}
            required = {
                "deletion",
                "non_fast_forward",
                "pull_request",
                "required_status_checks",
            }
            if not required.issubset(rules):
                continue
            pull = rules["pull_request"].get("parameters") or {}
            checks = (
                rules["required_status_checks"].get("parameters") or {}
            ).get("required_status_checks") or []
            if (
                int(pull.get("required_approving_review_count") or 0) >= 2
                and pull.get("dismiss_stale_reviews_on_push") is True
                and pull.get("require_code_owner_review") is True
                and pull.get("require_last_push_approval") is True
                and pull.get("required_review_thread_resolution") is True
                and checks
            ):
                shape_closed = True
                break
    protection = github.call(
        f"/repos/{repository}/branches/main/protection",
        allow_error=True,
    )
    receipt_path = root / "docs/release-evidence/sequence54-governance-negative-probes-v1.json"
    receipt: dict[str, Any] | None = None
    probes_closed = False
    if receipt_path.is_file():
        try:
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            probes_closed = (
                receipt.get("schema")
                == "cex.sequence54-governance-negative-probes.v1"
                and receipt.get("head_sha") == head
                and receipt.get("direct_update_rejected") is True
                and receipt.get("non_fast_forward_rejected") is True
                and receipt.get("deletion_rejected") is True
                and receipt.get("same_actor_positive_control") is True
                and receipt.get("administrator_bypass_used") is False
            )
        except (OSError, json.JSONDecodeError) as error:
            receipt = {"parse_error": str(error)}
    return {
        "ruleset_summaries": summaries,
        "ruleset_details": details,
        "branch_protection": protection,
        "ruleset_shape_closed": shape_closed,
        "negative_probe_receipt": receipt,
        "negative_probes_closed": probes_closed,
        "closed": shape_closed
        and protection.get("status") == 200
        and probes_closed,
    }


def check_runs_green(
    github: GitHub,
    repository: str,
    sha: str,
) -> tuple[bool, list[dict[str, Any]]]:
    result = github.call(
        f"/repos/{repository}/commits/{sha}/check-runs?per_page=100&filter=latest",
        allow_error=True,
    )
    if result.get("status") != 200:
        return False, []
    runs = list((result.get("value") or {}).get("check_runs", []))
    return bool(runs) and all(
        run.get("status") == "completed"
        and run.get("conclusion") in {"success", "neutral", "skipped"}
        for run in runs
    ), [
        {
            "name": run.get("name"),
            "status": run.get("status"),
            "conclusion": run.get("conclusion"),
            "details_url": run.get("details_url"),
        }
        for run in runs
    ]


def cross_repository(
    github: GitHub,
    *,
    organization: str,
    cex_repository: str,
    cex_head: str,
) -> dict[str, Any]:
    target_numbers = {104: "World", 150: "Game/Nakama", 62: "Chain", 8: "Integration"}
    role_terms = {
        "World": ("world",),
        "Game/Nakama": ("game", "nakama"),
        "Chain": ("chain", "trnm"),
        "Integration": ("integration",),
    }
    repositories: list[dict[str, Any]] = []
    for page in range(1, 8):
        result = github.call(
            f"/orgs/{organization}/repos?type=all&per_page=100&page={page}",
            allow_error=True,
        )
        if result.get("status") != 200:
            break
        values = result.get("value") or []
        repositories.extend(values)
        if len(values) < 100:
            break

    items: list[dict[str, Any]] = []
    component_heads: list[str] = []
    for number, role in target_numbers.items():
        matches: list[dict[str, Any]] = []
        for repository in repositories:
            full_name = repository.get("full_name")
            if not full_name or full_name == cex_repository:
                continue
            result = github.call(
                f"/repos/{full_name}/pulls/{number}",
                allow_error=True,
            )
            if result.get("status") != 200:
                continue
            pull = result["value"]
            haystack = " ".join(
                [full_name, pull.get("title") or "", pull.get("body") or ""]
            ).lower()
            if not any(term in haystack for term in role_terms[role]):
                continue
            head = (pull.get("head") or {}).get("sha")
            author = (pull.get("user") or {}).get("login")
            reviews_result = github.call(
                f"/repos/{full_name}/pulls/{number}/reviews?per_page=100",
                allow_error=True,
            )
            approvals = (
                latest_exact_approvals(
                    reviews_result.get("value") or [],
                    author=author or "",
                    head=head or "",
                )
                if reviews_result.get("status") == 200
                else []
            )
            checks_green, checks = check_runs_green(github, full_name, head or "")
            matches.append(
                {
                    "repository": full_name,
                    "number": number,
                    "head_sha": head,
                    "draft": pull.get("draft"),
                    "state": pull.get("state"),
                    "body": pull.get("body") or "",
                    "eligible_exact_head_approvals": approvals,
                    "latest_check_runs_green": checks_green,
                    "check_runs": checks,
                    "accepted": pull.get("draft") is False
                    and bool(approvals)
                    and checks_green,
                }
            )
        item = {"role": role, "matches": matches, "unique": len(matches) == 1}
        if len(matches) == 1 and role != "Integration" and matches[0]["head_sha"]:
            component_heads.append(matches[0]["head_sha"])
        items.append(item)

    integration = next(item for item in items if item["role"] == "Integration")
    if integration["unique"]:
        match = integration["matches"][0]
        body = match["body"]
        pins = cex_head in body and all(head in body for head in component_heads)
        match["pins_exact_tuple"] = pins
        match["accepted"] = match["accepted"] and pins
    for item in items:
        for match in item["matches"]:
            match.pop("body", None)
    return {
        "items": items,
        "closed": all(
            item["unique"] and item["matches"][0]["accepted"] for item in items
        ),
    }


def production(
    *,
    root: Path,
    head: str,
    tree: str,
    author: str,
) -> dict[str, Any]:
    path = root / "docs/release-evidence/sequence54-production-authority-v1.json"
    required = {
        "restore_pitr",
        "deployment_cutover_rollback",
        "world_no_dual_writer",
        "provider_unknown_outcome",
        "kms_hsm_custody",
        "ha_endurance_slo",
        "security_operations_support",
        "financial_legal_commercial",
        "accountable_human_go_no_go",
    }
    receipt: dict[str, Any] | None = None
    closed = False
    if path.is_file():
        try:
            receipt = json.loads(path.read_text(encoding="utf-8"))
            receipts = receipt.get("receipts") or []
            categories = {
                item.get("category") for item in receipts if isinstance(item, dict)
            }
            actors = {item.get("actor") for item in receipts if isinstance(item, dict)}
            closed = (
                receipt.get("schema") == "cex.sequence54-production-authority.v1"
                and receipt.get("head_sha") == head
                and receipt.get("head_tree") == tree
                and receipt.get("production_authorization") == "granted"
                and required.issubset(categories)
                and len(actors - {None, "", author}) >= 3
                and all(
                    isinstance(item.get("evidence_url"), str)
                    and item["evidence_url"].startswith("https://github.com/")
                    and item.get("accepted") is True
                    for item in receipts
                )
            )
        except (OSError, json.JSONDecodeError) as error:
            receipt = {"parse_error": str(error)}
    return {"receipt": receipt, "required_categories": sorted(required), "closed": closed}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--repository", required=True)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--master-readback", required=True, type=Path)
    parser.add_argument("--master-run-id", required=True, type=int)
    parser.add_argument("--master-conclusion", required=True)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()

    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        raise SnapshotError("GH_TOKEN is required")
    github = GitHub(arguments.repository, token)
    owner, name = arguments.repository.split("/", 1)
    pull = github.call(
        f"/repos/{arguments.repository}/pulls/{arguments.pull_request}"
    )["value"]
    head = pull["head"]["sha"]
    tree = subprocess.check_output(
        ["git", "-C", str(arguments.root), "rev-parse", "HEAD^{tree}"],
        text=True,
    ).strip()
    checkout = subprocess.check_output(
        ["git", "-C", str(arguments.root), "rev-parse", "HEAD"],
        text=True,
    ).strip()
    if checkout != head:
        raise SnapshotError(f"checkout drift: {checkout} != {head}")

    master = json.loads(arguments.master_readback.read_text(encoding="utf-8"))
    master_bound = (
        arguments.master_conclusion == "success"
        and master.get("head_sha") == head
        and master.get("head_tree") == tree
        and master.get("source_green") is True
        and master.get("exact_head_gates_green") is True
        and master.get("aggregate_p0_green") is True
    )

    reviews = github.call(
        f"/repos/{arguments.repository}/pulls/{arguments.pull_request}/reviews?per_page=100"
    )["value"]
    author = pull["user"]["login"]
    approvals = latest_exact_approvals(reviews, author=author, head=head)
    threads, thread_error = review_threads(
        github,
        owner=owner,
        name=name,
        number=arguments.pull_request,
    )
    conversations_closed = thread_error is None and all(threads)
    governance_result = governance(
        github,
        repository=arguments.repository,
        head=head,
        root=arguments.root,
    )
    cross_result = cross_repository(
        github,
        organization=owner,
        cex_repository=arguments.repository,
        cex_head=head,
    )
    production_result = production(
        root=arguments.root,
        head=head,
        tree=tree,
        author=author,
    )
    all_closed = all(
        [
            master_bound,
            len(approvals) >= 2,
            conversations_closed,
            governance_result["closed"],
            cross_result["closed"],
            production_result["closed"],
        ]
    )
    report = {
        "schema": "cex.sequence54-final-closure-snapshot.v1",
        "observed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repository": arguments.repository,
        "pull_request": arguments.pull_request,
        "head_sha": head,
        "head_tree": tree,
        "base_sha": pull["base"]["sha"],
        "prospective_merge_sha": pull.get("merge_commit_sha"),
        "draft": pull.get("draft"),
        "master_run_id": arguments.master_run_id,
        "master_conclusion": arguments.master_conclusion,
        "master_readback": master,
        "repository_owned_qualification_bound": master_bound,
        "eligible_current_head_approvals": approvals,
        "independent_approvals_closed": len(approvals) >= 2,
        "review_thread_count": len(threads),
        "unresolved_review_thread_count": sum(not value for value in threads),
        "review_thread_error": thread_error,
        "review_conversations_closed": conversations_closed,
        "governance": governance_result,
        "cross_repository": cross_result,
        "production": production_result,
        "all_gaps_closed": all_closed,
        "merge_authorized": all_closed,
        "production_authorization": "granted" if all_closed else "not_granted",
        "force_push_used": False,
        "administrator_bypass_used": False,
        "synthetic_status_used": False,
        "self_approval_used": False,
    }
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, json.JSONDecodeError, SnapshotError) as error:
        print(f"Sequence 54 final closure snapshot failed: {error}", file=sys.stderr)
        raise SystemExit(1)
