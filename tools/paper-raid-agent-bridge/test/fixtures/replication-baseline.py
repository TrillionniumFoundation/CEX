#!/usr/bin/env python3
"""Exact-arithmetic independent replication baseline."""

from __future__ import annotations

import csv
from fractions import Fraction
import json
import pathlib
import sys


def analyse(path: str) -> dict[str, object]:
    groups: dict[str, list[int]] = {"control": [], "treated": []}
    with pathlib.Path(path).open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            groups[row["group"]].append(int(row["value"]))
    stats = {}
    means = {}
    for name in ("control", "treated"):
        values = groups[name]
        mean = Fraction(sum(values), len(values))
        means[name] = mean
        stats[name] = {
            "count": len(values),
            "mean_denominator": mean.denominator,
            "mean_numerator": mean.numerator,
            "sum": sum(values),
        }
    difference = means["treated"] - means["control"]
    return {
        "analysis_id": "frozen-mean-difference-v1",
        "difference_denominator": difference.denominator,
        "difference_numerator": difference.numerator,
        "groups": stats,
        "predeclared_tolerance": {"denominator": 1, "numerator": 0},
        "schema": "hepta.replication.report.v1",
    }


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: baseline.py DATASET.csv")
    print(json.dumps(analyse(sys.argv[1]), separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
