use super::*;

fn client_app_visible_copy(value: &str) -> String {
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
        (
            "browser commerce E2E",
            "browser adventure E2E / 浏览器冒险验收",
        ),
        ("AI 设计公司", "AI Design Studio / AI 设计工坊"),
        (
            "服务真实客户",
            "serve real global clients / 完成海外真实委托",
        ),
        ("真实客户", "real global client / 海外真实委托"),
        ("委托方", "client / 委托目标"),
        ("委托目标", "client goal"),
        ("成果标准", "deliverable standard"),
        ("评级规则", "rating rules"),
        ("成果内容", "deliverable"),
        ("证据包", "evidence pack"),
        ("评级清单", "rating checklist"),
    ];
    if let Some((_, to)) = replacements.iter().find(|(from, _)| value == *from) {
        return (*to).to_string();
    }
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
    let copy = client_app_visible_copy(value);
    i18n_span_from_bilingual_slash_copy(&copy).unwrap_or_else(|| escape_html_text(&copy))
}

fn client_app_readiness_label(value: &str) -> String {
    match value {
        "first_playable_loop_100" | "global_first_playable_loop_100" => {
            "Global first playable 100% / 新手主线 100%".to_string()
        }
        "map_focus_visible" => "Map focus visible / 地图焦点可见".to_string(),
        "world_event_created" => "World event created / 世界事件已创建".to_string(),
        "contract_open_or_completed" => "Contract open or completed / 契约已开启或完成".to_string(),
        "quest_work_order_created" => "Quest commission created / 冒险委托已创建".to_string(),
        "quest_rating_or_feedback_loop_visible" => {
            "Rating and revision route visible / 评级与返工路线可见".to_string()
        }
        "wallet_progression_feed_updated" => {
            "Reward progression feed updated / 奖励成长动态已更新".to_string()
        }
        "route_task_graph_next_action_visible" => {
            "Route next action visible / 路线下一步可见".to_string()
        }
        "playability_coach_visible" => "Playability coach visible / 可玩性教练可见".to_string(),
        "p0_next_best_action_visible" => "P0 next-best action visible / P0 下一步可见".to_string(),
        "p1_strategy_choices_visible" => {
            "P1 strategy choices visible / P1 策略选择可见".to_string()
        }
        "p2_retention_telemetry_visible" => {
            "P2 retention telemetry visible / P2 留存观测可见".to_string()
        }
        "failure_recovery_lane_visible" => {
            "Failure recovery lane visible / 失败恢复路线可见".to_string()
        }
        "economy_tradeoff_cards_visible" => {
            "Economy tradeoff cards visible / 经济取舍卡可见".to_string()
        }
        "retention_calendar_visible" => "Retention calendar visible / 留存日历可见".to_string(),
        "playability_funnel_visible" => "Playability funnel visible / 可玩性漏斗可见".to_string(),
        "anti_cheese_policy_visible" => "Anti-cheese policy visible / 反刷策略可见".to_string(),
        "ops_refresh_hooks_visible" => "Ops refresh hooks visible / 运营刷新钩子可见".to_string(),
        "visible" => "Visible / 可见".to_string(),
        "ready" => "Ready / 已准备".to_string(),
        _ => client_app_visible_copy(value),
    }
}

fn client_app_first_tactics_route_task(app: &Value) -> Option<&Value> {
    app.get("map_hub")
        .and_then(|hub| hub.get("route_task_graph"))
        .and_then(|graph| graph.get("tasks"))
        .and_then(Value::as_array)
        .and_then(|tasks| {
            tasks.iter().find(|task| {
                task.get("latest_bucket").and_then(Value::as_str) == Some("tactics_objective")
                    || task
                        .get("task_id")
                        .and_then(Value::as_str)
                        .map(|task_id| task_id.starts_with("tactics-objective:"))
                        .unwrap_or(false)
                    || task.get("tactics_route_task_binding").is_some()
            })
        })
}

fn client_app_tactics_repeat_block_count(tactics_board: &Value, binding: Option<&Value>) -> u64 {
    binding
        .and_then(|binding| binding.get("repeat_farming"))
        .and_then(|repeat| repeat.get("blocked_attempt_count"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            tactics_board
                .get("simulation_ticks")
                .and_then(Value::as_array)
                .map(|ticks| {
                    ticks
                        .iter()
                        .filter(|tick| {
                            tick.get("command").and_then(Value::as_str) == Some("attack")
                                && tick.get("outcome_accepted").and_then(Value::as_bool)
                                    == Some(false)
                                && tick.get("outcome_result").and_then(Value::as_str)
                                    == Some("repeat_farming_blocked")
                        })
                        .count() as u64
                })
                .unwrap_or(0)
        })
}

fn client_app_tactics_reward_history_cards_html(
    session: &Value,
    route_task: Option<&Value>,
    binding: Option<&Value>,
) -> String {
    let reward_history = route_task
        .and_then(|task| task.get("tactics_reward_history"))
        .and_then(Value::as_array)
        .or_else(|| {
            binding
                .and_then(|binding| binding.get("reward_history"))
                .and_then(Value::as_array)
        });
    if let Some(history) = reward_history {
        let cards = history
            .iter()
            .take(4)
            .map(|entry| {
                let stage = entry
                    .get("stage")
                    .and_then(Value::as_str)
                    .unwrap_or("reward_stage");
                let status = entry
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending");
                let label = entry
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(stage);
                let summary = entry
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("Reward history waits for server settlement.");
                format!(
                    "<article class=\"module tactics-reward-history-stage\" data-history-stage=\"{}\" data-history-status=\"{}\"><strong>{}</strong><span>{}</span><small>{}</small></article>",
                    escape_html_text(stage),
                    escape_html_text(status),
                    escape_client_app_visible_text(label),
                    escape_client_app_visible_text(status),
                    escape_client_app_visible_text(summary),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !cards.trim().is_empty() {
            return cards;
        }
    }
    let objective_progress = session
        .get("objective_progress")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let objective_goal = session
        .get("objective_goal")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let victory_state = session
        .get("victory_state")
        .and_then(Value::as_str)
        .unwrap_or("active");
    let reward_status = session
        .get("reward_status")
        .and_then(Value::as_str)
        .unwrap_or("not_eligible");
    format!(
        "<article class=\"module tactics-reward-history-stage\" data-history-stage=\"objective_progress\" data-history-status=\"{}\"><strong data-i18n-en=\"Tactics objective\" data-i18n-zh=\"战棋目标\">Tactics objective</strong><span>Progress {}/{}</span><small data-i18n-en=\"Rust session owns objective progress.\" data-i18n-zh=\"Rust 会话拥有目标进度。\">Rust session owns objective progress.</small></article>\n<article class=\"module tactics-reward-history-stage\" data-history-stage=\"victory_state\" data-history-status=\"{}\"><strong data-i18n-en=\"Victory state\" data-i18n-zh=\"胜负状态\">Victory state</strong><span>{}</span><small data-i18n-en=\"Browser only shows the result; Rust resolves combat.\" data-i18n-zh=\"浏览器只展示结果；Rust 结算战斗。\">Browser only shows the result; Rust resolves combat.</small></article>\n<article class=\"module tactics-reward-history-stage\" data-history-stage=\"reward_settlement\" data-history-status=\"{}\"><strong data-i18n-en=\"Reward settlement\" data-i18n-zh=\"奖励结算\">Reward settlement</strong><span>{}</span><small data-i18n-en=\"Route-runner history unlocks after server settlement.\" data-i18n-zh=\"服务器结算后解锁路线角色历史。\">Route-runner history unlocks after server settlement.</small></article>",
        if objective_progress >= objective_goal { "completed" } else { "in_progress" },
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_client_app_visible_text(victory_state),
        escape_html_text(reward_status),
        escape_client_app_visible_text(reward_status),
    )
}

fn client_app_tactics_player_hud_html(app: &Value) -> String {
    let null_value = Value::Null;
    let tactics_board = app
        .get("map")
        .and_then(|map| map.get("tactics_board"))
        .unwrap_or(&null_value);
    let session = tactics_board.get("game_session").unwrap_or(&Value::Null);
    let route_task = client_app_first_tactics_route_task(app);
    let binding = route_task.and_then(|task| task.get("tactics_route_task_binding"));
    let session_contract = tactics_board
        .get("tactics_game_session_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_game_session_v1");
    let tick_contract = tactics_board
        .get("tactics_simulation_tick_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_simulation_tick_v1");
    let reward_contract = tactics_board
        .get("tactics_reward_settlement_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_reward_settlement_v1");
    let anti_cheese_contract = tactics_board
        .get("tactics_repeat_farming_anti_cheese_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_repeat_farming_anti_cheese_v1");
    let reward_history_contract = route_task
        .and_then(|task| task.get("tactics_reward_history_contract_version"))
        .and_then(Value::as_str)
        .or_else(|| {
            binding
                .and_then(|binding| binding.get("reward_history_contract_version"))
                .and_then(Value::as_str)
        })
        .unwrap_or("trillionnium_tactics_reward_history_v1");
    let session_id = session
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("world-tactics-session:projected");
    let matrix_user_id = session
        .get("matrix_user_id")
        .and_then(Value::as_str)
        .unwrap_or("@alice:local.dev");
    let objective_id = session
        .get("objective_id")
        .and_then(Value::as_str)
        .unwrap_or("objective:projected");
    let objective = tactics_board
        .get("objectives")
        .and_then(Value::as_array)
        .and_then(|objectives| {
            objectives.iter().find(|objective| {
                objective.get("objective_id").and_then(Value::as_str) == Some(objective_id)
            })
        });
    let objective_label = objective
        .and_then(|objective| objective.get("label"))
        .and_then(Value::as_str)
        .unwrap_or("Real-street objective");
    let objective_command = objective
        .and_then(|objective| objective.get("suggested_command"))
        .and_then(Value::as_str)
        .unwrap_or("attack");
    let objective_progress = session
        .get("objective_progress")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let objective_goal = session
        .get("objective_goal")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let victory_state = session
        .get("victory_state")
        .and_then(Value::as_str)
        .unwrap_or("active");
    let reward_status = session
        .get("reward_status")
        .and_then(Value::as_str)
        .unwrap_or("not_eligible");
    let active_unit = session
        .get("active_unit_id")
        .and_then(Value::as_str)
        .unwrap_or("lord");
    let active_overlay = session
        .get("active_overlay_id")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium-world-node:mirror-city-square");
    let action_points = session
        .get("action_points_remaining")
        .and_then(Value::as_i64)
        .unwrap_or(2);
    let current_tick = session
        .get("current_tick")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let route_task_id = route_task
        .and_then(|task| task.get("task_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            binding
                .and_then(|binding| binding.get("route_task_id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("tactics-objective:{matrix_user_id}:{objective_id}"));
    let reward_history_summary = binding
        .and_then(|binding| binding.get("reward_history_summary"))
        .and_then(Value::as_str)
        .or_else(|| {
            route_task
                .and_then(|task| task.get("reward_history_summary"))
                .and_then(Value::as_str)
        })
        .unwrap_or(if reward_status == "settled" {
            "Tactics reward settled and visible in route-runner history."
        } else {
            "Tactics reward history is waiting for victory settlement."
        });
    let repeat_block_count = client_app_tactics_repeat_block_count(tactics_board, binding);
    let repeat_copy_en = if repeat_block_count > 0 {
        format!(
            "Repeat farming blocked: {repeat_block_count} extra attack intent(s) were rejected after the settled reward."
        )
    } else if victory_state == "victory" && reward_status == "settled" {
        "Repeat farming guard armed: further attacks on this settled objective will be blocked."
            .to_string()
    } else {
        "Repeat farming guard watches settlement; one tactics objective/session can release reward once.".to_string()
    };
    let repeat_copy_zh = if repeat_block_count > 0 {
        format!("反刷已拦截：奖励结算后又拒绝了 {repeat_block_count} 次攻击意图。")
    } else if victory_state == "victory" && reward_status == "settled" {
        "反刷守卫已开启：这个已结算目标上的后续攻击会被拦截。".to_string()
    } else {
        "反刷守卫等待结算：每个战棋目标/会话只释放一次奖励。".to_string()
    };
    let reward_history_cards =
        client_app_tactics_reward_history_cards_html(session, route_task, binding);
    format!(
        "<section id=\"app-tactics-player-hud\" class=\"module app-tactics-player-hud\" data-contract-version=\"trillionnium_tactics_player_visible_surface_v1\" data-surface=\"app\" data-source-of-truth=\"rust_world_tactics_sessions\" data-web-role=\"visualization_input_only\">\n  <strong data-i18n-en=\"Tactics objective\" data-i18n-zh=\"战棋目标\">Tactics objective</strong>\n  <article id=\"app-tactics-objective-card\" class=\"module tactics-objective-card\" data-session-contract=\"{}\" data-objective-id=\"{}\" data-route-task-id=\"{}\" data-objective-progress=\"{}\" data-objective-goal=\"{}\" data-victory-state=\"{}\" data-reward-status=\"{}\"><strong data-i18n-en=\"Current tactics objective\" data-i18n-zh=\"当前战棋目标\">Current tactics objective</strong><span>{}</span><small>progress {}/{} · command {}</small><code>{}</code></article>\n  <article id=\"app-tactics-current-session-card\" class=\"module tactics-session-card\" data-session-id=\"{}\" data-active-unit-id=\"{}\" data-active-overlay-id=\"{}\" data-current-tick=\"{}\" data-action-points-remaining=\"{}\" data-tick-contract=\"{}\"><strong data-i18n-en=\"Current session state\" data-i18n-zh=\"当前会话状态\">Current session state</strong><span>unit {} · AP {} · tick {}</span><small>{} · reward {}</small></article>\n  <article id=\"app-tactics-reward-history-handoff\" class=\"module tactics-reward-history-card\" data-reward-history-contract=\"{}\" data-reward-contract=\"{}\" data-route-task-id=\"{}\" data-reward-status=\"{}\"><strong data-i18n-en=\"Reward-history handoff\" data-i18n-zh=\"奖励历史交接\">Reward-history handoff</strong><span>{}</span><div class=\"grid\">{}</div></article>\n  <article id=\"app-tactics-repeat-farming-copy\" class=\"module tactics-anti-cheese-card\" data-anti-cheese-contract=\"{}\" data-repeat-farming-block-count=\"{}\" data-result=\"{}\" data-gate-owner=\"rust_tactics_repeat_farming_guard\"><strong data-i18n-en=\"Repeat-farming guard\" data-i18n-zh=\"反刷守卫\">Repeat-farming guard</strong><span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span><small data-i18n-en=\"Browser submits intent only; Rust blocks settled reward farming.\" data-i18n-zh=\"浏览器只提交意图；Rust 拦截已结算奖励的重复刷取。\">Browser submits intent only; Rust blocks settled reward farming.</small></article>\n</section>",
        escape_html_text(session_contract),
        escape_html_text(objective_id),
        escape_html_text(&route_task_id),
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_html_text(reward_status),
        escape_client_app_visible_text(objective_label),
        objective_progress,
        objective_goal,
        escape_client_app_visible_text(objective_command),
        escape_html_text(&route_task_id),
        escape_html_text(session_id),
        escape_html_text(active_unit),
        escape_html_text(active_overlay),
        current_tick,
        action_points,
        escape_html_text(tick_contract),
        escape_html_text(active_unit),
        action_points,
        current_tick,
        escape_html_text(victory_state),
        escape_html_text(reward_status),
        escape_html_text(reward_history_contract),
        escape_html_text(reward_contract),
        escape_html_text(&route_task_id),
        escape_html_text(reward_status),
        escape_client_app_visible_text(reward_history_summary),
        reward_history_cards,
        escape_html_text(anti_cheese_contract),
        repeat_block_count,
        if repeat_block_count > 0 { "repeat_farming_blocked" } else { "repeat_farming_watch" },
        escape_html_text(&repeat_copy_en),
        escape_html_text(&repeat_copy_zh),
        escape_html_text(&repeat_copy_en),
    )
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
                "<article class=\"module\"><strong>{}</strong><span>{} · {}</span><p data-i18n-en=\"Contact entry for messages, collaboration, contract progress, and world actions.\" data-i18n-zh=\"消息、协作、契约推进与世界行动的联系人入口。\">Contact entry for messages, collaboration, contract progress, and world actions.</p><code>{}</code></article>",
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
                "<article class=\"module\"><strong data-i18n-en=\"Task Thread\" data-i18n-zh=\"任务线程\">Task Thread</strong><span>{} · {} · {}</span><p>{}</p><code>{}</code></article>",
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
        "<article class=\"module\"><strong data-i18n-en=\"Messages\" data-i18n-zh=\"消息\">Messages</strong><span data-i18n-en=\"Chat room loop\" data-i18n-zh=\"聊天房间循环\">Chat room loop</span><p data-i18n-en=\"Contacts, Agents, notifications, contract threads, and task collaboration entries appear here.\" data-i18n-zh=\"这里会显示联系人、Agent、通知、契约线程和任务协作入口。\">Contacts, Agents, notifications, contract threads, and task collaboration entries appear here.</p><code>/social</code></article>".to_string()
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
    let map_product_name = map_engine
        .and_then(|engine| engine.get("product_name"))
        .and_then(Value::as_str)
        .unwrap_or("Trillionnium World Map");
    let map_upgrade_model = map_engine
        .and_then(|engine| engine.get("gameplay_layer_contract"))
        .and_then(|contract| contract.get("upgrade_model"))
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap upgraded with Trillionnium avatars, route nodes, quest cards, live events, and task completion loops.");
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
    let map_player_avatar_count = map_hub
        .and_then(|hub| hub.get("player_avatar_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_avatar_task_route_count = map_hub
        .and_then(|hub| hub.get("avatar_task_route_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_avatar_route_runner_count = map_hub
        .and_then(|hub| hub.get("avatar_route_runner_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_player_density_mode = map_hub
        .and_then(|hub| hub.get("player_density_mode"))
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let route_runner_handoff = map_hub.and_then(|hub| hub.get("route_runner_handoff"));
    let map_route_runner_handoff_summary = route_runner_handoff
        .and_then(|handoff| handoff.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("Route runner handoff: waiting for avatar task routes to unlock reward and next-route actions.");
    let map_route_runner_next_route_status = route_runner_handoff
        .and_then(|handoff| handoff.get("first_next_route_status"))
        .and_then(Value::as_str)
        .unwrap_or("next_route_preview_locked_until_reward_claim");
    let map_route_runner_reward_claim_count = route_runner_handoff
        .and_then(|handoff| handoff.get("reward_claim_action_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_route_runner_next_route_count = route_runner_handoff
        .and_then(|handoff| handoff.get("next_route_action_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
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
    let onboarding_quick_path_label = onboarding
        .and_then(|rail| rail.get("quick_path_label"))
        .and_then(Value::as_str)
        .unwrap_or("Quick Path");
    let onboarding_quick_path_label_zh = onboarding
        .and_then(|rail| rail.get("quick_path_label_zh"))
        .and_then(Value::as_str)
        .unwrap_or("快速路径");
    let onboarding_quick_path_summary = onboarding
        .and_then(|rail| rail.get("quick_path_summary"))
        .and_then(Value::as_str)
        .unwrap_or("Choose map focus → run one bounty → submit/review reward");
    let onboarding_quick_path_summary_zh = onboarding
        .and_then(|rail| rail.get("quick_path_summary_zh"))
        .and_then(Value::as_str)
        .unwrap_or("选择地图焦点 → 跑一个悬赏 → 提交/查看奖励");
    let onboarding_command_disclosure_label = onboarding
        .and_then(|rail| rail.get("command_disclosure_label"))
        .and_then(Value::as_str)
        .unwrap_or("Full Commands");
    let onboarding_command_disclosure_label_zh = onboarding
        .and_then(|rail| rail.get("command_disclosure_label_zh"))
        .and_then(Value::as_str)
        .unwrap_or("完整命令");
    let onboarding_command_disclosure = onboarding
        .and_then(|rail| rail.get("command_disclosure"))
        .and_then(Value::as_str)
        .unwrap_or("Use these when you are ready to submit real work with deliverable, evidence, risk controls, next action, and self-review anchors.");
    let onboarding_command_disclosure_zh = onboarding
        .and_then(|rail| rail.get("command_disclosure_zh"))
        .and_then(Value::as_str)
        .unwrap_or(
            "准备真实提交时再展开：每条命令都要带交付物、证据、风险控制、下一步和自检锚点。",
        );
    let onboarding_quick_path_label_html = format!(
        "<strong data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</strong>",
        escape_html_text(onboarding_quick_path_label),
        escape_html_text(onboarding_quick_path_label_zh),
        escape_html_text(onboarding_quick_path_label)
    );
    let onboarding_quick_path_summary_html = format!(
        "<span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span>",
        escape_html_text(onboarding_quick_path_summary),
        escape_html_text(onboarding_quick_path_summary_zh),
        escape_html_text(onboarding_quick_path_summary)
    );
    let onboarding_quick_path_step_items = onboarding
        .and_then(|rail| rail.get("quick_path_steps"))
        .and_then(Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .map(|step| {
                    let label = step.get("label").and_then(Value::as_str).unwrap_or("Next step");
                    let label_zh = step
                        .get("label_zh")
                        .and_then(Value::as_str)
                        .unwrap_or(label);
                    let description = step
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("Continue the first playable loop.");
                    let description_zh = step
                        .get("description_zh")
                        .and_then(Value::as_str)
                        .unwrap_or(description);
                    format!(
                        "<li><b data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</b><span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span></li>",
                        escape_html_text(label),
                        escape_html_text(label_zh),
                        escape_html_text(label),
                        escape_html_text(description),
                        escape_html_text(description_zh),
                        escape_html_text(description),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            [
                (
                    "1 · Choose map focus",
                    "1 · 选择地图焦点",
                    "Tap a city place, region, or live event.",
                    "点选城市地点、区域或实时事件。",
                ),
                (
                    "2 · Run one bounty",
                    "2 · 跑一个悬赏",
                    "Start the first world action and capture it as a rated commission.",
                    "发起第一次世界行动，并登记为待评级委托。",
                ),
                (
                    "3 · Submit / review reward",
                    "3 · 提交 / 查看奖励",
                    "Deliver evidence, check rating, reward, and next route.",
                    "提交证据，查看评级、奖励和下一步路线。",
                ),
            ]
            .into_iter()
            .map(|(label, label_zh, description, description_zh)| {
                format!(
                    "<li><b data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</b><span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span></li>",
                    escape_html_text(label),
                    escape_html_text(label_zh),
                    escape_html_text(label),
                    escape_html_text(description),
                    escape_html_text(description_zh),
                    escape_html_text(description),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
        });
    let onboarding_command_disclosure_summary_html = format!(
        "<summary data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</summary>",
        escape_html_text(onboarding_command_disclosure_label),
        escape_html_text(onboarding_command_disclosure_label_zh),
        escape_html_text(onboarding_command_disclosure_label)
    );
    let onboarding_command_disclosure_copy_html = format!(
        "<p class=\"subtitle\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</p>",
        escape_html_text(onboarding_command_disclosure),
        escape_html_text(onboarding_command_disclosure_zh),
        escape_html_text(onboarding_command_disclosure)
    );
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
                "<article class=\"module onboarding-step\" data-onboarding-step=\"{}\"><strong>{}</strong><span>{} · {}</span><p>{}</p><code>{}</code><p class=\"subtitle\"><span data-i18n-en=\"Success signal:\" data-i18n-zh=\"完成信号：\">Success signal:</span> <code>{}</code></p></article>",
                escape_html_text(step_id),
                escape_client_app_visible_text(label),
                escape_html_text(surface),
                escape_client_app_visible_text(&client_app_readiness_label(status)),
                escape_client_app_visible_text(description),
                escape_client_app_visible_text(&client_app_readiness_label(command)),
                escape_client_app_visible_text(&client_app_readiness_label(success_signal)),
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
                escape_client_app_visible_text(&client_app_readiness_label(&check))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let playability_coach = app.get("playability_coach");
    let playability_coach_version = playability_coach
        .and_then(|coach| coach.get("contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_playability_coach_v1");
    let playability_coach_lane_cards = playability_coach
        .and_then(|coach| coach.get("lanes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(3)
        .map(|lane| {
            let lane_id = lane.get("lane_id").and_then(Value::as_str).unwrap_or("lane");
            let priority = lane.get("priority").and_then(Value::as_str).unwrap_or("P0");
            let label = lane
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Playability lane / 可玩性路线");
            let player_goal = lane
                .get("player_goal")
                .and_then(Value::as_str)
                .unwrap_or("Keep the next action, tradeoff, and return reason visible.");
            let cta_label = lane
                .get("cta_label")
                .and_then(Value::as_str)
                .unwrap_or("Continue / 继续");
            let command = lane
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("/app");
            let command_preview = match lane_id {
                "p0_first_session" => "/world action <goal + evidence + risk + next>",
                "p1_strategy_depth" => "/world action compare market/faction/guild/recovery",
                "p2_retention_ops" => "/progression plan next unlock + weekly raid",
                _ => command,
            };
            format!(
                "<article class=\"coach-card\" data-playability-lane=\"{}\"><span>{}</span><strong>{}</strong><p>{}</p><code>{}</code><a class=\"quest-cta coach-cta\" href=\"/world\">{}</a></article>",
                escape_html_text(lane_id),
                escape_html_text(priority),
                escape_client_app_visible_text(label),
                escape_client_app_visible_text(player_goal),
                escape_html_text(command_preview),
                escape_client_app_visible_text(cta_label),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let economy_retention_ops = app
        .get("economy_retention_ops")
        .or_else(|| playability_coach.and_then(|coach| coach.get("economy_retention_ops")));
    let economy_tradeoff_cards = economy_retention_ops
        .and_then(|ops| ops.get("economy_tradeoff_cards"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(4)
        .map(|card| {
            let card_id = card.get("card_id").and_then(Value::as_str).unwrap_or("tradeoff");
            let label = card
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Economy tradeoff / 经济取舍");
            let upside = card.get("upside").and_then(Value::as_str).unwrap_or("upside");
            let risk = card.get("risk").and_then(Value::as_str).unwrap_or("risk");
            let live_count = card.get("live_count").and_then(Value::as_i64).unwrap_or(0);
            format!(
                "<article class=\"economy-card\" data-economy-tradeoff=\"{}\"><strong>{}</strong><span>{}</span><small>{} · live {}</small></article>",
                escape_html_text(card_id),
                escape_client_app_visible_text(label),
                escape_html_text(upside),
                escape_html_text(risk),
                live_count,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let economy_ops_chips = economy_retention_ops
        .and_then(|ops| ops.get("readiness_checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(8)
        .map(|check| {
            let label = check
                .as_str()
                .map(client_app_readiness_label)
                .unwrap_or_else(|| "Ready / 已准备".to_string());
            format!("<span>{}</span>", escape_client_app_visible_text(&label))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let route_runner_funnel_telemetry =
        economy_retention_ops.and_then(|ops| ops.get("route_runner_funnel_telemetry"));
    let route_runner_funnel_contract = route_runner_funnel_telemetry
        .and_then(|telemetry| telemetry.get("contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_route_runner_funnel_telemetry_v1");
    let route_runner_funnel_time_to_reward = route_runner_funnel_telemetry
        .and_then(|telemetry| telemetry.get("time_to_reward"))
        .and_then(|time| time.get("p50_seconds"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let route_runner_funnel_daily_resume_count = route_runner_funnel_telemetry
        .and_then(|telemetry| telemetry.get("event_counts"))
        .and_then(|counts| counts.get("daily_return_resume"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cohort_quality =
        route_runner_funnel_telemetry.and_then(|telemetry| telemetry.get("cohort_quality"));
    let reward_to_next_route_percent = cohort_quality
        .and_then(|cohort| cohort.get("reward_to_next_route_conversion_percent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let d1_resume_percent = cohort_quality
        .and_then(|cohort| cohort.get("d1_resume_rate_percent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let abandon_or_recovery_percent = cohort_quality
        .and_then(|cohort| cohort.get("route_abandon_or_recovery_rate_percent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let time_to_first_proof_seconds = cohort_quality
        .and_then(|cohort| cohort.get("time_to_first_proof_seconds"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let time_to_next_route_seconds = cohort_quality
        .and_then(|cohort| cohort.get("time_to_next_route_seconds"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let route_runner_funnel_chips = route_runner_funnel_telemetry
        .and_then(|telemetry| telemetry.get("event_counts"))
        .and_then(Value::as_object)
        .map(|counts| {
            [
                ("route_started", "routes started"),
                ("evidence_submitted", "evidence submitted"),
                ("reward_claimed", "rewards claimed"),
                ("next_route_opened", "next routes opened"),
                ("abandoned_or_recovery", "recoveries"),
                ("daily_return_resume", "daily resumes"),
            ]
            .iter()
            .map(|(key, label)| {
                let count = counts.get(*key).and_then(Value::as_i64).unwrap_or(0);
                format!(
                    "<span data-funnel-event=\"{}\">{} · {}</span>",
                    escape_html_text(key),
                    escape_html_text(label),
                    count,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
        })
        .unwrap_or_default();
    let commercial_operating_dashboard =
        economy_retention_ops.and_then(|ops| ops.get("commercial_operating_dashboard"));
    let commercial_dashboard_chips = commercial_operating_dashboard
        .map(|dashboard| {
            [
                (
                    "route_start_to_paid_task_conversion_percent",
                    "route → paid task",
                    "%",
                ),
                (
                    "reward_claim_to_next_commission_percent",
                    "reward → next commission",
                    "%",
                ),
                ("seller_completion_quality_percent", "seller quality", "%"),
                ("buyer_repeat_order_count", "buyer repeats", ""),
                ("dispute_refund_reopen_count", "disputes/reopens", ""),
            ]
            .iter()
            .map(|(key, label, suffix)| {
                let value = dashboard.get(*key).and_then(Value::as_i64).unwrap_or(0);
                format!(
                    "<span data-commercial-metric=\"{}\">{} · {}{}</span>",
                    escape_html_text(key),
                    escape_html_text(label),
                    value,
                    suffix,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
        })
        .unwrap_or_default();
    let route_archetype_cards = economy_retention_ops
        .and_then(|ops| ops.get("route_archetypes"))
        .and_then(|catalog| catalog.get("archetypes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(5)
        .map(|archetype| {
            let archetype_id = archetype
                .get("archetype_id")
                .and_then(Value::as_str)
                .unwrap_or("route");
            let label = archetype
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Route archetype");
            let promise = archetype
                .get("player_promise")
                .and_then(Value::as_str)
                .unwrap_or("different route meaning");
            let cta = archetype
                .get("primary_cta_copy")
                .and_then(Value::as_str)
                .unwrap_or("Run route");
            format!(
                "<article class=\"economy-card\" data-route-archetype=\"{}\"><strong>{}</strong><span>{}</span><small>{}</small></article>",
                escape_html_text(archetype_id),
                escape_client_app_visible_text(label),
                escape_html_text(promise),
                escape_html_text(cta),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let funnel_percent = economy_retention_ops
        .and_then(|ops| ops.get("playability_funnel"))
        .and_then(|funnel| funnel.get("completion_percent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let retention_reason = economy_retention_ops
        .and_then(|ops| ops.get("retention_calendar"))
        .and_then(|calendar| calendar.get("return_reason"))
        .and_then(Value::as_str)
        .unwrap_or("Queued route payoff, market movement, raid window, and next unlock.");
    let app_tactics_player_hud = client_app_tactics_player_hud_html(&app);
    let app_bootstrap = trillionnium_slim_map_bootstrap_json(&app, "client_app_web_shell");
    let app_bootstrap_bytes = serde_json::to_string(&app_bootstrap)
        .map(|value| value.len())
        .unwrap_or(0);
    let app_data_json = serde_json::to_string(&app_bootstrap)
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
        "Filter by Focus",
        "按焦点筛选路线",
        "Show Full Route",
        "显示完整路线",
    );
    let shared_map_route_target_resolution_js = real_world_map_route_target_resolution_js();
    let shared_map_route_status_js = real_world_map_route_status_js();
    let shared_map_route_contract_js = real_world_map_route_contract_js();
    let shared_map_route_action_js = real_world_map_route_action_js();
    let shared_map_viewport_hydration_js = real_world_map_viewport_hydration_js();
    let shared_map_render_cards_js =
        real_world_map_render_cards_js(RealWorldMapShellCardStyle::AppModule);
    let app_header_language_switcher =
        trillionnium_language_inline_switcher_html("trillionnium-app-language-select");
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
    main.app-mobile-shell > section {{ order:8; }}
    #app-tab-map.is-active {{ display:contents; }}
    #app-tab-map > .app-tab-header {{ order:1; }}
    #app-tab-map > .map-shell {{ order:2; }}
    #app-first-playable-onboarding {{ order:3; }}
    #app-tab-map > section:not(.map-shell) {{ order:7; }}
    #app-tab-messages {{ order:4; }}
    #app-tab-feed {{ order:5; }}
    #app-tab-me {{ order:6; }}
    h1 {{ margin:0; font-size:clamp(28px,5.6vw,52px); letter-spacing:-.06em; }}
    .subtitle {{ color:var(--muted); max-width:850px; line-height:1.55; }}
    .grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .map-shell {{ display:grid; grid-template-columns:minmax(320px,1.1fr) minmax(300px,.9fr); gap:18px; align-items:start; margin-bottom:22px; }}
    #real-world-map {{ order:1; }}
    #app-map-route-panel {{ order:2; }}
    #app-map-action-panel {{ order:3; }}
    .map-panel {{ order:4; border:1px solid rgba(255,255,255,.12); border-radius:26px; background:rgba(255,255,255,.07); padding:22px; box-shadow:0 20px 70px rgba(0,0,0,.35); }}
    #real-world-map {{ min-height:430px; border-radius:26px; overflow:hidden; border:1px solid rgba(100,227,255,.28); box-shadow:0 24px 90px rgba(0,0,0,.45); background:#0b1220; }}
    #real-world-map .leaflet-tile-pane {{ filter:saturate(.72) contrast(.88) brightness(.82); }}
    #real-world-map .trillionnium-active-route-line {{ filter:drop-shadow(0 0 8px rgba(100,227,255,.58)); }}
    #real-world-map .leaflet-control-zoom a {{ width:40px; height:40px; line-height:40px; font-size:20px; }}
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
    .trillionnium-avatar-task-route-path {{ animation: trillionnium-route-dash 1.5s linear infinite; filter: drop-shadow(0 0 8px rgba(167,139,250,.42)); }}
    .trillionnium-avatar-task-route-pulse {{ animation: trillionnium-route-pulse 1.8s ease-in-out infinite; }}
    .trillionnium-avatar-route-runner-dot {{ width:38px; height:38px; border-radius:999px; display:grid; place-items:center; background:linear-gradient(135deg,#a78bfa,#64e3ff); box-shadow:0 0 0 3px rgba(11,18,32,.84),0 0 24px rgba(167,139,250,.55); animation: trillionnium-runner-bob 820ms ease-in-out infinite; }}
    .trillionnium-avatar-route-runner-dot span {{ transform:translateY(-1px); }}
    .trillionnium-avatar-route-runner-progress {{ filter: drop-shadow(0 0 10px rgba(100,227,255,.48)); }}
    .trillionnium-avatar-route-runner-remaining {{ animation: trillionnium-route-dash 1.7s linear infinite; }}
    .trillionnium-avatar-route-reward-checkpoint {{ animation: trillionnium-route-pulse 1.35s ease-in-out infinite; filter: drop-shadow(0 0 12px rgba(141,255,176,.48)); }}
    .app-avatar-task-route-card {{ border-color:rgba(167,139,250,.36); box-shadow:0 14px 36px rgba(50,34,120,.22); }}
    .app-avatar-route-runner-card {{ border-color:rgba(100,227,255,.36); box-shadow:0 14px 36px rgba(34,90,120,.22); }}
    @keyframes trillionnium-route-dash {{ from {{ stroke-dashoffset: 0; }} to {{ stroke-dashoffset: -24; }} }}
    @keyframes trillionnium-route-pulse {{ 0%,100% {{ opacity:.55; transform:scale(1); }} 50% {{ opacity:1; transform:scale(1.08); }} }}
    @keyframes trillionnium-runner-bob {{ 0%,100% {{ transform:translateY(0) scale(1); }} 50% {{ transform:translateY(-5px) scale(1.06); }} }}
    .module p,.subtitle {{ color:var(--muted); }}
    .app-tactics-player-hud {{ grid-template-columns:repeat(auto-fit,minmax(150px,1fr)); gap:8px; padding:10px; max-height:112px; overflow:auto; scrollbar-width:thin; }}
    .app-tactics-player-hud > strong {{ grid-column:1/-1; font-size:14px; }}
    .app-tactics-player-hud article.module {{ gap:4px; padding:10px; border-radius:14px; box-shadow:none; }}
    .app-tactics-player-hud article.module strong {{ font-size:12px; }}
    .app-tactics-player-hud,
    .app-tactics-player-hud * {{ min-width:0; max-width:100%; box-sizing:border-box; }}
    .app-tactics-player-hud article.module span,
    .app-tactics-player-hud article.module small,
    .app-tactics-player-hud article.module code {{ font-size:11px; line-height:1.2; overflow-wrap:anywhere; }}
    .app-tactics-player-hud .tactics-reward-history-card .grid {{ display:none; }}
    .map-panel p {{ color:var(--muted); line-height:1.55; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    a {{ color:var(--gold); }}
    .app-mobile-shell {{ display:grid; gap:18px; }}
    .app-topbar-meta {{ display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; }}
    .app-topbar-actions {{ display:flex; align-items:center; justify-content:flex-end; gap:10px; flex-wrap:wrap; }}
    .app-topbar-actions p {{ margin:0; display:flex; align-items:center; gap:8px; flex-wrap:wrap; }}
    .app-topbar-actions a {{ min-height:40px; display:inline-flex; align-items:center; justify-content:center; padding:0 12px; border:1px solid rgba(248,195,91,.24); border-radius:999px; background:rgba(248,195,91,.08); text-decoration:none; font-weight:900; }}
    .language-switcher {{ display:inline-flex; align-items:center; gap:8px; width:max-content; max-width:100%; border:1px solid rgba(100,227,255,.24); background:rgba(255,255,255,.065); color:var(--cyan); border-radius:999px; padding:6px 8px 6px 10px; font-size:12px; font-weight:900; }}
    .language-switcher select {{ width:auto; min-height:40px; min-width:92px; max-width:130px; margin:0; border:0; background:rgba(7,8,20,.72); color:var(--text); border-radius:999px; padding:7px 26px 7px 10px; font:inherit; font-size:12px; }}
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
    .quest-next-card strong,.quest-status-card strong {{ display:block; color:var(--gold); font-size:15px; margin-bottom:6px; }}
    .quest-status-card {{ border:1px solid rgba(100,227,255,.16); background:rgba(100,227,255,.05); border-radius:18px; padding:14px; }}
    .quest-status-card .subtitle {{ margin:0 0 8px; }}
    .quest-cta {{ display:inline-flex; align-items:center; justify-content:center; min-height:44px; border-radius:14px; border:1px solid rgba(248,195,91,.42); background:linear-gradient(135deg,rgba(248,195,91,.92),rgba(255,150,89,.9)); color:#071019; text-decoration:none; font-weight:950; padding:0 16px; box-shadow:0 12px 28px rgba(248,195,91,.16); }}
    .quest-quick-path {{ margin:12px 0 0; border:1px solid rgba(248,195,91,.24); background:rgba(248,195,91,.08); border-radius:18px; padding:12px 14px; display:flex; align-items:center; gap:10px; flex-wrap:wrap; }}
    .quest-quick-path strong {{ color:var(--gold); font-size:13px; text-transform:uppercase; letter-spacing:.05em; }}
    .quest-quick-path span {{ color:var(--text); font-weight:850; }}
    .app-full-command-drawer {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.035); border-radius:18px; padding:10px; }}
    .app-full-command-drawer > p {{ margin:10px 0 0; }}
    .app-player-loop-steps {{ list-style:none; padding:0; margin:12px 0 0; display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; }}
    .app-player-loop-steps li {{ display:grid; gap:4px; min-height:86px; border:1px solid rgba(248,195,91,.2); background:rgba(248,195,91,.07); border-radius:16px; padding:12px; }}
    .app-player-loop-steps b {{ color:var(--gold); }}
    .app-player-loop-steps span {{ color:var(--muted); font-size:13px; line-height:1.35; }}
    .playability-coach-lanes {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:9px; margin:10px 0 0; }}
    .coach-card {{ display:grid; gap:5px; min-width:0; border:1px solid rgba(100,227,255,.2); background:rgba(7,8,20,.34); border-radius:16px; padding:10px; }}
    .coach-card span {{ color:var(--cyan); font-weight:950; font-size:12px; }}
    .coach-card strong {{ color:var(--gold); font-size:14px; line-height:1.2; }}
    .coach-card p {{ margin:0; color:var(--muted); font-size:12px; line-height:1.35; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }}
    .coach-card code {{ font-size:11px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
    .coach-cta {{ min-height:38px; padding:0 10px; font-size:12px; }}
    .economy-retention-drawer {{ margin-top:10px; }}
    .economy-retention-grid {{ display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:8px; margin:10px 0; }}
    .economy-card {{ display:grid; gap:4px; border:1px solid rgba(248,195,91,.18); background:rgba(248,195,91,.06); border-radius:14px; padding:9px; min-width:0; }}
    .economy-card strong {{ color:var(--gold); font-size:12px; line-height:1.2; }}
    .economy-card span,.economy-card small {{ color:var(--muted); font-size:11px; line-height:1.3; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
    .app-map-product-strip {{ display:grid; gap:10px; grid-template-columns:minmax(0,1fr) auto; align-items:center; border:1px solid rgba(100,227,255,.18); background:rgba(100,227,255,.055); border-radius:18px; padding:12px; margin:12px 0; }}
    .app-map-product-strip strong {{ display:block; color:var(--gold); margin-bottom:4px; }}
    .app-map-product-strip .quest-cta {{ min-width:154px; }}
    .app-mobile-action-sheet {{ position:relative; isolation:isolate; }}
    .app-mobile-action-sheet::before {{ content:""; display:none; position:absolute; top:6px; left:50%; width:42px; height:4px; transform:translateX(-50%); border-radius:999px; background:rgba(246,247,251,.32); }}
    .app-mobile-primary-cta {{ white-space:nowrap; }}
    .app-mobile-primary-cta::after {{ content:" →"; }}
    .app-copy-layer-summary {{ margin:8px 0 0; font-size:14px; line-height:1.42; }}
    #app-map-readability-lod {{ margin:4px 0 0; font-size:12px; line-height:1.22; }}
    .app-copy-layer-details {{ margin-top:8px; border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.035); border-radius:16px; padding:9px 11px; }}
    .app-copy-layer-details summary {{ cursor:pointer; color:var(--cyan); font-size:12px; font-weight:900; min-height:34px; display:flex; align-items:center; }}
    .app-copy-layer-details p {{ margin:6px 0 0; font-size:13px; line-height:1.42; }}
    .map-technical-drawer,.app-progress-drawer {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.035); border-radius:18px; padding:10px; }}
    #app-tile-shards-live,
    #app-region-shards-live,
    #app-poi-hotspots-live,
    #app-prefetch-queue-live,
    #app-live-events-live,
    #app-feed-items-live,
    #app-route-preview-live,
    #app-route-task-graph-live {{ max-height:460px; overflow:auto; padding-right:4px; }}
    .dev-details {{ margin-top:10px; color:var(--muted); }}
    .dev-details summary {{ cursor:pointer; width:max-content; border:1px solid rgba(255,255,255,.1); border-radius:999px; padding:8px 12px; min-height:36px; display:inline-flex; align-items:center; background:rgba(255,255,255,.05); color:rgba(246,247,251,.72); font-size:12px; font-weight:800; }}
    .app-tab-panel {{ display:none; gap:16px; }}
    .app-tab-panel.is-active {{ display:grid; }}
    .app-tab-header {{ display:grid; gap:6px; margin-bottom:4px; }}
    .app-bottom-tabs {{ position:fixed; left:0; right:0; bottom:0; z-index:30; display:grid; grid-template-columns:repeat(4,1fr); gap:8px; padding:10px min(4vw,24px) calc(10px + env(safe-area-inset-bottom, 0px)); border-top:1px solid rgba(255,255,255,.08); background:rgba(8,10,24,.92); backdrop-filter:blur(18px); }}
    .app-bottom-tab {{ min-height:44px; display:inline-flex; align-items:center; justify-content:center; border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.05); color:var(--muted); border-radius:16px; padding:10px 8px; font-weight:800; cursor:pointer; }}
    .app-bottom-tab.is-active {{ color:var(--text); background:rgba(100,227,255,.12); border-color:rgba(100,227,255,.32); }}
    .app-bottom-tab:focus-visible, .focus-chip:focus-visible, .overlay-toggle:focus-visible, .app-search-input:focus-visible, .app-search-clear:focus-visible {{ outline:2px solid var(--cyan); outline-offset:2px; }}
    @media (min-width: 821px) {{
      body {{ padding-bottom:0; }}
      header {{ position:relative; }}
      main {{ padding-top:16px; }}
      .app-bottom-tabs {{ position:sticky; top:0; bottom:auto; z-index:25; margin:0 min(5vw,32px); grid-template-columns:repeat(4,minmax(130px,1fr)); padding:12px; border:1px solid rgba(255,255,255,.1); border-top:0; border-radius:0 0 22px 22px; box-shadow:0 18px 60px rgba(0,0,0,.24); }}
      .app-bottom-tab {{ border-radius:14px; }}
    }}
    .app-me-grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .app-search-hidden {{ display:none !important; }}
    @media (max-width: 820px) {{
      header {{ padding:9px 12px 8px; }}
      main {{ padding:10px 14px 34px; gap:12px; }}
      h1 {{ font-size:25px; }}
      header > .subtitle {{ margin:4px 0 0; display:-webkit-box; -webkit-line-clamp:1; -webkit-box-orient:vertical; overflow:hidden; }}
      .subtitle {{ font-size:13px; line-height:1.34; }}
      .app-topbar-meta {{ display:grid; grid-template-columns:minmax(0,1fr) auto; align-items:center; gap:6px; margin-bottom:5px; }}
      .app-topbar-meta > p {{ margin:0; min-width:0; }}
      .app-topbar-actions {{ gap:4px; flex-wrap:nowrap; min-width:0; }}
      .app-topbar-actions p {{ display:none; }}
      .app-topbar-actions a {{ min-height:38px; padding:0 7px; font-size:10px; }}
      .app-beta-chip {{ max-width:150px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; padding:5px 8px; font-size:10px; }}
      header .language-switcher {{ min-height:40px; padding:4px 5px; font-size:10px; }}
      header .language-switcher > span {{ display:none; }}
      header .language-switcher select {{ min-height:40px; min-width:86px; max-width:112px; padding:5px 20px 5px 7px; font-size:10px; }}
      .app-search-shell {{ gap:7px; margin-top:7px; }}
      .app-search-input {{ min-height:40px; padding:10px 12px; border-radius:14px; font-size:14px; }}
      .app-search-clear {{ padding:8px 9px; border-radius:12px; }}
      .app-ux-status {{ margin-top:5px; min-height:20px; }}
      .app-ux-pill {{ padding:4px 8px; font-size:10px; }}
      .app-copy-layer-summary {{ display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; margin-top:6px; }}
      #app-map-readability-lod {{ margin:2px 0 0; font-size:11px; line-height:1.18; display:-webkit-box; -webkit-line-clamp:1; -webkit-box-orient:vertical; overflow:hidden; }}
      .app-copy-layer-details {{ padding:7px 9px; margin-top:6px; }}
      .app-copy-layer-details summary {{ min-height:30px; font-size:11px; }}

      #app-tab-map {{ order:1; }}
      #app-tab-map.is-active {{ display:contents; }}
      #app-tab-map > .app-tab-header {{ order:0; }}
      #app-tab-map > .map-shell {{ order:1; display:contents; }}
      #app-tab-map > .map-shell > #real-world-map {{ order:1; }}
      #app-tab-map > .map-shell > .map-panel {{ order:5; display:grid; gap:12px; max-height:520px; overflow:auto; }}
      #app-tab-map > section:not(.map-shell) {{ order:7; max-height:420px; overflow:auto; padding-right:4px; }}
      #app-tile-shards-live,
      #app-region-shards-live,
      #app-poi-hotspots-live,
      #app-prefetch-queue-live,
      #app-live-events-live,
      #app-feed-items-live,
      #app-route-preview-live,
      #app-route-task-graph-live {{ max-height:340px; }}
      #app-first-playable-onboarding {{ order:4; }}
      #app-map-route-panel {{ order:2; max-height:340px; overflow:auto; }}
      #app-map-action-panel {{ order:3; max-height:260px; overflow:auto; }}
      #app-map-camera-actions {{ order:5; }}
      #app-map-route-panel,#app-map-action-panel {{ padding:14px; margin-top:0 !important; }}
      #app-map-route-panel p,#app-map-action-panel p {{ margin:4px 0; line-height:1.35; }}
      #app-map-route-event-brief-status,#app-map-route-link-status {{ display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }}
      #app-map-route-actions,#app-map-action-rail {{ max-height:170px; overflow:auto; }}
      .map-technical-drawer {{ order:8; }}
      #app-tab-messages {{ order:3; }}
      #app-tab-feed {{ order:4; }}
      #app-tab-me {{ order:5; }}
      .map-shell {{ grid-template-columns:1fr; gap:12px; margin-bottom:12px; }}
      #real-world-map {{ order:-1; min-height:min(34svh,300px); border-radius:20px; }}
      .map-panel,.module {{ padding:15px; border-radius:19px; }}
      .map-stream-hud,.overlay-toggle-bar,.focus-stack {{ gap:6px; }}
      .hud-chip,.focus-chip,.overlay-toggle {{ padding:7px 9px; font-size:12px; min-height:44px; }}
      .app-bottom-tab,.quest-cta,.app-search-clear,.language-switcher,.language-switcher select {{ min-height:44px; }}
      .quest-summary {{ grid-template-columns:1fr; }}
      .app-map-product-strip {{ position:fixed; left:12px; right:12px; bottom:calc(76px + env(safe-area-inset-bottom, 0px)); z-index:28; grid-template-columns:minmax(0,1fr) auto; gap:8px; margin:0; padding:14px 10px 9px; border-radius:22px 22px 17px 17px; background:rgba(9,13,27,.92); box-shadow:0 -10px 45px rgba(0,0,0,.42); backdrop-filter:blur(16px); }}
      .app-mobile-action-sheet::before {{ display:block; }}
      .app-map-product-strip strong {{ margin-bottom:2px; font-size:11px; text-transform:uppercase; letter-spacing:.04em; }}
      .app-map-product-strip .subtitle {{ margin:0; font-size:11px; line-height:1.22; }}
      #app-map-density-summary,#app-map-camera-summary {{ display:none; }}
      #app-route-runner-handoff-summary {{ display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }}
      .app-map-product-strip .quest-cta {{ min-width:112px; min-height:40px; padding:0 10px; font-size:12px; }}
      .quest-hero {{ gap:8px; }}
      .quest-next-card .subtitle {{ display:none; }}
      .quest-status-card .subtitle {{ display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }}
      .app-player-loop-steps {{ grid-template-columns:repeat(3,minmax(0,1fr)); gap:7px; margin-top:10px; }}
      .app-player-loop-steps li {{ min-height:auto; padding:8px; border-radius:13px; }}
      .app-player-loop-steps b {{ font-size:12px; }}
      .app-player-loop-steps span {{ display:none; }}
      .playability-coach-lanes {{ gap:6px; margin-top:8px; }}
      .coach-card {{ padding:8px; border-radius:13px; }}
      .coach-card strong {{ font-size:11px; }}
      .coach-card p,.coach-card code {{ display:none; }}
      .coach-cta {{ min-height:36px; padding:0 7px; font-size:10px; }}
      .economy-retention-grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); gap:6px; }}
      .economy-card {{ padding:7px; }}
      .economy-card span,.economy-card small {{ display:none; }}
      #app-first-playable-steps {{ display:none; }}
    }}
    @media (max-width: 600px) {{
      body {{ padding-bottom:calc(104px + env(safe-area-inset-bottom, 0px)); }}
    }}
    @media (min-width: 601px) and (max-width: 820px) {{
      #real-world-map {{ min-height:min(28svh,260px); }}
      .app-map-product-strip {{ position:static; margin:0; box-shadow:none; backdrop-filter:none; }}
      .app-map-product-strip .quest-cta {{ min-height:44px; }}
      #app-map-route-panel {{ max-height:320px; }}
    }}
  </style>
</head>
<body>
  <header>
    <div class="app-topbar-meta">
      <p><span class="app-beta-chip" data-i18n-en="Global-first Beta · Mobile World Shell v1" data-i18n-zh="海外市场首发 · 移动世界壳 v1">Global-first Beta · Mobile World Shell v1</span></p>
      <div class="app-topbar-actions"><p><a href="/world" data-i18n-en="World" data-i18n-zh="世界">World</a><a href="/league" data-i18n-en="Arena" data-i18n-zh="竞技场">Arena</a></p>{app_header_language_switcher}</div>
    </div>
    <h1>Trillionnium World</h1>
    <p class="subtitle"><span data-i18n-en="Pick a route, run one quest, claim the reward. Current focus:" data-i18n-zh="选路线、跑任务、领奖励。当前位置：">Pick a route, run one quest, claim the reward. Current focus:</span> <strong>{}</strong></p>
    <div class="app-search-shell">
      <input id="app-global-search" class="app-search-input" type="search" inputmode="search" placeholder="Search world" data-i18n-placeholder-en="Search world" data-i18n-placeholder-zh="搜索世界" aria-label="Global search" data-i18n-aria-label-en="Global search" data-i18n-aria-label-zh="全局搜索" />
      <button id="app-search-clear" class="app-search-clear" type="button" aria-label="Clear global search" data-i18n-aria-label-en="Clear global search" data-i18n-aria-label-zh="清空全局搜索" data-i18n-en="Clear" data-i18n-zh="清空" hidden>Clear</button>
    </div>
    <div id="app-ux-status" class="app-ux-status" aria-live="polite">
      <span id="app-ux-status-pill" class="app-ux-pill" data-state="ready" data-i18n-en="Ready · World" data-i18n-zh="已就绪 · 世界">Ready · World</span>
      <span id="app-ux-live-status" class="sr-only" data-i18n-en="Adventure ready" data-i18n-zh="冒险体验已准备完成">Adventure ready</span>
    </div>
    <div id="app-search-empty-state" class="app-search-empty" role="status" aria-live="polite" data-i18n-en="No results · Try another keyword or tab." data-i18n-zh="无匹配结果 · 换个关键词或切换底部 Tab。">No results · Try another keyword or tab.</div>
  </header>
  <nav class="app-bottom-tabs" aria-label="Main navigation" data-i18n-aria-label-en="Main navigation" data-i18n-aria-label-zh="主导航" role="tablist">
    <button id="app-tab-button-messages" type="button" class="app-bottom-tab" data-app-tab="messages" role="tab" data-i18n-en="Messages" data-i18n-zh="消息" aria-controls="app-tab-messages" aria-selected="false" tabindex="-1">Messages</button>
    <button id="app-tab-button-map" type="button" class="app-bottom-tab is-active" data-app-tab="map" role="tab" data-i18n-en="World" data-i18n-zh="世界" aria-controls="app-tab-map" aria-selected="true" tabindex="0">World</button>
    <button id="app-tab-button-feed" type="button" class="app-bottom-tab" data-app-tab="feed" role="tab" data-i18n-en="Feed" data-i18n-zh="动态" aria-controls="app-tab-feed" aria-selected="false" tabindex="-1">Feed</button>
    <button id="app-tab-button-me" type="button" class="app-bottom-tab" data-app-tab="me" role="tab" data-i18n-en="Me" data-i18n-zh="我" aria-controls="app-tab-me" aria-selected="false" tabindex="-1">Me</button>
  </nav>
  <main class="app-mobile-shell">
    <section id="app-first-playable-onboarding" class="module quest-hero" aria-label="First playable main quest rail" data-i18n-aria-label-en="First playable main quest rail" data-i18n-aria-label-zh="第一条可玩主线">
      <span class="badge" data-i18n-en="Starter Quest" data-i18n-zh="新手主线">Starter Quest</span>
      <h2>{}</h2>
      <div class="quest-summary">
        <div class="quest-status-card">
          <strong data-i18n-en="Current Status" data-i18n-zh="当前状态">Current Status</strong>
          <p class="subtitle">{} <span data-i18n-en="Goal:" data-i18n-zh="目标：">Goal:</span> <code>{}</code></p>
        </div>
        <div class="quest-next-card">
          <strong data-i18n-en="Next Action" data-i18n-zh="下一步行动">Next Action</strong>
          <p class="subtitle" data-i18n-en="Choose a map focus in World, accept a quest card, submit results, then finish rating." data-i18n-zh="先在世界页选择地图焦点，再接取任务牌、提交成果并完成评级。">Choose a map focus in World, accept a quest card, submit results, then finish rating.</p>
          <a class="quest-cta" href="/world" data-i18n-en="Start World Quest" data-i18n-zh="开始世界任务">Start World Quest</a>
        </div>
      </div>
      <div id="app-first-playable-quick-path" class="quest-quick-path" aria-label="Quick path" data-i18n-aria-label-en="Quick path" data-i18n-aria-label-zh="快速路径">
        {}
        {}
      </div>
      <ol class="app-player-loop-steps" aria-label="Starter quest steps" data-i18n-aria-label-en="Starter quest steps" data-i18n-aria-label-zh="新手任务三步">
        {}
      </ol>
      <section id="app-playability-coach" class="playability-coach-lanes" data-contract-version="{}" aria-label="P0 P1 P2 playability coach" data-i18n-aria-label-en="P0 P1 P2 playability coach" data-i18n-aria-label-zh="P0 P1 P2 可玩性教练">{}</section>
      <details id="app-economy-retention-ops" class="dev-details economy-retention-drawer"><summary><span data-i18n-en="Economy · return · telemetry" data-i18n-zh="经济 · 回访 · 遥测">Economy · return · telemetry</span> · {}%</summary><p class="subtitle">{}</p><div id="app-route-runner-funnel-telemetry" class="map-stream-hud" data-contract-version="{}" data-time-to-reward-seconds="{}" data-time-to-reward-target-seconds="1800" data-daily-return-resume-count="{}" data-reward-to-next-route-percent="{}" data-d1-resume-percent="{}" data-route-abandon-or-recovery-percent="{}" data-time-to-first-proof-seconds="{}" data-time-to-next-route-seconds="{}">{}</div><div id="app-route-archetype-catalog" class="economy-retention-grid">{}</div><div id="app-commercial-operating-dashboard" class="map-stream-hud" data-contract-version="trillionnium_world_commercial_operating_dashboard_v1">{}</div><div class="economy-retention-grid">{}</div><div class="map-stream-hud">{}</div></details>
      <details class="dev-details app-progress-drawer"><summary data-i18n-en="Progress checks" data-i18n-zh="进度检查">Progress checks</summary><div id="app-first-playable-checks" class="map-stream-hud">{}</div></details>
      <details id="app-first-playable-full-commands" class="dev-details app-full-command-drawer">{}{}<section id="app-first-playable-steps" class="grid">{}</section></details>
    </section>
    <section id="app-tab-messages" class="app-tab-panel" data-app-panel="messages" role="tabpanel" aria-labelledby="app-tab-button-messages" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2 data-i18n-en="Messages" data-i18n-zh="消息">Messages</h2>
        <p class="subtitle" data-i18n-en="Hub for teammates, Agents, notifications, and quest threads." data-i18n-zh="队友、Agent 协作、提示与主线/支线线程。">Hub for teammates, Agents, notifications, and quest threads.</p>
      </div>
      <section id="app-message-cards" class="grid">{}</section>
    </section>
    <section id="app-tab-map" class="app-tab-panel is-active" data-app-panel="map" role="tabpanel" aria-labelledby="app-tab-button-map" aria-hidden="false" data-bootstrap-mode="truncated_runtime_bootstrap_with_lazy_delta_hydration" data-bootstrap-payload-bytes="{}" data-cache-contract="trillionnium_world_map_payload_cache_v1">
      <div class="app-tab-header">
        <h2 data-i18n-en="World" data-i18n-zh="世界">World</h2>
        <p class="subtitle" data-i18n-en="Main stage for exploration, routes, events, and actions." data-i18n-zh="探索、路线、事件和行动都从这里展开。">Main stage for exploration, routes, events, and actions.</p>
      </div>
      <section class="map-shell" aria-label="Trillionnium World Map" data-i18n-aria-label-en="Trillionnium World Map" data-i18n-aria-label-zh="Trillionnium 世界地图">
      <div class="map-panel">
        <span class="badge" data-i18n-en="{}" data-i18n-zh="Trillionnium 世界地图">{}</span>
        <h2 data-i18n-en="OpenStreetMap upgraded into a playable world" data-i18n-zh="把 OpenStreetMap 升级成可玩的世界地图">OpenStreetMap upgraded into a playable world</h2>
        <p id="app-map-copy-summary" class="app-copy-layer-summary" data-contract-version="trillionnium_mobile_copy_layering_v1" data-i18n-en="Pick a nearby route → submit proof → claim reward; the runner shows the next step." data-i18n-zh="选择附近路线 → 提交证据 → 领取奖励；角色显示下一步。">Pick a nearby route → submit proof → claim reward; the runner shows the next step.</p>
        <p id="app-map-readability-lod" class="subtitle" data-contract-version="trillionnium_world_map_readability_lod_v1" data-semantic-layer-contract="trillionnium_world_map_game_layer_semantics_v1" data-first-screen-mode="route_first_street_detail" data-primary-cta-budget="1" data-visible-marker-budget="18" data-avatar-runner-budget="6" data-copy-summary-budget="150" data-details-default-state="collapsed" data-semantic-legend-required="true" data-avatar-feedback-required="true" data-i18n-a11y-required="true" data-i18n-en="One route first: one CTA, muted OSM context, high-contrast route, start/objective/reward/locked pins." data-i18n-zh="先看一条路线：一个主行动、弱化底图、高对比路线、起点/目标/奖励/锁定图钉。">One route first: one CTA, muted OSM context, high-contrast route, start/objective/reward/locked pins.</p>
        <p id="app-map-performance-budget" class="subtitle" data-contract-version="trillionnium_world_map_runtime_performance_budget_v1" data-first-map-interactive-target-ms="2000" data-viewport-refresh-p95-target-ms="250" data-focus-to-action-rail-target-ms="300" data-main-thread-long-task-budget-ms="100" data-low-end-mobile-fps-floor="45" data-delta-viewport-updates-required="true" data-abort-previous-viewport-request="true" data-defer-noncritical-card-render="true" data-cluster-markers-before-hiding="true" data-spatial-cache-required="true" data-virtualized-cards-required="true" data-adaptive-density-required="true" data-i18n-en="Performance budget: interactive under 2s, focus-to-action under 300ms, abort stale viewport fetches, defer dense cards, and cluster before extra density." data-i18n-zh="性能预算：2 秒内可交互、焦点到行动栏 300ms 内、取消过期视口请求、延后密集卡片、先聚合再增加密度。">Performance budget: interactive under 2s, focus-to-action under 300ms, abort stale viewport fetches, defer dense cards, and cluster before extra density.</p>
        <p id="app-map-rum-slo" class="subtitle" data-contract-version="trillionnium_world_map_rum_slo_v1" data-quantiles="p50,p95,p99" data-surface-split="app,world" data-device-split="mobile,desktop" data-sample-kinds="cold_cache_interactive,warm_delta_or_304,weak_network_cached_snapshot" data-per-bucket-min-samples="1" data-matrix-contract="trillionnium_world_map_real_user_rum_matrix_v1" data-i18n-en="RUM SLO gate: p50/p95/p99 by /app vs /world and mobile vs desktop before adding map density." data-i18n-zh="真实用户性能门禁：按 /app 与 /world、移动端与桌面端拆 p50/p95/p99，再增加地图密度。">RUM SLO gate: p50/p95/p99 by /app vs /world and mobile vs desktop before adding map density.</p>
        <p id="app-map-transport-delta" class="subtitle" data-contract-version="trillionnium_world_map_transport_delta_v1" data-subsystem-contract="trillionnium_world_map_subsystem_v1" data-presence-delta-required="true" data-snapshot-fallback-required="true" data-changed-group-rendering-required="true" data-visible-marker-delta-required="true" data-marker-cluster-delta-required="true" data-i18n-en="Transport boundary: viewport snapshots stay compatible, while route runners, presence, markers, and clusters move by changed-group deltas." data-i18n-zh="传输边界：视口快照保持兼容，路线角色、在线状态、标记和聚合按变更组增量更新。">Transport boundary: viewport snapshots stay compatible, while route runners, presence, markers, and clusters move by changed-group deltas.</p>
        <p id="app-map-weak-network" class="subtitle" data-contract-version="trillionnium_world_map_weak_network_resilience_v1" data-cache-key="trillionnium-world-map:last-good-viewport:v1" data-delta-304-supported="true" data-offline-banner-required="true" data-pending-action-queue-required="true" data-conflict-sync-required="true" data-i18n-en="Weak network mode: delta first, 304 reuse, snapshot fallback, then last-good cached viewport without blanking the map." data-i18n-zh="弱网模式：先增量、304 复用、快照兜底，最后用上次可用视口，不让地图空白。">Weak network mode: delta first, 304 reuse, snapshot fallback, then last-good cached viewport without blanking the map.</p>
        <p id="app-map-location-privacy" class="subtitle" data-contract-version="trillionnium_world_map_location_privacy_v1" data-rum-excludes-lat-lng="true" data-cache-control="private" data-i18n-en="Location privacy: RUM sends surface/device/cursor only; personalized viewport and delta responses stay private-cache." data-i18n-zh="位置隐私：RUM 只发送界面/设备/游标；个性化视口和增量响应保持 private cache。">Location privacy: RUM sends surface/device/cursor only; personalized viewport and delta responses stay private-cache.</p>
        <p id="app-map-first-screen-decision" class="subtitle" data-contract-version="trillionnium_world_map_first_screen_decision_v1" data-first-screen-promises="current_route,next_action,reward_xp" data-primary-cta-id="app-mobile-primary-cta" data-primary-cta-target="app-map-action-rail" data-dense-detail-default="collapsed" data-i18n-en="First screen only has to answer: current route, next action, and reward/XP." data-i18n-zh="首屏只回答三件事：当前路线、下一步动作、奖励/XP。">First screen only has to answer: current route, next action, and reward/XP.</p>
        <details id="app-map-copy-layer-details" class="app-copy-layer-details" data-contract-version="trillionnium_mobile_copy_layering_v1" data-default-state="collapsed"><summary data-i18n-en="Why this map matters" data-i18n-zh="为什么这张地图重要">Why this map matters</summary><p data-i18n-en="{}" data-i18n-zh="Trillionnium World Map 不是普通地图工具，而是在 OpenStreetMap 真实地理底座上叠加游戏人物、路线节点、任务牌、实时事件和交付闭环。角色会在地图上跑来跑去，接任务、提交证据、拿评级和奖励。">{}</p><p><strong data-i18n-en="Game Map Main Entry" data-i18n-zh="游戏地图主入口">Game Map Main Entry</strong>: <span data-i18n-en="move your avatar between nearby places, live events, and bounty nodes before entering other modules." data-i18n-zh="先让角色在附近地点、实时事件和悬赏节点之间跑图，再进入其他模块。">move your avatar between nearby places, live events, and bounty nodes before entering other modules.</span></p></details>
        <div id="app-mobile-action-sheet" class="app-map-product-strip app-mobile-action-sheet" aria-label="Mobile route action sheet" data-i18n-aria-label-en="Mobile route action sheet" data-i18n-aria-label-zh="移动路线行动面板" data-contract-version="trillionnium_mobile_single_primary_cta_v1" data-first-screen-decision-contract="trillionnium_world_map_first_screen_decision_v1" data-bottom-sheet-mode="fixed_above_bottom_tabs_on_mobile" data-primary-cta-count="1" data-primary-cta-target="app-map-action-rail">
          <div>
            <strong data-i18n-en="Current Status" data-i18n-zh="当前状态">Current Status</strong>
            <p id="app-map-density-summary" class="subtitle">{}</p>
            <p id="app-route-runner-handoff-summary" class="subtitle" data-next-route-status="{}" data-runner-count="{}" data-reward-claim-count="{}" data-next-route-count="{}">{}</p>
            <p id="app-map-camera-summary" class="subtitle" data-i18n-en="Camera loading…" data-i18n-zh="镜头加载中…">Camera loading…</p>
          </div>
          <a id="app-mobile-primary-cta" class="quest-cta app-mobile-primary-cta" href='#app-map-action-rail' data-primary-cta="world-route-focus" data-primary-cta-target="app-map-action-rail" data-i18n-en="Continue Route" data-i18n-zh="继续路线">Continue Route</a>
        </div>
        {}
        <div id="app-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
        </div>
        <details class="dev-details map-technical-drawer"><summary data-i18n-en="Advanced map layers" data-i18n-zh="高级地图图层">Advanced map layers</summary>
          <p><strong>Real-world map engine</strong>: <code>{}</code> + <code>{}</code></p>
          <p><strong>Mirror</strong>: <code>{}</code> · <strong>Active Region</strong>: <code>{}</code> · <strong>Shards</strong>: {} · <strong>LOD Layers</strong>: {}</p>
          <p><strong>Viewport API</strong>: <code>{}</code></p>
          <p><strong>Web Viewport</strong>: <code>{}</code></p>
          <p><code>{}</code></p>
          <div id="app-map-stream-hud" class="map-stream-hud">
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="regional shards" data-i18n-zh="个区域分片">regional shards</span></span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="visible places" data-i18n-zh="个可见地点">visible places</span></span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="prefetch tiles" data-i18n-zh="个预热地图块">prefetch tiles</span></span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="live events" data-i18n-zh="个实时事件">live events</span> · {}</span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="task routes" data-i18n-zh="条任务路线">task routes</span></span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="moving avatars" data-i18n-zh="个动态角色">moving avatars</span></span>
            <span class="hud-chip"><strong>{}</strong> <span data-i18n-en="running avatars" data-i18n-zh="个跑图角色">running avatars</span></span>
          </div>
          <div id="app-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
          </div>
          <p id="app-map-overlay-status" class="subtitle" data-i18n-en="Active layers: density, regions, tiles, prefetch rings, live events, task routes, moving avatars, player avatars." data-i18n-zh="当前图层：密度、区域、地图块、预热圈、实时事件、任务路线、动态角色、跑图角色。">Active layers: density, regions, tiles, prefetch rings, live events, task routes, moving avatars, player avatars.</p>
          <p id="app-map-overlay-legend" class="subtitle" data-i18n-en="Layer legend: regional anchors · active tiles · prefetch rings · live-event pulses · avatar task routes · animated runners · running avatars." data-i18n-zh="图层说明：区域锚点 · 活跃地图块 · 预热探索圈 · 实时事件脉冲 · 角色任务路线 · 动态跑图 · 跑图角色。">Layer legend: regional anchors · active tiles · prefetch rings · live-event pulses · avatar task routes · animated runners · running avatars.</p>
          <h3 data-i18n-en="Avatar Movement" data-i18n-zh="角色跑图">Avatar Movement</h3>
          <section id="app-avatar-route-runners-live" class="grid"></section>
        </details>
      </div>
        <div id="app-map-action-panel" class="module" style="margin-top:14px; padding:16px 18px;">
          <strong data-i18n-en="Map Action Rail" data-i18n-zh="地图行动栏">Map Action Rail</strong>
          <span id="app-map-focus-summary" data-i18n-en="Waiting for map focus…" data-i18n-zh="等待选择地图焦点…">Waiting for map focus…</span>
          <p id="app-map-focus-detail" data-i18n-en="Select a region, place, or event to create the next action." data-i18n-zh="选择区域、地点或事件，把地图变成下一步行动。">Select a region, place, or event to create the next action.</p>
          <div id="app-map-action-rail" class="focus-stack"></div>
        </div>
        <div id="app-map-route-panel" class="module" style="margin-top:14px; padding:16px 18px;">
          <strong data-i18n-en="Adventure Route" data-i18n-zh="冒险路线">Adventure Route</strong>
          <span id="app-map-route-status" data-i18n-en="Adventure route: waiting for map focus…" data-i18n-zh="冒险路线：等待选择地图焦点…">Adventure route: waiting for map focus…</span>
          <p id="app-map-route-next-step-status" data-i18n-en="Recommended next step: choose a map focus first." data-i18n-zh="推荐下一步：先选择地图焦点。">Recommended next step: choose a map focus first.</p>
          <p id="app-map-route-event-brief-status" data-i18n-en="Event brief: waiting for event." data-i18n-zh="事件简报：等待选择事件。">Event brief: waiting for event.</p>
          <p id="app-map-route-link-status" data-i18n-en="Linked task route: none yet." data-i18n-zh="关联任务路线：暂无。">Linked task route: none yet.</p>
          <div id="app-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <div id="app-map-route-actions" class="focus-stack"></div>
        </div>
      <div id="real-world-map" data-engine="{}" data-provider="{}" aria-label="Trillionnium World Map" data-i18n-aria-label-en="Trillionnium World Map" data-i18n-aria-label-zh="Trillionnium 世界地图"></div>
    </section>
    <section>
      <h2 data-i18n-en="Map Tiles" data-i18n-zh="地图分片">Map Tiles</h2>
      <section id="app-tile-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Regional Hubs" data-i18n-zh="区域据点">Regional Hubs</h2>
      <section id="app-region-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Nearby Hotspots" data-i18n-zh="附近热点">Nearby Hotspots</h2>
      <section id="app-poi-hotspots-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Prefetch Rings" data-i18n-zh="预热探索圈">Prefetch Rings</h2>
      <section id="app-prefetch-queue-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Live Events" data-i18n-zh="实时事件">Live Events</h2>
      <section id="app-live-events-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Avatar Task Routes" data-i18n-zh="角色任务路线">Avatar Task Routes</h2>
      <section id="app-avatar-task-routes-live" class="grid"></section>
    </section>
    </section>
    <section id="app-tab-feed" class="app-tab-panel" data-app-panel="feed" role="tabpanel" aria-labelledby="app-tab-button-feed" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2 data-i18n-en="Feed" data-i18n-zh="动态">Feed</h2>
        <p class="subtitle" data-i18n-en="Discovery feed for city events, commissions, battle reports, adventure updates, and social posts." data-i18n-zh="城市事件、委托、战报、冒险动态与社交更新。">Discovery feed for city events, commissions, battle reports, adventure updates, and social posts.</p>
        <details class="dev-details"><summary data-i18n-en="Feed sync debug" data-i18n-zh="动态同步调试">Feed sync debug</summary><p><strong>Feed API</strong>: <code>{}</code></p><p><strong>Web Feed</strong>: <code>{}</code></p></details>
        <p id="app-feed-api-status" class="subtitle"><span data-i18n-en="Feed loading" data-i18n-zh="动态加载中">Feed loading</span> · <span data-i18n-en="Active region" data-i18n-zh="当前区域">Active region</span> <code>{}</code> · <span data-i18n-en="Ready items" data-i18n-zh="已准备动态">Ready items</span> {}.</p>
      </div>
      <div id="app-feed-filter-actions" class="focus-stack">{}</div>
      <div id="app-feed-summary" class="map-stream-hud">{}</div>
      <section>
        <h2 data-i18n-en="World Activity Timeline" data-i18n-zh="世界动态时间线">World Activity Timeline</h2>
        <section id="app-feed-items-live" class="grid">{}</section>
      </section>
    <section>
      <h2 data-i18n-en="Adventure Route Preview" data-i18n-zh="冒险路线预览">Adventure Route Preview</h2>
      <section id="app-route-preview-live" class="grid">{}</section>
    </section>
    <section>
      <h2 data-i18n-en="Quest Route Graph" data-i18n-zh="任务路线图">Quest Route Graph</h2>
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
        <h2 data-i18n-en="Character Modules" data-i18n-zh="角色模块">Character Modules</h2>
        <section class="grid">{}</section>
      </section>
    </section>
  </main>
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
      const routeRunnerHandoffSummary = document.getElementById('app-route-runner-handoff-summary');
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
      const taskRouteTarget = document.getElementById('app-avatar-task-routes-live');
      const routeRunnerTarget = document.getElementById('app-avatar-route-runners-live');
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
      let lastViewportCursor = null;
      let mapRumFirstInteractiveSent = false;
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
        announceUxStatus(uiText('Ready · ' + appTabLabel(activeAppTab), '已就绪 · ' + appTabLabel(activeAppTab)), 'ready');
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

      const renderRouteRunnerHandoffSummary = (viewport) => {{
        if (!routeRunnerHandoffSummary) return;
        const handoff = ((viewport || {{}}).route_runner_handoff) || {{}};
        routeRunnerHandoffSummary.textContent = String(handoff.summary || 'Route runner handoff: waiting for avatar task routes to unlock reward and next-route actions.');
        routeRunnerHandoffSummary.dataset.nextRouteStatus = String(handoff.first_next_route_status || 'next_route_preview_locked_until_reward_claim');
        routeRunnerHandoffSummary.dataset.runnerCount = String(handoff.runner_count ?? ((viewport || {{}}).avatar_route_runner_count ?? 0));
        routeRunnerHandoffSummary.dataset.rewardClaimCount = String(handoff.reward_claim_action_count ?? 0);
        routeRunnerHandoffSummary.dataset.nextRouteCount = String(handoff.next_route_action_count ?? 0);
        routeRunnerHandoffSummary.dataset.routeMasteryContract = String(handoff.route_mastery_contract_version || 'trillionnium_route_mastery_v1');
        routeRunnerHandoffSummary.dataset.routeMasteryTier = String(handoff.first_route_mastery_tier || 'route_novice');
        routeRunnerHandoffSummary.dataset.routeMasteryXp = String(handoff.first_route_mastery_xp ?? 0);
      }};

      const inferAppRouteNextStep = (selection, context) => inferConfiguredRouteNextStep(selection, context, {{
        statusPrefix: routePhrase('Recommended next step', '推荐下一步'),
        rejectionBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': reopen commission ' + workOrderId + ' with revision needs, renewed effort, and the next submission plan.', selectionTitle + ': 重开委托 ' + workOrderId + '，写清返工要求、再次投入和下一次提交计划。'),
        rejectionStatus: (workOrderId) => routePhrase('Reopen commission ' + workOrderId + '.', '重开委托 ' + workOrderId + '。'),
        reopenBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': resubmit commission ' + workOrderId + ' with results, evidence, and rating checklist.', selectionTitle + ': 重新提交委托 ' + workOrderId + '，补齐成果、证据和评级清单。'),
        reopenStatus: (workOrderId) => routePhrase('Resubmit commission ' + workOrderId + '.', '重新提交委托 ' + workOrderId + '。'),
        deliveryBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': rate commission ' + workOrderId + ' and state pass-or-revision reasons.', selectionTitle + ': 评定委托 ' + workOrderId + ' 的成果，明确通过或返工原因。'),
        deliveryStatus: (workOrderId) => routePhrase('Rate latest commission result ' + workOrderId + '.', '评定最新委托成果 ' + workOrderId + '。'),
        openWorkBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': prepare result, evidence, and next action for commission ' + workOrderId + '.', selectionTitle + ': 为委托 ' + workOrderId + ' 准备成果、证据和下一步行动。'),
        openWorkStatus: (workOrderId) => routePhrase('Submit current commission ' + workOrderId + '.', '提交当前委托 ' + workOrderId + '。'),
        contractBody: (selectionTitle, contractId) => routePhrase(selectionTitle + ': complete linked contract ' + contractId + ' with evidence, rating criteria, and next step.', selectionTitle + ': 完成关联契约 ' + contractId + '，带上证据、评级标准和下一步。'),
        contractStatus: (contractId) => routePhrase('Complete contract ' + contractId + '.', '完成契约 ' + contractId + '。'),
        listingBody: (selectionTitle, listingId) => routePhrase(selectionTitle + ': accept bounty card ' + listingId + ' with deliverable, rating, and risk controls.', selectionTitle + ': 接取任务牌 ' + listingId + '，定义成果、评级和风险控制。'),
        listingStatus: (listingId) => routePhrase('Connect bounty card ' + listingId + ' into the adventure route.', '把任务牌 ' + listingId + ' 接入冒险路线。'),
        defaultBody: (selectionTitle) => routePhrase(selectionTitle + ': draft the next world action for this map focus with evidence, risk, and route plan.', selectionTitle + ': 为这个地图焦点起草下一步世界行动，带上证据、风险和推进路线。'),
        defaultStatus: () => routePhrase('Draft a world action from the current focus.', '从当前焦点起草世界行动。'),
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
          return `<article class="module app-route-task-graph-item"><strong>${{escapeHtml(mapText(task.task_id || 'route task / 路线任务'))}}</strong><span>${{escapeHtml(mapText(task.latest_bucket || 'event'))}} · ${{escapeHtml(mapText(task.latest_status || 'pending'))}} · ${{escapeHtml(mapText('branch / 支线'))}} ${{escapeHtml(mapText(task.next_opportunity_kind || 'contract_capture'))}}</span><p>${{escapeHtml(task.event_count ?? 0)}} ${{escapeHtml(mapText('events / 事件'))}} · ${{escapeHtml(task.contract_count ?? 0)}} ${{escapeHtml(mapText('commissions / 委托'))}} · ${{escapeHtml(task.completion_count ?? 0)}} ${{escapeHtml(mapText('battle reports / 战报'))}}</p><p>${{escapeHtml(mapText(task.outcome_summary || '战果总结待生成。'))}}</p><p><strong>${{escapeHtml(mapText('next branch / 下一条支线'))}}</strong> · ${{escapeHtml(mapText(task.next_opportunity_hint || '支线提示待生成。'))}}</p><p>${{escapeHtml(mapText(task.next_opportunity_playbook || '支线打法待生成。'))}}</p><div class="focus-stack"><code>${{escapeHtml(mapText(task.next_opportunity_command || '/world action 继续推进下一步机会。'))}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
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
        pushUniqueRouteAction(actions, buildDraftWorldAction(locationId, activeTaskId, buildRouteDraftBody(selection, {{}}, {{ selectionTitleFallback: routePhrase('current route', '当前路线'), omitContextDetails: true, leadIn: routePhrase(': continue advancing ', ': 继续推进 '), emptyDetail: routePhrase('this map adventure route', '这条地图冒险路线'), suffix: routePhrase(', adding evidence, risk judgment, and next action.', '，补齐证据、风险判断和下一步行动。') }})));
        if (activeTaskId) pushUniqueRouteAction(actions, buildTaskFollowUpAction(selection, activeTaskId, locationId, routePhrase('Combine linked event/contract status, evidence, blockers, and next action', '结合关联事件/契约状态、证据、阻碍和下一步行动')));
        if (contractId) pushUniqueRouteAction(actions, buildLinkedContractRouteAction(selection, contractId, activeTaskId, locationId));
        if (linkedEventItem) pushUniqueRouteAction(actions, buildRouteEventTimelineAction(routePhrase('Open linked event', '打开关联事件'), {{
          locationId,
          eventId: String(linkedEventItem.event_id || '').trim(),
          eventKind: String(linkedEventItem.title || 'world_event'),
          eventBody: String(linkedEventItem.summary || '').trim(),
          eventResult: String(linkedEventItem.route_status || '').trim(),
          eventTaskId: activeTaskId,
          body: appendSelectionEventSignal(routePhrase(((routeSelection && routeSelection.title) || 'current route') + ': review linked event ' + String(linkedEventItem.title || 'event') + ' before the next world action.', ((routeSelection && routeSelection.title) || '当前路线') + ': 在下一步世界行动前复盘关联事件 ' + String(linkedEventItem.title || 'event') + '。'), selection),
        }}));
        lastRouteActions = actions;
        if (routeActionRail) {{
          routeActionRail.innerHTML = actions.map((action, index) => indexedRouteActionButtonHtml(action, index)).join(' ');
        }}
        if (routeStatus) routeStatus.textContent = routeFilterMode === 'all'
          ? routePhrase('Adventure route: full route overview' + (selection ? (' · focus ' + mapText(selection.title || 'focus')) : '') + ' · ' + filteredItems.length + ' routes' + (activeTaskId ? (' · task ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.', '冒险路线：显示完整路线概览' + (selection ? (' · 焦点 ' + (selection.title || 'focus')) : '') + ' · ' + filteredItems.length + ' 条路线' + (activeTaskId ? (' · 任务 ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.')
          : (routeSelection ? routePhrase('Adventure route: ' + mapText(routeSelection.title || 'focus') + ' · ' + (locationId || 'unknown place') + ' · ' + filteredItems.length + ' linked routes' + (activeTaskId ? (' · task ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.', '冒险路线：' + (routeSelection.title || 'focus') + ' · ' + (locationId || '未知地点') + ' · ' + filteredItems.length + ' 条关联路线' + (activeTaskId ? (' · 任务 ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.') : routePhrase('Adventure route: no map focus yet, showing latest routes.', '冒险路线：暂无地图焦点，显示最新路线。'));
        if (routeNextStepStatus) routeNextStepStatus.textContent = (nextStep && nextStep.status) || routePhrase('Recommended next step: draft a world action from the current focus.', '推荐下一步：从当前焦点起草世界行动。');
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
          renderCards(taskRouteTarget, filterAvatarTaskRoutes(lastViewport.avatar_task_routes || [], focus), 'taskRoute');
          renderCards(routeRunnerTarget, filterAvatarRouteRunners(lastViewport.avatar_route_runners || [], focus), 'routeRunner');
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
        if (kind === 'route_runner_handoff') return 'route_task';
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
        const runnerHandoff = (payload.route_runner_handoff || {{}});
        const runnerCount = runnerHandoff.runner_count ?? 0;
        const rewardClaimCount = runnerHandoff.reward_claim_action_count ?? 0;
        const nextRouteCount = runnerHandoff.next_route_action_count ?? 0;
        const nextRouteStatus = String(runnerHandoff.first_next_route_status || 'next_route_preview_locked_until_reward_claim');
        const routeMasteryContract = String(runnerHandoff.route_mastery_contract_version || 'trillionnium_route_mastery_v1');
        const routeMasteryTier = String(runnerHandoff.first_route_mastery_tier || 'route_novice');
        const routeMasteryXp = String(runnerHandoff.first_route_mastery_xp ?? 0);
        const handoffSummary = String(runnerHandoff.summary || 'Route runner handoff: waiting for avatar task routes to unlock reward and next-route actions.');
        const chips = [
          `<span class="hud-chip"><strong>${{escapeHtml((visibleItems || []).length)}}</strong> 条可见动态</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(payload.item_count ?? 0)}}</strong> 条总动态</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(contracts)}}</strong> 个委托 · <strong>${{escapeHtml(completions)}}</strong> 份战报</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(purchaseCount)}}</strong> 次接取 · <strong>${{escapeHtml(workOrderCount)}}</strong> 个冒险委托</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(nearbyAgents)}}</strong> 位附近角色 · ${{escapeHtml(payload.active_region_id || 'global')}}</span>`,
          `<span id="app-feed-route-runner-handoff" class="hud-chip" data-next-route-status="${{escapeHtml(nextRouteStatus)}}" data-runner-count="${{escapeHtml(runnerCount)}}" data-reward-claim-count="${{escapeHtml(rewardClaimCount)}}" data-next-route-count="${{escapeHtml(nextRouteCount)}}" data-route-mastery-contract="${{escapeHtml(routeMasteryContract)}}" data-route-mastery-tier="${{escapeHtml(routeMasteryTier)}}" data-route-mastery-xp="${{escapeHtml(routeMasteryXp)}}"><strong>${{escapeHtml(runnerCount)}}</strong> runner · <strong>${{escapeHtml(rewardClaimCount)}}</strong> reward · <strong>${{escapeHtml(nextRouteCount)}}</strong> next route · ${{escapeHtml(handoffSummary)}}</span>`
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
      const mapRumSurfaceId = 'app';
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
        escape_client_app_visible_text(onboarding_label),
        escape_client_app_visible_text(onboarding_goal),
        escape_client_app_visible_text(&client_app_readiness_label(onboarding_completion_target)),
        onboarding_quick_path_label_html,
        onboarding_quick_path_summary_html,
        onboarding_quick_path_step_items,
        escape_html_text(playability_coach_version),
        playability_coach_lane_cards,
        funnel_percent,
        escape_client_app_visible_text(retention_reason),
        escape_html_text(route_runner_funnel_contract),
        route_runner_funnel_time_to_reward,
        route_runner_funnel_daily_resume_count,
        reward_to_next_route_percent,
        d1_resume_percent,
        abandon_or_recovery_percent,
        time_to_first_proof_seconds,
        time_to_next_route_seconds,
        route_runner_funnel_chips,
        route_archetype_cards,
        commercial_dashboard_chips,
        economy_tradeoff_cards,
        economy_ops_chips,
        onboarding_acceptance_chips,
        onboarding_command_disclosure_summary_html,
        onboarding_command_disclosure_copy_html,
        onboarding_step_cards,
        message_cards,
        app_bootstrap_bytes,
        escape_html_text(map_product_name),
        escape_html_text(map_product_name),
        escape_html_text(map_upgrade_model),
        escape_html_text(map_upgrade_model),
        escape_html_text(&client_app_map_label(map_density_summary)),
        escape_html_text(map_route_runner_next_route_status),
        map_avatar_route_runner_count,
        map_route_runner_reward_claim_count,
        map_route_runner_next_route_count,
        escape_html_text(map_route_runner_handoff_summary),
        app_tactics_player_hud,
        escape_html_text(map_engine_name),
        escape_html_text(tile_provider),
        escape_html_text(mirror_scope),
        escape_html_text(active_region_id),
        region_shard_count,
        lod_layer_count,
        escape_html_text(viewport_path_template),
        escape_html_text(web_session_viewport_path_template),
        escape_html_text(map_engine_id),
        map_stream_region_count,
        map_visible_marker_count,
        map_prefetch_count,
        map_live_event_count,
        escape_html_text(&client_app_map_label(map_player_density_mode)),
        map_avatar_task_route_count,
        map_avatar_route_runner_count,
        map_player_avatar_count,
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

pub(super) async fn get_client_app_web_shell_response(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let html = get_client_app_web_shell(State(state), headers).await.0;
    html_resource_response(html, "trillionnium_world_map_app_shell_payload_v1")
}
