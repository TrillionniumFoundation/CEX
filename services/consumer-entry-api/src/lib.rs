#![recursion_limit = "512"]
#![allow(clippy::result_large_err, clippy::too_many_arguments)]

use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const DEFAULT_MAX_TEXT_CHARS: usize = 4_000;
const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const DEFAULT_RATE_LIMIT_MAX_REQUESTS: usize = 30;
const DEFAULT_REPLAY_WINDOW_SECS: u64 = 600;
const DEFAULT_REPLAY_CACHE_SIZE: usize = 2048;
const DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS: u64 = 2500;
const DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS: u64 = 3600;
const DEFAULT_SESSION_AUTH_MAX_CLOCK_SKEW_SECS: u64 = 300;
const DEFAULT_SESSION_AUTH_MAX_TTL_SECS: u64 = 900;
const WORLD_MAP_RUM_RECENT_WINDOW: usize = 512;
const USER_SESSION_ASSERTION_HEADER: &str = "x-cex-user-session";
const USER_SESSION_SIGNATURE_HEADER: &str = "x-cex-user-session-signature";
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs,
    path::Path as StdPath,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock as StdRwLock,
    },
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};

mod world_indexes;
use world_indexes::{build_world_indexes, indexed_recent, indexed_sorted, WorldIndexes};
mod world_route_projection;
use world_route_projection::*;
mod world_map_optimization;
use world_map_optimization::*;
mod real_world_map_shell;
use real_world_map_shell::*;
mod openstreetmap_geodata;
use openstreetmap_geodata::*;
mod world_tactics;
use world_tactics::*;
mod world_map_projection;
use world_map_projection::*;
mod league_core;
use league_core::*;
mod league_repository;
use league_repository::*;
mod identity_admin_routes;
use identity_admin_routes::*;
mod consumer_ingress;
use consumer_ingress::*;
mod task_routes;
use task_routes::*;
mod health_metrics;
use health_metrics::*;
mod world_routes;
use world_routes::*;
mod world_commerce_routes;
use world_commerce_routes::*;
mod world_web_shell;
use world_web_shell::*;
mod league_routes;
use league_routes::*;
mod client_app_shell;
use client_app_shell::*;
mod client_surfaces;
use client_surfaces::*;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    http: Client,
    config: ConsumerEntryConfig,
    identity_binding_store: RwLock<IdentityBindingStore>,
    identity_binding_audit_state: RwLock<IdentityBindingAuditState>,
    session_auth_issuer_registry_state: StdRwLock<SessionAuthIssuerRegistryRuntimeState>,
    league_state: Mutex<LeagueState>,
    health_world_readiness_cache_generation: AtomicU64,
    health_world_readiness_cache: Mutex<Option<HealthWorldReadinessBundleCache>>,
    rate_limits: Mutex<RateLimitCache>,
    replay_cache: Mutex<ReplayCache>,
    metrics: ConsumerEntryMetrics,
}

#[derive(Debug, Clone)]
struct WorldMapRumObservation {
    surface_class: String,
    device_class: String,
    sample_kind_class: String,
    first_map_interactive_ms: Option<u64>,
    viewport_refresh_ms: Option<u64>,
    focus_to_action_rail_ms: Option<u64>,
    main_thread_long_task_ms: Option<u64>,
    tile_error_count: u64,
}

#[derive(Debug, Default)]
struct ConsumerEntryMetrics {
    task_create_requests: AtomicU64,
    task_lookup_requests: AtomicU64,
    rate_limited_requests: AtomicU64,
    rate_limited_source_scope_requests: AtomicU64,
    rate_limited_user_requests: AtomicU64,
    rate_limited_room_requests: AtomicU64,
    rate_limited_session_requests: AtomicU64,
    rate_limited_org_requests: AtomicU64,
    identity_binding_failures: AtomicU64,
    identity_binding_matches: AtomicU64,
    identity_binding_reload_requests: AtomicU64,
    identity_binding_reload_successes: AtomicU64,
    identity_binding_reload_rejections: AtomicU64,
    identity_binding_reload_actor_rejections: AtomicU64,
    identity_binding_audit_failures: AtomicU64,
    ingress_auth_failures: AtomicU64,
    session_auth_successes: AtomicU64,
    session_auth_failures: AtomicU64,
    replay_hits: AtomicU64,
    world_map_rum_samples: AtomicU64,
    world_map_rum_first_interactive_ms_sum: AtomicU64,
    world_map_rum_first_interactive_ms_max: AtomicU64,
    world_map_rum_viewport_refresh_ms_sum: AtomicU64,
    world_map_rum_viewport_refresh_ms_max: AtomicU64,
    world_map_rum_focus_to_action_ms_sum: AtomicU64,
    world_map_rum_focus_to_action_ms_max: AtomicU64,
    world_map_rum_long_task_ms_sum: AtomicU64,
    world_map_rum_long_task_ms_max: AtomicU64,
    world_map_rum_tile_errors: AtomicU64,
    world_map_rum_recent: std::sync::Mutex<VecDeque<WorldMapRumObservation>>,
    world_map_delta_requests: AtomicU64,
    world_map_delta_noop_responses: AtomicU64,
    world_map_delta_snapshot_fallbacks: AtomicU64,
    world_map_delta_failures: AtomicU64,
}

impl ConsumerEntryMetrics {
    fn inc_task_create_requests(&self) {
        self.task_create_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_task_lookup_requests(&self) {
        self.task_lookup_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_requests(&self) {
        self.rate_limited_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_source_scope_requests(&self) {
        self.rate_limited_source_scope_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_user_requests(&self) {
        self.rate_limited_user_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_room_requests(&self) {
        self.rate_limited_room_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_session_requests(&self) {
        self.rate_limited_session_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_org_requests(&self) {
        self.rate_limited_org_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_failures(&self) {
        self.identity_binding_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_matches(&self) {
        self.identity_binding_matches
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_requests(&self) {
        self.identity_binding_reload_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_successes(&self) {
        self.identity_binding_reload_successes
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_rejections(&self) {
        self.identity_binding_reload_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_audit_failures(&self) {
        self.identity_binding_audit_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_actor_rejections(&self) {
        self.identity_binding_reload_actor_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_ingress_auth_failures(&self) {
        self.ingress_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_session_auth_successes(&self) {
        self.session_auth_successes.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_session_auth_failures(&self) {
        self.session_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_replay_hits(&self) {
        self.replay_hits.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_world_map_delta_requests(&self) {
        self.world_map_delta_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_world_map_delta_noop_responses(&self) {
        self.world_map_delta_noop_responses
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_world_map_delta_snapshot_fallbacks(&self) {
        self.world_map_delta_snapshot_fallbacks
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_world_map_rum(&self, sample: &WorldMapRumRequest) {
        self.world_map_rum_samples.fetch_add(1, Ordering::Relaxed);
        if let Some(value) = sample.first_map_interactive_ms {
            let value = value.min(60_000);
            self.world_map_rum_first_interactive_ms_sum
                .fetch_add(value, Ordering::Relaxed);
            atomic_max(&self.world_map_rum_first_interactive_ms_max, value);
        }
        if let Some(value) = sample.viewport_refresh_ms {
            let value = value.min(60_000);
            self.world_map_rum_viewport_refresh_ms_sum
                .fetch_add(value, Ordering::Relaxed);
            atomic_max(&self.world_map_rum_viewport_refresh_ms_max, value);
        }
        if let Some(value) = sample.focus_to_action_rail_ms {
            let value = value.min(60_000);
            self.world_map_rum_focus_to_action_ms_sum
                .fetch_add(value, Ordering::Relaxed);
            atomic_max(&self.world_map_rum_focus_to_action_ms_max, value);
        }
        if let Some(value) = sample.main_thread_long_task_ms {
            let value = value.min(60_000);
            self.world_map_rum_long_task_ms_sum
                .fetch_add(value, Ordering::Relaxed);
            atomic_max(&self.world_map_rum_long_task_ms_max, value);
        }
        self.world_map_rum_tile_errors.fetch_add(
            sample.tile_error_count.unwrap_or(0).min(10_000),
            Ordering::Relaxed,
        );
        if let Ok(mut recent) = self.world_map_rum_recent.lock() {
            recent.push_back(WorldMapRumObservation::from_request(sample));
            while recent.len() > WORLD_MAP_RUM_RECENT_WINDOW {
                recent.pop_front();
            }
        }
    }

    fn snapshot(&self) -> Value {
        json!({
            "task_create_requests": self.task_create_requests.load(Ordering::Relaxed),
            "task_lookup_requests": self.task_lookup_requests.load(Ordering::Relaxed),
            "rate_limited_requests": self.rate_limited_requests.load(Ordering::Relaxed),
            "rate_limited_source_scope_requests": self.rate_limited_source_scope_requests.load(Ordering::Relaxed),
            "rate_limited_user_requests": self.rate_limited_user_requests.load(Ordering::Relaxed),
            "rate_limited_room_requests": self.rate_limited_room_requests.load(Ordering::Relaxed),
            "rate_limited_session_requests": self.rate_limited_session_requests.load(Ordering::Relaxed),
            "rate_limited_org_requests": self.rate_limited_org_requests.load(Ordering::Relaxed),
            "identity_binding_failures": self.identity_binding_failures.load(Ordering::Relaxed),
            "identity_binding_matches": self.identity_binding_matches.load(Ordering::Relaxed),
            "identity_binding_reload_requests": self.identity_binding_reload_requests.load(Ordering::Relaxed),
            "identity_binding_reload_successes": self.identity_binding_reload_successes.load(Ordering::Relaxed),
            "identity_binding_reload_rejections": self.identity_binding_reload_rejections.load(Ordering::Relaxed),
            "identity_binding_reload_actor_rejections": self.identity_binding_reload_actor_rejections.load(Ordering::Relaxed),
            "identity_binding_audit_failures": self.identity_binding_audit_failures.load(Ordering::Relaxed),
            "ingress_auth_failures": self.ingress_auth_failures.load(Ordering::Relaxed),
            "session_auth_successes": self.session_auth_successes.load(Ordering::Relaxed),
            "session_auth_failures": self.session_auth_failures.load(Ordering::Relaxed),
            "replay_hits": self.replay_hits.load(Ordering::Relaxed),
            "world_map_rum": self.world_map_rum_snapshot(),
            "world_map_delta": self.world_map_delta_snapshot(),
        })
    }

    fn world_map_delta_snapshot(&self) -> Value {
        let requests = self.world_map_delta_requests.load(Ordering::Relaxed);
        let noop_responses = self.world_map_delta_noop_responses.load(Ordering::Relaxed);
        let snapshot_fallbacks = self
            .world_map_delta_snapshot_fallbacks
            .load(Ordering::Relaxed);
        let failures = self.world_map_delta_failures.load(Ordering::Relaxed);
        let percent = |value: u64| {
            if requests == 0 {
                0
            } else {
                ((value as f64 / requests.max(1) as f64) * 100.0).round() as u64
            }
        };
        json!({
            "contract_version": TRILLIONNIUM_WORLD_MAP_TRANSPORT_DELTA_CONTRACT_VERSION,
            "requests": requests,
            "noop_responses": noop_responses,
            "snapshot_fallbacks": snapshot_fallbacks,
            "failures": failures,
            "noop_rate_percent": percent(noop_responses),
            "snapshot_fallback_rate_percent": percent(snapshot_fallbacks),
            "failure_rate_percent": percent(failures),
            "snapshot_fallback_failure_rate_percent": percent(failures),
            "entity_delta_cache_contract": "entity_group_versioned_delta_v1",
            "failure_rate_within_target": failures == 0,
        })
    }

    fn world_map_rum_snapshot(&self) -> Value {
        let samples = self.world_map_rum_samples.load(Ordering::Relaxed);
        let avg = |sum: &AtomicU64| {
            if samples == 0 {
                0
            } else {
                sum.load(Ordering::Relaxed) / samples.max(1)
            }
        };
        let recent = self
            .world_map_rum_recent
            .lock()
            .map(|samples| samples.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let distributions = world_map_rum_distribution_json(&recent);
        let slo_gate = distributions
            .get("slo_gate")
            .cloned()
            .unwrap_or_else(|| json!({}));
        json!({
            "contract_version": TRILLIONNIUM_WORLD_MAP_RUNTIME_PERFORMANCE_BUDGET_CONTRACT_VERSION,
            "sample_count": samples,
            "first_map_interactive_avg_ms": avg(&self.world_map_rum_first_interactive_ms_sum),
            "first_map_interactive_max_ms": self.world_map_rum_first_interactive_ms_max.load(Ordering::Relaxed),
            "first_map_interactive_p50_ms": distributions.get("global").and_then(|global| global.get("first_map_interactive")).and_then(|metric| metric.get("p50_ms")).and_then(Value::as_u64).unwrap_or(0),
            "first_map_interactive_p95_ms": distributions.get("global").and_then(|global| global.get("first_map_interactive")).and_then(|metric| metric.get("p95_ms")).and_then(Value::as_u64).unwrap_or(0),
            "first_map_interactive_p99_ms": distributions.get("global").and_then(|global| global.get("first_map_interactive")).and_then(|metric| metric.get("p99_ms")).and_then(Value::as_u64).unwrap_or(0),
            "viewport_refresh_avg_ms": avg(&self.world_map_rum_viewport_refresh_ms_sum),
            "viewport_refresh_max_ms": self.world_map_rum_viewport_refresh_ms_max.load(Ordering::Relaxed),
            "viewport_refresh_p50_ms": distributions.get("global").and_then(|global| global.get("viewport_refresh")).and_then(|metric| metric.get("p50_ms")).and_then(Value::as_u64).unwrap_or(0),
            "viewport_refresh_p95_ms": distributions.get("global").and_then(|global| global.get("viewport_refresh")).and_then(|metric| metric.get("p95_ms")).and_then(Value::as_u64).unwrap_or(0),
            "viewport_refresh_p99_ms": distributions.get("global").and_then(|global| global.get("viewport_refresh")).and_then(|metric| metric.get("p99_ms")).and_then(Value::as_u64).unwrap_or(0),
            "focus_to_action_rail_avg_ms": avg(&self.world_map_rum_focus_to_action_ms_sum),
            "focus_to_action_rail_max_ms": self.world_map_rum_focus_to_action_ms_max.load(Ordering::Relaxed),
            "focus_to_action_rail_p50_ms": distributions.get("global").and_then(|global| global.get("focus_to_action_rail")).and_then(|metric| metric.get("p50_ms")).and_then(Value::as_u64).unwrap_or(0),
            "focus_to_action_rail_p95_ms": distributions.get("global").and_then(|global| global.get("focus_to_action_rail")).and_then(|metric| metric.get("p95_ms")).and_then(Value::as_u64).unwrap_or(0),
            "focus_to_action_rail_p99_ms": distributions.get("global").and_then(|global| global.get("focus_to_action_rail")).and_then(|metric| metric.get("p99_ms")).and_then(Value::as_u64).unwrap_or(0),
            "main_thread_long_task_avg_ms": avg(&self.world_map_rum_long_task_ms_sum),
            "main_thread_long_task_max_ms": self.world_map_rum_long_task_ms_max.load(Ordering::Relaxed),
            "tile_error_count": self.world_map_rum_tile_errors.load(Ordering::Relaxed),
            "distributions": distributions,
            "slo_gate": slo_gate,
            "source": "browser_real_user_measurement_endpoint",
        })
    }
}

impl WorldMapRumObservation {
    fn from_request(sample: &WorldMapRumRequest) -> Self {
        Self {
            surface_class: normalize_world_map_rum_surface(sample.surface_id.as_deref()),
            device_class: normalize_world_map_rum_device(sample.user_agent_class.as_deref()),
            sample_kind_class: normalize_world_map_rum_sample_kind(sample.sample_kind.as_deref()),
            first_map_interactive_ms: sample
                .first_map_interactive_ms
                .map(|value| value.min(60_000)),
            viewport_refresh_ms: sample.viewport_refresh_ms.map(|value| value.min(60_000)),
            focus_to_action_rail_ms: sample
                .focus_to_action_rail_ms
                .map(|value| value.min(60_000)),
            main_thread_long_task_ms: sample
                .main_thread_long_task_ms
                .map(|value| value.min(60_000)),
            tile_error_count: sample.tile_error_count.unwrap_or(0).min(10_000),
        }
    }
}

fn normalize_world_map_rum_sample_kind(sample_kind: Option<&str>) -> String {
    let normalized = sample_kind
        .unwrap_or("viewport_refresh")
        .trim()
        .to_ascii_lowercase();
    if normalized.contains("weak")
        || normalized.contains("offline")
        || normalized.contains("cached_snapshot")
        || normalized.contains("network")
    {
        "weak_network_cached_snapshot".to_string()
    } else if normalized.contains("delta")
        || normalized.contains("304")
        || normalized.contains("noop")
        || normalized.contains("viewport_refresh")
    {
        "warm_delta_or_304".to_string()
    } else if normalized.contains("first_map_interactive")
        || normalized.contains("runtime_ready")
        || normalized.contains("snapshot_ready")
    {
        "cold_cache_interactive".to_string()
    } else {
        "warm_delta_or_304".to_string()
    }
}

fn normalize_world_map_rum_surface(surface_id: Option<&str>) -> String {
    let normalized = surface_id.unwrap_or("world").trim().to_ascii_lowercase();
    if normalized.contains("app") {
        "app".to_string()
    } else {
        "world".to_string()
    }
}

fn normalize_world_map_rum_device(user_agent_class: Option<&str>) -> String {
    let normalized = user_agent_class
        .unwrap_or("desktop")
        .trim()
        .to_ascii_lowercase();
    if normalized.contains("mobile")
        || normalized.contains("android")
        || normalized.contains("iphone")
        || normalized.contains("ios")
    {
        "mobile".to_string()
    } else {
        "desktop".to_string()
    }
}

fn percentile_from_sorted(values: &[u64], percentile: u64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let last = values.len() - 1;
    let index = (last as u64 * percentile).div_ceil(100);
    values[index.min(last as u64) as usize]
}

fn world_map_rum_metric_summary_json<F>(
    observations: &[WorldMapRumObservation],
    target_ms: u64,
    extract: F,
) -> Value
where
    F: Fn(&WorldMapRumObservation) -> Option<u64>,
{
    let mut values = observations.iter().filter_map(extract).collect::<Vec<_>>();
    values.sort_unstable();
    let sample_count = values.len() as u64;
    let p50_ms = percentile_from_sorted(&values, 50);
    let p95_ms = percentile_from_sorted(&values, 95);
    let p99_ms = percentile_from_sorted(&values, 99);
    json!({
        "sample_count": sample_count,
        "target_ms": target_ms,
        "p50_ms": p50_ms,
        "p95_ms": p95_ms,
        "p99_ms": p99_ms,
        "p95_within_target": sample_count == 0 || p95_ms <= target_ms,
    })
}

fn world_map_rum_dimension_summary_json(
    observations: &[WorldMapRumObservation],
    surface_class: &str,
    device_class: &str,
) -> Value {
    let sample_count = observations.len() as u64;
    let tile_error_count = observations
        .iter()
        .map(|sample| sample.tile_error_count)
        .sum::<u64>();
    let tile_error_rate_percent = if sample_count == 0 {
        0
    } else {
        ((tile_error_count as f64 / sample_count.max(1) as f64) * 100.0).round() as u64
    };
    let first_map_interactive = world_map_rum_metric_summary_json(observations, 2000, |sample| {
        sample.first_map_interactive_ms
    });
    let viewport_refresh =
        world_map_rum_metric_summary_json(observations, 250, |sample| sample.viewport_refresh_ms);
    let focus_to_action_rail = world_map_rum_metric_summary_json(observations, 300, |sample| {
        sample.focus_to_action_rail_ms
    });
    let main_thread_long_task = world_map_rum_metric_summary_json(observations, 100, |sample| {
        sample.main_thread_long_task_ms
    });
    let metric_green = |metric: &Value| {
        metric
            .get("p95_within_target")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    };
    json!({
        "surface_class": surface_class,
        "device_class": device_class,
        "sample_count": sample_count,
        "first_map_interactive": first_map_interactive,
        "viewport_refresh": viewport_refresh,
        "focus_to_action_rail": focus_to_action_rail,
        "main_thread_long_task": main_thread_long_task,
        "tile_error_count": tile_error_count,
        "tile_error_rate_percent": tile_error_rate_percent,
        "tile_error_rate_target_percent": 1,
        "tile_error_rate_within_target": tile_error_rate_percent <= 1,
        "green": metric_green(&first_map_interactive)
            && metric_green(&viewport_refresh)
            && metric_green(&focus_to_action_rail)
            && metric_green(&main_thread_long_task)
            && tile_error_rate_percent <= 1,
    })
}

fn world_map_rum_filtered_summary_json(
    observations: &[WorldMapRumObservation],
    surface_class: &str,
    device_class: &str,
) -> Value {
    let filtered = observations
        .iter()
        .filter(|sample| {
            (surface_class == "all" || sample.surface_class == surface_class)
                && (device_class == "all" || sample.device_class == device_class)
        })
        .cloned()
        .collect::<Vec<_>>();
    world_map_rum_dimension_summary_json(&filtered, surface_class, device_class)
}

fn world_map_rum_matrix_bucket_json(
    observations: &[WorldMapRumObservation],
    surface_class: &str,
    device_class: &str,
    sample_kind_class: &str,
) -> Value {
    let sample_count = observations
        .iter()
        .filter(|sample| {
            sample.surface_class == surface_class
                && sample.device_class == device_class
                && sample.sample_kind_class == sample_kind_class
        })
        .count() as u64;
    json!({
        "bucket_id": format!("{surface_class}_{device_class}_{sample_kind_class}"),
        "surface_class": surface_class,
        "device_class": device_class,
        "sample_kind_class": sample_kind_class,
        "sample_count": sample_count,
        "per_bucket_min_samples": 1,
        "observed": sample_count >= 1,
    })
}

fn world_map_rum_sample_matrix_json(observations: &[WorldMapRumObservation]) -> Value {
    const SURFACES: [&str; 2] = ["app", "world"];
    const DEVICES: [&str; 2] = ["mobile", "desktop"];
    const SAMPLE_KINDS: [&str; 3] = [
        "cold_cache_interactive",
        "warm_delta_or_304",
        "weak_network_cached_snapshot",
    ];
    let mut buckets = Vec::new();
    for surface in SURFACES {
        for device in DEVICES {
            for sample_kind in SAMPLE_KINDS {
                buckets.push(world_map_rum_matrix_bucket_json(
                    observations,
                    surface,
                    device,
                    sample_kind,
                ));
            }
        }
    }
    let coverage_count = buckets
        .iter()
        .filter(|bucket| {
            bucket
                .get("observed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count() as u64;
    let required_bucket_count = buckets.len() as u64;
    let raw_matrix_green = coverage_count == required_bucket_count;
    let warming = observations.len() < 30;
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_REAL_USER_RUM_MATRIX_CONTRACT_VERSION,
        "required_surfaces": SURFACES,
        "required_device_classes": DEVICES,
        "required_sample_kinds": SAMPLE_KINDS,
        "per_bucket_min_samples": 1,
        "global_min_samples_before_enforcement": 30,
        "required_bucket_count": required_bucket_count,
        "coverage_count": coverage_count,
        "missing_bucket_count": required_bucket_count.saturating_sub(coverage_count),
        "raw_matrix_green": raw_matrix_green,
        "green": warming || raw_matrix_green,
        "enforcement_status": if warming { "warming_until_min_samples" } else { "enforced" },
        "buckets": buckets,
        "readiness_checks": [
            "app_world_surface_split_visible",
            "mobile_desktop_device_split_visible",
            "cold_warm_weak_sample_kinds_visible",
            "per_bucket_min_sample_visible",
            "raw_matrix_verdict_visible"
        ]
    })
}

fn world_map_rum_distribution_json(observations: &[WorldMapRumObservation]) -> Value {
    const RUM_SLO_MIN_ENFORCEMENT_SAMPLE_COUNT: usize = 30;
    let global = world_map_rum_filtered_summary_json(observations, "all", "all");
    let app = world_map_rum_filtered_summary_json(observations, "app", "all");
    let world = world_map_rum_filtered_summary_json(observations, "world", "all");
    let mobile = world_map_rum_filtered_summary_json(observations, "all", "mobile");
    let desktop = world_map_rum_filtered_summary_json(observations, "all", "desktop");
    let app_mobile = world_map_rum_filtered_summary_json(observations, "app", "mobile");
    let app_desktop = world_map_rum_filtered_summary_json(observations, "app", "desktop");
    let world_mobile = world_map_rum_filtered_summary_json(observations, "world", "mobile");
    let world_desktop = world_map_rum_filtered_summary_json(observations, "world", "desktop");
    let sample_matrix = world_map_rum_sample_matrix_json(observations);
    let sample_matrix_green = sample_matrix
        .get("green")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let sample_matrix_raw_green = sample_matrix
        .get("raw_matrix_green")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let split_green = [
        &global,
        &app,
        &world,
        &mobile,
        &desktop,
        &app_mobile,
        &app_desktop,
        &world_mobile,
        &world_desktop,
    ]
    .iter()
    .all(|summary| {
        summary
            .get("green")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }) && sample_matrix_green;
    let enforcement_warming = observations.len() < RUM_SLO_MIN_ENFORCEMENT_SAMPLE_COUNT;
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_RUM_SLO_CONTRACT_VERSION,
        "global": global,
        "by_surface": {
            "app": app,
            "world": world,
        },
        "by_device_class": {
            "mobile": mobile,
            "desktop": desktop,
        },
        "by_surface_device": {
            "app_mobile": app_mobile,
            "app_desktop": app_desktop,
            "world_mobile": world_mobile,
            "world_desktop": world_desktop,
        },
        "sample_matrix": sample_matrix,
        "slo_gate": {
            "contract_version": TRILLIONNIUM_WORLD_MAP_RUM_SLO_CONTRACT_VERSION,
            "sample_count": observations.len(),
            "split_by_surface": ["app", "world"],
            "split_by_device_class": ["mobile", "desktop"],
            "split_by_sample_kind": ["cold_cache_interactive", "warm_delta_or_304", "weak_network_cached_snapshot"],
            "first_map_interactive_target_ms": 2000,
            "viewport_refresh_p95_target_ms": 250,
            "focus_to_action_rail_target_ms": 300,
            "main_thread_long_task_budget_ms": 100,
            "tile_error_rate_target_percent": 1,
            "green": enforcement_warming || split_green,
            "raw_split_green": split_green,
            "sample_matrix_contract_version": TRILLIONNIUM_WORLD_MAP_REAL_USER_RUM_MATRIX_CONTRACT_VERSION,
            "sample_matrix_raw_green": sample_matrix_raw_green,
            "sample_matrix_coverage_count": sample_matrix.get("coverage_count").and_then(Value::as_u64).unwrap_or(0),
            "sample_matrix_required_bucket_count": sample_matrix.get("required_bucket_count").and_then(Value::as_u64).unwrap_or(12),
            "sample_matrix_missing_bucket_count": sample_matrix.get("missing_bucket_count").and_then(Value::as_u64).unwrap_or(12),
            "per_bucket_min_samples": 1,
            "min_enforcement_sample_count": RUM_SLO_MIN_ENFORCEMENT_SAMPLE_COUNT,
            "enforcement_status": if enforcement_warming { "warming_until_min_samples" } else { "enforced" },
            "readiness_checks": [
                "p50_p95_p99_quantiles_visible",
                "app_world_surface_split_visible",
                "mobile_desktop_device_split_visible",
                "tile_error_rate_visible",
                "slo_targets_bound_to_runtime_budget"
            ]
        }
    })
}

fn atomic_max(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn html_resource_response(body: String, resource_contract: &'static str) -> Response {
    let mut response = Html(body).into_response();
    apply_resource_headers(
        &mut response,
        "private, max-age=30, stale-while-revalidate=120",
        None,
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-trillionnium-resource-contract"),
        HeaderValue::from_static(resource_contract),
    );
    response
}

fn json_resource_response(
    value: Value,
    cache_control: &'static str,
    etag: Option<String>,
) -> Response {
    let mut response = Json(value).into_response();
    apply_resource_headers(&mut response, cache_control, etag);
    response
}

fn apply_resource_headers(
    response: &mut Response,
    cache_control: &'static str,
    etag: Option<String>,
) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    response.headers_mut().insert(
        HeaderName::from_static("x-trillionnium-cache-contract"),
        HeaderValue::from_static("trillionnium_world_map_payload_cache_v1"),
    );
    if let Some(etag) = etag.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response.headers_mut().insert(header::ETAG, etag);
    }
}

#[derive(Clone, Copy)]
enum RateLimitBucketKind {
    SourceScope,
    User,
    Room,
    Session,
    Org,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityScope {
    source_kind: &'static str,
    user_id: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityResolution {
    matched: bool,
    required: bool,
    binding_subject: Option<String>,
    binding_source_format: String,
    binding_version: u32,
    binding_revision: Option<String>,
    product_user_id: Option<String>,
    binding_source_kind: String,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedIdentity {
    scope: IdentityScope,
    resolution: IdentityResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserSessionAuthClaims {
    #[serde(default = "default_session_auth_version")]
    version: u32,
    issuer: String,
    key_id: Option<String>,
    subject: String,
    source_kind: String,
    audience: Option<String>,
    request_fingerprint: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryDocument {
    #[serde(default = "default_session_auth_issuer_registry_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    issuers: HashMap<String, SessionAuthIssuerRegistryIssuer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryMetadata {
    version: u32,
    revision: Option<String>,
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    issuer_count: usize,
    key_count: usize,
}

impl Default for SessionAuthIssuerRegistryMetadata {
    fn default() -> Self {
        Self {
            version: 0,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
            issuer_count: 0,
            key_count: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryIssuer {
    #[serde(default, alias = "activeKeyId")]
    active_key_id: Option<String>,
    #[serde(default)]
    keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct SessionAuthIssuerRegistryRuntimeState {
    metadata: SessionAuthIssuerRegistryMetadata,
    registry: HashMap<String, SessionAuthIssuerRegistryIssuer>,
}

#[derive(Debug, Clone, Serialize)]
struct AuthorizedUserSession {
    claims: UserSessionAuthClaims,
    assertion_header: &'static str,
    signature_header: &'static str,
}

#[derive(Debug, Clone, Default)]
struct IdentityBindingStore {
    metadata: IdentityBindingMetadata,
    registry_metadata: IdentityBindingMetadata,
    bindings: IdentityBindings,
    product_users: HashMap<String, ProductUserIdentity>,
}

#[derive(Debug, Clone)]
struct IdentityBindingAuditState {
    path: Option<String>,
    last_event_kind: Option<String>,
    last_event_epoch: Option<i64>,
    last_status: String,
    last_error: Option<String>,
    last_policy_decision: Option<String>,
    last_policy_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct IdentityAuditQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct IdentityApprovalQuery {
    limit: Option<usize>,
}

impl Default for IdentityBindingAuditState {
    fn default() -> Self {
        Self {
            path: None,
            last_event_kind: None,
            last_event_epoch: None,
            last_status: "disabled".to_string(),
            last_error: None,
            last_policy_decision: None,
            last_policy_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingMetadata {
    format: String,
    version: u32,
    revision: Option<String>,
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingReloadGovernance {
    accepted: bool,
    decision: String,
    reason: String,
    current_format: String,
    current_revision: Option<String>,
    current_registry_revision: Option<String>,
    current_effective_revision: Option<String>,
    current_missing_product_user_refs: usize,
    candidate_format: String,
    candidate_revision: Option<String>,
    candidate_registry_revision: Option<String>,
    candidate_effective_revision: Option<String>,
    candidate_load_status: String,
    candidate_registry_load_status: String,
    candidate_missing_product_user_refs: usize,
    separate_registry_configured: bool,
    approval_state_status: String,
    approval_state_revision: Option<String>,
    approved_revision_count: usize,
    candidate_revision_approved: Option<bool>,
    rollback_blocked: bool,
    requesting_actor: Option<String>,
    actor_header: String,
    actor_authorized: Option<bool>,
    actor_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingRevisionApprovalState {
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    version: u32,
    revision: Option<String>,
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionAuthIssuerRegistryRevisionApprovalState {
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    version: u32,
    revision: Option<String>,
    approved_revisions: Vec<String>,
}

impl Default for IdentityBindingRevisionApprovalState {
    fn default() -> Self {
        Self {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
            version: 0,
            revision: None,
            approved_revisions: Vec::new(),
        }
    }
}

impl Default for SessionAuthIssuerRegistryRevisionApprovalState {
    fn default() -> Self {
        Self {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
            version: 0,
            revision: None,
            approved_revisions: Vec::new(),
        }
    }
}

impl Default for IdentityBindingMetadata {
    fn default() -> Self {
        Self {
            format: "none".to_string(),
            version: 0,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindings {
    #[serde(default)]
    chat_users: HashMap<String, IdentityBindingEntry>,
    #[serde(default)]
    matrix_users: HashMap<String, IdentityBindingEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingsDocument {
    #[serde(default = "default_identity_bindings_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    product_users: HashMap<String, ProductUserIdentity>,
    #[serde(default)]
    chat_users: HashMap<String, IdentityBindingEntry>,
    #[serde(default)]
    matrix_users: HashMap<String, IdentityBindingEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProductUserRegistryDocument {
    #[serde(default = "default_identity_bindings_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    product_users: HashMap<String, ProductUserIdentity>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingRevisionApprovalDocument {
    #[serde(default = "default_identity_binding_approval_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryRevisionApprovalDocument {
    #[serde(default = "default_identity_binding_approval_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingEntry {
    product_user_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProductUserIdentity {
    org_id: Option<String>,
    account_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ReplayCache {
    seen: HashMap<String, ReplayEntry>,
    order: VecDeque<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayEntry {
    seen_at_epoch: i64,
    response: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueMatch {
    match_id: String,
    title: String,
    mode: String,
    status: String,
    objective: String,
    reward: String,
    recommended_roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeaguePlayer {
    player_id: String,
    matrix_user_id: String,
    display_name: String,
    class_tag: String,
    rank_tier: String,
    rating: i64,
    xp: i64,
    reputation: i64,
    battles: i64,
    submissions: i64,
    wins: i64,
    earned_credits: f64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueMatchEntry {
    entry_id: String,
    match_id: String,
    player_id: String,
    matrix_user_id: String,
    status: String,
    battles_started: i64,
    submissions: i64,
    best_score: f64,
    rewards_earned: f64,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueBattle {
    battle_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    task_id: String,
    prompt: String,
    status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueSubmission {
    submission_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    task_id: Option<String>,
    body: String,
    score: f64,
    grade: String,
    reward_amount: f64,
    #[serde(default)]
    judge_status: Option<String>,
    #[serde(default)]
    payout_status: Option<String>,
    #[serde(default)]
    anti_cheat_flags: Vec<String>,
    #[serde(default)]
    score_events: Vec<LeagueScoreEvent>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueScoreEvent {
    dimension: String,
    score: f64,
    weight: f64,
    judge_kind: String,
    evidence: Value,
}

#[derive(Debug, Clone)]
struct LeagueJudgement {
    score: f64,
    grade: String,
    reward_amount: f64,
    judge_status: String,
    payout_status: String,
    anti_cheat_flags: Vec<String>,
    score_events: Vec<LeagueScoreEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct LeagueExternalJudgeResponse {
    score: Option<f64>,
    grade: Option<String>,
    verdict: Option<String>,
    explanation: Option<String>,
    flags: Option<Vec<String>>,
    evidence: Option<Value>,
}

#[derive(Debug, Clone)]
struct LeagueExternalJudgeOutcome {
    event: LeagueScoreEvent,
    flags: Vec<String>,
    grade_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueReward {
    reward_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    amount: f64,
    currency_unit: String,
    reason: String,
    #[serde(default)]
    ledger_status: Option<String>,
    #[serde(default)]
    ledger_account_id: Option<String>,
    #[serde(default)]
    ledger_entry_id: Option<String>,
    #[serde(default)]
    ledger_balance_after: Option<f64>,
    #[serde(default)]
    ledger_error: Option<String>,
    #[serde(default)]
    review_status: Option<String>,
    #[serde(default)]
    reviewed_by: Option<String>,
    #[serde(default)]
    review_note: Option<String>,
    #[serde(default)]
    reviewed_at_epoch: Option<i64>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueInventoryItem {
    item_id: String,
    player_id: String,
    matrix_user_id: String,
    source_submission_id: String,
    item_kind: String,
    name: String,
    rarity: String,
    power: i64,
    cosmetic: bool,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueSkill {
    skill_id: String,
    name: String,
    skill_kind: String,
    school_id: String,
    description: String,
    unlock_level: i64,
    max_rank: i64,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueTool {
    tool_id: String,
    name: String,
    tool_kind: String,
    slot: String,
    description: String,
    unlock_level: i64,
    power_bonus: i64,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueSkin {
    skin_id: String,
    name: String,
    skin_kind: String,
    description: String,
    unlock_level: i64,
    agent_count: i64,
    multi_agent_capability: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueGuild {
    guild_id: String,
    name: String,
    motto: String,
    status: String,
    rating: i64,
    reputation: i64,
    treasury_credits: f64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueGuildMembership {
    guild_id: String,
    player_id: String,
    matrix_user_id: String,
    role: String,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueRaidContribution {
    contribution_id: String,
    match_id: String,
    guild_id: Option<String>,
    player_id: String,
    matrix_user_id: String,
    room_id: Option<String>,
    role: String,
    body: String,
    contribution_score: f64,
    progress_delta: f64,
    #[serde(default)]
    payout_status: Option<String>,
    #[serde(default)]
    anti_cheat_flags: Vec<String>,
    #[serde(default)]
    score_events: Vec<LeagueScoreEvent>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueRaidRosterSlot {
    slot_id: String,
    match_id: String,
    guild_id: Option<String>,
    player_id: String,
    matrix_user_id: String,
    room_id: Option<String>,
    role: String,
    hero_id: String,
    status: String,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldZone {
    zone_id: String,
    name: String,
    status: String,
    theme: String,
    mirror_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldLocation {
    location_id: String,
    zone_id: String,
    name: String,
    location_kind: String,
    description: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEntity {
    entity_id: String,
    location_id: String,
    name: String,
    entity_kind: String,
    role: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldAsset {
    asset_id: String,
    owner_matrix_user_id: String,
    location_id: String,
    asset_kind: String,
    name: String,
    status: String,
    value_score: i64,
    #[serde(default)]
    upgrade_level: i64,
    #[serde(default)]
    upgrade_points: i64,
    #[serde(default)]
    last_upgrade_kind: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldAssetUpgrade {
    upgrade_id: String,
    asset_id: String,
    matrix_user_id: String,
    body: String,
    upgrade_kind: String,
    score: f64,
    grade: String,
    judge_status: String,
    status: String,
    value_delta: i64,
    level_before: i64,
    level_after: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldCompany {
    company_id: String,
    owner_matrix_user_id: String,
    asset_id: String,
    location_id: String,
    name: String,
    company_kind: String,
    status: String,
    revenue_score: i64,
    reputation_score: i64,
    level: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldShop {
    shop_id: String,
    company_id: String,
    owner_matrix_user_id: String,
    location_id: String,
    name: String,
    shop_kind: String,
    status: String,
    listing_count: i64,
    gross_merchandise_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldListing {
    listing_id: String,
    shop_id: String,
    company_id: String,
    owner_matrix_user_id: String,
    asset_id: String,
    title: String,
    listing_kind: String,
    status: String,
    price_credits: i64,
    quality_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEconomyEvent {
    economy_event_id: String,
    matrix_user_id: String,
    event_kind: String,
    subject_id: String,
    credits_delta: i64,
    reputation_delta: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldPurchase {
    purchase_id: String,
    listing_id: String,
    shop_id: String,
    company_id: String,
    buyer_matrix_user_id: String,
    seller_matrix_user_id: String,
    price_credits: i64,
    status: String,
    #[serde(default)]
    ledger_status: Option<String>,
    #[serde(default)]
    ledger_account_id: Option<String>,
    #[serde(default)]
    ledger_entry_id: Option<String>,
    #[serde(default)]
    ledger_balance_after: Option<f64>,
    #[serde(default)]
    ledger_error: Option<String>,
    #[serde(default)]
    buyer_ledger_status: Option<String>,
    #[serde(default)]
    buyer_ledger_account_id: Option<String>,
    #[serde(default)]
    buyer_ledger_entry_id: Option<String>,
    #[serde(default)]
    buyer_ledger_balance_after: Option<f64>,
    #[serde(default)]
    buyer_ledger_error: Option<String>,
    #[serde(default)]
    buyer_consume_status: Option<String>,
    #[serde(default)]
    buyer_consume_entry_id: Option<String>,
    #[serde(default)]
    buyer_consume_balance_after: Option<f64>,
    #[serde(default)]
    buyer_consume_error: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkOrder {
    work_order_id: String,
    purchase_id: String,
    listing_id: String,
    buyer_matrix_user_id: String,
    seller_matrix_user_id: String,
    company_id: String,
    status: String,
    brief: String,
    value_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkDelivery {
    delivery_id: String,
    work_order_id: String,
    matrix_user_id: String,
    body: String,
    score: f64,
    judge_status: String,
    status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkAcceptance {
    acceptance_id: String,
    work_order_id: String,
    matrix_user_id: String,
    body: String,
    status: String,
    reputation_delta: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkRejection {
    rejection_id: String,
    work_order_id: String,
    matrix_user_id: String,
    body: String,
    status: String,
    refund_status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkReopen {
    reopen_id: String,
    work_order_id: String,
    matrix_user_id: String,
    body: String,
    status: String,
    reserve_status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldWorkCancellation {
    cancellation_id: String,
    work_order_id: String,
    matrix_user_id: String,
    body: String,
    status: String,
    refund_status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldMapNode {
    node_id: String,
    location_id: String,
    zone_id: String,
    name: String,
    node_kind: String,
    description: String,
    x: i64,
    y: i64,
    exits: HashMap<String, String>,
    interaction_tags: Vec<String>,
    freedom_hooks: Vec<String>,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldPlayerPosition {
    matrix_user_id: String,
    node_id: String,
    location_id: String,
    updated_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldFaction {
    faction_id: String,
    zone_id: String,
    name: String,
    faction_kind: String,
    status: String,
    reputation_score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldFactionStanding {
    standing_id: String,
    matrix_user_id: String,
    faction_id: String,
    reputation_score: i64,
    rank: String,
    updated_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEvent {
    event_id: String,
    actor_matrix_user_id: String,
    room_id: Option<String>,
    location_id: String,
    event_kind: String,
    body: String,
    result: String,
    impact_score: i64,
    #[serde(default)]
    cex_task_id: Option<String>,
    #[serde(default)]
    cex_status: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldContract {
    contract_id: String,
    event_id: String,
    actor_matrix_user_id: String,
    location_id: String,
    task_id: String,
    title: String,
    body: String,
    status: String,
    cex_status: Option<String>,
    value_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldContractCompletion {
    completion_id: String,
    contract_id: String,
    matrix_user_id: String,
    body: String,
    score: f64,
    grade: String,
    reward_amount: f64,
    judge_status: String,
    payout_status: String,
    anti_cheat_flags: Vec<String>,
    score_events: Vec<LeagueScoreEvent>,
    ledger_status: Option<String>,
    ledger_account_id: Option<String>,
    ledger_entry_id: Option<String>,
    ledger_balance_after: Option<f64>,
    ledger_error: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldRelationship {
    relationship_id: String,
    from_id: String,
    to_id: String,
    relation_kind: String,
    strength: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorldState {
    #[serde(default)]
    world_zones: HashMap<String, WorldZone>,
    #[serde(default)]
    world_locations: HashMap<String, WorldLocation>,
    #[serde(default)]
    world_entities: HashMap<String, WorldEntity>,
    #[serde(default)]
    world_map_nodes: HashMap<String, WorldMapNode>,
    #[serde(default)]
    world_player_positions: HashMap<String, WorldPlayerPosition>,
    #[serde(default)]
    world_trillionnium_characters: HashMap<String, WorldTrillionniumCharacter>,
    #[serde(default)]
    world_assets: Vec<WorldAsset>,
    #[serde(default)]
    world_asset_upgrades: Vec<WorldAssetUpgrade>,
    #[serde(default)]
    world_companies: Vec<WorldCompany>,
    #[serde(default)]
    world_shops: Vec<WorldShop>,
    #[serde(default)]
    world_listings: Vec<WorldListing>,
    #[serde(default)]
    world_economy_events: Vec<WorldEconomyEvent>,
    #[serde(default)]
    world_purchases: Vec<WorldPurchase>,
    #[serde(default)]
    world_work_orders: Vec<WorldWorkOrder>,
    #[serde(default)]
    world_work_deliveries: Vec<WorldWorkDelivery>,
    #[serde(default)]
    world_work_acceptances: Vec<WorldWorkAcceptance>,
    #[serde(default)]
    world_work_rejections: Vec<WorldWorkRejection>,
    #[serde(default)]
    world_work_reopens: Vec<WorldWorkReopen>,
    #[serde(default)]
    world_work_cancellations: Vec<WorldWorkCancellation>,
    #[serde(default)]
    world_factions: HashMap<String, WorldFaction>,
    #[serde(default)]
    world_faction_standings: Vec<WorldFactionStanding>,
    #[serde(default)]
    world_events: Vec<WorldEvent>,
    #[serde(default)]
    world_contracts: Vec<WorldContract>,
    #[serde(default)]
    world_contract_completions: Vec<WorldContractCompletion>,
    #[serde(default)]
    world_relationships: Vec<WorldRelationship>,
    #[serde(default)]
    world_tactics_sessions: HashMap<String, WorldTacticsGameSession>,
    #[serde(default)]
    world_tactics_simulation_ticks: Vec<WorldTacticsSimulationTick>,
}

#[derive(Debug, Deserialize)]
struct WorldActionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    location_id: Option<String>,
    body: String,
    message: Option<String>,
    event_id: Option<String>,
    capability_id: Option<String>,
    account_id: Option<String>,
    cex_task_id: Option<String>,
    cex_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueDraftRequest {
    matrix_user_id: String,
    heroes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LeagueState {
    #[serde(default)]
    matches: HashMap<String, LeagueMatch>,
    #[serde(default)]
    players_by_matrix_user: HashMap<String, LeaguePlayer>,
    #[serde(default)]
    entries: HashMap<String, LeagueMatchEntry>,
    #[serde(default)]
    battles: HashMap<String, LeagueBattle>,
    #[serde(default)]
    submissions: HashMap<String, LeagueSubmission>,
    #[serde(default)]
    rewards: Vec<LeagueReward>,
    #[serde(default)]
    player_loadouts: HashMap<String, Vec<String>>,
    #[serde(default)]
    guilds: HashMap<String, LeagueGuild>,
    #[serde(default)]
    guild_memberships: HashMap<String, LeagueGuildMembership>,
    #[serde(default)]
    inventory_items: Vec<LeagueInventoryItem>,
    #[serde(default)]
    league_skills: HashMap<String, LeagueSkill>,
    #[serde(default)]
    league_tools: HashMap<String, LeagueTool>,
    #[serde(default)]
    league_skins: HashMap<String, LeagueSkin>,
    #[serde(default)]
    raid_contributions: Vec<LeagueRaidContribution>,
    #[serde(default)]
    raid_rosters: Vec<LeagueRaidRosterSlot>,
    #[serde(default, flatten)]
    world: WorldState,
}

#[derive(Debug, Deserialize)]
struct LeagueJoinRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueBattleRequest {
    matrix_user_id: String,
    room_id: String,
    message: String,
    event_id: Option<String>,
    capability_id: Option<String>,
    account_id: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct LeagueSubmitRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    task_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LeagueRaidContributionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    role: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LeagueRaidRosterRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    role: Option<String>,
    hero_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueReviewRequest {
    reviewer_id: Option<String>,
    room_id: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueWebActionRequest {
    action: String,
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    match_id: Option<String>,
    guild_id: Option<String>,
    role: Option<String>,
    heroes: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebActionRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    location_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldTacticsCommandRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    command: String,
    unit_id: Option<String>,
    target_tile: Option<String>,
    skill_id: Option<String>,
    npc_id: Option<String>,
    task_archetype_id: Option<String>,
    osm_game_overlay_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebTacticsCommandRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    command: Option<String>,
    unit_id: Option<String>,
    target_tile: Option<String>,
    skill_id: Option<String>,
    npc_id: Option<String>,
    task_archetype_id: Option<String>,
    osm_game_overlay_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldContractCompleteRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebContractCompleteRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    contract_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldAssetUpgradeRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebAssetUpgradeRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    asset_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldCompanyRequest {
    matrix_user_id: String,
    asset_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebCompanyRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    asset_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldListingRequest {
    matrix_user_id: String,
    company_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldListingBuyRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWorkDeliverRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWorkAcceptRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWorkRejectRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWorkReopenRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWorkCancelRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebListingRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    company_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebListingBuyRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    listing_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebWorkDeliverRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    work_order_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebWorkAcceptRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    work_order_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebWorkRejectRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    work_order_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebWorkReopenRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    work_order_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebWorkCancelRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    work_order_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueWebSessionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    session_id: Option<String>,
    csrf: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueWebSessionClaims {
    version: u32,
    matrix_user_id: String,
    room_id: Option<String>,
    session_id: Option<String>,
    csrf: String,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct WorldMapMoveRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    target: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebMapMoveRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    target: Option<String>,
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldMapRumRequest {
    matrix_user_id: Option<String>,
    surface_id: Option<String>,
    session_id: Option<String>,
    viewport_cursor: Option<String>,
    sample_kind: Option<String>,
    user_agent_class: Option<String>,
    first_map_interactive_ms: Option<u64>,
    viewport_refresh_ms: Option<u64>,
    focus_to_action_rail_ms: Option<u64>,
    main_thread_long_task_ms: Option<u64>,
    tile_error_count: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RateLimitCache {
    seen: HashMap<String, VecDeque<i64>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProfile {
    LocalDev,
    Beta,
    Production,
}

impl RuntimeProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::LocalDev => "local_dev",
            Self::Beta => "beta",
            Self::Production => "production",
        }
    }

    fn from_env() -> Self {
        match first_present_env(&["CONSUMER_ENTRY_RUNTIME_PROFILE", "CEX_RUNTIME_PROFILE"])
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("beta") => Self::Beta,
            Some("production") | Some("prod") => Self::Production,
            _ => Self::LocalDev,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsumerEntryConfig {
    pub runtime_profile: RuntimeProfile,
    pub bind_addr: String,
    pub cex_gateway_base_url: String,
    pub cex_gateway_api_key: String,
    pub ledger_base_url: String,
    pub ledger_admin_token: Option<String>,
    pub default_capability_id: Option<String>,
    pub default_account_id: Option<String>,
    pub ingress_token: Option<String>,
    pub require_session_auth: bool,
    pub session_auth_secret: Option<String>,
    pub session_auth_issuer_secrets: HashMap<String, String>,
    pub session_auth_issuer_keys: HashMap<String, HashMap<String, String>>,
    pub session_auth_issuer_registry_path: Option<String>,
    pub session_auth_issuer_registry: HashMap<String, SessionAuthIssuerRegistryIssuer>,
    pub session_auth_issuer_registry_load_error: Option<String>,
    pub session_auth_issuer_registry_metadata: SessionAuthIssuerRegistryMetadata,
    pub session_auth_allowed_issuers: Vec<String>,
    pub session_auth_expected_audience: Option<String>,
    pub session_auth_issuer_registry_approved_revisions_path: Option<String>,
    pub session_auth_issuer_registry_require_approved_revision: bool,
    pub session_auth_issuer_registry_require_actor: bool,
    pub session_auth_issuer_registry_actor_header: String,
    pub session_auth_issuer_registry_allowed_actors: Vec<String>,
    pub session_auth_max_clock_skew_secs: u64,
    pub session_auth_max_ttl_secs: u64,
    pub identity_bindings_path: Option<String>,
    pub identity_registry_path: Option<String>,
    pub identity_binding_audit_log_path: Option<String>,
    pub identity_binding_approved_revisions_path: Option<String>,
    pub identity_binding_reload_require_revision: bool,
    pub identity_binding_reload_reject_same_revision: bool,
    pub identity_binding_reload_allow_legacy_format: bool,
    pub identity_binding_reload_require_approved_revision: bool,
    pub identity_binding_reload_allow_rollback: bool,
    pub identity_binding_reload_require_actor: bool,
    pub identity_binding_reload_actor_header: String,
    pub identity_binding_reload_allowed_actors: Vec<String>,
    pub require_identity_binding: bool,
    pub max_text_chars: usize,
    pub rate_limit_window_secs: u64,
    pub rate_limit_max_requests: usize,
    pub rate_limit_user_max_requests: usize,
    pub rate_limit_room_max_requests: usize,
    pub rate_limit_session_max_requests: usize,
    pub rate_limit_org_max_requests: usize,
    pub rate_limit_store_path: Option<String>,
    pub replay_window_secs: u64,
    pub replay_cache_size: usize,
    pub replay_store_path: Option<String>,
    pub league_state_path: Option<String>,
    pub league_sql_snapshot_path: Option<String>,
    pub league_normalized_database_url: Option<String>,
    pub league_normalized_dual_write_enabled: bool,
    pub league_normalized_read_switch_enabled: bool,
    pub league_normalized_final_cutover_enabled: bool,
    pub league_hidden_tests_enabled: bool,
    pub league_llm_judge_url: Option<String>,
    pub league_llm_judge_token: Option<String>,
    pub league_llm_judge_required: bool,
    pub league_llm_judge_timeout_ms: u64,
    pub league_web_session_required: bool,
    pub league_web_session_secret: Option<String>,
    pub league_web_session_cookie_name: String,
    pub league_web_session_ttl_secs: u64,
}

impl ConsumerEntryConfig {
    pub fn from_env() -> Self {
        let session_auth_issuer_registry_path = first_present_env(&[
            "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH",
            "CEX_SESSION_AUTH_ISSUER_REGISTRY_PATH",
        ]);
        let (session_auth_issuer_registry_metadata, session_auth_issuer_registry) =
            load_session_auth_issuer_registry(session_auth_issuer_registry_path.as_deref());
        let session_auth_issuer_registry_load_error =
            session_auth_issuer_registry_metadata.load_error.clone();

        Self {
            runtime_profile: RuntimeProfile::from_env(),
            bind_addr: env::var("CONSUMER_ENTRY_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8090".to_string()),
            cex_gateway_base_url: env::var("CEX_GATEWAY_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            cex_gateway_api_key: env::var("CEX_GATEWAY_API_KEY")
                .unwrap_or_else(|_| "local-dev-key".to_string()),
            ledger_base_url: first_present_env(&[
                "CONSUMER_ENTRY_LEDGER_BASE_URL",
                "LEDGER_BASE_URL",
            ])
            .unwrap_or_else(|| "http://127.0.0.1:7002".to_string()),
            ledger_admin_token: first_present_env(&[
                "CONSUMER_ENTRY_LEDGER_ADMIN_TOKEN",
                "LEDGER_ADMIN_TOKEN",
            ])
            .or_else(parse_first_ledger_admin_token_from_env),
            default_capability_id: env::var("CONSUMER_ENTRY_DEFAULT_CAPABILITY_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            default_account_id: env::var("CONSUMER_ENTRY_DEFAULT_ACCOUNT_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ingress_token: env::var("CONSUMER_ENTRY_INGRESS_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            require_session_auth: boolean_env("CONSUMER_ENTRY_REQUIRE_SESSION_AUTH", false),
            session_auth_secret: env::var("CONSUMER_ENTRY_SESSION_AUTH_SECRET")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            session_auth_issuer_secrets: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON",
            )
            .ok()
            .map(|value| parse_session_auth_issuer_secrets_json(&value))
            .unwrap_or_default(),
            session_auth_issuer_keys: env::var("CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON")
                .ok()
                .map(|value| parse_session_auth_issuer_keys_json(&value))
                .unwrap_or_default(),
            session_auth_issuer_registry_path,
            session_auth_issuer_registry,
            session_auth_issuer_registry_load_error,
            session_auth_issuer_registry_metadata,
            session_auth_allowed_issuers: env::var("CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS")
                .ok()
                .map(|value| parse_csv_list(&value))
                .unwrap_or_default(),
            session_auth_expected_audience: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
            session_auth_issuer_registry_approved_revisions_path: first_present_env(&[
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
                "CEX_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
            ]),
            session_auth_issuer_registry_require_approved_revision: boolean_env(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION",
                false,
            ),
            session_auth_issuer_registry_require_actor: boolean_env(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_ACTOR",
                false,
            ),
            session_auth_issuer_registry_actor_header: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ACTOR_HEADER",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "x-session-auth-issuer-registry-actor".to_string()),
            session_auth_issuer_registry_allowed_actors: parse_csv_list(
                &env::var("CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ALLOWED_ACTORS")
                    .ok()
                    .unwrap_or_default(),
            ),
            session_auth_max_clock_skew_secs: positive_u64_env(
                "CONSUMER_ENTRY_SESSION_AUTH_MAX_CLOCK_SKEW_SECS",
                DEFAULT_SESSION_AUTH_MAX_CLOCK_SKEW_SECS,
            ),
            session_auth_max_ttl_secs: positive_u64_env(
                "CONSUMER_ENTRY_SESSION_AUTH_MAX_TTL_SECS",
                DEFAULT_SESSION_AUTH_MAX_TTL_SECS,
            ),
            identity_bindings_path: env::var("CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            identity_registry_path: env::var("CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            identity_binding_audit_log_path: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH",
            )
            .ok()
            .filter(|v| !v.trim().is_empty()),
            identity_binding_approved_revisions_path: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH",
            )
            .ok()
            .filter(|v| !v.trim().is_empty()),
            identity_binding_reload_require_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION",
                false,
            ),
            identity_binding_reload_reject_same_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REJECT_SAME_REVISION",
                false,
            ),
            identity_binding_reload_allow_legacy_format: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_LEGACY_FORMAT",
                true,
            ),
            identity_binding_reload_require_approved_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION",
                false,
            ),
            identity_binding_reload_allow_rollback: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK",
                true,
            ),
            identity_binding_reload_require_actor: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR",
                false,
            ),
            identity_binding_reload_actor_header: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "x-identity-binding-actor".to_string()),
            identity_binding_reload_allowed_actors: parse_csv_list(
                &env::var("CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS")
                    .ok()
                    .unwrap_or_default(),
            ),
            require_identity_binding: boolean_env("CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING", false),
            max_text_chars: positive_usize_env(
                "CONSUMER_ENTRY_MAX_TEXT_CHARS",
                DEFAULT_MAX_TEXT_CHARS,
            ),
            rate_limit_window_secs: positive_u64_env(
                "CONSUMER_ENTRY_RATE_LIMIT_WINDOW_SECS",
                DEFAULT_RATE_LIMIT_WINDOW_SECS,
            ),
            rate_limit_max_requests: positive_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_MAX_REQUESTS",
                DEFAULT_RATE_LIMIT_MAX_REQUESTS,
            ),
            rate_limit_user_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS",
                0,
            ),
            rate_limit_room_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS",
                0,
            ),
            rate_limit_session_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS",
                0,
            ),
            rate_limit_org_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS",
                0,
            ),
            rate_limit_store_path: first_present_env(&["CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH"]),
            replay_window_secs: positive_u64_env_alias(
                &[
                    "CONSUMER_ENTRY_REPLAY_WINDOW_SECS",
                    "CONSUMER_ENTRY_MATRIX_EVENT_WINDOW_SECS",
                ],
                DEFAULT_REPLAY_WINDOW_SECS,
            ),
            replay_cache_size: positive_usize_env_alias(
                &[
                    "CONSUMER_ENTRY_REPLAY_CACHE_SIZE",
                    "CONSUMER_ENTRY_MATRIX_EVENT_CACHE_SIZE",
                ],
                DEFAULT_REPLAY_CACHE_SIZE,
            ),
            replay_store_path: first_present_env(&[
                "CONSUMER_ENTRY_REPLAY_STORE_PATH",
                "CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH",
            ]),
            league_state_path: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_STATE_PATH",
                "CEX_LEAGUE_STATE_PATH",
            ]),
            league_sql_snapshot_path: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH",
                "CEX_LEAGUE_SQL_SNAPSHOT_PATH",
            ]),
            league_normalized_database_url: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL",
                "CEX_LEAGUE_NORMALIZED_DATABASE_URL",
            ]),
            league_normalized_dual_write_enabled: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED",
                boolean_env("CEX_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED", false),
            ),
            league_normalized_read_switch_enabled: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED",
                boolean_env("CEX_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED", false),
            ),
            league_normalized_final_cutover_enabled: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED",
                boolean_env("CEX_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED", false),
            ),
            league_hidden_tests_enabled: boolean_env("CONSUMER_ENTRY_LEAGUE_HIDDEN_TESTS", true),
            league_llm_judge_url: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_URL",
                "CEX_LEAGUE_LLM_JUDGE_URL",
            ]),
            league_llm_judge_token: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TOKEN",
                "CEX_LEAGUE_LLM_JUDGE_TOKEN",
            ]),
            league_llm_judge_required: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_REQUIRED",
                false,
            ),
            league_llm_judge_timeout_ms: positive_u64_env(
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TIMEOUT_MS",
                DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS,
            ),
            league_web_session_required: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED",
                !matches!(RuntimeProfile::from_env(), RuntimeProfile::LocalDev),
            ),
            league_web_session_secret: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET",
                "CONSUMER_ENTRY_WEB_SESSION_SECRET",
            ]),
            league_web_session_cookie_name: env::var("CONSUMER_ENTRY_LEAGUE_WEB_SESSION_COOKIE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "cex_league_session".to_string()),
            league_web_session_ttl_secs: positive_u64_env(
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_TTL_SECS",
                DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
            ),
        }
    }

    fn profile_validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if matches!(
            self.runtime_profile,
            RuntimeProfile::Beta | RuntimeProfile::Production
        ) {
            if self.ingress_token.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_INGRESS_TOKEN".to_string(),
                );
            }
            if self.replay_store_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REPLAY_STORE_PATH".to_string(),
                );
            }
            if !self.require_session_auth {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true"
                        .to_string(),
                );
            }
            if let Some(error) = self.session_auth_issuer_registry_load_error.as_deref() {
                errors.push(format!(
                    "failed to load CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH: {error}"
                ));
            } else if self.session_auth_issuer_registry_path.is_some()
                && self.session_auth_issuer_registry.is_empty()
            {
                errors.push(
                    "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured but contains no issuers"
                        .to_string(),
                );
            }
            if self.session_auth_secret.is_none()
                && self.session_auth_issuer_secrets.is_empty()
                && self.session_auth_issuer_keys.is_empty()
                && self.session_auth_issuer_registry.is_empty()
            {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_SECRET or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH"
                        .to_string(),
                );
            }
            if self.session_auth_allowed_issuers.is_empty() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS"
                        .to_string(),
                );
            }
            if self.session_auth_expected_audience.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE"
                        .to_string(),
                );
            }
            if self.league_web_session_required && league_web_session_secret(self).is_none() {
                errors.push(
                    "beta/production League web actions require CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET or CONSUMER_ENTRY_SESSION_AUTH_SECRET"
                        .to_string(),
                );
            }
            if self.session_auth_issuer_registry_require_approved_revision {
                let approval_state =
                    load_session_auth_issuer_registry_revision_approval_state(self);
                let approval_checks = session_auth_issuer_registry_approval_checks_json(
                    self,
                    &self.session_auth_issuer_registry_metadata,
                    &approval_state,
                );
                let approval_status = approval_checks
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("approval_state_not_loaded");
                if self
                    .session_auth_issuer_registry_approved_revisions_path
                    .is_none()
                {
                    errors.push(
                        "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH"
                            .to_string(),
                    );
                } else if approval_status != "ok" {
                    errors.push(format!(
                        "session auth issuer registry approval invalid: {approval_status}"
                    ));
                }
            }
            if self.session_auth_issuer_registry_require_actor {
                let actor_checks = session_auth_issuer_registry_actor_checks_json(self);
                let actor_status = actor_checks
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("actor_header_missing");
                if actor_status != "ok" {
                    errors.push(format!(
                        "session auth issuer registry actor gate invalid: {actor_status}"
                    ));
                }
            }
            if self.rate_limit_store_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH"
                        .to_string(),
                );
            }
            if self.identity_bindings_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH"
                        .to_string(),
                );
            }
            if !self.require_identity_binding {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true"
                        .to_string(),
                );
            }
            if self.cex_gateway_api_key.trim() == "local-dev-key" {
                errors.push(
                    "beta/production profile requires non-default CEX_GATEWAY_API_KEY".to_string(),
                );
            }
        }

        if self.league_normalized_dual_write_enabled
            && self.league_normalized_database_url.is_none()
        {
            errors.push(
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED=true requires CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL"
                    .to_string(),
            );
        }
        if self.league_normalized_read_switch_enabled
            && self.league_normalized_database_url.is_none()
        {
            errors.push(
                "CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=true requires CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL"
                    .to_string(),
            );
        }
        if self.league_normalized_final_cutover_enabled {
            if self.league_normalized_database_url.is_none() {
                errors.push(
                    "CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED=true requires CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL"
                        .to_string(),
                );
            }
            if !self.league_normalized_dual_write_enabled {
                errors.push(
                    "CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED=true requires CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED=true"
                        .to_string(),
                );
            }
        }

        if self.runtime_profile == RuntimeProfile::Production {
            if self.identity_binding_audit_log_path.is_none() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH"
                        .to_string(),
                );
            }
            if self.identity_binding_approved_revisions_path.is_none() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_revision {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION=true"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_approved_revision {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION=true"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_actor {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR=true"
                        .to_string(),
                );
            }
            if self.identity_binding_reload_allowed_actors.is_empty() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS"
                        .to_string(),
                );
            }
            if self.session_auth_issuer_registry_path.is_some() {
                if !self.session_auth_issuer_registry_require_approved_revision {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
                if !self.session_auth_issuer_registry_require_actor {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_ACTOR=true when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
                if self.session_auth_issuer_registry_allowed_actors.is_empty() {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ALLOWED_ACTORS when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
            }
        }

        errors
    }

    fn validate_runtime_profile(&self) -> Result<(), Vec<String>> {
        let errors = self.profile_validation_errors();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn positive_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn boolean_env(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn positive_u64_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn non_negative_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn first_present_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn parse_first_ledger_admin_token_from_env() -> Option<String> {
    let value = env::var("LEDGER_ADMIN_TOKENS_JSON").ok()?;
    let parsed: Value = serde_json::from_str(&value).ok()?;

    match parsed {
        Value::Array(items) => items.into_iter().find_map(|item| {
            item.get("token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToString::to_string)
        }),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
}

fn positive_usize_env_alias(names: &[&str], default: usize) -> usize {
    names
        .iter()
        .find_map(|name| {
            env::var(name)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
        })
        .unwrap_or(default)
}

fn parse_csv_list(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    for item in value.split(',') {
        let normalized = item.trim();
        if normalized.is_empty() {
            continue;
        }
        if values.iter().any(|existing| existing == normalized) {
            continue;
        }
        values.push(normalized.to_string());
    }
    values
}

fn parse_session_auth_issuer_secrets_json(value: &str) -> HashMap<String, String> {
    let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(value) else {
        return HashMap::new();
    };

    let mut normalized = HashMap::new();
    for (issuer, secret) in parsed {
        let issuer = issuer.trim();
        let secret = secret.trim();
        if issuer.is_empty() || secret.is_empty() {
            continue;
        }
        normalized.insert(issuer.to_string(), secret.to_string());
    }
    normalized
}

fn parse_session_auth_issuer_keys_json(value: &str) -> HashMap<String, HashMap<String, String>> {
    let Ok(parsed) = serde_json::from_str::<HashMap<String, HashMap<String, String>>>(value) else {
        return HashMap::new();
    };

    let mut normalized = HashMap::new();
    for (issuer, keys) in parsed {
        let issuer = issuer.trim();
        if issuer.is_empty() {
            continue;
        }
        let mut normalized_keys = HashMap::new();
        for (key_id, secret) in keys {
            let key_id = key_id.trim();
            let secret = secret.trim();
            if key_id.is_empty() || secret.is_empty() {
                continue;
            }
            normalized_keys.insert(key_id.to_string(), secret.to_string());
        }
        if normalized_keys.is_empty() {
            continue;
        }
        normalized.insert(issuer.to_string(), normalized_keys);
    }
    normalized
}

fn positive_u64_env_alias(names: &[&str], default: u64) -> u64 {
    names
        .iter()
        .find_map(|name| {
            env::var(name)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
        })
        .unwrap_or(default)
}

fn default_identity_bindings_version() -> u32 {
    1
}

fn default_identity_binding_approval_version() -> u32 {
    1
}

fn default_session_auth_version() -> u32 {
    1
}

fn default_session_auth_issuer_registry_version() -> u32 {
    1
}

fn load_session_auth_issuer_registry(
    path: Option<&str>,
) -> (
    SessionAuthIssuerRegistryMetadata,
    HashMap<String, SessionAuthIssuerRegistryIssuer>,
) {
    let Some(path) = path.map(str::trim).filter(|value| !value.is_empty()) else {
        return (SessionAuthIssuerRegistryMetadata::default(), HashMap::new());
    };

    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);
    let loaded_at_epoch = Some(Utc::now().timestamp());
    let raw = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(err) => {
            return (
                SessionAuthIssuerRegistryMetadata {
                    source_path: Some(path.to_string()),
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                    ..SessionAuthIssuerRegistryMetadata::default()
                },
                HashMap::new(),
            )
        }
    };
    let parsed = match serde_json::from_str::<SessionAuthIssuerRegistryDocument>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return (
                SessionAuthIssuerRegistryMetadata {
                    source_path: Some(path.to_string()),
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                    ..SessionAuthIssuerRegistryMetadata::default()
                },
                HashMap::new(),
            )
        }
    };

    let mut normalized = HashMap::new();
    let mut key_count = 0usize;
    for (issuer, entry) in parsed.issuers {
        let issuer = issuer.trim();
        if issuer.is_empty() {
            continue;
        }
        let mut normalized_keys = HashMap::new();
        for (key_id, secret) in entry.keys {
            let key_id = key_id.trim();
            let secret = secret.trim();
            if key_id.is_empty() || secret.is_empty() {
                continue;
            }
            normalized_keys.insert(key_id.to_string(), secret.to_string());
        }
        if normalized_keys.is_empty() {
            continue;
        }
        let active_key_id = entry
            .active_key_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(active_key_id) = active_key_id.as_deref() {
            if !normalized_keys.contains_key(active_key_id) {
                return (
                    SessionAuthIssuerRegistryMetadata {
                        version: parsed.version,
                        revision: parsed.revision.clone(),
                        source_path: Some(path.to_string()),
                        source_modified_epoch,
                        loaded_at_epoch,
                        load_status: "invalid_active_key".to_string(),
                        load_error: Some(format!(
                            "issuer {issuer} declares active key {active_key_id} but no matching secret exists"
                        )),
                        issuer_count: normalized.len(),
                        key_count,
                    },
                    HashMap::new(),
                );
            }
        }
        key_count += normalized_keys.len();
        normalized.insert(
            issuer.to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id,
                keys: normalized_keys,
            },
        );
    }

    (
        SessionAuthIssuerRegistryMetadata {
            version: parsed.version,
            revision: parsed.revision,
            source_path: Some(path.to_string()),
            source_modified_epoch,
            loaded_at_epoch,
            load_status: "loaded".to_string(),
            load_error: None,
            issuer_count: normalized.len(),
            key_count,
        },
        normalized,
    )
}

fn system_time_to_epoch(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn load_product_user_registry(
    path: Option<&str>,
    fallback_users: HashMap<String, ProductUserIdentity>,
    fallback_metadata: &IdentityBindingMetadata,
) -> (
    IdentityBindingMetadata,
    HashMap<String, ProductUserIdentity>,
) {
    let Some(path) = path else {
        if fallback_users.is_empty() {
            return (IdentityBindingMetadata::default(), HashMap::new());
        }
        return (
            IdentityBindingMetadata {
                format: "embedded-in-binding".to_string(),
                version: fallback_metadata.version,
                revision: fallback_metadata.revision.clone(),
                source_path: fallback_metadata.source_path.clone(),
                source_modified_epoch: fallback_metadata.source_modified_epoch,
                loaded_at_epoch: fallback_metadata.loaded_at_epoch,
                load_status: fallback_metadata.load_status.clone(),
                load_error: fallback_metadata.load_error.clone(),
            },
            fallback_users,
        );
    };

    let loaded_at_epoch = Some(Utc::now().timestamp());
    let source_path = Some(path.to_string());
    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            return (
                IdentityBindingMetadata {
                    format: "separate-registry".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            )
        }
    };

    let value = match serde_json::from_str::<Value>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return (
                IdentityBindingMetadata {
                    format: "separate-registry".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            )
        }
    };

    let Some(object) = value.as_object() else {
        return (
            IdentityBindingMetadata {
                format: "separate-registry".to_string(),
                version: 0,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "invalid_shape".to_string(),
                load_error: Some("identity registry document must be a JSON object".to_string()),
            },
            HashMap::new(),
        );
    };

    if object.contains_key("version")
        || object.contains_key("revision")
        || object.contains_key("product_users")
    {
        return match serde_json::from_value::<ProductUserRegistryDocument>(value.clone()) {
            Ok(document) => (
                IdentityBindingMetadata {
                    format: "separate-registry-document".to_string(),
                    version: document.version,
                    revision: document.revision,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                document.product_users,
            ),
            Err(err) => (
                IdentityBindingMetadata {
                    format: "separate-registry-document".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            ),
        };
    }

    match serde_json::from_value::<HashMap<String, ProductUserIdentity>>(value) {
        Ok(product_users) => (
            IdentityBindingMetadata {
                format: "separate-registry-flat-map".to_string(),
                version: 1,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "loaded".to_string(),
                load_error: None,
            },
            product_users,
        ),
        Err(err) => (
            IdentityBindingMetadata {
                format: "separate-registry-flat-map".to_string(),
                version: 1,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "parse_error".to_string(),
                load_error: Some(err.to_string()),
            },
            HashMap::new(),
        ),
    }
}

fn load_identity_binding_store(config: &ConsumerEntryConfig) -> IdentityBindingStore {
    let Some(path) = config.identity_bindings_path.as_deref() else {
        return IdentityBindingStore::default();
    };

    let loaded_at_epoch = Some(Utc::now().timestamp());
    let source_path = Some(path.to_string());
    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            return IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "none".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            }
        }
    };

    let value = match serde_json::from_str::<Value>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "none".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            }
        }
    };

    let Some(object) = value.as_object() else {
        return IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "none".to_string(),
                version: 0,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "invalid_shape".to_string(),
                load_error: Some("identity binding document must be a JSON object".to_string()),
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
    };

    let store = if object.contains_key("version") || object.contains_key("revision") {
        match serde_json::from_value::<IdentityBindingsDocument>(value.clone()) {
            Ok(document) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "versioned-document".to_string(),
                    version: document.version,
                    revision: document.revision,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings {
                    chat_users: document.chat_users,
                    matrix_users: document.matrix_users,
                },
                product_users: document.product_users,
            },
            Err(err) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "versioned-document".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            },
        }
    } else {
        match serde_json::from_value::<IdentityBindings>(value) {
            Ok(bindings) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "legacy-flat-map".to_string(),
                    version: 1,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings,
                product_users: HashMap::new(),
            },
            Err(err) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "legacy-flat-map".to_string(),
                    version: 1,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            },
        }
    };

    let (registry_metadata, product_users) = load_product_user_registry(
        config.identity_registry_path.as_deref(),
        store.product_users.clone(),
        &store.metadata,
    );

    IdentityBindingStore {
        registry_metadata,
        product_users,
        ..store
    }
}

fn count_product_user_refs(bindings: &HashMap<String, IdentityBindingEntry>) -> usize {
    bindings
        .values()
        .filter(|binding| {
            binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
        })
        .count()
}

fn count_inline_identity_bindings(bindings: &HashMap<String, IdentityBindingEntry>) -> usize {
    bindings
        .values()
        .filter(|binding| {
            binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
                && (binding.org_id.is_some() || binding.account_id.is_some())
        })
        .count()
}

fn count_missing_product_user_refs(store: &IdentityBindingStore) -> usize {
    store
        .bindings
        .chat_users
        .values()
        .chain(store.bindings.matrix_users.values())
        .filter(|binding| {
            let Some(product_user_id) = binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return false;
            };
            !store.product_users.contains_key(product_user_id)
        })
        .count()
}

fn identity_source_of_truth_mode(store: &IdentityBindingStore) -> &'static str {
    let registry_refs = count_product_user_refs(&store.bindings.chat_users)
        + count_product_user_refs(&store.bindings.matrix_users);
    let inline_only = count_inline_identity_bindings(&store.bindings.chat_users)
        + count_inline_identity_bindings(&store.bindings.matrix_users);

    match (registry_refs > 0, inline_only > 0) {
        (true, true) => "mixed",
        (true, false) => "product_user_registry",
        _ => "inline_bindings",
    }
}

fn identity_binding_counts_json(store: &IdentityBindingStore) -> Value {
    json!({
        "chat_users": store.bindings.chat_users.len(),
        "matrix_users": store.bindings.matrix_users.len(),
        "product_users": store.product_users.len(),
        "chat_product_user_refs": count_product_user_refs(&store.bindings.chat_users),
        "matrix_product_user_refs": count_product_user_refs(&store.bindings.matrix_users),
        "inline_chat_users": count_inline_identity_bindings(&store.bindings.chat_users),
        "inline_matrix_users": count_inline_identity_bindings(&store.bindings.matrix_users),
        "missing_product_user_refs": count_missing_product_user_refs(store),
        "source_of_truth_mode": identity_source_of_truth_mode(store),
    })
}

fn metadata_revision(metadata: &IdentityBindingMetadata) -> Option<String> {
    metadata
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn effective_identity_revision(
    binding_metadata: &IdentityBindingMetadata,
    registry_metadata: &IdentityBindingMetadata,
    separate_registry_configured: bool,
) -> Option<String> {
    let binding_revision = metadata_revision(binding_metadata);
    if !separate_registry_configured {
        return binding_revision;
    }
    let registry_revision = metadata_revision(registry_metadata);
    match (binding_revision, registry_revision) {
        (Some(binding_revision), Some(registry_revision)) => Some(format!(
            "binding:{binding_revision}|registry:{registry_revision}"
        )),
        _ => None,
    }
}

fn normalize_revision_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if normalized.iter().any(|existing| existing == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

fn load_identity_binding_revision_approval_state(
    config: &ConsumerEntryConfig,
) -> IdentityBindingRevisionApprovalState {
    let Some(path) = config.identity_binding_approved_revisions_path.as_deref() else {
        return IdentityBindingRevisionApprovalState::default();
    };

    let loaded_at_epoch = Some(Utc::now().timestamp());
    let source_path = Some(path.to_string());
    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            return IdentityBindingRevisionApprovalState {
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "read_error".to_string(),
                load_error: Some(err.to_string()),
                version: 0,
                revision: None,
                approved_revisions: Vec::new(),
            }
        }
    };

    let document = match serde_json::from_str::<IdentityBindingRevisionApprovalDocument>(&raw) {
        Ok(document) => document,
        Err(err) => {
            return IdentityBindingRevisionApprovalState {
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "parse_error".to_string(),
                load_error: Some(err.to_string()),
                version: 0,
                revision: None,
                approved_revisions: Vec::new(),
            }
        }
    };

    IdentityBindingRevisionApprovalState {
        source_path,
        source_modified_epoch,
        loaded_at_epoch,
        load_status: "loaded".to_string(),
        load_error: None,
        version: document.version,
        revision: document.revision,
        approved_revisions: normalize_revision_list(document.approved_revisions),
    }
}

fn load_session_auth_issuer_registry_revision_approval_state(
    config: &ConsumerEntryConfig,
) -> SessionAuthIssuerRegistryRevisionApprovalState {
    let Some(path) = config
        .session_auth_issuer_registry_approved_revisions_path
        .as_deref()
    else {
        return SessionAuthIssuerRegistryRevisionApprovalState::default();
    };

    let loaded_at_epoch = Some(Utc::now().timestamp());
    let source_path = Some(path.to_string());
    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            return SessionAuthIssuerRegistryRevisionApprovalState {
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "read_error".to_string(),
                load_error: Some(err.to_string()),
                version: 0,
                revision: None,
                approved_revisions: Vec::new(),
            }
        }
    };

    let document =
        match serde_json::from_str::<SessionAuthIssuerRegistryRevisionApprovalDocument>(&raw) {
            Ok(document) => document,
            Err(err) => {
                return SessionAuthIssuerRegistryRevisionApprovalState {
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                    version: 0,
                    revision: None,
                    approved_revisions: Vec::new(),
                }
            }
        };

    SessionAuthIssuerRegistryRevisionApprovalState {
        source_path,
        source_modified_epoch,
        loaded_at_epoch,
        load_status: "loaded".to_string(),
        load_error: None,
        version: document.version,
        revision: document.revision,
        approved_revisions: normalize_revision_list(document.approved_revisions),
    }
}

fn evaluate_identity_binding_reload_governance(
    config: &ConsumerEntryConfig,
    current: &IdentityBindingStore,
    candidate: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    requesting_actor: Option<&str>,
) -> IdentityBindingReloadGovernance {
    let separate_registry_configured = config.identity_registry_path.is_some();
    let candidate_revision = metadata_revision(&candidate.metadata);
    let current_revision = metadata_revision(&current.metadata);
    let candidate_registry_revision = metadata_revision(&candidate.registry_metadata);
    let current_registry_revision = metadata_revision(&current.registry_metadata);
    let candidate_effective_revision = effective_identity_revision(
        &candidate.metadata,
        &candidate.registry_metadata,
        separate_registry_configured,
    );
    let current_effective_revision = effective_identity_revision(
        &current.metadata,
        &current.registry_metadata,
        separate_registry_configured,
    );
    let current_missing_product_user_refs = count_missing_product_user_refs(current);
    let candidate_missing_product_user_refs = count_missing_product_user_refs(candidate);
    let candidate_revision_approved = candidate_effective_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_revision_index = current_effective_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let candidate_revision_index = candidate_effective_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });

    let (actor_authorized, actor_reason) = if config.identity_binding_reload_require_actor {
        if config.identity_binding_reload_allowed_actors.is_empty() {
            (
                Some(false),
                Some("no_allowed_actors_configured".to_string()),
            )
        } else {
            match requesting_actor {
                Some(actor) if !actor.trim().is_empty() => {
                    let actor_is_allowed = config
                        .identity_binding_reload_allowed_actors
                        .iter()
                        .any(|allowed| allowed == actor.trim());
                    if actor_is_allowed {
                        (Some(true), None)
                    } else {
                        (Some(false), Some("actor_not_allowed".to_string()))
                    }
                }
                _ => (Some(false), Some("actor_missing".to_string())),
            }
        }
    } else {
        (None, None)
    };

    let decision = if actor_authorized == Some(false) {
        (false, "rejected", "actor_not_authorized", true)
    } else if candidate.metadata.load_status != "loaded" {
        (false, "rejected", "candidate_not_loaded", false)
    } else if separate_registry_configured && candidate.registry_metadata.load_status != "loaded" {
        (false, "rejected", "candidate_registry_not_loaded", false)
    } else if !config.identity_binding_reload_allow_legacy_format
        && candidate.metadata.format != "versioned-document"
    {
        (false, "rejected", "legacy_format_not_allowed", false)
    } else if candidate_missing_product_user_refs > 0 {
        (false, "rejected", "missing_product_user_refs", false)
    } else if config.identity_binding_reload_require_revision
        && candidate_effective_revision.is_none()
    {
        (
            false,
            "rejected",
            if separate_registry_configured {
                "effective_revision_required"
            } else {
                "revision_required"
            },
            false,
        )
    } else if config.identity_binding_reload_reject_same_revision
        && current_effective_revision.is_some()
        && current_effective_revision == candidate_effective_revision
    {
        (false, "rejected", "same_revision_rejected", false)
    } else if config.identity_binding_reload_require_approved_revision
        && approval_state.load_status != "loaded"
    {
        (false, "rejected", "approval_state_not_loaded", false)
    } else if config.identity_binding_reload_require_approved_revision
        && candidate_revision_approved != Some(true)
    {
        (false, "rejected", "candidate_revision_not_approved", false)
    } else if !config.identity_binding_reload_allow_rollback
        && current_effective_revision.is_some()
        && candidate_effective_revision.is_some()
        && approval_state.load_status != "loaded"
    {
        (
            false,
            "rejected",
            "rollback_policy_requires_approval_state",
            true,
        )
    } else if !config.identity_binding_reload_allow_rollback
        && current_effective_revision.is_some()
        && candidate_effective_revision.is_some()
        && (current_revision_index.is_none() || candidate_revision_index.is_none())
    {
        (false, "rejected", "rollback_order_unavailable", true)
    } else if !config.identity_binding_reload_allow_rollback
        && current_revision_index
            .zip(candidate_revision_index)
            .is_some_and(|(current_idx, candidate_idx)| candidate_idx < current_idx)
    {
        (false, "rejected", "rollback_revision_rejected", true)
    } else {
        (true, "accepted", "policy_ok", false)
    };

    IdentityBindingReloadGovernance {
        accepted: decision.0,
        decision: decision.1.to_string(),
        reason: decision.2.to_string(),
        current_format: current.metadata.format.clone(),
        current_revision,
        current_registry_revision,
        current_effective_revision,
        current_missing_product_user_refs,
        candidate_format: candidate.metadata.format.clone(),
        candidate_revision,
        candidate_registry_revision,
        candidate_effective_revision,
        candidate_load_status: candidate.metadata.load_status.clone(),
        candidate_registry_load_status: candidate.registry_metadata.load_status.clone(),
        candidate_missing_product_user_refs,
        separate_registry_configured,
        approval_state_status: approval_state.load_status.clone(),
        approval_state_revision: approval_state.revision.clone(),
        approved_revision_count: approval_state.approved_revisions.len(),
        candidate_revision_approved,
        rollback_blocked: decision.3,
        requesting_actor: requesting_actor.map(str::to_string),
        actor_header: config.identity_binding_reload_actor_header.clone(),
        actor_authorized,
        actor_reason,
    }
}

fn load_replay_cache(config: &ConsumerEntryConfig) -> ReplayCache {
    let Some(path) = config.replay_store_path.as_deref() else {
        return ReplayCache::default();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return ReplayCache::default();
    };

    let Ok(mut cache) = serde_json::from_str::<ReplayCache>(&raw) else {
        return ReplayCache::default();
    };

    prune_replay_cache(
        &mut cache,
        Utc::now().timestamp(),
        config.replay_window_secs,
        config.replay_cache_size,
    );
    cache
}

fn persist_replay_cache(cache: &ReplayCache, config: &ConsumerEntryConfig) {
    let Some(path) = config.replay_store_path.as_deref() else {
        return;
    };

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(body) = serde_json::to_vec(cache) {
        let _ = std::fs::write(path, body);
    }
}

fn max_rate_limit_entries(config: &ConsumerEntryConfig) -> usize {
    [
        config.rate_limit_max_requests,
        config.rate_limit_user_max_requests,
        config.rate_limit_room_max_requests,
        config.rate_limit_session_max_requests,
        config.rate_limit_org_max_requests,
    ]
    .into_iter()
    .max()
    .unwrap_or(DEFAULT_RATE_LIMIT_MAX_REQUESTS)
    .max(1)
}

fn prune_rate_limit_entries(
    entries: &mut VecDeque<i64>,
    now_epoch: i64,
    window_secs: u64,
    max_entries: usize,
) -> bool {
    let mut modified = false;
    while entries
        .front()
        .copied()
        .map(|seen_at| now_epoch - seen_at >= window_secs as i64)
        .unwrap_or(false)
    {
        entries.pop_front();
        modified = true;
    }
    while entries.len() > max_entries {
        entries.pop_front();
        modified = true;
    }
    modified
}

fn prune_rate_limit_cache(
    cache: &mut RateLimitCache,
    now_epoch: i64,
    window_secs: u64,
    max_entries: usize,
) {
    cache.seen.retain(|_, entries| {
        prune_rate_limit_entries(entries, now_epoch, window_secs, max_entries);
        !entries.is_empty()
    });
}

fn load_rate_limit_cache(config: &ConsumerEntryConfig) -> RateLimitCache {
    let Some(path) = config.rate_limit_store_path.as_deref() else {
        return RateLimitCache::default();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return RateLimitCache::default();
    };

    let Ok(mut cache) = serde_json::from_str::<RateLimitCache>(&raw) else {
        return RateLimitCache::default();
    };

    prune_rate_limit_cache(
        &mut cache,
        Utc::now().timestamp(),
        config.rate_limit_window_secs,
        max_rate_limit_entries(config),
    );
    cache
}

fn persist_rate_limit_cache(cache: &RateLimitCache, config: &ConsumerEntryConfig) {
    let Some(path) = config.rate_limit_store_path.as_deref() else {
        return;
    };

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(body) = serde_json::to_vec(cache) {
        let _ = std::fs::write(path, body);
    }
}

fn append_identity_binding_audit_event(
    config: &ConsumerEntryConfig,
    event_kind: &str,
    store: &IdentityBindingStore,
    governance: Option<&IdentityBindingReloadGovernance>,
) -> IdentityBindingAuditState {
    let Some(path) = config.identity_binding_audit_log_path.as_deref() else {
        return IdentityBindingAuditState::default();
    };

    let event_epoch = Utc::now().timestamp();
    let audit_event = json!({
        "event_kind": event_kind,
        "event_epoch": event_epoch,
        "identity_binding_metadata": {
            "format": store.metadata.format.clone(),
            "version": store.metadata.version,
            "revision": store.metadata.revision.clone(),
            "source_path": store.metadata.source_path.clone(),
            "source_modified_epoch": store.metadata.source_modified_epoch,
            "loaded_at_epoch": store.metadata.loaded_at_epoch,
            "load_status": store.metadata.load_status.clone(),
            "load_error": store.metadata.load_error.clone(),
        },
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": {
            "mode": identity_source_of_truth_mode(store),
            "product_users": store.product_users.len(),
            "missing_product_user_refs": count_missing_product_user_refs(store),
        },
        "identity_registry_metadata": {
            "format": store.registry_metadata.format.clone(),
            "version": store.registry_metadata.version,
            "revision": store.registry_metadata.revision.clone(),
            "source_path": store.registry_metadata.source_path.clone(),
            "source_modified_epoch": store.registry_metadata.source_modified_epoch,
            "loaded_at_epoch": store.registry_metadata.loaded_at_epoch,
            "load_status": store.registry_metadata.load_status.clone(),
            "load_error": store.registry_metadata.load_error.clone(),
        },
        "governance": governance,
    });

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        if let Err(err) = std::fs::create_dir_all(parent) {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "write_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            };
        }
    }

    let line = match serde_json::to_string(&audit_event) {
        Ok(line) => format!("{line}\n"),
        Err(err) => {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "serialize_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            }
        }
    };

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => file,
        Err(err) => {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "write_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            }
        }
    };

    match std::io::Write::write_all(&mut file, line.as_bytes()) {
        Ok(_) => IdentityBindingAuditState {
            path: Some(path.to_string()),
            last_event_kind: Some(event_kind.to_string()),
            last_event_epoch: Some(event_epoch),
            last_status: "written".to_string(),
            last_error: None,
            last_policy_decision: governance.map(|g| g.decision.clone()),
            last_policy_reason: governance.map(|g| g.reason.clone()),
        },
        Err(err) => IdentityBindingAuditState {
            path: Some(path.to_string()),
            last_event_kind: Some(event_kind.to_string()),
            last_event_epoch: Some(event_epoch),
            last_status: "write_error".to_string(),
            last_error: Some(err.to_string()),
            last_policy_decision: governance.map(|g| g.decision.clone()),
            last_policy_reason: governance.map(|g| g.reason.clone()),
        },
    }
}

impl AppState {
    pub async fn from_env() -> Result<Self, String> {
        let config = ConsumerEntryConfig::from_env();
        if let Err(errors) = config.validate_runtime_profile() {
            return Err(format!(
                "invalid consumer-entry-api runtime profile ({}): {}",
                config.runtime_profile.as_str(),
                errors.join("; ")
            ));
        }
        let league_state = load_league_state_for_startup(&config).await?;
        Ok(Self::new_with_league_state(config, league_state))
    }

    pub fn new(config: ConsumerEntryConfig) -> Self {
        let league_state = load_league_state(&config);
        Self::new_with_league_state(config, league_state)
    }

    fn new_with_league_state(config: ConsumerEntryConfig, league_state: LeagueState) -> Self {
        let identity_binding_store = load_identity_binding_store(&config);
        let identity_binding_audit_state = append_identity_binding_audit_event(
            &config,
            "startup_load",
            &identity_binding_store,
            None,
        );
        let session_auth_issuer_registry_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: config.session_auth_issuer_registry_metadata.clone(),
            registry: config.session_auth_issuer_registry.clone(),
        };
        let rate_limits = load_rate_limit_cache(&config);
        let replay_cache = load_replay_cache(&config);
        let metrics = ConsumerEntryMetrics::default();
        if identity_binding_audit_state.last_status != "written"
            && config.identity_binding_audit_log_path.is_some()
        {
            metrics.inc_identity_binding_audit_failures();
        }
        Self {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                identity_binding_store: RwLock::new(identity_binding_store),
                identity_binding_audit_state: RwLock::new(identity_binding_audit_state),
                session_auth_issuer_registry_state: StdRwLock::new(
                    session_auth_issuer_registry_state,
                ),
                league_state: Mutex::new(league_state),
                health_world_readiness_cache_generation: AtomicU64::new(0),
                health_world_readiness_cache: Mutex::new(None),
                rate_limits: Mutex::new(rate_limits),
                replay_cache: Mutex::new(replay_cache),
                metrics,
            }),
        }
    }

    pub fn config(&self) -> &ConsumerEntryConfig {
        &self.inner.config
    }
}

fn session_auth_issuer_registry_runtime_state(
    state: &AppState,
) -> SessionAuthIssuerRegistryRuntimeState {
    state
        .inner
        .session_auth_issuer_registry_state
        .read()
        .expect("session auth issuer registry state lock poisoned")
        .clone()
}

async fn get_favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/favicon.ico", get(get_favicon))
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/app", get(get_client_app_web_shell_response))
        .route("/league", get(get_league_web_shell))
        .route("/world", get(get_world_web_shell_response))
        .route("/league/web/session", post(post_league_web_session))
        .route("/league/web/action", post(post_league_web_action))
        .route("/app/web/map-viewport", get(get_world_web_map_viewport))
        .route("/world/web/map-viewport", get(get_world_web_map_viewport))
        .route("/world/web/map-delta", get(get_world_web_map_delta))
        .route("/world/web/map-rum", post(post_world_web_map_rum))
        .route("/world/web/action", post(post_world_web_action))
        .route(
            "/world/web/tactics-command",
            post(post_world_web_tactics_command),
        )
        .route("/world/web/map-move", post(post_world_web_map_move))
        .route(
            "/world/web/contract",
            post(post_world_web_contract_complete),
        )
        .route("/world/web/asset", post(post_world_web_asset_upgrade))
        .route("/world/web/company", post(post_world_web_company))
        .route("/world/web/listing", post(post_world_web_listing))
        .route("/world/web/buy", post(post_world_web_listing_buy))
        .route("/world/web/work-deliver", post(post_world_web_work_deliver))
        .route("/world/web/work-accept", post(post_world_web_work_accept))
        .route("/world/web/work-reject", post(post_world_web_work_reject))
        .route("/world/web/work-reopen", post(post_world_web_work_reopen))
        .route("/world/web/work-cancel", post(post_world_web_work_cancel))
        .route("/app/web/feed", get(get_client_web_feed_home))
        .route("/v1/chat/tasks", post(create_chat_task))
        .route("/v1/chat/tasks/:id", get(get_chat_task))
        .route("/v1/matrix/messages", post(create_matrix_message_task))
        .route("/v1/league/home", get(get_league_home))
        .route("/v1/league/world", get(get_league_world))
        .route("/v1/world/home", get(get_world_home))
        .route("/v1/client/app/:matrix_user_id", get(get_client_app_home))
        .route("/v1/client/feed/:matrix_user_id", get(get_client_feed_home))
        .route("/v1/world/map/:matrix_user_id", get(get_world_map))
        .route(
            "/v1/world/map/:matrix_user_id/viewport",
            get(get_world_map_viewport),
        )
        .route(
            "/v1/world/map/:matrix_user_id/delta",
            get(get_world_map_delta),
        )
        .route(
            "/v1/world/map/:matrix_user_id/rum",
            post(post_world_map_rum),
        )
        .route("/v1/world/map/move", post(move_world_map))
        .route("/v1/world/action", post(post_world_action))
        .route(
            "/v1/world/tactics/command",
            post(post_world_tactics_command),
        )
        .route("/v1/world/assets", get(get_world_assets))
        .route(
            "/v1/world/assets/:asset_id/upgrade",
            post(upgrade_world_asset),
        )
        .route(
            "/v1/world/companies",
            get(get_world_companies).post(create_world_company),
        )
        .route("/v1/world/shops", get(get_world_shops))
        .route("/v1/world/listings", post(create_world_listing))
        .route("/v1/world/commerce", get(get_world_commerce))
        .route("/v1/world/factions", get(get_world_factions))
        .route(
            "/v1/world/listings/:listing_id/buy",
            post(buy_world_listing),
        )
        .route(
            "/v1/world/work-orders/:work_order_id/deliver",
            post(deliver_world_work_order),
        )
        .route(
            "/v1/world/work-orders/:work_order_id/accept",
            post(accept_world_work_order),
        )
        .route(
            "/v1/world/work-orders/:work_order_id/reject",
            post(reject_world_work_order),
        )
        .route(
            "/v1/world/work-orders/:work_order_id/reopen",
            post(reopen_world_work_order),
        )
        .route(
            "/v1/world/work-orders/:work_order_id/cancel",
            post(cancel_world_work_order),
        )
        .route("/v1/world/contracts", get(get_world_contracts))
        .route(
            "/v1/world/contracts/:contract_id/complete",
            post(complete_world_contract),
        )
        .route("/v1/league/season", get(get_league_season))
        .route("/v1/league/matches", get(get_league_matches))
        .route("/v1/league/state/snapshot", get(get_league_state_snapshot))
        .route("/v1/league/raids", get(get_league_raids))
        .route(
            "/v1/league/raids/:match_id/contribute",
            post(contribute_league_raid),
        )
        .route(
            "/v1/league/raids/:match_id/roster",
            get(get_league_raid_roster).post(join_league_raid_roster),
        )
        .route("/v1/league/reviews/held", get(get_league_held_reviews))
        .route(
            "/v1/league/reviews/:reward_id/approve",
            post(approve_league_review),
        )
        .route(
            "/v1/league/reviews/:reward_id/reject",
            post(reject_league_review),
        )
        .route("/v1/league/guilds", get(get_league_guilds))
        .route("/v1/league/guilds/:guild_id/join", post(join_league_guild))
        .route("/v1/league/matches/:match_id/join", post(join_league_match))
        .route(
            "/v1/league/matches/:match_id/battle",
            post(create_league_battle),
        )
        .route(
            "/v1/league/matches/:match_id/submit",
            post(submit_league_match),
        )
        .route("/v1/league/rankings", get(get_league_rankings))
        .route(
            "/v1/league/players/:matrix_user_id/profile",
            get(get_league_player_profile),
        )
        .route(
            "/v1/league/players/:matrix_user_id/progression",
            get(get_league_player_progression),
        )
        .route(
            "/v1/league/players/:matrix_user_id/loadout",
            get(get_league_player_loadout),
        )
        .route(
            "/v1/league/players/:matrix_user_id/draft",
            post(update_league_player_draft),
        )
        .route(
            "/v1/league/players/:matrix_user_id/rewards",
            get(get_league_player_rewards),
        )
        .route(
            "/v1/league/players/:matrix_user_id/inventory",
            get(get_league_player_inventory),
        )
        .route(
            "/v1/league/players/:matrix_user_id/history",
            get(get_league_player_history),
        )
        .route(
            "/v1/matrix/users/:matrix_user_id/wallet",
            get(get_matrix_wallet),
        )
        .route(
            "/v1/admin/identity-bindings/reload",
            post(reload_identity_bindings),
        )
        .route(
            "/v1/admin/identity-registry/reload",
            post(reload_identity_registry),
        )
        .route(
            "/v1/admin/identity-registry/validate",
            post(validate_identity_registry),
        )
        .route(
            "/v1/admin/identity-registry/status",
            get(get_identity_registry_status),
        )
        .route(
            "/v1/admin/identity-registry/audit",
            get(get_identity_registry_audit),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/reload",
            post(reload_session_auth_issuer_registry),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/status",
            get(get_session_auth_issuer_registry_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/validate",
            post(validate_session_auth_issuer_registry),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/approval/status",
            get(get_session_auth_issuer_registry_approval_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/approval/validate",
            post(validate_session_auth_issuer_registry_approval),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/actors/status",
            get(get_session_auth_issuer_registry_actor_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/actors/validate",
            post(validate_session_auth_issuer_registry_actors),
        )
        .route(
            "/v1/admin/identity-approval/status",
            get(get_identity_approval_status),
        )
        .route(
            "/v1/admin/identity-approval/validate",
            post(validate_identity_approval),
        )
        .route(
            "/v1/admin/identity-approval/source",
            get(get_identity_approval_source),
        )
        .route(
            "/v1/admin/identity-approval/source/validate",
            post(validate_identity_approval_source),
        )
        .route(
            "/v1/admin/identity-governance/status",
            get(get_identity_governance_status),
        )
        .route(
            "/v1/admin/identity-governance/validate",
            post(validate_identity_governance),
        )
        .route(
            "/v1/admin/identity-actors/status",
            get(get_identity_actor_status),
        )
        .route(
            "/v1/admin/identity-actors/validate",
            post(validate_identity_actors),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests;
