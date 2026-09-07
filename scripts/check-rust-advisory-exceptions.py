#!/usr/bin/env python3
"""Stable workflow entry point for the authoritative Rust supply-chain gate."""
import json

from rust_advisory_feature_closure_v6 import validate_feature_closure
from rust_advisory_gate_v5 import main as baseline_main

if __name__ == "__main__":
    feature_evidence = validate_feature_closure()
    print(json.dumps({"feature_closure": feature_evidence}, indent=2, sort_keys=True))
    raise SystemExit(baseline_main())
