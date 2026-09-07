#!/usr/bin/env python3
"""Compatibility wrapper for exact World-to-CEX repinning.

Prefer an external repository credential so the resulting CEX candidate push
creates ordinary Actions runs. The workflow-scoped GITHUB_TOKEN remains a last
fallback and its non-recursive behavior is truthfully detected by the base
helper. Strip the unsupported `draft` field from PR PATCH while preserving the
existing Draft state.
"""

from __future__ import annotations

import importlib.util
import os
import pathlib

MODULE_PATH = pathlib.Path(__file__).with_name("repin-cex-world-head.py")
spec = importlib.util.spec_from_file_location("cex_repin_v1", MODULE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load CEX repin helper")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original_choose = module.choose_cex_token
original_api = module.api


def choose_recursion_capable_token(destination):
    default_token = os.environ.pop("GITHUB_TOKEN", None)
    try:
        try:
            return original_choose(destination)
        except RuntimeError:
            pass
    finally:
        if default_token is not None:
            os.environ["GITHUB_TOKEN"] = default_token
    return original_choose(destination)


def compatible_api(token, method, repository, path, body=None):
    if method == "PATCH" and path.endswith("/pulls/34") and isinstance(body, dict):
        body = {key: value for key, value in body.items() if key != "draft"}
    return original_api(token, method, repository, path, body)


module.choose_cex_token = choose_recursion_capable_token
module.api = compatible_api
raise SystemExit(module.main())
