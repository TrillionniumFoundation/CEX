#!/usr/bin/env python3
"""Import-only v2 reconciliation core retained for the canonical v3 command."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import stat
import sys
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
IMPLEMENTATION_PATH = ROOT / "scripts/reconcile-matrix-adapter-result-v2-internal.py"
DIRECT_SELF_TEST = ["--self-test"]

if __name__ == "__main__" and sys.argv[1:] != DIRECT_SELF_TEST:
    print("matrix_adapter_result_v2_historical_core_not_runnable", file=sys.stderr)
    raise SystemExit(1)


class HistoricalCoreError(RuntimeError):
    pass


def load_implementation() -> ModuleType:
    try:
        metadata = IMPLEMENTATION_PATH.lstat()
    except OSError:
        raise HistoricalCoreError("historical_v2_implementation_unavailable") from None
    if (
        IMPLEMENTATION_PATH.is_symlink()
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_mode & 0o111
    ):
        raise HistoricalCoreError("historical_v2_implementation_unsafe")
    spec = importlib.util.spec_from_file_location(
        "cex_matrix_adapter_result_reconciler_v2_internal",
        IMPLEMENTATION_PATH,
    )
    if spec is None or spec.loader is None:
        raise HistoricalCoreError("historical_v2_implementation_unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    if (
        getattr(module, "SCHEMA", None) != "cex.matrix.adapter-result-reconciler.v1"
        or getattr(module, "SECURITY_CONTRACT", None) != "v2"
    ):
        raise HistoricalCoreError("historical_v2_implementation_identity_mismatch")
    return module


_IMPLEMENTATION = load_implementation()
ReconciliationError = _IMPLEMENTATION.ReconciliationError
LOOKUP_ACTION = _IMPLEMENTATION.LOOKUP_ACTION
LOOKUP_SCHEMA = _IMPLEMENTATION.LOOKUP_SCHEMA
SCHEMA = _IMPLEMENTATION.SCHEMA
SECURITY_CONTRACT = _IMPLEMENTATION.SECURITY_CONTRACT
delivery_request_fingerprint = _IMPLEMENTATION.delivery_request_fingerprint
parse_lookup_response = _IMPLEMENTATION.parse_lookup_response
build_sql = _IMPLEMENTATION.build_sql
self_test = _IMPLEMENTATION.self_test


def main() -> int:
    # The v3 layer replaces these wrapper globals before entering the shared CLI.
    _IMPLEMENTATION.parse_lookup_response = parse_lookup_response
    _IMPLEMENTATION.build_sql = build_sql
    _IMPLEMENTATION.SCHEMA = SCHEMA
    _IMPLEMENTATION.SECURITY_CONTRACT = SECURITY_CONTRACT
    return int(_IMPLEMENTATION.main())


if __name__ == "__main__":
    raise SystemExit(main())
