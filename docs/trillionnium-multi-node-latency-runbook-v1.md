# Trillionnium Multi-Node / Live-Traffic Latency Runbook v1

Purpose: collect **real multi-node or live-traffic latency evidence** for Trillionnium World before claiming technical playability above 9.8. Local single-node soak, browser E2E, and synthetic template files are intentionally insufficient for this band.

## What this gate proves

The existing local-production gates prove the product can run the first session and keep `/health` + `/metrics` fast on one machine. This gate proves one of the following higher bars:

1. at least two real nodes, each running production-like Trillionnium services, meet `/health` and `/metrics` latency/error budgets; or
2. a real live-traffic window meets aggregate `/health` and `/metrics` latency/error budgets.

This is the evidence required before the technical playability assessment can move from 9.8 toward 9.9+.

## Non-negotiables

- Do not use localhost-only or single-node local soak as evidence for this gate.
- Keep raw logs/probe output retained outside the sanitized evidence file.
- Do not put secrets, tokens, IP allowlist details, personal data, or user identifiers in the evidence file.
- Redact node URLs if needed, but keep stable node ids and region/site labels.
- Synthetic/browser E2E summaries may support debugging, but they do not satisfy this gate by themselves.
- Keep Rust as source of truth; do not enable live OSM ingestion or promote MapLibre as part of this proof.

## Run the gate

Save the evidence JSON, then run:

```bash
CEX_ENV_FILE=run/local-production/.env \
TRILLIONNIUM_MULTI_NODE_LATENCY_EVIDENCE_PATH=run/multi-node-latency/evidence-YYYYMMDD.json \
scripts/check-trillionnium-multi-node-latency-evidence.sh
```

The script writes:

```text
run/multi-node-latency/multi-node-latency-summary-<epoch>.json
```

Default green thresholds:

- node count for multi-node mode: >= 2
- samples per endpoint: >= 100
- `/health` p95: <= 1.0s
- `/health` p99: <= 2.0s
- `/metrics` p95: <= 1.0s
- `/metrics` p99: <= 2.0s
- endpoint error rate: <= 0.001
- live-traffic window, if used: >= 30 minutes and >= 1000 requests

## Evidence schema: multi-node probe

Use this shape for two or more real nodes. Probe traffic may be synthetic, but the environment and measurements must be real, non-localhost, and production-like.

```json
{
  "contract_version": "trillionnium_multi_node_latency_evidence_v1",
  "evidence_type": "multi_node",
  "template": false,
  "fabricated": false,
  "localhost_only": false,
  "operator_attestation": {
    "real_multi_node_or_live_traffic_environment": true,
    "production_like_config": true,
    "no_localhost_only_measurement": true,
    "raw_logs_retained": true,
    "no_secrets_or_personal_data_in_evidence": true,
    "notes": "Sanitized summary; raw probe logs retained by operator."
  },
  "nodes": [
    {
      "node_id": "node-a",
      "role": "consumer-entry-api",
      "site_or_region": "region-a",
      "runtime_profile": "production",
      "base_url_redacted": "https://node-a.example.invalid",
      "localhost_only": false,
      "endpoints": {
        "/health": {
          "samples": 240,
          "p95_seconds": 0.21,
          "p99_seconds": 0.42,
          "error_rate": 0
        },
        "/metrics": {
          "samples": 240,
          "p95_seconds": 0.18,
          "p99_seconds": 0.39,
          "error_rate": 0
        }
      }
    },
    {
      "node_id": "node-b",
      "role": "consumer-entry-api",
      "site_or_region": "region-b",
      "runtime_profile": "production",
      "base_url_redacted": "https://node-b.example.invalid",
      "localhost_only": false,
      "endpoints": {
        "/health": {
          "samples": 240,
          "p95_seconds": 0.24,
          "p99_seconds": 0.48,
          "error_rate": 0
        },
        "/metrics": {
          "samples": 240,
          "p95_seconds": 0.19,
          "p99_seconds": 0.41,
          "error_rate": 0
        }
      }
    }
  ]
}
```

## Evidence schema: live traffic

Use this shape for a real traffic window. It may be single-node only if the traffic is real and the aggregate metrics meet the budget.

```json
{
  "contract_version": "trillionnium_multi_node_latency_evidence_v1",
  "evidence_type": "live_traffic",
  "template": false,
  "fabricated": false,
  "localhost_only": false,
  "operator_attestation": {
    "real_multi_node_or_live_traffic_environment": true,
    "production_like_config": true,
    "no_localhost_only_measurement": true,
    "raw_logs_retained": true,
    "no_secrets_or_personal_data_in_evidence": true
  },
  "traffic_window": {
    "started_at": "YYYY-MM-DDTHH:MM:SSZ",
    "duration_minutes": 60,
    "traffic_source": "production_user_traffic",
    "live_request_count": 2500,
    "real_user_sessions": 25
  },
  "aggregate": {
    "/health": {
      "samples": 2500,
      "p95_seconds": 0.28,
      "p99_seconds": 0.7,
      "error_rate": 0
    },
    "/metrics": {
      "samples": 600,
      "p95_seconds": 0.25,
      "p99_seconds": 0.65,
      "error_rate": 0
    }
  }
}
```

## How to use failures

- Node-specific latency failure -> inspect that node's database/cache/readiness bundle and compare with the green node.
- Aggregate live-traffic failure -> segment by endpoint, deploy version, provider failures, cache generation, and request fan-out.
- Error-rate failure -> treat as a reliability blocker, not a scoring nuance.
- Missing attestation/raw logs -> rerun the measurement; do not paper over provenance gaps.

Do not raise technical playability above 9.8 until this gate is green with real multi-node or live-traffic evidence.
