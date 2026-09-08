#!/usr/bin/env python3
"""Stable Sequence 54 entry point for the authoritative Rust supply-chain gate."""
import json

import rust_advisory_gate_v5 as baseline
from rust_advisory_feature_closure_v6 import validate_feature_closure
from rust_advisory_policy_v6 import validate_authority, validate_time

# Rebind only the candidate/renewal authority layer. The v5 dependency, release-surface,
# advisory, license, hostile-fixture and tool-version checks remain unchanged.
baseline.validate_authority = validate_authority
baseline.validate_time = validate_time

if __name__ == "__main__":
    feature_evidence = validate_feature_closure()
    print(json.dumps({"feature_closure": feature_evidence}, indent=2, sort_keys=True))
    raise SystemExit(baseline.main())
