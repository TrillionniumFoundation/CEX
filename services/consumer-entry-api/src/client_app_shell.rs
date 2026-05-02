use super::*;

fn client_app_visible_copy(value: &str) -> String {
    let mut copy = value.to_string();
    let replacements = [
        ("Starter Studio", "Starter Studio / 新手工坊"),
        ("Forge Workbench", "Forge Workbench / 锻造工坊"),
        ("Asset Yard", "Asset Yard / 道具庭院"),
        ("ZBJ Market Gate", "Bounty Market Gate / 悬赏集市门"),
        ("League Coliseum", "League Coliseum / League 竞技场"),
        ("starter-studio", "starter-studio / 新手工坊"),
        ("forge-workbench", "forge-workbench / 锻造工坊"),
        ("asset-yard", "asset-yard / 道具庭院"),
        ("zbj-market-gate", "bounty-market-gate / 悬赏集市门"),
        ("league-coliseum", "league-coliseum / League 竞技场"),
        ("cn-shanghai-core", "global-start-zone / 全球首发区"),
        ("dense", "dense / 高密度"),
        ("regional", "regional / 区域密度"),
        ("route_task", "route task / 路线任务"),
        ("contract_capture", "contract capture / 契约登记"),
        ("work_order", "quest commission / 冒险委托"),
        ("delivery", "result submit / 成果提交"),
        ("acceptance", "rating pass / 评级"),
        ("rejection", "revision / 返工"),
        ("reopen", "reopen / 重开"),
        ("cancellation", "cancel / 放弃"),
        ("pending", "pending / 待推进"),
        ("completed", "completed / 已完成"),
        ("accepted", "accepted / 已评级"),
        ("customer-facing", "player-facing / 玩家可用"),
        ("customer", "client / 委托目标"),
        ("buyer", "quest taker / 接取方"),
        ("seller", "service party / 服务方"),
        ("commercial", "market quest / 市场任务"),
        ("browser commerce E2E", "browser adventure E2E / 浏览器冒险验收"),
        ("AI 设计公司", "AI Design Studio / AI 设计工坊"),
        ("服务真实客户", "serve real global clients / 完成海外真实委托"),
        ("真实客户", "real global client / 海外真实委托"),
        ("委托方", "client / 委托目标"),
    ];
    for (from, to) in replacements {
        copy = copy.replace(from, to);
    }
    copy
}

fn client_app_map_label(value: &str) -> String {
    let label = match value {
        "prefetch" => "prefetch / 预热分片",
        "street_nodes" => "street nodes / 街区节点",
        "neighbor_tile_warmup" => "neighbor warmup / 邻近地图预热",
        "warm" => "warm / 预热",
        "active" => "active / 活跃",
        "planned" => "planned / 规划中",
        "open" | "OPEN" => "open / 开放",
        "contract" => "contract / 契约",
        "venture" => "venture / 探索",
        "no-task" => "no task / 未关联任务",
        "poi" => "POI / 热点",
        "world_event" => "world event / 世界事件",
        "dense" => "dense / 高密度",
        "regional" => "regional / 区域密度",
        "Map density booting." => "Map density loading / 地图密度加载中。",
        "hub_square" => "hub square / 主城广场",
        "agent_home" => "Agent home / Agent 居所",
        "ledger_office" => "reward office / 奖励窗口",
        "workshop_room" => "workshop room / 工坊房间",
        "craft_station" => "craft station / 锻造台",
        "asset_yard" => "asset yard / 道具庭院",
        "market_gate" => "bounty gate / 悬赏入口",
        "client_board" => "quest board / 悬赏牌",
        "delivery_dock" => "rating dock / 成果评定台",
        "dispute_desk" => "dispute desk / 仲裁柜台",
        "arena_gate" => "arena gate / 竞技入口",
        "raid_hall" => "raid hall / 团本大厅",
        _ => value,
    };
    client_app_visible_copy(label)
}

fn escape_client_app_visible_text(value: &str) -> String {
    escape_html_text(&client_app_visible_copy(value))
}

fn client_app_readiness_label(value: &str) -> String {
    match value {
        "first_playable_loop_100" | "global_first_playable_loop_100" => "Global first playable 100% / 新手主线 100%".to_string(),
        "map_focus_visible" => "地图焦点可见".to_string(),
        "world_event_created" => "世界事件已创建".to_string(),
        "contract_open_or_completed" => "契约已开启或完成".to_string(),
        "quest_work_order_created" => "冒险委托已创建".to_string(),
        "quest_rating_or_feedback_loop_visible" => "评级 / 返工路线可见".to_string(),
        "wallet_progression_feed_updated" => "奖励成长动态已更新".to_string(),
        "route_task_graph_next_action_visible" => "路线下一步可见".to_string(),
        "visible" => "可见".to_string(),
        "ready" => "已准备".to_string(),
        _ => client_app_visible_copy(value),
    }
}

pub(super) async fn get_client_app_web_shell(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Html<String> {
    let web_session = authorize_league_web_session_readonly(&state, &headers, true)
        .ok()
        .flatten();
    let current_matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.as_str())
        .unwrap_or("@alice:local.dev");
    let league = state.inner.league_state.lock().await;
    let app = client_app_json(&league, current_matrix_user_id);
    let modules = app
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let module_cards = modules
        .iter()
        .map(|module| {
            let name = module.get("name").and_then(Value::as_str).unwrap_or("Module");
            let style = module.get("style").and_then(Value::as_str).unwrap_or("client");
            let summary = module.get("summary").and_then(Value::as_str).unwrap_or("ready");
            let command = module
                .get("primary_command")
                .and_then(Value::as_str)
                .unwrap_or("/app");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{}</span><p>{}</p><code>{}</code></article>",
                escape_client_app_visible_text(name),
                escape_client_app_visible_text(style),
                escape_client_app_visible_text(summary),
                escape_client_app_visible_text(command),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let message_contact_cards = app
        .get("nearby_agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .take(6)
        .map(|entity| {
            let name = entity
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Agent");
            let kind = entity
                .get("entity_kind")
                .and_then(Value::as_str)
                .unwrap_or("contact");
            let role = entity.get("role").and_then(Value::as_str).unwrap_or("协作中");
            let location = entity
                .get("location_id")
                .and_then(Value::as_str)
                .unwrap_or("mirror-city");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {}</span><p>消息、协作、契约推进与世界行动的联系人入口。</p><code>{}</code></article>",
                escape_client_app_visible_text(name),
                escape_client_app_visible_text(kind),
                escape_client_app_visible_text(role),
                escape_client_app_visible_text(location),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let message_route_cards = app
        .get("map_hub")
        .and_then(|hub| hub.get("route_task_graph"))
        .map(|graph| world_route_task_graph_views(graph, 4))
        .unwrap_or_default()
        .iter()
        .map(|task| {
            format!(
                "<article class=\"module\"><strong>任务线程</strong><span>{} · {} / {}</span><p>{}</p><code>{}</code></article>",
                escape_client_app_visible_text(&task.task_id),
                escape_client_app_visible_text(&task.latest_bucket),
                escape_client_app_visible_text(&task.latest_status),
                escape_client_app_visible_text(&task.outcome_summary),
                escape_client_app_visible_text(&task.next_opportunity_hint),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let message_cards = [message_contact_cards, message_route_cards]
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let message_cards = if message_cards.trim().is_empty() {
        "<article class=\"module\"><strong>消息</strong><span>聊天房间循环</span><p>这里会显示联系人、Agent、通知、契约线程和任务协作入口。</p><code>/social</code></article>".to_string()
    } else {
        message_cards
    };
    let me_primary_cards = modules
        .iter()
        .filter(|module| {
            matches!(
                module.get("module_id").and_then(Value::as_str),
                Some("wallet") | Some("progression")
            )
        })
        .map(|module| {
            let name = module.get("name").and_then(Value::as_str).unwrap_or("Me");
            let style = module.get("style").and_then(Value::as_str).unwrap_or("profile");
            let summary = module.get("summary").and_then(Value::as_str).unwrap_or("ready");
            let command = module
                .get("primary_command")
                .and_then(Value::as_str)
                .unwrap_or("/app");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{}</span><p>{}</p><code>{}</code></article>",
                escape_html_text(name),
                escape_html_text(style),
                escape_client_app_visible_text(summary),
                escape_client_app_visible_text(command),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let current_node = app
        .get("map")
        .and_then(|map| map.get("current_node"))
        .and_then(|node| node.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("镜像城市广场");
    let feed_surface = ClientFeedSurfaceView::from_feed_value(app.get("feed"));
    let feed_filter_chips = feed_surface.filter_chips_html();
    let feed_summary_chips = feed_surface.summary_chips_html();
    let feed_item_cards = feed_surface.item_cards_html(12);
    let feed_filter_labels_js = client_feed_filter_labels_js_object();
    let map_engine = app.get("real_world_map_engine").or_else(|| {
        app.get("map")
            .and_then(|map| map.get("real_world_map_engine"))
    });
    let map_engine_id = map_engine
        .and_then(|engine| engine.get("engine_id"))
        .and_then(Value::as_str)
        .unwrap_or("leaflet_openstreetmap_v1");
    let map_engine_name = map_engine
        .and_then(|engine| engine.get("engine"))
        .and_then(Value::as_str)
        .unwrap_or("Leaflet");
    let tile_provider = map_engine
        .and_then(|engine| engine.get("tile_provider"))
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap");
    let mirror_scope = map_engine
        .and_then(|engine| engine.get("mirror_scope"))
        .and_then(Value::as_str)
        .unwrap_or("global_real_world_tiles");
    let active_region_id = app
        .get("map_hub")
        .and_then(|hub: &Value| hub.get("viewport"))
        .and_then(|viewport: &Value| viewport.get("active_region"))
        .and_then(|region: &Value| region.get("region_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            map_engine
                .and_then(|engine| engine.get("active_region_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("cn-shanghai-core");
    let region_shard_count = map_engine
        .and_then(|engine| engine.get("region_shards"))
        .and_then(Value::as_array)
        .map(|regions| regions.len())
        .unwrap_or(0);
    let lod_layer_count = map_engine
        .and_then(|engine| engine.get("lod_layers"))
        .and_then(Value::as_array)
        .map(|layers| layers.len())
        .unwrap_or(0);
    let viewport_path_template = map_engine
        .and_then(|engine| engine.get("viewport_api"))
        .and_then(|viewport| viewport.get("path_template"))
        .and_then(Value::as_str)
        .unwrap_or("/v1/world/map/{matrix_user_id}/viewport?lat={lat}&lng={lng}&zoom={zoom}&radius_km={radius_km}&limit={limit}");
    let web_session_viewport_path_template = map_engine
        .and_then(|engine| engine.get("viewport_api"))
        .and_then(|viewport| viewport.get("web_session_path_template"))
        .and_then(Value::as_str)
        .unwrap_or("/world/web/map-viewport?lat={lat}&lng={lng}&zoom={zoom}&radius_km={radius_km}&limit={limit}");
    let map_hub = app.get("map_hub");
    let route_surface = ClientAppRouteSurfaceView::from_map_hub(map_hub);
    let map_shard_cards = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("stream_region_shards"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(4)
        .map(|region| {
            let name = region.get("name").and_then(Value::as_str).unwrap_or("Region");
            let status = region.get("status").and_then(Value::as_str).unwrap_or("planned");
            let region_id = region.get("region_id").and_then(Value::as_str).unwrap_or("region");
            let center_lat = region
                .get("center")
                .and_then(|center| center.get("lat"))
                .and_then(Value::as_f64)
                .unwrap_or(31.230416);
            let center_lng = region
                .get("center")
                .and_then(|center| center.get("lng"))
                .and_then(Value::as_f64)
                .unwrap_or(121.473701);
            let zoom_focus = region
                .get("zoom_max")
                .and_then(Value::as_i64)
                .unwrap_or(15)
                .clamp(3, 19);
            let distance_km = region
                .get("distance_km")
                .cloned()
                .unwrap_or_else(|| json!(0.0));
            let focus_button =
                map_region_focus_button_html(center_lat, center_lng, zoom_focus, "聚焦区域");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} km</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(name),
                escape_html_text(status),
                escape_html_text(&distance_km.to_string()),
                escape_html_text(region_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let map_hotspot_cards = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("poi_hotspots"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(4)
        .map(|poi| {
            let name = poi.get("name").and_then(Value::as_str).unwrap_or("POI");
            let node_kind = poi.get("node_kind").and_then(Value::as_str).unwrap_or("poi");
            let node_id = poi.get("node_id").and_then(Value::as_str).unwrap_or("node");
            let focus_button = map_node_focus_button_html(node_id, "聚焦热点");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{}</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_client_app_visible_text(name),
                escape_html_text(&client_app_map_label(node_kind)),
                escape_html_text(node_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let map_tile_cards = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("visible_tile_shards"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(6)
        .map(|tile| {
            let tile_id = tile.get("tile_id").and_then(Value::as_str).unwrap_or("tile");
            let tile_z = tile.get("z").and_then(Value::as_i64).unwrap_or(15);
            let tile_x = tile.get("x").and_then(Value::as_i64).unwrap_or(0);
            let tile_y = tile.get("y").and_then(Value::as_i64).unwrap_or(0);
            let tile_status = tile
                .get("tile_status")
                .and_then(Value::as_str)
                .unwrap_or("prefetch");
            let lod_mode = tile
                .get("lod_mode")
                .and_then(Value::as_str)
                .unwrap_or("street_nodes");
            let marker_count = tile.get("marker_count").and_then(Value::as_u64).unwrap_or(0);
            let focus_button =
                map_tile_focus_button_html(tile_z, tile_x, tile_y, "查看分片");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} · {} 个地点</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&client_app_map_label(tile_status)),
                escape_html_text(&client_app_map_label(lod_mode)),
                escape_html_text(tile.get("quadkey").and_then(Value::as_str).unwrap_or("quadkey")),
                marker_count,
                escape_html_text(tile_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let map_prefetch_cards = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("prefetch_queue"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(6)
        .map(|tile| {
            let tile_id = tile.get("tile_id").and_then(Value::as_str).unwrap_or("tile");
            let tile_z = tile.get("z").and_then(Value::as_i64).unwrap_or(15);
            let tile_x = tile.get("x").and_then(Value::as_i64).unwrap_or(0);
            let tile_y = tile.get("y").and_then(Value::as_i64).unwrap_or(0);
            let priority = tile
                .get("priority_label")
                .and_then(Value::as_str)
                .unwrap_or("warm");
            let reason = tile
                .get("prefetch_reason")
                .and_then(Value::as_str)
                .unwrap_or("neighbor_tile_warmup");
            let marker_count = tile.get("marker_count").and_then(Value::as_u64).unwrap_or(0);
            let focus_button = map_tile_focus_button_html(tile_z, tile_x, tile_y, "预热分片");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} 个地点</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&client_app_map_label(priority)),
                escape_html_text(&client_app_map_label(reason)),
                marker_count,
                escape_html_text(tile_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let map_live_event_cards = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("live_event_stream"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(6)
        .map(|event| {
            let event_kind = event
                .get("event_kind")
                .and_then(Value::as_str)
                .unwrap_or("world_event");
            let node_name = event
                .get("node_name")
                .and_then(Value::as_str)
                .unwrap_or("POI");
            let distance_km = event.get("distance_km").cloned().unwrap_or(Value::Null);
            let event_id = event
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("event");
            let node_id = event.get("node_id").and_then(Value::as_str).unwrap_or("node");
            let task_id = event
                .get("cex_task_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let location_id = event
                .get("location_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let event_body = event.get("body").and_then(Value::as_str).unwrap_or("");
            let event_result = event.get("result").and_then(Value::as_str).unwrap_or("");
            let focus_button = map_event_focus_button_html(
                node_id,
                event_id,
                task_id,
                location_id,
                &client_app_map_label(event_kind),
                &client_app_visible_copy(node_name),
                &client_app_visible_copy(event_body),
                &client_app_visible_copy(event_result),
                "追踪事件",
            );
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} km</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&client_app_map_label(event_kind)),
                escape_client_app_visible_text(node_name),
                escape_html_text(&distance_km.to_string()),
                escape_html_text(event_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let map_route_preview_cards = route_surface.preview_cards_html();
    let map_route_task_graph_cards = route_surface.task_graph_cards_html();
    let map_density_summary = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("player_density"))
        .and_then(|density| density.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("Map density booting.");
    let map_stream_region_count = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("stream_region_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_visible_marker_count = map_hub
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("marker_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_prefetch_count = map_hub
        .and_then(|hub| hub.get("prefetch_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_live_event_count = map_hub
        .and_then(|hub| hub.get("live_event_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_player_density_mode = map_hub
        .and_then(|hub| hub.get("player_density_mode"))
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let onboarding = app.get("onboarding");
    let onboarding_label = onboarding
        .and_then(|rail| rail.get("rail_label"))
        .and_then(Value::as_str)
        .unwrap_or("新手主线：从地图到悬赏完成");
    let onboarding_goal = onboarding
        .and_then(|rail| rail.get("primary_goal"))
        .and_then(Value::as_str)
        .unwrap_or("把地图焦点推进成探索、契约、委托、成果提交、评级和奖励领取。");
    let onboarding_completion_target = onboarding
        .and_then(|rail| rail.get("completion_target"))
        .and_then(Value::as_str)
        .unwrap_or("first_playable_loop_100");
    let onboarding_step_cards = onboarding
        .and_then(|rail| rail.get("steps"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|step| {
            let step_id = step.get("step_id").and_then(Value::as_str).unwrap_or("step");
            let label = step.get("label").and_then(Value::as_str).unwrap_or("Next step");
            let surface = step.get("surface").and_then(Value::as_str).unwrap_or("/app");
            let status = step.get("status").and_then(Value::as_str).unwrap_or("ready");
            let description = step
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("继续推进第一个可玩闭环。");
            let command = step.get("command").and_then(Value::as_str).unwrap_or("/app");
            let success_signal = step
                .get("success_signal")
                .and_then(Value::as_str)
                .unwrap_or("visible");
            format!(
                "<article class=\"module onboarding-step\" data-onboarding-step=\"{}\"><strong>{}</strong><span>{} · {}</span><p>{}</p><code>{}</code><p class=\"subtitle\">完成信号: <code>{}</code></p></article>",
                escape_html_text(step_id),
                escape_client_app_visible_text(label),
                escape_html_text(surface),
                escape_html_text(&client_app_readiness_label(status)),
                escape_client_app_visible_text(description),
                escape_client_app_visible_text(command),
                escape_html_text(&client_app_readiness_label(success_signal)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let onboarding_acceptance_chips = onboarding
        .and_then(|rail| rail.get("acceptance_checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|check| check.as_str().map(ToString::to_string))
        .map(|check| {
            format!(
                "<span class=\"hud-chip\"><strong>✓</strong>{}</span>",
                escape_html_text(&client_app_readiness_label(&check))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let app_data_json = serde_json::to_string(&app)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
    let current_matrix_user_id_json = serde_json::to_string(current_matrix_user_id)
        .unwrap_or_else(|_| "\"current-player\"".to_string());
    let shared_map_runtime_bootstrap_js = real_world_map_runtime_bootstrap_js();
    let shared_map_runtime_primitives_js = real_world_map_runtime_primitives_js();
    let shared_map_focus_core_js = real_world_map_focus_core_js();
    let shared_map_selection_location_ids_js = real_world_map_selection_location_ids_js();
    let shared_map_selection_builder_js = real_world_map_selection_builder_js();
    let shared_map_selection_signal_js = real_world_map_selection_signal_js();
    let shared_map_focus_panel_js = real_world_map_focus_panel_js();
    let shared_map_focus_camera_js = real_world_map_focus_camera_js();
    let shared_map_static_marker_layers_js = real_world_map_static_marker_layers_js();
    let shared_map_overlay_render_js = real_world_map_overlay_render_js();
    let shared_map_card_focus_helpers_js = real_world_map_card_focus_helpers_js();
    let shared_map_click_action_helpers_js = real_world_map_click_action_helpers_js();
    let shared_map_overlay_controls_html = map_overlay_control_buttons_html();
    let shared_map_camera_actions_html = map_camera_action_buttons_html();
    let shared_route_filter_buttons_html = route_filter_buttons_html(
        "trillionnium-app-route-filter-action",
        "Filter by Focus / 按焦点筛选路线",
        "Show Full Route / 显示完整路线",
    );
    let shared_map_route_target_resolution_js = real_world_map_route_target_resolution_js();
    let shared_map_route_status_js = real_world_map_route_status_js();
    let shared_map_route_contract_js = real_world_map_route_contract_js();
    let shared_map_route_action_js = real_world_map_route_action_js();
    let shared_map_viewport_hydration_js = real_world_map_viewport_hydration_js();
    let shared_map_render_cards_js =
        real_world_map_render_cards_js(RealWorldMapShellCardStyle::AppModule);
    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium World Mobile</title>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
  <style>
    :root {{ color-scheme: dark; --bg:#070814; --panel:#14182d; --gold:#f8c35b; --cyan:#64e3ff; --text:#f6f7fb; --muted:#a6adbb; }}
    body {{ margin:0; min-height:100vh; font-family:Inter, ui-sans-serif, system-ui, sans-serif; background:radial-gradient(circle at 20% 0%, #153f58, transparent 32rem), var(--bg); color:var(--text); padding-bottom:88px; }}
    header {{ position:sticky; top:0; z-index:20; padding:18px min(5vw,32px) 16px; backdrop-filter:blur(18px); background:linear-gradient(180deg, rgba(7,8,20,.96), rgba(7,8,20,.78)); border-bottom:1px solid rgba(255,255,255,.08); }}
    main {{ padding:18px min(5vw,32px) 34px; }}
    h1 {{ margin:0; font-size:clamp(28px,5.6vw,52px); letter-spacing:-.06em; }}
    .subtitle {{ color:var(--muted); max-width:850px; line-height:1.55; }}
    .grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .map-shell {{ display:grid; grid-template-columns:minmax(260px,.8fr) minmax(320px,1.2fr); gap:18px; align-items:stretch; margin-bottom:22px; }}
    .map-panel {{ border:1px solid rgba(255,255,255,.12); border-radius:26px; background:rgba(255,255,255,.07); padding:22px; box-shadow:0 20px 70px rgba(0,0,0,.35); }}
    #real-world-map {{ min-height:430px; border-radius:26px; overflow:hidden; border:1px solid rgba(100,227,255,.28); box-shadow:0 24px 90px rgba(0,0,0,.45); background:#0b1220; }}
    .badge {{ display:inline-flex; width:max-content; color:#071019; background:var(--gold); border-radius:999px; padding:5px 10px; font-weight:800; }}
    .module {{ display:grid; gap:10px; border:1px solid rgba(255,255,255,.12); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); border-radius:22px; padding:22px; box-shadow:0 20px 70px rgba(0,0,0,.35); }}
    .module strong {{ color:var(--gold); font-size:24px; }}
    .module span {{ color:var(--cyan); }}
    .map-stream-hud {{ display:flex; flex-wrap:wrap; gap:10px; margin:12px 0; }}
    .hud-chip {{ display:inline-flex; align-items:center; gap:8px; padding:8px 12px; border-radius:999px; border:1px solid rgba(100,227,255,.22); background:rgba(255,255,255,.06); color:var(--muted); }}
    .hud-chip strong {{ color:var(--gold); font-size:15px; }}
    .focus-stack {{ display:flex; flex-wrap:wrap; gap:8px; margin-top:2px; }}
    .focus-chip {{ border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.08); color:var(--text); border-radius:999px; padding:8px 10px; font-weight:700; cursor:pointer; }}
    .overlay-toggle-bar {{ display:flex; flex-wrap:wrap; gap:8px; margin:10px 0; }}
    .overlay-toggle {{ border:1px solid rgba(248,195,91,.25); background:rgba(248,195,91,.08); color:var(--text); border-radius:999px; padding:8px 10px; font-weight:700; cursor:pointer; }}
    .overlay-toggle.is-off {{ opacity:.58; background:rgba(255,255,255,.04); border-color:rgba(255,255,255,.12); color:var(--muted); }}
    .module p,.subtitle {{ color:var(--muted); }}
    .map-panel p {{ color:var(--muted); line-height:1.55; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    a {{ color:var(--gold); }}
    .app-mobile-shell {{ display:grid; gap:18px; }}
    .app-topbar-meta {{ display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; }}
    .app-search-shell {{ position:relative; display:flex; gap:12px; align-items:center; }}
    .app-search-input {{ width:100%; border-radius:18px; border:1px solid rgba(255,255,255,.12); background:rgba(255,255,255,.08); color:var(--text); padding:14px 16px; font-size:15px; box-shadow:0 10px 30px rgba(0,0,0,.18) inset; }}
    .app-search-input::placeholder {{ color:rgba(246,247,251,.56); }}
    .system-settings select {{ width:100%; margin-top:10px; border:1px solid rgba(100,227,255,.28); background:rgba(7,8,20,.72); color:var(--text); border-radius:14px; padding:11px 12px; font-weight:850; }}
    .app-search-clear {{ flex:0 0 auto; border:1px solid rgba(248,195,91,.3); background:rgba(248,195,91,.1); color:var(--gold); border-radius:14px; padding:11px 12px; font-weight:900; cursor:pointer; }}
    .app-search-clear[hidden] {{ display:none; }}
    .app-ux-status {{ display:flex; align-items:center; gap:8px; margin-top:10px; min-height:28px; }}
    .app-ux-pill {{ display:inline-flex; align-items:center; max-width:100%; border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.08); color:var(--cyan); border-radius:999px; padding:6px 10px; font-size:12px; font-weight:900; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
    .app-ux-pill[data-state="loading"] {{ color:var(--gold); border-color:rgba(248,195,91,.32); background:rgba(248,195,91,.1); }}
    .app-ux-pill[data-state="offline"], .app-ux-pill[data-state="fallback"] {{ color:#ffb48a; border-color:rgba(255,180,138,.32); background:rgba(255,120,70,.1); }}
    .app-search-empty {{ display:none; margin-top:10px; border:1px dashed rgba(255,255,255,.16); border-radius:16px; padding:10px 12px; color:var(--muted); background:rgba(255,255,255,.04); }}
    .app-search-empty.is-visible {{ display:block; }}
    .sr-only {{ position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0; }}
    .app-beta-chip {{ display:inline-flex; align-items:center; gap:8px; border:1px solid rgba(248,195,91,.24); background:rgba(248,195,91,.1); color:var(--gold); border-radius:999px; padding:7px 11px; font-weight:900; font-size:12px; }}
    .quest-hero {{ position:relative; overflow:hidden; border-color:rgba(248,195,91,.26); background:linear-gradient(145deg,rgba(248,195,91,.16),rgba(100,227,255,.055) 44%,rgba(255,255,255,.045)); }}
    .quest-hero::after {{ content:""; position:absolute; inset:auto -18% -48% 38%; height:220px; background:radial-gradient(circle,rgba(100,227,255,.22),transparent 62%); pointer-events:none; }}
    .quest-summary {{ display:grid; gap:10px; grid-template-columns:minmax(0,1.25fr) minmax(220px,.75fr); align-items:stretch; }}
    .quest-next-card {{ border:1px solid rgba(255,255,255,.14); background:rgba(7,8,20,.38); border-radius:18px; padding:14px; }}
    .quest-next-card strong {{ display:block; color:var(--gold); font-size:15px; margin-bottom:6px; }}
    .quest-cta {{ display:inline-flex; align-items:center; justify-content:center; min-height:42px; border-radius:14px; border:1px solid rgba(248,195,91,.42); background:linear-gradient(135deg,rgba(248,195,91,.92),rgba(255,150,89,.9)); color:#071019; text-decoration:none; font-weight:950; padding:0 14px; box-shadow:0 12px 28px rgba(248,195,91,.16); }}
    .dev-details {{ margin-top:10px; color:var(--muted); }}
    .dev-details summary {{ cursor:pointer; width:max-content; border:1px solid rgba(255,255,255,.1); border-radius:999px; padding:6px 10px; background:rgba(255,255,255,.05); color:rgba(246,247,251,.72); font-size:12px; font-weight:800; }}
    .app-tab-panel {{ display:none; gap:16px; }}
    .app-tab-panel.is-active {{ display:grid; }}
    .app-tab-header {{ display:grid; gap:6px; margin-bottom:4px; }}
    .app-bottom-tabs {{ position:fixed; left:0; right:0; bottom:0; z-index:30; display:grid; grid-template-columns:repeat(4,1fr); gap:8px; padding:10px min(4vw,24px) calc(10px + env(safe-area-inset-bottom, 0px)); border-top:1px solid rgba(255,255,255,.08); background:rgba(8,10,24,.92); backdrop-filter:blur(18px); }}
    .app-bottom-tab {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.05); color:var(--muted); border-radius:16px; padding:10px 8px; font-weight:800; cursor:pointer; }}
    .app-bottom-tab.is-active {{ color:var(--text); background:rgba(100,227,255,.12); border-color:rgba(100,227,255,.32); }}
    .app-bottom-tab:focus-visible, .focus-chip:focus-visible, .overlay-toggle:focus-visible, .app-search-input:focus-visible, .app-search-clear:focus-visible {{ outline:2px solid var(--cyan); outline-offset:2px; }}
    .app-me-grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .app-search-hidden {{ display:none !important; }}
    @media (max-width: 820px) {{ .map-shell {{ grid-template-columns:1fr; }} .quest-summary {{ grid-template-columns:1fr; }} #real-world-map {{ min-height:360px; }} main {{ padding:14px 16px 34px; }} header {{ padding:14px 16px 12px; }} }}
  </style>
</head>
<body>
  <header>
    <div class="app-topbar-meta">
      <p><span class="app-beta-chip" data-i18n-en="Global-first Beta · Mobile World Shell v1" data-i18n-zh="海外市场首发 · 移动世界壳 v1">Global-first Beta · Mobile World Shell v1</span></p>
      <p><a href="/world" data-i18n-en="World" data-i18n-zh="世界">World</a> · <a href="/league" data-i18n-en="Arena" data-i18n-zh="竞技场">Arena</a></p>
    </div>
    <h1>Trillionnium World</h1>
    <p class="subtitle"><span data-i18n-en="Mobile reality-mirror adventure for overseas-first launch: search cities and agents, then use four tabs — Messages, World, Feed, Me. Current focus:" data-i18n-zh="面向海外首发的移动现实镜像冒险：搜索城市和 Agent，并使用「消息、世界、动态、我」四个页签。当前位置：">Mobile reality-mirror adventure for overseas-first launch: search cities and agents, then use four tabs — Messages, World, Feed, Me. Current focus:</span> <strong>{}</strong></p>
    <div class="app-search-shell">
      <input id="app-global-search" class="app-search-input" type="search" inputmode="search" placeholder="Search places, agents, quests / 搜索地点、联系人、任务、动态" aria-label="Global search / 全局搜索" />
      <button id="app-search-clear" class="app-search-clear" type="button" aria-label="Clear global search / 清空全局搜索" hidden>Clear / 清空</button>
    </div>
    <div id="app-ux-status" class="app-ux-status" aria-live="polite">
      <span id="app-ux-status-pill" class="app-ux-pill" data-state="ready">Adventure ready / 冒险准备完成 · World tab active / 世界页已激活</span>
      <span id="app-ux-live-status" class="sr-only">Adventure ready / 冒险体验已准备完成</span>
    </div>
    <div id="app-search-empty-state" class="app-search-empty" role="status" aria-live="polite" data-i18n-en="No results · Try another keyword or tab." data-i18n-zh="无匹配结果 · 换个关键词或切换底部 Tab。">No results · Try another keyword or tab.</div>
  </header>
  <main class="app-mobile-shell">
    <section id="app-first-playable-onboarding" class="module quest-hero" aria-label="First playable main quest rail">
      <span class="badge">Starter Quest / 新手主线</span>
      <h2>{}</h2>
      <div class="quest-summary">
        <div>
          <p class="subtitle">{} Goal / 目标：<code>{}</code></p>
          <div id="app-first-playable-checks" class="map-stream-hud">{}</div>
        </div>
        <div class="quest-next-card">
          <strong data-i18n-en="Next Action" data-i18n-zh="下一步行动">Next Action</strong>
          <p class="subtitle" data-i18n-en="Choose a map focus in World, accept a quest card, submit results, then finish rating." data-i18n-zh="先在世界页选择地图焦点，再接取任务牌、提交成果并完成评级。">Choose a map focus in World, accept a quest card, submit results, then finish rating.</p>
          <a class="quest-cta" href="/world" data-i18n-en="Open World Console" data-i18n-zh="进入世界行动台">Open World Console</a>
        </div>
      </div>
      <section id="app-first-playable-steps" class="grid">{}</section>
    </section>
    <section id="app-tab-messages" class="app-tab-panel" data-app-panel="messages" role="tabpanel" aria-labelledby="app-tab-button-messages" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2 data-i18n-en="Messages" data-i18n-zh="消息">Messages</h2>
        <p class="subtitle" data-i18n-en="Hub for teammates, Agents, notifications, and quest threads." data-i18n-zh="队友、Agent 协作、提示与主线/支线线程。">Hub for teammates, Agents, notifications, and quest threads.</p>
      </div>
      <section id="app-message-cards" class="grid">{}</section>
    </section>
    <section id="app-tab-map" class="app-tab-panel is-active" data-app-panel="map" role="tabpanel" aria-labelledby="app-tab-button-map" aria-hidden="false">
      <div class="app-tab-header">
        <h2 data-i18n-en="World" data-i18n-zh="世界">World</h2>
        <p class="subtitle" data-i18n-en="Main stage for exploration, routes, events, and actions." data-i18n-zh="探索、路线、事件和行动都从这里展开。">Main stage for exploration, routes, events, and actions.</p>
      </div>
      <section class="map-shell" aria-label="现实镜像地图">
      <div class="map-panel">
        <span class="badge">Reality Mirror Map / 现实镜像地图</span>
        <h2>Global Launch Zone · 海外首发探索路线</h2>
        <p>Start from a real-world map for global/overseas players：nearby places, live events, quest cards, and collaborative Agents become adventure routes. 普通玩家只需要选焦点、接委托、提交成果、拿评级；底层地图引擎和接口细节已经收进调试信息。</p>
        <details class="dev-details"><summary>调试信息</summary>
          <p><strong>Real-world map engine</strong>: <code>{}</code> + <code>{}</code></p>
          <p><strong>Mirror</strong>: <code>{}</code> · <strong>Active Region</strong>: <code>{}</code> · <strong>Shards</strong>: {} · <strong>LOD Layers</strong>: {}</p>
          <p><strong>Viewport API</strong>: <code>{}</code></p>
          <p><strong>Web Viewport</strong>: <code>{}</code></p>
        </details>
        <p><code>{}</code></p>
        <p><strong>Map Main Entry / 地图主入口</strong>: start with nearby places, events, and bounties / 先看附近地点、事件和悬赏，再进入其他模块。</p>
        <p id="app-map-density-summary" class="subtitle">{}</p>
        <p id="app-map-camera-summary" class="subtitle">镜头加载中…</p>
        <div id="app-map-stream-hud" class="map-stream-hud">
          <span class="hud-chip"><strong>{}</strong> 个区域分片</span>
          <span class="hud-chip"><strong>{}</strong> 个可见地点</span>
          <span class="hud-chip"><strong>{}</strong> 个预热地图块</span>
          <span class="hud-chip"><strong>{}</strong> 个实时事件 · {}</span>
        </div>
        <div id="app-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
        </div>
        <div id="app-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
        </div>
        <p id="app-map-overlay-status" class="subtitle">当前图层：密度、区域、地图块、预热圈、实时事件。</p>
        <div class="module" style="margin-top:14px; padding:16px 18px;">
          <strong>Map Action Rail / 地图行动栏</strong>
          <span id="app-map-focus-summary">Waiting for map focus / 等待选择地图焦点…</span>
          <p id="app-map-focus-detail">Select a region, place, or event to create the next action / 选择区域、地点或事件，把地图变成下一步行动。</p>
          <div id="app-map-action-rail" class="focus-stack"></div>
        </div>
        <div class="module" style="margin-top:14px; padding:16px 18px;">
          <strong>Adventure Route / 冒险路线</strong>
          <span id="app-map-route-status">Adventure route / 冒险路线：waiting for map focus / 等待选择地图焦点…</span>
          <p id="app-map-route-next-step-status">Recommended next step / 推荐下一步：choose a map focus first / 先选择地图焦点。</p>
          <p id="app-map-route-event-brief-status">Event brief / 事件简报：waiting for event / 等待选择事件。</p>
          <p id="app-map-route-link-status">Linked task route / 关联任务路线：none yet / 暂无。</p>
          <div id="app-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <div id="app-map-route-actions" class="focus-stack"></div>
        </div>
        <p id="app-map-overlay-legend" class="subtitle">图层说明：区域锚点 · 活跃地图块 · 预热探索圈 · 实时事件脉冲。</p>
      </div>
      <div id="real-world-map" data-engine="{}" data-provider="{}" aria-label="现实镜像地图"></div>
    </section>
    <section>
      <h2>Map Tiles / 地图分片</h2>
      <section id="app-tile-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Regional Hubs / 区域据点</h2>
      <section id="app-region-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Nearby Hotspots / 附近热点</h2>
      <section id="app-poi-hotspots-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Prefetch Rings / 预热探索圈</h2>
      <section id="app-prefetch-queue-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Live Events / 实时事件</h2>
      <section id="app-live-events-live" class="grid">{}</section>
    </section>
    </section>
    <section id="app-tab-feed" class="app-tab-panel" data-app-panel="feed" role="tabpanel" aria-labelledby="app-tab-button-feed" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2 data-i18n-en="Feed" data-i18n-zh="动态">Feed</h2>
        <p class="subtitle" data-i18n-en="Discovery feed for city events, commissions, battle reports, adventure updates, and social posts." data-i18n-zh="城市事件、委托、战报、冒险动态与社交更新。">Discovery feed for city events, commissions, battle reports, adventure updates, and social posts.</p>
        <details class="dev-details"><summary>动态同步调试</summary><p><strong>Feed API</strong>: <code>{}</code></p><p><strong>Web Feed</strong>: <code>{}</code></p></details>
        <p id="app-feed-api-status" class="subtitle">动态加载中 · 当前区域 <code>{}</code> · 已准备 {} 条动态。</p>
      </div>
      <div id="app-feed-filter-actions" class="focus-stack">{}</div>
      <div id="app-feed-summary" class="map-stream-hud">{}</div>
      <section>
        <h2>World Activity Timeline / 世界动态时间线</h2>
        <section id="app-feed-items-live" class="grid">{}</section>
      </section>
    <section>
      <h2>Adventure Route Preview / 冒险路线预览</h2>
      <section id="app-route-preview-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Quest Route Graph / 任务路线图</h2>
      <section id="app-route-task-graph-live" class="grid">{}</section>
    </section>
    </section>
    <section id="app-tab-me" class="app-tab-panel" data-app-panel="me" role="tabpanel" aria-labelledby="app-tab-button-me" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2 data-i18n-en="Me" data-i18n-zh="我">Me</h2>
        <p class="subtitle" data-i18n-en="Rewards, progression, items, settings, and system abilities." data-i18n-zh="奖励、成长、道具、设置与系统能力统一归到个人中心。">Rewards, progression, items, settings, and system abilities.</p>
      </div>
      {}
      <section class="app-me-grid">{}</section>
      <section>
        <h2>Character Modules / 角色模块</h2>
        <section class="grid">{}</section>
      </section>
    </section>
  </main>
  <nav class="app-bottom-tabs" aria-label="移动端主导航" role="tablist">
    <button id="app-tab-button-messages" type="button" class="app-bottom-tab" data-app-tab="messages" role="tab" data-i18n-en="Messages" data-i18n-zh="消息" aria-controls="app-tab-messages" aria-selected="false" tabindex="-1">Messages</button>
    <button id="app-tab-button-map" type="button" class="app-bottom-tab is-active" data-app-tab="map" role="tab" data-i18n-en="World" data-i18n-zh="世界" aria-controls="app-tab-map" aria-selected="true" tabindex="0">World</button>
    <button id="app-tab-button-feed" type="button" class="app-bottom-tab" data-app-tab="feed" role="tab" data-i18n-en="Feed" data-i18n-zh="动态" aria-controls="app-tab-feed" aria-selected="false" tabindex="-1">Feed</button>
    <button id="app-tab-button-me" type="button" class="app-bottom-tab" data-app-tab="me" role="tab" data-i18n-en="Me" data-i18n-zh="我" aria-controls="app-tab-me" aria-selected="false" tabindex="-1">Me</button>
  </nav>
  {}
  <script id="trillionnium-app-data" type="application/json">{}</script>
  <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
  <script>
    (function () {{
      const dataNode = document.getElementById('trillionnium-app-data');
      const target = document.getElementById('real-world-map');
      if (!dataNode || !target || !window.L) return;
      const app = JSON.parse(dataNode.textContent || '{{}}');
      const engine = app.real_world_map_engine || (app.map && app.map.real_world_map_engine) || {{}};
      const center = engine.center || {{ lat: 31.230416, lng: 121.473701 }};
      const viewportTemplate = (((engine.viewport_api || {{}}).web_session_path_template) || '/world/web/map-viewport?lat={{lat}}&lng={{lng}}&zoom={{zoom}}&radius_km={{radius_km}}&limit={{limit}}');
      const cameraSummary = document.getElementById('app-map-camera-summary');
      const densitySummary = document.getElementById('app-map-density-summary');
      const streamHud = document.getElementById('app-map-stream-hud');
      const overlayControls = document.getElementById('app-map-overlay-controls');
      const overlayStatus = document.getElementById('app-map-overlay-status');
      const focusSummary = document.getElementById('app-map-focus-summary');
      const focusDetail = document.getElementById('app-map-focus-detail');
      const actionRail = document.getElementById('app-map-action-rail');
      const routeStatus = document.getElementById('app-map-route-status');
      const routeNextStepStatus = document.getElementById('app-map-route-next-step-status');
      const routeEventBriefStatus = document.getElementById('app-map-route-event-brief-status');
      const routeLinkStatus = document.getElementById('app-map-route-link-status');
      const routeActionRail = document.getElementById('app-map-route-actions');
      const overlayLegend = document.getElementById('app-map-overlay-legend');
      const tileTarget = document.getElementById('app-tile-shards-live');
      const regionTarget = document.getElementById('app-region-shards-live');
      const poiTarget = document.getElementById('app-poi-hotspots-live');
      const prefetchTarget = document.getElementById('app-prefetch-queue-live');
      const liveEventTarget = document.getElementById('app-live-events-live');
      const feedApiStatus = document.getElementById('app-feed-api-status');
      const feedFilterTarget = document.getElementById('app-feed-filter-actions');
      const feedSummaryTarget = document.getElementById('app-feed-summary');
      const feedItemTarget = document.getElementById('app-feed-items-live');
      const routePreviewTarget = document.getElementById('app-route-preview-live');
      const routeTaskGraphTarget = document.getElementById('app-route-task-graph-live');
      const appSearchInput = document.getElementById('app-global-search');
      const appSearchClearButton = document.getElementById('app-search-clear');
      const appSearchEmptyState = document.getElementById('app-search-empty-state');
      const appUxLiveStatus = document.getElementById('app-ux-live-status');
      const appUxStatusPill = document.getElementById('app-ux-status-pill');
      const appBottomTabs = Array.from(document.querySelectorAll('[data-app-tab]'));
      const appPanels = Array.from(document.querySelectorAll('[data-app-panel]'));
      const uiLanguage = () => ((window.TrillionniumLanguage && window.TrillionniumLanguage.get && window.TrillionniumLanguage.get()) === 'zh' ? 'zh' : 'en');
      const uiText = (english, chinese) => uiLanguage() === 'zh' ? chinese : english;
      const appTabLabels = {{ messages: {{ en: 'Messages', zh: '消息' }}, map: {{ en: 'World', zh: '世界' }}, feed: {{ en: 'Feed', zh: '动态' }}, me: {{ en: 'Me', zh: '我' }} }};
      const appTabPlaceholders = {{
        messages: {{ en: 'Search teammates, Agents, quest chats', zh: '搜索队友、群组、Agent、任务对话' }},
        map: {{ en: 'Search world places, studios, quests, events', zh: '搜索世界地点、工坊、任务、事件' }},
        feed: {{ en: 'Search posts, topics, events, adventure logs', zh: '搜索动态、话题、事件、冒险记录' }},
        me: {{ en: 'Search rewards, contracts, items, settings', zh: '搜索奖励、契约、道具、设置' }},
      }};
      const appTabLabel = (tabId) => ((appTabLabels[tabId] || {{ en: tabId, zh: tabId }})[uiLanguage()] || tabId);
      const appTabPlaceholder = (tabId) => ((appTabPlaceholders[tabId] || appTabPlaceholders.map)[uiLanguage()] || appTabPlaceholders.map.en);
      let activeAppTab = 'map';
      {shared_map_runtime_bootstrap_js}

      const routePreviewItems = ((((app.map_hub || {{}}).route_preview) || {{}}).items) || [];
      const routeTaskGraphItems = ((((app.map_hub || {{}}).route_task_graph) || {{}}).tasks) || [];
      const currentMatrixUserId = {};
      const feedApiPath = ((((app.feed || {{}}).api_path) || '')) || ('/v1/client/feed/' + encodeURIComponent(currentMatrixUserId));
      const feedWebSessionPath = ((((app.feed || {{}}).web_session_path) || '')) || '/app/web/feed';
      const feedFilterLabels = {};
      let lastViewport = null;
      let lastFeed = app.feed || {{}};
      let lastSelection = null;
      let lastRouteActions = [];
      let routeFilterMode = 'selection';
      let feedFilterMode = 'all';
      let feedLoadedViaApi = false;
      let feedRequestInFlight = null;
      const announceUxStatus = (message, state = 'ready') => {{
        const text = String(message || '').trim() || uiText('Adventure ready', '冒险体验已准备完成');
        if (appUxLiveStatus) appUxLiveStatus.textContent = text;
        if (appUxStatusPill) {{
          appUxStatusPill.textContent = text;
          appUxStatusPill.dataset.state = state || 'ready';
        }}
      }};
      const updateSearchEmptyState = (query, visibleCount, totalCount) => {{
        const hasQuery = !!String(query || '').trim();
        const isEmpty = hasQuery && totalCount > 0 && visibleCount === 0;
        if (appSearchClearButton) appSearchClearButton.hidden = !hasQuery;
        if (appSearchEmptyState) {{
          appSearchEmptyState.classList.toggle('is-visible', isEmpty);
          appSearchEmptyState.textContent = isEmpty
            ? (uiLanguage() === 'zh'
                ? ('无匹配结果 · “' + String(query || '').trim() + '” 未命中当前' + appTabLabel(activeAppTab) + '页，请换个关键词或切换底部 Tab。')
                : ('No results · “' + String(query || '').trim() + '” missed the current ' + appTabLabel(activeAppTab) + ' tab. Try another keyword or tab.'))
            : uiText('No results · Try another keyword or tab.', '无匹配结果 · 换个关键词或切换底部 Tab。');
        }}
      }};
      const applyAppSearchFilter = () => {{
        const query = String((appSearchInput && appSearchInput.value) || '').trim().toLowerCase();
        const activePanel = appPanels.find((panel) => panel.dataset.appPanel === activeAppTab) || null;
        if (!activePanel) {{
          updateSearchEmptyState(query, 0, 0);
          return;
        }}
        let totalCount = 0;
        let visibleCount = 0;
        activePanel.querySelectorAll('article.module, article.mini, li.world-route-filter-item').forEach((card) => {{
          totalCount += 1;
          const text = String(card.textContent || '').toLowerCase();
          const visible = !query || text.includes(query);
          if (visible) visibleCount += 1;
          card.classList.toggle('app-search-hidden', !visible);
        }});
        updateSearchEmptyState(query, visibleCount, totalCount);
      }};
      const setActiveAppTab = (tabId) => {{
        activeAppTab = Object.prototype.hasOwnProperty.call(appTabPlaceholders, tabId) ? tabId : 'map';
        appPanels.forEach((panel) => {{
          const active = panel.dataset.appPanel === activeAppTab;
          panel.classList.toggle('is-active', active);
          panel.hidden = !active;
          panel.setAttribute('aria-hidden', String(!active));
        }});
        appBottomTabs.forEach((button) => {{
          const active = button.dataset.appTab === activeAppTab;
          button.classList.toggle('is-active', active);
          button.setAttribute('aria-selected', String(active));
          button.tabIndex = active ? 0 : -1;
        }});
        if (appSearchInput) appSearchInput.placeholder = appTabPlaceholder(activeAppTab);
        announceUxStatus(uiText('Adventure ready · ' + appTabLabel(activeAppTab) + ' tab active', '冒险准备完成 · ' + appTabLabel(activeAppTab) + '页已激活'), 'ready');
        applyAppSearchFilter();
        if (activeAppTab === 'feed') {{
          renderFeedSurface(lastFeed, lastSelection);
          loadFeedSurface('tab-open');
        }}
        if (activeAppTab === 'map') requestAnimationFrame(() => mapAdapter.invalidateSize(mapRuntime));
      }};
      const focusAppTabByOffset = (currentButton, offset) => {{
        const index = Math.max(0, appBottomTabs.indexOf(currentButton));
        const next = (index + offset + appBottomTabs.length) % appBottomTabs.length;
        const nextButton = appBottomTabs[next];
        if (!nextButton) return;
        setActiveAppTab(nextButton.dataset.appTab || 'map');
        nextButton.focus();
      }};
      const handleAppTabKeydown = (event) => {{
        if (!event || !event.currentTarget) return;
        if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {{
          event.preventDefault();
          focusAppTabByOffset(event.currentTarget, 1);
        }} else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {{
          event.preventDefault();
          focusAppTabByOffset(event.currentTarget, -1);
        }} else if (event.key === 'Home') {{
          event.preventDefault();
          const first = appBottomTabs[0];
          if (first) {{ setActiveAppTab(first.dataset.appTab || 'messages'); first.focus(); }}
        }} else if (event.key === 'End') {{
          event.preventDefault();
          const last = appBottomTabs[appBottomTabs.length - 1];
          if (last) {{ setActiveAppTab(last.dataset.appTab || 'me'); last.focus(); }}
        }}
      }};
      {shared_map_runtime_primitives_js}
      const worldHandoffKey = () => routeHandoffStorageKey();

      const writeWorldHandoff = (payload) => {{
        try {{
          if (!window.sessionStorage || !payload) return;
          const record = buildRouteHandoffRecord(payload);
          record[routeHandoffFieldName('saved_at_epoch', 'saved_at_epoch')] = Date.now();
          window.sessionStorage.setItem(worldHandoffKey(), JSON.stringify(record));
        }} catch (_error) {{}}
      }};
      const buildWorldHandoff = (nodeId, actionId) => {{
        const prepared = buildMarkerActionHandoff(markerById.get(String(nodeId)) || {{}}, nodeId, actionId);
        writeWorldHandoff(prepared.handoff);
        return prepared;
      }};
      const navigateToWorldPanel = (panelId) => {{
        window.location.href = '/world' + (panelId ? ('#' + panelId) : '');
      }};
      {shared_map_route_target_resolution_js}
      {shared_map_focus_core_js}
      {shared_map_selection_location_ids_js}

      {shared_map_selection_builder_js}
      {shared_map_selection_signal_js}
      {shared_map_focus_panel_js}
      {shared_map_route_status_js}
      {shared_map_route_contract_js}
      {shared_map_route_action_js}

      const inferAppRouteNextStep = (selection, context) => inferConfiguredRouteNextStep(selection, context, {{
        statusPrefix: '推荐下一步',
        rejectionBody: (selectionTitle, workOrderId) => selectionTitle + ': 重开委托 ' + workOrderId + '，写清返工要求、再次投入和下一次提交计划。',
        rejectionStatus: (workOrderId) => '重开委托 ' + workOrderId + '。',
        reopenBody: (selectionTitle, workOrderId) => selectionTitle + ': 重新提交委托 ' + workOrderId + '，补齐成果、证据和评级清单。',
        reopenStatus: (workOrderId) => '重新提交委托 ' + workOrderId + '。',
        deliveryBody: (selectionTitle, workOrderId) => selectionTitle + ': 评定委托 ' + workOrderId + ' 的成果，明确通过或返工原因。',
        deliveryStatus: (workOrderId) => '评定最新委托成果 ' + workOrderId + '。',
        openWorkBody: (selectionTitle, workOrderId) => selectionTitle + ': 为委托 ' + workOrderId + ' 准备成果、证据和下一步行动。',
        openWorkStatus: (workOrderId) => '提交当前委托 ' + workOrderId + '。',
        contractBody: (selectionTitle, contractId) => selectionTitle + ': 完成关联契约 ' + contractId + '，带上证据、评级标准和下一步。',
        contractStatus: (contractId) => '完成契约 ' + contractId + '。',
        listingBody: (selectionTitle, listingId) => selectionTitle + ': 接取任务牌 ' + listingId + '，定义成果、评级和风险控制。',
        listingStatus: (listingId) => '把任务牌 ' + listingId + ' 接入冒险路线。',
        defaultBody: (selectionTitle) => selectionTitle + ': 为这个地图焦点起草下一步世界行动，带上证据、风险和推进路线。',
        defaultStatus: () => '从当前焦点起草世界行动。',
      }});
      const openWorldRouteAction = (action) => {{
        const selection = buildSelectionFromFocus(lastSelection || buildDefaultFocus()) || {{}};
        const prepared = buildRouteHandoffPayload(action, selection);
        writeWorldHandoff(prepared.payload);
        navigateToWorldPanel(action.panelId || routeActionPanelId());
      }};
      const renderAppRoutePreview = (items) => {{
        if (!routePreviewTarget) return;
        const visible = items.length ? items.slice(0, 8) : routePreviewItems.slice(0, 8);
        routePreviewTarget.innerHTML = visible.map((item) => `<article class="module"><strong>${{escapeHtml(mapText(item.title || '路线项目'))}}</strong><span>${{escapeHtml(mapText(item.detail || item.route_bucket || 'route'))}}</span><p>${{escapeHtml(mapText(item.summary || item.route_status || 'waiting'))}}</p><div class="focus-stack"><code>${{escapeHtml(mapText(item.task_id || item.location_id || item.route_bucket || 'route'))}}</code></div></article>`).join('');
      }};
      const renderAppTaskGraph = (tasks) => {{
        if (!routeTaskGraphTarget) return;
        const visible = tasks.length ? tasks.slice(0, 6) : routeTaskGraphItems.slice(0, 6);
        routeTaskGraphTarget.innerHTML = visible.map((task) => {{
          const actionButtons = routeTaskGraphActionButtonsHtml(task, 'trillionnium-app-route-flow-action');
          return `<article class="module app-route-task-graph-item"><strong>${{escapeHtml(mapText(task.task_id || '任务'))}}</strong><span>${{escapeHtml(mapText(task.latest_bucket || 'event'))}} · ${{escapeHtml(mapText(task.latest_status || 'pending'))}} · 支线 ${{escapeHtml(mapText(task.next_opportunity_kind || 'contract_capture'))}}</span><p>${{escapeHtml(task.event_count ?? 0)}} 事件 · ${{escapeHtml(task.contract_count ?? 0)}} 委托 · ${{escapeHtml(task.completion_count ?? 0)}} 战报</p><p>${{escapeHtml(mapText(task.outcome_summary || '战果总结待生成。'))}}</p><p><strong>下一条支线</strong> · ${{escapeHtml(mapText(task.next_opportunity_hint || '支线提示待生成。'))}}</p><p>${{escapeHtml(mapText(task.next_opportunity_playbook || '支线打法待生成。'))}}</p><div class="focus-stack"><code>${{escapeHtml(mapText(task.next_opportunity_command || '/world action 继续推进下一步机会。'))}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
        }}).join('');
      }};

      const refreshRouteCockpit = () => {{
        const focus = lastSelection || buildDefaultFocus();
        const selection = buildSelectionFromFocus(focus);
        const routeSelection = routeFilterMode === 'all' ? null : selection;
        const locationIds = routeSelection ? resolveSelectionLocationIds(focus) : new Set();
        const selectedTaskId = String((routeSelection && routeSelection.taskId) || '').trim();
        let filteredItems = routePreviewItems;
        if (routeSelection && selectedTaskId) {{
          filteredItems = routePreviewItems.filter((item) => {{
            const itemTaskId = String(item.task_id || '').trim();
            const itemLocationId = String(item.location_id || '').trim();
            if (itemTaskId) return itemTaskId === selectedTaskId;
            return !!itemLocationId && locationIds.has(itemLocationId);
          }});
        }} else if (routeSelection && locationIds.size) {{
          filteredItems = routePreviewItems.filter((item) => !item.location_id || locationIds.has(String(item.location_id || '')));
        }}
        if (!filteredItems.length && routeSelection && locationIds.size) {{
          filteredItems = routePreviewItems.filter((item) => !item.location_id || locationIds.has(String(item.location_id || '')));
        }}
        if (!filteredItems.length) filteredItems = routePreviewItems;
        renderAppRoutePreview(filteredItems);
        let filteredTaskGraph = routeTaskGraphItems;
        if (selectedTaskId) {{
          filteredTaskGraph = routeTaskGraphItems.filter((task) => String(task.task_id || '').trim() === selectedTaskId);
        }} else if (routeSelection && locationIds.size) {{
          filteredTaskGraph = routeTaskGraphItems.filter((task) => !task.latest_location_id || locationIds.has(String(task.latest_location_id || '')));
        }}
        if (!filteredTaskGraph.length && routeSelection && locationIds.size) {{
          filteredTaskGraph = routeTaskGraphItems.filter((task) => !task.latest_location_id || locationIds.has(String(task.latest_location_id || '')));
        }}
        if (!filteredTaskGraph.length) filteredTaskGraph = routeTaskGraphItems;
        renderAppTaskGraph(filteredTaskGraph);
        const latestTaskItem = (selectedTaskId
          ? filteredItems.find((item) => String(item.task_id || '').trim() === selectedTaskId)
          : null) || filteredItems.find((item) => ['event', 'contract'].includes(String(item.route_bucket || '')) && String(item.task_id || '').trim()) || null;
        const activeTaskId = selectedTaskId || String((latestTaskItem && latestTaskItem.task_id) || '').trim();
        const linkedContractItem = filteredItems.find((item) => String(item.route_bucket || '') === 'contract' && String(item.task_id || '').trim() === activeTaskId) || filteredItems.find((item) => String(item.route_bucket || '') === 'contract') || null;
        const linkedEventItem = filteredItems.find((item) => String(item.route_bucket || '') === 'event' && String(item.task_id || '').trim() === activeTaskId) || filteredItems.find((item) => String(item.route_bucket || '') === 'event') || null;
        const latestWorkItem = filteredItems.find((item) => ['purchase', 'work_order', 'delivery', 'acceptance', 'rejection', 'reopen', 'cancellation'].includes(String(item.route_bucket || '')) && (item.work_order_id || item.listing_id)) || null;
        const workOrderId = String((latestWorkItem && latestWorkItem.work_order_id) || '').trim();
        const contractId = String((linkedContractItem && linkedContractItem.contract_id) || '').trim();
        const listingId = String((filteredItems.find((item) => String(item.route_bucket || '') === 'purchase' && item.listing_id) || {{}}).listing_id || '').trim();
        const locationId = String((((routeSelection || {{}}).locationId) || ((filteredItems[0] || {{}}).location_id) || '')).trim();
        const linkedEventCount = activeTaskId ? filteredItems.filter((item) => String(item.route_bucket || '') === 'event' && String(item.task_id || '').trim() === activeTaskId).length : 0;
        const linkedContractCount = activeTaskId ? filteredItems.filter((item) => String(item.route_bucket || '') === 'contract' && String(item.task_id || '').trim() === activeTaskId).length : 0;
        const opportunityTask = (activeTaskId
          ? filteredTaskGraph.find((task) => String(task.task_id || '').trim() === activeTaskId)
          : null) || filteredTaskGraph[0] || null;
        const opportunityAction = buildRouteOpportunityAction(opportunityTask, locationId);
        const eventSignalText = selectionEventSignalText(selection);
        const nextStep = inferAppRouteNextStep(routeSelection, {{
          locationId,
          taskId: activeTaskId,
          workOrderId,
          contractId,
          listingId,
          latestWorkBucket: String((latestWorkItem && latestWorkItem.route_bucket) || '').trim(),
        }});
        const actions = [];
        pushUniqueRouteAction(actions, nextStep);
        pushUniqueRouteAction(actions, opportunityAction);
        pushUniqueRouteAction(actions, buildDraftWorldAction(locationId, activeTaskId, buildRouteDraftBody(selection, {{}}, {{ selectionTitleFallback: '当前路线', omitContextDetails: true, leadIn: ': 继续推进 ', emptyDetail: '这条地图冒险路线', suffix: '，补齐证据、风险判断和下一步行动。' }})));
        if (activeTaskId) pushUniqueRouteAction(actions, buildTaskFollowUpAction(selection, activeTaskId, locationId, '结合关联事件/契约状态、证据、阻碍和下一步行动'));
        if (contractId) pushUniqueRouteAction(actions, buildLinkedContractRouteAction(selection, contractId, activeTaskId, locationId));
        if (linkedEventItem) pushUniqueRouteAction(actions, buildRouteEventTimelineAction('打开关联事件', {{
          locationId,
          eventId: String(linkedEventItem.event_id || '').trim(),
          eventKind: String(linkedEventItem.title || 'world_event'),
          eventBody: String(linkedEventItem.summary || '').trim(),
          eventResult: String(linkedEventItem.route_status || '').trim(),
          eventTaskId: activeTaskId,
          body: appendSelectionEventSignal(((routeSelection && routeSelection.title) || '当前路线') + ': 在下一步世界行动前复盘关联事件 ' + String(linkedEventItem.title || 'event') + '。', selection),
        }}));
        lastRouteActions = actions;
        if (routeActionRail) {{
          routeActionRail.innerHTML = actions.map((action, index) => indexedRouteActionButtonHtml(action, index)).join(' ');
        }}
        if (routeStatus) routeStatus.textContent = routeFilterMode === 'all'
          ? ('冒险路线：显示完整路线概览' + (selection ? (' · 焦点 ' + (selection.title || 'focus')) : '') + ' · ' + filteredItems.length + ' 条路线' + (activeTaskId ? (' · 任务 ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.')
          : (routeSelection ? ('冒险路线：' + (routeSelection.title || 'focus') + ' · ' + (locationId || '未知地点') + ' · ' + filteredItems.length + ' 条关联路线' + (activeTaskId ? (' · 任务 ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.') : '冒险路线：暂无地图焦点，显示最新路线。');
        if (routeNextStepStatus) routeNextStepStatus.textContent = (nextStep && nextStep.status) || '推荐下一步：从当前焦点起草世界行动。';
        if (routeEventBriefStatus) routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, routeFilterMode === 'all');
        if (routeLinkStatus) routeLinkStatus.textContent = routeLinkStatusText({{ taskId: activeTaskId, linkedEventCount, linkedContractCount, opportunityTask, emptyText: '关联任务路线：当前焦点没有关联事件/契约。' }});
      }};
      const renderFocusPanel = () => {{
        renderMapFocusPanel({{
          focusSummary,
          focusDetail,
          actionRail,
          focus: lastSelection || buildDefaultFocus(),
          emptyDetail: '选择区域、地点或事件，把地图变成下一步行动。',
          nodeButtonExtraAttrs: ' data-open-world="true"',
          onEmpty: refreshRouteCockpit,
          onRendered: () => refreshRouteCockpit(),
        }});
      }};
      const setFocusSelection = (focus) => {{
        lastSelection = focus;
        routeFilterMode = 'selection';
        if (lastViewport) {{
          renderStreamHud(lastViewport, focus);
          renderCards(liveEventTarget, filterLiveEventStream(lastViewport.live_event_stream || [], focus), 'event');
        }}
        renderFeedSurface(lastFeed, focus);
        renderFocusPanel();
      }};
      {shared_map_focus_camera_js}
      window.trillionniumApplyMarkerAction = (nodeId, actionId) => {{
        const {{ action, handoff }} = buildWorldHandoff(nodeId, actionId);
        const state = buildMarkerRouteActionState(action, handoff, nodeId);
        const moveTarget = forceRouteFieldValueById(routeMoveTargetId(), state.moveTarget || nodeId);
        if (moveTarget) moveTarget.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
        setFocusSelection({{ kind: 'node', nodeId }});
        if (focusDetail) {{
          focusDetail.textContent = '冒险行动已准备：' + state.actionLabel + ' · 在 /world 打开 ' + state.panelId + '。';
        }}
        if (cameraSummary) {{ cameraSummary.textContent = '已选择地图行动：' + state.actionLabel + ' · ' + state.command; }}
        announceUxStatus(uiText('Map action ready · ' + state.actionLabel, '地图行动已准备 · ' + state.actionLabel), 'ready');
        return handoff;
      }};
      window.trillionniumSetMoveTarget = (nodeId) => window.trillionniumApplyMarkerAction(nodeId, 'move_here');
      document.addEventListener('click', (event) => {{
        const actionButton = closestFromEvent(event, mapClickSelectors.action);
        if (handleMapActionButton(actionButton)) return;
        const overlayButton = closestFromEvent(event, mapClickSelectors.overlay);
        if (overlayButton) {{
          handleOverlayToggleButton(overlayButton);
          return;
        }}
        const selectionActionButton = closestFromEvent(event, mapClickSelectors.selection);
        if (handleSelectionActionButton(selectionActionButton, {{
          afterNodeAction: (button, handoff) => {{
            if (button.dataset.openWorld === 'true') {{
              navigateToWorldPanel(routeHandoffPanelId(handoff, routeActionPanelId()));
            }}
          }},
        }})) return;
        const feedFilterButton = closestFromEvent(event, '.trillionnium-app-feed-filter');
        if (feedFilterButton) {{
          feedFilterMode = Object.prototype.hasOwnProperty.call(feedFilterLabels, feedFilterButton.dataset.feedFilter || '')
            ? (feedFilterButton.dataset.feedFilter || 'all')
            : 'all';
          renderFeedSurface(lastFeed, lastSelection);
          applyAppSearchFilter();
          return;
        }}
        const feedActionButton = closestFromEvent(event, '.trillionnium-app-feed-action');
        if (handleRouteActionButton(feedActionButton, openWorldRouteAction, '打开动态')) return;
        const routeFilterButton = closestFromEvent(event, '.trillionnium-app-route-filter-action');
        if (routeFilterButton) {{
          routeFilterMode = routeFilterModeFromButton(routeFilterButton, 'selection');
          refreshRouteCockpit();
          return;
        }}
        const routeActionButton = closestFromEvent(event, '.trillionnium-app-route-action');
        if (handleIndexedRouteActionButton(routeActionButton, lastRouteActions, openWorldRouteAction)) return;
        const appRouteFlowButton = closestFromEvent(event, '.trillionnium-app-route-flow-action');
        if (handleRouteActionButton(appRouteFlowButton, openWorldRouteAction, '路线行动')) return;
        const cameraActionButton = closestFromEvent(event, mapClickSelectors.camera);
        if (handleMapCameraActionButton(cameraActionButton)) return;
        const focusButton = closestFromEvent(event, mapClickSelectors.focus);
        handleMapFocusButton(focusButton);
      }});
      {shared_map_static_marker_layers_js}

      {shared_map_overlay_render_js}
      {shared_map_card_focus_helpers_js}
      {shared_map_click_action_helpers_js}
      {shared_map_render_cards_js}
      const feedGroupForItem = (item) => {{
        const feedItem = item || {{}};
        const explicitGroup = String(feedItem.feed_group || '').trim();
        if (explicitGroup) return explicitGroup;
        const kind = String(feedItem.feed_kind || '').trim();
        if (['commerce_purchase', 'work_order', 'delivery'].includes(kind)) return 'commerce';
        if (kind === 'social_agent') return 'social';
        return kind || 'all';
      }};
      const buildFeedFilterCounts = (items) => {{
        const counts = {{ all: 0, live_event: 0, route_task: 0, contract: 0, completion: 0, commerce: 0, social: 0 }};
        (Array.isArray(items) ? items : []).forEach((item) => {{
          counts.all += 1;
          const group = feedGroupForItem(item);
          if (Object.prototype.hasOwnProperty.call(counts, group)) counts[group] += 1;
        }});
        return counts;
      }};
      const feedFilterButtonHtml = (key, label, count, active) => `<button type="button" class="focus-chip trillionnium-app-feed-filter${{active ? ' is-active' : ''}}" data-feed-filter="${{escapeHtml(key)}}">${{escapeHtml(label)}} · ${{escapeHtml(count)}}</button>`;
      const renderFeedFilters = (items) => {{
        if (!feedFilterTarget) return;
        const counts = buildFeedFilterCounts(items);
        feedFilterTarget.innerHTML = Object.entries(feedFilterLabels).map(([key, label]) => {{
          const count = counts[key] ?? 0;
          return feedFilterButtonHtml(key, label, count, feedFilterMode === key);
        }}).join(' ');
      }};
      const filterFeedItemsBySelection = (items, focus = lastSelection) => {{
        const source = Array.isArray(items) ? items : [];
        if (!source.length) return source;
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return source;
        const selectionTaskId = String(selection.taskId || '').trim();
        const selectionEventId = String(selection.eventId || '').trim();
        const locationIds = resolveSelectionLocationIds(focus);
        let filtered = source;
        if (selectionTaskId) {{
          filtered = source.filter((item) => String(item.task_id || '').trim() === selectionTaskId);
        }}
        if (!filtered.length && selectionEventId) {{
          filtered = source.filter((item) => String(item.event_id || '').trim() === selectionEventId);
        }}
        if (!filtered.length && locationIds.size) {{
          filtered = source.filter((item) => {{
            const locationId = String(item.location_id || '').trim();
            return locationId && locationIds.has(locationId);
          }});
        }}
        return filtered.length ? filtered : source;
      }};
      const filterFeedItemsByMode = (items) => {{
        const source = Array.isArray(items) ? items : [];
        if (!source.length || feedFilterMode === 'all') return source;
        const filtered = source.filter((item) => feedGroupForItem(item) === feedFilterMode);
        return filtered.length ? filtered : source;
      }};
      const buildFeedFocusButton = (item) => {{
        const feedItem = item || {{}};
        if (String(feedItem.focus_kind || '').trim() !== 'event') return '';
        return mapEventFocusButton({{
          node_id: feedItem.focus_node_id || '',
          event_id: feedItem.focus_event_id || '',
          cex_task_id: feedItem.focus_task_id || '',
          location_id: feedItem.focus_location_id || '',
          event_kind: feedItem.focus_event_kind || 'world_event',
          node_name: feedItem.focus_node_name || 'POI',
          body: feedItem.focus_event_body || '',
          result: feedItem.focus_event_result || '',
        }}, '去地图看');
      }};
      const buildFeedActionBase = (item) => {{
        const feedItem = item || {{}};
        const label = String(feedItem.action_label || '').trim();
        if (!label) return null;
        return buildSimpleRouteTargetAction({{
          label,
          panelId: String(feedItem.action_panel_id || routeActionPanelId()),
          inputId: String(feedItem.action_input_id || ''),
          value: String(feedItem.action_input_value || ''),
          textareaId: String(feedItem.action_textarea_id || routeActionTextareaId()),
          locationId: String(feedItem.action_location_id || ''),
          targetNodeId: String(feedItem.action_target_node_id || ''),
          taskId: String(feedItem.action_task_id || ''),
          contractId: String(feedItem.action_contract_id || ''),
          listingId: String(feedItem.action_listing_id || ''),
          workOrderId: String(feedItem.action_work_order_id || ''),
          eventId: String(feedItem.action_event_id || ''),
          eventKind: String(feedItem.action_event_kind || ''),
          eventBody: String(feedItem.action_event_body || ''),
          eventResult: String(feedItem.action_event_result || ''),
          eventTaskId: String(feedItem.action_event_task_id || ''),
          body: String(feedItem.action_body_base || ''),
        }});
      }};
      const buildFeedAction = (item, focus = lastSelection) => {{
        const action = buildFeedActionBase(item);
        if (!action) return null;
        const selection = buildSelectionFromFocus(focus) || {{}};
        return {{
          ...action,
          body: appendSelectionEventSignal(action.body || '', selection),
        }};
      }};
      const renderFeedSummary = (feed, visibleItems, focus = lastSelection) => {{
        if (!feedSummaryTarget) return;
        const payload = feed || {{}};
        const selection = buildSelectionFromFocus(focus);
        const contracts = ((((payload.snapshots || {{}}).contracts || {{}}).count) ?? 0);
        const completions = ((((payload.snapshots || {{}}).completions || {{}}).count) ?? 0);
        const purchaseCount = ((((payload.snapshots || {{}}).commerce || {{}}).purchase_count) ?? 0);
        const workOrderCount = ((((payload.snapshots || {{}}).commerce || {{}}).work_order_count) ?? 0);
        const nearbyAgents = (((((payload.snapshots || {{}}).social || {{}}).nearby_agents) || [])).length;
        const chips = [
          `<span class="hud-chip"><strong>${{escapeHtml((visibleItems || []).length)}}</strong> 条可见动态</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(payload.item_count ?? 0)}}</strong> 条总动态</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(contracts)}}</strong> 个委托 · <strong>${{escapeHtml(completions)}}</strong> 份战报</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(purchaseCount)}}</strong> 次接取 · <strong>${{escapeHtml(workOrderCount)}}</strong> 个冒险委托</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(nearbyAgents)}}</strong> 位附近角色 · ${{escapeHtml(payload.active_region_id || 'global')}}</span>`
        ];
        if (selection) {{
          chips.push(`<span class="hud-chip"><strong>focus</strong> ${{escapeHtml(selection.taskId ? ('task ' + selection.taskId) : (selection.title || selection.locationId || selection.kind || 'selection'))}}</span>`);
        }}
        feedSummaryTarget.innerHTML = chips.join('');
      }};
      const renderFeedCards = (items, focus = lastSelection) => {{
        if (!feedItemTarget) return;
        const visible = Array.isArray(items) ? items : [];
        if (!visible.length) {{
          feedItemTarget.innerHTML = '<article class="module app-feed-item"><strong>动态暂时安静</strong><span>等待新事件</span><p>切到世界选择一个实时事件，或者稍后再拉一次动态。</p><div class="focus-stack"><code>' + escapeHtml(feedApiPath) + '</code></div></article>';
          return;
        }}
        feedItemTarget.innerHTML = visible.slice(0, 14).map((item) => {{
          const kind = String(item.feed_kind || 'update');
          const group = feedGroupForItem(item);
          const detail = String(item.detail || 'feed');
          const summary = String(item.summary || '');
          const source = String(item.source || group || 'feed');
          const action = buildFeedAction(item, focus);
          const buttons = [buildFeedFocusButton(item)];
          if (action) {{
            buttons.push(routeFlowActionButtonHtml(action, 'trillionnium-app-feed-action'));
          }}
          const actionButtons = buttons.filter(Boolean).join(' ');
          return `<article class="module app-feed-item" data-feed-kind="${{escapeHtml(kind)}}" data-feed-group="${{escapeHtml(group)}}"><strong>${{escapeHtml(item.title || '动态')}}</strong><span>${{escapeHtml(detail)}}</span><p>${{escapeHtml(summary)}}</p><div class="focus-stack"><code>${{escapeHtml(source)}}</code>${{actionButtons ? ' ' + actionButtons : ''}}</div></article>`;
        }}).join('');
      }};
      const renderFeedSurface = (feed = lastFeed, focus = lastSelection) => {{
        const payload = feed || {{}};
        const sourceItems = Array.isArray(payload.items) ? payload.items : [];
        const selectionScopedItems = filterFeedItemsBySelection(sourceItems, focus);
        const visibleItems = filterFeedItemsByMode(selectionScopedItems);
        renderFeedFilters(selectionScopedItems);
        renderFeedSummary(payload, visibleItems, focus);
        renderFeedCards(visibleItems, focus);
        if (feedApiStatus) {{
          const sourceLabel = feedLoadedViaApi ? '动态已同步' : '内置动态快照';
          const visibleFeedPath = payload.web_session_path || feedWebSessionPath || payload.api_path || feedApiPath;
          feedApiStatus.textContent = sourceLabel + ' · ' + visibleFeedPath + ' · 活跃区域 ' + (payload.active_region_id || 'global') + ' · ' + String(payload.item_count ?? sourceItems.length ?? 0) + ' 条动态。';
        }}
      }};
      const loadFeedSurface = async (reason = 'manual') => {{
        const hydrationPath = feedWebSessionPath || feedApiPath;
        if (!hydrationPath || feedRequestInFlight) return feedRequestInFlight;
        feedRequestInFlight = (async () => {{
          try {{
            announceUxStatus(uiText('Loading feed · ' + reason, '动态加载中 · ' + reason), 'loading');
            const response = await fetch(hydrationPath, {{ credentials: 'same-origin' }});
            if (!response.ok) throw new Error('feed_http_' + response.status);
            const payload = await response.json();
            if (payload && typeof payload === 'object') {{
              lastFeed = payload;
              feedLoadedViaApi = true;
              renderFeedSurface(lastFeed, lastSelection);
              applyAppSearchFilter();
              if (feedApiStatus) feedApiStatus.textContent = '动态已同步 · ' + hydrationPath + ' · reason ' + reason + ' · ' + String(payload.item_count ?? 0) + ' 条动态。';
              announceUxStatus(uiText('Feed synced · ' + String(payload.item_count ?? 0) + ' items', '动态已同步 · ' + String(payload.item_count ?? 0) + ' 条'), 'ready');
            }}
          }} catch (_error) {{
            feedLoadedViaApi = false;
            renderFeedSurface(lastFeed, lastSelection);
            if (feedApiStatus) feedApiStatus.textContent = '动态备用快照 · ' + hydrationPath + ' · 使用内置快照。';
            announceUxStatus(uiText('Fallback feed snapshot · using bundled data', '动态备用快照 · 使用内置快照'), navigator.onLine === false ? 'offline' : 'fallback');
          }} finally {{
            feedRequestInFlight = null;
          }}
        }})();
        return feedRequestInFlight;
      }};
      {shared_map_viewport_hydration_js}
      let viewportTimer = null;
      const refreshViewport = () => {{
        if (viewportTimer) window.clearTimeout(viewportTimer);
        viewportTimer = window.setTimeout(async () => {{
          try {{
            const viewport = await fetchViewportSnapshot();
            if (!viewport) return;
            renderFeedSurface(lastFeed, lastSelection);
            renderFocusPanel();
            applyAppSearchFilter();
          }} catch (_error) {{}}
        }}, 180);
      }};
      mapAdapter.onViewportChange(mapRuntime, refreshViewport);
      appBottomTabs.forEach((button) => {{
        button.addEventListener('click', () => setActiveAppTab(button.dataset.appTab || 'map'));
        button.addEventListener('keydown', handleAppTabKeydown);
      }});
      if (appSearchInput) {{
        appSearchInput.addEventListener('input', applyAppSearchFilter);
        appSearchInput.addEventListener('keydown', (event) => {{
          if (event.key === 'Escape' && appSearchInput.value) {{
            appSearchInput.value = '';
            applyAppSearchFilter();
            announceUxStatus(uiText('Search cleared', '搜索已清空'), 'ready');
          }}
        }});
      }}
      if (appSearchClearButton && appSearchInput) {{
        appSearchClearButton.addEventListener('click', () => {{
          appSearchInput.value = '';
          applyAppSearchFilter();
          appSearchInput.focus();
          announceUxStatus(uiText('Search cleared', '搜索已清空'), 'ready');
        }});
      }}
      window.addEventListener('offline', () => announceUxStatus(uiText('Offline · bundled snapshot available', '离线模式 · 内置快照可用'), 'offline'));
      window.addEventListener('online', () => {{
        announceUxStatus(uiText('Online · refreshing feed', '已联网 · 刷新动态'), 'loading');
        if (activeAppTab === 'feed') loadFeedSurface('online');
      }});
      window.addEventListener('trillionnium:languagechange', () => setActiveAppTab(activeAppTab));
      refreshOverlayControls();
      renderOverlayStatus();
      renderFeedSurface(lastFeed, lastSelection);
      setActiveAppTab(activeAppTab);
      renderFocusPanel();
      refreshViewport();
      loadFeedSurface('boot');
    }})();
  </script>
</body>
</html>"#,
        escape_client_app_visible_text(current_node),
        escape_html_text(onboarding_label),
        escape_html_text(onboarding_goal),
        escape_html_text(&client_app_readiness_label(onboarding_completion_target)),
        onboarding_acceptance_chips,
        onboarding_step_cards,
        message_cards,
        escape_html_text(map_engine_name),
        escape_html_text(tile_provider),
        escape_html_text(mirror_scope),
        escape_html_text(active_region_id),
        region_shard_count,
        lod_layer_count,
        escape_html_text(viewport_path_template),
        escape_html_text(web_session_viewport_path_template),
        escape_html_text(map_engine_id),
        escape_html_text(&client_app_map_label(map_density_summary)),
        map_stream_region_count,
        map_visible_marker_count,
        map_prefetch_count,
        map_live_event_count,
        escape_html_text(&client_app_map_label(map_player_density_mode)),
        escape_html_text(map_engine_id),
        escape_html_text(tile_provider),
        map_tile_cards,
        map_shard_cards,
        map_hotspot_cards,
        map_prefetch_cards,
        map_live_event_cards,
        escape_html_text(&feed_surface.api_path),
        escape_html_text(&feed_surface.web_session_path),
        escape_html_text(&feed_surface.active_region_id),
        feed_surface.item_count,
        feed_filter_chips,
        feed_summary_chips,
        feed_item_cards,
        map_route_preview_cards,
        map_route_task_graph_cards,
        trillionnium_language_settings_html(),
        me_primary_cards,
        module_cards,
        trillionnium_language_runtime_script(),
        app_data_json,
        current_matrix_user_id_json,
        feed_filter_labels_js,
    ))
}
