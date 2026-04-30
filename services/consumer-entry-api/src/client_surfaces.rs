use super::*;

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
    ("completion", "完成"),
    ("commerce", "成交"),
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
            .unwrap_or("/v1/client/feed/@alice:local.dev")
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
                "<span class=\"hud-chip\"><strong>{}</strong> feed items</span>",
                self.item_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> contracts</span>",
                self.contract_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> completions</span>",
                self.completion_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> purchases · <strong>{}</strong> work orders</span>",
                self.purchase_count, self.work_order_count
            ),
            format!(
                "<span class=\"hud-chip\"><strong>{}</strong> nearby agents · {}</span>",
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
        escape_html_text(label),
    )
}

pub(super) fn map_node_focus_button_html(node_id: &str, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"node\" data-node-id=\"{}\">{}</button>",
        escape_html_text(node_id),
        escape_html_text(label),
    )
}

pub(super) fn map_tile_focus_button_html(z: i64, x: i64, y: i64, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip trillionnium-map-focus\" data-focus-kind=\"tile\" data-tile-z=\"{}\" data-tile-x=\"{}\" data-tile-y=\"{}\">{}</button>",
        z,
        x,
        y,
        escape_html_text(label),
    )
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
        escape_html_text(label),
    )
}

pub(super) fn map_overlay_control_buttons_html() -> &'static str {
    r#"          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="density" aria-pressed="true">Density</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="regions" aria-pressed="true">Regions</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="tiles" aria-pressed="true">Tiles</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="prefetch" aria-pressed="true">Prefetch</button>
          <button type="button" class="overlay-toggle trillionnium-overlay-toggle" data-overlay-target="events" aria-pressed="true">Live events</button>"#
}

pub(super) fn map_camera_action_buttons_html() -> &'static str {
    r#"          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="active_region">Center active region</button>
          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="nearest_poi">Nearest hotspot</button>
          <button type="button" class="focus-chip trillionnium-map-camera-action" data-camera-action="hottest_event">Hottest event</button>"#
}

pub(super) fn route_filter_buttons_html(
    button_class: &str,
    selection_label: &str,
    all_label: &str,
) -> String {
    format!(
        "<button type=\"button\" class=\"focus-chip {}\" data-route-filter=\"selection\">{}</button>\n            <button type=\"button\" class=\"focus-chip {}\" data-route-filter=\"all\">{}</button>",
        escape_html_text(button_class),
        escape_html_text(selection_label),
        escape_html_text(button_class),
        escape_html_text(all_label),
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
            "source": "completion_snapshot",
            "completion_id": completion_id,
            "contract_id": contract_id,
            "task_id": task_id,
            "location_id": location_id,
            "title": format!("Completion {}", completion_id),
            "summary": completion.get("body").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("task {} · score {:.1} · reward {:.2} · {}", if task_id.is_empty() { "unlinked" } else { task_id }, score, reward_amount, payout_status),
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
            "source": "commerce_snapshot",
            "purchase_id": purchase_id,
            "listing_id": purchase.get("listing_id").cloned().unwrap_or(Value::Null),
            "company_id": purchase.get("company_id").cloned().unwrap_or(Value::Null),
            "title": format!("Purchase {}", purchase_id),
            "summary": purchase.get("listing_id").cloned().unwrap_or_else(|| json!("listing")),
            "detail": format!("{} credits · {}", price_credits, status),
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
            "source": "commerce_snapshot",
            "work_order_id": work_order_id,
            "purchase_id": work_order.get("purchase_id").cloned().unwrap_or(Value::Null),
            "listing_id": work_order.get("listing_id").cloned().unwrap_or(Value::Null),
            "company_id": work_order.get("company_id").cloned().unwrap_or(Value::Null),
            "title": format!("Work order {}", work_order_id),
            "summary": work_order.get("brief").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("{} · value {}", status, value_score),
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
            "source": "commerce_snapshot",
            "delivery_id": delivery_id,
            "work_order_id": work_order_id,
            "listing_id": linked_work_order.map(|work_order| work_order.listing_id.clone()).unwrap_or_default(),
            "company_id": linked_work_order.map(|work_order| work_order.company_id.clone()).unwrap_or_default(),
            "title": format!("Delivery {}", delivery_id),
            "summary": delivery.get("body").cloned().unwrap_or_else(|| json!("")),
            "detail": format!("work order {} · {} · score {:.1}", if work_order_id.is_empty() { "pending" } else { work_order_id }, status, score),
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
            .unwrap_or_else(|| json!("Route next opportunity")),
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
        format!(
            "{}: update contract evidence, acceptance standard, and next route step.",
            title
        ),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_task_id(client_feed_object_value(object, "task_id"))
    .with_contract_id(client_feed_object_value(object, "contract_id"))
    .apply(object);
}

pub(super) fn decorate_completion_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Completion");
    ClientFeedActionTarget::from_route_target(world_route_action_console_target(
        "复盘收益",
        format!(
            "{}: review reward, proof quality, and the next repeat-order or upsell move.",
            title
        ),
    ))
    .with_location_id(client_feed_object_value(object, "location_id"))
    .with_task_id(client_feed_object_value(object, "task_id"))
    .with_contract_id(client_feed_object_value(object, "contract_id"))
    .apply(object);
}

pub(super) fn decorate_purchase_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Purchase");
    let listing_id = client_feed_object_string(object, "listing_id", "");
    ClientFeedActionTarget::from_route_target(world_route_purchase_lane_target(
        "打开成交",
        listing_id,
        format!(
            "{}: review buyer intent, acceptance scope, and convert this purchase into a solid delivery path.",
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
    let title = client_feed_object_string(object, "title", "Work order");
    decorate_work_lane_feed_item(
        object,
        "打开工单",
        "delivery",
        format!(
            "{}: inspect this work order, tighten deliverable scope, and clear the next blocker.",
            title
        ),
    );
}

pub(super) fn decorate_delivery_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Delivery");
    decorate_work_lane_feed_item(
        object,
        "去验收",
        "acceptance",
        format!(
            "{}: review delivery quality, acceptance proof, and revision risk.",
            title
        ),
    );
}

pub(super) fn decorate_social_agent_feed_item(object: &mut serde_json::Map<String, Value>) {
    let title = client_feed_object_string(object, "title", "Agent");
    ClientFeedActionTarget::from_route_target(world_route_action_console_target(
        "去世界",
        format!(
            "{}: connect this nearby agent with the current route, contract, or live event.",
            title
        ),
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
            "active_region_id": active_region_id,
            "item_count": items.len(),
            "source_count": 6,
            "sources": [
                "live_event_stream",
                "route_preview",
                "route_task_graph",
                "contract_snapshot",
                "commerce_snapshot",
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
            "/world action 在{}发起一个真实客户服务任务：明确客户、交付物、证据、风险和下一步。",
            current_node_name
        );
        json!({
            "contract_version": "trillionnium_first_playable_onboarding_v1",
            "rail_id": "first_playable_main_quest_rail",
            "rail_label": "新手主线：从地图到成交",
            "completion_target": "first_playable_loop_100",
            "primary_goal": "把一个地图焦点推进成 world action、contract、commerce work order、delivery、acceptance 和 ledger reward。",
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
                    "surface": "世界",
                    "label": "选择地图焦点",
                    "description": "先在真实世界镜像地图里选择 region、POI、tile 或 live event，让后续 action 有明确位置。",
                    "command": "/map",
                    "web_panel_id": "app-tab-map",
                    "success_signal": "route_focus_selected",
                },
                {
                    "step_id": "start_world_action",
                    "status": "ready",
                    "surface": "世界",
                    "label": "发起 World Action",
                    "description": "把地图焦点转成一个真实客户服务任务，写清交付物、证据、风险和下一步。",
                    "command": starter_world_action,
                    "web_panel_id": WORLD_ROUTE_ACTION_PANEL_ID,
                    "textarea_id": WORLD_ROUTE_ACTION_TEXTAREA_ID,
                    "success_signal": "world_event_created",
                },
                {
                    "step_id": "capture_contract",
                    "status": "ready",
                    "surface": "世界 / 消息",
                    "label": "登记可验收委托",
                    "description": "把 action 收成 contract，确保任务可以在 Web、Matrix 和 route graph 里追踪。",
                    "command": "/contract <客户目标 + 交付物 + 验收标准>",
                    "web_panel_id": WORLD_ROUTE_CONTRACTS_PANEL_ID,
                    "success_signal": "contract_open",
                },
                {
                    "step_id": "commerce_delivery",
                    "status": "ready",
                    "surface": "动态 / 世界",
                    "label": "完成一次商业交付",
                    "description": "通过 listing、buy、work deliver、accept/reject/reopen/cancel 跑完真实 commerce lifecycle。",
                    "command": "/work deliver latest <交付内容 + 证据包 + 验收清单>",
                    "web_panel_id": WORLD_ROUTE_COMMERCE_PANEL_ID,
                    "input_id": WORLD_ROUTE_WORK_DELIVER_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID,
                    "success_signal": "work_accepted_or_feedback_loop_recorded",
                },
                {
                    "step_id": "read_reward_and_next_route",
                    "status": "ready",
                    "surface": "动态 / 我",
                    "label": "查看奖励与下一步路线",
                    "description": "验收后检查 feed、wallet、progression 和 route task graph，把下一次复购/升级机会接起来。",
                    "command": "/app",
                    "web_panel_id": "app-tab-feed",
                    "success_signal": "ledger_or_route_next_opportunity_visible",
                }
            ],
            "acceptance_checks": [
                "map_focus_visible",
                "world_event_created",
                "contract_open_or_completed",
                "commerce_work_order_created",
                "delivery_acceptance_or_feedback_loop_visible",
                "wallet_progression_feed_updated",
                "route_task_graph_next_action_visible"
            ],
            "beta_readiness_checks": [
                "four_tab_mobile_shell_visible",
                "global_search_filters_active_tab",
                "next_action_rail_visible",
                "feed_api_hydration_visible",
                "matrix_app_card_exposes_onboarding"
            ],
            "full_vision_hooks": [
                "real_world_map_engine",
                "route_task_graph",
                "commerce_lifecycle",
                "ledger_settlement",
                "matrix_social_loop",
                "normalized_repository_read_models"
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
            "onboarding": onboarding,
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
                "command": "/duel nearby <出招>",
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
                "name": "World Map",
                "style": "Leaflet + OpenStreetMap + 英雄坛说/Gather overlay",
                "status": "playable",
                "entry_priority": 1,
                "ui_role": "primary_super_entry",
                "engine_id": "leaflet_openstreetmap_v1",
                "tile_provider": "OpenStreetMap",
                "active_region_id": self.active_region_id,
                "tile_shard_count": self.map_metrics.tile_shard_count,
                "nearby_poi_count": self.map_metrics.nearby_poi_count,
                "prefetch_count": self.map_metrics.prefetch_count,
                "live_event_count": self.map_metrics.live_event_count,
                "player_density_mode": self.map_metrics.player_density_mode.clone(),
                "primary_command": "/map",
                "secondary_command": "/go <direction|node-id>",
                "summary": format!(
                    "{} / {} · region {} · {} nearby POIs · {} live events · {} density",
                    self.current_node_name,
                    self.current_node_id,
                    self.active_region_id,
                    self.map_metrics.nearby_poi_count,
                    self.map_metrics.live_event_count,
                    self.map_metrics.player_density_mode,
                ),
            }),
            json!({
                "module_id": "face_duel",
                "name": "Face Duel",
                "style": "Pokémon-like nearby battle",
                "status": "playable",
                "primary_command": "/duel nearby <出招>",
                "match_id": "face-duel-001",
                "summary": "面对面选择 Agent 阵容、出招、评分和结算",
            }),
            json!({
                "module_id": "social",
                "name": "Social",
                "style": "WeChat / Telegram room loop",
                "status": "playable",
                "primary_command": "/social",
                "contact_count": self.nearby_agent_count,
                "summary": "Matrix room + Agent/NPC contacts + guild presence",
            }),
            json!({
                "module_id": "wallet",
                "name": "Wallet",
                "style": "Alipay-like credits wallet",
                "status": "playable",
                "primary_command": "/wallet",
                "secondary_command": "/pay",
                "summary": "balance / reserved / reserve / consume / refund",
            }),
            json!({
                "module_id": "progression",
                "name": "Progression",
                "style": "门派 / skills / tools / skins / XP / level",
                "status": "playable",
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
