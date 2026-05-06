use super::*;

const TRILLIONNIUM_PLAYABILITY_COACH_CONTRACT_VERSION: &str = "trillionnium_playability_coach_v1";
const TRILLIONNIUM_ECONOMY_RETENTION_OPS_CONTRACT_VERSION: &str =
    "trillionnium_economy_retention_ops_v1";

#[derive(Debug, Clone)]
pub(super) struct ClientRouteWorldContext {
    active_region_id: String,
    viewport: Value,
    route_preview: Value,
    route_task_graph: Value,
    route_task_views: Vec<WorldRouteTaskGraphView>,
    route_story: WorldRouteStoryView,
}

impl ClientRouteWorldContext {
    fn active_region_id_from_viewport(
        viewport: &Value,
        fallback_active_region_id: Option<&str>,
    ) -> String {
        viewport
            .get("active_region")
            .and_then(|region| region.get("region_id"))
            .and_then(Value::as_str)
            .or(fallback_active_region_id)
            .unwrap_or("cn-shanghai-core")
            .to_string()
    }

    fn from_viewport_and_artifacts(
        viewport: Value,
        fallback_active_region_id: Option<&str>,
        route_artifacts: &WorldRouteArtifacts,
    ) -> Self {
        let active_region_id =
            Self::active_region_id_from_viewport(&viewport, fallback_active_region_id);
        Self {
            active_region_id,
            viewport,
            route_preview: route_artifacts.preview.clone(),
            route_task_graph: route_artifacts.task_graph.clone(),
            route_task_views: route_artifacts.task_views.clone(),
            route_story: route_artifacts.story.clone(),
        }
    }

    fn from_world_with_artifacts(
        world: &WorldState,
        matrix_user_id: &str,
        live_event_limit: Option<usize>,
        fallback_active_region_id: Option<&str>,
        route_artifacts: &WorldRouteArtifacts,
    ) -> Self {
        let viewport = world_map_viewport_json(
            world,
            matrix_user_id,
            None,
            None,
            None,
            None,
            live_event_limit,
        );
        Self::from_viewport_and_artifacts(viewport, fallback_active_region_id, route_artifacts)
    }

    fn into_client_app_map_hub_json(self, metrics: &ClientAppMapHubMetrics) -> Value {
        json!({
            "ui_role": "primary_super_entry",
            "entry_priority": 1,
            "active_region_id": self.active_region_id,
            "region_shard_count": metrics.region_shard_count,
            "lod_layer_count": metrics.lod_layer_count,
            "tile_shard_count": metrics.tile_shard_count,
            "nearby_poi_count": metrics.nearby_poi_count,
            "prefetch_count": metrics.prefetch_count,
            "live_event_count": metrics.live_event_count,
            "player_avatar_count": metrics.player_avatar_count,
            "avatar_task_route_count": metrics.avatar_task_route_count,
            "avatar_route_runner_count": metrics.avatar_route_runner_count,
            "player_density_mode": metrics.player_density_mode.clone(),
            "estimated_concurrent_players": metrics.estimated_concurrent_players,
            "viewport": self.viewport,
            "route_preview": self.route_preview,
            "route_task_graph": self.route_task_graph,
            "route_story": self.route_story.to_value(),
            "route_contract": world_route_ui_contract_json(),
        })
    }
}

pub(super) fn client_feed_group_for_kind(feed_kind: &str) -> &str {
    match feed_kind {
        "commerce_purchase" | "work_order" | "delivery" => "commerce",
        "social_agent" => "social",
        _ => feed_kind,
    }
}

pub(super) fn client_feed_group_from_item(item: &Value) -> String {
    item.get("feed_group")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            client_feed_group_for_kind(
                item.get("feed_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("update"),
            )
            .to_string()
        })
}

const CLIENT_FEED_FILTER_SPECS: [(&str, &str); 7] = [
    ("all", "推荐"),
    ("live_event", "事件"),
    ("route_task", "任务"),
    ("contract", "委托"),
    ("completion", "战报"),
    ("commerce", "冒险"),
    ("social", "社交"),
];

pub(super) fn client_feed_filter_specs() -> &'static [(&'static str, &'static str)] {
    &CLIENT_FEED_FILTER_SPECS
}

pub(super) fn client_feed_filter_labels_js_object() -> String {
    let mut labels = Map::new();
    for (key, label) in client_feed_filter_specs() {
        labels.insert((*key).to_string(), json!(*label));
    }
    Value::Object(labels).to_string()
}

#[derive(Debug, Clone)]
pub(super) struct ClientFeedSurfaceView {
    pub(super) api_path: String,
    pub(super) web_session_path: String,
    pub(super) active_region_id: String,
    pub(super) item_count: u64,
    contract_count: u64,
    completion_count: u64,
    purchase_count: u64,
    work_order_count: u64,
    nearby_agent_count: usize,
    items: Vec<Value>,
}

impl ClientFeedSurfaceView {
    pub(super) fn from_feed_value(feed: Option<&Value>) -> Self {
        let api_path = feed
            .and_then(|feed| feed.get("api_path"))
            .and_then(Value::as_str)
            .unwrap_or("/app/web/feed")
            .to_string();
        let web_session_path = feed
            .and_then(|feed| feed.get("web_session_path"))
            .and_then(Value::as_str)
            .unwrap_or("/app/web/feed")
            .to_string();
        let active_region_id = feed
            .and_then(|feed| feed.get("active_region_id"))
            .and_then(Value::as_str)
            .unwrap_or("cn-shanghai-core")
            .to_string();
        let item_count = feed
            .and_then(|feed| feed.get("item_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let contract_count = feed
            .and_then(|feed| feed.get("snapshots"))
            .and_then(|snapshots| snapshots.get("contracts"))
            .and_then(|contracts| contracts.get("count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let completion_count = feed
            .and_then(|feed| feed.get("snapshots"))
            .and_then(|snapshots| snapshots.get("completions"))
            .and_then(|completions| completions.get("count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let purchase_count = feed
            .and_then(|feed| feed.get("snapshots"))
            .and_then(|snapshots| snapshots.get("commerce"))
            .and_then(|commerce| commerce.get("purchase_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let work_order_count = feed
            .and_then(|feed| feed.get("snapshots"))
            .and_then(|snapshots| snapshots.get("commerce"))
            .and_then(|commerce| commerce.get("work_order_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let nearby_agent_count = feed
            .and_then(|feed| feed.get("snapshots"))
            .and_then(|snapshots| snapshots.get("social"))
            .and_then(|social| social.get("nearby_agents"))
            .and_then(Value::as_array)
            .map(|agents| agents.len())
            .unwrap_or(0);
        let items = feed
            .and_then(|feed| feed.get("items"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Self {
            api_path,
            web_session_path,
            active_region_id,
            item_count,
            contract_count,
            completion_count,
            purchase_count,
            work_order_count,
            nearby_agent_count,
            items,
        }
    }

    fn filter_count(&self, key: &str) -> usize {
        if key == "all" {
            self.items.len()
        } else {
            self.items
                .iter()
                .filter(|item| client_feed_group_from_item(item) == key)
                .count()
        }
    }

    pub(super) fn filter_chips_html(&self) -> String {
        client_feed_filter_specs()
            .iter()
            .map(|(key, label)| {
                let active = if *key == "all" { " is-active" } else { "" };
                format!(
                    "<button type=\"button\" class=\"focus-chip trillionnium-app-feed-filter{}\" data-feed-filter=\"{}\">{} · {}</button>",
                    active,
                    escape_html_text(key),
                    escape_html_text(label),
                    self.filter_count(key),
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(super) fn summary_chips_html(&self) -> String {
        [
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> 条动态</span>",
                self.item_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> 个委托</span>",
                self.contract_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> 份战报</span>",
                self.completion_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> 次接取 · <strong>{}</strong> 个冒险委托</span>",
                self.purchase_count, self.work_order_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> 位附近角色 · {}</span>",
                self.nearby_agent_count,
                escape_html_text(&self.active_region_id),
            ),
        ]
        .join(" ")
    }

    pub(super) fn item_cards_html(&self, limit: usize) -> String {
        self.items
            .iter()
            .take(limit)
            .map(|item| client_feed_card_html(item, None))
            .collect::<Vec<_>>()
            .join("")
    }
}

pub(super) fn map_region_focus_button_html(lat: f64, lng: f64, zoom: i64, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"region\" data-lat=\"{:.6}\" data-lng=\"{:.6}\" data-zoom=\"{}\">{}</button>",
        lat,
        lng,
        zoom,
        map_focus_button_label_html(label),
    )
}

pub(super) fn map_node_focus_button_html(node_id: &str, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"node\" data-node-id=\"{}\">{}</button>",
        escape_html_text(node_id),
        map_focus_button_label_html(label),
    )
}

pub(super) fn map_tile_focus_button_html(z: i64, x: i64, y: i64, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"tile\" data-tile-z=\"{}\" data-tile-x=\"{}\" data-tile-y=\"{}\">{}</button>",
        z,
        x,
        y,
        map_focus_button_label_html(label),
    )
}

fn map_focus_button_label_html(label: &str) -> String {
    let copy = match label {
        "聚焦区域" => "Focus region / 聚焦区域",
        "聚焦热点" => "Focus hotspot / 聚焦热点",
        "查看分片" => "View tile / 查看分片",
        "查看地图分片" => "View map tile / 查看地图分片",
        "预热分片" => "Warm tile / 预热分片",
        "追踪事件" => "Track event / 追踪事件",
        "行动" => "Action / 行动",
        _ => label,
    };
    i18n_span_from_bilingual_slash_copy(copy).unwrap_or_else(|| escape_html_text(copy))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn map_event_focus_button_html(
    node_id: &str,
    event_id: &str,
    task_id: &str,
    location_id: &str,
    event_kind: &str,
    node_name: &str,
    event_body: &str,
    event_result: &str,
    label: &str,
) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"event\" data-node-id=\"{}\" data-event-id=\"{}\" data-task-id=\"{}\" data-location-id=\"{}\" data-event-kind=\"{}\" data-node-name=\"{}\" data-event-body=\"{}\" data-event-result=\"{}\" data-suppress-action=\"true\">{}</button>",
        escape_html_text(node_id),
        escape_html_text(event_id),
        escape_html_text(task_id),
        escape_html_text(location_id),
        escape_html_text(event_kind),
        escape_html_text(node_name),
        escape_html_text(event_body),
        escape_html_text(event_result),
        map_focus_button_label_html(label),
    )
}

pub(super) fn map_overlay_control_buttons_html() -> &'static str {
    r#"          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="density" aria-pressed="true">Density</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="regions" aria-pressed="true">Regions</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="tiles" aria-pressed="true">Tiles</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="prefetch" aria-pressed="true">Prefetch</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="events" aria-pressed="true">Live events</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="taskRoutes" aria-pressed="true">Task routes</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="routeRunners" aria-pressed="true">Moving avatars</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="avatars" aria-pressed="true">Avatars</button>"#
}

pub(super) fn map_camera_action_buttons_html() -> &'static str {
    r#"          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="active_region">Center active region</button>
          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="nearest_poi">Nearest hotspot</button>
          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="hottest_event">Hottest event</button>"#
}

pub(super) fn route_filter_buttons_html(
    button_class: &str,
    selection_label_en: &str,
    selection_label_zh: &str,
    all_label_en: &str,
    all_label_zh: &str,
) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip {}\" data-route-filter=\"selection\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</button>\n            <button type=\"button\" class=\"focus-chip {}\" data-route-filter=\"all\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</button>",
        escape_html_text(button_class),
        escape_html_text(selection_label_en),
        escape_html_text(selection_label_zh),
        escape_html_text(selection_label_en),
        escape_html_text(button_class),
        escape_html_text(all_label_en),
        escape_html_text(all_label_zh),
        escape_html_text(all_label_en),
    )
}

pub(super) fn client_feed_focus_button_html(item: &Value) -> Option<String> {
    if item.get("focus_kind").and_then(Value::as_str) != Some("event") {
        return None;
    }
    Some(map_event_focus_button_html(
        item.get("focus_node_id")
            .and_then(Value::as_str)
            .unwrap_or(""),
        item.get("focus_event_id")
            .and_then(Value::as_str)
            .unwrap_or(""),
        item.get("focus_task_id")
            .and_then(Value::as_str)
            .unwrap_or(""),
        item.get("focus_location_id")
            .and_then(Value::as_str)
            .unwrap_or(""),
        item.get("focus_event_kind")
            .and_then(Value::as_str)
            .unwrap_or("world_event"),
        item.get("focus_node_name")
            .and_then(Value::as_str)
            .unwrap_or("POI"),
        item.get("focus_event_body")
            .and_then(Value::as_str)
            .unwrap_or(""),
        item.get("focus_event_result")
            .and_then(Value::as_str)
            .unwrap_or(""),
        "去地图看",
    ))
}

pub(super) fn client_feed_action_button_html(
    item: &Value,
    action_body_override: Option<&str>,
) -> Option<String> {
    let action_label = item
        .get("action_label")
        .and_then(Value::as_str)
        .unwrap_or("");
    if action_label.is_empty() {
        return None;
    }
    let action_body = action_body_override.unwrap_or_else(|| {
        item.get("action_body_base")
            .and_then(Value::as_str)
            .unwrap_or("")
    });
    Some(
        WorldRouteActionButtonView {
            label: action_label,
            panel_id: item
                .get("action_panel_id")
                .and_then(Value::as_str)
                .unwrap_or(WORLD_ROUTE_ACTION_PANEL_ID),
            input_id: item
                .get("action_input_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            input_value: item
                .get("action_input_value")
                .and_then(Value::as_str)
                .unwrap_or(""),
            textarea_id: item
                .get("action_textarea_id")
                .and_then(Value::as_str)
                .unwrap_or(WORLD_ROUTE_ACTION_TEXTAREA_ID),
            location_id: item
                .get("action_location_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            target_node_id: item
                .get("action_target_node_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            task_id: item
                .get("action_task_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            contract_id: item
                .get("action_contract_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            listing_id: item
                .get("action_listing_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            work_order_id: item
                .get("action_work_order_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            event_id: item
                .get("action_event_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            event_kind: item
                .get("action_event_kind")
                .and_then(Value::as_str)
                .unwrap_or(""),
            event_body: item
                .get("action_event_body")
                .and_then(Value::as_str)
                .unwrap_or(""),
            event_result: item
                .get("action_event_result")
                .and_then(Value::as_str)
                .unwrap_or(""),
            event_task_id: item
                .get("action_event_task_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            body: action_body,
        }
        .render("trillionnium-app-feed-action"),
    )
}

pub(super) fn client_feed_card_html(item: &Value, action_body_override: Option<&str>) -> String {
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Feed item");
    let summary = item.get("summary").and_then(Value::as_str).unwrap_or("");
    let detail = item.get("detail").and_then(Value::as_str).unwrap_or("feed");
    let feed_kind = item
        .get("feed_kind")
        .and_then(Value::as_str)
        .unwrap_or("update");
    let feed_group = client_feed_group_from_item(item);
    let source = item.get("source").and_then(Value::as_str).unwrap_or("feed");
    let buttons = [
        client_feed_focus_button_html(item),
        client_feed_action_button_html(item, action_body_override),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    format!(
        "<article class=\"module app-feed-item\" data-feed-kind=\"{}\" data-feed-group=\"{}\"><strong>{}</strong><span>{}</span><p>{}</p><div class=\"focus-stack\"><code>{}</code>{}</div></article>",
        escape_html_text(feed_kind),
        escape_html_text(&feed_group),
        escape_html_text(title),
        escape_html_text(detail),
        escape_html_text(summary),
        escape_html_text(source),
        if buttons.is_empty() {
            String::new()
        } else {
            format!(" {}", buttons.join(" "))
        },
    )
}

pub(super) fn client_route_world_context(
    league: &LeagueState,
    matrix_user_id: &str,
    live_event_limit: Option<usize>,
    fallback_active_region_id: Option<&str>,
) -> ClientRouteWorldContext {
    let route_artifacts = build_world_route_artifacts(&league.world);
    client_route_world_context_with_artifacts(
        &league.world,
        matrix_user_id,
        live_event_limit,
        fallback_active_region_id,
        &route_artifacts,
    )
}

pub(super) fn client_route_world_context_with_artifacts(
    world: &WorldState,
    matrix_user_id: &str,
    live_event_limit: Option<usize>,
    fallback_active_region_id: Option<&str>,
    route_artifacts: &WorldRouteArtifacts,
) -> ClientRouteWorldContext {
    ClientRouteWorldContext::from_world_with_artifacts(
        world,
        matrix_user_id,
        live_event_limit,
        fallback_active_region_id,
        route_artifacts,
    )
}

#[derive(Debug, Clone)]
pub(super) struct ClientFeedSnapshots {
    live_event_stream: Vec<Value>,
    nearby_agents: Vec<Value>,
    recent_contracts: Vec<Value>,
    recent_completions: Vec<Value>,
    recent_purchases: Vec<Value>,
    recent_work_orders: Vec<Value>,
    recent_deliveries: Vec<Value>,
}

pub(super) fn build_client_feed_snapshots(
    world: &WorldState,
    indexes: &WorldIndexes,
    viewport: &Value,
) -> ClientFeedSnapshots {
    let live_event_stream = viewport
        .get("live_event_stream")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let nearby_agents = indexes
        .sorted_entity_ids
        .iter()
        .filter_map(|entity_id| world.world_entities.get(entity_id))
        .filter(|entity| entity.status == "available")
        .take(6)
        .map(|entity| {
            json!({
                "entity_id": &entity.entity_id,
                "name": &entity.name,
                "entity_kind": &entity.entity_kind,
                "role": &entity.role,
                "location_id": &entity.location_id,
                "status": &entity.status,
            })
        })
        .collect();
    let recent_contracts =
        indexed_recent(&world.world_contracts, &indexes.recent_contract_indices, 6)
            .map(|contract| {
                json!({
                    "contract_id": &contract.contract_id,
                    "task_id": &contract.task_id,
                    "location_id": &contract.location_id,
                    "title": &contract.title,
                    "body": &contract.body,
                    "status": &contract.status,
                    "value_score": contract.value_score,
                    "created_at_epoch": contract.created_at_epoch,
                })
            })
            .collect();
    let recent_completions = indexed_recent(
        &world.world_contract_completions,
        &indexes.recent_contract_completion_indices,
        6,
    )
    .map(|completion| {
        json!({
            "completion_id": &completion.completion_id,
            "contract_id": &completion.contract_id,
            "body": &completion.body,
            "score": completion.score,
            "reward_amount": completion.reward_amount,
            "payout_status": &completion.payout_status,
            "ledger_status": &completion.ledger_status,
            "created_at_epoch": completion.created_at_epoch,
        })
    })
    .collect();
    let recent_purchases =
        indexed_recent(&world.world_purchases, &indexes.recent_purchase_indices, 6)
            .map(|purchase| {
                json!({
                    "purchase_id": &purchase.purchase_id,
                    "listing_id": &purchase.listing_id,
                    "company_id": &purchase.company_id,
                    "price_credits": purchase.price_credits,
                    "status": &purchase.status,
                    "created_at_epoch": purchase.created_at_epoch,
                })
            })
            .collect();
    let recent_work_orders = indexed_recent(
        &world.world_work_orders,
        &indexes.recent_work_order_indices,
        6,
    )
    .map(|work_order| {
        json!({
            "work_order_id": &work_order.work_order_id,
            "purchase_id": &work_order.purchase_id,
            "listing_id": &work_order.listing_id,
            "company_id": &work_order.company_id,
            "brief": &work_order.brief,
            "status": &work_order.status,
            "value_score": work_order.value_score,
            "created_at_epoch": work_order.created_at_epoch,
        })
    })
    .collect();
    let recent_deliveries = indexed_recent(
        &world.world_work_deliveries,
        &indexes.recent_work_delivery_indices,
        6,
    )
    .map(|delivery| {
        json!({
            "delivery_id": &delivery.delivery_id,
            "work_order_id": &delivery.work_order_id,
            "body": &delivery.body,
            "status": &delivery.status,
            "score": delivery.score,
            "created_at_epoch": delivery.created_at_epoch,
        })
    })
    .collect();

    ClientFeedSnapshots {
        live_event_stream,
        nearby_agents,
        recent_contracts,
        recent_completions,
        recent_purchases,
        recent_work_orders,
        recent_deliveries,
    }
}

pub(super) fn build_client_feed_items(
    world: &WorldState,
    indexes: &WorldIndexes,
    route_task_views: &[WorldRouteTaskGraphView],
    snapshots: &ClientFeedSnapshots,
) -> Vec<Value> {
    let mut items: Vec<Value> = snapshots
        .live_event_stream
        .iter()
        .take(8)
        .map(|event| {
            let node_name = event
                .get("node_name")
                .and_then(Value::as_str)
                .unwrap_or("World event");
            let result = event.get("result").and_then(Value::as_str).unwrap_or("queued");
            let impact_score = event
                .get("impact_score")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            json!({
                "feed_kind": "live_event",
                "source": "live_event_stream",
                "event_id": event.get("event_id").cloned().unwrap_or(Value::Null),
                "task_id": event.get("cex_task_id").cloned().unwrap_or(Value::Null),
                "location_id": event.get("location_id").cloned().unwrap_or(Value::Null),
                "node_id": event.get("node_id").cloned().unwrap_or(Value::Null),
                "node_name": event.get("node_name").cloned().unwrap_or_else(|| json!(node_name)),
                "event_kind": event.get("event_kind").cloned().unwrap_or_else(|| json!("world_event")),
                "event_result": event.get("result").cloned().unwrap_or_else(|| json!(result)),
                "title": event
                    .get("event_kind")
                    .cloned()
                    .unwrap_or_else(|| json!("world_event")),
                "summary": event.get("body").cloned().unwrap_or_else(|| json!("")),
                "detail": format!("{} · {} · impact +{}", node_name, result, impact_score),
                "created_at_epoch": event
                    .get("created_at_epoch")
                    .cloned()
                    .unwrap_or_else(|| json!(0)),
            })
        })
        .collect();
    items.extend(
        route_task_views
            .iter()
            .map(WorldRouteTaskGraphView::to_feed_item),
    );
    items.extend(snapshots.recent_contracts.iter().take(4).map(|contract| {
        let contract_id = contract
            .get("contract_id")
            .and_then(Value::as_str)
            .unwrap_or("contract");
        let task_id = contract.get("task_id").and_then(Value::as_str).unwrap_or("");
        let status = contract.get("status").and_then(Value::as_str).unwrap_or("open");
        let value_score = contract
            .get("value_score")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        json!({
            "feed_kind": "contract",
            "source": "contract_snapshot",
            "contract_id": contract_id,
            "task_id": task_id,
            "location_id": contract.get("location_id").cloned().unwrap_or(Value::Null),
            "title": contract.get("title").cloned().unwrap_or_else(|| json!(format!("Contract {}", contract_id))),
            "summary": contract.get("body").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("task {} · {} · value {}", if task_id.is_empty() { "unlinked" } else { task_id }, status, value_score),
            "created_at_epoch": contract.get("created_at_epoch").cloned().unwrap_or_else(|| json!(0)),
        })
    }));
    items.extend(snapshots.recent_completions.iter().take(4).map(|completion| {
        let completion_id = completion
            .get("completion_id")
            .and_then(Value::as_str)
            .unwrap_or("completion");
        let contract_id = completion
            .get("contract_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let linked_contract = indexes.contract(world, contract_id);
        let task_id = linked_contract.map(|contract| contract.task_id.as_str()).unwrap_or("");
        let location_id = linked_contract
            .map(|contract| contract.location_id.as_str())
            .unwrap_or("");
        let score = completion.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        let reward_amount = completion
            .get("reward_amount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let payout_status = completion
            .get("payout_status")
            .and_then(Value::as_str)
            .unwrap_or("queued");
        json!({
            "feed_kind": "completion",
            "source": "quest_report_snapshot",
            "completion_id": completion_id,
            "contract_id": contract_id,
            "task_id": task_id,
            "location_id": location_id,
            "title": format!("战报 {}", completion_id),
            "summary": completion.get("body").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("任务 {} · 评分 {:.1} · 奖励 {:.2} · {}", if task_id.is_empty() { "unlinked" } else { task_id }, score, reward_amount, payout_status),
            "created_at_epoch": completion.get("created_at_epoch").cloned().unwrap_or_else(|| json!(0)),
        })
    }));
    items.extend(snapshots.recent_purchases.iter().take(4).map(|purchase| {
        let purchase_id = purchase
            .get("purchase_id")
            .and_then(Value::as_str)
            .unwrap_or("purchase");
        let price_credits = purchase
            .get("price_credits")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let status = purchase
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending");
        json!({
            "feed_kind": "commerce_purchase",
            "source": "adventure_snapshot",
            "purchase_id": purchase_id,
            "listing_id": purchase.get("listing_id").cloned().unwrap_or(Value::Null),
            "company_id": purchase.get("company_id").cloned().unwrap_or(Value::Null),
            "title": format!("接取契约 {}", purchase_id),
            "summary": purchase.get("listing_id").cloned().unwrap_or_else(|| json!("任务牌")),
            "detail": format!("赏金 {} · {}", price_credits, status),
            "created_at_epoch": purchase.get("created_at_epoch").cloned().unwrap_or_else(|| json!(0)),
        })
    }));
    items.extend(snapshots.recent_work_orders.iter().take(4).map(|work_order| {
        let work_order_id = work_order
            .get("work_order_id")
            .and_then(Value::as_str)
            .unwrap_or("work_order");
        let status = work_order
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("queued");
        let value_score = work_order
            .get("value_score")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        json!({
            "feed_kind": "work_order",
            "source": "adventure_snapshot",
            "work_order_id": work_order_id,
            "purchase_id": work_order.get("purchase_id").cloned().unwrap_or(Value::Null),
            "listing_id": work_order.get("listing_id").cloned().unwrap_or(Value::Null),
            "company_id": work_order.get("company_id").cloned().unwrap_or(Value::Null),
            "title": format!("冒险委托 {}", work_order_id),
            "summary": work_order.get("brief").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("{} · 难度 {}", status, value_score),
            "created_at_epoch": work_order.get("created_at_epoch").cloned().unwrap_or_else(|| json!(0)),
        })
    }));
    items.extend(snapshots.recent_deliveries.iter().take(4).map(|delivery| {
        let delivery_id = delivery
            .get("delivery_id")
            .and_then(Value::as_str)
            .unwrap_or("delivery");
        let work_order_id = delivery
            .get("work_order_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let linked_work_order = indexes
            .work_order_index(work_order_id)
            .and_then(|index| world.world_work_orders.get(index));
        let status = delivery
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("submitted");
        let score = delivery.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        json!({
            "feed_kind": "delivery",
            "source": "adventure_snapshot",
            "delivery_id": delivery_id,
            "work_order_id": work_order_id,
            "listing_id": linked_work_order.map(|work_order| work_order.listing_id.clone()).unwrap_or_default(),
            "company_id": linked_work_order.map(|work_order| work_order.company_id.clone()).unwrap_or_default(),
            "title": format!("成果提交 {}", delivery_id),
            "summary": delivery.get("body").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("委托 {} · {} · 评分 {:.1}", if work_order_id.is_empty() { "pending" } else { work_order_id }, status, score),
            "created_at_epoch": delivery.get("created_at_epoch").cloned().unwrap_or_else(|| json!(0)),
        })
    }));
    items.extend(snapshots.nearby_agents.iter().take(4).map(|agent| {
        let name = agent.get("name").and_then(Value::as_str).unwrap_or("Agent");
        let entity_kind = agent
            .get("entity_kind")
            .and_then(Value::as_str)
            .unwrap_or("agent");
        let role = agent.get("role").and_then(Value::as_str).unwrap_or("available");
        let location_id = agent
            .get("location_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        json!({
            "feed_kind": "social_agent",
            "source": "social_snapshot",
            "entity_id": agent.get("entity_id").cloned().unwrap_or(Value::Null),
            "location_id": location_id,
            "title": name,
            "summary": format!("{} · {}", entity_kind, role),
            "detail": if location_id.is_empty() { "social link ready".to_string() } else { format!("available near {}", location_id) },
            "created_at_epoch": 0,
        })
    }));
    items
}

pub(super) fn client_feed_object_value(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Value {
    object.get(key).cloned().unwrap_or(Value::Null)
}

pub(super) fn client_feed_object_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    fallback: &str,
) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn decorate_live_event_feed_item(object: &mut serde_json::Map<String, Value>) {
    let location_id = client_feed_object_value(object, "location_id");
    let task_id = client_feed_object_value(object, "task_id");
    let event_id = client_feed_object_value(object, "event_id");
    let event_kind = object
        .get("event_kind")
        .cloned()
        .unwrap_or_else(|| json!("world_event"));
    let summary = object.get("summary").cloned().unwrap_or_else(|| json!(""));
    let event_result = object
        .get("event_result")
        .cloned()
        .unwrap_or_else(|| json!(""));
    object.insert("focus_kind".to_string(), json!("event"));
    object.insert(
        "focus_node_id".to_string(),
        client_feed_object_value(object, "node_id"),
    );
    object.insert("focus_event_id".to_string(), event_id.clone());
    object.insert("focus_task_id".to_string(), task_id.clone());
    object.insert("focus_location_id".to_string(), location_id.clone());
    object.insert("focus_event_kind".to_string(), event_kind.clone());
    object.insert(
        "focus_node_name".to_string(),
        object
            .get("node_name")
            .cloned()
            .unwrap_or_else(|| json!("POI")),
    );
    object.insert("focus_event_body".to_string(), summary.clone());
    object.insert("focus_event_result".to_string(), event_result.clone());
    let title = client_feed_object_string(object, "title", "Feed item");
    ClientFeedActionTarget::from_route_target(world_route_action_console_target(
        "继续推进",
        format!(
            "{}: continue this feed signal with current evidence, owner, risk, and next action.",
            title
        ),
    ))
    .with_location_id(location_id)
    .with_task_id(task_id.clone())
    .with_event_details(event_id, event_kind, summary, event_result, task_id)
    .apply(object);
}

pub(super) fn decorate_route_task_feed_item(object: &mut serde_json::Map<String, Value>) {
    ClientFeedActionTarget::from_values(
        object
            .get("next_opportunity_action_label")
            .cloned()
            .unwrap_or_else(|| json!("推进下一条支线")),
        object
            .get("next_opportunity_panel_id")
            .cloned()
            .unwrap_or_else(|| json!(WORLD_ROUTE_ACTION_PANEL_ID)),
        object
            .get("next_opportunity_input_id")
            .cloned()
            .unwrap_or_else(|| json!("")),
        object
            .get("next_opportunity_input_value")
            .cloned()
            .unwrap_or_else(|| json!("")),
        object
            .get("next_opportunity_textarea_id")
            .cloned()
            .unwrap_or_else(|| json!(WORLD_ROUTE_ACTION_TEXTAREA_ID)),
        object
            .get("next_opportunity_body")
            .cloned()
            .or_else(|| object.get("next_opportunity_command").cloned())
            .unwrap_or_else(|| json!("")),
    )
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_target_node_id(
        object
            .get("next_opportunity_node_id")
            .cloned()
            .unwrap_or_else(|| json!("")),
    )
    .with_task_id(client_feed_object_value(object, "task_id"))
    .with_contract_id(client_feed_object_value(object, "latest_contract_id"))
    .apply(object);
}

pub(super) fn decorate_contract_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Contract");
    let contract_id = client_feed_object_string(object, "contract_id", "");
    ClientFeedActionTarget::from_route_target(world_route_contract_lane_target(
        "打开委托",
        contract_id,
        format!("{}: 更新契约证据、评级标准和下一步路线。", title),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_task_id(client_feed_object_value(object, "task_id"))
    .with_contract_id(client_feed_object_value(object, "contract_id"))
    .apply(object);
}

pub(super) fn decorate_completion_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "战报");
    ClientFeedActionTarget::from_route_target(world_route_action_console_target(
        "复盘收益",
        format!("{}: 复盘奖励、证据质量、下一段支线和声望成长。", title),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_task_id(client_feed_object_value(object, "task_id"))
    .with_contract_id(client_feed_object_value(object, "contract_id"))
    .apply(object);
}

pub(super) fn decorate_purchase_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "契约接取");
    let listing_id = client_feed_object_string(object, "listing_id", "");
    ClientFeedActionTarget::from_route_target(world_route_purchase_lane_target(
        "打开契约",
        listing_id,
        format!(
            "{}: 读取委托目标、评级范围，并把这次接取推进成清晰的冒险路线。",
            title
        ),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_listing_id(client_feed_object_value(object, "listing_id"))
    .apply(object);
}

pub(super) fn decorate_work_lane_feed_item(
    object: &mut serde_json::Map<String, Value>,
    action_label: &str,
    lane_kind: &str,
    body: String,
) {
    ClientFeedActionTarget::from_route_target(world_route_work_lane_target_by_kind(
        action_label,
        lane_kind,
        body,
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_listing_id(client_feed_object_value(object, "listing_id"))
    .with_work_order_id(client_feed_object_value(object, "work_order_id"))
    .apply(object);
}

pub(super) fn decorate_work_order_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "冒险委托");
    decorate_work_lane_feed_item(
        object,
        "打开委托",
        "delivery",
        format!("{}: 检查委托目标，收束成果范围，并清掉下一处阻碍。", title),
    );
}

pub(super) fn decorate_delivery_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "成果提交");
    decorate_work_lane_feed_item(
        object,
        "去评级",
        "acceptance",
        format!("{}: 评定成果质量、证据完整度和是否需要返工。", title),
    );
}

pub(super) fn decorate_social_agent_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Agent");
    ClientFeedActionTarget::from_route_target(world_route_action_console_target(
        "去世界",
        format!("{}: 把附近 Agent 接入当前路线、契约或实时事件。", title),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .apply(object);
}

pub(super) fn decorate_client_feed_items(items: &mut [Value]) {
    items.iter_mut().for_each(|item| {
        let Some(object) = item.as_object_mut() else {
            return;
        };
        let feed_kind = object
            .get("feed_kind")
            .and_then(Value::as_str)
            .unwrap_or("update")
            .to_string();
        let feed_group = client_feed_group_for_kind(&feed_kind).to_string();
        object.insert("feed_group".to_string(), json!(feed_group));
        match feed_kind.as_str() {
            "live_event" => decorate_live_event_feed_item(object),
            "route_task" => decorate_route_task_feed_item(object),
            "contract" => decorate_contract_feed_item(object),
            "completion" => decorate_completion_feed_item(object),
            "commerce_purchase" => decorate_purchase_feed_item(object),
            "work_order" => decorate_work_order_feed_item(object),
            "delivery" => decorate_delivery_feed_item(object),
            "social_agent" => decorate_social_agent_feed_item(object),
            _ => {}
        }
    });
}

#[derive(Debug, Clone)]
pub(super) struct ClientFeedProjectionContext<'a> {
    world: &'a WorldState,
    matrix_user_id: &'a str,
    guild_count: usize,
    route_context: ClientRouteWorldContext,
}

impl<'a> ClientFeedProjectionContext<'a> {
    fn new(
        world: &'a WorldState,
        matrix_user_id: &'a str,
        guild_count: usize,
        route_context: ClientRouteWorldContext,
    ) -> Self {
        Self {
            world,
            matrix_user_id,
            guild_count,
            route_context,
        }
    }

    fn json(self) -> Value {
        let ClientRouteWorldContext {
            active_region_id,
            viewport,
            route_preview,
            route_task_graph,
            route_task_views,
            route_story,
            ..
        } = self.route_context;
        let indexes = build_world_indexes(self.world);
        let snapshots = build_client_feed_snapshots(self.world, &indexes, &viewport);
        let mut items =
            build_client_feed_items(self.world, &indexes, &route_task_views, &snapshots);
        decorate_client_feed_items(&mut items);
        items.sort_by(|left, right| {
            right
                .get("created_at_epoch")
                .and_then(Value::as_i64)
                .cmp(&left.get("created_at_epoch").and_then(Value::as_i64))
        });

        json!({
            "kind": "trillionnium_client_feed",
            "projection_layer": "client_feed_projection_v1",
            "projection_context": "ClientFeedProjectionContext",
            "index_layer": "WorldIndexes::client_feed_recent_indices_v1",
            "matrix_user_id": self.matrix_user_id,
            "api_path": format!("/v1/client/feed/{}", self.matrix_user_id),
            "web_session_path": "/app/web/feed",
            "active_region_id": active_region_id,
            "item_count": items.len(),
            "source_count": 6,
            "sources": [
                "live_event_stream",
                "route_preview",
                "route_task_graph",
                "contract_snapshot",
                "adventure_snapshot",
                "social_snapshot"
            ],
            "items": items,
            "live_event_stream": snapshots.live_event_stream,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
            "route_story": route_story.to_value(),
            "route_contract": world_route_ui_contract_json(),
            "snapshots": {
                "contracts": {
                    "count": self.world.world_contracts.len(),
                    "recent": snapshots.recent_contracts,
                },
                "completions": {
                    "count": self.world.world_contract_completions.len(),
                    "recent": snapshots.recent_completions,
                },
                "commerce": {
                    "purchase_count": self.world.world_purchases.len(),
                    "work_order_count": self.world.world_work_orders.len(),
                    "delivery_count": self.world.world_work_deliveries.len(),
                    "recent_purchases": snapshots.recent_purchases,
                    "recent_work_orders": snapshots.recent_work_orders,
                    "recent_deliveries": snapshots.recent_deliveries,
                },
                "social": {
                    "nearby_agents": snapshots.nearby_agents,
                    "guild_count": self.guild_count,
                    "entity_count": self.world.world_entities.len(),
                }
            }
        })
    }
}

pub(super) fn build_client_feed_json_from_context(
    world: &WorldState,
    matrix_user_id: &str,
    guild_count: usize,
    context: ClientRouteWorldContext,
) -> Value {
    ClientFeedProjectionContext::new(world, matrix_user_id, guild_count, context).json()
}

pub(super) fn client_feed_json(league: &LeagueState, matrix_user_id: &str) -> Value {
    let context = client_route_world_context(league, matrix_user_id, Some(10), None);
    build_client_feed_json_from_context(&league.world, matrix_user_id, league.guilds.len(), context)
}

#[derive(Debug, Clone)]
pub(super) struct ClientAppProjectionContext<'a> {
    league: &'a LeagueState,
    world: &'a WorldState,
    matrix_user_id: &'a str,
    guild_count: usize,
    indexes: WorldIndexes,
    route_artifacts: WorldRouteArtifacts,
    map: Value,
    real_world_map_engine: Value,
}

impl<'a> ClientAppProjectionContext<'a> {
    fn new(league: &'a LeagueState, matrix_user_id: &'a str) -> Self {
        let world = &league.world;
        let indexes = build_world_indexes(world);
        let route_artifacts = build_world_route_artifacts(world);
        let map = world_map_json_with_route_artifacts(world, matrix_user_id, &route_artifacts);
        let real_world_map_engine = map
            .get("real_world_map_engine")
            .cloned()
            .unwrap_or_else(|| real_world_map_engine_json(&[], None));
        Self {
            league,
            world,
            matrix_user_id,
            guild_count: league.guilds.len(),
            indexes,
            route_artifacts,
            map,
            real_world_map_engine,
        }
    }

    fn engine_active_region_id(&self) -> Option<&str> {
        self.real_world_map_engine
            .get("active_region_id")
            .and_then(Value::as_str)
    }

    fn app_route_context(&self) -> ClientRouteWorldContext {
        client_route_world_context_with_artifacts(
            self.world,
            self.matrix_user_id,
            None,
            self.engine_active_region_id(),
            &self.route_artifacts,
        )
    }

    fn feed_context(&self, active_region_id: &str) -> ClientRouteWorldContext {
        client_route_world_context_with_artifacts(
            self.world,
            self.matrix_user_id,
            Some(10),
            Some(active_region_id),
            &self.route_artifacts,
        )
    }

    fn feed_json(&self, active_region_id: &str) -> Value {
        build_client_feed_json_from_context(
            self.world,
            self.matrix_user_id,
            self.guild_count,
            self.feed_context(active_region_id),
        )
    }

    fn current_node_id(&self) -> String {
        self.map
            .get("current_node_id")
            .and_then(Value::as_str)
            .unwrap_or(default_world_node_id())
            .to_string()
    }

    fn current_node_name(&self) -> String {
        self.map
            .get("current_node")
            .and_then(|node| node.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("镜像城市广场")
            .to_string()
    }

    fn nearby_agents(&self) -> Vec<WorldEntity> {
        self.indexes
            .sorted_entity_ids
            .iter()
            .filter_map(|entity_id| self.world.world_entities.get(entity_id))
            .filter(|entity| entity.status == "available")
            .cloned()
            .collect()
    }

    fn progression_player(&self) -> LeaguePlayer {
        self.league
            .players_by_matrix_user
            .get(self.matrix_user_id)
            .cloned()
            .unwrap_or_else(|| LeaguePlayer {
                player_id: league_hash_id("player", self.matrix_user_id),
                matrix_user_id: self.matrix_user_id.to_string(),
                display_name: self.matrix_user_id.to_string(),
                class_tag: "summoner".to_string(),
                rank_tier: "Bronze I".to_string(),
                rating: 1000,
                xp: 0,
                reputation: 0,
                battles: 0,
                submissions: 0,
                wins: 0,
                earned_credits: 0.0,
                created_at_epoch: 0,
            })
    }

    fn progression_json(&self) -> Value {
        let progression_player = self.progression_player();
        league_player_progression_json(self.league, &progression_player, self.matrix_user_id)
    }

    fn map_hub_metrics(&self, map_viewport: &Value) -> ClientAppMapHubMetrics {
        ClientAppMapHubMetrics::from_engine_and_viewport(&self.real_world_map_engine, map_viewport)
    }

    fn progression_summary(&self, progression: &Value) -> ClientAppProgressionSummary {
        ClientAppProgressionSummary::from_progression_json(progression)
    }

    fn first_playable_onboarding_json(
        &self,
        active_region_id: &str,
        current_node_id: &str,
        current_node_name: &str,
        progression_summary: &ClientAppProgressionSummary,
        map_metrics: &ClientAppMapHubMetrics,
    ) -> Value {
        let starter_world_action = format!(
            "/world action Global bounty found at {}: define customer deliverable, outcome standard, evidence package, risk controls, next action, and self-review.",
            current_node_name
        );
        json!({
            "contract_version": "trillionnium_first_playable_onboarding_v1",
            "rail_id": "first_playable_main_quest_rail",
            "rail_label": "Starter Quest / 新手主线：from map focus to bounty reward / 从地图到悬赏完成",
            "completion_target": "first_playable_loop_100",
            "quick_path_label": "Quick Path",
            "quick_path_label_zh": "快速路径",
            "quick_path_summary": "Choose map focus → run one bounty → submit/review reward",
            "quick_path_summary_zh": "选择地图焦点 → 跑一个悬赏 → 提交/查看奖励",
            "quick_path_steps": [
                {
                    "label": "1 · Choose map focus",
                    "label_zh": "1 · 选择地图焦点",
                    "description": "Tap a city place, region, or live event.",
                    "description_zh": "点选城市地点、区域或实时事件。"
                },
                {
                    "label": "2 · Run one bounty",
                    "label_zh": "2 · 跑一个悬赏",
                    "description": "Start the first world action and capture it as a rated commission.",
                    "description_zh": "发起第一次世界行动，并登记为待评级委托。"
                },
                {
                    "label": "3 · Submit / review reward",
                    "label_zh": "3 · 提交 / 查看奖励",
                    "description": "Deliver evidence, check rating, reward, and next route.",
                    "description_zh": "提交证据，查看评级、奖励和下一步路线。"
                }
            ],
            "command_disclosure_label": "Full Commands",
            "command_disclosure_label_zh": "完整命令",
            "command_disclosure": "Use these when you are ready to submit real work with deliverable, evidence, risk controls, next action, and self-review anchors.",
            "command_disclosure_zh": "准备真实提交时再展开：每条命令都要带交付物、证据、风险控制、下一步和自检锚点。",
            "launch_market": "global_first_overseas_beta",
            "primary_goal": "Move one map focus through exploration, contract, commission, result submission, rating, and reward / 把一个地图焦点推进成探索、契约、委托、成果提交、评级和奖励领取。",
            "current_state": {
                "matrix_user_id": self.matrix_user_id,
                "active_region_id": active_region_id,
                "current_node_id": current_node_id,
                "current_node_name": current_node_name,
                "progression_level": progression_summary.level,
                "successful_task_count": progression_summary.successful_task_count,
                "live_event_count": map_metrics.live_event_count,
                "nearby_poi_count": map_metrics.nearby_poi_count,
                "prefetch_count": map_metrics.prefetch_count,
                "player_density_mode": map_metrics.player_density_mode.clone(),
            },
            "entry_surfaces": ["/app", "/world", "/map", "/feed", "Matrix /app"],
            "steps": [
                {
                    "step_id": "orient_on_map",
                    "status": "ready",
                    "surface": "World / 世界",
                    "label": "Choose map focus / 选择地图焦点",
                    "description": "Choose a region, hotspot, tile, or live event in the reality mirror map / 先在现实镜像地图里选择区域、热点、地图块或实时事件，让后续行动有明确位置。",
                    "command": "/map",
                    "web_panel_id": "app-tab-map",
                    "success_signal": "Map focus selected / 地图焦点已选定",
                },
                {
                    "step_id": "start_world_action",
                    "status": "ready",
                    "surface": "World / 世界",
                    "label": "Start world action / 发起世界行动",
                    "description": "Turn the map focus into a global city bounty / 把地图焦点转成一个全球城市悬赏，写清委托目标、成果、证据、风险和下一步。",
                    "command": starter_world_action,
                    "web_panel_id": WORLD_ROUTE_ACTION_PANEL_ID,
                    "textarea_id": WORLD_ROUTE_ACTION_TEXTAREA_ID,
                    "success_signal": "World event created / 世界事件已创建",
                },
                {
                    "step_id": "capture_contract",
                    "status": "ready",
                    "surface": "World / Messages · 世界 / 消息",
                    "label": "Capture rated commission / 登记待评级委托",
                    "description": "Capture the world action as a trackable contract / 把世界行动收成可追踪契约，确保主线、消息和路线图都能看到同一个任务。",
                    "command": "/contract Start bounty contract: define customer deliverable, outcome standard, evidence package, risk controls, rating rules, next action, and self-review.",
                    "web_panel_id": WORLD_ROUTE_CONTRACTS_PANEL_ID,
                    "success_signal": "Contract opened / 契约已开启",
                },
                {
                    "step_id": "quest_delivery",
                    "status": "ready",
                    "surface": "Feed / World · 动态 / 世界",
                    "label": "Complete one bounty commission / 完成一次悬赏委托",
                    "description": "Run the full quest-card loop: accept, submit, rate/revise/reopen/cancel / 通过任务牌、接取、提交成果、评级/返工/重开/放弃跑完真实冒险循环。",
                    "command": "/work deliver latest First delivery: submit customer deliverable, evidence package, rating checklist, risk controls, next action, and self-review.",
                    "web_panel_id": WORLD_ROUTE_COMMERCE_PANEL_ID,
                    "input_id": WORLD_ROUTE_WORK_DELIVER_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID,
                    "success_signal": "Rating or revision recorded / 评级或返工路线已记录",
                },
                {
                    "step_id": "read_reward_and_next_route",
                    "status": "ready",
                    "surface": "Feed / Me · 动态 / 我",
                    "label": "Read reward and next route / 查看奖励与下一步路线",
                    "description": "After rating, check feed, rewards, progression, and route graph / 评级后检查动态、奖励、成长和路线图，把下一条支线/升级机会接起来。",
                    "command": "/app",
                    "web_panel_id": "app-tab-feed",
                    "success_signal": "Reward or next route visible / 奖励或下一条路线可见"
                }
            ],
            "acceptance_checks": [
                "map_focus_visible",
                "world_event_created",
                "contract_open_or_completed",
                "quest_work_order_created",
                "quest_rating_or_feedback_loop_visible",
                "wallet_progression_feed_updated",
                "route_task_graph_next_action_visible"
            ],
            "beta_readiness_checks": [
                "four_tab_mobile_shell_visible",
                "mobile_tablist_a11y_visible",
                "keyboard_tab_navigation_visible",
                "global_search_filters_active_tab",
                "search_empty_state_visible",
                "search_clear_and_escape_visible",
                "aria_live_ux_status_visible",
                "offline_feed_fallback_status_visible",
                "web_session_feed_hydration_visible",
                "next_action_rail_visible",
                "feed_api_hydration_visible",
                "playability_coach_visible",
                "p0_next_best_action_visible",
                "p1_strategy_choices_visible",
                "p2_retention_telemetry_visible",
                "failure_recovery_lane_visible",
                "matrix_app_card_exposes_onboarding"
            ],
            "full_vision_hooks": [
                "real_world_map_engine",
                "route_task_graph",
                "quest_lifecycle",
                "ledger_settlement",
                "matrix_social_loop",
                "normalized_repository_read_models"
            ]
        })
    }

    fn mobile_shell_contract_json(&self) -> Value {
        json!({
            "contract_version": "trillionnium_mobile_shell_ux_v1",
            "shell_id": "trillionnium_mobile_shell",
            "default_tab": "map",
            "tabs": [
                {"tab_id": "messages", "label": "消息", "panel_id": "app-tab-messages", "role": "tab"},
                {"tab_id": "map", "label": "世界", "panel_id": "app-tab-map", "role": "tab", "default_active": true},
                {"tab_id": "feed", "label": "动态", "panel_id": "app-tab-feed", "role": "tab"},
                {"tab_id": "me", "label": "我", "panel_id": "app-tab-me", "role": "tab"}
            ],
            "navigation": {
                "role": "tablist",
                "keyboard": ["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp", "Home", "End"],
                "state_attributes": ["aria-selected", "aria-controls", "aria-hidden", "tabindex", "hidden"],
            },
            "search": {
                "input_id": "app-global-search",
                "active_tab_filter": true,
                "clear_button_id": "app-search-clear",
                "empty_state_id": "app-search-empty-state",
                "escape_to_clear": true,
            },
            "live_status": {
                "visible_status_id": "app-ux-status-pill",
                "screen_reader_status_id": "app-ux-live-status",
                "aria_live": "polite",
                "states": ["ready", "loading", "fallback", "offline"],
            },
            "resilience": {
                "feed_api_hydration": "loadFeedSurface",
                "web_session_feed_path": "/app/web/feed",
                "offline_fallback": "embedded feed snapshot",
                "online_refresh": true,
            },
            "readiness_checks": [
                "four_tab_mobile_shell_visible",
                "mobile_tablist_a11y_visible",
                "keyboard_tab_navigation_visible",
                "global_search_filters_active_tab",
                "search_empty_state_visible",
                "search_clear_and_escape_visible",
                "aria_live_ux_status_visible",
                "offline_feed_fallback_status_visible",
                "web_session_feed_hydration_visible",
                "feed_api_hydration_visible",
                "next_action_rail_visible",
                "playability_coach_visible",
                "p0_next_best_action_visible",
                "p1_strategy_choices_visible",
                "p2_retention_telemetry_visible",
                "failure_recovery_lane_visible",
                "economy_tradeoff_cards_visible",
                "retention_calendar_visible",
                "playability_funnel_visible",
                "anti_cheese_policy_visible",
                "ops_refresh_hooks_visible"
            ]
        })
    }

    fn economy_retention_ops_json(
        &self,
        feed: &Value,
        progression: &Value,
        map_metrics: &ClientAppMapHubMetrics,
    ) -> Value {
        let matrix_user_id = self.matrix_user_id;
        let now = Utc::now().timestamp();
        let feed_item_count = feed.get("item_count").and_then(Value::as_u64).unwrap_or(0) as i64;
        let progression_level = progression
            .get("level")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        let successful_task_count = progression
            .get("successful_task_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let world_action_count = self
            .world
            .world_events
            .iter()
            .filter(|event| event.actor_matrix_user_id == matrix_user_id)
            .count() as i64;
        let purchase_count = self
            .world
            .world_purchases
            .iter()
            .filter(|purchase| {
                purchase.buyer_matrix_user_id == matrix_user_id
                    || purchase.seller_matrix_user_id == matrix_user_id
            })
            .count() as i64;
        let work_order_count = self
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| {
                work_order.buyer_matrix_user_id == matrix_user_id
                    || work_order.seller_matrix_user_id == matrix_user_id
            })
            .count() as i64;
        let delivery_count = self
            .world
            .world_work_deliveries
            .iter()
            .filter(|delivery| delivery.matrix_user_id == matrix_user_id)
            .count() as i64;
        let completion_count = self
            .world
            .world_contract_completions
            .iter()
            .filter(|completion| completion.matrix_user_id == matrix_user_id)
            .count() as i64;
        let submission_count = self
            .league
            .submissions
            .values()
            .filter(|submission| submission.matrix_user_id == matrix_user_id)
            .count() as i64;
        let acceptance_count = self
            .world
            .world_work_acceptances
            .iter()
            .filter(|acceptance| acceptance.matrix_user_id == matrix_user_id)
            .count() as i64;
        let recovery_count = self
            .world
            .world_work_rejections
            .iter()
            .filter(|rejection| rejection.matrix_user_id == matrix_user_id)
            .count()
            + self
                .world
                .world_work_reopens
                .iter()
                .filter(|reopen| reopen.matrix_user_id == matrix_user_id)
                .count()
            + self
                .world
                .world_work_cancellations
                .iter()
                .filter(|cancellation| cancellation.matrix_user_id == matrix_user_id)
                .count();
        let rating_or_recovery_count = acceptance_count + recovery_count as i64;
        let reward_count = self
            .league
            .rewards
            .iter()
            .filter(|reward| reward.matrix_user_id == matrix_user_id)
            .count() as i64
            + completion_count
            + acceptance_count;
        let listed_count = self
            .world
            .world_listings
            .iter()
            .filter(|listing| listing.status == "listed")
            .count() as i64;
        let review_hold_count = self
            .league
            .submissions
            .values()
            .filter(|submission| {
                submission.matrix_user_id == matrix_user_id
                    && submission.payout_status.as_deref() == Some("review_hold")
            })
            .count() as i64
            + self
                .world
                .world_work_orders
                .iter()
                .filter(|work_order| {
                    (work_order.buyer_matrix_user_id == matrix_user_id
                        || work_order.seller_matrix_user_id == matrix_user_id)
                        && matches!(
                            work_order.status.as_str(),
                            "delivery_review_hold" | "reopen_reserve_hold" | "payment_hold"
                        )
                })
                .count() as i64;
        let anti_cheat_flag_count = self
            .league
            .submissions
            .values()
            .filter(|submission| submission.matrix_user_id == matrix_user_id)
            .flat_map(|submission| submission.anti_cheat_flags.iter())
            .count() as i64;
        let playability_telemetry_event_count = self
            .world
            .world_economy_events
            .iter()
            .filter(|event| event.event_kind == "playability_telemetry")
            .count() as i64;
        let market_tax_sink_count = self
            .world
            .world_economy_events
            .iter()
            .filter(|event| event.event_kind == "market_tax_sink")
            .count() as i64;
        let encounter_state_event_count = self
            .league
            .submissions
            .values()
            .flat_map(|submission| submission.score_events.iter())
            .filter(|event| event.dimension == "encounter_state")
            .count() as i64;
        let route_backlog_count = self.route_artifacts.task_views.len() as i64;
        let mut active_days = HashSet::new();
        for epoch in self
            .world
            .world_events
            .iter()
            .filter(|event| event.actor_matrix_user_id == matrix_user_id)
            .map(|event| event.created_at_epoch)
            .chain(
                self.world
                    .world_purchases
                    .iter()
                    .filter(|purchase| {
                        purchase.buyer_matrix_user_id == matrix_user_id
                            || purchase.seller_matrix_user_id == matrix_user_id
                    })
                    .map(|purchase| purchase.created_at_epoch),
            )
            .chain(
                self.world
                    .world_work_deliveries
                    .iter()
                    .filter(|delivery| delivery.matrix_user_id == matrix_user_id)
                    .map(|delivery| delivery.created_at_epoch),
            )
            .chain(
                self.league
                    .submissions
                    .values()
                    .filter(|submission| submission.matrix_user_id == matrix_user_id)
                    .map(|submission| submission.created_at_epoch),
            )
        {
            active_days.insert(epoch / 86_400);
        }
        let funnel_steps = vec![
            json!({"step_id": "first_focus_selected", "label": "Map focus selected / 选择地图焦点", "count": if self.world.world_player_positions.contains_key(matrix_user_id) { 1 } else { 0 }, "completed": self.world.world_player_positions.contains_key(matrix_user_id)}),
            json!({"step_id": "world_action_started", "label": "World action started / 开始世界行动", "count": world_action_count, "completed": world_action_count > 0}),
            json!({"step_id": "commission_accepted", "label": "Commission accepted / 接取委托", "count": purchase_count.max(work_order_count), "completed": purchase_count > 0 || work_order_count > 0}),
            json!({"step_id": "result_submitted", "label": "Result submitted / 提交成果", "count": delivery_count + completion_count + submission_count, "completed": delivery_count + completion_count + submission_count > 0}),
            json!({"step_id": "rating_or_recovery_chosen", "label": "Rating or recovery chosen / 评级或恢复路径", "count": rating_or_recovery_count, "completed": rating_or_recovery_count > 0}),
            json!({"step_id": "reward_read", "label": "Reward read / 奖励可读", "count": reward_count, "completed": reward_count > 0}),
            json!({"step_id": "next_route_queued", "label": "Next route queued / 下一条路线已排队", "count": route_backlog_count, "completed": route_backlog_count > 0}),
        ];
        let completed_funnel_steps = funnel_steps
            .iter()
            .filter(|step| {
                step.get("completed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .count();
        let funnel_completion_percent =
            ((completed_funnel_steps as f64 / funnel_steps.len() as f64) * 100.0).round() as i64;
        json!({
            "contract_version": TRILLIONNIUM_ECONOMY_RETENTION_OPS_CONTRACT_VERSION,
            "status": "instrumented",
            "matrix_user_id": matrix_user_id,
            "live_counts": {
                "feed_item_count": feed_item_count,
                "route_backlog_count": route_backlog_count,
                "world_action_count": world_action_count,
                "purchase_count": purchase_count,
                "work_order_count": work_order_count,
                "delivery_count": delivery_count,
                "rating_or_recovery_count": rating_or_recovery_count,
                "reward_count": reward_count,
                "listed_count": listed_count,
                "review_hold_count": review_hold_count,
                "anti_cheat_flag_count": anti_cheat_flag_count,
                "playability_telemetry_event_count": playability_telemetry_event_count,
                "market_tax_sink_count": market_tax_sink_count,
                "encounter_state_event_count": encounter_state_event_count,
                "active_day_count": active_days.len(),
                "progression_level": progression_level,
                "successful_task_count": successful_task_count,
                "live_event_count": map_metrics.live_event_count,
            },
            "economy_tradeoff_cards": [
                {"card_id": "high_reward_delivery", "label": "High reward delivery / 高收益交付", "upside": "credits + reputation", "risk": "review_hold if evidence is weak", "source_sink": "buyer escrow → seller settlement", "live_count": work_order_count, "command": "/work deliver latest High-reward delivery: submit customer deliverable, evidence package, acceptance checklist, risk recap, next action, and self-review."},
                {"card_id": "safe_refund_reopen", "label": "Safe refund / reopen / 安全退款重开", "upside": "protect trust and retry", "risk": "slower payout, but no dead end", "source_sink": "refund reserve → reopen reserve", "live_count": recovery_count, "command": "/work reject latest Safe refund: record customer delivery gap, evidence package issue, refund confirmation, seller chargeback risk, reopen condition, next action, and self-review.", "alternative_commands": ["/work reopen latest Revision route: restate customer deliverable, evidence package, rating standard, risk controls, next action, and self-review."]},
                {"card_id": "faction_reputation", "label": "Faction reputation / 阵营声望", "upside": "rank unlock and better routes", "risk": "profit is slower than direct sales", "source_sink": "standing delta", "live_count": self.world.world_faction_standings.len(), "command": "/world action Build faction reputation: submit customer deliverable, evidence package, risk controls, next ally action, and self-review."},
                {"card_id": "company_supply", "label": "Company supply / 公司供给", "upside": "repeatable listings and market depth", "risk": "requires quality and refresh cadence", "source_sink": "asset → company → listing", "live_count": listed_count, "command": "/sell latest Company supply listing: describe customer deliverable, price, evidence package, risk controls, acceptance standard, next action, and self-review."}
            ],
            "retention_calendar": {
                "season_id": "preseason-zero",
                "daily_loop": "pick one route backlog item, finish one delivery/rating, queue tomorrow's route",
                "weekly_loop": "guild raid window + market refresh + faction standing push",
                "season_loop": "unlock target + public leaderboard + economy refresh",
                "next_reset_epoch": now + 86_400,
                "return_reason": "A player should come back for queued route payoff, market movement, raid window, and next unlock."
            },
            "playability_funnel": {
                "funnel_id": "first_session_focus_to_reward_then_next_route",
                "completed_steps": completed_funnel_steps,
                "total_steps": funnel_steps.len(),
                "completion_percent": funnel_completion_percent,
                "steps": funnel_steps,
            },
            "anti_cheese_policy": {
                "policy_id": "trillionnium_playability_anti_cheese_v1",
                "cooldown_seconds": 300,
                "duplicate_gate": "review_hold_zero_reward",
                "backend_gate_enforced": true,
                "review_hold_count": review_hold_count,
                "anti_cheat_flag_count": anti_cheat_flag_count,
                "signals": ["too_short", "repetition_suspected", "hidden_tests_failed", "hidden_missing_evidence", "judge_disagreement", "duplicate_action_signature", "repeat_kind_cooldown_pressure"],
                "player_copy": "Fast play is welcome; duplicate or evidence-free farming goes to review hold instead of silent payout."
            },
            "engine_contracts": {
                "world_action_engine": "trillionnium_world_action_engine_v1",
                "market_simulator": "trillionnium_market_simulator_v1",
                "league_encounter_state": "trillionnium_league_encounter_state_v1",
                "telemetry_stream": "world_economy_events:playability_telemetry",
                "balance_config": "trillionnium_playability_balance_config_v1"
            },
            "playability_balance_config": {
                "contract_version": "trillionnium_playability_balance_config_v1",
                "world_action_cooldown_seconds": 300,
                "market_tax_rate_percent": 5,
                "demand_window_seconds": 86400,
                "league_mode_multipliers": {"daily_dungeon": 1.0, "guild_raid": 1.25, "bounty_arena": 1.5},
                "review_hold_signals": ["duplicate_action_signature", "too_short", "hidden_tests_failed", "judge_disagreement"],
                "tunable_without_ui_rewrite": true
            },
            "ops_refresh_hooks": [
                {"hook_id": "daily_route_refresh", "cadence": "daily", "owner_surface": "/app", "status": "declared"},
                {"hook_id": "weekly_guild_raid_window", "cadence": "weekly", "owner_surface": "/league", "status": "declared"},
                {"hook_id": "market_supply_refresh", "cadence": "daily", "owner_surface": "/world", "status": "declared"},
                {"hook_id": "season_scoreboard_reset", "cadence": "seasonal", "owner_surface": "/league/season", "status": "declared"}
            ],
            "readiness_checks": [
                "economy_tradeoff_cards_visible",
                "retention_calendar_visible",
                "playability_funnel_visible",
                "anti_cheese_policy_visible",
                "ops_refresh_hooks_visible",
                "live_counts_connected",
                "funnel_steps_cover_first_reward",
                "risk_reward_language_visible",
                "season_loop_visible",
                "cooldown_policy_visible",
                "anti_cheese_gate_enforced_visible",
                "backend_outcome_engine_visible",
                "market_simulator_visible",
                "league_encounter_state_visible",
                "persistent_telemetry_stream_visible",
                "balance_config_visible"
            ]
        })
    }

    fn playability_coach_json(
        &self,
        active_region_id: &str,
        current_node_id: &str,
        current_node_name: &str,
        onboarding: &Value,
        feed: &Value,
        progression: &Value,
        map_metrics: &ClientAppMapHubMetrics,
        nearby_agents: &[WorldEntity],
    ) -> Value {
        let feed_item_count = feed.get("item_count").and_then(Value::as_u64).unwrap_or(0);
        let progression_level = progression
            .get("level")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        let successful_task_count = progression
            .get("successful_task_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let first_action_command = onboarding
            .get("steps")
            .and_then(Value::as_array)
            .and_then(|steps| {
                steps.iter().find_map(|step| {
                    (step.get("step_id").and_then(Value::as_str) == Some("start_world_action"))
                        .then(|| step.get("command").and_then(Value::as_str))
                        .flatten()
                })
            })
            .unwrap_or("/world action Draft the first global bounty with goal, evidence, risk, and next step.");
        let reviewable_work_count = self
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| {
                matches!(
                    work_order.status.as_str(),
                    "delivered" | "delivery_review_hold"
                )
            })
            .count();
        let reopenable_work_count = self
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| matches!(work_order.status.as_str(), "rejected_refunded"))
            .count();
        let settlement_recovery_work_count = self
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| {
                matches!(
                    work_order.status.as_str(),
                    "rejected_refund_hold"
                        | "rejected_refund_failed"
                        | "rejected_pending_refund"
                        | "rejected_chargeback_failed"
                        | "rejected_pending_chargeback"
                        | "cancelled_refund_hold"
                        | "cancelled_refund_failed"
                        | "cancel_pending_refund"
                        | "cancelled_chargeback_failed"
                        | "cancel_pending_chargeback"
                )
            })
            .count();
        let open_work_count = self
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| matches!(work_order.status.as_str(), "open" | "payment_hold"))
            .count();
        let economy_retention_ops = self.economy_retention_ops_json(feed, progression, map_metrics);
        json!({
            "contract_version": TRILLIONNIUM_PLAYABILITY_COACH_CONTRACT_VERSION,
            "optimization_scope": "p0_p1_p2_full_playability",
            "status": "optimized",
            "player_promise": "one glance should tell the player what to do now, why it matters, what can go wrong, and why they should return tomorrow",
            "context": {
                "matrix_user_id": self.matrix_user_id,
                "active_region_id": active_region_id,
                "current_node_id": current_node_id,
                "current_node_name": current_node_name,
                "feed_item_count": feed_item_count,
                "live_event_count": map_metrics.live_event_count,
                "nearby_poi_count": map_metrics.nearby_poi_count,
                "nearby_agent_count": nearby_agents.len(),
                "progression_level": progression_level,
                "successful_task_count": successful_task_count,
            },
            "lanes": [
                {
                    "lane_id": "p0_first_session",
                    "priority": "P0",
                    "label": "P0 · First 3-minute quest / 首局三分钟主线",
                    "player_goal": "Choose focus → accept bounty → submit evidence → read rating/reward",
                    "visible_surface_id": "app-first-playable-onboarding",
                    "cta_label": "Start World Quest / 开始世界任务",
                    "command": first_action_command,
                    "success_signal": "first_playable_loop_100"
                },
                {
                    "lane_id": "p1_strategy_depth",
                    "priority": "P1",
                    "label": "P1 · Strategy choices / 策略选择",
                    "player_goal": "Pick between market profit, faction reputation, team raid, or safe refund/reopen",
                    "visible_surface_id": "app-playability-coach",
                    "cta_label": "Compare Routes / 比较路线",
                    "command": "/app feed strategy",
                    "success_signal": "economy_social_strategy_depth_visible"
                },
                {
                    "lane_id": "p2_retention_ops",
                    "priority": "P2",
                    "label": "P2 · Return reasons / 回访理由",
                    "player_goal": "Daily route backlog, weekly guild raid, unlock plan, and telemetry-backed polish",
                    "visible_surface_id": "app-tab-me",
                    "cta_label": "Check Progression / 查看成长",
                    "command": "/progression",
                    "success_signal": "retention_telemetry_contract_visible"
                }
            ],
            "next_best_actions": [
                {
                    "action_id": "p0_start_focus",
                    "priority": 1,
                    "metric": "first_playable_completeness",
                    "label": "Start from current map focus / 从当前地图焦点开始",
                    "panel_id": WORLD_ROUTE_ACTION_PANEL_ID,
                    "command": first_action_command,
                    "success_signal": "world_event_created"
                },
                {
                    "action_id": "p0_rate_or_recover",
                    "priority": 2,
                    "metric": "real_player_comprehension_cost",
                    "label": "Rate, reopen, or refund current commission / 评级、重开或退款当前委托",
                    "panel_id": WORLD_ROUTE_COMMERCE_PANEL_ID,
                    "command": "/work accept latest Acceptance confirmation: verify customer deliverable, evidence package, risk controls, next collaboration action, and self-review.",
                    "alternative_commands": [
                        "/work reject latest Refund rejection: record customer delivery gap, evidence package issue, risk controls, refund state, next recovery action, and self-review.",
                        "/work reopen latest Revision route: restate customer deliverable, evidence package, rating standard, risk controls, next action, and self-review."
                    ],
                    "success_signal": "quest_rating_or_feedback_loop_visible"
                },
                {
                    "action_id": "p1_choose_strategy",
                    "priority": 3,
                    "metric": "economy_social_strategy_depth",
                    "label": "Choose profit, reputation, or co-op route / 选择收益、声望或协作路线",
                    "panel_id": WORLD_ROUTE_COMMERCE_PANEL_ID,
                    "command": "/world action Compare routes: evaluate market profit, faction standing, guild cooperation, recovery path, customer deliverable, evidence package, risk controls, next action, and self-review.",
                    "success_signal": "strategy_tradeoff_visible"
                },
                {
                    "action_id": "p2_return_hook",
                    "priority": 4,
                    "metric": "long_term_replayability",
                    "label": "Queue tomorrow's route and weekly raid / 排明日路线与每周团本",
                    "panel_id": "app-tab-me",
                    "command": "/progression plan next unlock and weekly guild raid",
                    "success_signal": "daily_return_hook_visible"
                }
            ],
            "failure_recovery": {
                "visible_surface_id": "app-playability-coach",
                "states": [
                    "delivery_review_hold",
                    "rejected_pending_refund",
                    "rejected_refund_hold",
                    "rejected_refund_failed",
                    "rejected_chargeback_failed",
                    "rejected_pending_chargeback",
                    "rejected_refunded",
                    "reopen_reserve_hold",
                    "cancel_pending_refund",
                    "cancel_pending_chargeback",
                    "cancelled_refund_hold",
                    "cancelled_refund_failed",
                    "cancelled_chargeback_failed",
                    "cancelled_refunded"
                ],
                "reviewable_work_count": reviewable_work_count,
                "settlement_recovery_work_count": settlement_recovery_work_count,
                "reopenable_work_count": reopenable_work_count,
                "open_work_count": open_work_count,
                "settlement_recovery_command": "/work reject latest Settlement recovery: confirm customer delivery gap, evidence package, refund or chargeback state, ledger blocker, risk controls, next action, and self-review.",
                "settlement_recovery_alternative_commands": [
                    "/work cancel latest Cancellation settlement: confirm customer delivery cancellation reason, evidence package, refund state, seller chargeback, risk controls, next calibration, and self-review."
                ],
                "player_copy": "If a result fails, the player sees why, which funds moved, whether settlement retry must happen before reopen, and the exact reopen/refund route instead of a dead end."
            },
            "strategy_depth": {
                "economy_choices": ["high_reward_delivery", "safe_refund_reopen", "faction_reputation", "company_listing_supply"],
                "social_choices": ["nearby_agent_help", "guild_raid", "face_duel", "relationship_route"],
                "risk_tradeoffs": ["speed_vs_evidence", "profit_vs_reputation", "solo_vs_coop", "accept_vs_revise"]
            },
            "retention_ops": {
                "season_loop": "daily route backlog + weekly guild raid + market refresh + unlock target",
                "daily_return_hooks": ["next_route_backlog", "unlock_progress", "market_result", "guild_raid_window"],
                "telemetry_events": [
                    "first_focus_selected",
                    "world_action_started",
                    "commission_accepted",
                    "result_submitted",
                    "rating_or_recovery_chosen",
                    "reward_read",
                    "next_route_queued"
                ],
                "funnel_target": "first_session_focus_to_reward_then_next_route"
            },
            "economy_retention_ops": economy_retention_ops,
            "readiness_checks": [
                "p0_next_best_action_visible",
                "p0_failure_recovery_copy_visible",
                "p1_economy_tradeoffs_visible",
                "p1_social_coop_choices_visible",
                "p2_daily_return_hook_visible",
                "p2_telemetry_contract_visible",
                "economy_tradeoff_cards_visible",
                "retention_calendar_visible",
                "playability_funnel_visible",
                "anti_cheese_policy_visible",
                "ops_refresh_hooks_visible",
                "coach_lanes_cover_p0_p1_p2",
                "coach_actions_link_world_panels",
                "coach_uses_live_runtime_counts",
                "coach_visible_in_mobile_app"
            ]
        })
    }

    fn json(&self) -> Value {
        let map = self.map.clone();
        let real_world_map_engine = self.real_world_map_engine.clone();
        let app_context = self.app_route_context();
        let active_region_id = app_context.active_region_id.clone();
        let feed = self.feed_json(active_region_id.as_str());
        let current_node_id = self.current_node_id();
        let current_node_name = self.current_node_name();
        let nearby_agents = self.nearby_agents();
        let progression = self.progression_json();
        let progression_summary = self.progression_summary(&progression);
        let map_metrics = self.map_hub_metrics(&app_context.viewport);
        let modules = ClientAppModulesProjection {
            active_region_id: active_region_id.as_str(),
            current_node_id: current_node_id.as_str(),
            current_node_name: current_node_name.as_str(),
            nearby_agent_count: nearby_agents.len(),
            map_metrics: &map_metrics,
            progression_summary: &progression_summary,
        }
        .json();
        let module_count = modules.len();
        let onboarding = self.first_playable_onboarding_json(
            active_region_id.as_str(),
            current_node_id.as_str(),
            current_node_name.as_str(),
            &progression_summary,
            &map_metrics,
        );
        let mobile_shell_contract = self.mobile_shell_contract_json();
        let playability_coach = self.playability_coach_json(
            active_region_id.as_str(),
            current_node_id.as_str(),
            current_node_name.as_str(),
            &onboarding,
            &feed,
            &progression,
            &map_metrics,
            &nearby_agents,
        );
        let next_best_actions = playability_coach
            .get("next_best_actions")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let economy_retention_ops = playability_coach
            .get("economy_retention_ops")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let map_hub = app_context.into_client_app_map_hub_json(&map_metrics);
        json!({
            "kind": "trillionnium_client_app",
            "projection_layer": "client_app_projection_v1",
            "projection_context": "ClientAppProjectionContext",
            "index_layer": "WorldIndexes::client_app_sorted_entities_v1",
            "client": "trillionnium_mobile_shell",
            "matrix_user_id": self.matrix_user_id,
            "primary_entry_module_id": "world_map",
            "modules": modules,
            "module_count": module_count,
            "route_contract": world_route_ui_contract_json(),
            "mobile_shell_contract": mobile_shell_contract,
            "onboarding": onboarding,
            "playability_coach": playability_coach,
            "next_best_actions": next_best_actions,
            "economy_retention_ops": economy_retention_ops,
            "map": map,
            "feed": feed,
            "map_hub": map_hub,
            "real_world_map_engine": real_world_map_engine,
            "progression": progression,
            "nearby_agents": nearby_agents,
            "social": {
                "provider_style": "wechat_telegram",
                "room_command": "/social",
                "contact_count": self.world.world_entities.len(),
                "guild_count": self.guild_count,
            },
            "wallet": {
                "provider_style": "alipay",
                "commands": ["/wallet", "/balance", "/pay"],
                "ledger_actions": ["reserve", "consume", "refund", "grant"],
            },
            "duel": {
                "provider_style": "pokemon_face_to_face",
                "match_id": "face-duel-001",
                "command": "/duel nearby Scout opponent intent with Oracle Scout; record fairness evidence, risk controls, and next action.",
            }
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct ClientAppMapHubMetrics {
    region_shard_count: usize,
    lod_layer_count: usize,
    tile_shard_count: usize,
    nearby_poi_count: usize,
    prefetch_count: usize,
    live_event_count: usize,
    player_avatar_count: usize,
    avatar_task_route_count: usize,
    avatar_route_runner_count: usize,
    player_density_mode: String,
    estimated_concurrent_players: i64,
}

impl ClientAppMapHubMetrics {
    fn from_engine_and_viewport(real_world_map_engine: &Value, map_viewport: &Value) -> Self {
        Self {
            region_shard_count: real_world_map_engine
                .get("region_shards")
                .and_then(Value::as_array)
                .map(|regions| regions.len())
                .unwrap_or(0),
            lod_layer_count: real_world_map_engine
                .get("lod_layers")
                .and_then(Value::as_array)
                .map(|layers| layers.len())
                .unwrap_or(0),
            nearby_poi_count: map_viewport
                .get("poi_hotspots")
                .and_then(Value::as_array)
                .map(|pois| pois.len())
                .unwrap_or(0),
            tile_shard_count: map_viewport
                .get("visible_tile_shards")
                .and_then(Value::as_array)
                .map(|tiles| tiles.len())
                .unwrap_or(0),
            prefetch_count: map_viewport
                .get("prefetch_queue")
                .and_then(Value::as_array)
                .map(|tiles| tiles.len())
                .unwrap_or(0),
            live_event_count: map_viewport
                .get("live_event_stream")
                .and_then(Value::as_array)
                .map(|events| events.len())
                .unwrap_or(0),
            player_avatar_count: map_viewport
                .get("player_avatars")
                .and_then(Value::as_array)
                .map(|avatars| avatars.len())
                .unwrap_or(0),
            avatar_task_route_count: map_viewport
                .get("avatar_task_routes")
                .and_then(Value::as_array)
                .map(|routes| routes.len())
                .unwrap_or(0),
            avatar_route_runner_count: map_viewport
                .get("avatar_route_runners")
                .and_then(Value::as_array)
                .map(|runners| runners.len())
                .unwrap_or(0),
            player_density_mode: map_viewport
                .get("player_density")
                .and_then(|density| density.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("dense")
                .to_string(),
            estimated_concurrent_players: map_viewport
                .get("player_density")
                .and_then(|density| density.get("estimated_concurrent_players"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ClientAppProgressionSummary {
    level: i64,
    rank_title: String,
    successful_task_count: i64,
    unlocked_skill_count: u64,
    unlocked_tool_count: u64,
    unlocked_skin_count: u64,
    current_school_name: String,
}

impl ClientAppProgressionSummary {
    fn from_progression_json(progression: &Value) -> Self {
        Self {
            level: progression
                .get("level")
                .and_then(Value::as_i64)
                .unwrap_or(1),
            rank_title: progression
                .get("rank_title")
                .and_then(Value::as_str)
                .unwrap_or("Apprentice")
                .to_string(),
            successful_task_count: progression
                .get("successful_task_count")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            unlocked_skill_count: progression
                .get("unlocked_skill_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            unlocked_tool_count: progression
                .get("unlocked_tool_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            unlocked_skin_count: progression
                .get("unlocked_skin_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            current_school_name: progression
                .get("current_school")
                .and_then(|school| school.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("City Clerks")
                .to_string(),
        }
    }

    fn summary_text(&self) -> String {
        format!(
            "Level {} {} · {} · {} successful tasks · unlocks {}/{}/{}",
            self.level,
            self.rank_title,
            self.current_school_name,
            self.successful_task_count,
            self.unlocked_skill_count,
            self.unlocked_tool_count,
            self.unlocked_skin_count
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ClientAppModulesProjection<'a> {
    active_region_id: &'a str,
    current_node_id: &'a str,
    current_node_name: &'a str,
    nearby_agent_count: usize,
    map_metrics: &'a ClientAppMapHubMetrics,
    progression_summary: &'a ClientAppProgressionSummary,
}

impl<'a> ClientAppModulesProjection<'a> {
    fn json(&self) -> Vec<Value> {
        vec![
            json!({
                "module_id": "world_map",
                "name": "Trillionnium World Map",
                "style": "OpenStreetMap upgraded with game avatars, quest routes, and Hero Tale / Gather exploration layers",
                "status": "可玩",
                "entry_priority": 1,
                "ui_role": "primary_super_entry",
                "engine_id": "leaflet_openstreetmap_v1",
                "tile_provider": "OpenStreetMap",
                "active_region_id": self.active_region_id,
                "tile_shard_count": self.map_metrics.tile_shard_count,
                "nearby_poi_count": self.map_metrics.nearby_poi_count,
                "prefetch_count": self.map_metrics.prefetch_count,
                "live_event_count": self.map_metrics.live_event_count,
                "player_avatar_count": self.map_metrics.player_avatar_count,
                "avatar_task_route_count": self.map_metrics.avatar_task_route_count,
                "avatar_route_runner_count": self.map_metrics.avatar_route_runner_count,
                "player_density_mode": self.map_metrics.player_density_mode.clone(),
                "primary_command": "/map",
                "secondary_command": "/go west",
                "summary": format!(
                    "{} / {} · 区域 {} · {} 个附近热点 · {} 个实时事件 · {} 条任务路线 · {} 个动态角色 · {} 个角色跑图 · {} 密度",
                    self.current_node_name,
                    self.current_node_id,
                    self.active_region_id,
                    self.map_metrics.nearby_poi_count,
                    self.map_metrics.live_event_count,
                    self.map_metrics.avatar_task_route_count,
                    self.map_metrics.avatar_route_runner_count,
                    self.map_metrics.player_avatar_count,
                    self.map_metrics.player_density_mode,
                ),
            }),
            json!({
                "module_id": "face_duel",
                "name": "面对面切磋",
                "style": "宝可梦式附近对战",
                "status": "可玩",
                "primary_command": "/duel nearby Scout opponent intent with Oracle Scout; record fairness evidence, risk controls, and next action.",
                "match_id": "face-duel-001",
                "summary": "面对面选择 Agent 阵容、出招、评分和奖励",
            }),
            json!({
                "module_id": "social",
                "name": "队友消息",
                "style": "微信 / Telegram 房间循环",
                "status": "可玩",
                "primary_command": "/social",
                "contact_count": self.nearby_agent_count,
                "summary": "Matrix 房间 + Agent/NPC 联系人 + 公会在线状态",
            }),
            json!({
                "module_id": "wallet",
                "name": "奖励钱包",
                "style": "支付宝式积分钱包",
                "status": "可玩",
                "primary_command": "/wallet",
                "secondary_command": "/pay",
                "summary": "余额 / 托管 / 领取 / 退回",
            }),
            json!({
                "module_id": "progression",
                "name": "角色成长",
                "style": "门派 / 技能 / 道具 / 皮肤 / 经验 / 等级",
                "status": "可玩",
                "primary_command": "/progression",
                "secondary_command": "/skills /tools /skins",
                "level": self.progression_summary.level,
                "rank_title": self.progression_summary.rank_title.clone(),
                "current_school": self.progression_summary.current_school_name.clone(),
                "successful_task_count": self.progression_summary.successful_task_count,
                "unlocked_skill_count": self.progression_summary.unlocked_skill_count,
                "unlocked_tool_count": self.progression_summary.unlocked_tool_count,
                "unlocked_skin_count": self.progression_summary.unlocked_skin_count,
                "summary": self.progression_summary.summary_text(),
            }),
        ]
    }
}

pub(super) fn client_app_json(league: &LeagueState, matrix_user_id: &str) -> Value {
    ClientAppProjectionContext::new(league, matrix_user_id).json()
}
