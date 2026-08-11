#!/usr/bin/env python3
"""Deterministic integer benchmark and ablation runner."""

from __future__ import annotations

import csv
import json
import pathlib
import sys


MODES = ("full", "zeroed-signal", "without-shortcut")


def rows(path: str) -> list[dict[str, int | str]]:
    with pathlib.Path(path).open(encoding="utf-8", newline="") as handle:
        return [
            {
                "sample_id": row["sample_id"],
                "signal": int(row["signal"]),
                "shortcut": int(row["shortcut"]),
                "target": int(row["target"]),
            }
            for row in csv.DictReader(handle)
        ]


def run(dataset: list[dict[str, int | str]], mode: str) -> dict[str, object]:
    predictions = []
    for row in dataset:
        signal = int(row["signal"])
        shortcut = int(row["shortcut"])
        if mode == "full":
            prediction = 2 * signal + shortcut
        elif mode == "zeroed-signal":
            prediction = shortcut
        elif mode == "without-shortcut":
            prediction = 2 * signal
        else:
            raise ValueError(mode)
        predictions.append({"sample_id": row["sample_id"], "prediction": prediction})
    targets = {row["sample_id"]: int(row["target"]) for row in dataset}
    squared_error = sum(
        (item["prediction"] - targets[item["sample_id"]]) ** 2 for item in predictions
    )
    return {
        "mode": mode,
        "predictions": predictions,
        "retained": mode == "zeroed-signal",
        "sample_count": len(dataset),
        "status": "failed" if mode == "zeroed-signal" else "successful",
        "sum_squared_error": squared_error,
    }


def main() -> int:
    if len(sys.argv) != 3 or sys.argv[2] not in MODES:
        raise SystemExit(f"usage: baseline.py DATASET.csv {'|'.join(MODES)}")
    print(json.dumps(run(rows(sys.argv[1]), sys.argv[2]), separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
