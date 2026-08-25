#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
metrics=$service_root/src/metrics.rs
app=$service_root/src/app.rs
main=$service_root/src/main.rs
browser=$service_root/src/browser.js

for file in "$metrics" "$app" "$main" "$browser"; do
  [[ -f $file && ! -L $file ]] || {
    echo "observability boundary requires a regular source file: $file" >&2
    exit 1
  }
done

for required in \
  'paper_raid_bff_http_requests_total' \
  'paper_raid_bff_http_request_duration_seconds' \
  'paper_raid_bff_hepta_upstream_errors_total' \
  'paper_raid_bff_match_queue_depth' \
  'paper_raid_bff_match_queue_eta_tickets' \
  'paper_raid_bff_agent_bridge_events_total' \
  'paper_raid_bff_stale_ui_reloads_total' \
  'paper_raid_bff_invite_authentication_total' \
  'fn bounded_route(' \
  'fn bounded_method(' \
  'fn bounded_status_class('
do
  rg -q --fixed-strings -- "$required" "$metrics" || {
    echo "observability boundary is missing: $required" >&2
    exit 1
  }
done

for forbidden in \
  'request.uri().path()' \
  'player_id=\"' \
  'paper_id=\"' \
  'agent_id=\"' \
  'binding_id=\"' \
  'subject_id=\"' \
  'challenge_id=\"' \
  'team_id=\"'
do
  if rg -q --fixed-strings -- "$forbidden" "$metrics"; then
    echo "observability boundary contains an identifier/high-cardinality path: $forbidden" >&2
    exit 1
  fi
done

for required in \
  '.route("/metrics", get(operator_metrics))' \
  'loopback_metrics_response(&state.metrics, peer)' \
  'into_make_service_with_connect_info::<std::net::SocketAddr>()'
do
  if ! rg -q --fixed-strings -- "$required" "$app" "$main"; then
    echo "loopback-only metrics exposure boundary is missing: $required" >&2
    exit 1
  fi
done

rg -q --fixed-strings 'recordProductEvent("stale_ui_reload", { paperId })' "$browser" || {
  echo "stale-authority reload is not represented by an aggregate browser signal" >&2
  exit 1
}

# Replay progression is intentionally a bounded, identifier-only browser
# signal. It must remain best-effort and must not grow into authority/hash
# material or a second replay command path.
replay_begin=$(sed -n '/function createTimelineReplayController(/,/const stop = () => {/p' "$browser")
grep -q -- 'recordProductEvent("replay_started", {' <<<"$replay_begin" || {
  echo "read-only replay is missing its durable identifier-only milestone" >&2
  exit 1
}
grep -q -- 'paperId: card.dataset.paperId || null' <<<"$replay_begin" || {
  echo "replay milestone is missing the existing paper scope" >&2
  exit 1
}
grep -q -- '}).catch(() => {});' <<<"$replay_begin" || {
  echo "replay milestone is not best-effort" >&2
  exit 1
}
for forbidden in authority hash digest signature 'uuid('; do
  if grep -q -- "$forbidden" <<<"$replay_begin"; then
    echo "replay milestone contains forbidden authority material: $forbidden" >&2
    exit 1
  fi
done

# Readiness remains an authority/dependency gate. Metrics are intentionally
# absent from the ready join and predicate, so monitoring failure cannot make
# the application advertise a false domain-readiness state.
ready_block=$(sed -n '/async fn ready(/,/async fn login_page()/p' "$app")
if grep -q 'metrics' <<<"$ready_block"; then
  echo "readiness incorrectly depends on the metrics path" >&2
  exit 1
fi

echo "paper-raid-bff bounded observability boundary: ok"
