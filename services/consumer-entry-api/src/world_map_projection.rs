use super::*;

pub(super) struct WorldHomeProjectionContext<'a> {
    world: &'a WorldState,
    indexes: WorldIndexes,
}

impl<'a> WorldHomeProjectionContext<'a> {
    fn new(world: &'a WorldState) -> Self {
        Self {
            world,
            indexes: build_world_indexes(world),
        }
    }

    fn sorted_zones(&self) -> Vec<WorldZone> {
        self.indexes
            .sorted_zone_ids
            .iter()
            .filter_map(|zone_id| self.world.world_zones.get(zone_id).cloned())
            .collect()
    }

    fn sorted_locations(&self) -> Vec<WorldLocation> {
        self.indexes
            .sorted_location_ids
            .iter()
            .filter_map(|location_id| self.world.world_locations.get(location_id).cloned())
            .collect()
    }

    fn sorted_entities(&self) -> Vec<WorldEntity> {
        self.indexes
            .sorted_entity_ids
            .iter()
            .filter_map(|entity_id| self.world.world_entities.get(entity_id).cloned())
            .collect()
    }

    fn sorted_factions(&self) -> Vec<WorldFaction> {
        self.indexes
            .sorted_faction_ids
            .iter()
            .filter_map(|faction_id| self.world.world_factions.get(faction_id).cloned())
            .collect()
    }

    fn recent_events(&self) -> Vec<WorldEvent> {
        indexed_recent(
            &self.world.world_events,
            &self.indexes.recent_event_indices,
            8,
        )
        .cloned()
        .collect()
    }

    fn sorted_map_nodes_for_engine(&self) -> Vec<WorldMapNode> {
        self.indexes
            .sorted_map_node_ids_by_id
            .iter()
            .filter_map(|node_id| self.world.world_map_nodes.get(node_id).cloned())
            .collect()
    }

    fn current_map_node(&self, map_nodes_for_engine: &[WorldMapNode]) -> Option<&WorldMapNode> {
        self.world
            .world_map_nodes
            .get(default_world_node_id())
            .or_else(|| {
                map_nodes_for_engine
                    .first()
                    .and_then(|node| self.world.world_map_nodes.get(&node.node_id))
            })
    }

    fn real_world_map_engine(&self) -> Value {
        let map_nodes_for_engine = self.sorted_map_nodes_for_engine();
        real_world_map_engine_json(
            &map_nodes_for_engine,
            self.current_map_node(&map_nodes_for_engine),
        )
    }

    fn route_artifacts(&self) -> WorldRouteArtifacts {
        build_world_route_artifacts(self.world)
    }

    fn modules_json(&self) -> Value {
        json!({
            "league": "Trillionnium League",
            "craft": "Trillionnium Craft",
            "ledger": "Trillionnium Ledger",
            "agents": "Trillionnium Agents"
        })
    }

    fn counts_json(&self) -> Value {
        json!({
            "zones": self.world.world_zones.len(),
            "locations": self.world.world_locations.len(),
            "entities": self.world.world_entities.len(),
            "map_nodes": self.world.world_map_nodes.len(),
            "player_positions": self.world.world_player_positions.len(),
            "assets": self.world.world_assets.len(),
            "asset_upgrades": self.world.world_asset_upgrades.len(),
            "companies": self.world.world_companies.len(),
            "shops": self.world.world_shops.len(),
            "listings": self.world.world_listings.len(),
            "economy_events": self.world.world_economy_events.len(),
            "purchases": self.world.world_purchases.len(),
            "work_orders": self.world.world_work_orders.len(),
            "work_deliveries": self.world.world_work_deliveries.len(),
            "work_acceptances": self.world.world_work_acceptances.len(),
            "work_rejections": self.world.world_work_rejections.len(),
            "work_reopens": self.world.world_work_reopens.len(),
            "work_cancellations": self.world.world_work_cancellations.len(),
            "factions": self.world.world_factions.len(),
            "faction_standings": self.world.world_faction_standings.len(),
            "contracts": self.world.world_contracts.len(),
            "contract_completions": self.world.world_contract_completions.len(),
            "events": self.world.world_events.len(),
            "relationships": self.world.world_relationships.len(),
        })
    }

    fn static_fields(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert("kind".to_string(), json!("trillionnium_world"));
        fields.insert(
            "projection_layer".to_string(),
            json!("world_home_projection_v1"),
        );
        fields.insert(
            "projection_context".to_string(),
            json!("WorldHomeProjectionContext"),
        );
        fields.insert(
            "index_layer".to_string(),
            json!("WorldIndexes::world_home_sorted_ids_v1"),
        );
        fields.insert("world".to_string(), json!("trillionnium_world"));
        fields.insert(
            "tagline".to_string(),
            json!("现实世界被游戏引擎化：城市、工坊、市场、Agent 居民、资产和自由行动。"),
        );
        fields.insert("modules".to_string(), self.modules_json());
        fields
    }

    fn world_collection_fields(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert("zones".to_string(), json!(self.sorted_zones()));
        fields.insert("locations".to_string(), json!(self.sorted_locations()));
        fields.insert("entities".to_string(), json!(self.sorted_entities()));
        fields.insert("map_nodes".to_string(), json!(self.world.world_map_nodes));
        fields.insert(
            "player_positions".to_string(),
            json!(self.world.world_player_positions),
        );
        fields.insert("assets".to_string(), json!(self.world.world_assets));
        fields.insert(
            "asset_upgrades".to_string(),
            json!(self.world.world_asset_upgrades),
        );
        fields.insert("companies".to_string(), json!(self.world.world_companies));
        fields.insert("shops".to_string(), json!(self.world.world_shops));
        fields.insert("listings".to_string(), json!(self.world.world_listings));
        fields.insert(
            "economy_events".to_string(),
            json!(self.world.world_economy_events),
        );
        fields.insert("purchases".to_string(), json!(self.world.world_purchases));
        fields.insert(
            "work_orders".to_string(),
            json!(self.world.world_work_orders),
        );
        fields.insert(
            "work_deliveries".to_string(),
            json!(self.world.world_work_deliveries),
        );
        fields.insert(
            "work_acceptances".to_string(),
            json!(self.world.world_work_acceptances),
        );
        fields.insert(
            "work_rejections".to_string(),
            json!(self.world.world_work_rejections),
        );
        fields.insert(
            "work_reopens".to_string(),
            json!(self.world.world_work_reopens),
        );
        fields.insert(
            "work_cancellations".to_string(),
            json!(self.world.world_work_cancellations),
        );
        fields.insert("factions".to_string(), json!(self.sorted_factions()));
        fields.insert(
            "faction_standings".to_string(),
            json!(self.world.world_faction_standings),
        );
        fields.insert("contracts".to_string(), json!(self.world.world_contracts));
        fields.insert(
            "contract_completions".to_string(),
            json!(self.world.world_contract_completions),
        );
        fields.insert("recent_events".to_string(), json!(self.recent_events()));
        fields
    }

    fn map_runtime_fields(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert(
            "real_world_map_engine".to_string(),
            self.real_world_map_engine(),
        );
        fields
    }

    fn route_runtime_fields(&self) -> Map<String, Value> {
        let WorldRouteArtifacts {
            preview: route_preview,
            task_graph: route_task_graph,
            story: route_story,
            ..
        } = self.route_artifacts();
        let mut fields = Map::new();
        fields.insert("route_contract".to_string(), world_route_ui_contract_json());
        fields.insert("route_preview".to_string(), route_preview);
        fields.insert("route_task_graph".to_string(), route_task_graph);
        fields.insert("route_story".to_string(), route_story.to_value());
        fields
    }

    fn playability_runtime_json(&self) -> Value {
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
        let recovery_count = self.world.world_work_rejections.len()
            + self.world.world_work_reopens.len()
            + self.world.world_work_cancellations.len();
        json!({
            "contract_version": "trillionnium_world_playability_runtime_v1",
            "optimization_scope": "p0_p1_p2_full_playability",
            "player_loop": "map focus → bounty/contract → commission → submit/rate/recover → reward → next route",
            "lanes": [
                {"lane_id": "p0_first_session", "surface": "/app", "target": "first 3-minute playable loop"},
                {"lane_id": "p1_strategy_depth", "surface": "/world", "target": "economy, faction, guild, and recovery tradeoffs"},
                {"lane_id": "p2_retention_ops", "surface": "/league", "target": "daily route backlog, weekly raid, unlocks, and telemetry"}
            ],
            "runtime_counts": {
                "map_nodes": self.world.world_map_nodes.len(),
                "contracts": self.world.world_contracts.len(),
                "listings": self.world.world_listings.len(),
                "work_orders": self.world.world_work_orders.len(),
                "reviewable_work_orders": reviewable_work_count,
                "recovery_records": recovery_count,
                "economy_events": self.world.world_economy_events.len(),
                "relationships": self.world.world_relationships.len(),
                "faction_standings": self.world.world_faction_standings.len(),
            },
            "telemetry_events": [
                "first_focus_selected",
                "world_action_started",
                "commission_accepted",
                "result_submitted",
                "rating_or_recovery_chosen",
                "reward_read",
                "next_route_queued"
            ],
            "readiness_checks": [
                "world_home_exposes_p0_p1_p2_lanes",
                "runtime_counts_cover_recovery_and_economy",
                "telemetry_funnel_declared"
            ]
        })
    }

    fn projection_fields(&self) -> Map<String, Value> {
        let mut projection = self.static_fields();
        projection.extend(self.world_collection_fields());
        projection.extend(self.map_runtime_fields());
        projection.extend(self.route_runtime_fields());
        projection.insert(
            "playability_runtime".to_string(),
            self.playability_runtime_json(),
        );
        projection.insert("counts".to_string(), self.counts_json());
        projection
    }

    fn json(&self) -> Value {
        Value::Object(self.projection_fields())
    }
}

pub(super) fn world_home_projection_json(world: &WorldState) -> Value {
    WorldHomeProjectionContext::new(world).json()
}

pub(super) fn world_home_json(league: &LeagueState) -> Value {
    world_home_projection_json(&league.world)
}

pub(super) fn default_world_node_id() -> &'static str {
    "mirror-city-square"
}

pub(super) fn real_world_node_coordinates(node: &WorldMapNode) -> (f64, f64) {
    // Trillionnium World is a reality-mirror overlay. The first playable city is
    // anchored to a real map around Shanghai city center, then fine-grained MUD / Gather
    // nodes are placed as walkable markers on top of the real-world tile engine.
    const BASE_LAT: f64 = 31.230416;
    const BASE_LNG: f64 = 121.473701;
    const LAT_STEP: f64 = 0.0048;
    const LNG_STEP: f64 = 0.0065;
    let lat = BASE_LAT - (node.y as f64 * LAT_STEP);
    let lng = BASE_LNG + (node.x as f64 * LNG_STEP);
    (lat, lng)
}

pub(super) fn world_map_primary_action_json(
    action_id: &str,
    label: &str,
    kind: &str,
    command: String,
    panel_id: &str,
    body: String,
    move_target: Option<&str>,
) -> Value {
    let mut action = serde_json::Map::from_iter([
        ("action_id".to_string(), json!(action_id)),
        ("label".to_string(), json!(label)),
        ("kind".to_string(), json!(kind)),
        ("command".to_string(), json!(command)),
        ("web_panel_id".to_string(), json!(panel_id)),
        ("web_action_body".to_string(), json!(body)),
    ]);
    if let Some(move_target) = move_target {
        action.insert("web_move_target".to_string(), json!(move_target));
    }
    Value::Object(action)
}

pub(super) fn world_map_node_primary_actions_json(node: &WorldMapNode) -> Vec<Value> {
    let has_tag = |tag: &str| node.interaction_tags.iter().any(|value| value == tag);
    let mut actions = vec![
        world_map_primary_action_json(
            "move_here",
            "Move Here / 移动到这里",
            "movement",
            format!("/go {}", node.node_id),
            WORLD_ROUTE_MAP_MOVE_PANEL_ID,
            format!(
                "Move to「{0}」and inspect reality-mirror quests, Agents, and interactables / 移动到「{0}」并观察这里的现实镜像任务、Agent 和可交互对象。",
                node.name
            ),
            Some(node.node_id.as_str()),
        ),
        world_map_primary_action_json(
            "inspect_node",
            "Inspect Hub / 观察据点",
            "inspect",
            "/look".to_string(),
            WORLD_ROUTE_ACTION_PANEL_ID,
            format!(
                "Inspect「{}」in detail / 在这里详细观察：{}",
                node.name, node.description
            ),
            None,
        ),
    ];
    if has_tag("market") || has_tag("buy") || has_tag("sell") || has_tag("listing") {
        actions.push(world_map_primary_action_json(
            "open_market",
            "Open Quest Cards / 打开任务牌",
            "commerce",
            "/shops".to_string(),
            WORLD_ROUTE_COMMERCE_PANEL_ID,
            format!(
                "Browse bounties, commission needs, studio quest cards, and contracts at「{}」/ 在这里浏览悬赏机会、委托需求、工坊任务牌和可接取契约。",
                node.name
            ),
            None,
        ));
    }
    if has_tag("craft") || has_tag("asset") || has_tag("upgrade") || has_tag("company") {
        actions.push(world_map_primary_action_json(
            "open_workshop",
            "Enter Studio / 进入工坊",
            "craft",
            "/assets".to_string(),
            WORLD_ROUTE_ASSETS_PANEL_ID,
            format!(
                "Manage items, upgrade studios, create hubs, or prepare submittable results at「{}」/ 在这里整理道具、升级工坊、创建据点或准备可提交成果。",
                node.name
            ),
            None,
        ));
    }
    if has_tag("deliver") || has_tag("accept") || has_tag("reject") || has_tag("cancel") {
        actions.push(world_map_primary_action_json(
            "open_work_orders",
            "View Commissions / 查看委托",
            "work_order",
            "/work".to_string(),
            WORLD_ROUTE_COMMERCE_PANEL_ID,
            format!(
                "Handle result submission, rating, revision, reopen, cancel, and evidence packs at「{}」/ 在这里处理成果提交、评级、返工、重开、放弃和证据包。",
                node.name
            ),
            None,
        ));
    }
    if has_tag("arena") || has_tag("raid") || has_tag("guild") || has_tag("team") {
        actions.push(world_map_primary_action_json(
            "open_league",
            "Enter Arena / 进入竞技场",
            "league",
            "/league".to_string(),
            WORLD_ROUTE_LEAGUE_LINK_ID,
            format!(
                "Enter League from「{}」and turn real-world quests into ratings and team play / 从这里进入 League，把现实任务带进竞技评分和队伍协作。",
                node.name
            ),
            None,
        ));
    }
    if has_tag("wallet") || has_tag("refund") || has_tag("contract") {
        actions.push(world_map_primary_action_json(
            "open_wallet_contracts",
            "Rewards / Contracts · 奖励 / 契约",
            "ledger_contract",
            "/wallet".to_string(),
            WORLD_ROUTE_CONTRACTS_PANEL_ID,
            format!(
                "Check rewards, contracts, refunds, disputes, or rating status at「{}」/ 在这里检查奖励、契约、退回、争议或评级状态。",
                node.name
            ),
            None,
        ));
    }
    actions.truncate(5);
    actions
}

pub(super) fn real_world_node_marker_json(node: &WorldMapNode) -> Value {
    let (lat, lng) = real_world_node_coordinates(node);
    json!({
        "node_id": &node.node_id,
        "location_id": &node.location_id,
        "zone_id": &node.zone_id,
        "name": &node.name,
        "node_kind": &node.node_kind,
        "description": &node.description,
        "lat": lat,
        "lng": lng,
        "lat_string": format!("{lat:.6}"),
        "lng_string": format!("{lng:.6}"),
        "x": node.x,
        "y": node.y,
        "interaction_tags": &node.interaction_tags,
        "freedom_hooks": &node.freedom_hooks,
        "primary_actions": world_map_node_primary_actions_json(node),
    })
}

pub(super) fn trillionnium_world_map_gameplay_layer_contract_json() -> Value {
    json!({
        "contract_version": "trillionnium_world_map_gameplay_layer_v1",
        "product_name": "Trillionnium World Map",
        "base_map_role": "OpenStreetMap geospatial base layer / OpenStreetMap 真实地理底座",
        "upgrade_model": "OpenStreetMap upgraded with Trillionnium avatars, route nodes, quest cards, live events, and task completion loops / 在 OpenStreetMap 上升级游戏人物、路线节点、任务牌、实时事件和完成任务闭环",
        "primary_player_loop": "avatar runs on Trillionnium World Map → choose node → accept bounty/contract → submit evidence → rating/reward → next route",
        "avatar_layer_id": "trillionnium_player_avatar_runner_layer",
        "task_layer_id": "trillionnium_world_task_route_layer",
        "supports": {
            "player_avatars": true,
            "avatar_movement_between_nodes": true,
            "quest_route_edges": true,
            "avatar_task_route_overlays": true,
            "avatar_route_runners": true,
            "live_event_task_pulses": true,
            "openstreetmap_base_tiles": true
        }
    })
}

pub(super) fn world_map_player_avatars_json(
    world: &WorldState,
    matrix_user_id: &str,
    visible_markers: &[Value],
) -> Vec<Value> {
    let mut marker_by_node_id: HashMap<String, Value> = HashMap::new();
    let mut marker_by_location_id: HashMap<String, Value> = HashMap::new();
    for marker in visible_markers {
        if let Some(node_id) = marker.get("node_id").and_then(Value::as_str) {
            marker_by_node_id.insert(node_id.to_string(), marker.clone());
        }
        if let Some(location_id) = marker.get("location_id").and_then(Value::as_str) {
            marker_by_location_id
                .entry(location_id.to_string())
                .or_insert_with(|| marker.clone());
        }
    }

    let avatar_from_marker = |avatar_matrix_user_id: &str,
                              position: &WorldPlayerPosition,
                              marker: &Value| {
        let is_current_player = avatar_matrix_user_id == matrix_user_id;
        json!({
            "avatar_id": format!("avatar:{}", avatar_matrix_user_id),
            "matrix_user_id": avatar_matrix_user_id,
            "display_name": if is_current_player { "You / 你" } else { "Player / 玩家" },
            "avatar_kind": if is_current_player { "current_player" } else { "nearby_player" },
            "icon": if is_current_player { "🧍" } else { "🏃" },
            "node_id": &position.node_id,
            "location_id": &position.location_id,
            "node_name": marker.get("name").and_then(Value::as_str).unwrap_or("World node"),
            "lat": marker.get("lat").and_then(Value::as_f64).unwrap_or(31.230416),
            "lng": marker.get("lng").and_then(Value::as_f64).unwrap_or(121.473701),
            "updated_at_epoch": position.updated_at_epoch,
            "movement_status": "ready_to_run_task",
            "task_loop": "move → inspect → accept bounty/contract → submit evidence → rating/reward",
            "animation_hint": "run_between_route_nodes"
        })
    };

    let mut avatars = Vec::new();
    let mut positions: Vec<(&String, &WorldPlayerPosition)> =
        world.world_player_positions.iter().collect();
    positions.sort_by(|left, right| left.0.cmp(right.0));
    for (avatar_matrix_user_id, position) in positions {
        let marker = marker_by_node_id
            .get(&position.node_id)
            .or_else(|| marker_by_location_id.get(&position.location_id));
        if let Some(marker) = marker {
            avatars.push(avatar_from_marker(avatar_matrix_user_id, position, marker));
        }
    }

    let current_player_visible = avatars
        .iter()
        .any(|avatar| avatar.get("matrix_user_id").and_then(Value::as_str) == Some(matrix_user_id));
    if !current_player_visible {
        if let Some(default_node) = world.world_map_nodes.get(default_world_node_id()) {
            let marker = real_world_node_marker_json(default_node);
            let position = WorldPlayerPosition {
                matrix_user_id: matrix_user_id.to_string(),
                node_id: default_node.node_id.clone(),
                location_id: default_node.location_id.clone(),
                updated_at_epoch: 0,
            };
            avatars.push(avatar_from_marker(matrix_user_id, &position, &marker));
        }
    }

    avatars.truncate(16);
    avatars
}

pub(super) fn world_map_avatar_task_routes_json(
    world: &WorldState,
    indexes: &WorldIndexes,
    matrix_user_id: &str,
    route_artifacts: &WorldRouteArtifacts,
    limit: usize,
) -> Vec<Value> {
    let current_node = world
        .world_player_positions
        .get(matrix_user_id)
        .and_then(|position| world.world_map_nodes.get(&position.node_id))
        .or_else(|| world.world_map_nodes.get(default_world_node_id()))
        .or_else(|| {
            indexes
                .sorted_map_node_ids
                .first()
                .and_then(|node_id| world.world_map_nodes.get(node_id))
        });
    let Some(current_node) = current_node else {
        return Vec::new();
    };
    let (from_lat, from_lng) = real_world_node_coordinates(current_node);
    let mut seen_task_ids = HashSet::new();
    let mut routes = Vec::new();
    let tasks = route_artifacts
        .task_graph
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for task in tasks {
        if routes.len() >= limit {
            break;
        }
        let task_id = task
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or("route-task")
            .trim();
        if task_id.is_empty() || !seen_task_ids.insert(task_id.to_string()) {
            continue;
        }
        let latest_location_id = task
            .get("latest_location_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let target_node = task
            .get("suggested_node_id")
            .and_then(Value::as_str)
            .filter(|node_id| !node_id.trim().is_empty())
            .and_then(|node_id| world.world_map_nodes.get(node_id))
            .or_else(|| {
                task.get("next_opportunity_node_id")
                    .and_then(Value::as_str)
                    .filter(|node_id| !node_id.trim().is_empty())
                    .and_then(|node_id| world.world_map_nodes.get(node_id))
            })
            .or_else(|| indexes.first_map_node_for_location(world, latest_location_id));
        let Some(target_node) = target_node else {
            continue;
        };
        let (to_lat, to_lng) = real_world_node_coordinates(target_node);
        let latest_bucket = task
            .get("latest_bucket")
            .and_then(Value::as_str)
            .unwrap_or("route_task");
        let latest_status = task
            .get("latest_status")
            .and_then(Value::as_str)
            .unwrap_or("pending");
        let next_action_label = task
            .get("suggested_action_label")
            .and_then(Value::as_str)
            .filter(|label| !label.trim().is_empty())
            .or_else(|| {
                task.get("next_opportunity_action_label")
                    .and_then(Value::as_str)
                    .filter(|label| !label.trim().is_empty())
            })
            .unwrap_or("Open task route / 打开任务路线");
        let command = task
            .get("suggested_matrix_command")
            .and_then(Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .or_else(|| {
                task.get("next_opportunity_command")
                    .and_then(Value::as_str)
                    .filter(|command| !command.trim().is_empty())
            })
            .unwrap_or("/world action 跟进当前地图任务：补齐 deliverable、evidence、risk controls、next action 和 self-review。");
        routes.push(json!({
            "route_id": format!("avatar-task-route:{}:{}", matrix_user_id, task_id),
            "route_layer_id": "trillionnium_avatar_task_route_overlay",
            "route_kind": "avatar_task_route",
            "task_id": task_id,
            "matrix_user_id": matrix_user_id,
            "from_node_id": &current_node.node_id,
            "from_node_name": &current_node.name,
            "from": {"lat": from_lat, "lng": from_lng},
            "to_node_id": &target_node.node_id,
            "to_node_name": &target_node.name,
            "to": {"lat": to_lat, "lng": to_lng},
            "latest_location_id": latest_location_id,
            "latest_bucket": latest_bucket,
            "latest_status": latest_status,
            "route_stage_summary": task.get("route_stage_summary").and_then(Value::as_str).unwrap_or("task route ready"),
            "outcome_summary": task.get("outcome_summary").and_then(Value::as_str).unwrap_or("route outcome pending"),
            "next_action_label": next_action_label,
            "command": command,
            "reward_loop": "move avatar → complete task → submit evidence → rating/reward → next route",
            "movement_hint": "draw_avatar_task_route_from_current_node_to_target_node",
        }));
    }

    routes
}

pub(super) fn world_map_avatar_route_runners_json(
    avatar_task_routes: &[Value],
    limit: usize,
) -> Vec<Value> {
    avatar_task_routes
        .iter()
        .take(limit)
        .enumerate()
        .filter_map(|(index, route)| {
            let from = route.get("from")?.clone();
            let to = route.get("to")?.clone();
            let task_id = route
                .get("task_id")
                .and_then(Value::as_str)
                .unwrap_or("route-task");
            let matrix_user_id = route
                .get("matrix_user_id")
                .and_then(Value::as_str)
                .unwrap_or("@player:local.dev");
            let progress_ratio = (0.18 + (index as f64 % 5.0) * 0.14).min(0.82);
            Some(json!({
                "runner_id": format!("avatar-route-runner:{}:{}", matrix_user_id, task_id),
                "route_id": route.get("route_id").cloned().unwrap_or_else(|| json!("avatar-task-route")),
                "route_layer_id": "trillionnium_avatar_route_runner_layer",
                "route_kind": "avatar_route_runner",
                "task_id": task_id,
                "matrix_user_id": matrix_user_id,
                "from_node_id": route.get("from_node_id").cloned().unwrap_or_else(|| json!("current-node")),
                "from_node_name": route.get("from_node_name").cloned().unwrap_or_else(|| json!("Current node")),
                "from": from,
                "to_node_id": route.get("to_node_id").cloned().unwrap_or_else(|| json!("target-node")),
                "to_node_name": route.get("to_node_name").cloned().unwrap_or_else(|| json!("Task node")),
                "to": to,
                "latest_location_id": route.get("latest_location_id").cloned().unwrap_or_else(|| json!("")),
                "latest_status": route.get("latest_status").cloned().unwrap_or_else(|| json!("pending")),
                "next_action_label": route.get("next_action_label").cloned().unwrap_or_else(|| json!("Run to task / 跑向任务")),
                "reward_loop": route.get("reward_loop").cloned().unwrap_or_else(|| json!("move avatar → complete task → submit evidence → rating/reward → next route")),
                "movement_state": "en_route_to_task_reward",
                "movement_label": "Avatar running to task / 角色正在跑向任务",
                "runner_icon": "🏃",
                "progress_ratio": progress_ratio,
                "animation_kind": "looping_avatar_task_run",
                "animation_duration_ms": 4800 + (index as i64 * 360),
                "animation_hint": "animate_avatar_marker_between_route_endpoints",
            }))
        })
        .collect()
}

pub(super) fn real_world_map_region_shards_json() -> Vec<Value> {
    vec![
        json!({
            "region_id": "cn-shanghai-core",
            "name": "Shanghai Core",
            "status": "active",
            "coverage_kind": "street_block",
            "zoom_min": 14,
            "zoom_max": 19,
            "player_density_mode": "dense",
            "center": {"lat": 31.230416, "lng": 121.473701},
        }),
        json!({
            "region_id": "east-asia-grid",
            "name": "East Asia Grid",
            "status": "warm",
            "coverage_kind": "metro_cluster",
            "zoom_min": 8,
            "zoom_max": 14,
            "player_density_mode": "regional",
            "center": {"lat": 31.230416, "lng": 121.473701},
        }),
        json!({
            "region_id": "eurasia-corridors",
            "name": "Eurasia Corridors",
            "status": "planned",
            "coverage_kind": "continent_routes",
            "zoom_min": 5,
            "zoom_max": 9,
            "player_density_mode": "sparse",
            "center": {"lat": 48.8566, "lng": 2.3522},
        }),
        json!({
            "region_id": "global-overview",
            "name": "Global Overview",
            "status": "planned",
            "coverage_kind": "world_shards",
            "zoom_min": 3,
            "zoom_max": 5,
            "player_density_mode": "overview",
            "center": {"lat": 20.0, "lng": 0.0},
        }),
    ]
}

pub(super) fn real_world_map_lod_layers_json() -> Vec<Value> {
    vec![
        json!({
            "layer_id": "global_region_shards",
            "name": "Global Region Shards",
            "zoom_min": 3,
            "zoom_max": 5,
            "render_mode": "region_shards",
            "presentation": "continent_and_ocean_overview",
        }),
        json!({
            "layer_id": "metro_corridors",
            "name": "Metro Corridors",
            "zoom_min": 6,
            "zoom_max": 9,
            "render_mode": "city_clusters_routes",
            "presentation": "gather_like_city_clusters",
        }),
        json!({
            "layer_id": "district_pois",
            "name": "District POIs",
            "zoom_min": 10,
            "zoom_max": 13,
            "render_mode": "district_pois",
            "presentation": "hero_tale_district_nodes",
        }),
        json!({
            "layer_id": "street_level_world_nodes",
            "name": "Street Level World Nodes",
            "zoom_min": 14,
            "zoom_max": 19,
            "render_mode": "street_nodes",
            "presentation": "full_trillionnium_interactable_nodes",
        }),
    ]
}

pub(super) fn world_map_default_radius_km(zoom: i64) -> f64 {
    match zoom {
        i if i <= 5 => 4000.0,
        i if i <= 8 => 900.0,
        i if i <= 11 => 120.0,
        i if i <= 13 => 22.0,
        _ => 4.5,
    }
}

pub(super) fn world_map_lod_mode(zoom: i64) -> &'static str {
    match zoom {
        i if i <= 5 => "region_shards",
        i if i <= 9 => "city_clusters_routes",
        i if i <= 13 => "district_pois",
        _ => "street_nodes",
    }
}

pub(super) fn clamp_web_mercator_lat(lat: f64) -> f64 {
    lat.clamp(-85.05112878, 85.05112878)
}

pub(super) fn normalize_lng(lng: f64) -> f64 {
    ((lng + 180.0).rem_euclid(360.0)) - 180.0
}

pub(super) fn world_map_quadkey(x: i64, y: i64, zoom: i64) -> String {
    let mut quadkey = String::new();
    for level in (1..=zoom).rev() {
        let mask = 1_i64 << (level - 1);
        let mut digit = 0;
        if (x & mask) != 0 {
            digit += 1;
        }
        if (y & mask) != 0 {
            digit += 2;
        }
        quadkey.push(char::from_digit(digit, 10).unwrap_or('0'));
    }
    quadkey
}

pub(super) fn world_map_tile_coord(lat: f64, lng: f64, zoom: i64) -> (i64, i64, i64) {
    let zoom = zoom.clamp(0, 22);
    let n = 2_f64.powi(zoom as i32);
    let lat_rad = clamp_web_mercator_lat(lat).to_radians();
    let lng = normalize_lng(lng);
    let x = (((lng + 180.0) / 360.0) * n).floor().clamp(0.0, n - 1.0) as i64;
    let y = ((1.0 - ((lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / std::f64::consts::PI)) / 2.0
        * n)
        .floor()
        .clamp(0.0, n - 1.0) as i64;
    (x, y, zoom)
}

pub(super) fn world_map_tile_json(lat: f64, lng: f64, zoom: i64, role: &str) -> Value {
    let (x, y, z) = world_map_tile_coord(lat, lng, zoom);
    json!({
        "tile_id": format!("osm-z{z}-x{x}-y{y}"),
        "provider": "OpenStreetMap",
        "projection": "EPSG:3857",
        "z": z,
        "x": x,
        "y": y,
        "quadkey": world_map_quadkey(x, y, z),
        "role": role,
        "lod_mode": world_map_lod_mode(z),
        "url_template": "https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png",
        "tile_url": format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
    })
}

pub(super) fn world_map_tile_pyramid_json(lat: f64, lng: f64) -> Vec<Value> {
    [3, 6, 10, 14, 15]
        .into_iter()
        .map(|zoom| world_map_tile_json(lat, lng, zoom, "pyramid_anchor"))
        .collect()
}

pub(super) fn world_map_visible_tile_shards_json(
    center_lat: f64,
    center_lng: f64,
    zoom: i64,
    visible_markers: &[Value],
) -> Vec<Value> {
    let (center_x, center_y, z) = world_map_tile_coord(center_lat, center_lng, zoom);
    let n = 1_i64 << z;
    let tile_radius = match z {
        i if i <= 5 => 1,
        i if i <= 9 => 1,
        i if i <= 13 => 2,
        _ => 1,
    };
    let mut shards = Vec::new();
    for dy in -tile_radius..=tile_radius {
        for dx in -tile_radius..=tile_radius {
            let x = (center_x + dx).clamp(0, n - 1);
            let y = (center_y + dy).clamp(0, n - 1);
            let mut node_ids = Vec::new();
            for marker in visible_markers {
                let marker_lat = marker
                    .get("lat")
                    .and_then(Value::as_f64)
                    .unwrap_or(center_lat);
                let marker_lng = marker
                    .get("lng")
                    .and_then(Value::as_f64)
                    .unwrap_or(center_lng);
                let (marker_x, marker_y, _) = world_map_tile_coord(marker_lat, marker_lng, z);
                if marker_x == x && marker_y == y {
                    if let Some(node_id) = marker.get("node_id").and_then(Value::as_str) {
                        node_ids.push(node_id.to_string());
                    }
                }
            }
            shards.push(json!({
                "tile_id": format!("osm-z{z}-x{x}-y{y}"),
                "z": z,
                "x": x,
                "y": y,
                "quadkey": world_map_quadkey(x, y, z),
                "lod_mode": world_map_lod_mode(z),
                "tile_status": if x == center_x && y == center_y { "active" } else { "prefetch" },
                "marker_count": node_ids.len(),
                "node_ids": node_ids,
                "url": format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
            }));
        }
    }
    shards
}

pub(super) fn geo_distance_km(from_lat: f64, from_lng: f64, to_lat: f64, to_lng: f64) -> f64 {
    let earth_radius_km = 6371.0;
    let delta_lat = (to_lat - from_lat).to_radians();
    let delta_lng = (to_lng - from_lng).to_radians();
    let from_lat = from_lat.to_radians();
    let to_lat = to_lat.to_radians();
    let a = (delta_lat / 2.0).sin().powi(2)
        + from_lat.cos() * to_lat.cos() * (delta_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    earth_radius_km * c
}

pub(super) fn world_map_region_window_json(
    center_lat: f64,
    center_lng: f64,
    zoom: i64,
) -> Vec<Value> {
    let mut regions = real_world_map_region_shards_json();
    for region in &mut regions {
        let region_lat = region
            .get("center")
            .and_then(|center| center.get("lat"))
            .and_then(Value::as_f64)
            .unwrap_or(center_lat);
        let region_lng = region
            .get("center")
            .and_then(|center| center.get("lng"))
            .and_then(Value::as_f64)
            .unwrap_or(center_lng);
        let zoom_min = region.get("zoom_min").and_then(Value::as_i64).unwrap_or(3);
        let zoom_max = region.get("zoom_max").and_then(Value::as_i64).unwrap_or(19);
        let distance_km = geo_distance_km(center_lat, center_lng, region_lat, region_lng);
        let active_threshold_km = match zoom {
            i if i >= 14 => 35.0,
            i if i >= 10 => 420.0,
            i if i >= 6 => 4200.0,
            _ => 18000.0,
        };
        let warm_threshold_km = active_threshold_km * 3.0;
        let in_zoom_band = zoom >= zoom_min && zoom <= zoom_max;
        let near_zoom_band = zoom + 1 >= zoom_min && zoom - 1 <= zoom_max;
        let status = if in_zoom_band && distance_km <= active_threshold_km {
            "active"
        } else if distance_km <= warm_threshold_km || near_zoom_band {
            "warm"
        } else {
            "planned"
        };
        if let Some(object) = region.as_object_mut() {
            object.insert(
                "distance_km".to_string(),
                json!((distance_km * 10.0).round() / 10.0),
            );
            object.insert("status".to_string(), json!(status));
            object.insert(
                "camera_relevance".to_string(),
                json!(match status {
                    "active" => "in_view",
                    "warm" => "nearby",
                    _ => "distant",
                }),
            );
        }
    }
    regions.sort_by(|left, right| {
        let status_rank = |value: &Value| match value.get("status").and_then(Value::as_str) {
            Some("active") => 0,
            Some("warm") => 1,
            _ => 2,
        };
        status_rank(left).cmp(&status_rank(right)).then_with(|| {
            let left_distance = left
                .get("distance_km")
                .and_then(Value::as_f64)
                .unwrap_or(f64::MAX);
            let right_distance = right
                .get("distance_km")
                .and_then(Value::as_f64)
                .unwrap_or(f64::MAX);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    regions
}

pub(super) fn world_map_player_density_summary_json(
    world: &WorldState,
    active_region: &Value,
    visible_markers: &[Value],
    zoom: i64,
) -> Value {
    let mode = active_region
        .get("player_density_mode")
        .and_then(Value::as_str)
        .unwrap_or(match zoom {
            i if i >= 14 => "dense",
            i if i >= 10 => "regional",
            i if i >= 6 => "sparse",
            _ => "overview",
        });
    let known_player_positions = world.world_player_positions.len() as i64;
    let visible_node_count = visible_markers.len() as i64;
    let estimated_concurrent_players = match mode {
        "dense" => (known_player_positions * 4).max(18 + visible_node_count * 3),
        "regional" => (known_player_positions * 3).max(8 + visible_node_count * 2),
        "sparse" => (known_player_positions * 2).max(3 + visible_node_count),
        _ => (known_player_positions * 5).max(12),
    };
    let shard_pressure = match (mode, visible_node_count, zoom) {
        ("dense", count, z) if count >= 6 && z >= 14 => "high",
        (_, count, _) if count >= 4 => "medium",
        _ => "low",
    };
    let prefetch_budget_tiles = match shard_pressure {
        "high" => 6,
        "medium" => 4,
        _ => 3,
    };
    let recommended_refresh_ms = match (mode, shard_pressure) {
        ("dense", "high") => 1200,
        ("dense", _) => 1600,
        ("regional", _) => 2200,
        ("sparse", _) => 3200,
        _ => 4200,
    };
    let scaling_tier = match estimated_concurrent_players {
        i if i >= 48 => "metro_cluster",
        i if i >= 20 => "district_band",
        _ => "street_cluster",
    };
    json!({
        "mode": mode,
        "visible_node_count": visible_node_count,
        "known_player_positions": known_player_positions,
        "estimated_concurrent_players": estimated_concurrent_players,
        "shard_pressure": shard_pressure,
        "prefetch_budget_tiles": prefetch_budget_tiles,
        "recommended_refresh_ms": recommended_refresh_ms,
        "scaling_tier": scaling_tier,
        "summary": format!(
            "{} density · est {} concurrent players · shard pressure {} · refresh {}ms",
            mode, estimated_concurrent_players, shard_pressure, recommended_refresh_ms
        ),
    })
}

pub(super) fn world_map_prefetch_queue_json(
    visible_tile_shards: &[Value],
    player_density: &Value,
    zoom: i64,
) -> Vec<Value> {
    let mode = player_density
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let budget = player_density
        .get("prefetch_budget_tiles")
        .and_then(Value::as_u64)
        .unwrap_or(4) as usize;
    let density_weight = match mode {
        "dense" => 4,
        "regional" => 3,
        "sparse" => 2,
        _ => 1,
    };
    let mut queue: Vec<(i64, Value)> = visible_tile_shards
        .iter()
        .filter(|tile| tile.get("tile_status").and_then(Value::as_str) != Some("active"))
        .map(|tile| {
            let marker_count = tile
                .get("marker_count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as i64;
            let lod_mode = tile
                .get("lod_mode")
                .and_then(Value::as_str)
                .unwrap_or("street_nodes");
            let lod_weight = match lod_mode {
                "street_nodes" => 3,
                "district_pois" => 2,
                _ => 1,
            };
            let priority_score = marker_count * 4
                + density_weight
                + lod_weight
                + if zoom >= 14 {
                    3
                } else if zoom >= 10 {
                    2
                } else {
                    1
                };
            let priority_label = if priority_score >= 12 {
                "high"
            } else if priority_score >= 8 {
                "medium"
            } else {
                "low"
            };
            let prefetch_reason = if marker_count > 0 {
                "marker_cluster"
            } else if mode == "dense" {
                "dense_region_ring"
            } else {
                "neighbor_tile_warmup"
            };
            let mut enriched = tile.clone();
            if let Some(object) = enriched.as_object_mut() {
                object.insert("priority_score".to_string(), json!(priority_score));
                object.insert("priority_label".to_string(), json!(priority_label));
                object.insert("prefetch_reason".to_string(), json!(prefetch_reason));
            }
            (priority_score, enriched)
        })
        .collect();
    queue.sort_by(|left, right| right.0.cmp(&left.0));
    queue
        .into_iter()
        .take(budget)
        .map(|(_, tile)| tile)
        .collect()
}

pub(super) fn world_map_live_event_stream_json(
    world: &WorldState,
    indexes: &WorldIndexes,
    visible_markers: &[Value],
    center_lat: f64,
    center_lng: f64,
    limit: usize,
) -> Vec<Value> {
    let marker_by_location: HashMap<String, Value> = visible_markers
        .iter()
        .filter_map(|marker| {
            marker
                .get("location_id")
                .and_then(Value::as_str)
                .map(|location_id| (location_id.to_string(), marker.clone()))
        })
        .collect();
    let mut stream = Vec::new();
    let mut event_candidates: Vec<(usize, &WorldEvent)> = marker_by_location
        .keys()
        .filter_map(|location_id| indexes.event_indices_by_location.get(location_id))
        .flat_map(|event_indices| event_indices.iter().rev().take(limit))
        .filter_map(|index| world.world_events.get(*index).map(|event| (*index, event)))
        .collect();
    event_candidates.sort_by(|left, right| right.0.cmp(&left.0));

    for (_, event) in event_candidates.into_iter().take(limit) {
        if let Some(marker) = marker_by_location.get(&event.location_id) {
            let node_id = marker
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("node");
            let node_name = marker.get("name").and_then(Value::as_str).unwrap_or("POI");
            let marker_lat = marker
                .get("lat")
                .and_then(Value::as_f64)
                .unwrap_or(center_lat);
            let marker_lng = marker
                .get("lng")
                .and_then(Value::as_f64)
                .unwrap_or(center_lng);
            let distance_km = geo_distance_km(center_lat, center_lng, marker_lat, marker_lng);
            stream.push(json!({
                "event_id": &event.event_id,
                "event_kind": &event.event_kind,
                "location_id": &event.location_id,
                "node_id": node_id,
                "node_name": node_name,
                "distance_km": (distance_km * 10.0).round() / 10.0,
                "impact_score": event.impact_score,
                "body": &event.body,
                "result": &event.result,
                "cex_task_id": &event.cex_task_id,
                "created_at_epoch": event.created_at_epoch,
                "stream_status": "nearby",
            }));
        }
    }
    if stream.is_empty() {
        for event in indexed_recent(&world.world_events, &indexes.recent_event_indices, limit) {
            stream.push(json!({
                "event_id": &event.event_id,
                "event_kind": &event.event_kind,
                "location_id": &event.location_id,
                "node_id": "global-feed",
                "node_name": &event.location_id,
                "distance_km": Value::Null,
                "impact_score": event.impact_score,
                "body": &event.body,
                "result": &event.result,
                "cex_task_id": &event.cex_task_id,
                "created_at_epoch": event.created_at_epoch,
                "stream_status": "global_fallback",
            }));
        }
    }
    stream
}

pub(super) fn real_world_map_renderer_adapter_json() -> Value {
    json!({
        "adapter_id": "leaflet_renderer_adapter_v1",
        "adapter_contract_version": 1,
        "active_engine_id": "leaflet_openstreetmap_v1",
        "runtime_shape": "shared_browser_js_adapter",
        "runtime_handle_name": "mapRuntime",
        "surface_scope": ["client_app_web_shell", "world_web_shell"],
        "adapter_methods": [
            "createMap",
            "setBaseLayer",
            "createOverlayLayer",
            "setOverlayVisibility",
            "clearOverlay",
            "latLngBounds",
            "renderRouteLine",
            "renderPoiMarker",
            "renderDensityCircle",
            "renderRegionAnchor",
            "renderTileFrame",
            "renderEventPulse",
            "renderPlayerAvatar",
            "renderMovingAvatar",
            "getCenter",
            "getZoom",
            "onViewportChange",
            "fitBounds",
            "focus",
            "invalidateSize"
        ],
        "adapter_contract": {
            "purpose": "keep web map surfaces renderer-neutral while Leaflet remains the active implementation",
            "supports_overlay_primitives": true,
            "supports_player_avatar_layer": true,
            "supports_avatar_route_runner_layer": true,
            "supports_camera_reads": true,
            "supports_viewport_events": true,
            "supports_future_engine_swap": true
        },
        "future_engine_candidate": "maplibre_gl_v1"
    })
}

pub(super) fn real_world_map_planned_upgrade_engine_json() -> Value {
    json!({
        "engine_id": "maplibre_gl_v1",
        "promotion_trigger": "vector_webgl_pressure_after_adapter_seam",
        "status": "planned_not_active",
        "gating_contract": "renderer_adapter.adapter_contract_version >= 1"
    })
}

pub(super) fn world_map_viewport_json(
    world: &WorldState,
    matrix_user_id: &str,
    lat: Option<f64>,
    lng: Option<f64>,
    zoom: Option<i64>,
    radius_km: Option<f64>,
    limit: Option<usize>,
) -> Value {
    let indexes = build_world_indexes(world);
    let route_artifacts = build_world_route_artifacts(world);
    let map = world_map_json_with_route_artifacts(world, matrix_user_id, &route_artifacts);
    let engine = map
        .get("real_world_map_engine")
        .cloned()
        .unwrap_or_else(|| real_world_map_engine_json(&[], None));
    let fallback_center_lat = engine
        .get("center")
        .and_then(|center| center.get("lat"))
        .and_then(Value::as_f64)
        .unwrap_or(31.230416);
    let fallback_center_lng = engine
        .get("center")
        .and_then(|center| center.get("lng"))
        .and_then(Value::as_f64)
        .unwrap_or(121.473701);
    let center_lat = lat.unwrap_or(fallback_center_lat);
    let center_lng = lng.unwrap_or(fallback_center_lng);
    let zoom = zoom
        .unwrap_or_else(|| engine.get("zoom").and_then(Value::as_i64).unwrap_or(15))
        .clamp(3, 19);
    let radius_km = radius_km
        .unwrap_or_else(|| world_map_default_radius_km(zoom))
        .clamp(0.5, 5000.0);
    let marker_limit = limit
        .unwrap_or(if zoom >= 14 {
            18
        } else if zoom >= 10 {
            12
        } else {
            8
        })
        .clamp(1, 64);

    let mut markers_with_distance: Vec<(f64, Value)> = engine
        .get("markers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|marker| {
            let marker_lat = marker
                .get("lat")
                .and_then(Value::as_f64)
                .unwrap_or(center_lat);
            let marker_lng = marker
                .get("lng")
                .and_then(Value::as_f64)
                .unwrap_or(center_lng);
            let distance = geo_distance_km(center_lat, center_lng, marker_lat, marker_lng);
            (distance, marker)
        })
        .collect();
    markers_with_distance.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut visible_markers: Vec<Value> = markers_with_distance
        .iter()
        .filter(|(distance, _)| *distance <= radius_km)
        .take(marker_limit)
        .map(|(distance, marker)| {
            let mut marker = marker.clone();
            if let Some(object) = marker.as_object_mut() {
                object.insert(
                    "distance_km".to_string(),
                    json!((distance * 10.0).round() / 10.0),
                );
            }
            marker
        })
        .collect();
    if visible_markers.is_empty() {
        visible_markers = markers_with_distance
            .into_iter()
            .take(marker_limit)
            .map(|(distance, mut marker)| {
                if let Some(object) = marker.as_object_mut() {
                    object.insert(
                        "distance_km".to_string(),
                        json!((distance * 10.0).round() / 10.0),
                    );
                }
                marker
            })
            .collect();
    }

    let stream_region_shards = world_map_region_window_json(center_lat, center_lng, zoom);
    let active_region = stream_region_shards
        .iter()
        .find(|region| region.get("status").and_then(Value::as_str) == Some("active"))
        .cloned()
        .or_else(|| stream_region_shards.first().cloned())
        .unwrap_or_else(|| {
            json!({
                "region_id": "cn-shanghai-core",
                "name": "Shanghai Core",
                "status": "active",
                "player_density_mode": "dense",
            })
        });
    let poi_hotspots: Vec<Value> = visible_markers
        .iter()
        .take(6)
        .map(|marker| {
            json!({
                "node_id": marker.get("node_id").and_then(Value::as_str).unwrap_or("unknown"),
                "name": marker.get("name").and_then(Value::as_str).unwrap_or("POI"),
                "node_kind": marker.get("node_kind").and_then(Value::as_str).unwrap_or("poi"),
                "distance_km": marker.get("distance_km").cloned().unwrap_or_else(|| json!(0.0)),
                "primary_actions": marker.get("primary_actions").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect();
    let tile_center = world_map_tile_json(center_lat, center_lng, zoom, "viewport_center");
    let visible_tile_shards =
        world_map_visible_tile_shards_json(center_lat, center_lng, zoom, &visible_markers);
    let player_density =
        world_map_player_density_summary_json(world, &active_region, &visible_markers, zoom);
    let player_avatars = world_map_player_avatars_json(world, matrix_user_id, &visible_markers);
    let avatar_task_routes =
        world_map_avatar_task_routes_json(world, &indexes, matrix_user_id, &route_artifacts, 6);
    let avatar_route_runners = world_map_avatar_route_runners_json(&avatar_task_routes, 6);
    let prefetch_queue = world_map_prefetch_queue_json(&visible_tile_shards, &player_density, zoom);
    let live_event_stream = world_map_live_event_stream_json(
        world,
        &indexes,
        &visible_markers,
        center_lat,
        center_lng,
        6,
    );
    let stream_region_count = stream_region_shards.len();
    let tile_shard_count = visible_tile_shards.len();
    let prefetch_count = prefetch_queue.len();
    let marker_count = visible_markers.len();
    let live_event_count = live_event_stream.len();
    let player_avatar_count = player_avatars.len();
    let avatar_task_route_count = avatar_task_routes.len();
    let avatar_route_runner_count = avatar_route_runners.len();
    let viewport_path = format!(
        "/v1/world/map/{}/viewport?lat={:.6}&lng={:.6}&zoom={}&radius_km={:.1}&limit={}",
        matrix_user_id, center_lat, center_lng, zoom, radius_km, marker_limit
    );
    let web_session_viewport_path = format!(
        "/world/web/map-viewport?lat={:.6}&lng={:.6}&zoom={}&radius_km={:.1}&limit={}",
        center_lat, center_lng, zoom, radius_km, marker_limit
    );

    json!({
        "kind": "trillionnium_world_map_viewport",
        "world": "trillionnium_world",
        "matrix_user_id": matrix_user_id,
        "engine_id": engine.get("engine_id").and_then(Value::as_str).unwrap_or("leaflet_openstreetmap_v1"),
        "tile_provider": engine.get("tile_provider").and_then(Value::as_str).unwrap_or("OpenStreetMap"),
        "mirror_scope": engine.get("mirror_scope").and_then(Value::as_str).unwrap_or("global_real_world_tiles"),
        "active_region": active_region,
        "center": {
            "lat": center_lat,
            "lng": center_lng,
            "lat_string": format!("{center_lat:.6}"),
            "lng_string": format!("{center_lng:.6}"),
        },
        "zoom": zoom,
        "radius_km": radius_km,
        "lod_mode": world_map_lod_mode(zoom),
        "tile_center": tile_center,
        "stream_region_shards": stream_region_shards,
        "stream_region_count": stream_region_count,
        "visible_tile_shards": visible_tile_shards,
        "tile_shard_count": tile_shard_count,
        "prefetch_queue": prefetch_queue,
        "prefetch_count": prefetch_count,
        "visible_markers": visible_markers,
        "marker_count": marker_count,
        "poi_hotspots": poi_hotspots,
        "player_density": player_density,
        "player_avatars": player_avatars,
        "player_avatar_count": player_avatar_count,
        "avatar_task_routes": avatar_task_routes,
        "avatar_task_route_count": avatar_task_route_count,
        "avatar_route_runners": avatar_route_runners,
        "avatar_route_runner_count": avatar_route_runner_count,
        "gameplay_layer_contract": trillionnium_world_map_gameplay_layer_contract_json(),
        "live_event_stream": live_event_stream,
        "live_event_stream_index_layer": "WorldIndexes::event_indices_by_location_v1",
        "live_event_count": live_event_count,
        "viewport_path": viewport_path,
        "web_session_viewport_path": web_session_viewport_path,
        "viewport_contract": {
            "purpose": "stream active region shards, POIs, nearby world nodes, live events, and tile prefetch hints for the current camera",
            "default_radius_km": world_map_default_radius_km(zoom),
            "limit": marker_limit,
            "supports_live_event_stream": true,
            "supports_prefetch_queue": true,
            "supports_player_density": true,
            "supports_player_avatars": true,
            "supports_avatar_task_routes": true,
            "supports_avatar_route_runners": true,
        }
    })
}

pub(super) fn real_world_map_engine_json(
    nodes: &[WorldMapNode],
    current_node: Option<&WorldMapNode>,
) -> Value {
    let markers: Vec<Value> = nodes.iter().map(real_world_node_marker_json).collect();
    let mut node_lookup: HashMap<&str, &WorldMapNode> = HashMap::new();
    for node in nodes {
        node_lookup.insert(node.node_id.as_str(), node);
    }
    let mut route_edges = Vec::new();
    for node in nodes {
        let (from_lat, from_lng) = real_world_node_coordinates(node);
        let mut exits: Vec<(&String, &String)> = node.exits.iter().collect();
        exits.sort_by(|left, right| left.0.cmp(right.0));
        for (direction, target_node_id) in exits {
            if let Some(target_node) = node_lookup.get(target_node_id.as_str()) {
                let (to_lat, to_lng) = real_world_node_coordinates(target_node);
                route_edges.push(json!({
                    "direction": direction,
                    "from_node_id": &node.node_id,
                    "to_node_id": &target_node.node_id,
                    "from": {"lat": from_lat, "lng": from_lng},
                    "to": {"lat": to_lat, "lng": to_lng},
                }));
            }
        }
    }
    let mut poi_hotspots: Vec<Value> = nodes
        .iter()
        .map(|node| {
            let mut marker = real_world_node_marker_json(node);
            let hotspot_score = (node.interaction_tags.len() + node.freedom_hooks.len()) as i64;
            if let Some(object) = marker.as_object_mut() {
                object.insert("hotspot_score".to_string(), json!(hotspot_score));
            }
            marker
        })
        .collect();
    poi_hotspots.sort_by(|left, right| {
        right
            .get("hotspot_score")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .cmp(
                &left
                    .get("hotspot_score")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            )
    });
    poi_hotspots.truncate(6);
    let region_shards = real_world_map_region_shards_json();
    let lod_layers = real_world_map_lod_layers_json();
    let (center_lat, center_lng) = current_node
        .map(real_world_node_coordinates)
        .unwrap_or((31.230416, 121.473701));
    let center_tile = world_map_tile_json(center_lat, center_lng, 15, "engine_center");
    let tile_pyramid = world_map_tile_pyramid_json(center_lat, center_lng);
    json!({
        "engine_id": "leaflet_openstreetmap_v1",
        "product_name": "Trillionnium World Map",
        "engine": "Leaflet",
        "tile_provider": "OpenStreetMap",
        "tile_url_template": "https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png",
        "attribution": "© OpenStreetMap contributors",
        "projection": "EPSG:3857",
        "overlay_kind": "trillionnium_world_gameplay_layers",
        "real_world_anchor": "Shanghai, China",
        "mirror_scope": "global_real_world_tiles",
        "full_mirror_strategy": "openstreetmap_global_base_upgraded_with_trillionnium_avatar_task_layer",
        "simplification_style": "gather_hero_tale_lod",
        "scaling_goal": "many_players_via_lightweight_nodes_routes_and_region_shards",
        "map_ui_mode": "game_world_first",
        "gameplay_layer_contract": trillionnium_world_map_gameplay_layer_contract_json(),
        "renderer_adapter": real_world_map_renderer_adapter_json(),
        "planned_upgrade_engine": real_world_map_planned_upgrade_engine_json(),
        "active_region_id": "cn-shanghai-core",
        "region_shards": region_shards,
        "lod_layers": lod_layers,
        "tile_pyramid": tile_pyramid,
        "center_tile": center_tile,
        "tile_shard_strategy": {
            "kind": "web_mercator_tile_window",
            "provider": "OpenStreetMap",
            "projection": "EPSG:3857",
            "active_tile_role": "viewport_center",
            "prefetch_strategy": "neighbor_tiles_by_zoom_lod",
            "lod_source": "zoom_to_gather_hero_tale_layer",
        },
        "poi_hotspots": poi_hotspots,
        "viewport_api": {
            "path_template": "/v1/world/map/{matrix_user_id}/viewport?lat={lat}&lng={lng}&zoom={zoom}&radius_km={radius_km}&limit={limit}",
            "web_session_path_template": "/world/web/map-viewport?lat={lat}&lng={lng}&zoom={zoom}&radius_km={radius_km}&limit={limit}",
            "default_radius_km": 4.5,
            "supported_zoom_min": 3,
            "supported_zoom_max": 19,
            "purpose": "camera-scoped streaming for real-world mirror shards and Trillionnium overlays",
        },
        "center": {
            "lat": center_lat,
            "lng": center_lng,
            "lat_string": format!("{center_lat:.6}"),
            "lng_string": format!("{center_lng:.6}"),
        },
        "zoom": 15,
        "min_zoom": 3,
        "max_zoom": 19,
        "markers": markers,
        "route_edges": route_edges,
    })
}

#[derive(Debug)]
pub(super) struct WorldMapProjectionContext<'a> {
    world: &'a WorldState,
    matrix_user_id: &'a str,
    route_artifacts: &'a WorldRouteArtifacts,
    indexes: WorldIndexes,
}

impl<'a> WorldMapProjectionContext<'a> {
    fn new(
        world: &'a WorldState,
        matrix_user_id: &'a str,
        route_artifacts: &'a WorldRouteArtifacts,
    ) -> Self {
        Self {
            world,
            matrix_user_id,
            route_artifacts,
            indexes: build_world_indexes(world),
        }
    }

    fn sorted_nodes(&self) -> Vec<WorldMapNode> {
        self.indexes
            .sorted_map_node_ids
            .iter()
            .filter_map(|node_id| self.world.world_map_nodes.get(node_id).cloned())
            .collect()
    }

    fn current_node_id(&self) -> String {
        self.world
            .world_player_positions
            .get(self.matrix_user_id)
            .map(|position| position.node_id.as_str())
            .filter(|node_id| self.world.world_map_nodes.contains_key(*node_id))
            .unwrap_or(default_world_node_id())
            .to_string()
    }

    fn current_node(&self, current_node_id: &str) -> Option<WorldMapNode> {
        self.world
            .world_map_nodes
            .get(current_node_id)
            .or_else(|| self.world.world_map_nodes.get(default_world_node_id()))
            .cloned()
    }

    fn json(&self) -> Value {
        let nodes = self.sorted_nodes();
        let current_node_id = self.current_node_id();
        let current_node = self.current_node(&current_node_id);
        let exits = current_node
            .as_ref()
            .map(|node| node.exits.clone())
            .unwrap_or_default();
        let real_world_map_engine = real_world_map_engine_json(&nodes, current_node.as_ref());
        json!({
            "kind": "trillionnium_world_map",
            "projection_layer": "world_map_projection_v1",
            "projection_context": "WorldMapProjectionContext",
            "index_layer": "WorldIndexes::sorted_map_node_ids_v1",
            "world": "trillionnium_world",
            "matrix_user_id": self.matrix_user_id,
            "style": "leaflet_openstreetmap_real_world_overlay_v1",
            "fallback_style": "hero_tale_gather_text_map_v1",
            "real_world_map_engine": real_world_map_engine,
            "route_preview": self.route_artifacts.preview.clone(),
            "route_task_graph": self.route_artifacts.task_graph.clone(),
            "route_story": self.route_artifacts.story.to_value(),
            "route_contract": world_route_ui_contract_json(),
            "map_nodes": nodes,
            "current_node": current_node,
            "current_node_id": current_node_id,
            "exits": exits,
            "counts": {
                "map_nodes": self.world.world_map_nodes.len(),
                "player_positions": self.world.world_player_positions.len(),
            }
        })
    }
}

pub(super) fn world_map_json_with_route_artifacts(
    world: &WorldState,
    matrix_user_id: &str,
    route_artifacts: &WorldRouteArtifacts,
) -> Value {
    WorldMapProjectionContext::new(world, matrix_user_id, route_artifacts).json()
}

pub(super) fn world_map_json(league: &LeagueState, matrix_user_id: &str) -> Value {
    let route_artifacts = build_world_route_artifacts(&league.world);
    world_map_json_with_route_artifacts(&league.world, matrix_user_id, &route_artifacts)
}
