#!/usr/bin/env python3
"""Compatibility wrapper for the atomic World patcher.

GitHub's update-pull-request REST endpoint preserves Draft state but does not
accept a `draft` request member. Strip only that unsupported field; all source,
verification and fail-closed behavior remains in the reviewed v1 helper.
"""

from __future__ import annotations

import importlib.util
import pathlib

MODULE_PATH = pathlib.Path(__file__).with_name("patch-world-single-writer.py")
spec = importlib.util.spec_from_file_location("world_patch_v1", MODULE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load World patch helper")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original_api = module.api


def compatible_api(token, method, path, body=None):
    if method == "PATCH" and path.endswith("/pulls/60") and isinstance(body, dict):
        body = {key: value for key, value in body.items() if key != "draft"}
    return original_api(token, method, path, body)


module.api = compatible_api
raise SystemExit(module.main())
