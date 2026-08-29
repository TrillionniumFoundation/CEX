#!/usr/bin/env python3
"""Frozen, dependency-free evaluator for the evidence-audit pack."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import sys

from hepta_review_context import KIND


DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
MICROS = 1_000_000


def expected_decisions(dataset: dict[str, object]) -> list[dict[str, object]]:
    sources = {item["source_id"]: item for item in dataset["sources"]}
    expected = []
    for claim in dataset["claims"]:
        reasons = []
        source = sources.get(claim["citation_source_id"])
        if source is None:
            reasons.append("missing_source")
        elif claim["declared_license"] != source["license"]:
            reasons.append("license_mismatch")
        actual_digest = "sha256:" + hashlib.sha256(
            claim["evidence_excerpt"].encode("utf-8")
        ).hexdigest()
        if not DIGEST.fullmatch(claim["evidence_sha256"]) or actual_digest != claim["evidence_sha256"]:
            reasons.append("provenance_mismatch")
        expected.append(
            {
                "claim_id": claim["claim_id"],
                "outcome": "pass" if not reasons else "fail",
                "reasons": reasons,
            }
        )
    return expected


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: evaluator.py DATASET.json CANDIDATE.json")
    dataset = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    candidate = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    expected = expected_decisions(dataset)
    expected_summary = {
        "failed": sum(item["outcome"] == "fail" for item in expected),
        "passed": sum(item["outcome"] == "pass" for item in expected),
    }
    passed = (
        candidate.get("schema") == "hepta.evidence_audit.report.v1"
        and candidate.get("decisions") == expected
        and candidate.get("summary") == expected_summary
        and set(candidate) == {"schema", "decisions", "summary"}
    )
    reference_metrics = {
        "candidate_conformance_micros": MICROS,
        "claims_failed_micros": expected_summary["failed"] * MICROS,
        "claims_passed_micros": expected_summary["passed"] * MICROS,
        "claims_total_micros": len(expected) * MICROS,
    }
    if KIND == "evaluate":
        result = {
            "candidate_passed": passed,
            "reference_metrics_micros": reference_metrics,
            "tolerance_policy_version": "paper-raid-evidence-audit-tolerance-v1",
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
