#!/usr/bin/env python3
"""Frozen evaluator for the exact-arithmetic replication pack."""

from __future__ import annotations

import json
import pathlib
import sys

from baseline import analyse
from hepta_review_context import KIND


MICROS = 1_000_000


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: evaluator.py DATASET.csv CANDIDATE.json")
    candidate = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    expected = analyse(sys.argv[1])
    passed = candidate == expected
    reference_metrics = {
        "candidate_conformance_micros": MICROS,
        "control_mean_micros": expected["groups"]["control"]["mean_numerator"]
        * MICROS
        // expected["groups"]["control"]["mean_denominator"],
        "mean_difference_micros": expected["difference_numerator"]
        * MICROS
        // expected["difference_denominator"],
        "predeclared_tolerance_micros": expected["predeclared_tolerance"]["numerator"]
        * MICROS
        // expected["predeclared_tolerance"]["denominator"],
        "treated_mean_micros": expected["groups"]["treated"]["mean_numerator"]
        * MICROS
        // expected["groups"]["treated"]["mean_denominator"],
    }
    if KIND == "evaluate":
        result = {
            "candidate_passed": passed,
            "reference_metrics_micros": reference_metrics,
            "tolerance_policy_version": "paper-raid-replication-tolerance-v1",
            "tolerance_rules": [
                {
                    "kind": "absolute",
                    "max_delta_micros": 0,
                    "metric": metric,
                }
                for metric in sorted(reference_metrics)
            ],
        }
    elif KIND == "reproduce":
        result = {
            "observed_metrics_micros": {
                **reference_metrics,
                "candidate_conformance_micros": MICROS if passed else 0,
            },
            "statistical_evidence": {},
        }
    else:
        raise SystemExit("unsupported frozen review kind")
    print(json.dumps(result, separators=(",", ":"), sort_keys=True))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
