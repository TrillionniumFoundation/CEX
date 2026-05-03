use super::*;

fn world_user_visible_copy(value: &str) -> String {
    let mut copy = value.to_string();
    let replacements = [
        ("Starter Studio", "Starter Studio / 新手工坊"),
        ("Forge Workbench", "Forge Workbench / 锻造工坊"),
        ("Asset Yard", "Asset Yard / 道具庭院"),
        ("ZBJ Market Gate", "Bounty Market Gate / 悬赏集市门"),
        ("League Coliseum", "League Coliseum / League 竞技场"),
        ("Mirror City Square", "Mirror City Square / 镜像城市广场"),
        ("Guild Raid Hall", "Guild Raid Hall / 公会团本厅"),
        ("Bounty Board", "Bounty Board / 悬赏任务牌"),
        ("Result Rating Dock", "Result Rating Dock / 成果评定台"),
        ("Dispute Desk", "Dispute Desk / 争议柜台"),
        ("镜像城市广场", "Mirror City Square / 镜像城市广场"),
        ("公会团本厅", "Guild Raid Hall / 公会团本厅"),
        ("悬赏任务牌", "Bounty Board / 悬赏任务牌"),
        ("成果评定台", "Result Rating Dock / 成果评定台"),
        ("争议柜台", "Dispute Desk / 争议柜台"),
        ("League 竞技场", "League Arena / League 竞技场"),
        ("starter-studio", "starter-studio / 新手工坊"),
        ("forge-workbench", "forge-workbench / 锻造工坊"),
        ("asset-yard", "asset-yard / 道具庭院"),
        ("zbj-market-gate", "bounty-market-gate / 悬赏集市门"),
        ("league-coliseum", "league-coliseum / League 竞技场"),
        ("cn-shanghai-core", "global-start-zone / 全球首发区"),
        ("客户需求牌", "Bounty Board / 悬赏任务牌"),
        ("交付码头", "Result Rating Dock / 成果评定台"),
        ("所有 World asset 的仓库和展示院，未来可拖拽摆放。", "Asset yard for inventory, materials, and display pieces; free placement arrives later. / 收纳道具、素材和展示件的庭院，后续可自由布置。"),
        ("把想法打磨成方案、商品、素材和交付包的工作台。", "Workbench for turning ideas into proposals, materials, items, and result packages. / 把想法打磨成方案、素材、道具和成果包的工作台。"),
        ("现实任务映射成世界委托的市场门口。", "Gateway where real opportunities become world bounties. / 现实机会映射成世界悬赏的入口。"),
        ("像文字 MUD 的公告栏：任务、需求、报价和线索都贴在这里。", "Text-MUD style bounty board for quests, prompts, offers, and clues. / 像文字 MUD 的公告栏：任务、悬赏、提示和线索都贴在这里。"),
        ("交付、验收、拒收、返工与取消都在这里形成流水线。", "Result submission, rating, revision, and cancellation become adventure routes here. / 成果提交、评级、返工和放弃都在这里形成冒险路线。"),
        ("League 入口，任务可以从市场被带进竞技场评分。", "League entry where bounty-market quests can enter arena rating. / League 入口，任务可以从悬赏集市带进竞技场评级。"),
        ("生成资产 / 审稿 / 做交付包", "Generate items / Review drafts / Build result packs / 生成道具 / 审稿 / 做成果包"),
        ("摆放资产 / 升级工坊 / 创建公司", "Place items / Upgrade studio / Create base / 摆放道具 / 升级工坊 / 创建据点"),
        ("查看资产 / 升级资产 / 挂到店铺", "View items / Upgrade items / List at stall / 查看道具 / 升级道具 / 挂到摊位"),
        ("接任务 / 上架服务 / 雇佣卖家", "Accept bounty / Publish service / Recruit teammates / 接悬赏 / 发布服务 / 招募队友"),
        ("浏览需求 / 投标 / 发布服务", "Browse bounties / Accept challenge / Publish service / 浏览悬赏 / 接取挑战 / 发布服务"),
        ("提交交付 / 验收 / 发起返工/取消", "Submit result / Rate result / Request revision or cancel / 提交成果 / 评级 / 发起返工或放弃"),
        ("craft a real customer-facing studio asset with deliverable, evidence package, risk controls, operating loop, next action, and self review for browser commerce E2E.", "Craft a global-client-ready AI design studio item with result, evidence pack, risk controls, operating loop, next action, and self-review for browser adventure E2E. / 打造一个真实委托可用的 AI 设计工坊道具：写清成果、证据包、风险控制、行动循环、下一步和自检记录，用于 browser adventure E2E。"),
        ("browser commerce E2E", "browser adventure E2E / 浏览器冒险验收"),
        ("AI 设计公司", "AI Design Studio / AI 设计工坊"),
        ("服务真实客户", "serve real global clients / 完成海外真实委托"),
        ("route_task", "route task / 路线任务"),
        ("contract_capture", "contract capture / 契约登记"),
        ("work_order", "adventure commission / 冒险委托"),
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
        ("commerce", "commerce / 集市"),
        ("governance", "governance / 治理"),
        ("builder", "builder / 建造"),
        ("competition", "competition / 竞技"),
        ("faction-city-clerks", "City Clerks / 城市书记门"),
        ("faction-craft-union", "Craft Union / 工坊同盟"),
        ("faction-market-guild", "Market Guild / 集市公会"),
        ("faction-league-order", "League Order / League 教团"),
        ("public_hub", "public hub / 公共枢纽"),
        ("workshop", "workshop / 工坊"),
        ("real_task_gateway", "real quest gate / 真实任务入口"),
        ("arena", "arena / 竞技场"),
        ("market", "market / 集市"),
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
        ("加入赛场 / 提交比赛 / 查看排行", "Join arena / Submit match / View ranking / 加入赛场 / 提交比赛 / 查看排行"),
        ("认领职责 / 组队打本 / 分配 Agent", "Claim role / Form raid team / Assign Agent / 认领职责 / 组队打本 / 分配 Agent"),
        ("摆放道具 / 升级工坊 / 创建据点", "Place items / Upgrade studio / Create base / 摆放道具 / 升级工坊 / 创建据点"),
        ("生成道具 / 审稿 / 做成果包", "Generate items / Review drafts / Build result pack / 生成道具 / 审稿 / 做成果包"),
        ("查账 / 发起争议 / 登记合约", "Check ledger / Open dispute / Register contract / 查账 / 发起争议 / 登记合约"),
        ("查看道具 / 升级道具 / 挂到摊位", "View items / Upgrade items / List at stall / 查看道具 / 升级道具 / 挂到摊位"),
        ("接悬赏 / 发布服务 / 招募队友", "Accept bounty / Publish service / Recruit teammates / 接悬赏 / 发布服务 / 招募队友"),
        ("浏览悬赏 / 接取挑战 / 发布服务", "Browse bounties / Accept challenge / Publish service / 浏览悬赏 / 接取挑战 / 发布服务"),
        ("提交成果 / 评级 / 发起返工或放弃", "Submit result / Rate result / Request revision or cancel / 提交成果 / 评级 / 发起返工或放弃"),
        ("申请仲裁 / 查看退款 / 提交证据", "Request arbitration / Check refund / Submit evidence / 申请仲裁 / 查看退款 / 提交证据"),
        ("多人协作任务和 Agent 阵容站位的大厅。", "Guild raid lobby for co-op missions and Agent loadouts. / 多人协作任务和 Agent 阵容站位的大厅。"),
        ("现实任务和客户需求映射为世界委托的入口。", "Real-world tasks and client briefs become world commissions here. / 现实任务和客户需求映射为世界委托的入口。"),
        ("现实世界映射、身份、关系和城市自由行动", "Reality mapping, identity, relationships, and city free-roam / 现实世界映射、身份、关系和城市自由行动"),
        ("建造、工坊、道具、摊位和创造系统", "Building, studios, items, stalls, and creation systems / 建造、工坊、道具、摊位和创造系统"),
        ("建造、工坊、资产、店铺和创造系统", "Building, studios, assets, shops, and creation systems / 建造、工坊、资产、店铺和创造系统"),
        ("真实任务、委托、悬赏、招募和声望", "Real tasks, commissions, bounties, recruiting, and reputation / 真实任务、委托、悬赏、招募和声望"),
        ("真实任务、客户、交易、雇佣和声望", "Real tasks, clients, trades, hiring, and reputation / 真实任务、客户、交易、雇佣和声望"),
        ("竞技、团本、赛季和裁判结算", "Arenas, raids, seasons, judging, and settlement / 竞技、团本、赛季和裁判结算"),
        ("玩家、Agent 居民、公会和现实事件进入世界的公共入口。", "Public entrance where players, Agent residents, guilds, and real-world events enter the world. / 玩家、Agent 居民、公会和现实事件进入世界的公共入口。"),
        ("自由建造第一间工坊、Agent 据点或公会基地。", "Freely build the first studio, Agent base, or guild home. / 自由建造第一间工坊、Agent 据点或公会基地。"),
        ("现实机会和线索映射为世界悬赏的入口。", "Entrance where real opportunities and clues become world bounties. / 现实机会和线索映射为世界悬赏的入口。"),
        ("League 赛事、团本、评级、奖励与排行榜发生地。", "Home of League matches, raids, ratings, rewards, and leaderboards. / League 赛事、团本、评级、奖励与排行榜发生地。"),
        ("新手工坊", "Starter Studio / 新手工坊"),
        ("悬赏集市门", "Bounty Market Gate / 悬赏集市门"),
        ("侦察、现实情报、任务发现", "scouting, real-world intel, quest discovery / 侦察、现实情报、任务发现"),
        ("建造、生成、工坊资产", "building, generation, studio assets / 建造、生成、工坊资产"),
        ("资产登记、声望、合约和结算提示", "asset registry, reputation, contracts, and settlement prompts / 资产登记、声望、合约和结算提示"),
        ("Map density booting.", "Map density loading / 地图密度加载中。"),
    ];
    if let Some((_, to)) = replacements.iter().find(|(from, _)| value == *from) {
        return (*to).to_string();
    }
    for (from, to) in replacements {
        copy = copy.replace(from, to);
    }
    copy
}

fn escape_world_visible_text(value: &str) -> String {
    let copy = world_user_visible_copy(value);
    i18n_span_from_bilingual_slash_copy(&copy).unwrap_or_else(|| escape_html_text(&copy))
}

fn world_node_kind_label(kind: &str) -> &str {
    match kind {
        "hub_square" => "hub square",
        "agent_home" => "Agent home",
        "ledger_office" => "reward office",
        "workshop_room" => "workshop room",
        "craft_station" => "craft station",
        "asset_yard" => "asset yard",
        "market_gate" => "bounty gate",
        "client_board" => "quest board",
        "delivery_dock" => "rating dock",
        "dispute_desk" => "dispute desk",
        "arena_gate" => "arena gate",
        "raid_hall" => "raid hall",
        _ => kind,
    }
}

fn world_map_status_label(value: &str) -> String {
    world_user_visible_copy(match value {
        "prefetch" => "prefetch",
        "street_nodes" => "street nodes",
        "neighbor_tile_warmup" => "neighbor warmup",
        "warm" => "warm",
        "active" => "active",
        "planned" => "planned",
        "open" | "OPEN" => "open",
        "contract" => "contract",
        "venture" => "venture",
        "no-task" => "no task",
        "world_event" => "world event",
        "dense" => "dense",
        "regional" => "regional",
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
                escape_world_visible_text(&entity.role),
                escape_html_text(&entity.entity_id),
                escape_world_visible_text(&entity.status),
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
                "{} · {} · <span data-i18n-en=\"exits\" data-i18n-zh=\"出口\">exits</span> {}",
                escape_world_visible_text(&node.name),
                escape_world_visible_text(&node.description),
                node.exits.len()
            )
        })
        .unwrap_or_else(|| {
            "<span data-i18n-en=\"Map booting…\" data-i18n-zh=\"地图启动中…\">Map booting…</span>"
                .to_string()
        });
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
                "<article class=\"mini company\"><strong>{}</strong><span>{} · Lv {} · Studio value {}</span><code>{}</code><small>Items {} · Reputation {}</small></article>",
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
                "<article class=\"mini shop\"><strong>{}</strong><span>{} · Quest cards {} · Heat {}</span><code>{}</code><small>Studio {} · {}</small></article>",
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
                "<article class=\"mini listing\"><strong>{}</strong><span>{} · Bounty {} · Quality {}</span><code>{}</code><small>Base {} · {}</small></article>",
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
        "<article class=\"mini listing\"><strong>No bounty cards yet</strong><span>Use /sell latest to publish an acceptable studio commission.</span><code>/sell latest</code></article>".to_string()
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
                "<article class=\"mini purchase world-route-filter-item\" data-route-bucket=\"purchase\" data-location-id=\"{}\" data-purchase-id=\"{}\" data-listing-id=\"{}\" data-company-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Contract Accepted</strong><span>Bounty {} · {}</span><code>{}</code><small>Quest card {} · Escrow {} · Claim {}</small></article>",
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
        "<article class=\"mini purchase\"><strong>No accept records yet</strong><span>Accept a quest card to open an adventure commission and escrowed reward.</span><code>/buy latest</code></article>".to_string()
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
                "<article class=\"mini work world-route-filter-item\" data-route-bucket=\"work_order\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-purchase-id=\"{}\" data-company-id=\"{}\" data-listing-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Adventure Commission</strong><span>{} · Difficulty {}</span><code>{}</code><small>Quest card {} · Purchase {}</small></article>",
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
        "<article class=\"mini work\"><strong>No adventure commissions yet</strong><span>After accepting a quest card, the commission enters a result-submission route.</span><code>/work</code></article>".to_string()
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
                "<article class=\"mini delivery world-route-filter-item\" data-route-bucket=\"delivery\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Result Submission</strong><span>Score {:.1} · {}</span><code>{}</code><small>Commission {}</small></article>",
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
        "<article class=\"mini delivery\"><strong>No result submissions yet</strong><span>After accepting a commission, submit the result and evidence pack here.</span><code>/work deliver latest</code></article>".to_string()
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
                "<article class=\"mini acceptance world-route-filter-item\" data-route-bucket=\"acceptance\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Rating Passed</strong><span>{} · Reputation +{}</span><code>{}</code><small>Commission {}</small></article>",
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
        "<article class=\"mini acceptance\"><strong>No rating passes yet</strong><span>Once results meet the bar, pass rating and claim reputation rewards.</span><code>/work accept latest</code></article>".to_string()
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
                "<article class=\"mini rejection world-route-filter-item\" data-route-bucket=\"rejection\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Revision Needed</strong><span>{} · Reward refund {}</span><code>{}</code><small>Commission {}</small></article>",
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
        "<article class=\"mini rejection\"><strong>No revision records yet</strong><span>When results miss the bar, mark evidence gaps and refund escrowed reward.</span><code>/work reject latest</code></article>".to_string()
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
                "<article class=\"mini reopen world-route-filter-item\" data-route-bucket=\"reopen\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Commission Reopened</strong><span>{} · Re-escrow {}</span><code>{}</code><small>Commission {}</small></article>",
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
        "<article class=\"mini reopen\"><strong>No reopen records yet</strong><span>After revision, re-escrow the reward and allow another submission.</span><code>/work reopen latest</code></article>".to_string()
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
                "<article class=\"mini cancellation world-route-filter-item\" data-route-bucket=\"cancellation\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>Commission Canceled</strong><span>{} · Reward refund {}</span><code>{}</code><small>Commission {}</small></article>",
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
        "<article class=\"mini cancellation\"><strong>No cancellation records yet</strong><span>Before result submission, cancel the commission and refund escrowed reward.</span><code>/work cancel latest</code></article>".to_string()
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
                "<article class=\"mini faction\"><strong>{}</strong><span>{} · Reputation {}</span><code>{}</code><small>{}</small></article>",
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
                "<article class=\"mini standing\"><strong>{}</strong><span>{} Reputation · {}</span><code>{}</code><small>{}</small></article>",
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
                "<article class=\"mini contract world-route-filter-item\" data-route-bucket=\"contract\" data-location-id=\"{}\" data-contract-id=\"{}\" data-task-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · Value {}</span><code>{}</code><small>Task {} · {}</small></article>",
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
        "<article class=\"mini contract\"><strong>No world contracts yet</strong><span>Use /contract to mirror real opportunities into trackable commissions.</span><code>/contract</code></article>".to_string()
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
                escape_html_text(event.cex_status.as_deref().unwrap_or(&event.result)),
                escape_html_text(&event.event_kind),
                escape_html_text(&event.body),
                escape_html_text(&event.result),
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
        "Filter by Focus",
        "按焦点筛选路线",
        "Show All Routes",
        "显示全部路线",
    );
    let shared_map_route_target_resolution_js = real_world_map_route_target_resolution_js();
    let shared_map_route_status_js = real_world_map_route_status_js();
    let shared_map_route_contract_js = real_world_map_route_contract_js();
    let shared_map_route_action_js = real_world_map_route_action_js();
    let shared_map_viewport_hydration_js = real_world_map_viewport_hydration_js();
    let shared_map_render_cards_js =
        real_world_map_render_cards_js(RealWorldMapShellCardStyle::WorldMini);
    let world_header_language_switcher =
        trillionnium_language_inline_switcher_html("trillionnium-world-language-select");

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
    body {{ margin:0; min-height:100vh; overflow-x:hidden; font-family:Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:radial-gradient(circle at 22% 0%, #133f38 0, transparent 32rem), radial-gradient(circle at 90% 18%, #3b245c 0, transparent 30rem), var(--bg); color:var(--text); }}
    header.world-hero {{ padding:32px min(6vw,72px) 16px; display:grid; gap:18px; grid-template-columns:minmax(0,1fr) minmax(280px,.46fr); align-items:stretch; }}
    h1 {{ margin:0; font-size:clamp(44px,7vw,96px); line-height:.88; letter-spacing:-.075em; }}
    h2 {{ margin:0 0 16px; letter-spacing:-.03em; }}
    .subtitle {{ color:var(--muted); font-size:18px; max-width:840px; line-height:1.55; }}
    .hero-card,.card,.panel {{ border:1px solid rgba(255,255,255,.11); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); box-shadow:0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(14px); border-radius:24px; }}
    .hero-card,.panel,.card {{ padding:24px; }}
    .world-hero-main {{ min-height:350px; display:grid; align-content:center; gap:16px; }}
    .world-hero-kicker {{ display:flex; align-items:center; justify-content:space-between; gap:12px; flex-wrap:wrap; }}
    .world-hero-title {{ display:grid; gap:10px; }}
    .world-hero-actions {{ display:flex; flex-wrap:wrap; gap:10px; margin-top:2px; }}
    .world-mobile-promise {{ display:flex; flex-wrap:wrap; gap:8px; }}
    .world-mobile-promise span {{ border:1px solid rgba(100,227,255,.2); background:rgba(100,227,255,.075); color:var(--cyan); border-radius:999px; padding:8px 11px; font-size:12px; font-weight:850; }}
    .hero-card {{ display:grid; gap:14px; align-content:space-between; }}
    .hero-card strong {{ color:var(--gold); font-size:22px; }}
    .language-switcher {{ display:inline-flex; align-items:center; gap:8px; width:max-content; max-width:100%; border:1px solid rgba(100,227,255,.24); background:rgba(255,255,255,.065); color:var(--cyan); border-radius:999px; padding:6px 8px 6px 10px; font-size:12px; font-weight:900; }}
    .language-switcher select {{ width:auto; min-width:92px; max-width:130px; margin:0; border:0; background:rgba(7,8,20,.72); color:var(--text); border-radius:999px; padding:7px 26px 7px 10px; font:inherit; font-size:12px; }}
    .world-hero-steps {{ list-style:none; padding:0; margin:0; display:grid; gap:9px; }}
    .world-hero-steps li {{ display:grid; gap:3px; padding:10px 12px; border:1px solid rgba(255,255,255,.1); border-radius:16px; background:rgba(255,255,255,.055); }}
    .world-hero-steps b {{ color:var(--text); }}
    .world-hero-steps span {{ color:var(--muted); font-size:13px; line-height:1.35; }}
    .stats.world-pulse-strip {{ display:grid; grid-template-columns:repeat(6,minmax(0,1fr)); gap:10px; margin:0; align-items:stretch; }}
    .pulse-card {{ min-width:0; min-height:96px; padding:13px; display:grid; align-content:space-between; gap:6px; border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.06); border-radius:18px; }}
    .pulse-card.is-primary {{ grid-column:span 2; border-color:rgba(248,195,91,.28); background:linear-gradient(145deg,rgba(248,195,91,.16),rgba(100,227,255,.055)); }}
    .pulse-card span,.stat span {{ color:var(--muted); font-size:12px; font-weight:850; text-transform:uppercase; letter-spacing:.08em; }}
    .pulse-card b {{ display:block; font-size:clamp(28px,4vw,42px); color:var(--gold); letter-spacing:-.05em; line-height:.92; }}
    .pulse-card small {{ color:var(--muted); line-height:1.35; }}
    .world-stats-more {{ grid-column:1 / -1; border:1px solid rgba(255,255,255,.1); border-radius:18px; background:rgba(255,255,255,.045); overflow:hidden; }}
    .world-stats-more summary {{ cursor:pointer; list-style:none; display:flex; align-items:center; justify-content:space-between; gap:10px; padding:11px 14px; color:var(--cyan); font-weight:900; }}
    .world-stats-more summary::-webkit-details-marker {{ display:none; }}
    .world-stats-more summary::after {{ content:"+"; color:var(--gold); font-size:18px; }}
    .world-stats-more[open] summary::after {{ content:"–"; }}
    .stats-more-grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(120px,1fr)); gap:8px; padding:0 12px 12px; }}
    .stat {{ padding:10px; background:rgba(255,255,255,.055); border-radius:14px; }}
    .stat b {{ display:block; font-size:19px; color:var(--gold); }}
    main {{ padding:20px min(6vw,72px) 60px; display:grid; gap:24px; }}
    .grid {{ display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:18px; }}
    .mini-grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; }}
    .card h3 {{ margin:12px 0; font-size:24px; }}
    .card p {{ color:var(--muted); line-height:1.55; }}
    .card footer {{ display:grid; gap:8px; margin-top:18px; color:var(--gold); }}
    .pill {{ display:inline-flex; border:1px solid rgba(100,227,255,.35); color:var(--cyan); padding:5px 10px; border-radius:999px; font-size:12px; text-transform:uppercase; letter-spacing:.12em; }}
    .world-next-card {{ display:grid; gap:12px; border:1px solid rgba(248,195,91,.2); background:linear-gradient(145deg,rgba(248,195,91,.13),rgba(100,227,255,.055)); border-radius:20px; padding:18px; }}
    .world-next-card strong {{ color:var(--gold); font-size:22px; }}
    .world-map-player-summary {{ display:grid; grid-template-columns:minmax(0,1fr) auto; gap:12px; align-items:center; border:1px solid rgba(100,227,255,.18); background:rgba(100,227,255,.055); border-radius:20px; padding:14px; margin:14px 0; }}
    .world-map-player-summary strong {{ display:block; color:var(--gold); margin-bottom:4px; }}
    .world-map-player-summary .subtitle {{ margin:0; }}
    .world-map-loop-steps {{ list-style:none; padding:0; margin:12px 0; display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; }}
    .world-map-loop-steps li {{ display:grid; gap:4px; min-height:82px; border:1px solid rgba(248,195,91,.2); background:rgba(248,195,91,.07); border-radius:16px; padding:12px; }}
    .world-map-loop-steps b {{ color:var(--gold); }}
    .world-map-loop-steps span {{ color:var(--muted); font-size:13px; line-height:1.35; }}
    .world-advanced-map-drawer {{ min-width:0; border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.035); border-radius:18px; padding:10px; overflow:hidden; }}
    .world-adventure-steps {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; margin:12px 0; }}
    .world-adventure-step {{ border:1px solid rgba(248,195,91,.22); background:rgba(248,195,91,.075); border-radius:18px; padding:14px; display:grid; gap:6px; }}
    .world-adventure-step b {{ color:var(--gold); }}
    .world-route-drawer {{ border-color:rgba(100,227,255,.18); background:rgba(100,227,255,.045); border-radius:18px; padding:12px; }}
    .dev-details {{ margin-top:12px; color:var(--muted); min-width:0; overflow-wrap:anywhere; }}
    .dev-details p {{ min-width:0; overflow-wrap:anywhere; }}
    .dev-details summary {{ cursor:pointer; width:max-content; border:1px solid rgba(255,255,255,.1); border-radius:999px; padding:8px 12px; min-height:36px; display:inline-flex; align-items:center; background:rgba(255,255,255,.05); color:rgba(246,247,251,.72); font-size:12px; font-weight:800; }}
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
    #world-real-map .leaflet-control-zoom a {{ width:40px; height:40px; line-height:40px; font-size:20px; }}
    .asset strong {{ color:var(--green); }}
    .mini span,.mini small,.timeline small,.timeline em {{ color:var(--muted); }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:.55fr 1.35fr .55fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    .timeline em {{ grid-column:1 / -1; font-style:normal; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; max-width:100%; overflow-wrap:anywhere; word-break:break-word; white-space:normal; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:14px 18px; border-radius:16px; display:inline-flex; justify-content:center; align-items:center; font-weight:800; text-decoration:none; }}
    .cta.secondary {{ color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(100,227,255,.24); }}
    @media (max-width:1050px) {{ header.world-hero,.play,.map-shell {{ grid-template-columns:1fr; }} .grid,.mini-grid,.world-adventure-steps {{ grid-template-columns:1fr; }} .stats.world-pulse-strip {{ grid-template-columns:repeat(3,minmax(0,1fr)); }} #world-real-map {{ order:-1; }} }}
    @media (max-width:720px) {{
      header.world-hero {{ padding:12px 14px 6px; gap:8px; }}
      .world-hero-main {{ min-height:auto; gap:8px; }}
      .world-hero-kicker {{ gap:8px; }}
      h1 {{ font-size:clamp(36px,13vw,54px); letter-spacing:-.068em; }}
      h2 {{ margin-bottom:10px; }}
      .subtitle {{ font-size:14px; line-height:1.42; }}
      .world-hero-title .subtitle {{ margin:0; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }}
      .pill {{ font-size:10px; padding:4px 8px; }}
      .world-mobile-promise {{ display:none; }}
      .world-mobile-promise span {{ padding:6px 8px; font-size:11px; }}
      .language-switcher {{ padding:5px 6px 5px 8px; font-size:11px; }}
      .language-switcher select {{ min-width:82px; max-width:112px; padding:6px 22px 6px 8px; font-size:11px; }}
      .world-hero-actions {{ display:grid; grid-template-columns:1fr 1fr; gap:8px; }}
      .world-hero-actions .cta,.hero-card .cta,.world-map-player-summary .cta {{ min-height:44px; padding:10px 11px; border-radius:14px; font-size:13px; }}
      button,input,textarea,select,.focus-chip,.overlay-toggle {{ min-height:44px; }}
      .hero-card,.panel,.card {{ padding:15px; border-radius:20px; }}
      .hero-card {{ gap:8px; }}
      .hero-card strong {{ font-size:18px; }}
      .hero-card .cta {{ display:none; }}
      .world-hero-steps {{ grid-template-columns:repeat(3,minmax(0,1fr)); gap:7px; }}
      .world-hero-steps li {{ padding:8px; border-radius:13px; }}
      .world-hero-steps b {{ font-size:12px; }}
      .world-hero-steps span {{ display:none; }}
      main {{ padding:8px 12px 42px; gap:14px; }}
      .stats.world-pulse-strip {{ grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px; }}
      .pulse-card {{ min-height:72px; padding:10px; border-radius:15px; }}
      .pulse-card.is-primary {{ grid-column:1 / -1; min-height:78px; }}
      .pulse-card span,.stat span {{ font-size:10px; letter-spacing:.06em; }}
      .pulse-card b {{ font-size:26px; }}
      .pulse-card small {{ font-size:11px; }}
      .world-stats-more summary {{ padding:10px 12px; font-size:13px; }}
      .stats-more-grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); padding:0 9px 9px; }}
      .stat {{ padding:8px; border-radius:12px; }}
      .stat b {{ font-size:16px; }}
      .world-map-player-summary,.world-map-loop-steps {{ grid-template-columns:1fr; }}
      .world-map-loop-steps li {{ min-height:auto; padding:10px 12px; }}
      main > section {{ order:3; }}
      #world-map-shell-panel {{ display:contents; order:1; }}
      #world-pulse-strip {{ order:2; }}
      #world-map-shell-panel .map-shell {{ display:contents; }}
      #world-map-shell-panel .map-copy {{ order:3; border:1px solid rgba(255,255,255,.12); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); border-radius:20px; padding:15px; }}
      .map-shell {{ gap:12px; }}
      #world-real-map {{ order:1; min-height:min(54svh,410px); border-radius:18px; }}
      .map-stream-hud,.overlay-toggle-bar,.focus-stack {{ gap:6px; }}
      .hud-chip,.focus-chip,.overlay-toggle {{ padding:7px 9px; font-size:12px; }}
      .timeline li {{ grid-template-columns:1fr; }}
    }}
  </style>
</head>
<body>
  <header id="world-mobile-first-screen" class="world-hero">
    <section class="world-hero-main">
      <div class="world-hero-kicker"><div class="pill" data-i18n-en="Reality Mirror Adventure" data-i18n-zh="现实镜像冒险">Reality Mirror Adventure</div><div id="world-language-switcher">{world_header_language_switcher}</div></div>
      <div class="world-hero-title">
        <h1>Trillionnium World</h1>
        <p class="subtitle" data-i18n-en="Global-first open world built for one-thumb exploration: pick a real city focus, accept a bounty, submit a result, get rated, and claim rewards." data-i18n-zh="面向海外首发、为单手探索重做的开放世界：选择现实城市焦点，接取悬赏，提交成果，获得评级并领取奖励。">Global-first open world built for one-thumb exploration: pick a real city focus, accept a bounty, submit a result, get rated, and claim rewards.</p>
      </div>
      <div class="world-mobile-promise" aria-label="World mobile promises" data-i18n-aria-label-en="World mobile promises" data-i18n-aria-label-zh="世界移动端承诺">
        <span data-i18n-en="Map first" data-i18n-zh="地图优先">Map first</span>
        <span data-i18n-en="Player actions only" data-i18n-zh="只露出玩家行动">Player actions only</span>
        <span data-i18n-en="Stats stay compact" data-i18n-zh="统计保持紧凑">Stats stay compact</span>
      </div>
      <div id="world-hero-mobile-actions" class="world-hero-actions">
        <a class="cta" href='#world-real-map' data-i18n-en="Open Map" data-i18n-zh="打开地图">Open Map</a>
        <a class="cta secondary" href='#world-action-console' data-i18n-en="Start Action" data-i18n-zh="发起行动">Start Action</a>
      </div>
    </section>
    <aside class="hero-card">
      <strong data-i18n-en="Next Adventure" data-i18n-zh="下一步冒险">Next Adventure</strong>
      <ol class="world-hero-steps">
        <li><b data-i18n-en="1 · Focus" data-i18n-zh="1 · 选焦点">1 · Focus</b><span data-i18n-en="Tap a map place, event, or route." data-i18n-zh="点选地图地点、事件或路线。">Tap a map place, event, or route.</span></li>
        <li><b data-i18n-en="2 · Quest" data-i18n-zh="2 · 接任务">2 · Quest</b><span data-i18n-en="Accept a bounty and submit results." data-i18n-zh="接取悬赏并提交成果。">Accept a bounty and submit results.</span></li>
        <li><b data-i18n-en="3 · Reward" data-i18n-zh="3 · 拿奖励">3 · Reward</b><span data-i18n-en="Get rated, paid, and routed onward." data-i18n-zh="获得评级、奖励和下一步路线。">Get rated, paid, and routed onward.</span></li>
      </ol>
      <a id="world-league-link" class="cta" href="/league" data-i18n-en="Enter League Arena" data-i18n-zh="进入 League 竞技场">Enter League Arena</a>
    </aside>
  </header>
  <main>
    <section id="world-pulse-strip" class="stats world-pulse-strip" aria-label="World pulse counters" data-i18n-aria-label-en="World pulse counters" data-i18n-aria-label-zh="世界脉冲统计">
      <article class="pulse-card is-primary"><span data-i18n-en="Live Events" data-i18n-zh="实时事件">Live Events</span><b>{events}</b><small data-i18n-en="Tap one to turn the map into a route." data-i18n-zh="点一个事件，把地图变成路线。">Tap one to turn the map into a route.</small></article>
      <article class="pulse-card"><span data-i18n-en="Map Points" data-i18n-zh="地图点">Map Points</span><b>{map_nodes}</b><small data-i18n-en="Real city anchors" data-i18n-zh="现实城市锚点">Real city anchors</small></article>
      <article class="pulse-card"><span data-i18n-en="Quest Cards" data-i18n-zh="任务牌">Quest Cards</span><b>{listings}</b><small data-i18n-en="Available bounties" data-i18n-zh="可接取悬赏">Available bounties</small></article>
      <article class="pulse-card"><span data-i18n-en="Commissions" data-i18n-zh="委托">Commissions</span><b>{work_orders}</b><small data-i18n-en="Accepted loops" data-i18n-zh="已进入执行循环">Accepted loops</small></article>
      <article class="pulse-card"><span data-i18n-en="Agents" data-i18n-zh="居民">Agents</span><b>{entities}</b><small data-i18n-en="World residents" data-i18n-zh="世界居民">World residents</small></article>
      <details id="world-stats-compact-more" class="world-stats-more">
        <summary data-i18n-en="More world counters" data-i18n-zh="更多世界统计">More world counters</summary>
        <div class="stats-more-grid">
          <div class="stat"><span data-i18n-en="Zones" data-i18n-zh="区域">Zones</span><b>{zones}</b></div>
          <div class="stat"><span data-i18n-en="Places" data-i18n-zh="地点">Places</span><b>{locations}</b></div>
          <div class="stat"><span data-i18n-en="Items" data-i18n-zh="道具">Items</span><b>{assets}</b></div>
          <div class="stat"><span data-i18n-en="Upgrades" data-i18n-zh="升级">Upgrades</span><b>{asset_upgrades}</b></div>
          <div class="stat"><span data-i18n-en="Studios" data-i18n-zh="工坊">Studios</span><b>{companies}</b></div>
          <div class="stat"><span data-i18n-en="Hubs" data-i18n-zh="据点">Hubs</span><b>{shops}</b></div>
          <div class="stat"><span data-i18n-en="Accepted" data-i18n-zh="已接取">Accepted</span><b>{purchases}</b></div>
          <div class="stat"><span data-i18n-en="Revisions" data-i18n-zh="返工">Revisions</span><b>{work_rejections}</b></div>
          <div class="stat"><span data-i18n-en="Reopens" data-i18n-zh="重开">Reopens</span><b>{work_reopens}</b></div>
          <div class="stat"><span data-i18n-en="Cancels" data-i18n-zh="放弃">Cancels</span><b>{work_cancellations}</b></div>
          <div class="stat"><span data-i18n-en="Factions" data-i18n-zh="阵营">Factions</span><b>{factions}</b></div>
          <div class="stat"><span data-i18n-en="Contracts" data-i18n-zh="契约">Contracts</span><b>{contracts}</b></div>
          <div class="stat"><span data-i18n-en="Reports" data-i18n-zh="战报">Reports</span><b>{completions}</b></div>
          <div class="stat"><span data-i18n-en="Relations" data-i18n-zh="关系">Relations</span><b>{relationships}</b></div>
        </div>
      </details>
    </section>
    <section id="world-map-shell-panel" class="panel">
      <div class="map-shell">
        <div class="map-copy">
          <div class="pill" data-i18n-en="Reality Mirror Map" data-i18n-zh="现实镜像地图">Reality Mirror Map</div>
          <h2 data-i18n-en="Global City Exploration" data-i18n-zh="海外首发城市探索">Global City Exploration</h2>
          <p class="subtitle" data-i18n-en="Start from any map focus: regions, hotspots, live events, and route tasks become the next action. Players see story, places, commissions, and rewards; engine details stay in debug drawers." data-i18n-zh="从地图焦点进入冒险：区域、热点、实时事件和任务路线会自动串成下一步行动。玩家看到故事、地点、委托和奖励；引擎细节收进调试抽屉。">Start from any map focus: regions, hotspots, live events, and route tasks become the next action. Players see story, places, commissions, and rewards; engine details stay in debug drawers.</p>
          <div class="world-map-player-summary">
            <div>
              <strong data-i18n-en="Current Status" data-i18n-zh="当前状态">Current Status</strong>
              <p id="world-map-density-summary" class="subtitle">{map_density_summary}</p>
              <p id="world-map-camera-summary" class="subtitle" data-i18n-en="Camera loading…" data-i18n-zh="镜头加载中…">Camera loading…</p>
            </div>
            <a class="cta" href='#world-action-console' data-i18n-en="Start Next Action" data-i18n-zh="发起下一步行动">Start Next Action</a>
          </div>
          <ol class="world-map-loop-steps" aria-label="World player loop" data-i18n-aria-label-en="World player loop" data-i18n-aria-label-zh="世界玩家三步循环">
            <li><b data-i18n-en="1 · Choose focus" data-i18n-zh="1 · 选择焦点">1 · Choose focus</b><span data-i18n-en="Tap a place, event, or route." data-i18n-zh="点选地点、事件或路线。">Tap a place, event, or route.</span></li>
            <li><b data-i18n-en="2 · Accept bounty" data-i18n-zh="2 · 接悬赏">2 · Accept bounty</b><span data-i18n-en="Convert the focus into a quest card." data-i18n-zh="把焦点转成任务牌。">Convert the focus into a quest card.</span></li>
            <li><b data-i18n-en="3 · Submit result" data-i18n-zh="3 · 提交成果">3 · Submit result</b><span data-i18n-en="Get rated, rewarded, and routed onward." data-i18n-zh="获得评级、奖励和下一步路线。">Get rated, rewarded, and routed onward.</span></li>
          </ol>
          <div id="world-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
          </div>
          <div class="mini" style="margin-top:14px;">
            <strong data-i18n-en="Map Action Rail" data-i18n-zh="地图行动栏">Map Action Rail</strong>
            <span id="world-map-focus-summary" data-i18n-en="Waiting for map focus…" data-i18n-zh="等待选择地图焦点…">Waiting for map focus…</span>
            <small id="world-map-focus-detail" data-i18n-en="Choose a region, tile, hotspot, or live event to drive movement and world action." data-i18n-zh="选择区域、地图块、热点或实时事件，推动移动和世界行动。">Choose a region, tile, hotspot, or live event to drive movement and world action.</small>
            <div id="world-map-action-rail" class="focus-stack"></div>
          </div>
          <p id="world-map-route-filter-status" class="subtitle" data-i18n-en="Route filter: show all world activity." data-i18n-zh="路线筛选：显示全部世界活动。">Route filter: show all world activity.</p>
          <div id="world-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <p id="world-map-route-flow-status" class="subtitle" data-i18n-en="Adventure route: waiting for map focus." data-i18n-zh="冒险路线：等待地图焦点。">Adventure route: waiting for map focus.</p>
          <p id="world-map-route-next-step-status" class="subtitle" data-i18n-en="Recommended next step: choose a map focus first." data-i18n-zh="推荐下一步：先选择地图焦点。">Recommended next step: choose a map focus first.</p>
          <p id="world-map-route-event-brief-status" class="subtitle" data-i18n-en="Event brief: waiting for live-event focus." data-i18n-zh="事件简报：等待实时事件焦点。">Event brief: waiting for live-event focus.</p>
          <p id="world-map-route-link-status" class="subtitle" data-i18n-en="Linked task route: none yet." data-i18n-zh="关联任务路线：暂无。">Linked task route: none yet.</p>
          <div id="world-map-route-flow-actions" class="focus-stack"></div>
          <details class="dev-details world-advanced-map-drawer">
            <summary data-i18n-en="Advanced map layers" data-i18n-zh="高级地图图层">Advanced map layers</summary>
            <div id="world-tile-shards-live" class="mini-grid">{tile_shard_cards}</div>
            <div id="world-region-shards-live" class="mini-grid" style="margin-top:12px">{region_shard_cards}</div>
            <div class="mini-grid" style="margin-top:12px">{lod_layer_cards}</div>
            <div id="world-poi-hotspots-live" class="mini-grid" style="margin-top:12px">{hotspot_cards}</div>
            <div id="world-prefetch-queue-live" class="mini-grid" style="margin-top:12px">{prefetch_cards}</div>
            <div id="world-live-events-live" class="mini-grid" style="margin-top:12px">{live_event_cards}</div>
            <p style="margin-top:12px"><strong>Global Real-world Map Engine</strong>: <code>{map_engine_name}</code> + <code>{tile_provider}</code></p>
            <p><strong>Mirror</strong>: <code>{mirror_scope}</code> · <strong>Strategy</strong>: <code>{full_mirror_strategy}</code> · <strong>Style</strong>: <code>{simplification_style}</code> · <strong>Goal</strong>: <code>{scaling_goal}</code></p>
            <p><strong>Viewport API</strong>: <code>{viewport_path}</code></p>
            <p><strong>Web Viewport</strong>: <code>{web_session_viewport_path}</code></p>
            <div id="world-map-stream-hud" class="map-stream-hud">
              <span class="hud-chip"><strong>{map_stream_region_count}</strong> <span data-i18n-en="regional shards" data-i18n-zh="个区域分片">regional shards</span></span>
              <span class="hud-chip"><strong>{map_visible_marker_count}</strong> <span data-i18n-en="visible places" data-i18n-zh="个可见地点">visible places</span></span>
              <span class="hud-chip"><strong>{map_prefetch_count}</strong> <span data-i18n-en="prefetch tiles" data-i18n-zh="个预热地图块">prefetch tiles</span></span>
              <span class="hud-chip"><strong>{map_live_event_count}</strong> <span data-i18n-en="live events" data-i18n-zh="个实时事件">live events</span> · {map_player_density_mode}</span>
            </div>
            <div id="world-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
            </div>
            <p id="world-map-overlay-status" class="subtitle" data-i18n-en="Active layers: density, regions, tiles, prefetch rings, live events." data-i18n-zh="当前图层：密度、区域、地图块、预热圈、实时事件。">Active layers: density, regions, tiles, prefetch rings, live events.</p>
            <p id="world-map-overlay-legend" class="subtitle" data-i18n-en="Layer legend: regional anchors, active tiles, prefetch rings, live-event pulses." data-i18n-zh="图层说明：区域锚点、活跃地图块、预热探索圈、实时事件脉冲。">Layer legend: regional anchors, active tiles, prefetch rings, live-event pulses.</p>
          </details>
        </div>
        <div id="world-real-map" data-engine="{map_engine_id}" data-provider="{tile_provider}" aria-label="Reality mirror map" data-i18n-aria-label-en="Reality mirror map" data-i18n-aria-label-zh="现实镜像地图"></div>
      </div>
    </section>
    <section>
      <h2 data-i18n-en="World Regions" data-i18n-zh="世界区域">World Regions</h2>
      <div class="grid">{zone_cards}</div>
    </section>
    <section id="world-map-move-panel" class="panel">
      <h2 data-i18n-en="Detailed World Map" data-i18n-zh="详细世界地图">Detailed World Map</h2>
      <p class="subtitle">{current_map_summary}</p>
      <div class="mini-grid">{map_cards}</div>
      <form method="post" action="/world/web/map-move" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <select id="world-map-move-target" name="target">{map_exit_options}</select>
        <button type="submit" data-i18n-en="Move Here" data-i18n-zh="移动到这里">Move Here</button>
      </form>
    </section>
    <section class="play">
      <div id="world-action-console" class="panel">
        <h2 data-i18n-en="World Action Console" data-i18n-zh="世界行动台">World Action Console</h2>
        <p id="world-action-console-status" class="subtitle">{console_note}</p>
        <form method="post" action="/world/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
          <select id="world-action-location" name="location_id">{location_options}</select>
          <textarea id="world-action-body" name="body" data-i18n-value-en="Launch an AI Design Studio for overseas/global players: recruit Agents, complete real global commissions, and convert key opportunities into League quests." data-i18n-value-zh="我要在全球镜像城市建立 AI 设计工坊，招募 Agent，完成海外真实委托，并把关键机会转成 League 任务。">Launch an AI Design Studio for overseas/global players: recruit Agents, complete real global commissions, and convert key opportunities into League quests.</textarea>
          <button type="submit" data-i18n-en="Submit World Action" data-i18n-zh="提交世界行动">Submit World Action</button>
        </form>
      </div>
      <div class="panel">
        <h2 data-i18n-en="World Event Timeline" data-i18n-zh="世界事件时间线">World Event Timeline</h2>
        <ul id="world-event-timeline" class="timeline">{event_items}</ul>
      </div>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Places" data-i18n-zh="地点">Places</h2>
      <div class="mini-grid">{location_cards}</div>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Agent Residents · NPC" data-i18n-zh="Agent 居民 · NPC">Agent Residents · NPC</h2>
      <div class="mini-grid">{entity_cards}</div>
    </section>
    <section id="world-assets-panel" class="panel">
      <h2 data-i18n-en="Character Items" data-i18n-zh="角色道具">Character Items</h2>
      <div class="mini-grid">{asset_cards}</div>
      <form method="post" action="/world/web/asset" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-asset-body" name="body" data-i18n-value-en="Upgrade this world item: strengthen capability, evidence, risk control, action loop, and the next side quest." data-i18n-value-zh="升级这件世界道具：强化能力、证据、风险控制、行动循环和下一条支线。">Upgrade this world item: strengthen capability, evidence, risk control, action loop, and the next side quest.</textarea>
        <button type="submit" data-i18n-en="Upgrade Item" data-i18n-zh="升级道具">Upgrade Item</button>
      </form>
    </section>
    <section id="world-companies-panel" class="panel">
      <h2 data-i18n-en="Studios and Hubs" data-i18n-zh="工坊与据点">Studios and Hubs</h2>
      <div class="mini-grid">{company_cards}</div>
      <form method="post" action="/world/web/company" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-company-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-company-body" name="body" data-i18n-value-en="Launch a global-facing studio hub with this item: define capability, target clients, action loop, evidence, and the first bounty route." data-i18n-value-zh="用这件道具建立面向海外玩家的工坊据点：定义能力、服务对象、行动循环、证据和第一条悬赏路线。">Launch a global-facing studio hub with this item: define capability, target clients, action loop, evidence, and the first bounty route.</textarea>
        <button type="submit" data-i18n-en="Launch Studio" data-i18n-zh="建立工坊">Launch Studio</button>
      </form>
    </section>
    <section id="world-listings-panel" class="panel">
      <h2 data-i18n-en="Hubs and Quest Cards" data-i18n-zh="据点与任务牌">Hubs and Quest Cards</h2>
      <div class="mini-grid">{shop_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{listing_cards}</div>
      <form method="post" action="/world/web/listing" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-listing-company-id" name="company_id" value="{latest_company_id}" placeholder="自动填充或 latest" />
        <textarea id="world-listing-body" name="body" data-i18n-value-en="Publish a global bounty card: specify deliverables, reward logic, evidence package, commitments, risk controls, self-review, and next action." data-i18n-value-zh="发布一个海外可接取的工坊委托：写清成果、赏金逻辑、证据包、承诺、风险控制、自检和下一步行动。">Publish a global bounty card: specify deliverables, reward logic, evidence package, commitments, risk controls, self-review, and next action.</textarea>
        <button type="submit" data-i18n-en="Publish Quest Card" data-i18n-zh="发布任务牌">Publish Quest Card</button>
      </form>
    </section>
    <section id="world-commerce-panel" class="panel">
      <h2 data-i18n-en="Bounties and Adventure Commissions" data-i18n-zh="悬赏与冒险委托">Bounties and Adventure Commissions</h2>
      <p class="subtitle" data-i18n-en="Overseas beta player loop has three steps: accept quest card → submit result → get rating and reward. Advanced route operations remain expandable for beta validation and power users." data-i18n-zh="海外 beta 玩家视角只需要三步：接取任务牌 → 提交成果 → 获得评级与奖励。完整路线操作仍可展开，方便 beta 验证和高级玩家调试。">Overseas beta player loop has three steps: accept quest card → submit result → get rating and reward. Advanced route operations remain expandable for beta validation and power users.</p>
      <div class="world-adventure-steps">
        <div class="world-adventure-step"><b><span data-i18n-en="1 · Accept" data-i18n-zh="1 · 接取任务牌">1 · Accept</span></b><span data-i18n-en="Choose a bounty and turn it into an executable commission." data-i18n-zh="选择一个悬赏，把它变成可执行的冒险委托。">Choose a bounty and turn it into an executable commission.</span></div>
        <div class="world-adventure-step"><b><span data-i18n-en="2 · Submit" data-i18n-zh="2 · 提交成果">2 · Submit</span></b><span data-i18n-en="Submit result package, evidence, risk review, and next action." data-i18n-zh="提交成果包、证据、风险复盘和下一步行动。">Submit result package, evidence, risk review, and next action.</span></div>
        <div class="world-adventure-step"><b><span data-i18n-en="3 · Rate & Reward" data-i18n-zh="3 · 评级领奖励">3 · Rate & Reward</span></b><span data-i18n-en="Pass rating to earn reputation and rewards; otherwise enter revision." data-i18n-zh="通过评级后获得声望与奖励；不通过则进入返工路线。">Pass rating to earn reputation and rewards; otherwise enter revision.</span></div>
      </div>
      <div id="world-purchase-cards-live" class="mini-grid">{purchase_cards}</div>
      <div id="world-work-orders-live" class="mini-grid" style="margin-top:12px">{work_order_cards}</div>
      <details class="dev-details world-route-drawer"><summary data-i18n-en="Expand full route console" data-i18n-zh="展开完整路线操作台">Expand full route console</summary>
        <form id="world-buy-form" method="post" action="/world/web/buy" style="margin-top:16px">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
          <input id="world-buy-listing-id" name="listing_id" value="{latest_listing_id}" placeholder="自动填充或 latest" />
          <textarea id="world-buy-body" name="body" data-i18n-value-en="Accept this quest card and open an adventure commission: confirm deliverables, evidence package, rating standards, risk controls, and next action." data-i18n-value-zh="接取这个任务牌，开启冒险委托：确认成果、证据包、评级标准、风险控制和下一步行动。">Accept this quest card and open an adventure commission: confirm deliverables, evidence package, rating standards, risk controls, and next action.</textarea>
          <button type="submit" data-i18n-en="Accept Quest Card" data-i18n-zh="接取任务牌">Accept Quest Card</button>
        </form>
      <div id="world-work-deliveries-live" class="mini-grid" style="margin-top:12px">{work_delivery_cards}</div>
      <form id="world-work-deliver-form" method="post" action="/world/web/work-deliver" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-deliver-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-deliver-body" name="body" data-i18n-value-en="Result package: deliverable, evidence package, rating checklist, risk review, next action, and self-check notes." data-i18n-value-zh="成果提交包：成果、证据包、评级清单、风险复盘、下一步行动和自检记录。">Result package: deliverable, evidence package, rating checklist, risk review, next action, and self-check notes.</textarea>
        <button type="submit" data-i18n-en="Submit Result" data-i18n-zh="提交成果">Submit Result</button>
      </form>
      <div id="world-work-acceptances-live" class="mini-grid" style="margin-top:12px">{work_acceptance_cards}</div>
      <form id="world-work-accept-form" method="post" action="/world/web/work-accept" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-accept-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-accept-body" name="body" data-i18n-value-en="Rating passed: confirm result evidence, quality note, next side quest, and reputation reward." data-i18n-value-zh="评级通过：确认成果证据、质量备注、下一条支线和声望奖励。">Rating passed: confirm result evidence, quality note, next side quest, and reputation reward.</textarea>
        <button type="submit" data-i18n-en="Pass Rating" data-i18n-zh="评级通过">Pass Rating</button>
      </form>
      <div id="world-work-rejections-live" class="mini-grid" style="margin-top:12px">{work_rejection_cards}</div>
      <form id="world-work-reject-form" method="post" action="/world/web/work-reject" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reject-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reject-body" name="body" data-i18n-value-en="Revision required: record failure reason, evidence gap, reward refund, revision requirement, and next action." data-i18n-value-zh="需要返工：记录未通过原因、证据缺口、奖励退回、返工要求和下一步行动。">Revision required: record failure reason, evidence gap, reward refund, revision requirement, and next action.</textarea>
        <button type="submit" data-i18n-en="Request Revision" data-i18n-zh="要求返工">Request Revision</button>
      </form>
      <div id="world-work-reopens-live" class="mini-grid" style="margin-top:12px">{work_reopen_cards}</div>
      <form id="world-work-reopen-form" method="post" action="/world/web/work-reopen" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reopen-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reopen-body" name="body" data-i18n-value-en="Reopen commission: escrow reward again, list revision requirements, evidence gaps, rating standards, and resubmission action." data-i18n-value-zh="重开委托：重新托管奖励，列出返工要求、证据缺口、评级标准和再次提交行动。">Reopen commission: escrow reward again, list revision requirements, evidence gaps, rating standards, and resubmission action.</textarea>
        <button type="submit" data-i18n-en="Reopen Commission" data-i18n-zh="重开委托">Reopen Commission</button>
      </form>
      <div id="world-work-cancellations-live" class="mini-grid" style="margin-top:12px">{work_cancellation_cards}</div>
      <form id="world-work-cancel-form" method="post" action="/world/web/work-cancel" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-cancel-id" name="work_order_id" value="{latest_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-cancel-body" name="body" data-i18n-value-en="Cancel commission: end the route before result submission, refund escrowed reward, record reason, and close the commission." data-i18n-value-zh="放弃委托：在成果提交前结束路线、退回托管奖励、记录原因并关闭委托。">Cancel commission: end the route before result submission, refund escrowed reward, record reason, and close the commission.</textarea>
        <button type="submit" data-i18n-en="Cancel Commission" data-i18n-zh="放弃委托">Cancel Commission</button>
      </form>
      </details>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Faction Reputation Map" data-i18n-zh="阵营声望图">Faction Reputation Map</h2>
      <div class="mini-grid">{faction_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{standing_cards}</div>
    </section>
    <section id="world-contracts-panel" class="panel">
      <h2 data-i18n-en="World Contracts" data-i18n-zh="世界契约">World Contracts</h2>
      <div id="world-contract-cards-live" class="mini-grid">{contract_cards}</div>
      <form id="world-contract-completion-form" method="post" action="/world/web/contract" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-contract-completion-id" name="contract_id" value="{latest_contract_id}" placeholder="自动填充或契约 ID" />
        <textarea id="world-contract-completion-body" name="body" data-i18n-value-en="World contract report: result, evidence, risk review, next step, and rating standards." data-i18n-value-zh="世界契约战报：成果、证据、风险复盘、下一步和评级标准。">World contract report: result, evidence, risk review, next step, and rating standards.</textarea>
        <button type="submit" data-i18n-en="Complete Contract" data-i18n-zh="完成契约">Complete Contract</button>
      </form>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Quest Route Graph" data-i18n-zh="任务路线图">Quest Route Graph</h2>
      <div id="world-route-task-graph-live" class="mini-grid">{world_route_task_graph_cards}</div>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Playable Commands" data-i18n-zh="可玩指令">Playable Commands</h2>
      <p class="subtitle"><code>/world</code> <code data-i18n-en="/world action Launch an AI Design Studio" data-i18n-zh="/world action 我要开一家 AI 设计工坊">/world action Launch an AI Design Studio</code> <code>/league</code> <code>/arena</code> <code>/guild</code> <code>/raid</code></p>
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
          return `<article class="mini task-graph"><strong>${{escapeHtml(mapText(task.task_id || 'route task / 路线任务'))}}</strong><span>${{escapeHtml(mapText(task.latest_bucket || 'event'))}} · ${{escapeHtml(mapText(task.latest_status || 'pending'))}} · ${{escapeHtml(mapText('branch / 支线'))}} ${{escapeHtml(mapText(task.next_opportunity_kind || 'contract_capture'))}}</span><code>${{escapeHtml(mapText(task.latest_location_id || task.task_id || 'route'))}}</code><small>${{escapeHtml(task.event_count ?? 0)}} ${{escapeHtml(mapText('events / 事件'))}} · ${{escapeHtml(task.contract_count ?? 0)}} ${{escapeHtml(mapText('contracts / 契约'))}} · ${{escapeHtml(task.completion_count ?? 0)}} ${{escapeHtml(mapText('battle reports / 战报'))}}</small><small>${{escapeHtml(mapText(task.outcome_summary || '战果总结待生成。'))}}</small><small><strong>${{escapeHtml(mapText('next branch / 下一条支线'))}}</strong> · ${{escapeHtml(mapText(task.next_opportunity_hint || '支线提示待生成。'))}}</small><div class="focus-stack"><code>${{escapeHtml(mapText(task.next_opportunity_command || '/world action 继续推进下一步机会。'))}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
        }}).join('') : '<article class="mini task-graph"><strong>' + escapeHtml(mapText('No task-linked routes yet / 还没有任务路线')) + '</strong><span>' + escapeHtml(mapText('Create a world contract or task event to grow the route graph. / 创建世界契约或任务事件后，路线图会生长出来。')) + '</span><code>task graph</code></article>';
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
        current_map_summary = current_map_summary,
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
