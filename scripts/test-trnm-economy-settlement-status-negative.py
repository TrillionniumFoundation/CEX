#!/usr/bin/env python3
"""Prove that the settlement status checker rejects promotion overclaims."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "docs/status/trnm-economy-settlement-v1.json"
CHECKER = ROOT / "scripts/check-trnm-economy-settlement-contract.py"

mutations = (
    ("trusted_settlement", True),
    ("public_online", True),
    ("public_player_market", True),
    ("verified_commit", "a" * 40),
)

source = json.loads(SOURCE.read_text(encoding="utf-8"))
for field, value in mutations:
    candidate = json.loads(json.dumps(source))
    candidate[field] = value
    with tempfile.NamedTemporaryFile("w", suffix=".json", encoding="utf-8", delete=False) as handle:
        json.dump(candidate, handle)
        path = pathlib.Path(handle.name)
    try:
        result = subprocess.run(
            [sys.executable, str(CHECKER), str(path)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"status checker accepted forbidden overclaim: {field}")
    finally:
        path.unlink(missing_ok=True)

print("TRNM CEX settlement negative status fixtures: PASS")
