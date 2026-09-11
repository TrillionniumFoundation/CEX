#!/usr/bin/env python3
"""Canonical Matrix adapter-result reconciler; only security contract v3 is runnable."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
V3_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v3.py"
SCHEMA = "cex.matrix.adapter-result-reconciler.v3"
SECURITY_CONTRACT = "v3"


class CanonicalReconcilerError(RuntimeError):
    pass


def load_v3() -> ModuleType:
    if V3_PATH.is_symlink() or not V3_PATH.is_file():
        raise CanonicalReconcilerError("reconciler_v3_unavailable")
    spec = importlib.util.spec_from_file_location(
        "cex_matrix_adapter_result_reconciler_v3",
        V3_PATH,
    )
    if spec is None or spec.loader is None:
        raise CanonicalReconcilerError("reconciler_v3_unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    if (
        getattr(module, "SCHEMA", None) != SCHEMA
        or getattr(module, "SECURITY_CONTRACT", None) != SECURITY_CONTRACT
    ):
        raise CanonicalReconcilerError("reconciler_v3_identity_mismatch")
    return module


def main() -> int:
    try:
        return int(load_v3().main())
    except Exception:
        print("matrix_adapter_result_reconciler_v3_failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
