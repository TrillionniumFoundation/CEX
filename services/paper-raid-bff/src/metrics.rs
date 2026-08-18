use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    extract::{ConnectInfo, MatchedPath, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::Value;

const LATENCY_BUCKETS_SECONDS: [f64; 11] = [
    0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000,
];

#[derive(Clone, Default)]
pub struct Metrics {
    inner: Arc<Mutex<MetricsState>>,
}

#[derive(Default)]
struct MetricsState {
    http: BTreeMap<HttpKey, HttpSeries>,
    hepta_errors: BTreeMap<&'static str, u64>,
    bridge_events: BTreeMap<(&'static str, &'static str), u64>,
    invite_events: BTreeMap<&'static str, u64>,
    product_events: BTreeMap<&'static str, u64>,
    stale_ui_reloads: u64,
    match_queue_depth: u64,
    match_eta_available: u64,
    match_eta_unavailable: u64,
    match_snapshot_available: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct HttpKey {
    method: &'static str,
    route: &'static str,
    status_class: &'static str,
}

#[derive(Clone)]
struct HttpSeries {
    count: u64,
    sum_seconds: f64,
    buckets: [u64; LATENCY_BUCKETS_SECONDS.len()],
}

impl Default for HttpSeries {
    fn default() -> Self {
        Self {
            count: 0,
            sum_seconds: 0.0,
            buckets: [0; LATENCY_BUCKETS_SECONDS.len()],
        }
    }
}

#[derive(Clone, Copy)]
pub enum HeptaErrorKind {
    Transport,
    Protocol,
    Server,
    FinalityUnknown,
    FinalityUnavailable,
    FinalityError,
    FinalityInvalid,
}

#[derive(Clone, Copy)]
pub enum InviteOutcome {
    Authenticated,
    Redeemed,
    Denied,
    RateLimited,
    Error,
}

impl Metrics {
    pub fn observe_hepta_error(&self, kind: HeptaErrorKind) {
        let kind = match kind {
            HeptaErrorKind::Transport => "transport",
            HeptaErrorKind::Protocol => "protocol",
            HeptaErrorKind::Server => "server",
            HeptaErrorKind::FinalityUnknown => "finality_unknown",
            HeptaErrorKind::FinalityUnavailable => "finality_unavailable",
            HeptaErrorKind::FinalityError => "finality_error",
            HeptaErrorKind::FinalityInvalid => "finality_invalid",
        };
        let mut state = self.lock();
        *state.hepta_errors.entry(kind).or_default() += 1;
    }

    pub fn observe_finality_availability(&self, projection: &Value) {
        match projection.get("status").and_then(Value::as_str) {
            Some("unknown_finality") => self.observe_hepta_error(HeptaErrorKind::FinalityUnknown),
            Some("unavailable_finality") => {
                self.observe_hepta_error(HeptaErrorKind::FinalityUnavailable)
            }
            Some("error_finality") => self.observe_hepta_error(HeptaErrorKind::FinalityError),
            Some("pending_finality" | "verified_finality") => {}
            _ => self.observe_hepta_error(HeptaErrorKind::FinalityInvalid),
        }
    }

    pub fn observe_invite(&self, outcome: InviteOutcome) {
        let outcome = match outcome {
            InviteOutcome::Authenticated => "authenticated",
            InviteOutcome::Redeemed => "redeemed",
            InviteOutcome::Denied => "denied",
            InviteOutcome::RateLimited => "rate_limited",
            InviteOutcome::Error => "error",
        };
        let mut state = self.lock();
        *state.invite_events.entry(outcome).or_default() += 1;
    }

    pub fn observe_bridge_recovery(&self, succeeded: bool) {
        self.observe_bridge(
            "recovery",
            if succeeded { "success" } else { "server_error" },
        );
    }

    pub fn observe_product_event(&self, event: &str) {
        let event = match event {
            "login_succeeded" => "login_succeeded",
            "first_action" => "first_action",
            _ => return,
        };
        let mut state = self.lock();
        *state.product_events.entry(event).or_default() += 1;
    }

    pub fn observe_stale_ui_reload(&self) {
        self.lock().stale_ui_reloads += 1;
    }

    pub fn observe_match_queue(&self, tickets: Option<&Value>) {
        let Some(tickets) = tickets.and_then(Value::as_array) else {
            self.lock().match_snapshot_available = false;
            return;
        };
        let mut depth = 0_u64;
        let mut eta_available = 0_u64;
        let mut eta_unavailable = 0_u64;
        for ticket in tickets {
            if ticket.get("status").and_then(Value::as_str) != Some("queued") {
                continue;
            }
            depth += 1;
            if ticket
                .get("queue_hint")
                .and_then(|hint| hint.get("eta_seconds"))
                .and_then(Value::as_u64)
                .is_some()
            {
                eta_available += 1;
            } else {
                eta_unavailable += 1;
            }
        }
        let mut state = self.lock();
        state.match_queue_depth = depth;
        state.match_eta_available = eta_available;
        state.match_eta_unavailable = eta_unavailable;
        state.match_snapshot_available = true;
    }

    fn observe_http(
        &self,
        method: &'static str,
        route: &'static str,
        status_class: &'static str,
        elapsed_seconds: f64,
    ) {
        let mut state = self.lock();
        let series = state
            .http
            .entry(HttpKey {
                method,
                route,
                status_class,
            })
            .or_default();
        series.count += 1;
        series.sum_seconds += elapsed_seconds;
        for (index, upper_bound) in LATENCY_BUCKETS_SECONDS.iter().enumerate() {
            if elapsed_seconds <= *upper_bound {
                series.buckets[index] += 1;
            }
        }
    }

    fn observe_bridge(&self, operation: &'static str, outcome: &'static str) {
        let mut state = self.lock();
        *state.bridge_events.entry((operation, outcome)).or_default() += 1;
    }

    pub fn render(&self) -> String {
        let state = self.lock();
        let mut output = String::new();
        output.push_str("# HELP paper_raid_bff_http_requests_total Completed HTTP requests by bounded route, method, and status class.\n");
        output.push_str("# TYPE paper_raid_bff_http_requests_total counter\n");
        output.push_str("# HELP paper_raid_bff_http_request_duration_seconds HTTP request duration by bounded route, method, and status class.\n");
        output.push_str("# TYPE paper_raid_bff_http_request_duration_seconds histogram\n");
        for (key, series) in &state.http {
            let labels = format!(
                "method=\"{}\",route=\"{}\",status_class=\"{}\"",
                key.method, key.route, key.status_class
            );
            output.push_str(&format!(
                "paper_raid_bff_http_requests_total{{{labels}}} {}\n",
                series.count
            ));
            for (index, bound) in LATENCY_BUCKETS_SECONDS.iter().enumerate() {
                output.push_str(&format!(
                    "paper_raid_bff_http_request_duration_seconds_bucket{{{labels},le=\"{}\"}} {}\n",
                    prometheus_float(*bound),
                    series.buckets[index]
                ));
            }
            output.push_str(&format!(
                "paper_raid_bff_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}\n",
                series.count
            ));
            output.push_str(&format!(
                "paper_raid_bff_http_request_duration_seconds_sum{{{labels}}} {:.6}\n",
                series.sum_seconds
            ));
            output.push_str(&format!(
                "paper_raid_bff_http_request_duration_seconds_count{{{labels}}} {}\n",
                series.count
            ));
        }
        output.push_str("# HELP paper_raid_bff_hepta_upstream_errors_total Hepta transport, protocol, server, and finality availability failures.\n");
        output.push_str("# TYPE paper_raid_bff_hepta_upstream_errors_total counter\n");
        for kind in [
            "transport",
            "protocol",
            "server",
            "finality_unknown",
            "finality_unavailable",
            "finality_error",
            "finality_invalid",
        ] {
            let count = state.hepta_errors.get(kind).copied().unwrap_or(0);
            output.push_str(&format!(
                "paper_raid_bff_hepta_upstream_errors_total{{kind=\"{kind}\"}} {count}\n"
            ));
        }
        output.push_str("# HELP paper_raid_bff_agent_bridge_events_total Agent Bridge pairing, work submission, and recovery outcomes.\n");
        output.push_str("# TYPE paper_raid_bff_agent_bridge_events_total counter\n");
        for operation in ["pairing", "work_submit", "recovery"] {
            for outcome in ["success", "client_error", "conflict", "server_error"] {
                let count = state
                    .bridge_events
                    .get(&(operation, outcome))
                    .copied()
                    .unwrap_or(0);
                output.push_str(&format!(
                    "paper_raid_bff_agent_bridge_events_total{{operation=\"{operation}\",outcome=\"{outcome}\"}} {count}\n"
                ));
            }
        }
        output.push_str("# HELP paper_raid_bff_invite_authentication_total Closed-Alpha credential authentication and invite redemption outcomes.\n");
        output.push_str("# TYPE paper_raid_bff_invite_authentication_total counter\n");
        for outcome in [
            "authenticated",
            "redeemed",
            "denied",
            "rate_limited",
            "error",
        ] {
            let count = state.invite_events.get(outcome).copied().unwrap_or(0);
            output.push_str(&format!(
                "paper_raid_bff_invite_authentication_total{{outcome=\"{outcome}\"}} {count}\n"
            ));
        }
        output.push_str("# HELP paper_raid_bff_product_events_total Aggregate onboarding events used by credential-free synthetic/funnel monitoring.\n");
        output.push_str("# TYPE paper_raid_bff_product_events_total counter\n");
        for event in ["login_succeeded", "first_action"] {
            let count = state.product_events.get(event).copied().unwrap_or(0);
            output.push_str(&format!(
                "paper_raid_bff_product_events_total{{event=\"{event}\"}} {count}\n"
            ));
        }
        output.push_str("# HELP paper_raid_bff_stale_ui_reloads_total Browser reloads caused by an authoritative teammate mutation.\n");
        output.push_str("# TYPE paper_raid_bff_stale_ui_reloads_total counter\n");
        output.push_str(&format!(
            "paper_raid_bff_stale_ui_reloads_total {}\n",
            state.stale_ui_reloads
        ));
        output.push_str("# HELP paper_raid_bff_match_queue_depth Queued tickets in the last authoritative lobby snapshot.\n");
        output.push_str("# TYPE paper_raid_bff_match_queue_depth gauge\n");
        output.push_str(&format!(
            "paper_raid_bff_match_queue_depth {}\n",
            state.match_queue_depth
        ));
        output.push_str("# HELP paper_raid_bff_match_queue_eta_tickets Tickets with and without an authoritative ETA in the last lobby snapshot.\n");
        output.push_str("# TYPE paper_raid_bff_match_queue_eta_tickets gauge\n");
        output.push_str(&format!(
            "paper_raid_bff_match_queue_eta_tickets{{availability=\"available\"}} {}\n",
            state.match_eta_available
        ));
        output.push_str(&format!(
            "paper_raid_bff_match_queue_eta_tickets{{availability=\"unavailable\"}} {}\n",
            state.match_eta_unavailable
        ));
        output.push_str("# HELP paper_raid_bff_match_queue_snapshot_available Whether the latest lobby read produced an authoritative ticket snapshot.\n");
        output.push_str("# TYPE paper_raid_bff_match_queue_snapshot_available gauge\n");
        output.push_str(&format!(
            "paper_raid_bff_match_queue_snapshot_available {}\n",
            u8::from(state.match_snapshot_available)
        ));
        output
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MetricsState> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub async fn observe_http(
    State(metrics): State<Metrics>,
    request: Request,
    next: Next,
) -> Response {
    let method = bounded_method(request.method().as_str());
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .and_then(bounded_route)
        .unwrap_or("unmatched");
    let started = Instant::now();
    let response = next.run(request).await;
    let status_class = bounded_status_class(response.status());
    metrics.observe_http(method, route, status_class, started.elapsed().as_secs_f64());
    match route {
        "/api/agent-bridge/pair" => {
            metrics.observe_bridge("pairing", bridge_outcome(response.status()))
        }
        "/api/agent-bridge/proposals" => {
            metrics.observe_bridge("work_submit", bridge_outcome(response.status()))
        }
        _ => {}
    }
    response
}

pub fn loopback_metrics_response(
    metrics: &Metrics,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !peer.ip().is_loopback() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = metrics.render().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn bounded_method(method: &str) -> &'static str {
    match method {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        _ => "OTHER",
    }
}

fn bounded_status_class(status: StatusCode) -> &'static str {
    match status.as_u16() / 100 {
        2 => "2xx",
        3 => "3xx",
        4 => "4xx",
        5 => "5xx",
        _ => "other",
    }
}

fn bridge_outcome(status: StatusCode) -> &'static str {
    if status.is_success() {
        "success"
    } else if status == StatusCode::CONFLICT {
        "conflict"
    } else if status.is_client_error() {
        "client_error"
    } else {
        "server_error"
    }
}

fn bounded_route(route: &str) -> Option<&'static str> {
    Some(match route {
        "/health" => "/health",
        "/ready" => "/ready",
        "/metrics" => "/metrics",
        "/login" => "/login",
        "/assets/paper-raid.js" => "/assets/paper-raid.js",
        "/assets/paper-raid.css" => "/assets/paper-raid.css",
        "/alpha/login" => "/alpha/login",
        "/session/logout" => "/session/logout",
        "/session/refresh" => "/session/refresh",
        "/api/session" => "/api/session",
        "/api/onboarding/human/challenge" => "/api/onboarding/human/challenge",
        "/api/onboarding/human/register" => "/api/onboarding/human/register",
        "/api/onboarding/human/signing-frame" => "/api/onboarding/human/signing-frame",
        "/api/hepta/commands" => "/api/hepta/commands",
        "/api/agent-bindings" => "/api/agent-bindings",
        "/api/agent-bridge/pairing-grants" => "/api/agent-bridge/pairing-grants",
        "/api/agent-bridge/pairing-grants/:grant_id/revoke" => {
            "/api/agent-bridge/pairing-grants/:grant_id/revoke"
        }
        "/api/agent-bridge/pairing-context" => "/api/agent-bridge/pairing-context",
        "/api/agent-bridge/pair" => "/api/agent-bridge/pair",
        "/api/agent-bridge/binding" => "/api/agent-bridge/binding",
        "/api/agent-bridge/health" => "/api/agent-bridge/health",
        "/api/agent-bridge/inbox" => "/api/agent-bridge/inbox",
        "/api/agent-bridge/delivery-drafts" => "/api/agent-bridge/delivery-drafts",
        "/api/agent-bridge/proposals" => "/api/agent-bridge/proposals",
        "/api/product-events" => "/api/product-events",
        "/api/practice/session" => "/api/practice/session",
        "/api/practice/start" => "/api/practice/start",
        "/api/practice/advance" => "/api/practice/advance",
        "/api/practice/abandon" => "/api/practice/abandon",
        "/api/papers/:paper_id/timeline" => "/api/papers/:paper_id/timeline",
        "/api/papers/:paper_id/outcome" => "/api/papers/:paper_id/outcome",
        "/api/papers/:paper_id/artifacts/:digest" => "/api/papers/:paper_id/artifacts/:digest",
        "/league/start" => "/league/start",
        "/league/onboarding" => "/league/onboarding",
        "/league" => "/league",
        "/league/review" => "/league/review",
        "/league/review/:paper_id" => "/league/review/:paper_id",
        "/league/practice" => "/league/practice",
        "/league/formation/:team_id" => "/league/formation/:team_id",
        "/league/papers/:paper_id" => "/league/papers/:paper_id",
        _ => return None,
    })
}

fn prometheus_float(value: f64) -> String {
    let value = format!("{value:.3}");
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_labels_are_fixed_and_never_accept_identifiers() {
        assert_eq!(
            bounded_route("/api/papers/:paper_id/timeline"),
            Some("/api/papers/:paper_id/timeline")
        );
        assert_eq!(
            bounded_route("/api/papers/7c1b6b2b-9a68-4674-a44a-a90f52203f7f/timeline"),
            None
        );
        for route in [
            "/league/practice",
            "/api/practice/session",
            "/api/practice/start",
            "/api/practice/advance",
            "/api/practice/abandon",
        ] {
            assert_eq!(bounded_route(route), Some(route));
        }
    }

    #[test]
    fn render_uses_only_bounded_aggregate_labels() {
        let metrics = Metrics::default();
        metrics.observe_http("GET", "/health", "2xx", 0.012);
        metrics.observe_hepta_error(HeptaErrorKind::FinalityUnavailable);
        metrics.observe_bridge("recovery", "success");
        metrics.observe_invite(InviteOutcome::Redeemed);
        metrics.observe_match_queue(Some(&serde_json::json!([
            {"status":"queued","queue_hint":{"eta_seconds":42}},
            {"status":"queued","queue_hint":{"eta_seconds":null}},
            {"status":"matched","queue_hint":{"eta_seconds":0}}
        ])));
        let rendered = metrics.render();
        assert!(rendered.contains("route=\"/health\""));
        assert!(rendered.contains("kind=\"finality_unavailable\""));
        assert!(rendered.contains("operation=\"recovery\",outcome=\"success\""));
        assert!(rendered.contains("outcome=\"redeemed\""));
        assert!(rendered.contains("paper_raid_bff_match_queue_depth 2"));
        assert!(!rendered.contains("player_id"));
        assert!(!rendered.contains("paper_id="));
        assert!(!rendered.contains("agent_id"));
    }

    #[test]
    fn histogram_buckets_are_cumulative() {
        let metrics = Metrics::default();
        metrics.observe_http("GET", "/health", "2xx", 0.012);
        let rendered = metrics.render();
        assert!(rendered.contains("le=\"0.01\"} 0"));
        assert!(rendered.contains("le=\"0.025\"} 1"));
        assert!(rendered.contains("le=\"+Inf\"} 1"));
    }
}
