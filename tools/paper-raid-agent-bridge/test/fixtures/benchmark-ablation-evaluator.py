#!/usr/bin/env python3
"""Frozen evaluator for the deterministic benchmark/ablation pack."""

from __future__ import annotations

import json
import pathlib
import sys

from baseline import MODES, rows, run
from hepta_review_context import KIND


MICROS = 1_000_000


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: evaluator.py DATASET.csv CANDIDATE.json")
    candidate = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    expected_runs = [run(rows(sys.argv[1]), mode) for mode in MODES]
    passed = (
        candidate.get("schema") == "hepta.benchmark_ablation.report.v1"
        and candidate.get("seed") == 1701
        and candidate.get("runs") == expected_runs
        and set(candidate) == {"schema", "seed", "runs"}
    )
    reference_metrics = {
        "candidate_conformance_micros": MICROS,
        "full_sum_squared_error_micros": expected_runs[0]["sum_squared_error"] * MICROS,
        "retained_failure_micros": MICROS
        if any(item["status"] == "failed" and item["retained"] for item in expected_runs)
        else 0,
        "without_shortcut_sum_squared_error_micros": expected_runs[2]["sum_squared_error"]
        * MICROS,
        "zeroed_signal_sum_squared_error_micros": expected_runs[1]["sum_squared_error"]
        * MICROS,
    }
    if KIND == "evaluate":
        result = {
            "candidate_passed": passed,
            "reference_metrics_micros": reference_metrics,
            "tolerance_policy_version": "paper-raid-benchmark-ablation-tolerance-v1",
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
