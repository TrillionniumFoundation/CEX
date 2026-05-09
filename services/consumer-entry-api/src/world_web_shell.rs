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
        ("选中单位", "Select unit / 选中单位"),
        ("行军路线", "Move route / 行军路线"),
        ("发起攻击", "Attack / 发起攻击"),
        ("施展技能", "Use skill / 施展技能"),
        ("导师修炼", "Mentor training / 导师修炼"),
        ("交谈问路", "Talk to NPC / 交谈问路"),
        ("接取Trillionnium任务", "Accept Trillionnium task / 接取Trillionnium任务"),
        ("提交任务战报", "Submit task report / 提交任务战报"),
        ("接取悬赏", "Accept bounty / 接取悬赏"),
        ("查看底图", "Inspect underlay / 查看底图"),
        ("结束回合", "End turn / 结束回合"),
        ("聚焦区域", "Focus region / 聚焦区域"),
        ("聚焦热点", "Focus hotspot / 聚焦热点"),
        ("查看分片", "View tile / 查看分片"),
        ("预热分片", "Warm tile / 预热分片"),
        ("追踪事件", "Track event / 追踪事件"),
        ("按焦点筛选路线", "Filter routes by focus / 按焦点筛选路线"),
        ("显示全部路线", "Show all routes / 显示全部路线"),
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

fn world_tactics_board_cells_html(tactics_board: &Value) -> String {
    tactics_board
        .get("board")
        .and_then(|board| board.get("cells"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|cell| {
            let tile_id = cell
                .get("tile_id")
                .and_then(Value::as_str)
                .unwrap_or("A1");
            let terrain = cell
                .get("terrain")
                .and_then(Value::as_str)
                .unwrap_or("plain");
            let overlay_id = cell
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let movement_cost = cell
                .get("movement_cost")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let grid_row = cell
                .get("grid_row")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let grid_col = cell
                .get("grid_column")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            format!(
                "<button type=\"button\" role=\"gridcell\" class=\"tactics-tile terrain-{}\" data-tile=\"{}\" data-draft-target-tile=\"{}\" data-terrain=\"{}\" data-grid-row=\"{}\" data-grid-col=\"{}\" data-osm-game-overlay-id=\"{}\" data-movement-cost=\"{}\" data-board-cell-interaction-contract=\"{}\" data-command-intent-draft-contract=\"{}\" data-accessibility-contract=\"{}\" data-selection-role=\"target_tile\" data-keyboard-focus-role=\"target_tile_gridcell\" data-roving-tabindex=\"tactics_board\" data-draft-input-name=\"target_tile\" data-validation-owner=\"rust_tactics_command_validator\" data-source-of-truth=\"rust_tactics_board_projection\" data-web-role=\"intent_only_visualization_input\" aria-rowindex=\"{}\" aria-colindex=\"{}\" aria-selected=\"false\" aria-describedby=\"world-tactics-keyboard-help world-tactics-command-draft-status\" aria-label=\"Select tactical tile {} row {} column {}, terrain {}, movement cost {}\" tabindex=\"-1\"><small>{}</small></button>",
                escape_html_text(terrain),
                escape_html_text(tile_id),
                escape_html_text(tile_id),
                escape_html_text(terrain),
                grid_row,
                grid_col,
                escape_html_text(overlay_id),
                movement_cost,
                escape_html_text(TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION),
                escape_html_text(TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION),
                escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
                grid_row,
                grid_col,
                escape_html_text(tile_id),
                grid_row,
                grid_col,
                escape_html_text(terrain),
                movement_cost,
                escape_html_text(tile_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_tactics_unit_english_label(unit_id: &str, side: &str, label: &str) -> &'static str {
    match unit_id {
        "lord" => "Hero",
        "strategist" => "Advisor",
        "scout" => "Scout",
        "market-bandit" => "Bandit",
        "street-raider" => "Raider",
        _ if side == "enemy" => "Enemy",
        _ if side == "player" => "Hero",
        _ if label.chars().count() <= 1 => "Unit",
        _ => "Unit",
    }
}

fn world_tactics_objective_english_label(label: &str) -> &'static str {
    match label {
        "市" => "Market",
        "战" => "Battle",
        "路" => "Route",
        "师" => "Mentor",
        "赏" => "Bounty",
        _ => "Objective",
    }
}

fn world_tactics_battle_log_english(kind: &str) -> &'static str {
    match kind {
        "status" => {
            "> status: live events, bounty cards, and moving squads are bound to Rust state."
        }
        "objective" => {
            "> objective: capture the real-street objective and keep the evidence package attached."
        }
        "movement" => {
            "> movement: the squad advances across the street grid without browser-owned state."
        }
        "combat" => "> combat: deterministic Rust resolution controls target, damage, and result.",
        "reward" => "> reward: settlement remains proof-gated before XP or bounty release.",
        _ => "> log: tactics state projected from Rust.",
    }
}

fn world_trillionnium_visible_english(value: &str) -> &'static str {
    match value {
        "镜城游侠" => "Mirror City Ranger",
        "初入Trillionnium" => "New Trillionnium Adventurer",
        "街巷游侠" => "Street Ranger",
        "镜城夜巡" => "Mirror City Night Watch",
        "玄门导师" => "Mystic Mentor",
        "市集掌柜" => "Market Keeper",
        "镖局总管" => "Escort Chief",
        "Trillionnium说书人" => "Trillionnium Storyteller",
        "青石街导师" => "Bluestone Street Mentor",
        "桥边账房" => "Bridge Ledger Clerk",
        "夜巡步" => "Night Patrol Step",
        "市井拳" => "Market Fist",
        "轻身术" => "Lightfoot Technique",
        "听风诀" => "Wind-Listening Method",
        "护镖心法" => "Escort Guard Method",
        "青石门" => "Bluestone Sect",
        "市井盟" => "Market Alliance",
        "镖局会" => "Escort Guild",
        _ => "Trillionnium state",
    }
}

fn world_trillionnium_skill_english_label(skill_id: &str) -> &'static str {
    match skill_id {
        "basic_inner_power" => "Inner Power",
        "basic_unarmed" => "Unarmed Form",
        "basic_blade" => "Blade Form",
        "basic_sword" => "Sword Form",
        "basic_lightness" => "Lightness Step",
        "reading_and_contracts" => "Contracts Reading",
        "merchant_routecraft" => "Merchant Routecraft",
        "artifact_crafting" => "Artifact Crafting",
        "streetwise_investigation" => "Streetwise Investigation",
        _ => "Trillionnium Skill",
    }
}

fn world_tactics_units_html(tactics_board: &Value) -> String {
    tactics_board
        .get("units")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|unit| {
            let unit_id = unit
                .get("unit_id")
                .and_then(Value::as_str)
                .unwrap_or("unit");
            let side = unit.get("side").and_then(Value::as_str).unwrap_or("ally");
            let label = unit.get("label").and_then(Value::as_str).unwrap_or("?");
            let label_en = world_tactics_unit_english_label(unit_id, side, label);
            let title = unit.get("title").and_then(Value::as_str).unwrap_or(unit_id);
            let grid_column = unit
                .get("grid_column")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let grid_row = unit.get("grid_row").and_then(Value::as_i64).unwrap_or(1);
            let hp = unit.get("hp").and_then(Value::as_i64).unwrap_or(1);
            let unit_move = unit.get("move").and_then(Value::as_i64).unwrap_or(1);
            let tile_id = unit
                .get("position")
                .and_then(|position| position.get("tile_id"))
                .and_then(Value::as_str)
                .unwrap_or("A1");
            let overlay_id = unit
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let class_name = match side {
                "player" => "player",
                "enemy" => "enemy",
                _ => "ally",
            };
            format!(
                "<button type=\"button\" class=\"tactics-unit {}\" style=\"grid-column:{};grid-row:{}\" data-unit=\"{}\" data-draft-unit-id=\"{}\" data-side=\"{}\" data-tile=\"{}\" data-hp=\"{}\" data-move=\"{}\" data-osm-game-overlay-id=\"{}\" data-unit-selection-contract=\"{}\" data-command-intent-draft-contract=\"{}\" data-accessibility-contract=\"{}\" data-selection-role=\"active_unit\" data-keyboard-focus-role=\"active_unit_button\" data-draft-input-name=\"unit_id\" data-validation-owner=\"rust_tactics_command_validator\" data-source-of-truth=\"rust_tactics_unit_model\" data-web-role=\"intent_only_visualization_input\" aria-selected=\"false\" aria-describedby=\"world-tactics-keyboard-help world-tactics-command-draft-status\" aria-label=\"Select {} unit {} at tile {}, HP {}, movement {}\" title=\"{}\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</button>",
                class_name,
                grid_column,
                grid_row,
                escape_html_text(unit_id),
                escape_html_text(unit_id),
                escape_html_text(side),
                escape_html_text(tile_id),
                hp,
                unit_move,
                escape_html_text(overlay_id),
                escape_html_text(TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION),
                escape_html_text(TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION),
                escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
                escape_html_text(label_en),
                escape_html_text(unit_id),
                escape_html_text(tile_id),
                hp,
                unit_move,
                escape_html_text(title),
                escape_html_text(label_en),
                escape_html_text(label),
                escape_html_text(label_en),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_tactics_objectives_html(tactics_board: &Value) -> String {
    tactics_board
        .get("objectives")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|objective| {
            let objective_id = objective
                .get("objective_id")
                .and_then(Value::as_str)
                .unwrap_or("objective");
            let label = objective
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("赏");
            let label_en = world_tactics_objective_english_label(label);
            let title = objective
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("占领目标 / Objective");
            let grid_column = objective
                .get("grid_column")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let grid_row = objective
                .get("grid_row")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let overlay_id = objective
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let contract = objective
                .get("contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_osm_objective_v1");
            let completion_owner = objective
                .get("completion_owner")
                .and_then(Value::as_str)
                .unwrap_or("rust_command_handler_ledger_progression");
            let source_of_truth = objective
                .get("source_of_truth")
                .and_then(Value::as_str)
                .unwrap_or("rust_trillionnium_osm_objective_generator");
            format!(
                "<span class=\"tactics-marker objective\" style=\"grid-column:{};grid-row:{}\" data-objective=\"{}\" data-objective-contract=\"{}\" data-osm-game-overlay-id=\"{}\" data-completion-owner=\"{}\" data-source-of-truth=\"{}\" title=\"{}\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span>",
                grid_column,
                grid_row,
                escape_html_text(objective_id),
                escape_html_text(contract),
                escape_html_text(overlay_id),
                escape_html_text(completion_owner),
                escape_html_text(source_of_truth),
                escape_html_text(title),
                escape_html_text(label_en),
                escape_html_text(label),
                escape_html_text(label_en),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_tactics_battle_log_html(tactics_board: &Value) -> String {
    tactics_board
        .get("battle_log")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("log");
            let text = entry.get("text").and_then(Value::as_str).unwrap_or(">");
            let text_en = world_tactics_battle_log_english(kind);
            format!(
                "<p data-log-kind=\"{}\" data-source-of-truth=\"rust_tactics_board_projection\" data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</p>",
                escape_html_text(kind),
                escape_html_text(text_en),
                escape_html_text(text),
                escape_html_text(text_en),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_tactics_session_html(tactics_board: &Value) -> String {
    let session = tactics_board.get("game_session").unwrap_or(&Value::Null);
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
    let overlay_contract = tactics_board
        .get("map_overlay_identity_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_map_overlay_identity_v1");
    let session_id = session
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("world-tactics-session:projected");
    let persistence_status = session
        .get("persistence_status")
        .and_then(Value::as_str)
        .unwrap_or("persisted");
    let active_unit = session
        .get("active_unit_id")
        .and_then(Value::as_str)
        .unwrap_or("lord");
    let active_overlay = session
        .get("active_overlay_id")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium-world-node:mirror-city-square");
    let round = session.get("round").and_then(Value::as_i64).unwrap_or(1);
    let action_points = session
        .get("action_points_remaining")
        .and_then(Value::as_i64)
        .unwrap_or(2);
    let current_tick = session
        .get("current_tick")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let objective_progress = session
        .get("objective_progress")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let objective_goal = session
        .get("objective_goal")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let victory_state = session
        .get("victory_state")
        .and_then(Value::as_str)
        .unwrap_or("active");
    let reward_status = session
        .get("reward_status")
        .and_then(Value::as_str)
        .unwrap_or("not_eligible");
    let tick_count = tactics_board
        .get("simulation_ticks")
        .and_then(Value::as_array)
        .map(|ticks| ticks.len())
        .unwrap_or(0);
    let overlay_count = tactics_board
        .get("map_overlay_identity_index")
        .and_then(Value::as_array)
        .map(|identities| identities.len())
        .unwrap_or(0);
    format!(
        "<article id=\"trillionnium-tactics-session-state\" class=\"tactics-base-note\" data-session-contract=\"{}\" data-tick-contract=\"{}\" data-tactics-reward-settlement-contract=\"{}\" data-map-overlay-identity-contract=\"{}\" data-session-id=\"{}\" data-persistence-status=\"{}\" data-current-tick=\"{}\" data-objective-progress=\"{}\" data-objective-goal=\"{}\" data-victory-state=\"{}\" data-reward-status=\"{}\" data-tick-count=\"{}\" data-overlay-identity-count=\"{}\" data-source-of-truth=\"rust_world_tactics_game_session\"><strong data-i18n-en=\"Tactics session\" data-i18n-zh=\"战棋会话\">Tactics session</strong><span>round {} · AP {} · unit {} · objective {}/{} · {}</span><small>reward {} · overlay {} · identities {}</small></article>",
        escape_html_text(session_contract),
        escape_html_text(tick_contract),
        escape_html_text(reward_contract),
        escape_html_text(overlay_contract),
        escape_html_text(session_id),
        escape_html_text(persistence_status),
        current_tick,
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_html_text(reward_status),
        tick_count,
        overlay_count,
        round,
        action_points,
        escape_html_text(active_unit),
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_html_text(reward_status),
        escape_html_text(active_overlay),
        overlay_count,
    )
}

fn world_first_tactics_route_task(route_task_graph: Option<&Value>) -> Option<&Value> {
    route_task_graph
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

fn world_tactics_repeat_block_count(tactics_board: &Value, binding: Option<&Value>) -> u64 {
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

fn world_tactics_reward_history_cards_html(
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
                    "<article class=\"mini tactics-reward-history-stage\" data-history-stage=\"{}\" data-history-status=\"{}\"><strong>{}</strong><span>{}</span><small>{}</small></article>",
                    escape_html_text(stage),
                    escape_html_text(status),
                    escape_world_visible_text(label),
                    escape_world_visible_text(status),
                    escape_world_visible_text(summary),
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
        "<article class=\"mini tactics-reward-history-stage\" data-history-stage=\"objective_progress\" data-history-status=\"{}\"><strong data-i18n-en=\"Tactics objective\" data-i18n-zh=\"战棋目标\">Tactics objective</strong><span>Progress {}/{}</span><small data-i18n-en=\"Rust session owns objective progress.\" data-i18n-zh=\"Rust 会话拥有目标进度。\">Rust session owns objective progress.</small></article>\n<article class=\"mini tactics-reward-history-stage\" data-history-stage=\"victory_state\" data-history-status=\"{}\"><strong data-i18n-en=\"Victory state\" data-i18n-zh=\"胜负状态\">Victory state</strong><span>{}</span><small data-i18n-en=\"Browser only shows the result; Rust resolves combat.\" data-i18n-zh=\"浏览器只展示结果；Rust 结算战斗。\">Browser only shows the result; Rust resolves combat.</small></article>\n<article class=\"mini tactics-reward-history-stage\" data-history-stage=\"reward_settlement\" data-history-status=\"{}\"><strong data-i18n-en=\"Reward settlement\" data-i18n-zh=\"奖励结算\">Reward settlement</strong><span>{}</span><small data-i18n-en=\"Route-runner history unlocks after server settlement.\" data-i18n-zh=\"服务器结算后解锁路线角色历史。\">Route-runner history unlocks after server settlement.</small></article>",
        if objective_progress >= objective_goal { "completed" } else { "in_progress" },
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_world_visible_text(victory_state),
        escape_html_text(reward_status),
        escape_world_visible_text(reward_status),
    )
}

fn world_tactics_player_hud_html(
    tactics_board: &Value,
    route_task_graph: Option<&Value>,
    surface_id: &str,
) -> String {
    let session = tactics_board.get("game_session").unwrap_or(&Value::Null);
    let route_task = world_first_tactics_route_task(route_task_graph);
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
    let repeat_block_count = world_tactics_repeat_block_count(tactics_board, binding);
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
        world_tactics_reward_history_cards_html(session, route_task, binding);
    let shell_id = format!("{surface_id}-tactics-player-hud");
    let objective_card_id = format!("{surface_id}-tactics-objective-card");
    let session_card_id = format!("{surface_id}-tactics-current-session-card");
    let reward_card_id = format!("{surface_id}-tactics-reward-history-handoff");
    let anti_cheese_card_id = format!("{surface_id}-tactics-repeat-farming-copy");
    format!(
        "<section id=\"{}\" class=\"tactics-player-hud mini-grid\" data-contract-version=\"trillionnium_tactics_player_visible_surface_v1\" data-surface=\"{}\" data-source-of-truth=\"rust_world_tactics_sessions\" data-web-role=\"visualization_input_only\">\n  <article id=\"{}\" class=\"mini tactics-objective-card\" data-session-contract=\"{}\" data-objective-id=\"{}\" data-route-task-id=\"{}\" data-objective-progress=\"{}\" data-objective-goal=\"{}\" data-victory-state=\"{}\" data-reward-status=\"{}\"><strong data-i18n-en=\"Current tactics objective\" data-i18n-zh=\"当前战棋目标\">Current tactics objective</strong><span>{}</span><small>progress {}/{} · command {}</small><code>{}</code></article>\n  <article id=\"{}\" class=\"mini tactics-session-card\" data-session-id=\"{}\" data-active-unit-id=\"{}\" data-active-overlay-id=\"{}\" data-current-tick=\"{}\" data-action-points-remaining=\"{}\" data-tick-contract=\"{}\"><strong data-i18n-en=\"Current session state\" data-i18n-zh=\"当前会话状态\">Current session state</strong><span>unit {} · AP {} · tick {}</span><small>{} · reward {}</small></article>\n  <article id=\"{}\" class=\"mini tactics-reward-history-card\" data-reward-history-contract=\"{}\" data-reward-contract=\"{}\" data-route-task-id=\"{}\" data-reward-status=\"{}\"><strong data-i18n-en=\"Reward-history handoff\" data-i18n-zh=\"奖励历史交接\">Reward-history handoff</strong><span>{}</span><div class=\"mini-grid\">{}</div></article>\n  <article id=\"{}\" class=\"mini tactics-anti-cheese-card\" data-anti-cheese-contract=\"{}\" data-repeat-farming-block-count=\"{}\" data-result=\"{}\" data-gate-owner=\"rust_tactics_repeat_farming_guard\"><strong data-i18n-en=\"Repeat-farming guard\" data-i18n-zh=\"反刷守卫\">Repeat-farming guard</strong><span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span><small data-i18n-en=\"Browser submits intent only; Rust blocks settled reward farming.\" data-i18n-zh=\"浏览器只提交意图；Rust 拦截已结算奖励的重复刷取。\">Browser submits intent only; Rust blocks settled reward farming.</small></article>\n</section>",
        escape_html_text(&shell_id),
        escape_html_text(surface_id),
        escape_html_text(&objective_card_id),
        escape_html_text(session_contract),
        escape_html_text(objective_id),
        escape_html_text(&route_task_id),
        objective_progress,
        objective_goal,
        escape_html_text(victory_state),
        escape_html_text(reward_status),
        escape_world_visible_text(objective_label),
        objective_progress,
        objective_goal,
        escape_world_visible_text(objective_command),
        escape_html_text(&route_task_id),
        escape_html_text(&session_card_id),
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
        escape_html_text(&reward_card_id),
        escape_html_text(reward_history_contract),
        escape_html_text(reward_contract),
        escape_html_text(&route_task_id),
        escape_html_text(reward_status),
        escape_world_visible_text(reward_history_summary),
        reward_history_cards,
        escape_html_text(&anti_cheese_card_id),
        escape_html_text(anti_cheese_contract),
        repeat_block_count,
        if repeat_block_count > 0 { "repeat_farming_blocked" } else { "repeat_farming_watch" },
        escape_html_text(&repeat_copy_en),
        escape_html_text(&repeat_copy_zh),
        escape_html_text(&repeat_copy_en),
    )
}

fn world_tactics_command_grid_html(tactics_board: &Value) -> String {
    tactics_board
        .get("available_commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let command_id = command
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("inspect");
            let label = command
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(command_id);
            let web_target = command
                .get("web_target")
                .and_then(Value::as_str)
                .unwrap_or("#trillionnium-tactics-game-shell");
            let command_contract = command
                .get("contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_world_tactics_command_v1");
            let validation_owner = command
                .get("validation_owner")
                .and_then(Value::as_str)
                .unwrap_or("rust_tactics_command_validator");
            let required_skill_id = command
                .get("required_skill_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let target_tile_required = command
                .get("target_tile_required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let class_name = if index == 0 {
                "tactics-command primary"
            } else {
                "tactics-command"
            };
            format!(
                "<a class=\"{}\" role=\"button\" href='{}' data-command=\"{}\" data-draft-command=\"{}\" data-command-contract=\"{}\" data-command-intent-draft-contract=\"{}\" data-accessibility-contract=\"{}\" data-keyboard-focus-role=\"command_button\" data-validation-owner=\"{}\" data-required-skill-id=\"{}\" data-target-tile-required=\"{}\" data-unit-selection-required=\"true\" data-draft-input-name=\"command\" data-draft-owner=\"browser_tactics_intent_builder\" data-source-of-truth=\"rust_tactics_command_model\" data-web-role=\"intent_only_visualization_input\" aria-controls=\"world-tactics-command-draft-form\" aria-label=\"Draft tactics command {}\">{}</a>",
                class_name,
                escape_html_text(web_target),
                escape_html_text(command_id),
                escape_html_text(command_id),
                escape_html_text(command_contract),
                escape_html_text(TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION),
                escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
                escape_html_text(validation_owner),
                escape_html_text(required_skill_id),
                target_tile_required,
                escape_html_text(label),
                escape_world_visible_text(label),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_tactics_first_unit_id_by_side(tactics_board: &Value, side: &str) -> String {
    tactics_board
        .get("units")
        .and_then(Value::as_array)
        .and_then(|units| {
            units.iter().find(|unit| {
                unit.get("side")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate == side)
            })
        })
        .and_then(|unit| unit.get("unit_id"))
        .and_then(Value::as_str)
        .unwrap_or(if side == "enemy" {
            "market-bandit"
        } else {
            "lord"
        })
        .to_string()
}

fn world_tactics_first_unit_tile_by_side(tactics_board: &Value, side: &str) -> String {
    tactics_board
        .get("units")
        .and_then(Value::as_array)
        .and_then(|units| {
            units.iter().find(|unit| {
                unit.get("side")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate == side)
            })
        })
        .and_then(|unit| unit.get("position"))
        .and_then(|position| position.get("tile_id"))
        .and_then(Value::as_str)
        .unwrap_or(if side == "enemy" { "F5" } else { "B2" })
        .to_string()
}

fn world_tactics_command_draft_panel_html(
    tactics_board: &Value,
    current_matrix_user_id: &str,
    csrf_input: &str,
) -> String {
    let default_unit_id = world_tactics_first_unit_id_by_side(tactics_board, "player");
    let default_target_tile = world_tactics_first_unit_tile_by_side(tactics_board, "enemy");
    let default_command = tactics_board
        .get("available_commands")
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands.iter().find(|command| {
                command
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == "attack")
            })
        })
        .and_then(|command| command.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("select_unit");
    let draft_body = format!(
        "tactics intent draft: command={default_command} unit={default_unit_id} target={default_target_tile}; browser selected only, Rust validates legality and outcome."
    );
    format!(
        "<section id=\"world-tactics-command-draft-panel\" class=\"tactics-command-draft-panel\" data-contract-version=\"{}\" data-board-cell-interaction-contract=\"{}\" data-unit-selection-contract=\"{}\" data-accessibility-contract=\"{}\" data-keyboard-traversal=\"roving_grid_focus\" data-low-motion-support=\"prefers_reduced_motion\" data-selected-command=\"{}\" data-selected-unit-id=\"{}\" data-selected-target-tile=\"{}\" data-draft-owner=\"browser_tactics_intent_builder\" data-command-handler-owner=\"rust_world_tactics_command_handler\" data-validation-owner=\"rust_tactics_command_validator\" data-source-of-truth=\"rust_tactics_command_model\" data-web-role=\"intent_only_visualization_input\" aria-describedby=\"world-tactics-command-draft-status world-tactics-keyboard-help\"><h4 data-i18n-en=\"Command draft · intent only\" data-i18n-zh=\"指令草稿 · 仅提交意图\">Command draft · intent only</h4><p id=\"world-tactics-command-draft-status\" role=\"status\" aria-live=\"polite\" data-i18n-en=\"Select a unit, a board cell, then a command. Browser drafts intent only; Rust validates movement, combat, objective, and reward.\" data-i18n-zh=\"选择单位、棋格和指令。浏览器只起草意图；移动、战斗、目标和奖励由 Rust 校验。\">Select a unit, a board cell, then a command. Browser drafts intent only; Rust validates movement, combat, objective, and reward.</p><small id=\"world-tactics-keyboard-help\" data-accessibility-contract=\"{}\" data-i18n-en=\"Keyboard: arrow keys move across board cells, Home/End jump across a row, Enter/Space select the focused tile or command. Reduced-motion follows system preference.\" data-i18n-zh=\"键盘：方向键在棋格间移动，Home/End 跳到行首/行尾，Enter/Space 选择焦点棋格或指令。低动效跟随系统设置。\">Keyboard: arrow keys move across board cells, Home/End jump across a row, Enter/Space select the focused tile or command. Reduced-motion follows system preference.</small><form id=\"world-tactics-command-draft-form\" method=\"post\" action=\"/world/web/tactics-command\" data-api-command-endpoint=\"/v1/world/tactics/command\" data-contract-version=\"{}\" data-accessibility-contract=\"{}\" data-source-of-truth=\"rust_world_tactics_command_handler\" data-web-role=\"intent_only_visualization_input\" aria-label=\"Submit tactics intent draft to Rust\">{}<input type=\"hidden\" name=\"matrix_user_id\" value=\"{}\"><input type=\"hidden\" name=\"command\" value=\"{}\"><input type=\"hidden\" name=\"unit_id\" value=\"{}\"><input type=\"hidden\" name=\"target_tile\" value=\"{}\"><input type=\"hidden\" name=\"skill_id\" value=\"basic_unarmed\"><input type=\"hidden\" name=\"body\" value=\"{}\"><button type=\"submit\" data-i18n-en=\"Submit drafted intent to Rust\" data-i18n-zh=\"提交草稿意图给 Rust\">Submit drafted intent to Rust</button></form><small id=\"world-tactics-command-draft-preview\" data-preview-format=\"command_unit_tile\" aria-live=\"polite\">draft: {} · {} → {}</small></section>",
        escape_html_text(TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION),
        escape_html_text(TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION),
        escape_html_text(TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION),
        escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
        escape_html_text(default_command),
        escape_html_text(&default_unit_id),
        escape_html_text(&default_target_tile),
        escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
        escape_html_text(TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION),
        escape_html_text(TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION),
        csrf_input,
        escape_html_text(current_matrix_user_id),
        escape_html_text(default_command),
        escape_html_text(&default_unit_id),
        escape_html_text(&default_target_tile),
        escape_html_text(&draft_body),
        escape_html_text(default_command),
        escape_html_text(&default_unit_id),
        escape_html_text(&default_target_tile),
    )
}

fn world_trillionnium_training_forms_html(
    tactics_board: &Value,
    current_matrix_user_id: &str,
    csrf_input: &str,
) -> String {
    tactics_board
        .get("training_commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|command| {
            let skill_id = command
                .get("skill_id")
                .and_then(Value::as_str)
                .unwrap_or("basic_unarmed");
            let mentor_npc_id = command
                .get("mentor_npc_id")
                .and_then(Value::as_str)
                .unwrap_or("npc-street-compass-sifu");
            let required_role = command
                .get("required_semantic_role")
                .and_then(Value::as_str)
                .unwrap_or("civic_square");
            let required_overlay_id = command
                .get("required_osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium-world-node:mirror-city-square");
            let cost_xp = command
                .get("cost_xp")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let cooldown_seconds = command
                .get("cooldown_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let contract_version = command
                .get("contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_training_command_v1");
            let skill_label_en = world_trillionnium_skill_english_label(skill_id);
            format!(
                "<form class=\"trillionnium-training-form\" method=\"post\" action=\"/world/web/tactics-command\" data-training-contract=\"{}\" data-command=\"train_skill\" data-validation-owner=\"rust_mentor_training_validator\" data-source-of-truth=\"rust_mentor_training_command_model\" data-web-role=\"intent_only_visualization_input\">{}<input type=\"hidden\" name=\"matrix_user_id\" value=\"{}\"><input type=\"hidden\" name=\"command\" value=\"train_skill\"><input type=\"hidden\" name=\"unit_id\" value=\"lord\"><input type=\"hidden\" name=\"target_tile\" value=\"G8\"><input type=\"hidden\" name=\"skill_id\" value=\"{}\"><input type=\"hidden\" name=\"osm_game_overlay_id\" value=\"{}\"><input type=\"hidden\" name=\"body\" value=\"mentor training: {} at {}\"><button type=\"submit\" data-i18n-en=\"Train {}\" data-i18n-zh=\"修炼 {}\">Train {}</button><small>mentor={} · place={} · cost={}xp · cooldown={}s</small></form>",
                escape_html_text(contract_version),
                csrf_input,
                escape_html_text(current_matrix_user_id),
                escape_html_text(skill_id),
                escape_html_text(required_overlay_id),
                escape_html_text(skill_id),
                escape_html_text(required_role),
                escape_html_text(skill_label_en),
                escape_html_text(skill_id),
                escape_html_text(skill_label_en),
                escape_html_text(mentor_npc_id),
                escape_html_text(required_role),
                cost_xp,
                cooldown_seconds,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_trillionnium_sect_cards_html(tactics_board: &Value) -> String {
    tactics_board
        .get("sects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|sect| {
            let sect_id = sect.get("sect_id").and_then(Value::as_str).unwrap_or("sect");
            let display_name = sect
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(sect_id);
            let specialization = sect
                .get("specialization")
                .and_then(Value::as_str)
                .unwrap_or("world_training");
            let anchor_role = sect
                .get("anchor_semantic_role")
                .and_then(Value::as_str)
                .unwrap_or("sect_hall");
            let overlay_id = sect
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let contract_version = sect
                .get("contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_sect_v1");
            let titles = sect
                .get("title_ladder")
                .and_then(Value::as_array)
                .map(|titles| {
                    titles
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" → ")
                })
                .unwrap_or_default();
            format!(
                "<article class=\"mini trillionnium-sect-card\" data-sect-id=\"{}\" data-sect-contract=\"{}\" data-osm-game-overlay-id=\"{}\" data-source-of-truth=\"rust_trillionnium_sect_model\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(sect_id),
                escape_html_text(contract_version),
                escape_html_text(overlay_id),
                escape_world_visible_text(display_name),
                escape_world_visible_text(specialization),
                escape_html_text(anchor_role),
                escape_world_visible_text(&titles),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_trillionnium_npc_cards_html(
    tactics_board: &Value,
    current_matrix_user_id: &str,
    csrf_input: &str,
) -> String {
    tactics_board
        .get("npcs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|npc| {
            let npc_id = npc.get("npc_id").and_then(Value::as_str).unwrap_or("npc");
            let display_name = npc
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(npc_id);
            let role = npc.get("role").and_then(Value::as_str).unwrap_or("mentor");
            let sect_id = npc
                .get("sect_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let overlay_id = npc
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("none");
            let contract_version = npc
                .get("contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_npc_v1");
            let spawn_contract_version = npc
                .get("spawn_contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_npc_spawn_anchor_v1");
            let relationship_contract = npc
                .get("relationship_contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_npc_relationship_v1");
            let relationship_score = npc
                .get("relationship")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let trust = npc.get("trust").and_then(Value::as_i64).unwrap_or(0);
            let risk_posture = npc
                .get("risk_posture")
                .and_then(Value::as_str)
                .unwrap_or("watchful");
            let command_forms = npc
                .get("command_descriptors")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|descriptor| {
                    let command = descriptor
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or("talk_npc");
                    let label = descriptor
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(command);
                    let descriptor_contract = descriptor
                        .get("contract_version")
                        .and_then(Value::as_str)
                        .unwrap_or("trillionnium_npc_command_descriptor_v1");
                    let validation_owner = descriptor
                        .get("validation_owner")
                        .and_then(Value::as_str)
                        .unwrap_or("rust_trillionnium_npc_interaction_validator");
                    let descriptor_overlay_id = descriptor
                        .get("required_osm_game_overlay_id")
                        .and_then(Value::as_str)
                        .unwrap_or(overlay_id);
                    let task_archetype_id = descriptor
                        .get("task_archetype_ids")
                        .and_then(Value::as_array)
                        .and_then(|ids| ids.iter().filter_map(Value::as_str).next())
                        .unwrap_or("");
                    let body_template = descriptor
                        .get("body_template")
                        .and_then(Value::as_str)
                        .unwrap_or("talk to Trillionnium NPC");
                    format!(
                        "<form class=\"trillionnium-npc-command-form\" method=\"post\" action=\"/world/web/tactics-command\" data-npc-command-contract=\"{}\" data-command=\"{}\" data-npc-id=\"{}\" data-validation-owner=\"{}\" data-source-of-truth=\"rust_trillionnium_npc_model\" data-web-role=\"intent_only_visualization_input\">{}<input type=\"hidden\" name=\"matrix_user_id\" value=\"{}\"><input type=\"hidden\" name=\"command\" value=\"{}\"><input type=\"hidden\" name=\"unit_id\" value=\"lord\"><input type=\"hidden\" name=\"target_tile\" value=\"G8\"><input type=\"hidden\" name=\"npc_id\" value=\"{}\"><input type=\"hidden\" name=\"task_archetype_id\" value=\"{}\"><input type=\"hidden\" name=\"osm_game_overlay_id\" value=\"{}\"><input type=\"hidden\" name=\"body\" value=\"{}\"><button type=\"submit\">{}</button></form>",
                        escape_html_text(descriptor_contract),
                        escape_html_text(command),
                        escape_html_text(npc_id),
                        escape_html_text(validation_owner),
                        csrf_input,
                        escape_html_text(current_matrix_user_id),
                        escape_html_text(command),
                        escape_html_text(npc_id),
                        escape_html_text(task_archetype_id),
                        escape_html_text(descriptor_overlay_id),
                        escape_html_text(body_template),
                        escape_world_visible_text(label),
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!(
                "<article class=\"mini trillionnium-npc-card\" data-npc-id=\"{}\" data-npc-contract=\"{}\" data-npc-spawn-contract=\"{}\" data-npc-relationship-contract=\"{}\" data-relationship-score=\"{}\" data-trust=\"{}\" data-risk-posture=\"{}\" data-sect-id=\"{}\" data-osm-game-overlay-id=\"{}\" data-source-of-truth=\"rust_trillionnium_npc_model\"><strong>{}</strong><span>{}</span><small>{} · relationship {} · trust {}</small><div class=\"trillionnium-npc-command-stack\">{}</div></article>",
                escape_html_text(npc_id),
                escape_html_text(contract_version),
                escape_html_text(spawn_contract_version),
                escape_html_text(relationship_contract),
                relationship_score,
                trust,
                escape_html_text(risk_posture),
                escape_html_text(sect_id),
                escape_html_text(overlay_id),
                escape_world_visible_text(display_name),
                escape_world_visible_text(role),
                escape_html_text(sect_id),
                relationship_score,
                trust,
                command_forms,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_trillionnium_task_candidate_forms_html(
    tactics_board: &Value,
    current_matrix_user_id: &str,
    csrf_input: &str,
) -> String {
    tactics_board
        .get("task_candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|candidate| {
            let candidate_id = candidate
                .get("candidate_id")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium-task:courier_letter");
            let task_archetype_id = candidate
                .get("task_archetype_id")
                .and_then(Value::as_str)
                .unwrap_or("courier_letter");
            let overlay_id = candidate
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium-world-node:mirror-city-square");
            let role = candidate
                .get("source_semantic_role")
                .and_then(Value::as_str)
                .unwrap_or("objective");
            let reward_gate = candidate
                .get("reward_gate")
                .and_then(Value::as_str)
                .unwrap_or("ledger_settlement_review_hold_anti_cheese");
            let completion_contract = candidate
                .get("completion_contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_task_completion_v1");
            let reward_gate_contract = candidate
                .get("reward_gate_contract_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_reward_gate_v1");
            let task_label = world_user_visible_copy(task_archetype_id);
            format!(
                "<form class=\"trillionnium-task-completion-form\" method=\"post\" action=\"/world/web/tactics-command\" data-candidate-id=\"{}\" data-command=\"complete_task\" data-completion-contract=\"{}\" data-reward-gate-contract=\"{}\" data-reward-gate=\"{}\" data-ledger-reward-requires-settlement=\"true\" data-review-hold-gate-enforced=\"true\" data-anti-cheese-gate-enforced=\"true\" data-source-of-truth=\"rust_trillionnium_task_completion_handler\" data-web-role=\"intent_only_visualization_input\">{}<input type=\"hidden\" name=\"matrix_user_id\" value=\"{}\"><input type=\"hidden\" name=\"command\" value=\"complete_task\"><input type=\"hidden\" name=\"unit_id\" value=\"lord\"><input type=\"hidden\" name=\"target_tile\" value=\"G8\"><input type=\"hidden\" name=\"task_archetype_id\" value=\"{}\"><input type=\"hidden\" name=\"osm_game_overlay_id\" value=\"{}\"><input type=\"hidden\" name=\"body\" value=\"Trillionnium task report: deliverable captured, evidence package attached, risk controls checked, next action queued, self-review complete.\"><button type=\"submit\" data-i18n-en=\"Submit report · {}\" data-i18n-zh=\"提交战报 · {}\">Submit report · {}</button><small>{} · {}</small></form>",
                escape_html_text(candidate_id),
                escape_html_text(completion_contract),
                escape_html_text(reward_gate_contract),
                escape_html_text(reward_gate),
                csrf_input,
                escape_html_text(current_matrix_user_id),
                escape_html_text(task_archetype_id),
                escape_html_text(overlay_id),
                escape_html_text(&task_label),
                escape_html_text(task_archetype_id),
                escape_html_text(&task_label),
                escape_html_text(role),
                escape_html_text(overlay_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn world_trillionnium_status_html(trillionnium_character: &Value) -> String {
    let attributes = trillionnium_character
        .get("attributes")
        .unwrap_or(&Value::Null);
    [
        ("Physique", "体魄", "physique"),
        ("Force", "臂力", "force"),
        ("Agility", "身法", "agility"),
        ("Insight", "悟性", "insight"),
        ("Resolve", "定力", "resolve"),
        ("Reputation", "声望", "reputation"),
    ]
    .into_iter()
    .map(|(label_en, label_zh, key)| {
        let value = attributes.get(key).and_then(Value::as_i64).unwrap_or(0);
        let width = if key == "reputation" {
            (50 + value).clamp(5, 100)
        } else {
            (value * 6).clamp(5, 100)
        };
        format!(
            "<div class=\"tactics-stat-line trillionnium-stat\" data-attribute=\"{}\" data-source-of-truth=\"rust_trillionnium_character\"><span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span><div class=\"tactics-meter\"><span style=\"width:{}%\"></span></div><b>{}</b></div>",
            escape_html_text(key),
            escape_html_text(label_en),
            escape_html_text(label_zh),
            escape_html_text(label_en),
            width,
            value,
        )
    })
    .collect::<Vec<_>>()
    .join("\n")
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
    Query(query): Query<HashMap<String, String>>,
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
    let console_note = if query.get("played").map(String::as_str) == Some("1") {
        "行动已结算：世界事件、奖励影响和下一步路线已写入。"
    } else if query.contains_key("recovery") {
        "行动没有丢失：请按恢复卡补齐证据、风险和下一步后重试。"
    } else if web_session.is_some() {
        "已登录的世界会话：行动会绑定当前玩家并通过 CSRF 保护。"
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "本地开发世界：探索城市、打造道具、招募 Agent，并把现实机会镜像成冒险事件。"
    } else {
        "只读世界：提交行动前需要先获取签名 /league/web/session。"
    };
    let recovery_notice_html = if query.contains_key("recovery") {
        r#"<article id="world-action-recovery-card" class="mini recovery-card" data-recovery="world_action_failure">
          <strong data-i18n-en="Recovery route ready" data-i18n-zh="恢复路线已准备">Recovery route ready</strong>
          <span data-i18n-en="Your web action hit a validation or persistence guard. Add deliverable, evidence, risk control, and next action, then resubmit from the same console." data-i18n-zh="网页行动触发了校验或持久化保护。补齐成果、证据、风险控制和下一步，然后从同一个行动台重新提交。">Your web action hit a validation or persistence guard. Add deliverable, evidence, risk control, and next action, then resubmit from the same console.</span>
          <code>/world action deliverable + evidence + risk + next</code>
        </article>"#
            .to_string()
    } else {
        String::new()
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
        .get(current_matrix_user_id)
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
    let map_product_name = real_world_map_engine
        .get("product_name")
        .and_then(Value::as_str)
        .unwrap_or("Trillionnium World Map");
    let map_product_name_html = escape_html_text(map_product_name);
    let map_upgrade_model = real_world_map_engine
        .get("gameplay_layer_contract")
        .and_then(|contract| contract.get("upgrade_model"))
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap upgraded with Trillionnium avatars, route nodes, quest cards, live events, and task completion loops.");
    let map_upgrade_model_html = escape_html_text(map_upgrade_model);
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
        .unwrap_or("openstreetmap_global_base_upgraded_with_trillionnium_avatar_task_layer");
    let simplification_style = real_world_map_engine
        .get("simplification_style")
        .and_then(Value::as_str)
        .unwrap_or("gather_hero_tale_lod");
    let scaling_goal = real_world_map_engine
        .get("scaling_goal")
        .and_then(Value::as_str)
        .unwrap_or("many_players_via_lightweight_nodes_routes_and_region_shards");
    let openstreetmap_geodata = world_map
        .get("openstreetmap_geodata")
        .cloned()
        .unwrap_or_else(|| openstreetmap_geodata_v1_json(&map_nodes, current_map_node));
    let osm_geodata_contract = openstreetmap_geodata
        .get("contract_version")
        .and_then(Value::as_str)
        .unwrap_or("openstreetmap_geodata_v1");
    let osm_geodata_provider_contract = openstreetmap_geodata
        .get("provider_contract")
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMapDataProvider");
    let osm_geodata_provider_id = openstreetmap_geodata
        .get("provider_id")
        .and_then(Value::as_str)
        .unwrap_or("fixture_openstreetmap_data_provider_v1");
    let osm_geodata_source_mode = openstreetmap_geodata
        .get("source_mode")
        .and_then(Value::as_str)
        .unwrap_or("local_fixture_mock_first_no_live_overpass");
    let osm_geodata_feature_count = openstreetmap_geodata
        .get("feature_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let osm_fixture_layers_contract = openstreetmap_geodata
        .get("fixture_layers_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("openstreetmap_fixture_layers_v1");
    let osm_provider_readiness = openstreetmap_geodata
        .get("provider_readiness")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let osm_provider_readiness_contract = osm_provider_readiness
        .get("contract_version")
        .and_then(Value::as_str)
        .unwrap_or("openstreetmap_provider_readiness_v1");
    let osm_provider_readiness_status = osm_provider_readiness
        .get("readiness_status")
        .and_then(Value::as_str)
        .unwrap_or("fixture_ready_live_fail_closed");
    let osm_fixture_mode_green = osm_provider_readiness
        .get("fixture_mode_green")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let osm_live_modes_fail_closed = osm_provider_readiness
        .get("live_modes_fail_closed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let osm_live_network_ingestion_enabled = osm_provider_readiness
        .get("live_network_ingestion_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let osm_production_ingestion_enabled = osm_provider_readiness
        .get("production_ingestion_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let osm_fail_closed_mode_count = osm_provider_readiness
        .get("fail_closed_mode_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let osm_expected_fail_closed_mode_count = osm_provider_readiness
        .get("expected_fail_closed_mode_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let osm_stable_fixture_identity_coverage_complete = osm_provider_readiness
        .get("stable_fixture_identity_coverage_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let osm_fixture_layer_feature_count = osm_provider_readiness
        .get("fixture_layer_feature_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let osm_geodata_feature_cards = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(6)
        .map(|feature| {
            let osm_type = feature
                .get("osm_type")
                .and_then(Value::as_str)
                .unwrap_or("node");
            let osm_id = feature.get("osm_id").and_then(Value::as_i64).unwrap_or(0);
            let name = feature
                .get("tags")
                .and_then(|tags| tags.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("OSM feature");
            let node_id = feature
                .get("game_binding")
                .and_then(|binding| binding.get("node_id"))
                .and_then(Value::as_str)
                .unwrap_or("world-node");
            let game_overlay_id = feature
                .get("game_overlay_id")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium-world-node:unknown");
            let lat = feature
                .get("lat_string")
                .and_then(Value::as_str)
                .unwrap_or("0.000000");
            let lng = feature
                .get("lng_string")
                .and_then(Value::as_str)
                .unwrap_or("0.000000");
            format!(
                "<article class=\"mini osm-feature\" data-osm-type=\"{}\" data-osm-id=\"{}\" data-game-overlay-id=\"{}\"><strong>{}</strong><span>osm_type={} · osm_id={} · {},{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(osm_type),
                osm_id,
                escape_html_text(game_overlay_id),
                escape_world_visible_text(name),
                escape_html_text(osm_type),
                osm_id,
                escape_html_text(lat),
                escape_html_text(lng),
                escape_html_text(node_id),
                escape_html_text(game_overlay_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tactics_board = world_map.get("tactics_board").cloned().unwrap_or_else(|| {
        world_tactics_board_projection_json(
            &league.world,
            current_matrix_user_id,
            current_map_node,
            &openstreetmap_geodata,
        )
    });
    let trillionnium_character = world_map
        .get("trillionnium_character")
        .cloned()
        .unwrap_or_else(|| {
            world_trillionnium_character_projection_json(&league.world, current_matrix_user_id)
        });
    let tactics_board_contract = tactics_board
        .get("contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_world_tactics_board_v1");
    let tactics_unit_contract = tactics_board
        .get("unit_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_world_tactics_unit_v1");
    let tactics_command_contract = tactics_board
        .get("command_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_world_tactics_command_v1");
    let trillionnium_skill_contract = tactics_board
        .get("trillionnium_skill_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_skill_v1");
    let trillionnium_npc_command_descriptor_contract = tactics_board
        .get("trillionnium_npc_command_descriptor_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_npc_command_descriptor_v1");
    let trillionnium_mentor_training_task_contract = tactics_board
        .get("mentor_training_task_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_mentor_training_task_v1");
    let trillionnium_task_archetype_contract = tactics_board
        .get("trillionnium_task_archetype_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_task_archetype_v1");
    let trillionnium_task_completion_contract = tactics_board
        .get("trillionnium_task_completion_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_task_completion_v1");
    let trillionnium_reward_gate_contract = tactics_board
        .get("trillionnium_reward_gate_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_reward_gate_v1");
    let trillionnium_battle_log_style_contract = tactics_board
        .get("trillionnium_battle_log_style_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_battle_log_style_v1");
    let trillionnium_combat_log_contract = tactics_board
        .get("trillionnium_combat_log_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_combat_log_v1");
    let trillionnium_npc_relationship_contract = tactics_board
        .get("trillionnium_npc_relationship_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_npc_relationship_v1");
    let trillionnium_osm_objective_contract = tactics_board
        .get("trillionnium_osm_objective_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_osm_objective_v1");
    let tactics_combat_resolution_contract = tactics_board
        .get("tactics_combat_resolution_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_combat_resolution_v1");
    let tactics_game_session_contract = tactics_board
        .get("tactics_game_session_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_game_session_v1");
    let tactics_simulation_tick_contract = tactics_board
        .get("tactics_simulation_tick_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_simulation_tick_v1");
    let tactics_reward_settlement_contract = tactics_board
        .get("tactics_reward_settlement_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_reward_settlement_v1");
    let tactics_accessibility_contract = tactics_board
        .get("tactics_accessibility_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_accessibility_v1");
    let map_overlay_identity_contract = tactics_board
        .get("map_overlay_identity_contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_map_overlay_identity_v1");
    let tactics_board_cells = world_tactics_board_cells_html(&tactics_board);
    let tactics_board_units = world_tactics_units_html(&tactics_board);
    let tactics_objective_markers = world_tactics_objectives_html(&tactics_board);
    let tactics_battle_log = world_tactics_battle_log_html(&tactics_board);
    let tactics_session_state = world_tactics_session_html(&tactics_board);
    let tactics_player_hud =
        world_tactics_player_hud_html(&tactics_board, world_map.get("route_task_graph"), "world");
    let tactics_command_grid = world_tactics_command_grid_html(&tactics_board);
    let tactics_command_draft_panel =
        world_tactics_command_draft_panel_html(&tactics_board, current_matrix_user_id, &csrf_input);
    let trillionnium_training_forms =
        world_trillionnium_training_forms_html(&tactics_board, current_matrix_user_id, &csrf_input);
    let trillionnium_sect_cards = world_trillionnium_sect_cards_html(&tactics_board);
    let trillionnium_npc_cards =
        world_trillionnium_npc_cards_html(&tactics_board, current_matrix_user_id, &csrf_input);
    let trillionnium_task_completion_forms = world_trillionnium_task_candidate_forms_html(
        &tactics_board,
        current_matrix_user_id,
        &csrf_input,
    );
    let trillionnium_status_lines = world_trillionnium_status_html(&trillionnium_character);
    let trillionnium_character_contract = trillionnium_character
        .get("contract_version")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_character_v1");
    let trillionnium_title = trillionnium_character
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("初入Trillionnium");
    let trillionnium_display_name = trillionnium_character
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("镜城游侠");
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
    let map_player_avatar_count = world_viewport
        .get("player_avatar_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_avatar_task_route_count = world_viewport
        .get("avatar_task_route_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let map_avatar_route_runner_count = world_viewport
        .get("avatar_route_runner_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_runner_handoff = world_viewport.get("route_runner_handoff");
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
    let map_route_runner_mastery_contract = route_runner_handoff
        .and_then(|handoff| handoff.get("route_mastery_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_route_mastery_v1");
    let map_route_runner_mastery_tier = route_runner_handoff
        .and_then(|handoff| handoff.get("first_route_mastery_tier"))
        .and_then(Value::as_str)
        .unwrap_or("route_novice");
    let map_route_runner_mastery_xp = route_runner_handoff
        .and_then(|handoff| handoff.get("first_route_mastery_xp"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let world_route_archetype_catalog = trillionnium_world_route_archetypes_json(
        map_avatar_task_route_count as i64,
        league.world.world_listings.len() as i64,
        league.world.world_work_orders.len() as i64,
        league.world.world_contract_completions.len() as i64,
        1,
    );
    let world_route_archetype_cards = world_route_archetype_catalog
        .get("archetypes")
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
            let proof_mode = archetype
                .get("proof_mode")
                .and_then(Value::as_str)
                .unwrap_or("proof package");
            let reward_model = archetype
                .get("reward_model")
                .and_then(Value::as_str)
                .unwrap_or("reward + next route");
            let risk_model = archetype
                .get("risk_model")
                .and_then(Value::as_str)
                .unwrap_or("review risk");
            let cta = archetype
                .get("primary_cta_copy")
                .and_then(Value::as_str)
                .unwrap_or("Run route");
            format!(
                "<article class=\"mini route-archetype\" data-route-archetype=\"{}\"><strong>{}</strong><span>{}</span><small>{}</small><code>{} · {}</code></article>",
                escape_html_text(archetype_id),
                escape_world_visible_text(label),
                escape_world_visible_text(proof_mode),
                escape_world_visible_text(reward_model),
                escape_html_text(risk_model),
                escape_html_text(cta),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_player_density_mode = world_map_status_label(
        world_viewport
            .get("player_density")
            .and_then(|density| density.get("mode"))
            .and_then(Value::as_str)
            .unwrap_or("dense"),
    );
    let world_map_bootstrap = trillionnium_slim_map_bootstrap_json(&world_map, "world_web_shell");
    let world_map_bootstrap_bytes = serde_json::to_string(&world_map_bootstrap)
        .map(|value| value.len())
        .unwrap_or(0);
    let world_map_data_json = serde_json::to_string(&world_map_bootstrap)
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
        .latest_buyable_listing_index_for_buyer(&league.world, current_matrix_user_id)
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
    let latest_deliverable_work_order_id = world_indexes
        .resolve_deliverable_work_order_index("latest", current_matrix_user_id)
        .and_then(|index| league.world.world_work_orders.get(index))
        .map(|work| work.work_order_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let latest_acceptable_work_order_id = world_indexes
        .resolve_acceptable_work_order_index("latest", current_matrix_user_id)
        .and_then(|index| league.world.world_work_orders.get(index))
        .map(|work| work.work_order_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let latest_rejectable_work_order_id = world_indexes
        .resolve_rejectable_work_order_index("latest", current_matrix_user_id)
        .and_then(|index| league.world.world_work_orders.get(index))
        .map(|work| work.work_order_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let latest_reopenable_work_order_id = world_indexes
        .resolve_reopenable_work_order_index("latest", current_matrix_user_id)
        .and_then(|index| league.world.world_work_orders.get(index))
        .map(|work| work.work_order_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let latest_cancellable_work_order_id = world_indexes
        .resolve_cancellable_work_order_index("latest", current_matrix_user_id)
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
    .trillionnium-game-shell {{ position:relative; grid-column:1 / -1; display:grid; grid-template-columns:minmax(340px,1.05fr) minmax(280px,.7fr) minmax(280px,.75fr); gap:14px; align-items:stretch; border:1px solid rgba(248,195,91,.32); background:radial-gradient(circle at 18% 0%,rgba(248,195,91,.16),transparent 28rem),linear-gradient(145deg,rgba(19,17,10,.94),rgba(8,10,18,.92)); box-shadow:0 26px 90px rgba(0,0,0,.48), inset 0 0 0 1px rgba(255,255,255,.045); border-radius:26px; padding:16px; margin-bottom:16px; overflow:hidden; }}
    .trillionnium-game-shell::before {{ content:""; position:absolute; inset:0; pointer-events:none; opacity:.13; background-image:linear-gradient(90deg,rgba(100,227,255,.42) 1px,transparent 1px),linear-gradient(0deg,rgba(100,227,255,.32) 1px,transparent 1px),radial-gradient(circle at 68% 34%,rgba(125,255,155,.7),transparent 0.35rem); background-size:54px 54px,54px 54px,100% 100%; mask-image:linear-gradient(180deg,rgba(0,0,0,.75),transparent 72%); }}
    .trillionnium-game-shell > * {{ position:relative; z-index:1; }}
    .trillionnium-room,.trillionnium-status,.trillionnium-engine-card {{ min-width:0; border:1px solid rgba(248,195,91,.2); background:linear-gradient(180deg,rgba(255,245,205,.08),rgba(255,255,255,.035)); border-radius:20px; padding:16px; }}
    .trillionnium-room {{ display:grid; gap:12px; font-family:"Noto Serif SC", "Songti SC", ui-serif, Georgia, serif; }}
    .trillionnium-room-title {{ display:flex; justify-content:space-between; gap:10px; align-items:center; color:var(--gold); font-weight:950; letter-spacing:.08em; }}
    .trillionnium-log {{ display:grid; gap:7px; border:1px solid rgba(125,255,155,.16); background:rgba(0,0,0,.28); border-radius:16px; padding:12px; color:#d6ffd8; font-size:13px; line-height:1.45; }}
    .trillionnium-log p {{ margin:0; }}
    .trillionnium-prompt {{ display:flex; gap:8px; align-items:center; border-top:1px dashed rgba(248,195,91,.18); padding-top:9px; color:var(--gold); font-weight:950; }}
    .trillionnium-prompt code {{ color:#071018; background:linear-gradient(135deg,#f8c35b,#7dff9b); border-radius:999px; padding:4px 8px; }}
    .trillionnium-scene-text {{ min-height:178px; border-radius:16px; padding:16px; color:#f7e7bd; background:linear-gradient(180deg,rgba(2,5,8,.86),rgba(8,10,15,.92)); border:1px solid rgba(248,195,91,.18); box-shadow:inset 0 0 34px rgba(0,0,0,.54); line-height:1.7; }}
    .trillionnium-scene-text p {{ margin:0 0 10px; }}
    .trillionnium-scene-text code {{ color:#7dff9b; background:rgba(125,255,155,.08); }}
    .trillionnium-exits {{ display:flex; flex-wrap:wrap; gap:8px; margin:0; padding:0; list-style:none; }}
    .trillionnium-exits li {{ border:1px solid rgba(100,227,255,.2); background:rgba(100,227,255,.07); color:var(--cyan); border-radius:999px; padding:6px 10px; font-size:12px; font-weight:900; }}
    .trillionnium-command-grid {{ display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px; }}
    .trillionnium-command {{ min-height:44px; display:flex; align-items:center; justify-content:center; text-align:center; text-decoration:none; border-radius:14px; border:1px solid rgba(248,195,91,.24); background:rgba(248,195,91,.095); color:#ffe2a2; font-weight:950; }}
    .trillionnium-command.primary {{ color:#071018; background:linear-gradient(135deg,#f8c35b,#7dff9b); }}
    .trillionnium-status {{ display:grid; gap:10px; }}
    .trillionnium-status h3,.trillionnium-engine-card h3 {{ margin:0; color:var(--gold); }}
    .trillionnium-stat-line {{ display:grid; grid-template-columns:78px minmax(0,1fr) auto; gap:8px; align-items:center; color:var(--muted); font-size:13px; }}
    .trillionnium-meter {{ height:9px; border-radius:999px; overflow:hidden; background:rgba(255,255,255,.08); }}
    .trillionnium-meter span {{ display:block; height:100%; border-radius:inherit; background:linear-gradient(90deg,#64e3ff,#7dff9b); }}
    .trillionnium-inventory {{ display:flex; flex-wrap:wrap; gap:7px; }}
    .trillionnium-inventory span {{ border:1px solid rgba(255,255,255,.12); background:rgba(255,255,255,.055); color:var(--text); border-radius:999px; padding:6px 9px; font-size:12px; }}
    .trillionnium-engine-card {{ display:grid; gap:10px; }}
    .trillionnium-engine-rune {{ border:1px dashed rgba(248,195,91,.24); background:rgba(248,195,91,.055); border-radius:16px; padding:12px; }}
    .trillionnium-engine-rune b {{ display:block; color:#ffe2a2; font-size:20px; margin:2px 0; letter-spacing:-.02em; }}
    .trillionnium-engine-rune code {{ color:var(--cyan); background:rgba(100,227,255,.08); border-radius:999px; padding:2px 7px; }}
    .trillionnium-engine-card small {{ color:var(--muted); line-height:1.45; }}
    .trillionnium-npc-command-stack {{ display:flex; flex-wrap:wrap; gap:6px; margin-top:8px; }}
    .trillionnium-npc-command-form {{ margin:0; }}
    .trillionnium-npc-command-form button {{ border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.08); color:var(--cyan); border-radius:999px; padding:5px 8px; font-size:11px; font-weight:900; cursor:pointer; }}
    .trillionnium-bounty-list {{ display:grid; gap:7px; margin:0; padding:0; list-style:none; }}
    .trillionnium-bounty-list li {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.05); border-radius:14px; padding:9px 10px; color:#f7e7bd; font-size:13px; }}
    .trillionnium-game-shell [data-engine-role=underlay] {{ color:var(--cyan); font-weight:900; }}
    .trillionnium-engine-drawer {{ grid-column:1 / -1; border:1px solid rgba(100,227,255,.18); background:rgba(100,227,255,.045); border-radius:18px; overflow:hidden; }}
    .trillionnium-engine-drawer > summary {{ cursor:pointer; list-style:none; display:flex; justify-content:space-between; gap:10px; align-items:center; padding:13px 15px; color:var(--cyan); font-weight:950; }}
    .trillionnium-engine-drawer > summary::-webkit-details-marker {{ display:none; }}
    .trillionnium-engine-drawer > summary::after {{ content:"+"; color:var(--gold); font-size:20px; }}
    .trillionnium-engine-drawer[open] > summary::after {{ content:"–"; }}
    .trillionnium-engine-drawer-body {{ max-height:360px; overflow:auto; padding:0 15px 15px; }}
    .trillionnium-underlay-label {{ grid-column:1 / -1; display:flex; justify-content:space-between; gap:10px; align-items:center; border:1px solid rgba(248,195,91,.18); background:rgba(248,195,91,.06); border-radius:16px; padding:10px 12px; color:var(--muted); font-size:12px; font-weight:850; }}
    .trillionnium-underlay-label strong {{ color:var(--gold); }}
    .tactics-game-shell {{ position:relative; grid-column:1 / -1; order:4; display:grid; grid-template-columns:minmax(360px,1.15fr) minmax(280px,.72fr) minmax(280px,.76fr); gap:14px; align-items:stretch; border:1px solid rgba(248,195,91,.34); background:radial-gradient(circle at 18% 0%,rgba(248,195,91,.18),transparent 26rem),radial-gradient(circle at 78% 26%,rgba(100,227,255,.14),transparent 22rem),linear-gradient(145deg,rgba(16,18,26,.96),rgba(6,8,15,.94)); box-shadow:0 26px 90px rgba(0,0,0,.52), inset 0 0 0 1px rgba(255,255,255,.045); border-radius:26px; padding:16px; margin-bottom:16px; max-height:1900px; overflow:auto; }}
    .tactics-game-shell::before {{ content:""; position:absolute; inset:0; pointer-events:none; opacity:.16; background-image:linear-gradient(90deg,rgba(248,195,91,.42) 1px,transparent 1px),linear-gradient(0deg,rgba(248,195,91,.32) 1px,transparent 1px); background-size:56px 56px; mask-image:linear-gradient(180deg,rgba(0,0,0,.75),transparent 80%); }}
    .tactics-game-shell > * {{ position:relative; z-index:1; }}
    .tactics-board-card,.tactics-command-card,.tactics-base-card {{ min-width:0; border:1px solid rgba(248,195,91,.22); background:linear-gradient(180deg,rgba(255,245,205,.08),rgba(255,255,255,.035)); border-radius:20px; padding:16px; }}
    .tactics-board-card {{ display:grid; gap:12px; }}
    .tactics-board-title {{ display:flex; justify-content:space-between; gap:10px; align-items:center; color:var(--gold); font-weight:950; letter-spacing:.06em; }}
    .tactics-board-title code,.tactics-base-card code {{ color:var(--cyan); background:rgba(100,227,255,.08); border-radius:999px; padding:3px 8px; }}
    .tactics-board {{ --tile-size:minmax(28px,1fr); position:relative; display:grid; grid-template-columns:repeat(8,var(--tile-size)); grid-template-rows:repeat(8,var(--tile-size)); gap:4px; min-height:clamp(330px,46vw,540px); border:1px solid rgba(248,195,91,.24); border-radius:18px; padding:9px; background:linear-gradient(135deg,rgba(1,4,9,.84),rgba(19,20,26,.94)); box-shadow:inset 0 0 50px rgba(0,0,0,.56); }}
    .tactics-tile {{ position:relative; min-width:0; min-height:0; border:1px solid rgba(255,255,255,.085); border-radius:10px; background:rgba(255,255,255,.055); color:inherit; padding:0; cursor:pointer; appearance:none; font:inherit; }}
    .tactics-tile small {{ position:absolute; left:5px; top:4px; color:rgba(246,247,251,.38); font-size:10px; font-weight:850; }}
    .tactics-tile:hover,.tactics-tile:focus-visible,.tactics-tile.is-selected {{ border-color:rgba(100,227,255,.72); box-shadow:0 0 0 2px rgba(100,227,255,.18),0 0 18px rgba(100,227,255,.22); outline:none; }}
    .tactics-tile[tabindex="0"] {{ outline:1px solid rgba(100,227,255,.42); outline-offset:1px; }}
    .terrain-road {{ background:linear-gradient(135deg,rgba(248,195,91,.24),rgba(255,255,255,.06)); }}
    .terrain-forest {{ background:linear-gradient(135deg,rgba(125,255,155,.22),rgba(26,77,46,.16)); }}
    .terrain-river {{ background:linear-gradient(135deg,rgba(100,227,255,.28),rgba(25,71,96,.18)); }}
    .terrain-camp {{ background:linear-gradient(135deg,rgba(167,139,250,.26),rgba(248,195,91,.1)); }}
    .terrain-market {{ background:linear-gradient(135deg,rgba(248,195,91,.26),rgba(100,227,255,.12)); }}
    .terrain-objective {{ background:linear-gradient(135deg,rgba(255,105,135,.32),rgba(248,195,91,.18)); box-shadow:0 0 0 1px rgba(248,195,91,.26),0 0 18px rgba(248,195,91,.18); }}
    .tactics-unit,.tactics-marker {{ align-self:center; justify-self:center; width:min(78%,50px); aspect-ratio:1; display:grid; place-items:center; border-radius:14px; font-size:clamp(18px,3vw,30px); font-weight:950; box-shadow:0 8px 22px rgba(0,0,0,.44),0 0 0 2px rgba(6,7,17,.76); z-index:3; }}
    .tactics-unit {{ border:0; cursor:pointer; font:inherit; }}
    .tactics-unit:focus-visible,.tactics-unit.is-selected {{ outline:2px solid #fff5b8; outline-offset:2px; box-shadow:0 8px 22px rgba(0,0,0,.44),0 0 0 3px rgba(255,245,184,.82),0 0 22px rgba(248,195,91,.45); }}
    .tactics-unit.player {{ background:linear-gradient(135deg,#f8c35b,#7dff9b); color:#071018; }}
    .tactics-unit.ally {{ background:linear-gradient(135deg,#64e3ff,#a78bfa); color:#06101a; }}
    .tactics-unit.enemy {{ background:linear-gradient(135deg,#ff6b8d,#f8c35b); color:#1c050b; }}
    .tactics-marker.objective {{ background:rgba(248,195,91,.16); border:1px dashed rgba(248,195,91,.62); color:#ffe2a2; }}
    .tactics-selection-ring {{ grid-column:2; grid-row:7; z-index:4; border:2px solid #fff5b8; border-radius:16px; box-shadow:0 0 20px rgba(248,195,91,.7), inset 0 0 18px rgba(248,195,91,.22); pointer-events:none; animation:tactics-cursor-pulse 1.2s ease-in-out infinite; }}
    @keyframes tactics-cursor-pulse {{ 0%,100% {{ opacity:.52; transform:scale(.96); }} 50% {{ opacity:1; transform:scale(1.04); }} }}
    .tactics-log {{ display:grid; gap:7px; border:1px solid rgba(125,255,155,.16); background:rgba(0,0,0,.28); border-radius:16px; padding:12px; color:#d6ffd8; font-size:13px; line-height:1.45; }}
    .tactics-log p {{ margin:0; }}
    .tactics-command-card,.tactics-base-card {{ display:grid; gap:10px; }}
    .tactics-command-card h3,.tactics-base-card h3 {{ margin:0; color:var(--gold); }}
    .tactics-command-grid {{ display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px; }}
    .tactics-command {{ min-height:44px; display:flex; align-items:center; justify-content:center; text-align:center; text-decoration:none; border-radius:14px; border:1px solid rgba(248,195,91,.24); background:rgba(248,195,91,.095); color:#ffe2a2; font-weight:950; }}
    .tactics-command.primary {{ color:#071018; background:linear-gradient(135deg,#f8c35b,#7dff9b); }}
    .tactics-command:focus-visible,.tactics-command.is-selected {{ border-color:rgba(125,255,155,.72); box-shadow:0 0 0 2px rgba(125,255,155,.16),0 0 18px rgba(125,255,155,.18); outline:none; }}
    .tactics-command-draft-panel {{ display:grid; gap:8px; border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.07); border-radius:16px; padding:12px; }}
    .tactics-command-draft-panel h4,.tactics-command-draft-panel p {{ margin:0; }}
    .tactics-command-draft-panel p,.tactics-command-draft-panel small {{ color:var(--muted); line-height:1.42; }}
    .tactics-command-draft-panel form {{ display:grid; gap:8px; }}
    .tactics-command-draft-panel button[type="submit"] {{ min-height:42px; border:0; border-radius:13px; background:linear-gradient(135deg,#64e3ff,#7dff9b); color:#061018; font-weight:950; cursor:pointer; }}
    .tactics-stat-line {{ display:grid; grid-template-columns:88px minmax(0,1fr) auto; gap:8px; align-items:center; color:var(--muted); font-size:13px; }}
    .tactics-meter {{ height:9px; border-radius:999px; overflow:hidden; background:rgba(255,255,255,.08); }}
    .tactics-meter span {{ display:block; height:100%; border-radius:inherit; background:linear-gradient(90deg,#64e3ff,#7dff9b); }}
    .tactics-chip-row {{ display:flex; flex-wrap:wrap; gap:7px; }}
    .tactics-chip-row span {{ border:1px solid rgba(255,255,255,.12); background:rgba(255,255,255,.055); color:var(--text); border-radius:999px; padding:6px 9px; font-size:12px; }}
    .tactics-base-note {{ border:1px dashed rgba(248,195,91,.25); background:rgba(248,195,91,.06); border-radius:16px; padding:12px; color:#f7e7bd; line-height:1.5; }}
    .tactics-battle-list {{ display:grid; gap:7px; margin:0; padding:0; list-style:none; }}
    .tactics-battle-list li {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.05); border-radius:14px; padding:9px 10px; color:#f7e7bd; font-size:13px; }}
    .tactics-game-shell[data-reduced-motion="true"] .tactics-selection-ring {{ animation:none; opacity:.82; transform:none; }}
    @media (prefers-reduced-motion: reduce) {{
      .tactics-game-shell *,.tactics-game-shell::before,.tactics-selection-ring {{ animation-duration:.001ms!important; animation-iteration-count:1!important; transition-duration:.001ms!important; scroll-behavior:auto!important; }}
      .tactics-selection-ring {{ animation:none; opacity:.82; transform:none; }}
    }}
    .world-mobile-action-sheet {{ display:grid; gap:9px; border:1px solid rgba(248,195,91,.24); background:linear-gradient(145deg,rgba(248,195,91,.13),rgba(100,227,255,.07)); border-radius:20px; padding:14px; }}
    .world-mobile-action-sheet p {{ margin:0; color:var(--muted); line-height:1.38; }}
    .world-mobile-action-sheet .world-route-stepper {{ display:flex; flex-wrap:wrap; gap:7px; }}
    .world-mobile-action-sheet .world-route-stepper span {{ border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.075); color:var(--cyan); border-radius:999px; padding:6px 9px; font-size:12px; font-weight:850; }}
    .world-mobile-promise {{ display:flex; flex-wrap:wrap; gap:8px; }}
    .world-mobile-promise span {{ border:1px solid rgba(100,227,255,.2); background:rgba(100,227,255,.075); color:var(--cyan); border-radius:999px; padding:8px 11px; font-size:12px; font-weight:850; }}
    .hero-card {{ display:grid; gap:14px; align-content:space-between; }}
    .hero-card strong {{ color:var(--gold); font-size:22px; }}
    .language-switcher {{ display:inline-flex; align-items:center; gap:8px; width:max-content; max-width:100%; border:1px solid rgba(100,227,255,.24); background:rgba(255,255,255,.065); color:var(--cyan); border-radius:999px; padding:6px 8px 6px 10px; font-size:12px; font-weight:900; }}
    .language-switcher select {{ width:auto; min-height:40px; min-width:92px; max-width:130px; margin:0; border:0; background:rgba(7,8,20,.72); color:var(--text); border-radius:999px; padding:7px 26px 7px 10px; font:inherit; font-size:12px; }}
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
    main > section {{ order:8; }}
    #world-map-shell-panel {{ display:contents; order:1; }}
    #world-map-shell-panel .map-shell {{ display:contents; }}
    #world-pulse-strip {{ order:2; }}
    main > .play {{ order:3; }}
    #world-map-move-panel {{ order:4; }}
    #world-commerce-panel {{ order:5; }}
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
    .play {{ display:grid; grid-template-columns:.8fr 1.2fr; gap:18px; align-items:start; }}
    .play .timeline {{ max-height:680px; overflow:auto; padding-right:4px; }}
    .map-shell {{ display:grid; grid-template-columns:1fr; gap:14px; align-items:start; }}
    #world-map-shell-panel .map-copy {{ max-height:none; overflow:hidden; }}
    #world-commerce-panel {{ max-height:840px; overflow:auto; }}
    #world-map-move-panel {{ max-height:680px; overflow:auto; }}
    #world-assets-panel,
    #world-companies-panel,
    #world-listings-panel,
    #world-contracts-panel,
    main > section.panel:not(#world-map-shell-panel):not(#world-commerce-panel):not(#world-map-move-panel) {{ max-height:620px; overflow:auto; }}
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
    .trillionnium-avatar-task-route-path {{ animation: trillionnium-route-dash 1.5s linear infinite; filter: drop-shadow(0 0 8px rgba(167,139,250,.42)); }}
    .trillionnium-avatar-task-route-pulse {{ animation: trillionnium-route-pulse 1.8s ease-in-out infinite; }}
    .trillionnium-avatar-route-runner-dot {{ width:38px; height:38px; border-radius:999px; display:grid; place-items:center; background:linear-gradient(135deg,#a78bfa,#64e3ff); box-shadow:0 0 0 3px rgba(11,18,32,.84),0 0 24px rgba(167,139,250,.55); animation: trillionnium-runner-bob 820ms ease-in-out infinite; }}
    .trillionnium-avatar-route-runner-dot span {{ transform:translateY(-1px); }}
    .trillionnium-avatar-route-runner-progress {{ filter: drop-shadow(0 0 10px rgba(100,227,255,.48)); }}
    .trillionnium-avatar-route-runner-remaining {{ animation: trillionnium-route-dash 1.7s linear infinite; }}
    .trillionnium-avatar-route-reward-checkpoint {{ animation: trillionnium-route-pulse 1.35s ease-in-out infinite; filter: drop-shadow(0 0 12px rgba(141,255,176,.48)); }}
    @keyframes trillionnium-route-dash {{ from {{ stroke-dashoffset: 0; }} to {{ stroke-dashoffset: -24; }} }}
    @keyframes trillionnium-route-pulse {{ 0%,100% {{ opacity:.55; transform:scale(1); }} 50% {{ opacity:1; transform:scale(1.08); }} }}
    @keyframes trillionnium-runner-bob {{ 0%,100% {{ transform:translateY(0) scale(1); }} 50% {{ transform:translateY(-5px) scale(1.06); }} }}
    .mini {{ display:grid; gap:7px; padding:14px; border-radius:16px; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.08); min-width:0; overflow-wrap:anywhere; word-break:break-word; }}
    .mini > * {{ min-width:0; max-width:100%; overflow-wrap:anywhere; word-break:break-word; }}
    #world-real-map {{ order:1; min-height:min(28vh,260px); border-radius:20px; overflow:hidden; border:1px solid rgba(100,227,255,.24); box-shadow:0 18px 60px rgba(0,0,0,.34); background:#0b1220; opacity:.72; }}
    #world-map-shell-panel .map-copy {{ order:3; }}
    .trillionnium-underlay-label {{ order:2; }}
    #world-real-map .leaflet-tile-pane {{ filter:saturate(.72) contrast(.88) brightness(.82); }}
    #world-real-map .trillionnium-active-route-line {{ filter:drop-shadow(0 0 8px rgba(100,227,255,.58)); }}
    #world-real-map .leaflet-control-zoom a {{ width:40px; height:40px; line-height:40px; font-size:20px; }}
    .asset strong {{ color:var(--green); }}
    .mini span,.mini small,.timeline small,.timeline em {{ color:var(--muted); }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:.55fr 1.35fr .55fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    .timeline em {{ grid-column:1 / -1; font-style:normal; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; max-width:100%; overflow-wrap:anywhere; word-break:break-word; white-space:normal; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:14px 18px; border-radius:16px; display:inline-flex; justify-content:center; align-items:center; font-weight:800; text-decoration:none; }}
    .cta.secondary {{ color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(100,227,255,.24); }}
    .secondary-link {{ color:var(--cyan); font-weight:850; text-decoration:none; border-bottom:1px solid rgba(100,227,255,.42); width:max-content; min-height:44px; display:inline-flex; align-items:center; }}
    @media (max-width:1180px) {{
      .trillionnium-game-shell {{ grid-template-columns:1fr; }}
      .trillionnium-command-grid {{ grid-template-columns:repeat(3,minmax(0,1fr)); }}
      .tactics-game-shell {{ grid-template-columns:1fr; }}
      .tactics-command-grid {{ grid-template-columns:repeat(3,minmax(0,1fr)); }}
    }}
    @media (max-width:1050px) {{
      header.world-hero {{ padding:22px min(4vw,34px) 12px; gap:14px; grid-template-columns:1fr; }}
      .world-hero-main {{ min-height:auto; gap:12px; }}
      .world-hero-title .subtitle {{ margin:0; }}
      .hero-card {{ display:none; gap:10px; }}
      .world-hero-steps {{ grid-template-columns:repeat(3,minmax(0,1fr)); }}
      main {{ padding:14px min(4vw,34px) 56px; gap:18px; }}
      .play,.map-shell {{ grid-template-columns:1fr; }}
      .grid,.mini-grid,.world-adventure-steps {{ grid-template-columns:1fr; }}
      .stats.world-pulse-strip {{ grid-template-columns:repeat(3,minmax(0,1fr)); }}
      main > section {{ order:6; }}
      #world-map-shell-panel {{ display:contents; order:1; }}
      #world-pulse-strip {{ order:2; }}
      main > .play {{ order:3; }}
      #world-commerce-panel {{ order:4; }}
      #world-map-move-panel {{ order:5; }}
      #world-map-shell-panel .map-shell {{ display:contents; }}
      .trillionnium-game-shell {{ order:4; }}
      .tactics-game-shell {{ order:4; max-height:2400px; }}
      .trillionnium-underlay-label {{ order:2; }}
      #world-real-map {{ order:1; min-height:min(28svh,240px); }}
      #world-map-shell-panel .map-copy {{ order:3; max-height:none; overflow:hidden; border:1px solid rgba(100,227,255,.18); background:rgba(100,227,255,.045); border-radius:22px; padding:0; }}
      #world-map-move-panel .mini-grid,
      #world-commerce-panel #world-purchase-cards-live,
      #world-commerce-panel #world-work-orders-live,
      #world-assets-panel .mini-grid,
      #world-companies-panel .mini-grid,
      #world-listings-panel .mini-grid,
      #world-contract-cards-live,
      #world-route-task-graph-live,
      main > section.panel > .mini-grid,
      main > section > .grid {{ max-height:420px; overflow:auto; padding-right:4px; }}
      #world-commerce-panel {{ max-height:760px; overflow:auto; }}
      #world-map-move-panel {{ max-height:620px; overflow:auto; }}
      #world-assets-panel,
      #world-companies-panel,
      #world-listings-panel,
      #world-contracts-panel,
      main > section.panel:not(#world-map-shell-panel):not(#world-commerce-panel):not(#world-map-move-panel) {{ max-height:540px; overflow:auto; }}
      .play .timeline,.timeline {{ max-height:420px; overflow:auto; padding-right:4px; }}
    }}
    @media (min-width:721px) and (max-width:1050px) {{
      .hero-card {{ grid-template-columns:auto minmax(0,1fr) auto; align-items:center; }}
      .hero-card strong {{ font-size:18px; }}
      .hero-card .cta {{ min-height:44px; white-space:nowrap; }}
    }}
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
      .language-switcher select {{ min-height:40px; min-width:82px; max-width:112px; padding:6px 22px 6px 8px; font-size:11px; }}
      .world-hero-actions {{ display:grid; grid-template-columns:1fr; gap:8px; }}
      .trillionnium-game-shell {{ padding:10px; border-radius:20px; gap:10px; }}
      .trillionnium-room,.trillionnium-status,.trillionnium-engine-card {{ padding:12px; border-radius:16px; }}
      .trillionnium-scene-text {{ min-height:150px; padding:12px; font-size:13px; line-height:1.58; }}
      .trillionnium-command-grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); }}
      .trillionnium-command {{ font-size:12px; padding:8px; }}
      .tactics-game-shell {{ padding:10px; border-radius:20px; gap:10px; }}
      .tactics-board-card,.tactics-command-card,.tactics-base-card {{ padding:12px; border-radius:16px; }}
      .tactics-board {{ min-height:min(88vw,430px); gap:3px; padding:6px; border-radius:15px; }}
      .tactics-tile {{ border-radius:7px; }}
      .tactics-tile small {{ display:none; }}
      .tactics-command-grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); }}
      .tactics-command {{ font-size:12px; padding:8px; }}
      .world-mobile-action-sheet {{ padding:11px; border-radius:16px; }}
      .world-mobile-action-sheet .world-route-stepper span {{ font-size:11px; padding:5px 7px; }}
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
      main > section {{ order:6; }}
      #world-map-shell-panel {{ display:contents; order:1; }}
      #world-pulse-strip {{ order:2; }}
      main > .play {{ order:3; }}
      #world-commerce-panel {{ order:4; }}
      #world-map-move-panel {{ order:5; }}
      #world-map-shell-panel .map-shell {{ display:contents; }}
      #world-map-shell-panel .map-copy {{ order:3; border:1px solid rgba(100,227,255,.18); background:rgba(100,227,255,.045); border-radius:20px; padding:0; }}
      .map-shell {{ gap:12px; }}
      .trillionnium-underlay-label {{ order:2; }}
      .tactics-game-shell {{ order:4; max-height:2200px; }}
      #world-real-map {{ order:1; min-height:min(28svh,220px); border-radius:18px; }}
      .map-stream-hud,.overlay-toggle-bar,.focus-stack {{ gap:6px; }}
      .hud-chip,.focus-chip,.overlay-toggle {{ padding:7px 9px; font-size:12px; }}
      #world-map-shell-panel .map-copy {{ max-height:560px; overflow:auto; }}
      #world-commerce-panel {{ max-height:680px; }}
      #world-map-move-panel {{ max-height:540px; }}
      #world-assets-panel,
      #world-companies-panel,
      #world-listings-panel,
      #world-contracts-panel,
      main > section.panel:not(#world-map-shell-panel):not(#world-commerce-panel):not(#world-map-move-panel) {{ max-height:460px; }}
      #world-map-move-panel .mini-grid,
      #world-commerce-panel #world-purchase-cards-live,
      #world-commerce-panel #world-work-orders-live,
      #world-assets-panel .mini-grid,
      #world-companies-panel .mini-grid,
      #world-listings-panel .mini-grid,
      #world-contract-cards-live,
      #world-route-task-graph-live,
      main > section.panel > .mini-grid,
      main > section > .grid {{ max-height:420px; overflow:auto; padding-right:4px; }}
      .timeline {{ max-height:420px; overflow:auto; padding-right:4px; }}
      .timeline li {{ grid-template-columns:1fr; }}
      .world-secondary-collapsed {{ max-height:104px !important; overflow:hidden; position:relative; }}
      .world-secondary-collapsed::after {{ content:'More in command palette'; position:absolute; left:12px; right:12px; bottom:8px; padding:7px 10px; border-radius:999px; background:rgba(6,7,17,.88); color:var(--muted); font-size:11px; border:1px solid rgba(255,255,255,.1); }}
    }}
  </style>
</head>
<body>
  <header id="world-mobile-first-screen" class="world-hero">
    <section class="world-hero-main">
      <div class="world-hero-kicker"><div class="pill" data-i18n-en="Open-source tactics RPG · Three Kingdoms mod shell" data-i18n-zh="开源战棋 RPG · 三国魔改界面">Open-source tactics RPG · Three Kingdoms mod shell</div><div id="world-language-switcher">{world_header_language_switcher}</div></div>
      <div class="world-hero-title">
        <h1 data-i18n-en="Trillionnium Tactics Chronicle" data-i18n-zh="Trillionnium 战棋志">Trillionnium Tactics Chronicle</h1>
        <p class="subtitle" data-i18n-en="Global-first open world rebuilt on a real open-source tactics base: tranchikhang/MedievalWar (MIT, Phaser 3, Fire Emblem-inspired) supplies the map/cursor/turn/pathfinding/menu/objective loop, while OpenClawStreetMap quietly feeds real streets, encounters, and route rewards underneath." data-i18n-zh="面向海外首发的战棋式开放世界：以 MIT 开源 tranchikhang/MedievalWar（Phaser 3、Fire Emblem 风格）作为地图/光标/回合/寻路/菜单/目标循环底座魔改；OpenClawStreetMap 在后台提供真实街巷、遭遇和路线奖励。">Global-first open world rebuilt on a real open-source tactics base: tranchikhang/MedievalWar (MIT, Phaser 3, Fire Emblem-inspired) supplies the map/cursor/turn/pathfinding/menu/objective loop, while OpenClawStreetMap quietly feeds real streets, encounters, and route rewards underneath.</p>
      </div>
      <div class="world-mobile-promise" aria-label="World mobile promises" data-i18n-aria-label-en="World mobile promises" data-i18n-aria-label-zh="世界移动端承诺">
        <span data-i18n-en="Unit turn" data-i18n-zh="单位回合">Unit turn</span>
        <span data-i18n-en="Capture objective" data-i18n-zh="占领目标">Capture objective</span>
        <span data-i18n-en="Reward / XP" data-i18n-zh="奖励 / XP">Reward / XP</span>
      </div>
      <div id="world-hero-mobile-actions" class="world-hero-actions" data-contract-version="trillionnium_mobile_single_primary_cta_v1" data-first-screen-decision-contract="trillionnium_world_map_first_screen_decision_v1" data-parity-source="app-mobile-primary-cta" data-primary-cta-count="1" data-first-screen-loop="pick_route_submit_proof_claim_reward">
        <section id="world-mobile-route-first-sheet" class="world-mobile-action-sheet" aria-label="World mobile one route first" data-i18n-aria-label-en="World mobile one route first" data-i18n-aria-label-zh="世界移动端一条路线优先">
          <strong data-i18n-en="Current route" data-i18n-zh="当前路线">Current route</strong>
          <p id="world-mobile-current-route" data-i18n-en="{map_route_runner_handoff_summary}" data-i18n-zh="{map_route_runner_handoff_summary}">{map_route_runner_handoff_summary}</p>
          <div class="world-route-stepper" aria-label="Pick route submit proof claim reward" data-i18n-aria-label-en="Pick route submit proof claim reward" data-i18n-aria-label-zh="选路线、交证据、领奖励">
            <span data-i18n-en="Pick route" data-i18n-zh="选路线">Pick route</span>
            <span data-i18n-en="Submit proof" data-i18n-zh="交证据">Submit proof</span>
            <span data-i18n-en="Claim reward" data-i18n-zh="领奖励">Claim reward</span>
          </div>
          <p id="world-mobile-next-action" data-i18n-en="Next action: move the selected unit toward the real-street objective, then submit proof or claim reward." data-i18n-zh="下一步动作：让选中单位推进到真实街巷目标，再提交证据或领取奖励。">Next action: move the selected unit toward the real-street objective, then submit proof or claim reward.</p>
          <p id="world-mobile-reward-xp" data-route-mastery-contract="{map_route_runner_mastery_contract}" data-route-mastery-tier="{map_route_runner_mastery_tier}" data-route-mastery-xp="{map_route_runner_mastery_xp}">Reward / XP · {map_route_runner_reward_claim_count} claim · {map_route_runner_mastery_xp} XP · {map_route_runner_mastery_tier}</p>
          <a id="world-mobile-primary-cta" class="cta" href='#trillionnium-tactics-game-shell' data-i18n-en="Continue route: enter tactics board" data-i18n-zh="继续路线：进入战棋棋盘">Continue route: enter tactics board</a>
        </section>
      </div>
    </section>
    <aside class="hero-card">
      <strong data-i18n-en="Next Adventure" data-i18n-zh="下一步冒险">Next Adventure</strong>
      <ol class="world-hero-steps">
        <li><b data-i18n-en="1 · Unit" data-i18n-zh="1 · 单位">1 · Unit</b><span data-i18n-en="Select a hero, adviser, or agent squad." data-i18n-zh="选中主公、军师或 Agent 小队。">Select a hero, adviser, or agent squad.</span></li>
        <li><b data-i18n-en="2 · Move" data-i18n-zh="2 · 行军">2 · Move</b><span data-i18n-en="Advance across real-street terrain." data-i18n-zh="沿真实街巷地形推进。">Advance across real-street terrain.</span></li>
        <li><b data-i18n-en="3 · Reward" data-i18n-zh="3 · 战利品">3 · Reward</b><span data-i18n-en="Capture the objective, submit proof, and claim XP." data-i18n-zh="占领目标、提交证据并领取 XP。">Capture the objective, submit proof, and claim XP.</span></li>
      </ol>
      <a id="world-league-link" class="secondary-link" href="/league" data-i18n-en="League arena stays one tap away" data-i18n-zh="League 竞技场保留一跳入口">League arena stays one tap away</a>
    </aside>
  </header>
  <main>
    <section id="world-pulse-strip" class="stats world-pulse-strip" aria-label="World pulse counters" data-i18n-aria-label-en="World pulse counters" data-i18n-aria-label-zh="世界脉冲统计">
      <article class="pulse-card is-primary"><span data-i18n-en="Live Events" data-i18n-zh="实时事件">Live Events</span><b>{events}</b><small data-i18n-en="Tap one to turn the map into a route." data-i18n-zh="点一个事件，把地图变成路线。">Tap one to turn the map into a route.</small></article>
      <article class="pulse-card"><span data-i18n-en="Map Points" data-i18n-zh="地图点">Map Points</span><b>{map_nodes}</b><small data-i18n-en="Real city anchors" data-i18n-zh="现实城市锚点">Real city anchors</small></article>
      <article class="pulse-card"><span data-i18n-en="Quest Cards" data-i18n-zh="任务牌">Quest Cards</span><b>{listings}</b><small data-i18n-en="Available bounties" data-i18n-zh="可接取悬赏">Available bounties</small></article>
      <article class="pulse-card"><span data-i18n-en="Commissions" data-i18n-zh="委托">Commissions</span><b>{work_orders}</b><small data-i18n-en="Accepted loops" data-i18n-zh="已进入执行循环">Accepted loops</small></article>
      <article class="pulse-card"><span data-i18n-en="Agents" data-i18n-zh="居民">Agents</span><b>{entities}</b><small data-i18n-en="World residents" data-i18n-zh="世界居民">World residents</small></article>
      <details id="world-stats-compact-more" class="world-stats-more" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_counter_drawer" data-main-experience="false" data-default-state="collapsed">
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
    <section id="world-map-shell-panel" class="panel" data-bootstrap-mode="truncated_runtime_bootstrap_with_lazy_delta_hydration" data-bootstrap-payload-bytes="{world_map_bootstrap_bytes}" data-cache-contract="trillionnium_world_map_payload_cache_v1">
      <div class="map-shell">
        <section id="trillionnium-tactics-game-shell" class="tactics-game-shell" data-contract-version="trillionnium_open_source_tactics_world_shell_v1" data-tactics-board-contract="{tactics_board_contract}" data-tactics-unit-contract="{tactics_unit_contract}" data-tactics-command-contract="{tactics_command_contract}" data-trillionnium-character-contract="{trillionnium_character_contract}" data-trillionnium-skill-contract="{trillionnium_skill_contract}" data-trillionnium-npc-command-descriptor-contract="{trillionnium_npc_command_descriptor_contract}" data-mentor-training-task-contract="{trillionnium_mentor_training_task_contract}" data-trillionnium-task-archetype-contract="{trillionnium_task_archetype_contract}" data-trillionnium-task-completion-contract="{trillionnium_task_completion_contract}" data-trillionnium-reward-gate-contract="{trillionnium_reward_gate_contract}" data-trillionnium-battle-log-style-contract="{trillionnium_battle_log_style_contract}" data-trillionnium-combat-log-contract="{trillionnium_combat_log_contract}" data-trillionnium-npc-relationship-contract="{trillionnium_npc_relationship_contract}" data-trillionnium-osm-objective-contract="{trillionnium_osm_objective_contract}" data-tactics-combat-resolution-contract="{tactics_combat_resolution_contract}" data-tactics-game-session-contract="{tactics_game_session_contract}" data-tactics-simulation-tick-contract="{tactics_simulation_tick_contract}" data-tactics-reward-settlement-contract="{tactics_reward_settlement_contract}" data-tactics-accessibility-contract="{tactics_accessibility_contract}" data-keyboard-traversal="roving_grid_focus" data-low-motion-support="prefers_reduced_motion" data-map-overlay-identity-contract="{map_overlay_identity_contract}" data-interface-style="turn_based_strategy_rpg" data-open-source-base="tranchikhang/MedievalWar" data-base-license="MIT" data-base-url="https://github.com/tranchikhang/MedievalWar" data-base-engine="Phaser 3" data-base-patterns="map,cursor,control,turn_system,pathfinding,context_menu,objectives,ai" data-asset-policy="no_proprietary_assets_css_tokens_first" data-map-engine-role="openclawstreetmap_underlay" data-underlay-engine="{map_engine_id}" data-underlay-provider="{tile_provider}" data-source-of-truth="rust_trillionnium_game_state" aria-label="Open-source tactics Trillionnium game shell" data-i18n-aria-label-en="Open-source tactics Trillionnium game shell" data-i18n-aria-label-zh="开源战棋 Trillionnium 游戏界面">
          <article class="tactics-board-card">
            <div class="tactics-board-title"><span data-i18n-en="Three Kingdoms Tactics · Mirror Street Battle" data-i18n-zh="【三国战棋】镜像街巷战役">Three Kingdoms Tactics · Mirror Street Battle</span><code data-engine-role="underlay" data-underlay-name="OpenClawStreetMap" data-i18n-en="real street engine" data-i18n-zh="真实街巷引擎">real street engine</code></div>
            <div class="tactics-board" role="grid" aria-label="Trillionnium turn based tactics board" data-i18n-aria-label-en="Trillionnium turn based tactics board" data-i18n-aria-label-zh="Trillionnium 回合制战棋棋盘" aria-describedby="world-tactics-keyboard-help" data-accessibility-contract="{tactics_accessibility_contract}" data-keyboard-traversal="roving_grid_focus">
              {tactics_board_cells}
              <span class="tactics-selection-ring" aria-hidden="true"></span>
              {tactics_board_units}
              {tactics_objective_markers}
            </div>
            <div class="tactics-log" aria-label="Tactics battle log" data-i18n-aria-label-en="Tactics battle log" data-i18n-aria-label-zh="战棋战报">
              {tactics_battle_log}
              <p data-log-kind="status" data-source-of-truth="rust_world_projection" data-i18n-en="> status: {events} live events, {listings} bounty cards, {map_avatar_route_runner_count} moving squads." data-i18n-zh="> 状态：{events} 条实时事件、{listings} 张悬赏牌、{map_avatar_route_runner_count} 支移动小队。">&gt; status: {events} live events, {listings} bounty cards, {map_avatar_route_runner_count} moving squads.</p>
            </div>
          </article>
          {tactics_player_hud}
          <aside class="tactics-command-card" aria-label="Tactics command menu" data-i18n-aria-label-en="Tactics command menu" data-i18n-aria-label-zh="战棋指令菜单">
            <h3 data-i18n-en="Tactics command menu" data-i18n-zh="战棋指令菜单">Tactics command menu</h3>
            <div class="tactics-base-note" data-source-of-truth="rust_trillionnium_character"><strong data-i18n-en="{trillionnium_display_name_en}" data-i18n-zh="{trillionnium_display_name_zh}">{trillionnium_display_name_en}</strong><span data-i18n-en="{trillionnium_title_en}" data-i18n-zh="{trillionnium_title_zh}">{trillionnium_title_en}</span></div>
            {trillionnium_status_lines}
            {tactics_session_state}
            <div class="tactics-stat-line"><span data-i18n-en="Orders" data-i18n-zh="军令">Orders</span><div class="tactics-meter"><span style="width:86%"></span></div><b>{map_avatar_route_runner_count}</b></div>
            <div class="tactics-stat-line"><span data-i18n-en="Reputation" data-i18n-zh="声望">Reputation</span><div class="tactics-meter"><span style="width:72%"></span></div><b>{map_route_runner_mastery_xp}</b></div>
            <div class="tactics-stat-line"><span data-i18n-en="Bounties" data-i18n-zh="悬赏">Bounties</span><div class="tactics-meter"><span style="width:64%"></span></div><b>{listings}</b></div>
            <div class="tactics-stat-line"><span data-i18n-en="Rewards" data-i18n-zh="领奖">Rewards</span><div class="tactics-meter"><span style="width:58%"></span></div><b>{map_route_runner_reward_claim_count}</b></div>
            <nav class="tactics-command-grid" aria-label="Tactical actions" data-i18n-aria-label-en="Tactical actions" data-i18n-aria-label-zh="战术动作">
              {tactics_command_grid}
            </nav>
            {tactics_command_draft_panel}
            <section id="trillionnium-training" class="trillionnium-training-panel" data-training-contract="trillionnium_training_command_v1" data-command-endpoint="/world/web/tactics-command" data-api-command-endpoint="/v1/world/tactics/command" data-source-of-truth="rust_mentor_training_validator" aria-label="Trillionnium mentor training" data-i18n-aria-label-en="Trillionnium mentor training" data-i18n-aria-label-zh="Trillionnium导师修炼">
              <h4 data-i18n-en="Mentor training" data-i18n-zh="导师修炼">Mentor training</h4>
              <p data-i18n-en="Web sends intent only; Rust checks mentor, OSM place, cost, cooldown, and skill mutation." data-i18n-zh="网页只提交意图；Rust 校验导师、OSM 地点、消耗、冷却和技能变更。">Web sends intent only; Rust checks mentor, OSM place, cost, cooldown, and skill mutation.</p>
              {trillionnium_training_forms}
            </section>
            <div class="tactics-chip-row" aria-label="Tactics rules" data-i18n-aria-label-en="Tactics rules" data-i18n-aria-label-zh="战棋规则">
              <span data-i18n-en="deterministic combat" data-i18n-zh="确定性战斗">deterministic combat</span>
              <span data-i18n-en="RPS unit counters" data-i18n-zh="兵种相克">RPS unit counters</span>
              <span data-i18n-en="real-street terrain" data-i18n-zh="真实街巷地形">real-street terrain</span>
              <span data-i18n-en="proof-gated rewards" data-i18n-zh="证据领奖">proof-gated rewards</span>
            </div>
          </aside>
          <aside class="tactics-base-card" aria-label="Open source base and OpenClawStreetMap support" data-i18n-aria-label-en="Open source base and OpenClawStreetMap support" data-i18n-aria-label-zh="开源底座与 OpenClawStreetMap 支撑">
            <h3 data-i18n-en="Open-source base · tactics mod" data-i18n-zh="开源底座 · 三国魔改">Open-source base · tactics mod</h3>
            <div class="tactics-base-note">
              <strong data-i18n-en="Base: tranchikhang/MedievalWar" data-i18n-zh="底座：tranchikhang/MedievalWar">Base: tranchikhang/MedievalWar</strong>
              <p data-i18n-en="MIT licensed Phaser 3 tactics game: map loading, cursor control, context menu, unit turn system, movement/pathfinding, enemy AI, and map objectives are the modding base; proprietary Three Kingdoms titles are only style references." data-i18n-zh="MIT 许可的 Phaser 3 战棋游戏：地图加载、光标控制、上下文菜单、单位回合、移动/寻路、敌方 AI 和地图目标作为魔改底座；三国群英传/三国策/三国志/三国霸业等商业作品只作风格参考，不复制素材或代码。">MIT licensed Phaser 3 tactics game: map loading, cursor control, context menu, unit turn system, movement/pathfinding, enemy AI, and map objectives are the modding base; proprietary Three Kingdoms titles are only style references.</p>
              <code>https://github.com/tranchikhang/MedievalWar</code>
            </div>
            <ul class="tactics-battle-list" aria-label="Battle objectives" data-i18n-aria-label-en="Battle objectives" data-i18n-aria-label-zh="战役目标">
              <li data-i18n-en="Win: capture the real-street objective and bring back an evidence package." data-i18n-zh="胜利：占领真实街区目标点，带回证据包。">Win: capture the real-street objective and bring back an evidence package.</li>
              <li data-i18n-en="Resources: bounty cards become orders, and reward pools become loot." data-i18n-zh="资源：悬赏牌变成军令，奖励池变成战利品。">Resources: bounty cards become orders, and reward pools become loot.</li>
              <li data-i18n-en="Underlay: OpenClawStreetMap only feeds terrain and encounters; it does not own the main UI." data-i18n-zh="底图：OpenClawStreetMap 只做地形和遭遇输入，不抢主界面。">Underlay: OpenClawStreetMap only feeds terrain and encounters; it does not own the main UI.</li>
            </ul>
            <section id="trillionnium-npcs" class="trillionnium-social-grid" data-sect-contract="trillionnium_sect_v1" data-sect-osm-binding-contract="trillionnium_sect_osm_binding_v1" data-npc-contract="trillionnium_npc_v1" data-npc-spawn-contract="trillionnium_npc_spawn_anchor_v1" data-npc-command-descriptor-contract="trillionnium_npc_command_descriptor_v1" data-source-of-truth="rust_trillionnium_npc_model" aria-label="Trillionnium sects and NPCs" data-i18n-aria-label-en="Trillionnium sects and NPCs" data-i18n-aria-label-zh="Trillionnium门派与 NPC">
              <h4 data-i18n-en="Sects / mentors / NPCs" data-i18n-zh="门派 / 导师 / NPC">Sects / mentors / NPCs</h4>
              <div class="mini-grid">{trillionnium_sect_cards}</div>
              <div class="mini-grid">{trillionnium_npc_cards}</div>
            </section>
            <section id="trillionnium-task-candidates" class="trillionnium-task-candidate-grid" data-completion-contract="{trillionnium_task_completion_contract}" data-reward-gate-contract="{trillionnium_reward_gate_contract}" data-ledger-reward-requires-settlement="true" data-review-hold-gate-enforced="true" data-anti-cheese-gate-enforced="true" data-source-of-truth="rust_trillionnium_task_completion_handler" data-web-role="intent_only_visualization_input" aria-label="Trillionnium task completion candidates" data-i18n-aria-label-en="Trillionnium task completion candidates" data-i18n-aria-label-zh="Trillionnium任务提交候选">
              <h4 data-i18n-en="Task reports / reward gate" data-i18n-zh="任务战报 / 奖励门禁">Task reports / reward gate</h4>
              <p data-i18n-en="Submit task reports from OSM-generated candidates; Rust validates completion, review hold, anti-cheese, and ledger settlement before rewards release." data-i18n-zh="从 OSM 生成的候选任务提交战报；Rust 校验完成、复核暂挂、反刷和账本结算后才释放奖励。">Submit task reports from OSM-generated candidates; Rust validates completion, review hold, anti-cheese, and ledger settlement before rewards release.</p>
              <div class="mini-grid">{trillionnium_task_completion_forms}</div>
            </section>
            <small data-i18n-en="Next mod path: replace placeholder units with Trillionnium agents, convert POIs into capture points, and use route evidence as battle reports." data-i18n-zh="下一步魔改：把占位单位替换为 Trillionnium Agent，把 POI 转成占领点，把路线证据转成战报。">Next mod path: replace placeholder units with Trillionnium agents, convert POIs into capture points, and use route evidence as battle reports.</small>
          </aside>
        </section>
        <details id="world-map-underlay-details" class="map-copy trillionnium-engine-drawer" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="supporting_engine_diagnostics" data-main-experience="false" data-default-state="collapsed" data-openclawstreetmap-role="supporting_engine_diagnostics">
          <summary data-i18n-en="OpenClawStreetMap underlay / engine details" data-i18n-zh="OpenClawStreetMap 底层引擎 / 技术细节">OpenClawStreetMap underlay / engine details</summary>
          <div class="trillionnium-engine-drawer-body">
          <div class="pill" data-i18n-en="{map_product_name_html}" data-i18n-zh="Trillionnium 世界地图">{map_product_name_html}</div>
          <h2 data-i18n-en="OpenStreetMap upgraded into a playable world" data-i18n-zh="把 OpenStreetMap 升级成可玩的世界地图">OpenStreetMap upgraded into a playable world</h2>
          <p class="subtitle" data-i18n-en="{map_upgrade_model_html}" data-i18n-zh="Trillionnium World Map 不是普通地图工具，而是在 OpenStreetMap 真实地理底座上叠加游戏人物、路线节点、任务牌、实时事件和交付闭环。角色会在地图上跑来跑去，接任务、提交证据、拿评级和奖励。">{map_upgrade_model_html}</p>
          <p id="world-map-readability-lod" class="subtitle" data-contract-version="trillionnium_world_map_readability_lod_v1" data-semantic-layer-contract="trillionnium_world_map_game_layer_semantics_v1" data-first-screen-mode="route_first_street_detail" data-primary-cta-budget="1" data-visible-marker-budget="18" data-avatar-runner-budget="6" data-copy-summary-budget="150" data-details-default-state="collapsed" data-semantic-legend-required="true" data-avatar-feedback-required="true" data-i18n-a11y-required="true" data-parity-source="app-map-readability-lod" data-i18n-en="One route first: one CTA, muted OSM context, high-contrast route, start/objective/reward/locked pins." data-i18n-zh="先看一条路线：一个主行动、弱化底图、高对比路线、起点/目标/奖励/锁定图钉。">One route first: one CTA, muted OSM context, high-contrast route, start/objective/reward/locked pins.</p>
          <p id="world-map-performance-budget" class="subtitle" data-contract-version="trillionnium_world_map_runtime_performance_budget_v1" data-first-map-interactive-target-ms="2000" data-viewport-refresh-p95-target-ms="250" data-focus-to-action-rail-target-ms="300" data-main-thread-long-task-budget-ms="100" data-low-end-mobile-fps-floor="45" data-delta-viewport-updates-required="true" data-abort-previous-viewport-request="true" data-defer-noncritical-card-render="true" data-cluster-markers-before-hiding="true" data-spatial-cache-required="true" data-virtualized-cards-required="true" data-adaptive-density-required="true" data-parity-source="app-map-performance-budget" data-i18n-en="Performance budget: interactive under 2s, focus-to-action under 300ms, abort stale viewport fetches, defer dense cards, and cluster before extra density." data-i18n-zh="性能预算：2 秒内可交互、焦点到行动栏 300ms 内、取消过期视口请求、延后密集卡片、先聚合再增加密度。">Performance budget: interactive under 2s, focus-to-action under 300ms, abort stale viewport fetches, defer dense cards, and cluster before extra density.</p>
          <p id="world-map-rum-slo" class="subtitle" data-contract-version="trillionnium_world_map_rum_slo_v1" data-quantiles="p50,p95,p99" data-surface-split="app,world" data-device-split="mobile,desktop" data-sample-kinds="cold_cache_interactive,warm_delta_or_304,weak_network_cached_snapshot" data-per-bucket-min-samples="1" data-matrix-contract="trillionnium_world_map_real_user_rum_matrix_v1" data-parity-source="app-map-rum-slo" data-i18n-en="RUM SLO gate: p50/p95/p99 by /app vs /world and mobile vs desktop before adding map density." data-i18n-zh="真实用户性能门禁：按 /app 与 /world、移动端与桌面端拆 p50/p95/p99，再增加地图密度。">RUM SLO gate: p50/p95/p99 by /app vs /world and mobile vs desktop before adding map density.</p>
          <p id="world-map-transport-delta" class="subtitle" data-contract-version="trillionnium_world_map_transport_delta_v1" data-subsystem-contract="trillionnium_world_map_subsystem_v1" data-presence-delta-required="true" data-snapshot-fallback-required="true" data-changed-group-rendering-required="true" data-visible-marker-delta-required="true" data-marker-cluster-delta-required="true" data-parity-source="app-map-transport-delta" data-i18n-en="Transport boundary: viewport snapshots stay compatible, while route runners, presence, markers, and clusters move by changed-group deltas." data-i18n-zh="传输边界：视口快照保持兼容，路线角色、在线状态、标记和聚合按变更组增量更新。">Transport boundary: viewport snapshots stay compatible, while route runners, presence, markers, and clusters move by changed-group deltas.</p>
          <p id="world-map-weak-network" class="subtitle" data-contract-version="trillionnium_world_map_weak_network_resilience_v1" data-cache-key="trillionnium-world-map:last-good-viewport:v1" data-delta-304-supported="true" data-offline-banner-required="true" data-pending-action-queue-required="true" data-conflict-sync-required="true" data-parity-source="app-map-weak-network" data-i18n-en="Weak network mode: delta first, 304 reuse, snapshot fallback, then last-good cached viewport without blanking the map." data-i18n-zh="弱网模式：先增量、304 复用、快照兜底，最后用上次可用视口，不让地图空白。">Weak network mode: delta first, 304 reuse, snapshot fallback, then last-good cached viewport without blanking the map.</p>
          <p id="world-map-location-privacy" class="subtitle" data-contract-version="trillionnium_world_map_location_privacy_v1" data-rum-excludes-lat-lng="true" data-cache-control="private" data-parity-source="app-map-location-privacy" data-i18n-en="Location privacy: RUM sends surface/device/cursor only; personalized viewport and delta responses stay private-cache." data-i18n-zh="位置隐私：RUM 只发送界面/设备/游标；个性化视口和增量响应保持 private cache。">Location privacy: RUM sends surface/device/cursor only; personalized viewport and delta responses stay private-cache.</p>
          <p id="world-map-shadow-renderer" class="subtitle" data-contract-version="trillionnium_world_map_renderer_shadow_v1" data-parity-contract="trillionnium_world_map_maplibre_shadow_parity_v1" data-active-engine="leaflet_openstreetmap_v1" data-shadow-engine="maplibre_gl_v1" data-status="shadow_only_not_user_facing" data-canary-percent="0" data-rollback-drill-required="true" data-i18n-en="MapLibre stays shadow-only until product loop retention and renderer pressure justify promotion." data-i18n-zh="MapLibre 保持影子渲染；只有留存和渲染压力证明必要时才晋升。">MapLibre stays shadow-only until product loop retention and renderer pressure justify promotion.</p>
          <div class="world-map-player-summary">
            <div>
              <strong data-i18n-en="Current Status" data-i18n-zh="当前状态">Current Status</strong>
              <p id="world-map-density-summary" class="subtitle">{map_density_summary}</p>
              <p id="world-route-runner-handoff-summary" class="subtitle" data-next-route-status="{map_route_runner_next_route_status}" data-runner-count="{map_avatar_route_runner_count}" data-reward-claim-count="{map_route_runner_reward_claim_count}" data-next-route-count="{map_route_runner_next_route_count}" data-route-mastery-contract="{map_route_runner_mastery_contract}" data-route-mastery-tier="{map_route_runner_mastery_tier}" data-route-mastery-xp="{map_route_runner_mastery_xp}">{map_route_runner_handoff_summary}</p>
              <p id="world-map-camera-summary" class="subtitle" data-i18n-en="Camera loading…" data-i18n-zh="镜头加载中…">Camera loading…</p>
            </div>
            <a class="cta" href='#world-action-console' data-i18n-en="Start Next Action" data-i18n-zh="发起下一步行动">Start Next Action</a>
          </div>
          <ol class="world-map-loop-steps" aria-label="World player loop" data-i18n-aria-label-en="World player loop" data-i18n-aria-label-zh="世界玩家三步循环">
            <li><b data-i18n-en="1 · Pick route" data-i18n-zh="1 · 选路线">1 · Pick route</b><span data-i18n-en="One route first, not every dashboard concept." data-i18n-zh="先看一条路线，不先看所有仪表盘概念。">One route first, not every dashboard concept.</span></li>
            <li><b data-i18n-en="2 · Submit proof" data-i18n-zh="2 · 交证据">2 · Submit proof</b><span data-i18n-en="Tie proof to the active objective." data-i18n-zh="把证据绑定到当前目标。">Tie proof to the active objective.</span></li>
            <li><b data-i18n-en="3 · Claim reward" data-i18n-zh="3 · 领奖励">3 · Claim reward</b><span data-i18n-en="Claim XP, then open the next route." data-i18n-zh="领取 XP，然后开启下一条路线。">Claim XP, then open the next route.</span></li>
          </ol>
          <section id="world-route-archetype-catalog" class="mini-grid" data-contract-version="trillionnium_world_route_archetypes_v1" aria-label="World route archetypes" data-i18n-aria-label-en="World route archetypes" data-i18n-aria-label-zh="世界路线类型">
            {world_route_archetype_cards}
          </section>
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
            <section id="world-openstreetmap-geodata" class="mini-grid" data-contract-version="{osm_geodata_contract}" data-fixture-layers-contract="{osm_fixture_layers_contract}" data-map-overlay-identity-contract="{map_overlay_identity_contract}" data-semantic-role-example="mentor_training_anchor" data-provider-contract="{osm_geodata_provider_contract}" data-provider-id="{osm_geodata_provider_id}" data-source-mode="{osm_geodata_source_mode}" data-source-of-truth="rust_openstreetmap_data_provider" data-web-role="visualization_input_only" data-feature-count="{osm_geodata_feature_count}" data-legal-obligation="odbl_database_obligations" aria-label="OpenStreetMap geodata substrate" data-i18n-aria-label-en="OpenStreetMap geodata substrate" data-i18n-aria-label-zh="OpenStreetMap 地理数据底座">
              <article class="mini osm-contract"><strong>OpenStreetMapDataProvider</strong><span>openstreetmap_geodata_v1 · openstreetmap_fixture_layers_v1 · Rust source of truth · fixture first before Overpass/Geofabrik</span><code>osm_id · osm_type · lat/lng · tags · game_overlay_id · mentor_training_anchor</code><small>Do not use public OSM tile servers for production traffic; cache/self-host/vendor first.</small></article>
              <article id="world-openstreetmap-provider-readiness" class="mini osm-provider-readiness" data-contract-version="{osm_provider_readiness_contract}" data-readiness-status="{osm_provider_readiness_status}" data-fixture-mode-green="{osm_fixture_mode_green}" data-live-modes-fail-closed="{osm_live_modes_fail_closed}" data-live-network-ingestion-enabled="{osm_live_network_ingestion_enabled}" data-production-ingestion-enabled="{osm_production_ingestion_enabled}" data-fail-closed-mode-count="{osm_fail_closed_mode_count}" data-expected-fail-closed-mode-count="{osm_expected_fail_closed_mode_count}" data-stable-fixture-identity-coverage-complete="{osm_stable_fixture_identity_coverage_complete}" data-fixture-layer-feature-count="{osm_fixture_layer_feature_count}" data-source-of-truth="rust_openstreetmap_data_provider" data-web-role="visualization_input_only"><strong>OSM provider readiness</strong><span>fixture_ready_live_fail_closed · fixture green · Overpass/Geofabrik/vendor live modes fail closed</span><code>openstreetmap_provider_readiness_v1 · overpass_bbox_cache · geofabrik_extract_import · vendor_tile_cache</code><small>Live network ingestion stays disabled until cache/rate-limit, ODbL derived-database tracking, and fresh production signoff exist.</small></article>
              {osm_geodata_feature_cards}
            </section>
            <div id="world-tile-shards-live" class="mini-grid">{tile_shard_cards}</div>
            <div id="world-region-shards-live" class="mini-grid" style="margin-top:12px">{region_shard_cards}</div>
            <div class="mini-grid" style="margin-top:12px">{lod_layer_cards}</div>
            <div id="world-poi-hotspots-live" class="mini-grid" style="margin-top:12px">{hotspot_cards}</div>
            <div id="world-prefetch-queue-live" class="mini-grid" style="margin-top:12px">{prefetch_cards}</div>
            <div id="world-live-events-live" class="mini-grid" style="margin-top:12px">{live_event_cards}</div>
            <h3 data-i18n-en="Avatar Task Routes" data-i18n-zh="角色任务路线">Avatar Task Routes</h3>
            <div id="world-avatar-task-routes-live" class="mini-grid" style="margin-top:12px"></div>
            <h3 data-i18n-en="Avatar Movement" data-i18n-zh="角色跑图">Avatar Movement</h3>
            <div id="world-avatar-route-runners-live" class="mini-grid" style="margin-top:12px"></div>
            <p style="margin-top:12px"><strong>Global Real-world Map Engine</strong>: <code>{map_engine_name}</code> + <code>{tile_provider}</code></p>
            <p><strong>Mirror</strong>: <code>{mirror_scope}</code> · <strong>Strategy</strong>: <code>{full_mirror_strategy}</code> · <strong>Style</strong>: <code>{simplification_style}</code> · <strong>Goal</strong>: <code>{scaling_goal}</code></p>
            <p><strong>Viewport API</strong>: <code>{viewport_path}</code></p>
            <p><strong>Web Viewport</strong>: <code>{web_session_viewport_path}</code></p>
            <div id="world-map-stream-hud" class="map-stream-hud">
              <span class="hud-chip"><strong>{map_stream_region_count}</strong> <span data-i18n-en="regional shards" data-i18n-zh="个区域分片">regional shards</span></span>
              <span class="hud-chip"><strong>{map_visible_marker_count}</strong> <span data-i18n-en="visible places" data-i18n-zh="个可见地点">visible places</span></span>
              <span class="hud-chip"><strong>{map_prefetch_count}</strong> <span data-i18n-en="prefetch tiles" data-i18n-zh="个预热地图块">prefetch tiles</span></span>
              <span class="hud-chip"><strong>{map_live_event_count}</strong> <span data-i18n-en="live events" data-i18n-zh="个实时事件">live events</span> · {map_player_density_mode}</span>
              <span class="hud-chip"><strong>{map_avatar_task_route_count}</strong> <span data-i18n-en="task routes" data-i18n-zh="条任务路线">task routes</span></span>
              <span class="hud-chip"><strong>{map_avatar_route_runner_count}</strong> <span data-i18n-en="moving avatars" data-i18n-zh="个动态角色">moving avatars</span></span>
              <span class="hud-chip"><strong>{map_player_avatar_count}</strong> <span data-i18n-en="running avatars" data-i18n-zh="个跑图角色">running avatars</span></span>
            </div>
            <div id="world-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
            </div>
            <p id="world-map-overlay-status" class="subtitle" data-i18n-en="Active layers: density, regions, tiles, prefetch rings, live events, task routes, moving avatars, player avatars." data-i18n-zh="当前图层：密度、区域、地图块、预热圈、实时事件、任务路线、动态角色、跑图角色。">Active layers: density, regions, tiles, prefetch rings, live events, task routes, moving avatars, player avatars.</p>
            <p id="world-map-overlay-legend" class="subtitle" data-i18n-en="Layer legend: regional anchors, active tiles, prefetch rings, live-event pulses, avatar task routes, animated runners, and running avatars." data-i18n-zh="图层说明：区域锚点、活跃地图块、预热探索圈、实时事件脉冲、角色任务路线、动态跑图和跑图角色。">Layer legend: regional anchors, active tiles, prefetch rings, live-event pulses, avatar task routes, animated runners, and running avatars.</p>
          </details>
          </div>
        </details>
        <div class="trillionnium-underlay-label" data-i18n-en="OpenClawStreetMap engine viewport · supporting layer, not the main UI" data-i18n-zh="OpenClawStreetMap 引擎视口 · 支撑层，不是主界面"><strong>OpenClawStreetMap</strong><span data-i18n-en="supporting real-world engine viewport" data-i18n-zh="底层真实世界引擎视口">supporting real-world engine viewport</span></div>
        <div id="world-real-map" data-engine="{map_engine_id}" data-provider="{tile_provider}" aria-label="Trillionnium World Map" data-i18n-aria-label-en="Trillionnium World Map" data-i18n-aria-label-zh="Trillionnium 世界地图"></div>
      </div>
    </section>
    <section class="world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="World Regions" data-i18n-zh="世界区域">World Regions</h2>
      <div class="grid">{zone_cards}</div>
    </section>
    <section id="world-map-move-panel" class="panel world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="available_after_core_loop" data-primary-loop-anchor="trillionnium-tactics-game-shell">
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
        {recovery_notice_html}
        <form method="post" action="/world/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
          <select id="world-action-location" name="location_id">{location_options}</select>
          <textarea id="world-action-body" name="body" data-i18n-value-en="Launch an AI Design Studio for global customers: define the customer deliverable, evidence package, risk controls, next action, self-review, and League quest handoff." data-i18n-value-zh="我要在全球镜像城市建立 AI 设计工坊：写清客户交付方案、证据包、风险控制、下一步行动、自检复盘，并把机会转成 League 任务。">Launch an AI Design Studio for global customers: define the customer deliverable, evidence package, risk controls, next action, self-review, and League quest handoff.</textarea>
          <button type="submit" data-i18n-en="Submit World Action" data-i18n-zh="提交世界行动">Submit World Action</button>
        </form>
      </div>
      <div class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
        <h2 data-i18n-en="World Event Timeline" data-i18n-zh="世界事件时间线">World Event Timeline</h2>
        <ul id="world-event-timeline" class="timeline">{event_items}</ul>
      </div>
    </section>
    <section class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Places" data-i18n-zh="地点">Places</h2>
      <div class="mini-grid">{location_cards}</div>
    </section>
    <section class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Agent Residents · NPC" data-i18n-zh="Agent 居民 · NPC">Agent Residents · NPC</h2>
      <div class="mini-grid">{entity_cards}</div>
    </section>
    <section id="world-assets-panel" class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Character Items" data-i18n-zh="角色道具">Character Items</h2>
      <div class="mini-grid">{asset_cards}</div>
      <form method="post" action="/world/web/asset" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-asset-body" name="body" data-i18n-value-en="Upgrade this world item for a customer deliverable: strengthen capability, evidence package, risk controls, next action loop, self-review, and side-quest handoff." data-i18n-value-zh="升级这件世界道具用于客户交付方案：强化能力、证据包、风险控制、下一步行动循环、自检复盘和支线交接。">Upgrade this world item for a customer deliverable: strengthen capability, evidence package, risk controls, next action loop, self-review, and side-quest handoff.</textarea>
        <button type="submit" data-i18n-en="Upgrade Item" data-i18n-zh="升级道具">Upgrade Item</button>
      </form>
    </section>
    <section id="world-companies-panel" class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Studios and Hubs" data-i18n-zh="工坊与据点">Studios and Hubs</h2>
      <div class="mini-grid">{company_cards}</div>
      <form method="post" action="/world/web/company" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-company-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="自动填充或 latest" />
        <textarea id="world-company-body" name="body" data-i18n-value-en="Launch a global-facing studio hub with this item: define customer deliverables, evidence package, risk controls, next action loop, self-review, and the first bounty route." data-i18n-value-zh="用这件道具建立面向海外玩家的工坊据点：定义客户交付方案、证据包、风险控制、下一步行动循环、自检复盘和第一条悬赏路线。">Launch a global-facing studio hub with this item: define customer deliverables, evidence package, risk controls, next action loop, self-review, and the first bounty route.</textarea>
        <button type="submit" data-i18n-en="Launch Studio" data-i18n-zh="建立工坊">Launch Studio</button>
      </form>
    </section>
    <section id="world-listings-panel" class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
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
          <textarea id="world-buy-body" name="body" data-i18n-value-en="Accept this quest card and open an adventure commission: confirm customer deliverables, evidence package, rating standards, risk controls, next action, and self-review." data-i18n-value-zh="接取这个任务牌，开启冒险委托：确认客户交付方案、证据包、评级标准、风险控制、下一步行动和自检复盘。">Accept this quest card and open an adventure commission: confirm customer deliverables, evidence package, rating standards, risk controls, next action, and self-review.</textarea>
          <button type="submit" data-i18n-en="Accept Quest Card" data-i18n-zh="接取任务牌">Accept Quest Card</button>
        </form>
      <div id="world-work-deliveries-live" class="mini-grid" style="margin-top:12px">{work_delivery_cards}</div>
      <form id="world-work-deliver-form" method="post" action="/world/web/work-deliver" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-deliver-id" name="work_order_id" value="{latest_deliverable_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-deliver-body" name="body" data-i18n-value-en="Result package: deliverable, evidence package, rating checklist, risk review, next action, and self-check notes." data-i18n-value-zh="成果提交包：成果、证据包、评级清单、风险复盘、下一步行动和自检记录。">Result package: deliverable, evidence package, rating checklist, risk review, next action, and self-check notes.</textarea>
        <button type="submit" data-i18n-en="Submit Result" data-i18n-zh="提交成果">Submit Result</button>
      </form>
      <div id="world-work-acceptances-live" class="mini-grid" style="margin-top:12px">{work_acceptance_cards}</div>
      <form id="world-work-accept-form" method="post" action="/world/web/work-accept" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-accept-id" name="work_order_id" value="{latest_acceptable_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-accept-body" name="body" data-i18n-value-en="Rating passed: confirm customer deliverable, evidence package, quality note, risk controls, next side quest, reputation reward, and self-review." data-i18n-value-zh="评级通过：确认客户交付方案、证据包、质量备注、风险控制、下一条支线、声望奖励和自检复盘。">Rating passed: confirm customer deliverable, evidence package, quality note, risk controls, next side quest, reputation reward, and self-review.</textarea>
        <button type="submit" data-i18n-en="Pass Rating" data-i18n-zh="评级通过">Pass Rating</button>
      </form>
      <div id="world-work-rejections-live" class="mini-grid" style="margin-top:12px">{work_rejection_cards}</div>
      <form id="world-work-reject-form" method="post" action="/world/web/work-reject" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reject-id" name="work_order_id" value="{latest_rejectable_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reject-body" name="body" data-i18n-value-en="Revision required: record customer deliverable gap, evidence package, refund risk controls, revision requirement, next action, and self-review." data-i18n-value-zh="需要返工：记录客户交付缺口、证据包、退款风险控制、返工要求、下一步行动和自检复盘。">Revision required: record customer deliverable gap, evidence package, refund risk controls, revision requirement, next action, and self-review.</textarea>
        <button type="submit" data-i18n-en="Request Revision" data-i18n-zh="要求返工">Request Revision</button>
      </form>
      <div id="world-work-reopens-live" class="mini-grid" style="margin-top:12px">{work_reopen_cards}</div>
      <form id="world-work-reopen-form" method="post" action="/world/web/work-reopen" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-reopen-id" name="work_order_id" value="{latest_reopenable_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-reopen-body" name="body" data-i18n-value-en="Reopen commission: escrow reward again, list customer deliverable revisions, evidence gaps, risk controls, rating standards, next resubmission action, and self-review." data-i18n-value-zh="重开委托：重新托管奖励，列出客户交付返工、证据缺口、风险控制、评级标准、下一步再次提交行动和自检复盘。">Reopen commission: escrow reward again, list customer deliverable revisions, evidence gaps, risk controls, rating standards, next resubmission action, and self-review.</textarea>
        <button type="submit" data-i18n-en="Reopen Commission" data-i18n-zh="重开委托">Reopen Commission</button>
      </form>
      <div id="world-work-cancellations-live" class="mini-grid" style="margin-top:12px">{work_cancellation_cards}</div>
      <form id="world-work-cancel-form" method="post" action="/world/web/work-cancel" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-work-cancel-id" name="work_order_id" value="{latest_cancellable_work_order_id}" placeholder="自动填充或 latest" />
        <textarea id="world-work-cancel-body" name="body" data-i18n-value-en="Cancel commission: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the route." data-i18n-value-zh="放弃委托：记录客户交付状态、证据包、退款风险控制、下一步行动和自检复盘，再关闭路线。">Cancel commission: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the route.</textarea>
        <button type="submit" data-i18n-en="Cancel Commission" data-i18n-zh="放弃委托">Cancel Commission</button>
      </form>
      </details>
    </section>
    <section class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Faction Reputation Map" data-i18n-zh="阵营声望图">Faction Reputation Map</h2>
      <div class="mini-grid">{faction_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{standing_cards}</div>
    </section>
    <section id="world-contracts-panel" class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="World Contracts" data-i18n-zh="世界契约">World Contracts</h2>
      <div id="world-contract-cards-live" class="mini-grid">{contract_cards}</div>
      <form id="world-contract-completion-form" method="post" action="/world/web/contract" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="{current_matrix_user_id}" />
        <input id="world-contract-completion-id" name="contract_id" value="{latest_contract_id}" placeholder="自动填充或契约 ID" />
        <textarea id="world-contract-completion-body" name="body" data-i18n-value-en="World contract report: customer deliverable, evidence package, risk review, next step, rating standards, and self-review." data-i18n-value-zh="世界契约战报：客户交付方案、证据包、风险复盘、下一步、评级标准和自检复盘。">World contract report: customer deliverable, evidence package, risk review, next step, rating standards, and self-review.</textarea>
        <button type="submit" data-i18n-en="Complete Contract" data-i18n-zh="完成契约">Complete Contract</button>
      </form>
    </section>
    <section class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
      <h2 data-i18n-en="Quest Route Graph" data-i18n-zh="任务路线图">Quest Route Graph</h2>
      <div id="world-route-task-graph-live" class="mini-grid">{world_route_task_graph_cards}</div>
    </section>
    <section class="panel world-secondary-collapsed world-secondary-detail-panel" data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1" data-secondary-dashboard-role="secondary_detail_panel" data-main-experience="false" data-default-state="collapsed_on_mobile" data-primary-loop-anchor="trillionnium-tactics-game-shell" data-mobile-ia="collapsed_secondary_panel">
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
      const routeRunnerHandoffSummary = document.getElementById('world-route-runner-handoff-summary');
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
      const taskRouteTarget = document.getElementById('world-avatar-task-routes-live');
      const routeRunnerTarget = document.getElementById('world-avatar-route-runners-live');
      const routeTaskGraphTarget = document.getElementById('world-route-task-graph-live');
      {shared_map_runtime_bootstrap_js}

      const routeTaskGraphItems = ((((payload.route_task_graph || {{}}).tasks) || []));
      let lastViewport = null;
      let lastViewportCursor = null;
      let mapRumFirstInteractiveSent = false;
      let lastSelection = null;
      let routeFilterMode = 'all';
      const initializeTacticsIntentDraft = () => {{
        const shell = document.getElementById('trillionnium-tactics-game-shell');
        const panel = document.getElementById('world-tactics-command-draft-panel');
        const form = document.getElementById('world-tactics-command-draft-form');
        if (!shell || !panel || !form) return;
        const preview = document.getElementById('world-tactics-command-draft-preview');
        const status = document.getElementById('world-tactics-command-draft-status');
        const board = shell.querySelector('.tactics-board[role="grid"]');
        const prefersReducedMotion = Boolean(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
        shell.dataset.reducedMotion = prefersReducedMotion ? 'true' : 'false';
        panel.dataset.reducedMotion = prefersReducedMotion ? 'true' : 'false';
        const state = {{
          command: panel.dataset.selectedCommand || 'attack',
          unitId: panel.dataset.selectedUnitId || 'lord',
          targetTile: panel.dataset.selectedTargetTile || 'F5',
        }};
        const focusElement = (element) => {{
          if (!element) return;
          try {{ element.focus({{ preventScroll: prefersReducedMotion }}); }} catch (_) {{ element.focus(); }}
        }};
        const boardTiles = () => Array.from(shell.querySelectorAll('.tactics-tile[data-tile]'));
        const tileByCoord = (row, col) => boardTiles().find((tile) => Number(tile.dataset.gridRow || 0) === row && Number(tile.dataset.gridCol || 0) === col);
        const tileById = (tileId) => boardTiles().find((tile) => String(tile.dataset.tile || '') === String(tileId || ''));
        const field = (name) => form.querySelector('[name="' + name + '"]');
        const setField = (name, value) => {{
          const input = field(name);
          if (input) input.value = String(value || '');
        }};
        const markSelected = () => {{
          shell.querySelectorAll('.tactics-tile[data-tile]').forEach((tile) => {{
            const selected = String(tile.dataset.tile || '') === state.targetTile;
            tile.classList.toggle('is-selected', selected);
            tile.setAttribute('aria-selected', String(selected));
            tile.tabIndex = selected ? 0 : -1;
          }});
          shell.querySelectorAll('.tactics-unit[data-unit]').forEach((unit) => {{
            const selectedPlayerUnit = String(unit.dataset.side || '') === 'player' && String(unit.dataset.unit || '') === state.unitId;
            const selectedTargetUnit = String(unit.dataset.side || '') === 'enemy' && String(unit.dataset.tile || '') === state.targetTile;
            unit.classList.toggle('is-selected', selectedPlayerUnit || selectedTargetUnit);
            unit.setAttribute('aria-selected', String(selectedPlayerUnit || selectedTargetUnit));
          }});
          shell.querySelectorAll('.tactics-command[data-command]').forEach((command) => {{
            command.classList.toggle('is-selected', String(command.dataset.command || '') === state.command);
            command.setAttribute('aria-selected', String(String(command.dataset.command || '') === state.command));
          }});
        }};
        const writeDraft = () => {{
          panel.dataset.selectedCommand = state.command;
          panel.dataset.selectedUnitId = state.unitId;
          panel.dataset.selectedTargetTile = state.targetTile;
          setField('command', state.command);
          setField('unit_id', state.unitId);
          setField('target_tile', state.targetTile);
          const body = 'tactics intent draft: command=' + state.command + ' unit=' + state.unitId + ' target=' + state.targetTile + '; browser selected only, Rust validates legality and outcome.';
          setField('body', body);
          if (preview) preview.textContent = 'draft: ' + state.command + ' · ' + state.unitId + ' → ' + state.targetTile;
          if (status) status.textContent = 'Draft ready: ' + state.command + ' with ' + state.unitId + ' targeting ' + state.targetTile + '. Rust remains source of truth.';
          markSelected();
        }};
        const selectTargetTile = (tile, shouldFocus = false) => {{
          if (!tile) return;
          state.targetTile = tile.dataset.tile || state.targetTile;
          writeDraft();
          if (shouldFocus) focusElement(tile);
        }};
        const focusAdjacentTile = (origin, deltaRow, deltaCol) => {{
          const row = Number(origin.dataset.gridRow || origin.getAttribute('aria-rowindex') || 1);
          const col = Number(origin.dataset.gridCol || origin.getAttribute('aria-colindex') || 1);
          const nextRow = Math.min(8, Math.max(1, row + deltaRow));
          const nextCol = Math.min(8, Math.max(1, col + deltaCol));
          selectTargetTile(tileByCoord(nextRow, nextCol) || origin, true);
        }};
        if (board) {{
          board.addEventListener('keydown', (event) => {{
            const activeTile = event.target.closest('.tactics-tile[data-tile]');
            if (!activeTile || !board.contains(activeTile)) return;
            const key = event.key;
            if (key === 'ArrowUp') {{ event.preventDefault(); focusAdjacentTile(activeTile, -1, 0); }}
            else if (key === 'ArrowDown') {{ event.preventDefault(); focusAdjacentTile(activeTile, 1, 0); }}
            else if (key === 'ArrowLeft') {{ event.preventDefault(); focusAdjacentTile(activeTile, 0, -1); }}
            else if (key === 'ArrowRight') {{ event.preventDefault(); focusAdjacentTile(activeTile, 0, 1); }}
            else if (key === 'Home') {{ event.preventDefault(); selectTargetTile(tileByCoord(Number(activeTile.dataset.gridRow || 1), 1) || activeTile, true); }}
            else if (key === 'End') {{ event.preventDefault(); selectTargetTile(tileByCoord(Number(activeTile.dataset.gridRow || 1), 8) || activeTile, true); }}
            else if (key === 'Enter' || key === ' ') {{ event.preventDefault(); activeTile.click(); }}
          }});
        }}
        shell.addEventListener('keydown', (event) => {{
          const command = event.target.closest('.tactics-command[data-command]');
          if (!command || !shell.contains(command)) return;
          if (event.key === ' ' || event.key === 'Enter') {{
            event.preventDefault();
            command.click();
          }}
        }});
        shell.addEventListener('click', (event) => {{
          const unit = event.target.closest('.tactics-unit[data-unit]');
          if (unit && shell.contains(unit)) {{
            event.preventDefault();
            if (String(unit.dataset.side || '') === 'enemy') {{
              state.targetTile = unit.dataset.tile || state.targetTile;
            }} else {{
              state.unitId = unit.dataset.unit || state.unitId;
            }}
            writeDraft();
            return;
          }}
          const tile = event.target.closest('.tactics-tile[data-tile]');
          if (tile && shell.contains(tile)) {{
            event.preventDefault();
            selectTargetTile(tile, false);
            return;
          }}
          const command = event.target.closest('.tactics-command[data-command]');
          if (command && shell.contains(command)) {{
            event.preventDefault();
            state.command = command.dataset.command || state.command;
            writeDraft();
          }}
        }});
        writeDraft();
        const initialTile = tileById(state.targetTile);
        if (initialTile) initialTile.tabIndex = 0;
        window.trillionniumTacticsIntentDraft = {{
          contract_version: panel.dataset.contractVersion,
          board_cell_interaction_contract_version: panel.dataset.boardCellInteractionContract,
          unit_selection_contract_version: panel.dataset.unitSelectionContract,
          accessibility_contract_version: panel.dataset.accessibilityContract,
          keyboard_traversal: panel.dataset.keyboardTraversal,
          low_motion_support: panel.dataset.lowMotionSupport,
          source_of_truth: panel.dataset.sourceOfTruth,
          web_role: panel.dataset.webRole,
          getState: () => ({{ ...state }}),
        }};
      }};
      initializeTacticsIntentDraft();
      {shared_map_runtime_primitives_js}
      const worldHandoffKey = () => routeHandoffStorageKey();

      {shared_map_route_target_resolution_js}
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
          cameraSummary.textContent = routePhrase('Route focus ready: ' + (focusedEventId ? ('event ' + focusedEventId) : focusedNodeId) + ' · panel ' + (target.panelId || routeActionPanelId()), '路线焦点已准备：' + (focusedEventId ? ('事件 ' + focusedEventId) : focusedNodeId) + ' · 面板 ' + (target.panelId || routeActionPanelId()));
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
      const buildRouteActionDraft = (selection, routeContext) => buildRouteDraftBody(selection, routeContext, {{ selectionTitleFallback: routePhrase('current world route', '当前世界路线') }});
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
        statusPrefix: routePhrase('Recommended next step', '推荐下一步'),
        rejectionBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': reopen commission ' + workOrderId + ' with revision needs, evidence gaps, renewed escrow, and next submission.', selectionTitle + ': 重开委托 ' + workOrderId + '，写清返工要求、证据缺口、再次托管和下一次提交。'),
        rejectionStatus: (workOrderId) => routePhrase('Reopen commission ' + workOrderId + ' for the current route.', '为当前路线重开委托 ' + workOrderId + '。'),
        reopenBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': resubmit commission ' + workOrderId + ' with revised result, evidence pack, rating checklist, and risk review.', selectionTitle + ': 重新提交委托 ' + workOrderId + '，带上修订成果、证据包、评级清单和风险复盘。'),
        reopenStatus: (workOrderId) => routePhrase('Resubmit commission ' + workOrderId + ' after reopening.', '重开后重新提交委托 ' + workOrderId + '。'),
        deliveryBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': rate commission ' + workOrderId + ', confirm evidence and quality, then choose pass or revision next step.', selectionTitle + ': 评定委托 ' + workOrderId + ' 的成果，确认证据和质量，再给出通过或返工的下一步。'),
        deliveryStatus: (workOrderId) => routePhrase('Rate latest commission result ' + workOrderId + '.', '评定最新委托成果 ' + workOrderId + '。'),
        openWorkBody: (selectionTitle, workOrderId) => routePhrase(selectionTitle + ': prepare result, evidence, rating checklist, and next action for commission ' + workOrderId + '.', selectionTitle + ': 为委托 ' + workOrderId + ' 准备成果、证据、评级清单和下一步行动。'),
        openWorkStatus: (workOrderId) => routePhrase('Submit current commission ' + workOrderId + '.', '提交当前委托 ' + workOrderId + '。'),
        closedWorkLabel: routePhrase('Draft follow-up action', '起草后续行动'),
        closedWorkBody: (selectionTitle, workOrderId, latestWorkBucket) => routePhrase(selectionTitle + ': follow up commission ' + workOrderId + ' after ' + mapText(latestWorkBucket) + ', recording result, next branch, and world-state changes.', selectionTitle + ': 在 ' + latestWorkBucket + ' 后跟进委托 ' + workOrderId + '，记录结果、下一条支线和世界状态变化。'),
        closedWorkStatus: (workOrderId, latestWorkBucket) => routePhrase('Draft follow-up world action for commission ' + workOrderId + ' after ' + mapText(latestWorkBucket) + '.', '在 ' + latestWorkBucket + ' 后起草委托 ' + workOrderId + ' 的后续世界行动。'),
        contractBody: (selectionTitle, contractId) => routePhrase(selectionTitle + ': complete contract ' + contractId + ' with result, evidence, risk review, rating criteria, and next step.', selectionTitle + ': 完成契约 ' + contractId + '，带上成果、证据、风险复盘、评级标准和下一步。'),
        contractStatus: (contractId) => routePhrase('Complete contract ' + contractId + ' for the current route.', '完成当前路线的契约 ' + contractId + '。'),
        listingBody: (selectionTitle, listingId) => routePhrase(selectionTitle + ': accept bounty card ' + listingId + ' with deliverable, evidence, rating, risk controls, and next action.', selectionTitle + ': 接取任务牌 ' + listingId + '，定义成果、证据、评级、风险控制和下一步行动。'),
        listingStatus: (listingId) => routePhrase('Connect bounty card ' + listingId + ' into the adventure route.', '把任务牌 ' + listingId + ' 接入冒险路线。'),
        defaultBody: (selectionTitle) => routePhrase(selectionTitle + ': draft the next world action from the current route with evidence, risk, and execution move.', selectionTitle + ': 从当前路线起草下一步世界行动，包含证据、风险和推进动作。'),
        defaultStatus: () => routePhrase('Draft a world action for the current route.', '为当前路线起草世界行动。'),
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
          routeFilterStatus.textContent = routePhrase('Route filter: showing all world activity.', '路线筛选：显示全部世界活动。');
        }} else {{
          const workflowCount = (counts.purchase || 0) + (counts.work_order || 0) + (counts.delivery || 0) + (counts.acceptance || 0) + (counts.rejection || 0) + (counts.reopen || 0) + (counts.cancellation || 0);
          routeFilterStatus.textContent = routePhrase('Route filter: ' + mapText(selection.title || 'focus') + (selectedTaskId ? (' · task ' + selectedTaskId) : '') + ' · ' + (counts.event || 0) + ' events · ' + (counts.contract || 0) + ' contracts · ' + workflowCount + ' workflow steps.', '路线筛选：' + (selection.title || '焦点') + (selectedTaskId ? (' · 任务 ' + selectedTaskId) : '') + ' · ' + (counts.event || 0) + ' 事件 · ' + (counts.contract || 0) + ' 契约 · ' + workflowCount + ' 个冒险环节。');
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
        const workOrderId = String(((latestWorkItem && latestWorkItem.dataset.workOrderId) || (workItem && workItem.dataset.workOrderId) || '')).trim();
        const activeTaskId = selectedTaskId || String((latestTaskItem && latestTaskItem.dataset.taskId) || '').trim();
        const linkedContractItem = findLatestVisibleByBucketAndTask(visibleItems, 'contract', activeTaskId);
        const linkedEventItem = findLatestVisibleByBucketAndTask(visibleItems, 'event', activeTaskId);
        const linkedEventCount = activeTaskId ? visibleItems.filter((item) => (item.dataset.routeBucket || '') === 'event' && String(item.dataset.taskId || '').trim() === activeTaskId).length : 0;
        const linkedContractCount = activeTaskId ? visibleItems.filter((item) => (item.dataset.routeBucket || '') === 'contract' && String(item.dataset.taskId || '').trim() === activeTaskId).length : 0;
        const latestWorkBucket = String((latestWorkItem && latestWorkItem.dataset.routeBucket) || '').trim();
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
          latestWorkBucket,
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
          if (opportunityAction) pushRouteFlowActionButton(actions, actionKeys, opportunityAction);
          if (nextStep) pushRouteFlowActionButton(actions, actionKeys, nextStep);
          if (locationId || draftBody) {{
            pushRouteFlowActionButton(actions, actionKeys, buildDraftWorldAction(locationId, activeTaskId, draftBody));
          }}
          if (activeTaskId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildTaskFollowUpAction(selection, activeTaskId, locationId, routePhrase('Combine linked event/contract, current world-state evidence, risk, and next action', '结合关联事件/契约、当前世界状态证据、风险和下一步行动')));
          }}
          if (eventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction(routePhrase('Open event lane', '打开事件线'), {{
              eventId: String(eventItem.dataset.eventId || ''),
              eventKind: String(eventItem.dataset.eventKind || 'world_event'),
              eventBody: String(eventItem.dataset.eventBody || ''),
              eventResult: String(eventItem.dataset.eventResult || ''),
              eventTaskId: String(eventItem.dataset.taskId || ''),
              locationId: String(eventItem.dataset.locationId || ''),
            }}));
          }}
          if (activeTaskId && linkedEventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction(routePhrase('Open linked event', '打开关联事件'), {{
              eventId: String(linkedEventItem.dataset.eventId || ''),
              eventKind: String(linkedEventItem.dataset.eventKind || 'world_event'),
              eventBody: String(linkedEventItem.dataset.eventBody || ''),
              eventResult: String(linkedEventItem.dataset.eventResult || ''),
              eventTaskId: String(linkedEventItem.dataset.taskId || ''),
              locationId: String(linkedEventItem.dataset.locationId || ''),
            }}));
          }}
          if (workOrderId && (!latestWorkBucket || ['purchase', 'work_order', 'reopen'].includes(latestWorkBucket))) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('delivery', workOrderId, {{ label: routePhrase('Advance commission', '推进委托'), locationId }}));
          }}
          if (contractId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldContractLaneAction(contractId, {{ locationId }}));
          }}
          if (activeTaskId && linkedContractItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildLinkedContractRouteAction(selection, String(linkedContractItem.dataset.contractId || ''), activeTaskId, locationId, {{ bodySuffix: routePhrase(', with evidence, rating criteria, and next step.', '，带上证据、评级标准和下一步。') }}));
          }}
          if (listingId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldPurchaseLaneAction(listingId, {{ locationId }}));
          }}
          if (((nextStep || {{}}).label) === routePhrase('Open rating route', '打开评级路线') && workOrderId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('rejection', workOrderId, {{
              locationId,
              body: appendSelectionEventSignal(routePhrase((selection && selection.title ? selection.title : 'current route') + ': mark commission ' + workOrderId + ' as needing revision, with evidence gap, reward return, and revision route.', (selection && selection.title ? selection.title : '当前路线') + ': 标记委托 ' + workOrderId + ' 需要返工，写清证据缺口、奖励退回和修订路线。'), selection),
            }}));
          }}
          routeFlowActions.innerHTML = actions.join(' ');
        }}
        if (!routeFlowStatus) return;
        if (routeFilterMode === 'all' || !selection) {{
          routeFlowStatus.textContent = routePhrase('Adventure route: waiting for map focus.', '冒险路线：等待地图焦点。');
          if (routeNextStepStatus) routeNextStepStatus.textContent = routePhrase('Recommended next step: choose a map focus first.', '推荐下一步：先选择地图焦点。');
          if (routeEventBriefStatus) routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, true);
          if (routeLinkStatus) routeLinkStatus.textContent = routeLinkStatusText({{ emptyText: '关联任务路线：暂无。' }});
          if (actionConsoleStatus) actionConsoleStatus.textContent = defaultActionConsoleStatus || routePhrase('Use the World Action Console to submit a new world action.', '使用世界行动台提交新的世界行动。');
          return;
        }}
        const contractLabel = routePhrase(contractId || 'no contract yet', contractId || '暂无契约');
        const workLabel = routePhrase(workOrderId || 'no commission yet', workOrderId || '暂无委托');
        routeFlowStatus.textContent = routePhrase('Adventure route: ' + mapText(selection.title || 'focus') + ' → event ' + mapText(eventLabel) + ' · commission ' + workLabel + ' · contract ' + contractLabel + routeOpportunitySegment(opportunityTask) + '.', '冒险路线：' + (selection.title || '焦点') + ' → 事件 ' + eventLabel + ' · 委托 ' + workLabel + ' · 契约 ' + contractLabel + routeOpportunitySegment(opportunityTask) + '。');
        if (routeNextStepStatus) {{
          routeNextStepStatus.textContent = ((opportunityAction || {{}}).status) || ((nextStep || {{}}).status) || routePhrase('Recommended next step: draft a world action for the current route.', '推荐下一步：为当前路线起草世界行动。');
        }}
        if (routeEventBriefStatus) {{
          routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, false);
        }}
        if (routeLinkStatus) {{
          routeLinkStatus.textContent = routeLinkStatusText({{ taskId: activeTaskId, linkedEventCount, linkedContractCount, opportunityTask, inFocus: true, emptyText: '关联任务路线：当前焦点没有事件/契约链接。' }});
        }}
        if (actionConsoleStatus) {{
          const effectiveNextAction = opportunityAction || nextStep || {{}};
          actionConsoleStatus.textContent = routePhrase('Current world action: ' + mapText(selection.title || 'focus') + ' · ' + (locationId || 'unknown place') + ' · event ' + mapText(eventLabel) + ' · commission ' + workLabel + ' · contract ' + contractLabel + routeOpportunitySegment(opportunityTask) + ' · next step ' + mapText(effectiveNextAction.label || 'Draft world action') + '.', '当前世界行动：' + (selection.title || '焦点') + ' · ' + (locationId || '未知地点') + ' · 事件 ' + eventLabel + ' · 委托 ' + workLabel + ' · 契约 ' + contractLabel + routeOpportunitySegment(opportunityTask) + ' · 下一步 ' + (effectiveNextAction.label || '起草世界行动') + '。');
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
          renderCards(taskRouteTarget, filterAvatarTaskRoutes(lastViewport.avatar_task_routes || [], focus), 'taskRoute');
          renderCards(routeRunnerTarget, filterAvatarRouteRunners(lastViewport.avatar_route_runners || [], focus), 'routeRunner');
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
      const mapRumSurfaceId = 'world';
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
        osm_geodata_contract = escape_html_text(osm_geodata_contract),
        osm_geodata_provider_contract = escape_html_text(osm_geodata_provider_contract),
        osm_geodata_provider_id = escape_html_text(osm_geodata_provider_id),
        osm_geodata_source_mode = escape_html_text(osm_geodata_source_mode),
        osm_geodata_feature_count = osm_geodata_feature_count,
        osm_fixture_layers_contract = escape_html_text(osm_fixture_layers_contract),
        osm_geodata_feature_cards = osm_geodata_feature_cards,
        tactics_board_contract = escape_html_text(tactics_board_contract),
        tactics_unit_contract = escape_html_text(tactics_unit_contract),
        tactics_command_contract = escape_html_text(tactics_command_contract),
        trillionnium_character_contract = escape_html_text(trillionnium_character_contract),
        trillionnium_skill_contract = escape_html_text(trillionnium_skill_contract),
        trillionnium_npc_command_descriptor_contract =
            escape_html_text(trillionnium_npc_command_descriptor_contract),
        trillionnium_mentor_training_task_contract =
            escape_html_text(trillionnium_mentor_training_task_contract),
        trillionnium_task_archetype_contract =
            escape_html_text(trillionnium_task_archetype_contract),
        trillionnium_task_completion_contract =
            escape_html_text(trillionnium_task_completion_contract),
        trillionnium_reward_gate_contract = escape_html_text(trillionnium_reward_gate_contract),
        trillionnium_battle_log_style_contract =
            escape_html_text(trillionnium_battle_log_style_contract),
        trillionnium_combat_log_contract = escape_html_text(trillionnium_combat_log_contract),
        trillionnium_npc_relationship_contract =
            escape_html_text(trillionnium_npc_relationship_contract),
        trillionnium_osm_objective_contract = escape_html_text(trillionnium_osm_objective_contract),
        tactics_combat_resolution_contract = escape_html_text(tactics_combat_resolution_contract),
        tactics_game_session_contract = escape_html_text(tactics_game_session_contract),
        tactics_simulation_tick_contract = escape_html_text(tactics_simulation_tick_contract),
        tactics_reward_settlement_contract = escape_html_text(tactics_reward_settlement_contract),
        map_overlay_identity_contract = escape_html_text(map_overlay_identity_contract),
        tactics_board_cells = tactics_board_cells,
        tactics_board_units = tactics_board_units,
        tactics_objective_markers = tactics_objective_markers,
        tactics_battle_log = tactics_battle_log,
        tactics_session_state = tactics_session_state,
        tactics_player_hud = tactics_player_hud,
        tactics_command_grid = tactics_command_grid,
        tactics_command_draft_panel = tactics_command_draft_panel,
        trillionnium_training_forms = trillionnium_training_forms,
        trillionnium_sect_cards = trillionnium_sect_cards,
        trillionnium_npc_cards = trillionnium_npc_cards,
        trillionnium_task_completion_forms = trillionnium_task_completion_forms,
        trillionnium_status_lines = trillionnium_status_lines,
        trillionnium_display_name_en = escape_html_text(world_trillionnium_visible_english(
            trillionnium_display_name
        )),
        trillionnium_display_name_zh = escape_html_text(trillionnium_display_name),
        trillionnium_title_en =
            escape_html_text(world_trillionnium_visible_english(trillionnium_title)),
        trillionnium_title_zh = escape_html_text(trillionnium_title),
        tile_shard_cards = tile_shard_cards,
        region_shard_cards = region_shard_cards,
        lod_layer_cards = lod_layer_cards,
        hotspot_cards = hotspot_cards,
        prefetch_cards = prefetch_cards,
        live_event_cards = live_event_cards,
        viewport_path = escape_html_text(viewport_path),
        web_session_viewport_path = escape_html_text(web_session_viewport_path),
        map_density_summary = escape_html_text(&map_density_summary),
        map_route_runner_handoff_summary = escape_html_text(map_route_runner_handoff_summary),
        map_route_runner_next_route_status = escape_html_text(map_route_runner_next_route_status),
        map_route_runner_reward_claim_count = map_route_runner_reward_claim_count,
        map_route_runner_next_route_count = map_route_runner_next_route_count,
        map_route_runner_mastery_contract = escape_html_text(map_route_runner_mastery_contract),
        map_route_runner_mastery_tier = escape_html_text(map_route_runner_mastery_tier),
        map_route_runner_mastery_xp = map_route_runner_mastery_xp,
        world_route_archetype_cards = world_route_archetype_cards,
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
        latest_deliverable_work_order_id = escape_html_text(&latest_deliverable_work_order_id),
        latest_acceptable_work_order_id = escape_html_text(&latest_acceptable_work_order_id),
        latest_rejectable_work_order_id = escape_html_text(&latest_rejectable_work_order_id),
        latest_reopenable_work_order_id = escape_html_text(&latest_reopenable_work_order_id),
        latest_cancellable_work_order_id = escape_html_text(&latest_cancellable_work_order_id),
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
        world_map_bootstrap_bytes = world_map_bootstrap_bytes,
    ))
}

pub(super) async fn get_world_web_shell_response(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let html = get_world_web_shell(State(state), headers, Query(query))
        .await
        .0;
    html_resource_response(html, "trillionnium_world_map_world_shell_payload_v1")
}
