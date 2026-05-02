use super::*;

fn world_user_visible_copy(value: &str) -> String {
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
        ("客户需求牌", "悬赏任务牌"),
        ("交付码头", "成果评定台"),
        ("所有 World asset 的仓库和展示院，未来可拖拽摆放。", "收纳道具、素材和展示件的庭院，后续可自由布置。"),
        ("把想法打磨成方案、商品、素材和交付包的工作台。", "把想法打磨成方案、素材、道具和成果包的工作台。"),
        ("现实任务映射成世界委托的市场门口。", "现实机会映射成世界悬赏的入口。"),
        ("像文字 MUD 的公告栏：任务、需求、报价和线索都贴在这里。", "像文字 MUD 的公告栏：任务、悬赏、提示和线索都贴在这里。"),
        ("交付、验收、拒收、返工与取消都在这里形成流水线。", "成果提交、评级、返工和放弃都在这里形成冒险路线。"),
        ("League 入口，任务可以从市场被带进竞技场评分。", "League 入口，任务可以从悬赏集市带进竞技场评级。"),
        ("生成资产 / 审稿 / 做交付包", "生成道具 / 审稿 / 做成果包"),
        ("摆放资产 / 升级工坊 / 创建公司", "摆放道具 / 升级工坊 / 创建据点"),
        ("查看资产 / 升级资产 / 挂到店铺", "查看道具 / 升级道具 / 挂到摊位"),
        ("接任务 / 上架服务 / 雇佣卖家", "接悬赏 / 发布服务 / 招募队友"),
        ("浏览需求 / 投标 / 发布服务", "浏览悬赏 / 接取挑战 / 发布服务"),
        ("提交交付 / 验收 / 发起返工/取消", "提交成果 / 评级 / 发起返工或放弃"),
        ("craft a real customer-facing studio asset with deliverable, evidence package, risk controls, operating loop, next action, and self review for browser commerce E2E.", "打造一个真实委托可用的 AI 设计工坊道具：写清成果、证据包、风险控制、行动循环、下一步和自检记录，用于 browser adventure E2E。"),
        ("browser commerce E2E", "browser adventure E2E / 浏览器冒险验收"),
        ("AI 设计公司", "AI Design Studio / AI 设计工坊"),
        ("服务真实客户", "serve real global clients / 完成海外真实委托"),
        ("route_task", "路线任务"),
        ("contract_capture", "契约登记"),
        ("work_order", "冒险委托"),
        ("delivery", "成果提交"),
        ("acceptance", "评级"),
        ("rejection", "返工"),
        ("reopen", "重开"),
        ("cancellation", "放弃"),
        ("pending", "待推进"),
        ("completed", "已完成"),
        ("accepted", "已评级"),
        ("customer-facing", "委托可用"),
        ("customer", "委托"),
        ("buyer", "接取方"),
        ("seller", "服务方"),
        ("commercial", "任务"),
        ("commerce", "集市"),
        ("governance", "治理"),
        ("builder", "建造"),
        ("competition", "竞技"),
        ("faction-city-clerks", "城市书记门"),
        ("faction-craft-union", "工坊同盟"),
        ("faction-market-guild", "集市公会"),
        ("faction-league-order", "League 教团"),
        ("public_hub", "公共枢纽"),
        ("workshop", "工坊"),
        ("real_task_gateway", "真实任务入口"),
        ("arena", "竞技场"),
        ("market", "集市"),
        ("open", "open / 开放"),
        ("OPEN", "open / 开放"),
        ("active", "active / 活跃"),
        ("warm", "warm / 预热"),
        ("planned", "planned / 规划中"),
        ("prefetch", "prefetch / 预热分片"),
        ("street_nodes", "street nodes / 街区节点"),
        ("neighbor_tile_warmup", "neighbor warmup / 邻近地图预热"),
        ("contract", "contract / 契约"),
        ("venture", "venture / 探索"),
        ("no-task", "no task / 未关联任务"),
        ("委托方", "client / 委托目标"),
        ("Map density booting.", "Map density loading / 地图密度加载中。"),
    ];
    for (from, to) in replacements {
        copy = copy.replace(from, to);
    }
    copy
}

fn escape_world_visible_text(value: &str) -> String {
    escape_html_text(&world_user_visible_copy(value))
}

fn world_node_kind_label(kind: &str) -> &str {
    match kind {
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
        _ => kind,
    }
}

fn world_map_status_label(value: &str) -> String {
    world_user_visible_copy(match value {
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
        "world_event" => "world event / 世界事件",
        "dense" => "dense / 高密度",
        "regional" => "regional / 区域密度",
        _ => value,
    })
}

pub(super) async fn get_world_web_shell(
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
    let csrf_input = web_session
        .as_ref()
        .map(|session| {
            format!(
                "<input type=\"hidden\" name=\"csrf\" value=\"{}\" />",
                escape_html_text(&session.csrf)
            )
        })
        .unwrap_or_default();
    let console_note = if web_session.is_some() {
        "已登录的世界会话：行动会绑定当前玩家并通过 CSRF 保护。"
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "本地开发世界：探索城市、打造道具、招募 Agent，并把现实机会镜像成冒险事件。"
    } else {
        "只读世界：提交行动前需要先获取签名 /league/web/session。"
    };
    let league = state.inner.league_state.lock().await;
    let world_indexes = build_world_indexes(&league.world);
    let zones: Vec<&WorldZone> = world_indexes
        .sorted_zone_ids
        .iter()
        .filter_map(|zone_id| league.world.world_zones.get(zone_id))
        .collect();
    let locations: Vec<&WorldLocation> = world_indexes
        .sorted_location_ids
        .iter()
        .filter_map(|location_id| league.world.world_locations.get(location_id))
        .collect();
    let entities: Vec<&WorldEntity> = world_indexes
        .sorted_entity_ids
        .iter()
        .filter_map(|entity_id| league.world.world_entities.get(entity_id))
        .collect();

    let zone_cards = zones
        .iter()
        .map(|zone| {
            format!(
                "<article class=\"card zone\"><div class=\"pill\">{}</div><h3>{}</h3><p>{}</p><footer><code>{}</code><span>{}</span></footer></article>",
                escape_html_text(&world_map_status_label(&zone.status)),
                escape_world_visible_text(&zone.name),
                escape_world_visible_text(&zone.theme),
                escape_html_text(&zone.zone_id),
                escape_html_text(&world_map_status_label(&zone.mirror_kind)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let location_cards = locations
        .iter()
        .map(|location| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_world_visible_text(&location.name),
                escape_world_visible_text(&location.description),
                escape_html_text(&location.location_id),
                escape_html_text(&world_map_status_label(&location.location_kind)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let location_options = locations
        .iter()
        .map(|location| {
            format!(
                "<option value=\"{}\">{} · {}</option>",
                escape_html_text(&location.location_id),
                escape_world_visible_text(&location.name),
                escape_html_text(&world_map_status_label(&location.location_kind)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let entity_cards = entities
        .iter()
        .map(|entity| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&entity.name),
                escape_html_text(&entity.role),
                escape_html_text(&entity.entity_id),
                escape_html_text(&entity.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let entity_cards = if entity_cards.is_empty() {
        "<article class=\"mini\"><strong>No Agent residents yet</strong><span>Use /world action 招募 Agent</span><code>agent</code></article>".to_string()
    } else {
        entity_cards
    };
    let current_map_node_id = league
        .world
        .world_player_positions
        .get("@alice:local.dev")
        .map(|position| position.node_id.clone())
        .filter(|node_id| league.world.world_map_nodes.contains_key(node_id))
        .unwrap_or_else(|| default_world_node_id().to_string());
    let current_map_node = league
        .world
        .world_map_nodes
        .get(&current_map_node_id)
        .or_else(|| league.world.world_map_nodes.get(default_world_node_id()));
    let map_nodes: Vec<WorldMapNode> = world_indexes
        .sorted_map_node_ids
        .iter()
        .filter_map(|node_id| league.world.world_map_nodes.get(node_id).cloned())
        .collect();
    let map_cards = map_nodes
        .iter()
        .map(|node| {
            let marker = if node.node_id == current_map_node_id {
                "📍 "
            } else {
                ""
            };
            format!(
                "<article class=\"mini map-node\"><strong>{}{}</strong><span>{} · ({},{})</span><code>{}</code><small>{}</small></article>",
                marker,
                escape_world_visible_text(&node.name),
                escape_html_text(world_node_kind_label(&node.node_kind)),
                node.x,
                node.y,
                escape_html_text(&node.node_id),
                escape_world_visible_text(&node.freedom_hooks.join(" / ")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_exit_options = current_map_node
        .map(|node| {
            let mut exits: Vec<(&String, &String)> = node.exits.iter().collect();
            exits.sort_by(|left, right| left.0.cmp(right.0));
            exits
                .into_iter()
                .map(|(direction, node_id)| {
                    format!(
                        "<option value=\"{}\">{} → {}</option>",
                        escape_html_text(direction),
                        escape_html_text(direction),
                        escape_html_text(node_id),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let current_map_summary = current_map_node
        .map(|node| {
            format!(
                "{} · {} · exits {}",
                world_user_visible_copy(&node.name),
                world_user_visible_copy(&node.description),
                node.exits.len()
            )
        })
        .unwrap_or_else(|| "地图启动中".to_string());
    let world_map = world_map_json(&league, current_matrix_user_id);
    let world_viewport = world_map_viewport_json(
        &league.world,
        current_matrix_user_id,
        None,
        None,
        None,
        None,
        None,
    );
    let real_world_map_engine = world_map
        .get("real_world_map_engine")
        .cloned()
        .unwrap_or_else(|| real_world_map_engine_json(&map_nodes, current_map_node));
    let map_engine_id = real_world_map_engine
        .get("engine_id")
        .and_then(Value::as_str)
        .unwrap_or("leaflet_openstreetmap_v1");
    let map_engine_name = real_world_map_engine
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("Leaflet");
    let tile_provider = real_world_map_engine
        .get("tile_provider")
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap");
    let mirror_scope = real_world_map_engine
        .get("mirror_scope")
        .and_then(Value::as_str)
        .unwrap_or("global_real_world_tiles");
    let full_mirror_strategy = real_world_map_engine
        .get("full_mirror_strategy")
        .and_then(Value::as_str)
        .unwrap_or("openstreetmap_global_base_with_gather_hero_tale_lod_overlay");
    let simplification_style = real_world_map_engine
        .get("simplification_style")
        .and_then(Value::as_str)
        .unwrap_or("gather_hero_tale_lod");
    let scaling_goal = real_world_map_engine
        .get("scaling_goal")
        .and_then(Value::as_str)
        .unwrap_or("many_players_via_lightweight_nodes_routes_and_region_shards");
    let viewport_path = world_viewport
        .get("viewport_path")
        .and_then(Value::as_str)
        .unwrap_or("/world/web/map-viewport");
    let web_session_viewport_path = world_viewport
        .get("web_session_viewport_path")
        .and_then(Value::as_str)
        .unwrap_or(
            "/world/web/map-viewport?lat=31.230416&lng=121.473701&zoom=15&radius_km=4.5&limit=6",
        );
    let region_shard_cards = world_viewport
        .get("stream_region_shards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|region| {
            let name = region.get("name").and_then(Value::as_str).unwrap_or("Region");
            let region_id = region.get("region_id").and_then(Value::as_str).unwrap_or("region");
            let status = region.get("status").and_then(Value::as_str).unwrap_or("planned");
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
            let coverage = region
                .get("coverage_kind")
                .and_then(Value::as_str)
                .unwrap_or("shard");
            let distance_km = region
                .get("distance_km")
                .cloned()
                .unwrap_or_else(|| json!(0.0));
            let focus_button =
                map_region_focus_button_html(center_lat, center_lng, zoom_focus, "聚焦区域");
            format!(
                "<article class=\"mini shard\"><strong>{}</strong><span>{} · {} · {} km</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(name),
                escape_html_text(status),
                escape_html_text(coverage),
                escape_html_text(&distance_km.to_string()),
                escape_html_text(region_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let lod_layer_cards = real_world_map_engine
        .get("lod_layers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|layer| {
            let name = layer.get("name").and_then(Value::as_str).unwrap_or("LOD Layer");
            let layer_id = layer.get("layer_id").and_then(Value::as_str).unwrap_or("layer");
            let render_mode = layer
                .get("render_mode")
                .and_then(Value::as_str)
                .unwrap_or("render");
            let zoom_min = layer.get("zoom_min").and_then(Value::as_i64).unwrap_or(0);
            let zoom_max = layer.get("zoom_max").and_then(Value::as_i64).unwrap_or(0);
            format!(
                "<article class=\"mini lod\"><strong>{}</strong><span>{} · z{}-z{}</span><code>{}</code></article>",
                escape_html_text(name),
                escape_html_text(render_mode),
                zoom_min,
                zoom_max,
                escape_html_text(layer_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let hotspot_cards = world_viewport
        .get("poi_hotspots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|poi| {
            let name = poi.get("name").and_then(Value::as_str).unwrap_or("POI");
            let node_id = poi.get("node_id").and_then(Value::as_str).unwrap_or("node");
            let node_kind = poi.get("node_kind").and_then(Value::as_str).unwrap_or("poi");
            let distance = poi.get("distance_km").cloned().unwrap_or_else(|| json!(0.0));
            let focus_button = map_node_focus_button_html(node_id, "聚焦热点");
            format!(
                "<article class=\"mini poi\"><strong>{}</strong><span>{} · {} km</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_world_visible_text(name),
                escape_html_text(&world_map_status_label(node_kind)),
                escape_html_text(&distance.to_string()),
                escape_html_text(node_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tile_shard_cards = world_viewport
        .get("visible_tile_shards")
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
                "<article class=\"mini tile\"><strong>{}</strong><span>{} · {} 个地点</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&world_map_status_label(tile_status)),
                escape_html_text(&world_map_status_label(lod_mode)),
                marker_count,
                escape_html_text(tile_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prefetch_cards = world_viewport
        .get("prefetch_queue")
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
                "<article class=\"mini prefetch\"><strong>{}</strong><span>{} · {} 个地点</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&world_map_status_label(priority)),
                escape_html_text(&world_map_status_label(reason)),
                marker_count,
                escape_html_text(tile_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let live_event_cards = world_viewport
        .get("live_event_stream")
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
                &world_map_status_label(event_kind),
                &world_user_visible_copy(node_name),
                &world_user_visible_copy(event_body),
                &world_user_visible_copy(event_result),
                "追踪事件",
            );
            format!(
                "<article class=\"mini event\"><strong>{}</strong><span>{} · {} km</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(&world_map_status_label(event_kind)),
                escape_world_visible_text(node_name),
                escape_html_text(&distance_km.to_string()),
                escape_html_text(event_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_density_summary = world_map_status_label(
        world_viewport
            .get("player_density")
            .and_then(|density| density.get("summary"))
            .and_then(Value::as_str)
            .unwrap_or("Map density booting."),
    );
    let map_stream_region_count = world_viewport
        .get("stream_region_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_visible_marker_count = world_viewport
        .get("marker_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_prefetch_count = world_viewport
        .get("prefetch_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_live_event_count = world_viewport
        .get("live_event_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_player_density_mode = world_map_status_label(
        world_viewport
            .get("player_density")
            .and_then(|density| density.get("mode"))
            .and_then(Value::as_str)
            .unwrap_or("dense"),
    );
    let world_map_data_json = serde_json::to_string(&world_map)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
    let latest_asset_id = world_indexes
        .latest_asset_index_for_owner(current_matrix_user_id)
        .and_then(|index| league.world.world_assets.get(index))
        .map(|asset| asset.asset_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let asset_cards = indexed_recent(
        &league.world.world_assets,
        &world_indexes.recent_asset_indices,
        8,
    )
    .map(|asset| {
            format!(
                "<article class=\"mini asset\"><strong>{}</strong><span>{} · Lv {} · 战力 {}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&asset.name),
                escape_html_text(&asset.asset_kind),
                asset.upgrade_level.max(1),
                asset.value_score,
                escape_html_text(&asset.asset_id),
                escape_html_text(&asset.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let asset_cards = if asset_cards.is_empty() {
        "<article class=\"mini asset\"><strong>还没有角色道具</strong><span>先发起一次世界行动，打造第一件可成长道具。</span><code>/world action</code></article>".to_string()
    } else {
        asset_cards
    };
    let company_cards = indexed_recent(
        &league.world.world_companies,
        &world_indexes.recent_company_indices,
        8,
    )
    .map(|company| {
            format!(
                "<article class=\"mini company\"><strong>{}</strong><span>{} · Lv {} · 工坊值 {}</span><code>{}</code><small>道具 {} · 声望 {}</small></article>",
                escape_html_text(&company.name),
                escape_html_text(&company.company_kind),
                company.level,
                company.revenue_score,
                escape_html_text(&company.company_id),
                escape_html_text(&company.asset_id),
                company.reputation_score,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let company_cards = if company_cards.is_empty() {
        "<article class=\"mini company\"><strong>还没有工坊</strong><span>用 /company latest 把道具升级成可承接委托的据点。</span><code>/company latest</code></article>".to_string()
    } else {
        company_cards
    };
    let latest_company_id = world_indexes
        .latest_company_index_for_owner(current_matrix_user_id)
        .and_then(|index| league.world.world_companies.get(index))
        .map(|company| company.company_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let shop_cards = indexed_recent(&league.world.world_shops, &world_indexes.recent_shop_indices, 8)
        .map(|shop| {
            format!(
                "<article class=\"mini shop\"><strong>{}</strong><span>{} · 任务牌 {} · 热度 {}</span><code>{}</code><small>工坊 {} · {}</small></article>",
                escape_html_text(&shop.name),
                escape_html_text(&shop.shop_kind),
                shop.listing_count,
                shop.gross_merchandise_score,
                escape_html_text(&shop.shop_id),
                escape_html_text(&shop.company_id),
                escape_html_text(&shop.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let shop_cards = if shop_cards.is_empty() {
        "<article class=\"mini shop\"><strong>还没有据点</strong><span>先建立工坊，再开启第一个公会摊位。</span><code>/company latest</code></article>".to_string()
    } else {
        shop_cards
    };
    let listing_cards = indexed_recent(
        &league.world.world_listings,
        &world_indexes.recent_listing_indices,
        8,
    )
    .map(|listing| {
            format!(
                "<article class=\"mini listing\"><strong>{}</strong><span>{} · 赏金 {} · 品质 {}</span><code>{}</code><small>据点 {} · {}</small></article>",
                escape_html_text(&listing.title),
                escape_html_text(&listing.listing_kind),
                listing.price_credits,
                listing.quality_score,
                escape_html_text(&listing.listing_id),
                escape_html_text(&listing.shop_id),
                escape_html_text(&listing.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let listing_cards = if listing_cards.is_empty() {
        "<article class=\"mini listing\"><strong>还没有任务牌</strong><span>用 /sell latest 发布一个可接取的工坊委托。</span><code>/sell latest</code></article>".to_string()
    } else {
        listing_cards
    };
    let latest_listing_id = world_indexes
        .latest_listed_listing_index()
        .and_then(|index| league.world.world_listings.get(index))
        .map(|listing| listing.listing_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let purchase_cards = indexed_recent(
        &league.world.world_purchases,
        &world_indexes.recent_purchase_indices,
        8,
    )
    .map(|purchase| {
            let location_id = world_indexes
                .company_location_id(&purchase.company_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini purchase world-route-filter-item\" data-route-bucket=\"purchase\" data-location-id=\"{}\" data-purchase-id=\"{}\" data-listing-id=\"{}\" data-company-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>接取契约</strong><span>赏金 {} · {}</span><code>{}</code><small>任务牌 {} · 托管 {} · 领取 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&purchase.purchase_id),
                escape_html_text(&purchase.listing_id),
                escape_html_text(&purchase.company_id),
                escape_html_text(&purchase.status),
                purchase.created_at_epoch,
                purchase.price_credits,
                escape_html_text(&purchase.status),
                escape_html_text(&purchase.purchase_id),
                escape_html_text(&purchase.listing_id),
                escape_html_text(purchase.ledger_status.as_deref().unwrap_or("pending")),
                escape_html_text(purchase.buyer_ledger_status.as_deref().unwrap_or("pending")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let purchase_cards = if purchase_cards.is_empty() {
        "<article class=\"mini purchase\"><strong>还没有接取记录</strong><span>接取任务牌后会开启冒险委托和奖励托管。</span><code>/buy latest</code></article>".to_string()
    } else {
        purchase_cards
    };
    let work_order_cards = indexed_recent(
        &league.world.world_work_orders,
        &world_indexes.recent_work_order_indices,
        8,
    )
    .map(|work_order| {
            let location_id = world_indexes
                .work_order_location_id(&work_order.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini work world-route-filter-item\" data-route-bucket=\"work_order\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-purchase-id=\"{}\" data-company-id=\"{}\" data-listing-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>冒险委托</strong><span>{} · 难度 {}</span><code>{}</code><small>任务牌 {} · 接取 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&work_order.work_order_id),
                escape_html_text(&work_order.purchase_id),
                escape_html_text(&work_order.company_id),
                escape_html_text(&work_order.listing_id),
                escape_html_text(&work_order.status),
                work_order.created_at_epoch,
                escape_html_text(&work_order.brief.chars().take(48).collect::<String>()),
                work_order.value_score,
                escape_html_text(&work_order.work_order_id),
                escape_html_text(&work_order.listing_id),
                escape_html_text(&work_order.purchase_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_order_cards = if work_order_cards.is_empty() {
        "<article class=\"mini work\"><strong>还没有冒险委托</strong><span>接取任务牌后，委托会进入可提交成果的路线。</span><code>/work</code></article>".to_string()
    } else {
        work_order_cards
    };
    let latest_work_order_id = world_indexes
        .latest_work_order_index_for_actor(current_matrix_user_id)
        .and_then(|index| league.world.world_work_orders.get(index))
        .map(|work| work.work_order_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let work_delivery_cards = indexed_recent(
        &league.world.world_work_deliveries,
        &world_indexes.recent_work_delivery_indices,
        8,
    )
    .map(|delivery| {
            let location_id = world_indexes
                .work_order_location_id(&delivery.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini delivery world-route-filter-item\" data-route-bucket=\"delivery\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>成果提交</strong><span>评分 {:.1} · {}</span><code>{}</code><small>委托 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&delivery.work_order_id),
                escape_html_text(&delivery.status),
                delivery.created_at_epoch,
                delivery.score,
                escape_html_text(&delivery.status),
                escape_html_text(&delivery.delivery_id),
                escape_html_text(&delivery.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_delivery_cards = if work_delivery_cards.is_empty() {
        "<article class=\"mini delivery\"><strong>还没有成果提交</strong><span>委托接取后，可以提交成果和证据包。</span><code>/work deliver latest</code></article>".to_string()
    } else {
        work_delivery_cards
    };
    let work_acceptance_cards = indexed_recent(
        &league.world.world_work_acceptances,
        &world_indexes.recent_work_acceptance_indices,
        8,
    )
    .map(|acceptance| {
            let location_id = world_indexes
                .work_order_location_id(&acceptance.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini acceptance world-route-filter-item\" data-route-bucket=\"acceptance\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>评级通过</strong><span>{} · 声望 +{}</span><code>{}</code><small>委托 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&acceptance.work_order_id),
                escape_html_text(&acceptance.status),
                acceptance.created_at_epoch,
                escape_html_text(&acceptance.status),
                acceptance.reputation_delta,
                escape_html_text(&acceptance.acceptance_id),
                escape_html_text(&acceptance.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_acceptance_cards = if work_acceptance_cards.is_empty() {
        "<article class=\"mini acceptance\"><strong>还没有评级通过</strong><span>成果达标后可完成评级并领取声望奖励。</span><code>/work accept latest</code></article>".to_string()
    } else {
        work_acceptance_cards
    };
    let work_rejection_cards = indexed_recent(
        &league.world.world_work_rejections,
        &world_indexes.recent_work_rejection_indices,
        8,
    )
    .map(|rejection| {
            let location_id = world_indexes
                .work_order_location_id(&rejection.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini rejection world-route-filter-item\" data-route-bucket=\"rejection\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>需要返工</strong><span>{} · 奖励退回 {}</span><code>{}</code><small>委托 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&rejection.work_order_id),
                escape_html_text(&rejection.status),
                rejection.created_at_epoch,
                escape_html_text(&rejection.status),
                escape_html_text(&rejection.refund_status),
                escape_html_text(&rejection.rejection_id),
                escape_html_text(&rejection.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_rejection_cards = if work_rejection_cards.is_empty() {
        "<article class=\"mini rejection\"><strong>还没有返工记录</strong><span>成果不达标时，可以标记证据缺口并退回托管奖励。</span><code>/work reject latest</code></article>".to_string()
    } else {
        work_rejection_cards
    };
    let work_reopen_cards = indexed_recent(
        &league.world.world_work_reopens,
        &world_indexes.recent_work_reopen_indices,
        8,
    )
    .map(|reopen| {
            let location_id = world_indexes
                .work_order_location_id(&reopen.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini reopen world-route-filter-item\" data-route-bucket=\"reopen\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>委托重开</strong><span>{} · 再次托管 {}</span><code>{}</code><small>委托 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&reopen.work_order_id),
                escape_html_text(&reopen.status),
                reopen.created_at_epoch,
                escape_html_text(&reopen.status),
                escape_html_text(&reopen.reserve_status),
                escape_html_text(&reopen.reopen_id),
                escape_html_text(&reopen.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_reopen_cards = if work_reopen_cards.is_empty() {
        "<article class=\"mini reopen\"><strong>还没有重开记录</strong><span>返工后可以重新托管奖励，并允许再次提交。</span><code>/work reopen latest</code></article>".to_string()
    } else {
        work_reopen_cards
    };
    let work_cancellation_cards = indexed_recent(
        &league.world.world_work_cancellations,
        &world_indexes.recent_work_cancellation_indices,
        8,
    )
    .map(|cancellation| {
            let location_id = world_indexes
                .work_order_location_id(&cancellation.work_order_id)
                .unwrap_or_default();
            format!(
                "<article class=\"mini cancellation world-route-filter-item\" data-route-bucket=\"cancellation\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>委托放弃</strong><span>{} · 奖励退回 {}</span><code>{}</code><small>委托 {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&cancellation.work_order_id),
                escape_html_text(&cancellation.status),
                cancellation.created_at_epoch,
                escape_html_text(&cancellation.status),
                escape_html_text(&cancellation.refund_status),
                escape_html_text(&cancellation.cancellation_id),
                escape_html_text(&cancellation.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_cancellation_cards = if work_cancellation_cards.is_empty() {
        "<article class=\"mini cancellation\"><strong>还没有放弃记录</strong><span>成果提交前可以放弃委托并退回托管奖励。</span><code>/work cancel latest</code></article>".to_string()
    } else {
        work_cancellation_cards
    };
    let factions: Vec<&WorldFaction> = world_indexes
        .sorted_faction_ids
        .iter()
        .filter_map(|faction_id| league.world.world_factions.get(faction_id))
        .collect();
    let faction_cards = factions
        .iter()
        .map(|faction| {
            format!(
                "<article class=\"mini faction\"><strong>{}</strong><span>{} · 声望 {}</span><code>{}</code><small>{}</small></article>",
                escape_world_visible_text(&faction.name),
                escape_html_text(&world_map_status_label(&faction.faction_kind)),
                faction.reputation_score,
                escape_world_visible_text(&faction.faction_id),
                escape_world_visible_text(&faction.zone_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let standing_cards = indexed_recent(
        &league.world.world_faction_standings,
        &world_indexes.recent_faction_standing_indices,
        8,
    )
    .map(|standing| {
            format!(
                "<article class=\"mini standing\"><strong>{}</strong><span>{} 声望 · {}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&standing.matrix_user_id),
                standing.reputation_score,
                escape_html_text(&standing.rank),
                escape_html_text(&standing.faction_id),
                escape_html_text(&standing.standing_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let standing_cards = if standing_cards.is_empty() {
        "<article class=\"mini standing\"><strong>还没有阵营声望</strong><span>完成委托和事件会逐步提升城市阵营声望。</span><code>/factions</code></article>".to_string()
    } else {
        standing_cards
    };
    let latest_contract_id = world_indexes
        .latest_contract_index_for_actor(current_matrix_user_id)
        .and_then(|index| league.world.world_contracts.get(index))
        .map(|contract| contract.contract_id.clone())
        .unwrap_or_default();
    let contract_cards = indexed_recent(
        &league.world.world_contracts,
        &world_indexes.recent_contract_indices,
        8,
    )
    .map(|contract| {
            format!(
                "<article class=\"mini contract world-route-filter-item\" data-route-bucket=\"contract\" data-location-id=\"{}\" data-contract-id=\"{}\" data-task-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · 价值 {}</span><code>{}</code><small>任务 {} · {}</small></article>",
                escape_html_text(&contract.location_id),
                escape_html_text(&contract.contract_id),
                escape_html_text(&contract.task_id),
                escape_html_text(&contract.status),
                contract.created_at_epoch,
                escape_world_visible_text(&contract.title),
                escape_world_visible_text(&contract.status),
                contract.value_score,
                escape_html_text(&contract.location_id),
                escape_html_text(&contract.task_id),
                escape_world_visible_text(contract.cex_status.as_deref().unwrap_or("created")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let contract_cards = if contract_cards.is_empty() {
        "<article class=\"mini contract\"><strong>还没有世界契约</strong><span>用 /contract 把真实机会镜像成可追踪委托。</span><code>/contract</code></article>".to_string()
    } else {
        contract_cards
    };
    let event_items = indexed_recent(
        &league.world.world_events,
        &world_indexes.recent_event_indices,
        12,
    )
    .map(|event| {
            let task_id = event.cex_task_id.as_deref().unwrap_or("");
            let focus_button = map_event_focus_button_html(
                "",
                &event.event_id,
                task_id,
                &event.location_id,
                &world_user_visible_copy(&event.event_kind),
                &event.location_id,
                &world_user_visible_copy(&event.body),
                &world_user_visible_copy(&event.result),
                "追踪事件",
            );
            format!(
                "<li id=\"world-event-timeline-item-{}\" class=\"world-route-filter-item world-event-timeline-item\" data-route-bucket=\"event\" data-location-id=\"{}\" data-event-id=\"{}\" data-task-id=\"{}\" data-route-status=\"{}\" data-event-kind=\"{}\" data-event-body=\"{}\" data-event-result=\"{}\" data-created-at=\"{}\" tabindex=\"-1\"><b>🌍 {}</b><span>{}</span><small>{} · +{} · {}</small><em>{}</em><div class=\"focus-stack\">{}</div></li>",
                escape_html_text(&event.event_id),
                escape_html_text(&event.location_id),
                escape_html_text(&event.event_id),
                escape_html_text(task_id),
                escape_world_visible_text(event.cex_status.as_deref().unwrap_or(&event.result)),
                escape_world_visible_text(&event.event_kind),
                escape_world_visible_text(&event.body),
                escape_world_visible_text(&event.result),
                event.created_at_epoch,
                escape_world_visible_text(&event.event_kind),
                escape_world_visible_text(&event.body),
                escape_html_text(&event.location_id),
                event.impact_score,
                escape_html_text(event.cex_task_id.as_deref().unwrap_or("no-task")),
                escape_world_visible_text(&event.result),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let event_items = if event_items.is_empty() {
        "<li><b>🌍 世界正在等待</b><span>从地图焦点起草第一条世界行动。</span><small>/world action</small><em>现实镜像加载中。</em></li>".to_string()
    } else {
        event_items
    };
    let world_route_task_graph_cards = world_map
        .get("route_task_graph")
        .map(|graph| world_route_task_graph_views(graph, 6))
        .unwrap_or_default()
        .into_iter()
        .map(|task| task.world_flow_card_html())
        .collect::<Vec<_>>()
        .join("\n");
    let world_route_task_graph_cards = if world_route_task_graph_cards.is_empty() {
        "<article class=\"mini task-graph\"><strong>No task-linked routes yet</strong><span>Create a world contract or task-linked event to grow the graph.</span><code>task graph</code></article>".to_string()
    } else {
        world_route_task_graph_cards
    };

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
        "trillionnium-route-filter-action",
        "Filter by Focus / 按焦点筛选路线",
        "Show All Routes / 显示全部路线",
    );
    let shared_map_route_target_resolution_js = real_world_map_route_target_resolution_js();
    let shared_map_route_status_js = real_world_map_route_status_js();
    let shared_map_route_contract_js = real_world_map_route_contract_js();
    let shared_map_route_action_js = real_world_map_route_action_js();
    let shared_map_viewport_hydration_js = real_world_map_viewport_hydration_js();
    let shared_map_render_cards_js =
        real_world_map_render_cards_js(RealWorldMapShellCardStyle::WorldMini);

    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium World</title>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
  <style>
    :root {{ color-scheme: dark; --bg:#060711; --panel:#111426; --panel2:#171b31; --gold:#f8c35b; --cyan:#64e3ff; --green:#7dff9b; --text:#f6f7fb; --muted:#9aa3b2; }}
    * {{ box-sizing:border-box; }}
    body {{ margin:0; min-height:100vh; font-family:Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:radial-gradient(circle at 22% 0%, #133f38 0, transparent 32rem), radial-gradient(circle at 90% 18%, #3b245c 0, transparent 30rem), var(--bg); color:var(--text); }}
    header {{ padding:42px min(6vw,72px) 18px; display:grid; gap:22px; grid-template-columns:1.3fr .7fr; align-items:end; }}
    h1 {{ margin:0; font-size:clamp(44px,7vw,96px); line-height:.88; letter-spacing:-.075em; }}
    h2 {{ margin:0 0 16px; letter-spacing:-.03em; }}
    .subtitle {{ color:var(--muted); font-size:18px; max-width:840px; line-height:1.55; }}
    .hero-card,.card,.panel {{ border:1px solid rgba(255,255,255,.11); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); box-shadow:0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(14px); border-radius:24px; }}
    .hero-card,.panel,.card {{ padding:24px; }}
    .stats {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(120px,1fr)); gap:14px; margin-top:22px; }}
    .stat {{ padding:18px; background:rgba(255,255,255,.06); border-radius:18px; }}
    .stat b {{ display:block; font-size:26px; color:var(--gold); }}
    main {{ padding:20px min(6vw,72px) 60px; display:grid; gap:24px; }}
    .grid {{ display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:18px; }}
    .mini-grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; }}
    .card h3 {{ margin:12px 0; font-size:24px; }}
    .card p {{ color:var(--muted); line-height:1.55; }}
    .card footer {{ display:grid; gap:8px; margin-top:18px; color:var(--gold); }}
    .pill {{ display:inline-flex; border:1px solid rgba(100,227,255,.35); color:var(--cyan); padding:5px 10px; border-radius:999px; font-size:12px; text-transform:uppercase; letter-spacing:.12em; }}
    .world-next-card {{ display:grid; gap:12px; border:1px solid rgba(248,195,91,.2); background:linear-gradient(145deg,rgba(248,195,91,.13),rgba(100,227,255,.055)); border-radius:20px; padding:18px; }}
    .world-next-card strong {{ color:var(--gold); font-size:22px; }}
    .world-adventure-steps {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; margin:12px 0; }}
    .world-adventure-step {{ border:1px solid rgba(248,195,91,.22); background:rgba(248,195,91,.075); border-radius:18px; padding:14px; display:grid; gap:6px; }}
    .world-adventure-step b {{ color:var(--gold); }}
    .world-route-drawer {{ border-color:rgba(100,227,255,.18); background:rgba(100,227,255,.045); border-radius:18px; padding:12px; }}
    .dev-details {{ margin-top:12px; color:var(--muted); }}
    .dev-details summary {{ cursor:pointer; width:max-content; border:1px solid rgba(255,255,255,.1); border-radius:999px; padding:6px 10px; background:rgba(255,255,255,.05); color:rgba(246,247,251,.72); font-size:12px; font-weight:800; }}
    .play {{ display:grid; grid-template-columns:.8fr 1.2fr; gap:18px; }}
    .map-shell {{ display:grid; grid-template-columns:minmax(320px,.9fr) minmax(360px,1.1fr); gap:18px; align-items:stretch; }}
    form {{ display:grid; gap:10px; margin:0; }}
    input,textarea,select {{ width:100%; color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(255,255,255,.14); border-radius:14px; padding:12px 14px; font:inherit; }}
    textarea {{ min-height:140px; resize:vertical; }}
    button {{ border:0; cursor:pointer; color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:12px 16px; border-radius:14px; font-weight:800; }}
    .map-stream-hud {{ display:flex; flex-wrap:wrap; gap:10px; margin:12px 0; }}
    .hud-chip {{ display:inline-flex; align-items:center; gap:8px; padding:8px 12px; border-radius:999px; border:1px solid rgba(100,227,255,.22); background:rgba(255,255,255,.06); color:var(--muted); }}
    .hud-chip strong {{ color:var(--gold); font-size:15px; }}
    .focus-stack {{ display:flex; flex-wrap:wrap; gap:8px; margin-top:2px; }}
    .focus-chip {{ border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.08); color:var(--text); border-radius:999px; padding:8px 10px; font-weight:700; cursor:pointer; }}
    .overlay-toggle-bar {{ display:flex; flex-wrap:wrap; gap:8px; margin:10px 0; }}
    .overlay-toggle {{ border:1px solid rgba(248,195,91,.25); background:rgba(248,195,91,.08); color:var(--text); border-radius:999px; padding:8px 10px; font-weight:700; cursor:pointer; }}
    .overlay-toggle.is-off {{ opacity:.58; background:rgba(255,255,255,.04); border-color:rgba(255,255,255,.12); color:var(--muted); }}
    .mini {{ display:grid; gap:7px; padding:14px; border-radius:16px; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.08); }}
    #world-real-map {{ min-height:520px; border-radius:22px; overflow:hidden; border:1px solid rgba(100,227,255,.24); box-shadow:0 24px 90px rgba(0,0,0,.42); background:#0b1220; }}
    .asset strong {{ color:var(--green); }}
    .mini span,.mini small,.timeline small,.timeline em {{ color:var(--muted); }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:.55fr 1.35fr .55fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    .timeline em {{ grid-column:1 / -1; font-style:normal; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:14px 18px; border-radius:16px; display:inline-block; font-weight:800; text-decoration:none; }}
    @media (max-width:1050px) {{ header,.play {{ grid-template-columns:1fr; }} .grid,.stats,.mini-grid,.world-adventure-steps {{ grid-template-columns:1fr; }} }}
  </style>
</head>
<body>
  <header>
    <section>
      <div class="pill">Reality Mirror Adventure / 现实镜像冒险</div>
      <h1>Trillionnium World</h1>
      <p class="subtitle">Global-first open world for overseas launch / 面向海外首发的开放世界：reality-mirror cities, studios, quest boards, League arenas, Agents, items, relationships, and free actions. 玩家看到的是探索、委托、评级和奖励。</p>
    </section>
    <aside class="hero-card">
      <strong>Next Adventure / 下一步冒险</strong>
      <p class="subtitle">Pick a map focus, then turn it into a contract, commission, submitted result, rating, and reward / 先选一个地图焦点，再推进成契约、委托、成果提交和评级奖励。This is a player action table, not an internal admin panel / 这是玩家行动台，不是内部系统面板。</p>
      <a id="world-league-link" class="cta" href="/league">Enter League Arena / 进入 League 竞技场</a>
    </aside>
  </header>
  <main>
    <section class="stats">
      <div class="stat"><span>Zones 区域</span><b>{zones}</b></div>
      <div class="stat"><span>Places 地点</span><b>{locations}</b></div>
      <div class="stat"><span>Agents 居民</span><b>{entities}</b></div>
      <div class="stat"><span>Map Points 地图点</span><b>{map_nodes}</b></div>
      <div class="stat"><span>Items 道具</span><b>{assets}</b></div>
      <div class="stat"><span>Upgrades 升级</span><b>{asset_upgrades}</b></div>
      <div class="stat"><span>Studios 工坊</span><b>{companies}</b></div>
      <div class="stat"><span>Hubs 据点</span><b>{shops}</b></div>
      <div class="stat"><span>Quest Cards 任务牌</span><b>{listings}</b></div>
      <div class="stat"><span>Accepted 已接取</span><b>{purchases}</b></div>
      <div class="stat"><span>Commissions 委托</span><b>{work_orders}</b></div>
      <div class="stat"><span>Revisions 返工</span><b>{work_rejections}</b></div>
      <div class="stat"><span>Reopens 重开</span><b>{work_reopens}</b></div>
      <div class="stat"><span>Cancels 放弃</span><b>{work_cancellations}</b></div>
      <div class="stat"><span>Factions 阵营</span><b>{factions}</b></div>
      <div class="stat"><span>Contracts 契约</span><b>{contracts}</b></div>
      <div class="stat"><span>Reports 战报</span><b>{completions}</b></div>
      <div class="stat"><span>Events 事件</span><b>{events}</b></div>
      <div class="stat"><span>Relations 关系</span><b>{relationships}</b></div>
    </section>
    <section class="panel">
      <div class="map-shell">
        <div>
          <div class="pill">Reality Mirror Map / 现实镜像地图</div>
          <h2>Global City Exploration / 海外首发城市探索</h2>
          <p class="subtitle">Start from any map focus: regions, hotspots, live events, and route tasks become the next action / 从地图焦点进入冒险：区域、热点、实时事件和任务路线会自动串成下一步行动。Players see story, places, commissions, and rewards; engine details stay in debug drawers / 玩家看到故事、地点、委托和奖励；引擎细节收进调试抽屉。</p>
          <div id="world-tile-shards-live" class="mini-grid">{tile_shard_cards}</div>
          <div id="world-region-shards-live" class="mini-grid">{region_shard_cards}</div>
          <div class="mini-grid" style="margin-top:12px">{lod_layer_cards}</div>
          <div id="world-poi-hotspots-live" class="mini-grid" style="margin-top:12px">{hotspot_cards}</div>
          <div id="world-prefetch-queue-live" class="mini-grid" style="margin-top:12px">{prefetch_cards}</div>
          <div id="world-live-events-live" class="mini-grid" style="margin-top:12px">{live_event_cards}</div>
          <details class="dev-details"><summary>Map debug / 地图调试信息</summary><p>Global Real-world Map Engine: <code>{map_engine_name}</code> + <code>{tile_provider}</code></p><p>Mirror: <code>{mirror_scope}</code> · Strategy: <code>{full_mirror_strategy}</code> · Style: <code>{simplification_style}</code> · Goal: <code>{scaling_goal}</code></p><p>Viewport API: <code>{viewport_path}</code></p><p>Web Viewport: <code>{web_session_viewport_path}</code></p></details>
          <p id="world-map-density-summary" class="subtitle">{map_density_summary}</p>
          <p id="world-map-camera-summary" class="subtitle">镜头加载中…</p>
          <div id="world-map-stream-hud" class="map-stream-hud">
            <span class="hud-chip"><strong>{map_stream_region_count}</strong> 个区域分片</span>
            <span class="hud-chip"><strong>{map_visible_marker_count}</strong> 个可见地点</span>
            <span class="hud-chip"><strong>{map_prefetch_count}</strong> 个预热地图块</span>
            <span class="hud-chip"><strong>{map_live_event_count}</strong> 个实时事件 · {map_player_density_mode}</span>
          </div>
          <div id="world-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
          </div>
          <div id="world-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
          </div>
          <p id="world-map-overlay-status" class="subtitle">Active layers / 当前图层：density, regions, tiles, prefetch rings, live events / 密度、区域、地图块、预热圈、实时事件。</p>
          <div class="mini" style="margin-top:14px;">
            <strong>Map Action Rail / 地图行动栏</strong>
            <span id="world-map-focus-summary">Waiting for map focus / 等待选择地图焦点…</span>
            <small id="world-map-focus-detail">Choose a region, tile, hotspot, or live event to drive movement and world action / 选择区域、地图块、热点或实时事件，推动移动和世界行动。</small>
            <div id="world-map-action-rail" class="focus-stack"></div>
          </div>
          <p id="world-map-route-filter-status" class="subtitle">Route filter / 路线筛选：show all world activity / 显示全部世界活动。</p>
          <div id="world-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <p id="world-map-route-flow-status" class="subtitle">Adventure route / 冒险路线：waiting for map focus / 等待地图焦点。</p>
          <p id="world-map-route-next-step-status" class="subtitle">Recommended next step / 推荐下一步：choose a map focus first / 先选择地图焦点。</p>
          <p id="world-map-route-event-brief-status" class="subtitle">Event brief / 事件简报：waiting for live-event focus / 等待实时事件焦点。</p>
          <p id="world-map-route-link-status" class="subtitle">Linked task route / 关联任务路线：none yet / 暂无。</p>
          <div id="world-map-route-flow-actions" class="focus-stack"></div>
          <p id="world-map-overlay-legend" class="subtitle">Layer legend / 图层说明：regional anchors, active tiles, prefetch rings, live-event pulses / 区域锚点、活跃地图块、预热探索圈、实时事件脉冲。</p>
        </div>
        <div id="world-real-map" data-engine="{map_engine_id}" data-provider="{tile_provider}" aria-label="现实镜像地图"></div>
      </div>
    </section>
    <section>
      <h2>World Regions / 世界区域</h2>
      <div class="grid">{zone_cards}</div>
    </section>
    <section id="world-map-move-panel" class="panel">
      <h2>Detailed World Map / 详细世界地图</h2>
      <p class="subtitle">{current_map_summary}</p>
      <div class="mini-grid">{map_cards}</div>
      <form method="post" action="/world/web/map-move" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <select id="world-map-move-target" name="target">{map_exit_options}</select>
        <button type="submit">Move Here / 移动到这里</button>
      </form>
    </section>
    <section class="play">
      <div id="world-action-console" class="panel">
        <h2>World Action Console / 世界行动台</h2>
        <p id="world-action-console-status" class="subtitle">{console_note}</p>
        <form method="post" action="/world/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
          <select id="world-action-location" name="location_id">{location_options}</select>
          <textarea id="world-action-body" name="body">Launch an AI Design Studio for overseas/global players / 我要在全球镜像城市建立 AI 设计工坊，招募 Agent，完成海外真实委托，并把关键机会转成 League 任务。</textarea>
          <button type="submit">Submit World Action / 提交世界行动</button>
        </form>
      </div>
      <div class="panel">
        <h2>World Event Timeline / 世界事件时间线</h2>
        <ul id="world-event-timeline" class="timeline">{event_items}</ul>
      </div>
    </section>
    <section class="panel">
      <h2>Places / 地点</h2>
      <div class="mini-grid">{location_cards}</div>
    </section>
    <section class="panel">
      <h2>Agent Residents / NPC · Agent 居民</h2>
      <div class="mini-grid">{entity_cards}</div>
    </section>
    <section id="world-assets-panel" class="panel">
      <h2>Character Items / 角色道具</h2>
      <div class="mini-grid">{asset_cards}</div>
      <form method="post" action="/world/web/asset" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-asset-body" name="body">Upgrade this world item / 升级这件世界道具：强化能力、证据、风险控制、行动循环和下一条支线。</textarea>
        <button type="submit">Upgrade Item / 升级道具</button>
      </form>
    </section>
    <section id="world-companies-panel" class="panel">
      <h2>Studios / Hubs · 工坊 / 据点</h2>
      <div class="mini-grid">{company_cards}</div>
      <form method="post" action="/world/web/company" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-company-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-company-body" name="body">Launch a global-facing studio hub / 用这件道具建立面向海外玩家的工坊据点：定义能力、服务对象、行动循环、证据和第一条悬赏路线。</textarea>
        <button type="submit">Launch Studio / 建立工坊</button>
      </form>
    </section>
    <section id="world-listings-panel" class="panel">
      <h2>Hubs / Quest Cards · 据点 / 任务牌</h2>
      <div class="mini-grid">{shop_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{listing_cards}</div>
      <form method="post" action="/world/web/listing" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-listing-company-id" name="company_id" value="{latest_company_id}" placeholder="自动填充或 latest" />
        <textarea id="world-listing-body" name="body">Publish a global bounty card / 发布一个海外可接取的工坊委托：写清成果、赏金逻辑、证据包、承诺、风险控制、自检和下一步行动。</textarea>
        <button type="submit">Publish Quest Card / 发布任务牌</button>
      </form>
    </section>
    <section id="world-commerce-panel" class="panel">
      <h2>Bounties / Adventure Commissions · 悬赏 / 冒险委托</h2>
      <p class="subtitle">Player loop for overseas beta / 海外 beta 玩家视角只需要三步：accept quest card → submit result → get rating and reward / 接取任务牌 → 提交成果 → 获得评级与奖励。完整路线操作仍可展开，方便 beta 验证和高级玩家调试。</p>
      <div class="world-adventure-steps">
        <div class="world-adventure-step"><b>1 · Accept / 接取任务牌</b><span>Choose a bounty and turn it into an executable commission / 选择一个悬赏，把它变成可执行的冒险委托。</span></div>
        <div class="world-adventure-step"><b>2 · Submit / 提交成果</b><span>Submit result package, evidence, risk review, and next action / 提交成果包、证据、风险复盘和下一步行动。</span></div>
        <div class="world-adventure-step"><b>3 · Rate & Reward / 评级领奖励</b><span>Pass rating to earn reputation and rewards; otherwise enter revision / 通过评级后获得声望与奖励；不通过则进入返工路线。</span></div>
      </div>
      <div id="world-purchase-cards-live" class="mini-grid">{purchase_cards}</div>
      <div id="world-work-orders-live" class="mini-grid" style="margin-top:12px">{work_order_cards}</div>
      <details class="dev-details world-route-drawer"><summary>展开完整路线操作台</summary>
        <form id="world-buy-form" method="post" action="/world/web/buy" style="margin-top:16px">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
          <input id="world-buy-listing-id" name="listing_id" value="{latest_listing_id}" placeholder="自动填充或 latest" />
          <textarea id="world-buy-body" name="body">Accept this quest card / 接取这个任务牌，开启冒险委托：确认成果、证据包、评级标准、风险控制和下一步行动。</textarea>
          <button type="submit">Accept Quest Card / 接取任务牌</button>
        </form>
      <div id="world-work-deliveries-live" class="mini-grid" style="margin-top:12px">{work_delivery_cards}</div>
      <form id="world-work-deliver-form" method="post" action="/world/web/work-deliver" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-deliver-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-deliver-body" name="body">Result package / 成果提交包：成果、证据包、评级清单、风险复盘、下一步行动和自检记录。</textarea>
        <button type="submit">Submit Result / 提交成果</button>
      </form>
      <div id="world-work-acceptances-live" class="mini-grid" style="margin-top:12px">{work_acceptance_cards}</div>
      <form id="world-work-accept-form" method="post" action="/world/web/work-accept" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-accept-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-accept-body" name="body">Rating passed / 评级通过：确认成果证据、质量备注、下一条支线和声望奖励。</textarea>
        <button type="submit">Pass Rating / 评级通过</button>
      </form>
      <div id="world-work-rejections-live" class="mini-grid" style="margin-top:12px">{work_rejection_cards}</div>
      <form id="world-work-reject-form" method="post" action="/world/web/work-reject" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reject-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reject-body" name="body">Revision required / 需要返工：记录未通过原因、证据缺口、奖励退回、返工要求和下一步行动。</textarea>
        <button type="submit">Request Revision / 要求返工</button>
      </form>
      <div id="world-work-reopens-live" class="mini-grid" style="margin-top:12px">{work_reopen_cards}</div>
      <form id="world-work-reopen-form" method="post" action="/world/web/work-reopen" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reopen-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reopen-body" name="body">Reopen commission / 重开委托：重新托管奖励，列出返工要求、证据缺口、评级标准和再次提交行动。</textarea>
        <button type="submit">Reopen Commission / 重开委托</button>
      </form>
      <div id="world-work-cancellations-live" class="mini-grid" style="margin-top:12px">{work_cancellation_cards}</div>
      <form id="world-work-cancel-form" method="post" action="/world/web/work-cancel" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-cancel-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-cancel-body" name="body">Cancel commission / 放弃委托：在成果提交前结束路线、退回托管奖励、记录原因并关闭委托。</textarea>
        <button type="submit">Cancel Commission / 放弃委托</button>
      </form>
      </details>
    </section>
    <section class="panel">
      <h2>Faction Reputation Map / 阵营声望图</h2>
      <div class="mini-grid">{faction_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{standing_cards}</div>
    </section>
    <section id="world-contracts-panel" class="panel">
      <h2>World Contracts / 世界契约</h2>
      <div id="world-contract-cards-live" class="mini-grid">{contract_cards}</div>
      <form id="world-contract-completion-form" method="post" action="/world/web/contract" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-contract-completion-id" name="contract_id" value="{latest_contract_id}" placeholder="自动填充或契约 ID" />
        <textarea id="world-contract-completion-body" name="body">World contract report / 世界契约战报：成果、证据、风险复盘、下一步和评级标准。</textarea>
        <button type="submit">Complete Contract / 完成契约</button>
      </form>
    </section>
    <section class="panel">
      <h2>Quest Route Graph / 任务路线图</h2>
      <div id="world-route-task-graph-live" class="mini-grid">{world_route_task_graph_cards}</div>
    </section>
    <section class="panel">
      <h2>Playable Commands / 可玩指令</h2>
      <p class="subtitle"><code>/world</code> <code>/world action Launch an AI Design Studio / 我要开一家 AI 设计工坊</code> <code>/league</code> <code>/arena</code> <code>/guild</code> <code>/raid</code></p>
    </section>
  </main>
  {language_runtime_script}
  <script id="trillionnium-world-map-data" type="application/json">{world_map_data_json}</script>
  <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
  <script>
    (function () {{
      const dataNode = document.getElementById('trillionnium-world-map-data');
      const target = document.getElementById('world-real-map');
      if (!dataNode || !target || !window.L) return;
      const payload = JSON.parse(dataNode.textContent || '{{}}');
      const engine = payload.real_world_map_engine || {{}};
      const center = engine.center || {{ lat: 31.230416, lng: 121.473701 }};
      const viewportTemplate = (((engine.viewport_api || {{}}).web_session_path_template) || '/world/web/map-viewport?lat={{lat}}&lng={{lng}}&zoom={{zoom}}&radius_km={{radius_km}}&limit={{limit}}');
      const cameraSummary = document.getElementById('world-map-camera-summary');
      const densitySummary = document.getElementById('world-map-density-summary');
      const streamHud = document.getElementById('world-map-stream-hud');
      const overlayControls = document.getElementById('world-map-overlay-controls');
      const overlayStatus = document.getElementById('world-map-overlay-status');
      const routeFilterStatus = document.getElementById('world-map-route-filter-status');
      const routeFlowStatus = document.getElementById('world-map-route-flow-status');
      const routeNextStepStatus = document.getElementById('world-map-route-next-step-status');
      const routeEventBriefStatus = document.getElementById('world-map-route-event-brief-status');
      const routeLinkStatus = document.getElementById('world-map-route-link-status');
      const routeFlowActions = document.getElementById('world-map-route-flow-actions');
      const actionConsoleStatus = document.getElementById('world-action-console-status');
      const focusSummary = document.getElementById('world-map-focus-summary');
      const focusDetail = document.getElementById('world-map-focus-detail');
      const actionRail = document.getElementById('world-map-action-rail');
      const overlayLegend = document.getElementById('world-map-overlay-legend');
      const tileTarget = document.getElementById('world-tile-shards-live');
      const regionTarget = document.getElementById('world-region-shards-live');
      const poiTarget = document.getElementById('world-poi-hotspots-live');
      const prefetchTarget = document.getElementById('world-prefetch-queue-live');
      const liveEventTarget = document.getElementById('world-live-events-live');
      const routeTaskGraphTarget = document.getElementById('world-route-task-graph-live');
      {shared_map_runtime_bootstrap_js}

      const routeTaskGraphItems = ((((payload.route_task_graph || {{}}).tasks) || []));
      let lastViewport = null;
      let lastSelection = null;
      let routeFilterMode = 'all';
      {shared_map_runtime_primitives_js}
      const worldHandoffKey = () => routeHandoffStorageKey();

      {shared_map_route_target_resolution_js}
      {shared_map_route_status_js}
      {shared_map_route_contract_js}
      {shared_map_route_action_js}
      const actionLocationSelect = document.getElementById(routeActionLocationId());
      const actionBodyField = document.getElementById(routeActionTextareaId());
      const defaultActionConsoleStatus = actionConsoleStatus ? actionConsoleStatus.textContent : '';
      const defaultActionBodyText = actionBodyField ? actionBodyField.value : '';
      const focusRouteTarget = (locationId, explicitNodeId, fallbackNodeId) => {{
        const nodeId = resolveRouteTargetNodeId(locationId, explicitNodeId, fallbackNodeId);
        if (!nodeId) return '';
        const focus = {{ kind: 'node', nodeId, suppressAction: true }};
        focusMapSurface(focus);
        setFocusSelection(focus);
        return nodeId;
      }};
      const focusRouteEvent = (eventId, locationId, fallbackNodeId, eventKind, eventBody, eventResult, taskId) => {{
        const normalizedEventId = String(eventId || '').trim();
        const focus = {{
          kind: 'event',
          eventId: normalizedEventId,
          taskId: String(taskId || '').trim(),
          locationId: String(locationId || '').trim(),
          nodeId: String(fallbackNodeId || '').trim(),
          eventKind: String(eventKind || 'world_event').trim(),
          eventBody: String(eventBody || '').trim(),
          eventResult: String(eventResult || '').trim(),
          suppressAction: true,
        }};
        const eventItem = findLiveEventByFocus(focus) || {{}};
        if (!focus.locationId && eventItem.location_id) focus.locationId = String(eventItem.location_id || '').trim();
        if (!focus.nodeId && eventItem.node_id) focus.nodeId = String(eventItem.node_id || '').trim();
        focusMapSurface(focus);
        setFocusSelection(focus);
        const timelineItem = Array.from(document.querySelectorAll('.world-route-filter-item[data-event-id]')).find((item) => String(item.dataset.eventId || '').trim() === normalizedEventId);
        if (timelineItem) {{
          timelineItem.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
          if (typeof timelineItem.focus === 'function') timelineItem.focus();
        }}
        return String(eventItem.event_id || normalizedEventId || '').trim();
      }};
      const consumeWorldHandoff = () => {{
        try {{
          if (!window.sessionStorage) return null;
          const raw = window.sessionStorage.getItem(worldHandoffKey());
          if (!raw) return null;
          window.sessionStorage.removeItem(worldHandoffKey());
          return JSON.parse(raw);
        }} catch (_error) {{
          return null;
        }}
      }};
      const applyWorldHandoff = (handoff) => {{
        if (!handoff) return;
        const state = buildRouteHandoffState(handoff);
        forceRouteFieldValueById(routeMoveTargetId(), state.moveTarget);
        forceRouteFieldValueById(routeActionLocationId(), state.locationId, {{ clearManual: true }});
        const actionBody = document.getElementById(routeActionTextareaId());
        forceRouteFieldValueById(state.targetInputId, state.targetValue);
        const targetTextarea = forceRouteFieldValueById(state.targetTextareaId, state.actionBody);
        if (!targetTextarea) forceRouteFieldValue(actionBody, state.actionBody);
        forceRouteFieldValueById(routePurchaseInputId(), state.listingId);
        routeWorkLaneInputIds().forEach((inputId) => forceRouteFieldValueById(inputId, state.workOrderId));
        forceRouteFieldValueById(routeContractInputId(), state.contractId);
        const handoffEventId = state.eventId
          ? focusRouteEvent(state.eventId, state.locationId || '', state.nodeId || state.moveTarget || '', state.eventKind || '', state.eventBody || '', state.eventResult || '', state.routeTaskId || '')
          : '';
        const handoffNodeId = handoffEventId ? '' : focusRouteTarget(state.locationId || '', state.nodeId, state.moveTarget || '');
        scrollRoutePanelIntoView(state.panelId);
        if (cameraSummary) {{
          cameraSummary.textContent = '世界行动交接：' + state.actionLabel + ' · ' + state.command + (handoffEventId ? (' · 事件 ' + handoffEventId) : (handoffNodeId ? (' · 焦点 ' + handoffNodeId) : '')) + (state.routeTaskId ? (' · 任务 ' + state.routeTaskId) : '');
        }}
        if (focusDetail) {{
          focusDetail.textContent = '来自 /app 的行动：' + state.actionLabel + ' · 面板 ' + state.panelId + ' · ' + state.actionBody + (handoffEventId ? (' · 事件 ' + handoffEventId) : '') + (state.routeTaskId ? (' · 任务 ' + state.routeTaskId) : '');
        }}
      }};
      const markRouteInputManual = (input) => {{
        if (!input) return;
        input.dataset.routeAutofilled = 'false';
      }};
      const maybeAutofillRouteSelect = (selectId, value) => {{
        const input = document.getElementById(selectId);
        if (!input || !value) return;
        const wasAutofilled = input.dataset.routeAutofilled === 'true';
        if (document.activeElement === input) return;
        if (input.dataset.routeManual === 'true' && !wasAutofilled) return;
        if (input.value !== value || wasAutofilled) {{
          input.value = value;
          input.dataset.routeAutofilled = 'true';
        }}
      }};
      const maybeAutofillRouteInput = (inputId, value) => {{
        const input = document.getElementById(inputId);
        if (!input || !value) return;
        const currentValue = String(input.value || '').trim();
        const wasAutofilled = input.dataset.routeAutofilled === 'true';
        if (document.activeElement === input) return;
        if (!currentValue || currentValue === 'latest' || wasAutofilled) {{
          input.value = value;
          input.dataset.routeAutofilled = 'true';
        }}
      }};
      const maybeAutofillRouteTextarea = (textAreaId, value) => {{
        const input = document.getElementById(textAreaId);
        if (!input || !value) return;
        const currentValue = String(input.value || '').trim();
        const wasAutofilled = input.dataset.routeAutofilled === 'true';
        if (document.activeElement === input) return;
        if (!currentValue || currentValue === String(defaultActionBodyText || '').trim() || wasAutofilled) {{
          input.value = value;
          input.dataset.routeAutofilled = 'true';
        }}
      }};
      const applyRouteActionDraft = (locationId, body, force) => {{
        if (locationId) {{
          if (force && actionLocationSelect) {{
            forceRouteFieldValue(actionLocationSelect, locationId, {{ clearManual: true }});
          }} else {{
            maybeAutofillRouteSelect(routeActionLocationId(), locationId);
          }}
        }}
        if (body) {{
          if (force && actionBodyField) {{
            forceRouteFieldValue(actionBodyField, body);
          }} else {{
            maybeAutofillRouteTextarea(routeActionTextareaId(), body);
          }}
        }}
      }};
      const openRouteFlowAction = (action) => {{
        const target = action || {{}};
        if (target.inputId && target.value) maybeAutofillRouteInput(target.inputId, target.value);
        if (target.locationId) applyRouteActionDraft(target.locationId, '', true);
        const selection = buildSelectionFromFocus(lastSelection || buildDefaultFocus()) || {{}};
        const effectiveEventId = String(target.eventId || ((selection.kind === 'event' && selection.eventId) ? selection.eventId : '') || '').trim();
        const focusedEventId = effectiveEventId
          ? focusRouteEvent(effectiveEventId, target.locationId || selection.locationId || '', target.targetNodeId || selection.nodeId || '', target.eventKind || selection.eventKind || '', target.eventBody || selection.eventBody || '', target.eventResult || selection.eventResult || '', target.eventTaskId || target.taskId || selection.taskId || '')
          : '';
        const focusedNodeId = focusedEventId ? '' : focusRouteTarget(target.locationId, target.targetNodeId, '');
        const bodyTargetId = target.textareaId || routeActionTextareaId();
        if (target.body) {{
          const bodyTarget = forceRouteFieldValueById(bodyTargetId, target.body);
          if (!bodyTarget) {{
            maybeAutofillRouteTextarea(bodyTargetId, target.body);
          }}
          if (bodyTargetId === routeActionTextareaId()) applyRouteActionDraft(target.locationId, target.body, true);
        }}
        const input = target.inputId ? document.getElementById(target.inputId) : null;
        const textarea = target.body ? document.getElementById(bodyTargetId) : null;
        const panel = document.getElementById(target.panelId);
        if (panel) panel.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
        if (cameraSummary && (focusedEventId || focusedNodeId)) {{
          cameraSummary.textContent = '路线焦点已准备：' + (focusedEventId ? ('事件 ' + focusedEventId) : focusedNodeId) + ' · 面板 ' + (target.panelId || routeActionPanelId());
        }}
        const finalFocusTarget = input || textarea || (target.locationId ? document.getElementById(routeActionLocationId()) : null);
        if (finalFocusTarget) {{
          finalFocusTarget.focus();
          if (typeof finalFocusTarget.select === 'function') finalFocusTarget.select();
        }} else if (input) {{
          input.focus();
          if (typeof input.select === 'function') input.select();
        }}
      }};
      const buildRouteActionDraft = (selection, routeContext) => buildRouteDraftBody(selection, routeContext, {{ selectionTitleFallback: '当前世界路线' }});
      const renderWorldTaskGraph = (tasks) => {{
        if (!routeTaskGraphTarget) return;
        const visible = (tasks && tasks.length ? tasks : routeTaskGraphItems).slice(0, 6);
        routeTaskGraphTarget.innerHTML = visible.length ? visible.map((task) => {{
          const actionButtons = routeTaskGraphActionButtonsHtml(task);
          return `<article class="mini task-graph"><strong>${{escapeHtml(mapText(task.task_id || '任务'))}}</strong><span>${{escapeHtml(mapText(task.latest_bucket || 'event'))}} · ${{escapeHtml(mapText(task.latest_status || 'pending'))}} · 支线 ${{escapeHtml(mapText(task.next_opportunity_kind || 'contract_capture'))}}</span><code>${{escapeHtml(mapText(task.latest_location_id || task.task_id || 'route'))}}</code><small>${{escapeHtml(task.event_count ?? 0)}} 事件 · ${{escapeHtml(task.contract_count ?? 0)}} 契约 · ${{escapeHtml(task.completion_count ?? 0)}} 战报</small><small>${{escapeHtml(mapText(task.outcome_summary || '战果总结待生成。'))}}</small><small><strong>下一条支线</strong> · ${{escapeHtml(mapText(task.next_opportunity_hint || '支线提示待生成。'))}}</small><div class="focus-stack"><code>${{escapeHtml(mapText(task.next_opportunity_command || '/world action 继续推进下一步机会。'))}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
        }}).join('') : '<article class="mini task-graph"><strong>还没有任务路线</strong><span>创建世界契约或任务事件后，路线图会生长出来。</span><code>task graph</code></article>';
      }};
      const findLatestVisibleByBuckets = (visibleItems, buckets) => {{
        const bucketSet = new Set(buckets);
        return [...visibleItems]
          .filter((item) => bucketSet.has(item.dataset.routeBucket || ''))
          .sort((left, right) => Number(right.dataset.createdAt || 0) - Number(left.dataset.createdAt || 0))[0] || null;
      }};
      const findLatestVisibleByBucketAndTask = (visibleItems, bucket, taskId) => {{
        if (!taskId) return null;
        return [...visibleItems]
          .filter((item) => (item.dataset.routeBucket || '') === bucket && String(item.dataset.taskId || '').trim() === taskId)
          .sort((left, right) => Number(right.dataset.createdAt || 0) - Number(left.dataset.createdAt || 0))[0] || null;
      }};
      const inferRouteNextStep = (selection, routeContext) => inferConfiguredRouteNextStep(selection, routeContext, {{
        statusPrefix: '推荐下一步',
        rejectionBody: (selectionTitle, workOrderId) => selectionTitle + ': 重开委托 ' + workOrderId + '，写清返工要求、证据缺口、再次托管和下一次提交。',
        rejectionStatus: (workOrderId) => '为当前路线重开委托 ' + workOrderId + '。',
        reopenBody: (selectionTitle, workOrderId) => selectionTitle + ': 重新提交委托 ' + workOrderId + '，带上修订成果、证据包、评级清单和风险复盘。',
        reopenStatus: (workOrderId) => '重开后重新提交委托 ' + workOrderId + '。',
        deliveryBody: (selectionTitle, workOrderId) => selectionTitle + ': 评定委托 ' + workOrderId + ' 的成果，确认证据和质量，再给出通过或返工的下一步。',
        deliveryStatus: (workOrderId) => '评定最新委托成果 ' + workOrderId + '。',
        openWorkBody: (selectionTitle, workOrderId) => selectionTitle + ': 为委托 ' + workOrderId + ' 准备成果、证据、评级清单和下一步行动。',
        openWorkStatus: (workOrderId) => '提交当前委托 ' + workOrderId + '。',
        closedWorkLabel: '起草后续行动',
        closedWorkBody: (selectionTitle, workOrderId, latestWorkBucket) => selectionTitle + ': 在 ' + latestWorkBucket + ' 后跟进委托 ' + workOrderId + '，记录结果、下一条支线和世界状态变化。',
        closedWorkStatus: (workOrderId, latestWorkBucket) => '在 ' + latestWorkBucket + ' 后起草委托 ' + workOrderId + ' 的后续世界行动。',
        contractBody: (selectionTitle, contractId) => selectionTitle + ': 完成契约 ' + contractId + '，带上成果、证据、风险复盘、评级标准和下一步。',
        contractStatus: (contractId) => '完成当前路线的契约 ' + contractId + '。',
        listingBody: (selectionTitle, listingId) => selectionTitle + ': 接取任务牌 ' + listingId + '，定义成果、证据、评级、风险控制和下一步行动。',
        listingStatus: (listingId) => '把任务牌 ' + listingId + ' 接入冒险路线。',
        defaultBody: (selectionTitle) => selectionTitle + ': 从当前路线起草下一步世界行动，包含证据、风险和推进动作。',
        defaultStatus: () => '为当前路线起草世界行动。',
      }});
      {shared_map_selection_location_ids_js}
      const applyRouteFilters = () => {{
        const items = Array.from(document.querySelectorAll('.world-route-filter-item'));
        if (!items.length) return;
        const focus = lastSelection || buildDefaultFocus();
        const selection = buildSelectionFromFocus(focus);
        const selectedTaskId = String((selection && selection.taskId) || '').trim();
        const locationIds = routeFilterMode === 'all' ? new Set() : resolveSelectionLocationIds(focus);
        const counts = {{}};
        items.forEach((item) => {{
          const locationId = String(item.dataset.locationId || '');
          const itemTaskId = String(item.dataset.taskId || '').trim();
          const show = routeFilterMode === 'all'
            || !selection
            || (!selectedTaskId && !locationIds.size)
            || (selectedTaskId
              ? (itemTaskId ? itemTaskId === selectedTaskId : (!locationId || locationIds.has(locationId)))
              : (!locationId || locationIds.has(locationId)));
          item.hidden = !show;
          if (show) {{
            const bucket = item.dataset.routeBucket || 'item';
            counts[bucket] = (counts[bucket] || 0) + 1;
          }}
        }});
        if (!routeFilterStatus) return;
        if (routeFilterMode === 'all' || !selection || (!selectedTaskId && !locationIds.size)) {{
          routeFilterStatus.textContent = '路线筛选：显示全部世界活动。';
        }} else {{
          const workflowCount = (counts.purchase || 0) + (counts.work_order || 0) + (counts.delivery || 0) + (counts.acceptance || 0) + (counts.rejection || 0) + (counts.reopen || 0) + (counts.cancellation || 0);
          routeFilterStatus.textContent = '路线筛选：' + (selection.title || '焦点') + (selectedTaskId ? (' · 任务 ' + selectedTaskId) : '') + ' · ' + (counts.event || 0) + ' 事件 · ' + (counts.contract || 0) + ' 契约 · ' + workflowCount + ' 个冒险环节。';
        }}
        applyRouteFlow();
      }};
      const applyRouteFlow = () => {{
        const visibleItems = Array.from(document.querySelectorAll('.world-route-filter-item')).filter((item) => !item.hidden);
        const firstVisible = (bucket) => visibleItems.find((item) => item.dataset.routeBucket === bucket) || null;
        const firstVisibleWith = (fieldName) => visibleItems.find((item) => String(item.dataset[fieldName] || '').trim()) || null;
        const selection = buildSelectionFromFocus(lastSelection || buildDefaultFocus());
        const selectedTaskId = String((selection && selection.taskId) || '').trim();
        const eventItem = firstVisible('event');
        const contractItem = firstVisible('contract');
        const purchaseItem = firstVisible('purchase');
        const workItem = firstVisible('work_order') || firstVisibleWith('workOrderId');
        const latestWorkItem = findLatestVisibleByBuckets(visibleItems, ['purchase', 'work_order', 'delivery', 'acceptance', 'rejection', 'reopen', 'cancellation']);
        const latestTaskItem = findLatestVisibleByBuckets(visibleItems, ['event', 'contract']);
        const locationItem = firstVisibleWith('locationId');
        const workOrderId = String((workItem && workItem.dataset.workOrderId) || '').trim();
        const activeTaskId = selectedTaskId || String((latestTaskItem && latestTaskItem.dataset.taskId) || '').trim();
        const linkedContractItem = findLatestVisibleByBucketAndTask(visibleItems, 'contract', activeTaskId);
        const linkedEventItem = findLatestVisibleByBucketAndTask(visibleItems, 'event', activeTaskId);
        const linkedEventCount = activeTaskId ? visibleItems.filter((item) => (item.dataset.routeBucket || '') === 'event' && String(item.dataset.taskId || '').trim() === activeTaskId).length : 0;
        const linkedContractCount = activeTaskId ? visibleItems.filter((item) => (item.dataset.routeBucket || '') === 'contract' && String(item.dataset.taskId || '').trim() === activeTaskId).length : 0;
        const locationId = String((((selection || {{}}).locationId) || ((locationItem && locationItem.dataset.locationId) || ''))).trim();
        let filteredTaskGraph = routeTaskGraphItems;
        if (activeTaskId) {{
          filteredTaskGraph = routeTaskGraphItems.filter((task) => String(task.task_id || '').trim() === activeTaskId);
        }} else if (locationId) {{
          filteredTaskGraph = routeTaskGraphItems.filter((task) => !task.latest_location_id || String(task.latest_location_id || '').trim() === locationId);
        }}
        if (!filteredTaskGraph.length) filteredTaskGraph = routeTaskGraphItems;
        renderWorldTaskGraph(filteredTaskGraph);
        const opportunityTask = (activeTaskId
          ? filteredTaskGraph.find((task) => String(task.task_id || '').trim() === activeTaskId)
          : null) || filteredTaskGraph[0] || null;
        const opportunityAction = buildRouteOpportunityAction(opportunityTask, locationId);
        const eventSignalText = selectionEventSignalText(selection);
        const contractId = String(((linkedContractItem && linkedContractItem.dataset.contractId) || (contractItem && contractItem.dataset.contractId) || '')).trim();
        const listingId = String((purchaseItem && purchaseItem.dataset.listingId) || '').trim();
        const eventLabel = (selection && selection.kind === 'event' && selection.title)
          ? selection.title
          : ((linkedEventItem || eventItem) ? String((((linkedEventItem || eventItem).querySelector('b')) || {{}}).textContent || 'event').replace(/^🌍\s*/, '') : 'no event');
        const nextStep = inferRouteNextStep(selection, {{
          locationId,
          taskId: activeTaskId,
          eventLabel,
          workOrderId,
          contractId,
          listingId,
          latestWorkBucket: String((latestWorkItem && latestWorkItem.dataset.routeBucket) || '').trim(),
        }});
        const draftBody = buildRouteActionDraft(selection, {{ locationId, taskId: activeTaskId, eventLabel, workOrderId, contractId, listingId, recommendedLabel: (nextStep || {{}}).label || '' }});
        if (workOrderId) {{
          routeWorkLaneInputIds().forEach((inputId) => maybeAutofillRouteInput(inputId, workOrderId));
        }}
        if (contractId) maybeAutofillRouteInput(routeContractInputId(), contractId);
        if (listingId) maybeAutofillRouteInput(routePurchaseInputId(), listingId);
        if (routeFilterMode !== 'all' && selection) applyRouteActionDraft(locationId, draftBody, false);
        if (routeFlowActions) {{
          const actions = [];
          const actionKeys = new Set();
          if (nextStep) pushRouteFlowActionButton(actions, actionKeys, nextStep);
          if (opportunityAction) pushRouteFlowActionButton(actions, actionKeys, opportunityAction);
          if (locationId || draftBody) {{
            pushRouteFlowActionButton(actions, actionKeys, buildDraftWorldAction(locationId, activeTaskId, draftBody));
          }}
          if (activeTaskId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildTaskFollowUpAction(selection, activeTaskId, locationId, '结合关联事件/契约、当前世界状态证据、风险和下一步行动'));
          }}
          if (eventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction('打开事件线', {{
              eventId: String(eventItem.dataset.eventId || ''),
              eventKind: String(eventItem.dataset.eventKind || 'world_event'),
              eventBody: String(eventItem.dataset.eventBody || ''),
              eventResult: String(eventItem.dataset.eventResult || ''),
              eventTaskId: String(eventItem.dataset.taskId || ''),
              locationId: String(eventItem.dataset.locationId || ''),
            }}));
          }}
          if (activeTaskId && linkedEventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction('打开关联事件', {{
              eventId: String(linkedEventItem.dataset.eventId || ''),
              eventKind: String(linkedEventItem.dataset.eventKind || 'world_event'),
              eventBody: String(linkedEventItem.dataset.eventBody || ''),
              eventResult: String(linkedEventItem.dataset.eventResult || ''),
              eventTaskId: String(linkedEventItem.dataset.taskId || ''),
              locationId: String(linkedEventItem.dataset.locationId || ''),
            }}));
          }}
          if (workOrderId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('delivery', workOrderId, {{ label: '推进委托', locationId }}));
          }}
          if (contractId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldContractLaneAction(contractId, {{ locationId }}));
          }}
          if (activeTaskId && linkedContractItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildLinkedContractRouteAction(selection, String(linkedContractItem.dataset.contractId || ''), activeTaskId, locationId, {{ bodySuffix: '，带上证据、评级标准和下一步。' }}));
          }}
          if (listingId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldPurchaseLaneAction(listingId, {{ locationId }}));
          }}
          if (((nextStep || {{}}).label) === '打开评级路线' && workOrderId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('rejection', workOrderId, {{
              locationId,
              body: appendSelectionEventSignal((selection && selection.title ? selection.title : '当前路线') + ': 标记委托 ' + workOrderId + ' 需要返工，写清证据缺口、奖励退回和修订路线。', selection),
            }}));
          }}
          routeFlowActions.innerHTML = actions.join(' ');
        }}
        if (!routeFlowStatus) return;
        if (routeFilterMode === 'all' || !selection) {{
          routeFlowStatus.textContent = '冒险路线：等待地图焦点。';
          if (routeNextStepStatus) routeNextStepStatus.textContent = '推荐下一步：先选择地图焦点。';
          if (routeEventBriefStatus) routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, true);
          if (routeLinkStatus) routeLinkStatus.textContent = routeLinkStatusText({{ emptyText: '关联任务路线：暂无。' }});
          if (actionConsoleStatus) actionConsoleStatus.textContent = defaultActionConsoleStatus || '使用世界行动台提交新的世界行动。';
          return;
        }}
        const contractLabel = contractId || '暂无契约';
        const workLabel = workOrderId || '暂无委托';
        routeFlowStatus.textContent = '冒险路线：' + (selection.title || '焦点') + ' → 事件 ' + eventLabel + ' · 委托 ' + workLabel + ' · 契约 ' + contractLabel + routeOpportunitySegment(opportunityTask) + '。';
        if (routeNextStepStatus) {{
          routeNextStepStatus.textContent = ((nextStep || {{}}).status) || '推荐下一步：为当前路线起草世界行动。';
        }}
        if (routeEventBriefStatus) {{
          routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, false);
        }}
        if (routeLinkStatus) {{
          routeLinkStatus.textContent = routeLinkStatusText({{ taskId: activeTaskId, linkedEventCount, linkedContractCount, opportunityTask, inFocus: true, emptyText: '关联任务路线：当前焦点没有事件/契约链接。' }});
        }}
        if (actionConsoleStatus) {{
          actionConsoleStatus.textContent = '当前世界行动：' + (selection.title || '焦点') + ' · ' + (locationId || '未知地点') + ' · 事件 ' + eventLabel + ' · 委托 ' + workLabel + ' · 契约 ' + contractLabel + routeOpportunitySegment(opportunityTask) + ' · 下一步 ' + (((nextStep || {{}}).label) || '起草世界行动') + '。';
        }}
      }};
      {shared_map_focus_core_js}
      {shared_map_selection_builder_js}
      {shared_map_selection_signal_js}
      {shared_map_focus_panel_js}

      const renderFocusPanel = () => {{
        renderMapFocusPanel({{
          focusSummary,
          focusDetail,
          actionRail,
          focus: lastSelection || buildDefaultFocus(),
          emptyDetail: '选择区域、地图块、热点或实时事件，推动移动和世界行动。',
        }});
      }};
      const setFocusSelection = (focus) => {{
        lastSelection = focus;
        routeFilterMode = 'selection';
        if (lastViewport) {{
          renderStreamHud(lastViewport, focus);
          renderCards(liveEventTarget, filterLiveEventStream(lastViewport.live_event_stream || [], focus), 'event');
        }}
        renderFocusPanel();
        applyRouteFilters();
      }};
      {shared_map_focus_camera_js}
      window.trillionniumApplyMarkerAction = (nodeId, actionId) => {{
        const {{ action, handoff }} = buildMarkerActionHandoff(markerById.get(String(nodeId)) || {{}}, nodeId, actionId);
        const state = buildMarkerRouteActionState(action, handoff, nodeId);
        const moveTarget = forceRouteFieldValueById(routeMoveTargetId(), state.moveTarget || nodeId);
        if (moveTarget) moveTarget.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
        forceRouteFieldValueById(routeActionLocationId(), state.locationId, {{ clearManual: true }});
        forceRouteFieldValueById(routeActionTextareaId(), state.actionBody);
        scrollRoutePanelIntoView(state.panelId);
        setFocusSelection({{ kind: 'node', nodeId }});
        if (cameraSummary) {{ cameraSummary.textContent = '已选择地图行动：' + state.actionLabel + ' · ' + state.command; }}
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
        if (handleSelectionActionButton(selectionActionButton)) return;
        const routeFilterButton = closestFromEvent(event, '.trillionnium-route-filter-action');
        if (routeFilterButton) {{
          routeFilterMode = routeFilterModeFromButton(routeFilterButton, 'all');
          applyRouteFilters();
          return;
        }}
        const routeFlowButton = closestFromEvent(event, '.trillionnium-route-flow-action');
        if (handleRouteActionButton(routeFlowButton, openRouteFlowAction, '路线行动')) return;
        const cameraActionButton = closestFromEvent(event, mapClickSelectors.camera);
        if (handleMapCameraActionButton(cameraActionButton)) return;
        const focusButton = closestFromEvent(event, mapClickSelectors.focus);
        handleMapFocusButton(focusButton);
      }});
      document.addEventListener('input', (event) => {{
        if (!event.target || !event.target.id) return;
        if (routeManualTrackedFieldIds().includes(event.target.id)) {{
          markRouteInputManual(event.target);
        }}
        if (event.target.id === routeActionLocationId()) {{
          event.target.dataset.routeManual = 'true';
          event.target.dataset.routeAutofilled = 'false';
        }}
      }});
      document.addEventListener('change', (event) => {{
        if (!event.target || event.target.id !== routeActionLocationId()) return;
        event.target.dataset.routeManual = 'true';
        event.target.dataset.routeAutofilled = 'false';
      }});
      {shared_map_static_marker_layers_js}

      {shared_map_overlay_render_js}
      {shared_map_card_focus_helpers_js}
      {shared_map_click_action_helpers_js}
      {shared_map_render_cards_js}
      {shared_map_viewport_hydration_js}
      let viewportTimer = null;
      const refreshWorldViewport = () => {{
        if (viewportTimer) window.clearTimeout(viewportTimer);
        viewportTimer = window.setTimeout(async () => {{
          try {{
            const viewport = await fetchViewportSnapshot();
            if (!viewport) return;
            renderFocusPanel();
            applyRouteFilters();
          }} catch (_error) {{}}
        }}, 180);
      }};
      mapAdapter.onViewportChange(mapRuntime, refreshWorldViewport);
      refreshOverlayControls();
      renderOverlayStatus();
      renderFocusPanel();
      applyRouteFilters();
      const startupWorldHandoff = consumeWorldHandoff();
      if (startupWorldHandoff) {{
        applyWorldHandoff(startupWorldHandoff);
      }}
      refreshWorldViewport();
    }})();
  </script>
</body>
</html>"#,
        zones = league.world.world_zones.len(),
        locations = league.world.world_locations.len(),
        entities = league.world.world_entities.len(),
        map_nodes = league.world.world_map_nodes.len(),
        assets = league.world.world_assets.len(),
        asset_upgrades = league.world.world_asset_upgrades.len(),
        companies = league.world.world_companies.len(),
        shops = league.world.world_shops.len(),
        listings = league.world.world_listings.len(),
        purchases = league.world.world_purchases.len(),
        work_orders = league.world.world_work_orders.len(),
        work_rejections = league.world.world_work_rejections.len(),
        work_reopens = league.world.world_work_reopens.len(),
        work_cancellations = league.world.world_work_cancellations.len(),
        factions = league.world.world_factions.len(),
        contracts = league.world.world_contracts.len(),
        completions = league.world.world_contract_completions.len(),
        events = league.world.world_events.len(),
        relationships = league.world.world_relationships.len(),
        map_engine_name = escape_html_text(map_engine_name),
        tile_provider = escape_html_text(tile_provider),
        mirror_scope = escape_html_text(mirror_scope),
        full_mirror_strategy = escape_html_text(full_mirror_strategy),
        simplification_style = escape_html_text(simplification_style),
        scaling_goal = escape_html_text(scaling_goal),
        tile_shard_cards = tile_shard_cards,
        region_shard_cards = region_shard_cards,
        lod_layer_cards = lod_layer_cards,
        hotspot_cards = hotspot_cards,
        prefetch_cards = prefetch_cards,
        live_event_cards = live_event_cards,
        viewport_path = escape_html_text(viewport_path),
        web_session_viewport_path = escape_html_text(web_session_viewport_path),
        map_density_summary = escape_html_text(&map_density_summary),
        map_engine_id = escape_html_text(map_engine_id),
        zone_cards = zone_cards,
        map_cards = map_cards,
        current_map_summary = escape_html_text(&current_map_summary),
        map_exit_options = map_exit_options,
        location_cards = location_cards,
        location_options = location_options,
        entity_cards = entity_cards,
        asset_cards = asset_cards,
        latest_asset_id = escape_html_text(&latest_asset_id),
        company_cards = company_cards,
        latest_company_id = escape_html_text(&latest_company_id),
        latest_listing_id = escape_html_text(&latest_listing_id),
        latest_work_order_id = escape_html_text(&latest_work_order_id),
        shop_cards = shop_cards,
        listing_cards = listing_cards,
        purchase_cards = purchase_cards,
        work_order_cards = work_order_cards,
        work_delivery_cards = work_delivery_cards,
        work_acceptance_cards = work_acceptance_cards,
        work_rejection_cards = work_rejection_cards,
        work_reopen_cards = work_reopen_cards,
        work_cancellation_cards = work_cancellation_cards,
        faction_cards = faction_cards,
        standing_cards = standing_cards,
        contract_cards = contract_cards,
        world_route_task_graph_cards = world_route_task_graph_cards,
        latest_contract_id = escape_html_text(&latest_contract_id),
        current_matrix_user_id = escape_html_text(current_matrix_user_id),
        event_items = event_items,
        console_note = escape_html_text(console_note),
        csrf_input = csrf_input,
        language_runtime_script = trillionnium_language_runtime_script(),
        world_map_data_json = world_map_data_json,
    ))
}
