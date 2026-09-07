#!/usr/bin/env python3
"""Hardened wrapper for the World catalog patcher.

Recover only the exact two-line installer state created by the interrupted
maintenance attempt, using the reviewed ancestor object. Unknown drift remains
fatal. The valid PR API compatibility fix from v2 is retained.
"""

from __future__ import annotations

import importlib.util
import pathlib
import subprocess

REVIEWED_BASE = "cff9f3fd3539420c676f3d1397c166b1c1e24ffb"
KNOWN_PARTIAL_INSTALLER = "#!/usr/bin/env bash\nset -euo pipefail\n"
MODULE_PATH = pathlib.Path(__file__).with_name("patch-world-single-writer.py")
spec = importlib.util.spec_from_file_location("world_patch_v1", MODULE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load World patch helper")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original_patch_installer = module.patch_installer
original_api = module.api


def recover_known_partial_then_patch(path: pathlib.Path) -> bool:
    current = path.read_text(encoding="utf-8")
    if current == KNOWN_PARTIAL_INSTALLER:
        repo = path.parent.parent
        restored = subprocess.run(
            ["git", "show", f"{REVIEWED_BASE}:scripts/apply-trnm-world-authority-cutover-v1.sh"],
            cwd=repo,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout
        if "active_index_definition not like" not in restored:
            raise RuntimeError("reviewed ancestor installer does not contain the expected weak block")
        path.write_text(restored, encoding="utf-8")
    return original_patch_installer(path)


def compatible_api(token, method, path, body=None):
    if method == "PATCH" and path.endswith("/pulls/60") and isinstance(body, dict):
        body = {key: value for key, value in body.items() if key != "draft"}
    return original_api(token, method, path, body)


module.patch_installer = recover_known_partial_then_patch
module.api = compatible_api
raise SystemExit(module.main())
