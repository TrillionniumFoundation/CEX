#!/usr/bin/env python3
"""Validate the Matrix transport durability source contract."""

from __future__ import annotations

from pathlib import Path

from matrix_transport_contract import dumps_result, validate_repository


if __name__ == "__main__":
    result = validate_repository(Path(__file__).resolve().parents[1])
    print(dumps_result(result))
    raise SystemExit(0 if result["status"] == "ok" else 1)
