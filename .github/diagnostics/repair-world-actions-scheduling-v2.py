#!/usr/bin/env python3
"""Compatibility wrapper for bounded World Actions repair.

GitHub's workflow-list endpoint may omit workflows that exist only on the PR
branch before their first run. Verify such files directly at the exact branch
and represent them as `branch_defined_unregistered`; do not claim they are
active and still require a real native run after the trigger commit.
"""

from __future__ import annotations

import importlib.util
import pathlib
import urllib.parse

MODULE_PATH = pathlib.Path(__file__).with_name("repair-world-actions-scheduling.py")
spec = importlib.util.spec_from_file_location("world_actions_v1", MODULE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load World Actions repair helper")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original_request = module.request


def compatible_request(token, method, path, body=None):
    payload = original_request(token, method, path, body)
    if method == "GET" and path.startswith(
        f"/repos/{module.REPOSITORY}/actions/workflows?"
    ):
        workflows = payload.setdefault("workflows", [])
        present = {
            pathlib.PurePosixPath(item.get("path", "")).name
            for item in workflows
            if isinstance(item, dict)
        }
        branch = urllib.parse.quote(module.BRANCH, safe="")
        for basename in sorted(module.REQUIRED_WORKFLOW_BASENAMES - present):
            content_path = (
                f"/repos/{module.REPOSITORY}/contents/.github/workflows/"
                f"{urllib.parse.quote(basename, safe='')}?ref={branch}"
            )
            try:
                original_request(token, "GET", content_path)
            except RuntimeError:
                continue
            workflows.append(
                {
                    "id": None,
                    "path": f".github/workflows/{basename}",
                    "name": basename,
                    "state": "branch_defined_unregistered",
                }
            )
        payload["total_count"] = len(workflows)
    return payload


module.request = compatible_request
raise SystemExit(module.main())
