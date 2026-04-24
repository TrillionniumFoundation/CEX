use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shared_tracing::init_tracing;
use std::{collections::VecDeque, env, fs, path::Path, time::SystemTime, time::UNIX_EPOCH};
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PollerConfig {
    homeserver_base_url: String,
    matrix_access_token: String,
    bot_relay_base_url: String,
    bot_user_id: String,
    poll_interval_ms: u64,
    sync_filter: String,
    state_file: String,
    max_recent_event_ids: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SyncState {
    next_batch: Option<String>,
    #[serde(default)]
    recent_event_ids: VecDeque<String>,
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    next_batch: String,
    rooms: Option<RoomsState>,
}

#[derive(Debug, Deserialize)]
struct RoomsState {
    join: Option<std::collections::HashMap<String, JoinRoomState>>,
}

#[derive(Debug, Deserialize)]
struct JoinRoomState {
    timeline: Option<TimelineState>,
}

#[derive(Debug, Deserialize)]
struct TimelineState {
    events: Vec<Value>,
}

#[tokio::main]
async fn main() {
    init_tracing();
    let config = PollerConfig::from_env();
    info!(
        homeserver = %config.homeserver_base_url,
        relay = %config.bot_relay_base_url,
        bot_user_id = %config.bot_user_id,
        max_recent_event_ids = config.max_recent_event_ids,
        "starting matrix-bot-poller"
    );

    let state_path = config.state_file.clone();
    let mut state = load_state(&state_path);
    trim_recent_event_ids(&mut state, config.max_recent_event_ids);

    let http = reqwest::Client::new();

    loop {
        let mut sync_url = format!(
            "{}/_matrix/client/v3/sync?timeout=30000&set_presence=offline",
            config.homeserver_base_url.trim_end_matches('/')
        );

        if let Some(next_batch) = state.next_batch.as_deref() {
            sync_url.push_str("&since=");
            sync_url.push_str(next_batch);
        }

        let sync_url = sync_url;
        let sync_request = if config.sync_filter.is_empty() {
            http.get(&sync_url).bearer_auth(&config.matrix_access_token)
        } else {
            http.get(&sync_url)
                .query(&[("filter", config.sync_filter.as_str())])
                .bearer_auth(&config.matrix_access_token)
        };

        let sync_response = sync_request.send().await;

        match sync_response {
            Ok(resp) if resp.status().is_success() => match resp.json::<SyncResponse>().await {
                Ok(body) => {
                    if let Some(rooms) = body.rooms {
                        if let Some(joined_rooms) = rooms.join {
                            for (room_id, room_state) in joined_rooms {
                                if let Some(timeline) = room_state.timeline {
                                    for raw_event in timeline.events {
                                        if let Some(event_type) =
                                            raw_event.get("type").and_then(Value::as_str)
                                        {
                                            if event_type != "m.room.message" {
                                                continue;
                                            }
                                        } else {
                                            continue;
                                        }

                                        let sender =
                                            match raw_event.get("sender").and_then(Value::as_str) {
                                                Some(v) => v.to_string(),
                                                None => continue,
                                            };

                                        if sender == config.bot_user_id {
                                            continue;
                                        }

                                        let content = raw_event
                                            .get("content")
                                            .cloned()
                                            .unwrap_or_else(|| json!({}));
                                        let text = content
                                            .get("body")
                                            .and_then(Value::as_str)
                                            .unwrap_or("")
                                            .to_string();

                                        let msgtype =
                                            content.get("msgtype").and_then(Value::as_str);
                                        let should_process =
                                            msgtype.map(|m| m == "m.text").unwrap_or(false);
                                        if !should_process || text.trim().is_empty() {
                                            continue;
                                        }

                                        let event_id = raw_event
                                            .get("event_id")
                                            .and_then(Value::as_str)
                                            .map(|v| v.to_string());
                                        let event_id_for_log =
                                            event_id.as_deref().unwrap_or("<missing>");

                                        let is_duplicate = event_id
                                            .as_ref()
                                            .is_some_and(|id| state.recent_event_ids.contains(id));

                                        if is_duplicate {
                                            warn!(
                                                room = %room_id,
                                                event_id = %event_id_for_log,
                                                "skipping duplicate event"
                                            );
                                            continue;
                                        }

                                        if let Some(id) = event_id.clone() {
                                            state.recent_event_ids.push_front(id);
                                            trim_recent_event_ids(
                                                &mut state,
                                                config.max_recent_event_ids,
                                            );
                                        }

                                        let timestamp_ms = raw_event
                                            .get("origin_server_ts")
                                            .and_then(Value::as_i64);

                                        let inbound = json!({
                                            "event_id": event_id,
                                            "event_type": "m.room.message",
                                            "room_id": room_id,
                                            "sender": sender,
                                            "text": text,
                                            "content": content,
                                            "timestamp_ms": timestamp_ms,
                                            "metadata": {
                                                "polled_at": SystemTime::now()
                                                    .duration_since(UNIX_EPOCH)
                                                    .map(|d| d.as_millis() as i64)
                                                    .unwrap_or_default()
                                            }
                                        });

                                        let relay_url = format!(
                                            "{}/v1/inbound/matrix-event",
                                            config.bot_relay_base_url.trim_end_matches('/')
                                        );
                                        let relay_resp =
                                            http.post(relay_url).json(&inbound).send().await;

                                        if let Err(err) = relay_resp {
                                            warn!(
                                                %err,
                                                room = %room_id,
                                                event_id = %event_id_for_log,
                                                "failed to forward event to relay"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    state.next_batch = Some(body.next_batch.clone());
                    if let Err(err) = save_state(&state_path, &state) {
                        warn!(%err, "failed to persist matrix sync state");
                    }
                }
                Err(err) => {
                    error!(%err, "failed to parse sync response json");
                }
            },
            Ok(resp) => {
                let status = resp.status();
                let txt = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "<non-text body>".to_string());
                warn!(%status, body = %txt, "matrix sync non-200");
            }
            Err(err) => {
                warn!(%err, "matrix sync request failed");
            }
        }

        let wait_ms = if config.poll_interval_ms == 0 {
            3000
        } else {
            config.poll_interval_ms
        };
        sleep(TokioDuration::from_millis(wait_ms)).await;
    }
}

impl PollerConfig {
    fn from_env() -> Self {
        Self {
            homeserver_base_url: env::var("MATRIX_POLL_HOMESERVER")
                .unwrap_or_else(|_| "http://127.0.0.1:8008".to_string()),
            matrix_access_token: env::var("MATRIX_ACCESS_TOKEN").unwrap_or_default(),
            bot_relay_base_url: env::var("MATRIX_BOT_RELAY_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8092".to_string()),
            bot_user_id: env::var("MATRIX_BOT_USER_ID")
                .unwrap_or_else(|_| "@cex-bot:local.dev".to_string()),
            poll_interval_ms: env::var("MATRIX_POLL_INTERVAL_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
            sync_filter: env::var("MATRIX_SYNC_FILTER").unwrap_or_else(|_| "0".to_string()),
            state_file: env::var("MATRIX_SYNC_STATE_FILE")
                .unwrap_or_else(|_| "/tmp/matrix-bot-poller.state".to_string()),
            max_recent_event_ids: env::var("MATRIX_POLL_MAX_RECENT_EVENT_IDS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1000),
        }
    }
}

fn load_state(path: &str) -> SyncState {
    if !Path::new(path).exists() {
        return SyncState::default();
    }

    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => SyncState::default(),
    }
}

fn save_state(path: &str, state: &SyncState) -> std::io::Result<()> {
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(path, raw)
}

fn trim_recent_event_ids(state: &mut SyncState, max: usize) {
    while state.recent_event_ids.len() > max {
        state.recent_event_ids.pop_back();
    }
}
