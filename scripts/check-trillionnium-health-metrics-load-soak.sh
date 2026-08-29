#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd python3 >/dev/null

CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$PROJECT_ROOT/run/health-metrics-load-soak"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
REQUESTS_PER_ENDPOINT="${TRILLIONNIUM_HEALTH_METRICS_SOAK_REQUESTS_PER_ENDPOINT:-40}"
CONCURRENCY="${TRILLIONNIUM_HEALTH_METRICS_SOAK_CONCURRENCY:-12}"
P95_TARGET_SECONDS="${TRILLIONNIUM_HEALTH_METRICS_SOAK_P95_TARGET_SECONDS:-0.75}"
MAX_TARGET_SECONDS="${TRILLIONNIUM_HEALTH_METRICS_SOAK_MAX_TARGET_SECONDS:-2.0}"
REQUEST_TIMEOUT_SECONDS="${TRILLIONNIUM_HEALTH_METRICS_SOAK_REQUEST_TIMEOUT_SECONDS:-10.0}"

python3 - \
  "$PROJECT_ROOT" \
  "$SUMMARY_DIR" \
  "$CHECKED_AT" \
  "$CONSUMER_ENTRY_BASE_URL" \
  "$REQUESTS_PER_ENDPOINT" \
  "$CONCURRENCY" \
  "$P95_TARGET_SECONDS" \
  "$MAX_TARGET_SECONDS" \
  "$REQUEST_TIMEOUT_SECONDS" <<'PY'
import concurrent.futures
import json
import math
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

project_root = Path(sys.argv[1])
summary_dir = Path(sys.argv[2])
checked_at = int(sys.argv[3])
base_url = sys.argv[4].rstrip("/")
requests_per_endpoint = int(sys.argv[5])
concurrency = int(sys.argv[6])
p95_target_seconds = float(sys.argv[7])
max_target_seconds = float(sys.argv[8])
request_timeout_seconds = float(sys.argv[9])

if requests_per_endpoint < 5:
    raise SystemExit("requests_per_endpoint must be >= 5")
if concurrency < 1:
    raise SystemExit("concurrency must be >= 1")

contract_version = "trillionnium_health_metrics_load_soak_v1"
endpoints = ["/health", "/metrics"]

def fetch(endpoint, sequence):
    url = f"{base_url}{endpoint}"
    started = time.perf_counter()
    try:
        request = urllib.request.Request(url, headers={"User-Agent": contract_version})
        with urllib.request.urlopen(request, timeout=request_timeout_seconds) as response:
            body = response.read()
            status = response.status
            content_type = response.headers.get("content-type") or ""
        return {
            "endpoint": endpoint,
            "sequence": sequence,
            "ok": status == 200,
            "status": status,
            "seconds": round(time.perf_counter() - started, 6),
            "bytes": len(body),
            "content_type": content_type,
            "error": None,
        }
    except Exception as error:  # pragma: no cover - diagnostic path for shell gate
        status = getattr(error, "code", None)
        return {
            "endpoint": endpoint,
            "sequence": sequence,
            "ok": False,
            "status": status,
            "seconds": round(time.perf_counter() - started, 6),
            "bytes": 0,
            "content_type": "",
            "error": repr(error),
        }

def percentile(sorted_values, percentile_value):
    if not sorted_values:
        return None
    if len(sorted_values) == 1:
        return sorted_values[0]
    rank = (len(sorted_values) - 1) * percentile_value
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return sorted_values[int(rank)]
    fraction = rank - lower
    return sorted_values[lower] * (1 - fraction) + sorted_values[upper] * fraction

# Warm once per endpoint so the gate measures steady interactive behavior while still
# proving the same cache serves concurrent /health and /metrics probes.
warmups = [fetch(endpoint, 0) for endpoint in endpoints]

tasks = []
for sequence in range(1, requests_per_endpoint + 1):
    for endpoint in endpoints:
        tasks.append((endpoint, sequence))

started_wall = time.perf_counter()
results = []
with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
    futures = [executor.submit(fetch, endpoint, sequence) for endpoint, sequence in tasks]
    for future in concurrent.futures.as_completed(futures):
        results.append(future.result())
wall_seconds = round(time.perf_counter() - started_wall, 6)

by_endpoint = {}
for endpoint in endpoints:
    endpoint_results = sorted(
        [result for result in results if result["endpoint"] == endpoint],
        key=lambda result: result["sequence"],
    )
    seconds = sorted(result["seconds"] for result in endpoint_results)
    failures = [result for result in endpoint_results if not result["ok"]]
    by_endpoint[endpoint] = {
        "request_count": len(endpoint_results),
        "failure_count": len(failures),
        "statuses": sorted({result["status"] for result in endpoint_results}),
        "min_seconds": round(seconds[0], 6) if seconds else None,
        "median_seconds": round(percentile(seconds, 0.50), 6) if seconds else None,
        "p90_seconds": round(percentile(seconds, 0.90), 6) if seconds else None,
        "p95_seconds": round(percentile(seconds, 0.95), 6) if seconds else None,
        "max_seconds": round(seconds[-1], 6) if seconds else None,
        "target_p95_seconds": p95_target_seconds,
        "target_max_seconds": max_target_seconds,
        "green": (
            len(endpoint_results) == requests_per_endpoint
            and not failures
            and (percentile(seconds, 0.95) or 999.0) <= p95_target_seconds
            and (seconds[-1] if seconds else 999.0) <= max_target_seconds
        ),
        "failures": failures[:10],
    }

summary = {
    "contract_version": contract_version,
    "ok": all(endpoint_summary["green"] for endpoint_summary in by_endpoint.values()),
    "checked_at_epoch": checked_at,
    "consumer_base_url": base_url,
    "config": {
        "requests_per_endpoint": requests_per_endpoint,
        "concurrency": concurrency,
        "p95_target_seconds": p95_target_seconds,
        "max_target_seconds": max_target_seconds,
        "request_timeout_seconds": request_timeout_seconds,
        "warmup_requests_per_endpoint": 1,
    },
    "wall_seconds": wall_seconds,
    "warmups": warmups,
    "endpoints": by_endpoint,
    "summary_reason": "Concurrent steady-state p95 gate for cached /health and /metrics readiness projections under accumulated normalized SQL state.",
}
path = summary_dir / f"health-metrics-load-soak-summary-{checked_at}.json"
summary["summary"] = str(path)
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if not summary["ok"]:
    raise SystemExit(1)
PY
