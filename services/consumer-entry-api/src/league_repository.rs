use super::*;

pub(super) fn default_league_state() -> LeagueState {
    let matches = [
        LeagueMatch {
            match_id: "daily-dungeon-001".to_string(),
            title: "Prompt Forge 入门战".to_string(),
            mode: "daily_dungeon".to_string(),
            status: "open".to_string(),
            objective: "用最少成本生成一个可提交成果，并给出自评/风险。".to_string(),
            reward: "XP + credits".to_string(),
            recommended_roles: vec![
                "strategist".to_string(),
                "builder".to_string(),
                "auditor".to_string(),
            ],
        },
        LeagueMatch {
            match_id: "bounty-arena-001".to_string(),
            title: "赏金赛：真实委托预备场".to_string(),
            mode: "bounty_arena".to_string(),
            status: "preview".to_string(),
            objective: "多人提交方案，按质量、速度、成本和委托适配评分。".to_string(),
            reward: "Prize Pool".to_string(),
            recommended_roles: vec![
                "scout".to_string(),
                "builder".to_string(),
                "closer".to_string(),
            ],
        },
        LeagueMatch {
            match_id: "guild-raid-001".to_string(),
            title: "公会团本：多阶段成果战".to_string(),
            mode: "guild_raid".to_string(),
            status: "preview".to_string(),
            objective: "团队分工完成调研、构建、审核和成果提交。".to_string(),
            reward: "Contribution split".to_string(),
            recommended_roles: vec![
                "scout".to_string(),
                "builder".to_string(),
                "auditor".to_string(),
                "closer".to_string(),
            ],
        },
        LeagueMatch {
            match_id: "face-duel-001".to_string(),
            title: "附近对战：Agent Face Duel".to_string(),
            mode: "face_to_face_duel".to_string(),
            status: "open".to_string(),
            objective:
                "像 Pokémon 近距离对战一样，面对面选择 Agent 阵容、出招、提交证据并结算奖励。"
                    .to_string(),
            reward: "Duel XP + rating".to_string(),
            recommended_roles: vec![
                "scout".to_string(),
                "builder".to_string(),
                "auditor".to_string(),
            ],
        },
    ]
    .into_iter()
    .map(|league_match| (league_match.match_id.clone(), league_match))
    .collect();

    let now = Utc::now().timestamp();
    let guilds = [
        LeagueGuild {
            guild_id: "guild-prompt-forge".to_string(),
            name: "Prompt Forge".to_string(),
            motto: "Draft fast. Ship clean.".to_string(),
            status: "open".to_string(),
            rating: 1000,
            reputation: 0,
            treasury_credits: 0.0,
            created_at_epoch: now,
        },
        LeagueGuild {
            guild_id: "guild-audit-sanctum".to_string(),
            name: "Audit Sanctum".to_string(),
            motto: "No hallucination survives the raid.".to_string(),
            status: "open".to_string(),
            rating: 1000,
            reputation: 0,
            treasury_credits: 0.0,
            created_at_epoch: now,
        },
    ]
    .into_iter()
    .map(|guild| (guild.guild_id.clone(), guild))
    .collect();

    let league_skills = default_league_skills();
    let league_tools = default_league_tools();
    let league_skins = default_league_skins();

    let world_zones = [
        WorldZone {
            zone_id: "reality-mirror-city".to_string(),
            name: "Reality Mirror City".to_string(),
            status: "open".to_string(),
            theme: "现实世界映射、身份、关系和城市自由行动".to_string(),
            mirror_kind: "city".to_string(),
        },
        WorldZone {
            zone_id: "craft-district".to_string(),
            name: "Trillionnium Craft District".to_string(),
            status: "open".to_string(),
            theme: "建造、工坊、道具、摊位和创造系统".to_string(),
            mirror_kind: "builder_sandbox".to_string(),
        },
        WorldZone {
            zone_id: "market-bazaar".to_string(),
            name: "Market Bazaar".to_string(),
            status: "open".to_string(),
            theme: "真实任务、委托、悬赏、招募和声望".to_string(),
            mirror_kind: "market".to_string(),
        },
        WorldZone {
            zone_id: "league-arena".to_string(),
            name: "Trillionnium League Arena".to_string(),
            status: "open".to_string(),
            theme: "竞技、团本、赛季和裁判结算".to_string(),
            mirror_kind: "arena".to_string(),
        },
        WorldZone {
            zone_id: "civic-watch".to_string(),
            name: "Civic Watch Ring".to_string(),
            status: "open".to_string(),
            theme: "巡逻、证词、调解、导师和 NPC 社会关系压力".to_string(),
            mirror_kind: "social_survival_ring".to_string(),
        },
        WorldZone {
            zone_id: "survival-belt".to_string(),
            name: "Survival Supply Belt".to_string(),
            status: "open".to_string(),
            theme: "粮水、医疗、疲劳、营地和长期行程资源压力".to_string(),
            mirror_kind: "resource_pressure".to_string(),
        },
        WorldZone {
            zone_id: "survey-ridge".to_string(),
            name: "Survey Ridge".to_string(),
            status: "open".to_string(),
            theme: "测绘、阻挡规则、视野和路线规划".to_string(),
            mirror_kind: "route_topology".to_string(),
        },
    ]
    .into_iter()
    .map(|zone| (zone.zone_id.clone(), zone))
    .collect();

    let world_locations = [
        WorldLocation {
            location_id: "mirror-city-square".to_string(),
            zone_id: "reality-mirror-city".to_string(),
            name: "镜像城市广场".to_string(),
            location_kind: "public_hub".to_string(),
            description: "玩家、Agent 居民、公会和现实事件进入世界的公共入口。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "starter-studio".to_string(),
            zone_id: "craft-district".to_string(),
            name: "新手工坊".to_string(),
            location_kind: "workshop".to_string(),
            description: "自由建造第一间工坊、Agent 据点或公会基地。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "zbj-market-gate".to_string(),
            zone_id: "market-bazaar".to_string(),
            name: "悬赏集市门".to_string(),
            location_kind: "real_task_gateway".to_string(),
            description: "现实机会和线索映射为世界悬赏的入口。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "league-coliseum".to_string(),
            zone_id: "league-arena".to_string(),
            name: "League 竞技场".to_string(),
            location_kind: "arena".to_string(),
            description: "League 赛事、团本、评级、奖励与排行榜发生地。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "witness-archive".to_string(),
            zone_id: "civic-watch".to_string(),
            name: "见证档案馆".to_string(),
            location_kind: "archive".to_string(),
            description: "保存任务证词、争议记录和关系事件的原创证据链。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "night-watch-yard".to_string(),
            zone_id: "civic-watch".to_string(),
            name: "夜巡校场".to_string(),
            location_kind: "patrol_yard".to_string(),
            description: "训练夜间移动、暗线传信和巡逻路线的校场。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "river-cistern".to_string(),
            zone_id: "survival-belt".to_string(),
            name: "河湾水仓".to_string(),
            location_kind: "water_supply".to_string(),
            description: "管理饮水、补给和长线行军压力的水仓。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "ration-kitchen".to_string(),
            zone_id: "survival-belt".to_string(),
            name: "行灶补给棚".to_string(),
            location_kind: "ration_kitchen".to_string(),
            description: "烹饪干粮、分配队伍补给和恢复远行体力。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "field-infirmary".to_string(),
            zone_id: "survival-belt".to_string(),
            name: "野外医棚".to_string(),
            location_kind: "infirmary".to_string(),
            description: "处理轻伤、疲劳和长期年龄压力的恢复节点。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "elder-step".to_string(),
            zone_id: "civic-watch".to_string(),
            name: "长者石阶".to_string(),
            location_kind: "mediation_steps".to_string(),
            description: "居民调解公共关系、信任和冲突热度的社交节点。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "mentor-cloister".to_string(),
            zone_id: "civic-watch".to_string(),
            name: "导师回廊".to_string(),
            location_kind: "mentor_cloister".to_string(),
            description: "承载原创门籍、训练试炼和长期成长路线。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "courier-yard".to_string(),
            zone_id: "civic-watch".to_string(),
            name: "信使马厩".to_string(),
            location_kind: "courier_yard".to_string(),
            description: "处理远程送达、信使队伍和路线接力。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "survey-tower".to_string(),
            zone_id: "survey-ridge".to_string(),
            name: "地势测绘塔".to_string(),
            location_kind: "survey_tower".to_string(),
            description: "观察地形、记录阻挡规则和规划探索路线。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "guild-vault".to_string(),
            zone_id: "league-arena".to_string(),
            name: "公会库房".to_string(),
            location_kind: "guild_vault".to_string(),
            description: "保管团队装备、团本凭证和结算证据。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "auction-arcade".to_string(),
            zone_id: "market-bazaar".to_string(),
            name: "拍卖廊".to_string(),
            location_kind: "auction_arcade".to_string(),
            description: "承载鉴定、竞价、补给和商路选择的市场节点。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "caravan-rest-camp".to_string(),
            zone_id: "survival-belt".to_string(),
            name: "商队歇脚营".to_string(),
            location_kind: "caravan_camp".to_string(),
            description: "长途路线中的休整营地，承担补给、时间和疲劳压力。".to_string(),
            status: "open".to_string(),
        },
    ]
    .into_iter()
    .map(|location| (location.location_id.clone(), location))
    .collect();

    let world_entities = [
        WorldEntity {
            entity_id: "agent-oracle-scout".to_string(),
            location_id: "mirror-city-square".to_string(),
            name: "Oracle Scout".to_string(),
            entity_kind: "agent_resident".to_string(),
            role: "侦察、现实情报、任务发现".to_string(),
            status: "available".to_string(),
        },
        WorldEntity {
            entity_id: "agent-forge-builder".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Forge Builder".to_string(),
            entity_kind: "agent_resident".to_string(),
            role: "建造、生成、工坊资产".to_string(),
            status: "available".to_string(),
        },
        WorldEntity {
            entity_id: "city-clerk-ledger".to_string(),
            location_id: "mirror-city-square".to_string(),
            name: "Ledger Clerk".to_string(),
            entity_kind: "npc".to_string(),
            role: "资产登记、声望、合约和结算提示".to_string(),
            status: "available".to_string(),
        },
    ]
    .into_iter()
    .map(|entity| (entity.entity_id.clone(), entity))
    .collect();

    let world_map_nodes = default_world_map_nodes();

    let world_factions = [
        WorldFaction {
            faction_id: "faction-city-clerks".to_string(),
            zone_id: "reality-mirror-city".to_string(),
            name: "City Clerks".to_string(),
            faction_kind: "governance".to_string(),
            status: "open".to_string(),
            reputation_score: 0,
        },
        WorldFaction {
            faction_id: "faction-craft-union".to_string(),
            zone_id: "craft-district".to_string(),
            name: "Craft Union".to_string(),
            faction_kind: "builder".to_string(),
            status: "open".to_string(),
            reputation_score: 0,
        },
        WorldFaction {
            faction_id: "faction-market-guild".to_string(),
            zone_id: "market-bazaar".to_string(),
            name: "Market Guild".to_string(),
            faction_kind: "commerce".to_string(),
            status: "open".to_string(),
            reputation_score: 0,
        },
        WorldFaction {
            faction_id: "faction-league-order".to_string(),
            zone_id: "league-arena".to_string(),
            name: "League Order".to_string(),
            faction_kind: "competition".to_string(),
            status: "open".to_string(),
            reputation_score: 0,
        },
    ]
    .into_iter()
    .map(|faction| (faction.faction_id.clone(), faction))
    .collect();

    LeagueState {
        matches,
        players_by_matrix_user: HashMap::new(),
        entries: HashMap::new(),
        battles: HashMap::new(),
        submissions: HashMap::new(),
        rewards: Vec::new(),
        player_loadouts: HashMap::new(),
        guilds,
        guild_memberships: HashMap::new(),
        inventory_items: Vec::new(),
        league_skills,
        league_tools,
        league_skins,
        raid_contributions: Vec::new(),
        raid_rosters: Vec::new(),
        term_exchange_receipts: HashMap::new(),
        world: WorldState {
            world_zones,
            world_locations,
            world_entities,
            world_map_nodes,
            world_player_positions: HashMap::new(),
            world_trillionnium_characters: HashMap::new(),
            world_assets: Vec::new(),
            world_asset_upgrades: Vec::new(),
            world_companies: Vec::new(),
            world_shops: Vec::new(),
            world_listings: Vec::new(),
            world_economy_events: Vec::new(),
            world_purchases: Vec::new(),
            world_work_orders: Vec::new(),
            world_work_deliveries: Vec::new(),
            world_work_acceptances: Vec::new(),
            world_work_rejections: Vec::new(),
            world_work_reopens: Vec::new(),
            world_work_cancellations: Vec::new(),
            world_factions,
            world_faction_standings: Vec::new(),
            world_events: Vec::new(),
            world_contracts: Vec::new(),
            world_contract_completions: Vec::new(),
            world_relationships: Vec::new(),
            world_tactics_sessions: HashMap::new(),
            world_tactics_simulation_ticks: Vec::new(),
            world_term_exchange_receipts: HashMap::new(),
        },
    }
}

pub(super) fn map_exits(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(direction, node_id)| ((*direction).to_string(), (*node_id).to_string()))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn world_map_node(
    node_id: &str,
    location_id: &str,
    zone_id: &str,
    name: &str,
    node_kind: &str,
    description: &str,
    x: i64,
    y: i64,
    exits: &[(&str, &str)],
    tags: &[&str],
    hooks: &[&str],
) -> WorldMapNode {
    WorldMapNode {
        node_id: node_id.to_string(),
        location_id: location_id.to_string(),
        zone_id: zone_id.to_string(),
        name: name.to_string(),
        node_kind: node_kind.to_string(),
        description: description.to_string(),
        x,
        y,
        exits: map_exits(exits),
        interaction_tags: tags.iter().map(|value| (*value).to_string()).collect(),
        freedom_hooks: hooks.iter().map(|value| (*value).to_string()).collect(),
        status: "open".to_string(),
    }
}

pub(super) fn default_world_map_nodes() -> HashMap<String, WorldMapNode> {
    [
        world_map_node(
            "mirror-city-square",
            "mirror-city-square",
            "reality-mirror-city",
            "镜像城市广场",
            "hub_square",
            "经典文字冒险式的开局主广场：公告牌、Ledger Clerk、Agent 居民和现实任务入口都在附近。",
            0,
            0,
            &[
                ("east", "starter-studio"),
                ("south", "zbj-market-gate"),
                ("north", "league-coliseum"),
                ("west", "agent-dormitory"),
            ],
            &["talk", "notice_board", "ledger", "meet_agents"],
            &[
                "和 NPC 打听任务",
                "贴公告招募 Agent",
                "把现实想法登记成 World action",
            ],
        ),
        world_map_node(
            "agent-dormitory",
            "mirror-city-square",
            "reality-mirror-city",
            "Agent 宿舍巷",
            "agent_home",
            "自由招募、聊天、派遣 Agent 的小巷，适合做关系和队伍系统。",
            -1,
            0,
            &[("east", "mirror-city-square"), ("south", "ledger-office")],
            &["recruit", "relationship", "party"],
            &["拜访 Agent", "组队出门", "训练专长"],
        ),
        world_map_node(
            "ledger-office",
            "mirror-city-square",
            "reality-mirror-city",
            "Ledger 办事处",
            "ledger_office",
            "余额、预留款、合同与退款的城市窗口。",
            -1,
            1,
            &[("north", "agent-dormitory"), ("east", "zbj-market-gate")],
            &["wallet", "refund", "contract"],
            &["查账", "发起争议", "登记合约"],
        ),
        world_map_node(
            "starter-studio",
            "starter-studio",
            "craft-district",
            "新手工坊",
            "workshop_room",
            "第一间可布置工坊，Gather 风格的冒险房间，也是公会据点起点。",
            1,
            0,
            &[
                ("west", "mirror-city-square"),
                ("east", "forge-workbench"),
                ("south", "asset-yard"),
            ],
            &["craft", "guild", "decorate"],
            &["摆放道具", "升级工坊", "创建据点"],
        ),
        world_map_node(
            "forge-workbench",
            "starter-studio",
            "craft-district",
            "锻造工坊",
            "craft_station",
            "把想法打磨成方案、素材、道具和成果包的工作台。",
            2,
            0,
            &[("west", "starter-studio"), ("south", "asset-yard")],
            &["craft", "upgrade", "review"],
            &["生成道具", "审稿", "做成果包"],
        ),
        world_map_node(
            "asset-yard",
            "starter-studio",
            "craft-district",
            "道具庭院",
            "asset_yard",
            "收纳道具、素材和展示件的庭院，后续可自由布置。",
            1,
            1,
            &[("north", "starter-studio"), ("east", "zbj-market-gate")],
            &["asset", "inventory", "display"],
            &["查看道具", "升级道具", "挂到摊位"],
        ),
        world_map_node(
            "zbj-market-gate",
            "zbj-market-gate",
            "market-bazaar",
            "悬赏集市门",
            "market_gate",
            "现实机会映射成世界悬赏的入口。",
            0,
            2,
            &[
                ("north", "mirror-city-square"),
                ("west", "asset-yard"),
                ("east", "client-board"),
                ("south", "dispute-desk"),
            ],
            &["market", "contract", "bounty", "service"],
            &["接悬赏", "发布服务", "招募队友"],
        ),
        world_map_node(
            "client-board",
            "zbj-market-gate",
            "market-bazaar",
            "悬赏任务牌",
            "client_board",
            "像文字 MUD 的公告栏：任务、悬赏、提示和线索都贴在这里。",
            1,
            2,
            &[("west", "zbj-market-gate"), ("east", "delivery-dock")],
            &["listing", "bounty", "brief"],
            &["浏览悬赏", "接取挑战", "发布服务"],
        ),
        world_map_node(
            "delivery-dock",
            "zbj-market-gate",
            "market-bazaar",
            "成果评定台",
            "delivery_dock",
            "成果提交、评级、返工和放弃都在这里形成冒险路线。",
            2,
            2,
            &[("west", "client-board"), ("north", "league-coliseum")],
            &["deliver", "accept", "reject", "cancel"],
            &["提交成果", "评级", "发起返工或放弃"],
        ),
        world_map_node(
            "dispute-desk",
            "zbj-market-gate",
            "market-bazaar",
            "争议柜台",
            "dispute_desk",
            "后续 dispute / review hold / 仲裁规则的入口。",
            0,
            3,
            &[("north", "zbj-market-gate"), ("south", "witness-archive")],
            &["dispute", "refund", "review"],
            &["申请仲裁", "查看退款", "提交证据"],
        ),
        world_map_node(
            "league-coliseum",
            "league-coliseum",
            "league-arena",
            "League 竞技场",
            "arena_gate",
            "League 入口，任务可以从悬赏集市带进竞技场评级。",
            0,
            -1,
            &[
                ("south", "mirror-city-square"),
                ("east", "raid-hall"),
                ("south-east", "delivery-dock"),
            ],
            &["arena", "ranking", "judge"],
            &["加入赛场", "提交比赛", "查看排行"],
        ),
        world_map_node(
            "raid-hall",
            "league-coliseum",
            "league-arena",
            "公会团本厅",
            "raid_hall",
            "多人协作任务和 Agent 阵容站位的大厅。",
            1,
            -1,
            &[("west", "league-coliseum"), ("east", "guild-vault")],
            &["guild", "raid", "team"],
            &["认领职责", "组队打本", "分配 Agent"],
        ),
        world_map_node(
            "witness-archive",
            "witness-archive",
            "civic-watch",
            "见证档案馆",
            "witness_archive",
            "保存任务证词、争议记录和关系事件的档案馆，用原创证据链替代任何外部剧情表。",
            0,
            4,
            &[
                ("north", "dispute-desk"),
                ("east", "night-watch-yard"),
                ("south", "elder-step"),
            ],
            &["archive", "witness", "relationship", "proof"],
            &["查阅证词", "整理关系事件", "补全争议证据"],
        ),
        world_map_node(
            "night-watch-yard",
            "night-watch-yard",
            "civic-watch",
            "夜巡校场",
            "night_watch_yard",
            "训练夜间移动、暗线传信和巡逻路线的校场，承担长期行动风险教学。",
            1,
            4,
            &[
                ("west", "witness-archive"),
                ("east", "river-cistern"),
                ("south", "courier-yard"),
            ],
            &["patrol", "stealth", "messaging", "night"],
            &["练夜巡步", "接暗线信使任务", "检查巡逻风险"],
        ),
        world_map_node(
            "river-cistern",
            "river-cistern",
            "survival-belt",
            "河湾水仓",
            "river_cistern",
            "管理饮水、补给和长线行军压力的水仓，服务 food/water/age 生存循环。",
            2,
            4,
            &[("west", "night-watch-yard"), ("south", "ration-kitchen")],
            &["water", "survival", "restock", "weather"],
            &["补水", "检查脱水风险", "安排雨季路线"],
        ),
        world_map_node(
            "ration-kitchen",
            "ration-kitchen",
            "survival-belt",
            "行灶补给棚",
            "ration_kitchen",
            "烹饪干粮、分配队伍补给和恢复远行体力的原创生存节点。",
            2,
            5,
            &[("north", "river-cistern"), ("west", "field-infirmary")],
            &["food", "cooking", "party_supply", "rest"],
            &["做行粮", "分配队伍补给", "降低饥饿风险"],
        ),
        world_map_node(
            "field-infirmary",
            "field-infirmary",
            "survival-belt",
            "野外医棚",
            "field_infirmary",
            "处理轻伤、疲劳和长期年龄压力的医棚，连接战斗失败后的恢复路线。",
            1,
            5,
            &[
                ("east", "ration-kitchen"),
                ("west", "elder-step"),
                ("south", "caravan-rest-camp"),
            ],
            &["medicine", "injury", "recovery", "age"],
            &["处理轻伤", "调配补剂", "评估长期体能"],
        ),
        world_map_node(
            "elder-step",
            "elder-step",
            "civic-watch",
            "长者石阶",
            "elder_step",
            "长者和居民调解公共关系的石阶，推动 NPC 信任、冲突热度和派系站位变化。",
            0,
            5,
            &[
                ("north", "witness-archive"),
                ("east", "field-infirmary"),
                ("south", "mentor-cloister"),
            ],
            &["elder", "mediation", "social", "trust"],
            &["听取传闻", "调停关系", "恢复导师信任"],
        ),
        world_map_node(
            "mentor-cloister",
            "mentor-cloister",
            "civic-watch",
            "导师回廊",
            "mentor_cloister",
            "原创导师回廊，承载门派称号、训练试炼和长期成长路线，而不是引用外部门派表。",
            0,
            6,
            &[("north", "elder-step"), ("east", "caravan-rest-camp")],
            &["mentor", "sect", "training", "title_ladder"],
            &["拜访导师", "进行门籍试炼", "规划长期成长"],
        ),
        world_map_node(
            "courier-yard",
            "courier-yard",
            "civic-watch",
            "信使马厩",
            "courier_yard",
            "处理远程送达、信使队伍和路线接力的马厩，连接地图探索和任务物流。",
            1,
            5,
            &[
                ("north", "night-watch-yard"),
                ("east", "survey-tower"),
                ("south", "auction-arcade"),
            ],
            &["courier", "route", "handoff", "party"],
            &["安排信使", "接力路线", "追踪队伍位置"],
        ),
        world_map_node(
            "survey-tower",
            "survey-tower",
            "survey-ridge",
            "地势测绘塔",
            "survey_tower",
            "观察地形、记录阻挡规则和规划探索路线的高塔。",
            2,
            5,
            &[("west", "courier-yard"), ("south", "guild-vault")],
            &["terrain", "survey", "map", "blocked_path"],
            &["测绘地形", "标注阻挡", "优化路线"],
        ),
        world_map_node(
            "guild-vault",
            "guild-vault",
            "league-arena",
            "公会库房",
            "guild_vault",
            "保管团队装备、团本凭证和结算证据的公会库房。",
            2,
            -1,
            &[
                ("west", "raid-hall"),
                ("north", "survey-tower"),
                ("south-west", "auction-arcade"),
            ],
            &["guild", "inventory", "raid", "evidence"],
            &["整理团本凭证", "保管装备", "复核结算证据"],
        ),
        world_map_node(
            "auction-arcade",
            "auction-arcade",
            "market-bazaar",
            "拍卖廊",
            "auction_arcade",
            "原创市场拍卖廊，承载鉴定、竞价、补给和商路选择。",
            1,
            6,
            &[
                ("north", "courier-yard"),
                ("north-east", "guild-vault"),
                ("south-east", "caravan-rest-camp"),
            ],
            &["auction", "appraisal", "commerce", "supply"],
            &["鉴定物件", "竞价补给", "选择商路"],
        ),
        world_map_node(
            "caravan-rest-camp",
            "caravan-rest-camp",
            "survival-belt",
            "商队歇脚营",
            "caravan_rest_camp",
            "长途路线中的休整营地，压力来自补给、时间、队伍疲劳和事件等待。",
            1,
            7,
            &[
                ("north", "field-infirmary"),
                ("west", "mentor-cloister"),
                ("north-west", "auction-arcade"),
            ],
            &["camp", "time", "fatigue", "caravan"],
            &["扎营休整", "计算行程天数", "处理队伍疲劳"],
        ),
    ]
    .into_iter()
    .map(|node| (node.node_id.clone(), node))
    .collect()
}

pub(super) fn league_skill(
    skill_id: &str,
    name: &str,
    skill_kind: &str,
    school_id: &str,
    description: &str,
    unlock_level: i64,
    max_rank: i64,
) -> LeagueSkill {
    LeagueSkill {
        skill_id: skill_id.to_string(),
        name: name.to_string(),
        skill_kind: skill_kind.to_string(),
        school_id: school_id.to_string(),
        description: description.to_string(),
        unlock_level,
        max_rank,
        status: "open".to_string(),
    }
}

pub(super) fn league_tool(
    tool_id: &str,
    name: &str,
    tool_kind: &str,
    slot: &str,
    description: &str,
    unlock_level: i64,
    power_bonus: i64,
) -> LeagueTool {
    LeagueTool {
        tool_id: tool_id.to_string(),
        name: name.to_string(),
        tool_kind: tool_kind.to_string(),
        slot: slot.to_string(),
        description: description.to_string(),
        unlock_level,
        power_bonus,
        status: "open".to_string(),
    }
}

pub(super) fn league_skin(
    skin_id: &str,
    name: &str,
    skin_kind: &str,
    description: &str,
    unlock_level: i64,
    agent_count: i64,
    multi_agent_capability: &str,
) -> LeagueSkin {
    LeagueSkin {
        skin_id: skin_id.to_string(),
        name: name.to_string(),
        skin_kind: skin_kind.to_string(),
        description: description.to_string(),
        unlock_level,
        agent_count,
        multi_agent_capability: multi_agent_capability.to_string(),
        status: "open".to_string(),
    }
}

pub(super) fn default_league_skills() -> HashMap<String, LeagueSkill> {
    [
        league_skill(
            "skill-reality-scouting",
            "Reality Scouting",
            "discovery",
            "guild-prompt-forge",
            "把真实世界线索、委托需求和地图兴趣点沉淀为可执行任务。",
            1,
            5,
        ),
        league_skill(
            "skill-prompt-forging",
            "Prompt Forging",
            "build",
            "guild-prompt-forge",
            "把需求拆成 Agent 可执行的提示、流程和评级标准。",
            2,
            5,
        ),
        league_skill(
            "skill-evidence-audit",
            "Evidence Audit",
            "audit",
            "guild-audit-sanctum",
            "复核证据、风险、隐藏测试和委托评级口径。",
            3,
            5,
        ),
        league_skill(
            "skill-commerce-closing",
            "Commerce Closing",
            "commerce",
            "faction-market-guild",
            "把报价、成果、评级、返工、退回和复访闭成委托路线。",
            4,
            5,
        ),
        league_skill(
            "skill-world-routing",
            "World Routing",
            "map",
            "faction-city-clerks",
            "在真实世界镜像地图上用轻量节点/分片组织多人移动与事件。",
            5,
            5,
        ),
    ]
    .into_iter()
    .map(|skill| (skill.skill_id.clone(), skill))
    .collect()
}

pub(super) fn default_league_tools() -> HashMap<String, LeagueTool> {
    [
        league_tool(
            "tool-scout-lens",
            "Scout Lens",
            "intel_tool",
            "main_hand",
            "提升任务发现、地图兴趣点识别和委托需求结构化能力。",
            1,
            8,
        ),
        league_tool(
            "tool-forge-kit",
            "Forge Kit",
            "build_tool",
            "workbench",
            "提升方案生成、资产升级和公司/店铺搭建质量。",
            2,
            12,
        ),
        league_tool(
            "tool-audit-mirror",
            "Audit Mirror",
            "review_tool",
            "off_hand",
            "提升自检、隐藏测试命中率和成果风险控制。",
            3,
            15,
        ),
        league_tool(
            "tool-ledger-key",
            "Ledger Key",
            "wallet_tool",
            "accessory",
            "解锁更稳定的 reserve / consume / refund / grant 商业结算体验。",
            4,
            18,
        ),
    ]
    .into_iter()
    .map(|tool| (tool.tool_id.clone(), tool))
    .collect()
}

pub(super) fn default_league_skins() -> HashMap<String, LeagueSkin> {
    [
        league_skin(
            "skin-single-agent-adventurer",
            "Single Agent Adventurer",
            "agent_skin",
            "单 Agent 冒险者外观；适合早期英雄坛说式独行探索。",
            1,
            1,
            "solo_agent",
        ),
        league_skin(
            "skin-duo-field-team",
            "Duo Field Team",
            "party_skin",
            "双 Agent 小队形态；适合 Gather 风格面对面协作/对战。",
            3,
            2,
            "dual_agent_coordination",
        ),
        league_skin(
            "skin-raid-cell",
            "Raid Cell",
            "multi_agent_skin",
            "四到五 Agent 团本细胞；把 scout/build/audit/close 变成可见协作形态。",
            5,
            4,
            "multi_agent_raid_cell",
        ),
        league_skin(
            "skin-guild-caravan",
            "Guild Caravan",
            "guild_skin",
            "门派商队/公会小队皮肤；把真实世界地图移动、交易和组队能力合并呈现。",
            8,
            6,
            "guild_scale_agent_caravan",
        ),
    ]
    .into_iter()
    .map(|skin| (skin.skin_id.clone(), skin))
    .collect()
}

pub(super) fn load_league_state(config: &ConsumerEntryConfig) -> LeagueState {
    let mut state = config
        .league_state_path
        .as_deref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<LeagueState>(&raw).ok())
        .unwrap_or_else(default_league_state);

    merge_default_league_state(&mut state);
    state
}

pub(super) async fn load_league_state_for_startup(
    config: &ConsumerEntryConfig,
) -> Result<LeagueState, String> {
    if config.league_normalized_read_switch_enabled {
        let mut state = load_league_state_from_normalized_repository(config).await?;
        merge_default_league_state(&mut state);
        return Ok(state);
    }

    Ok(load_league_state(config))
}

pub(super) async fn load_league_state_from_normalized_repository(
    config: &ConsumerEntryConfig,
) -> Result<LeagueState, String> {
    let database_url = config.league_normalized_database_url.as_deref().ok_or_else(|| {
        "CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=true requires CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL"
            .to_string()
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .map_err(|err| format!("failed to connect normalized repository database: {err}"))?;
    let snapshot = sqlx::query_as::<_, (String, String)>(
        "select state_hash, state::text from league_state_snapshots where snapshot_kind = 'consumer_entry_json_v1' order by created_at desc limit 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|err| format!("failed to read normalized repository state snapshot: {err}"));
    let (state_hash, state_json) = match snapshot? {
        Some(snapshot) => snapshot,
        None => {
            pool.close().await;
            return Err(
                "normalized repository read switch is enabled but no consumer_entry_json_v1 snapshot exists"
                    .to_string(),
            );
        }
    };
    let read_switch_gate_counts = sqlx::query_as::<_, (i64, i64)>(
        "select
            (select count(*)::bigint from league_state_repository_snapshots where state_hash = $1 and cutover_phase in ('final_cutover', 'shadow_snapshot')) as repository_audit_rows,
            (select count(*)::bigint from league_state_repository_write_set_audits where state_hash = $1 and cutover_phase in ('final_cutover', 'shadow_snapshot')) as write_set_audit_rows",
    )
    .bind(&state_hash)
    .fetch_one(&pool)
    .await
    .map_err(|err| format!("failed to validate normalized repository read-switch gates: {err}"));
    let (repository_audit_rows, write_set_audit_rows) = read_switch_gate_counts?;
    if repository_audit_rows < 1 {
        pool.close().await;
        return Err("normalized repository read switch is enabled but no repository audit snapshot exists for latest state hash".to_string());
    }
    if write_set_audit_rows < 12 {
        pool.close().await;
        return Err("normalized repository read switch is enabled but repository write-set audit is incomplete for latest state hash".to_string());
    }
    let world_home_read_model =
        sqlx::query_scalar::<_, Value>(normalized_repository_world_home_read_model_sql())
            .fetch_one(&pool)
            .await
            .map_err(|err| {
                format!(
                    "failed to validate normalized repository world-home read model gate: {err}"
                )
            });
    let client_feed_read_model =
        sqlx::query_scalar::<_, Value>(normalized_repository_client_feed_read_model_sql())
            .fetch_one(&pool)
            .await
            .map_err(|err| {
                format!(
                    "failed to validate normalized repository client-feed read model gate: {err}"
                )
            });
    pool.close().await;
    validate_normalized_repository_read_model_gate(&world_home_read_model?)?;
    validate_normalized_repository_client_feed_read_model_gate(&client_feed_read_model?)?;
    serde_json::from_str::<LeagueState>(&state_json)
        .map_err(|err| format!("failed to deserialize normalized repository state snapshot: {err}"))
}

pub(super) fn validate_normalized_repository_read_model_gate(
    read_model: &Value,
) -> Result<(), String> {
    if read_model.get("read_model_version").and_then(Value::as_str)
        != Some("trillionnium_normalized_world_home_read_model_v1")
    {
        return Err("normalized repository read switch is enabled but normalized world-home read model version is invalid".to_string());
    }
    let source_table_count = read_model
        .get("source_tables")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    if source_table_count < 6 {
        return Err("normalized repository read switch is enabled but normalized world-home read model source table coverage is incomplete".to_string());
    }
    if read_model
        .get("world_map_node_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
    {
        return Err("normalized repository read switch is enabled but normalized world-home read model has no map nodes".to_string());
    }
    Ok(())
}

pub(super) fn validate_normalized_repository_client_feed_read_model_gate(
    read_model: &Value,
) -> Result<(), String> {
    if read_model.get("read_model_version").and_then(Value::as_str)
        != Some("trillionnium_normalized_client_feed_read_model_v1")
    {
        return Err("normalized repository read switch is enabled but normalized client-feed read model version is invalid".to_string());
    }
    let source_table_count = read_model
        .get("source_tables")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    if source_table_count < 9 {
        return Err("normalized repository read switch is enabled but normalized client-feed read model source table coverage is incomplete".to_string());
    }
    if read_model
        .get("latest_feed_items")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err("normalized repository read switch is enabled but normalized client-feed read model is missing latest_feed_items".to_string());
    }
    Ok(())
}

pub(super) fn merge_default_league_state(state: &mut LeagueState) {
    let defaults = default_league_state();
    for (match_id, league_match) in defaults.matches {
        state.matches.entry(match_id).or_insert(league_match);
    }
    for (guild_id, guild) in defaults.guilds {
        state.guilds.entry(guild_id).or_insert(guild);
    }
    for (skill_id, skill) in defaults.league_skills {
        state.league_skills.entry(skill_id).or_insert(skill);
    }
    for (tool_id, tool) in defaults.league_tools {
        state.league_tools.entry(tool_id).or_insert(tool);
    }
    for (skin_id, skin) in defaults.league_skins {
        state.league_skins.entry(skin_id).or_insert(skin);
    }
    for (zone_id, zone) in defaults.world.world_zones {
        state.world.world_zones.entry(zone_id).or_insert(zone);
    }
    for (location_id, location) in defaults.world.world_locations {
        state
            .world
            .world_locations
            .entry(location_id)
            .or_insert(location);
    }
    for (entity_id, entity) in defaults.world.world_entities {
        state
            .world
            .world_entities
            .entry(entity_id)
            .or_insert(entity);
    }
    for (node_id, node) in defaults.world.world_map_nodes {
        state.world.world_map_nodes.entry(node_id).or_insert(node);
    }
    for (faction_id, faction) in defaults.world.world_factions {
        state
            .world
            .world_factions
            .entry(faction_id)
            .or_insert(faction);
    }
}

pub(super) fn sql_quote(value: &str) -> String {
    value.replace('\'', "''")
}

pub(super) fn sql_text_array_literal(values: &[String]) -> String {
    if values.is_empty() {
        return "array[]::text[]".to_string();
    }
    format!(
        "array[{}]::text[]",
        values
            .iter()
            .map(|value| format!("'{}'", sql_quote(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(super) fn normalized_shadow_json_upsert_sql<T: Serialize>(
    table_name: &str,
    rows: &[T],
    record_schema: &str,
    insert_columns: &[&str],
    select_columns: &[&str],
    conflict_target: &str,
    update_columns: &[&str],
) -> Result<String, serde_json::Error> {
    if rows.is_empty() {
        return Ok(String::new());
    }
    let rows_json = serde_json::to_string(rows)?;
    let updates = if update_columns.is_empty() {
        "do nothing".to_string()
    } else {
        format!(
            "do update set {}",
            update_columns
                .iter()
                .map(|column| format!("{column} = excluded.{column}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Ok(format!(
        "with raw_rows as (\n  select value, ordinal\n  from jsonb_array_elements('{}'::jsonb) with ordinality as raw(value, ordinal)\n), rows as (\n  select x.*, raw_rows.ordinal\n  from raw_rows\n  cross join lateral jsonb_to_record(raw_rows.value) as x({})\n), deduped as (\n  select distinct on ({}) *\n  from rows\n  order by {}, ordinal desc\n)\ninsert into {} ({})\nselect {}\nfrom deduped\non conflict ({}) {};\n",
        sql_quote(&rows_json),
        record_schema,
        conflict_target,
        conflict_target,
        table_name,
        insert_columns.join(", "),
        select_columns.join(", "),
        conflict_target,
        updates,
    ))
}

pub(super) fn normalized_term_exchange_receipts_shadow_sql(
    table_name: &str,
    receipts: &[&TermExchangeReceiptState],
) -> Result<String, serde_json::Error> {
    normalized_shadow_json_upsert_sql(
        table_name,
        receipts,
        "protocol_version text, receipt_id text, intent_id text, term_id text, backend_id text, backend_kind text, status text, progression_class text, settlement_reference text, ledger_entry_id text, reason text, finalized_at_epoch bigint",
        &[
            "protocol_version",
            "receipt_id",
            "intent_id",
            "term_id",
            "backend_id",
            "backend_kind",
            "status",
            "progression_class",
            "settlement_reference",
            "ledger_entry_id",
            "reason",
            "finalized_at",
            "updated_at",
        ],
        &[
            "protocol_version",
            "receipt_id",
            "intent_id",
            "term_id",
            "backend_id",
            "backend_kind",
            "status",
            "progression_class",
            "settlement_reference",
            "ledger_entry_id",
            "reason",
            "to_timestamp(finalized_at_epoch)",
            "now()",
        ],
        "receipt_id",
        &[
            "protocol_version",
            "intent_id",
            "term_id",
            "backend_id",
            "backend_kind",
            "status",
            "progression_class",
            "settlement_reference",
            "ledger_entry_id",
            "reason",
            "finalized_at",
            "updated_at",
        ],
    )
}

pub(super) fn league_term_exchange_receipts_shadow_sql(
    league: &LeagueState,
) -> Result<String, serde_json::Error> {
    let mut receipt_ids: Vec<&String> = league.term_exchange_receipts.keys().collect();
    receipt_ids.sort();
    let receipts: Vec<&TermExchangeReceiptState> = receipt_ids
        .into_iter()
        .filter_map(|receipt_id| league.term_exchange_receipts.get(receipt_id))
        .collect();
    normalized_term_exchange_receipts_shadow_sql("league_term_exchange_receipts", &receipts)
}

pub(super) fn world_state_normalized_shadow_sql(
    world: &WorldState,
) -> Result<String, serde_json::Error> {
    let mut sql =
        String::from("-- Normalized WorldState shadow upserts (generated from JSON snapshot).\n");
    let indexes = build_world_indexes(world);

    let zones: Vec<&WorldZone> = indexes
        .sorted_zone_ids
        .iter()
        .filter_map(|zone_id| world.world_zones.get(zone_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_zones",
        &zones,
        "zone_id text, name text, status text, theme text, mirror_kind text",
        &["zone_id", "name", "status", "theme", "mirror_kind"],
        &["zone_id", "name", "status", "theme", "mirror_kind"],
        "zone_id",
        &["name", "status", "theme", "mirror_kind"],
    )?);

    let locations: Vec<&WorldLocation> = indexes
        .sorted_location_ids
        .iter()
        .filter_map(|location_id| world.world_locations.get(location_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_locations",
        &locations,
        "location_id text, zone_id text, name text, location_kind text, description text, status text",
        &["location_id", "zone_id", "name", "location_kind", "description", "status"],
        &["location_id", "zone_id", "name", "location_kind", "description", "status"],
        "location_id",
        &["zone_id", "name", "location_kind", "description", "status"],
    )?);

    let entities: Vec<&WorldEntity> = indexes
        .sorted_entity_ids
        .iter()
        .filter_map(|entity_id| world.world_entities.get(entity_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_entities",
        &entities,
        "entity_id text, location_id text, name text, entity_kind text, role text, status text",
        &[
            "entity_id",
            "location_id",
            "name",
            "entity_kind",
            "role",
            "status",
        ],
        &[
            "entity_id",
            "location_id",
            "name",
            "entity_kind",
            "role",
            "status",
        ],
        "entity_id",
        &["location_id", "name", "entity_kind", "role", "status"],
    )?);

    let map_nodes: Vec<&WorldMapNode> = indexes
        .sorted_map_node_ids_by_id
        .iter()
        .filter_map(|node_id| world.world_map_nodes.get(node_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_map_nodes",
        &map_nodes,
        "node_id text, location_id text, zone_id text, name text, node_kind text, description text, x bigint, y bigint, exits jsonb, interaction_tags jsonb, freedom_hooks jsonb, status text",
        &["node_id", "location_id", "zone_id", "name", "node_kind", "description", "x", "y", "exits", "interaction_tags", "freedom_hooks", "status"],
        &["node_id", "location_id", "zone_id", "name", "node_kind", "description", "x", "y", "exits", "interaction_tags", "freedom_hooks", "status"],
        "node_id",
        &["location_id", "zone_id", "name", "node_kind", "description", "x", "y", "exits", "interaction_tags", "freedom_hooks", "status"],
    )?);

    let player_positions: Vec<&WorldPlayerPosition> = indexes
        .sorted_player_position_user_ids
        .iter()
        .filter_map(|matrix_user_id| world.world_player_positions.get(matrix_user_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_player_positions",
        &player_positions,
        "matrix_user_id text, node_id text, location_id text, updated_at_epoch bigint",
        &["matrix_user_id", "node_id", "location_id", "updated_at"],
        &[
            "matrix_user_id",
            "node_id",
            "location_id",
            "to_timestamp(updated_at_epoch)",
        ],
        "matrix_user_id",
        &["node_id", "location_id", "updated_at"],
    )?);

    let trillionnium_characters: Vec<&WorldTrillionniumCharacter> = indexes
        .sorted_trillionnium_character_user_ids
        .iter()
        .filter_map(|matrix_user_id| world.world_trillionnium_characters.get(matrix_user_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_trillionnium_characters",
        &trillionnium_characters,
        "matrix_user_id text, character_id text, display_name text, attributes jsonb, sect_id text, title text, skill_ids jsonb, inventory_items jsonb, equipment_slots jsonb, resource_pressure_state jsonb, region_story_unlock_state jsonb, combat_numerics_state jsonb, updated_at_epoch bigint",
        &["matrix_user_id", "character_id", "display_name", "attributes", "sect_id", "title", "skill_ids", "inventory_items", "equipment_slots", "resource_pressure_state", "region_story_unlock_state", "combat_numerics_state", "updated_at"],
        &["matrix_user_id", "character_id", "display_name", "attributes", "sect_id", "title", "skill_ids", "inventory_items", "equipment_slots", "resource_pressure_state", "region_story_unlock_state", "combat_numerics_state", "to_timestamp(updated_at_epoch)"],
        "matrix_user_id",
        &["character_id", "display_name", "attributes", "sect_id", "title", "skill_ids", "inventory_items", "equipment_slots", "resource_pressure_state", "region_story_unlock_state", "combat_numerics_state", "updated_at"],
    )?);

    let tactics_sessions: Vec<&WorldTacticsGameSession> = indexes
        .sorted_tactics_session_ids
        .iter()
        .filter_map(|session_id| world.world_tactics_sessions.get(session_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_tactics_sessions",
        &tactics_sessions,
        "contract_version text, session_id text, matrix_user_id text, room_id text, board_id text, active_node_id text, active_overlay_id text, active_unit_id text, active_side text, status text, round bigint, action_points_remaining bigint, current_tick bigint, objective_id text, objective_progress bigint, objective_goal bigint, victory_state text, reward_status text, reward_event_id text, reward_credits_awarded bigint, reward_xp_awarded bigint, created_at_epoch bigint, updated_at_epoch bigint, source_of_truth text, persistence_owner text",
        &["session_id", "matrix_user_id", "room_id", "board_id", "active_node_id", "active_overlay_id", "active_unit_id", "active_side", "status", "round", "action_points_remaining", "current_tick", "objective_id", "objective_progress", "objective_goal", "victory_state", "reward_status", "reward_event_id", "reward_credits_awarded", "reward_xp_awarded", "created_at", "updated_at", "source_of_truth", "persistence_owner"],
        &["session_id", "matrix_user_id", "room_id", "board_id", "active_node_id", "active_overlay_id", "active_unit_id", "active_side", "status", "round", "action_points_remaining", "current_tick", "objective_id", "objective_progress", "objective_goal", "victory_state", "reward_status", "reward_event_id", "reward_credits_awarded", "reward_xp_awarded", "to_timestamp(created_at_epoch)", "to_timestamp(updated_at_epoch)", "source_of_truth", "persistence_owner"],
        "session_id",
        &["matrix_user_id", "room_id", "board_id", "active_node_id", "active_overlay_id", "active_unit_id", "active_side", "status", "round", "action_points_remaining", "current_tick", "objective_id", "objective_progress", "objective_goal", "victory_state", "reward_status", "reward_event_id", "reward_credits_awarded", "reward_xp_awarded", "created_at", "updated_at", "source_of_truth", "persistence_owner"],
    )?);

    let tactics_ticks = indexed_sorted(
        &world.world_tactics_simulation_ticks,
        &indexes.sorted_tactics_simulation_tick_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_tactics_simulation_ticks",
        &tactics_ticks,
        "contract_version text, tick_id text, session_id text, matrix_user_id text, room_id text, tick_index bigint, command text, unit_id text, target_tile text, outcome_result text, outcome_accepted boolean, simulation_effect text, round_before bigint, round_after bigint, action_points_before bigint, action_points_after bigint, objective_id text, objective_progress_before bigint, objective_progress_after bigint, objective_delta bigint, victory_state_before text, victory_state_after text, reward_status_after text, active_unit_after text, generated_encounter_id text, osm_game_overlay_id text, created_at_epoch bigint, source_of_truth text",
        &["tick_id", "session_id", "matrix_user_id", "room_id", "tick_index", "command", "unit_id", "target_tile", "outcome_result", "outcome_accepted", "simulation_effect", "round_before", "round_after", "action_points_before", "action_points_after", "objective_id", "objective_progress_before", "objective_progress_after", "objective_delta", "victory_state_before", "victory_state_after", "reward_status_after", "active_unit_after", "generated_encounter_id", "osm_game_overlay_id", "created_at", "source_of_truth"],
        &["tick_id", "session_id", "matrix_user_id", "room_id", "tick_index", "command", "unit_id", "target_tile", "outcome_result", "outcome_accepted", "simulation_effect", "round_before", "round_after", "action_points_before", "action_points_after", "objective_id", "objective_progress_before", "objective_progress_after", "objective_delta", "victory_state_before", "victory_state_after", "reward_status_after", "active_unit_after", "generated_encounter_id", "osm_game_overlay_id", "to_timestamp(created_at_epoch)", "source_of_truth"],
        "tick_id",
        &["session_id", "matrix_user_id", "room_id", "tick_index", "command", "unit_id", "target_tile", "outcome_result", "outcome_accepted", "simulation_effect", "round_before", "round_after", "action_points_before", "action_points_after", "objective_id", "objective_progress_before", "objective_progress_after", "objective_delta", "victory_state_before", "victory_state_after", "reward_status_after", "active_unit_after", "generated_encounter_id", "osm_game_overlay_id", "created_at", "source_of_truth"],
    )?);

    let world_term_exchange_receipts: Vec<&TermExchangeReceiptState> = indexes
        .sorted_world_term_exchange_receipt_ids
        .iter()
        .filter_map(|receipt_id| world.world_term_exchange_receipts.get(receipt_id))
        .collect();
    sql.push_str(&normalized_term_exchange_receipts_shadow_sql(
        "world_term_exchange_receipts",
        &world_term_exchange_receipts,
    )?);

    let assets = indexed_sorted(&world.world_assets, &indexes.sorted_asset_indices_by_id);
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_assets",
        &assets,
        "asset_id text, owner_matrix_user_id text, location_id text, asset_kind text, name text, status text, value_score bigint, upgrade_level bigint, upgrade_points bigint, last_upgrade_kind text, created_at_epoch bigint",
        &["asset_id", "owner_matrix_user_id", "location_id", "asset_kind", "name", "status", "value_score", "upgrade_level", "upgrade_points", "last_upgrade_kind", "created_at"],
        &["asset_id", "owner_matrix_user_id", "location_id", "asset_kind", "name", "status", "value_score", "upgrade_level", "upgrade_points", "last_upgrade_kind", "to_timestamp(created_at_epoch)"],
        "asset_id",
        &["owner_matrix_user_id", "location_id", "asset_kind", "name", "status", "value_score", "upgrade_level", "upgrade_points", "last_upgrade_kind", "created_at"],
    )?);

    let events = indexed_sorted(&world.world_events, &indexes.sorted_event_indices_by_id);
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_events",
        &events,
        "event_id text, actor_matrix_user_id text, room_id text, location_id text, event_kind text, body text, result text, impact_score bigint, cex_task_id text, cex_status text, created_at_epoch bigint",
        &["event_id", "actor_matrix_user_id", "room_id", "location_id", "event_kind", "body", "result", "impact_score", "cex_task_id", "cex_status", "created_at"],
        &["event_id", "actor_matrix_user_id", "room_id", "location_id", "event_kind", "body", "result", "impact_score", "cex_task_id", "cex_status", "to_timestamp(created_at_epoch)"],
        "event_id",
        &["actor_matrix_user_id", "room_id", "location_id", "event_kind", "body", "result", "impact_score", "cex_task_id", "cex_status", "created_at"],
    )?);

    let relationships = indexed_sorted(
        &world.world_relationships,
        &indexes.sorted_relationship_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_relationships",
        &relationships,
        "relationship_id text, from_id text, to_id text, relation_kind text, strength bigint, updated_at_epoch bigint",
        &["relationship_id", "from_id", "to_id", "relation_kind", "strength", "updated_at"],
        &["relationship_id", "from_id", "to_id", "relation_kind", "strength", "to_timestamp(updated_at_epoch)"],
        "relationship_id",
        &["from_id", "to_id", "relation_kind", "strength", "updated_at"],
    )?);

    let contracts = indexed_sorted(
        &world.world_contracts,
        &indexes.sorted_contract_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_contracts",
        &contracts,
        "contract_id text, event_id text, actor_matrix_user_id text, location_id text, task_id text, title text, body text, status text, cex_status text, value_score bigint, created_at_epoch bigint",
        &["contract_id", "event_id", "actor_matrix_user_id", "location_id", "task_id", "title", "body", "status", "cex_status", "value_score", "created_at"],
        &["contract_id", "event_id", "actor_matrix_user_id", "location_id", "task_id", "title", "body", "status", "cex_status", "value_score", "to_timestamp(created_at_epoch)"],
        "contract_id",
        &["event_id", "actor_matrix_user_id", "location_id", "task_id", "title", "body", "status", "cex_status", "value_score", "created_at"],
    )?);

    let completions = indexed_sorted(
        &world.world_contract_completions,
        &indexes.sorted_contract_completion_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_contract_completions",
        &completions,
        "completion_id text, contract_id text, matrix_user_id text, body text, score numeric, grade text, reward_amount numeric, judge_status text, payout_status text, anti_cheat_flags jsonb, score_events jsonb, ledger_status text, ledger_account_id text, ledger_entry_id text, ledger_balance_after numeric, ledger_error text, created_at_epoch bigint",
        &["completion_id", "contract_id", "matrix_user_id", "body", "score", "grade", "reward_amount", "judge_status", "payout_status", "anti_cheat_flags", "score_events", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "created_at"],
        &["completion_id", "contract_id", "matrix_user_id", "body", "score", "grade", "reward_amount", "judge_status", "payout_status", "anti_cheat_flags", "score_events", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "to_timestamp(created_at_epoch)"],
        "completion_id",
        &["contract_id", "matrix_user_id", "body", "score", "grade", "reward_amount", "judge_status", "payout_status", "anti_cheat_flags", "score_events", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "created_at"],
    )?);

    let asset_upgrades = indexed_sorted(
        &world.world_asset_upgrades,
        &indexes.sorted_asset_upgrade_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_asset_upgrades",
        &asset_upgrades,
        "upgrade_id text, asset_id text, matrix_user_id text, body text, upgrade_kind text, score numeric, grade text, judge_status text, status text, value_delta bigint, level_before bigint, level_after bigint, created_at_epoch bigint",
        &["upgrade_id", "asset_id", "matrix_user_id", "body", "upgrade_kind", "score", "grade", "judge_status", "status", "value_delta", "level_before", "level_after", "created_at"],
        &["upgrade_id", "asset_id", "matrix_user_id", "body", "upgrade_kind", "score", "grade", "judge_status", "status", "value_delta", "level_before", "level_after", "to_timestamp(created_at_epoch)"],
        "upgrade_id",
        &["asset_id", "matrix_user_id", "body", "upgrade_kind", "score", "grade", "judge_status", "status", "value_delta", "level_before", "level_after", "created_at"],
    )?);

    let companies = indexed_sorted(
        &world.world_companies,
        &indexes.sorted_company_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_companies",
        &companies,
        "company_id text, owner_matrix_user_id text, asset_id text, location_id text, name text, company_kind text, status text, revenue_score bigint, reputation_score bigint, level bigint, created_at_epoch bigint",
        &["company_id", "owner_matrix_user_id", "asset_id", "location_id", "name", "company_kind", "status", "revenue_score", "reputation_score", "level", "created_at"],
        &["company_id", "owner_matrix_user_id", "asset_id", "location_id", "name", "company_kind", "status", "revenue_score", "reputation_score", "level", "to_timestamp(created_at_epoch)"],
        "company_id",
        &["owner_matrix_user_id", "asset_id", "location_id", "name", "company_kind", "status", "revenue_score", "reputation_score", "level", "created_at"],
    )?);

    let shops = indexed_sorted(&world.world_shops, &indexes.sorted_shop_indices_by_id);
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_shops",
        &shops,
        "shop_id text, company_id text, owner_matrix_user_id text, location_id text, name text, shop_kind text, status text, listing_count bigint, gross_merchandise_score bigint, created_at_epoch bigint",
        &["shop_id", "company_id", "owner_matrix_user_id", "location_id", "name", "shop_kind", "status", "listing_count", "gross_merchandise_score", "created_at"],
        &["shop_id", "company_id", "owner_matrix_user_id", "location_id", "name", "shop_kind", "status", "listing_count", "gross_merchandise_score", "to_timestamp(created_at_epoch)"],
        "shop_id",
        &["company_id", "owner_matrix_user_id", "location_id", "name", "shop_kind", "status", "listing_count", "gross_merchandise_score", "created_at"],
    )?);

    let listings = indexed_sorted(&world.world_listings, &indexes.sorted_listing_indices_by_id);
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_listings",
        &listings,
        "listing_id text, shop_id text, company_id text, owner_matrix_user_id text, asset_id text, title text, listing_kind text, status text, price_credits bigint, quality_score bigint, created_at_epoch bigint",
        &["listing_id", "shop_id", "company_id", "owner_matrix_user_id", "asset_id", "title", "listing_kind", "status", "price_credits", "quality_score", "created_at"],
        &["listing_id", "shop_id", "company_id", "owner_matrix_user_id", "asset_id", "title", "listing_kind", "status", "price_credits", "quality_score", "to_timestamp(created_at_epoch)"],
        "listing_id",
        &["shop_id", "company_id", "owner_matrix_user_id", "asset_id", "title", "listing_kind", "status", "price_credits", "quality_score", "created_at"],
    )?);

    let economy_events = indexed_sorted(
        &world.world_economy_events,
        &indexes.sorted_economy_event_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_economy_events",
        &economy_events,
        "economy_event_id text, matrix_user_id text, event_kind text, subject_id text, credits_delta bigint, reputation_delta bigint, created_at_epoch bigint",
        &["economy_event_id", "matrix_user_id", "event_kind", "subject_id", "credits_delta", "reputation_delta", "created_at"],
        &["economy_event_id", "matrix_user_id", "event_kind", "subject_id", "credits_delta", "reputation_delta", "to_timestamp(created_at_epoch)"],
        "economy_event_id",
        &["matrix_user_id", "event_kind", "subject_id", "credits_delta", "reputation_delta", "created_at"],
    )?);

    let purchases = indexed_sorted(
        &world.world_purchases,
        &indexes.sorted_purchase_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_purchases",
        &purchases,
        "purchase_id text, listing_id text, shop_id text, company_id text, buyer_matrix_user_id text, seller_matrix_user_id text, price_credits bigint, status text, ledger_status text, ledger_account_id text, ledger_entry_id text, ledger_balance_after double precision, ledger_error text, buyer_ledger_status text, buyer_ledger_account_id text, buyer_ledger_entry_id text, buyer_ledger_balance_after double precision, buyer_ledger_error text, buyer_consume_status text, buyer_consume_entry_id text, buyer_consume_balance_after double precision, buyer_consume_error text, created_at_epoch bigint",
        &["purchase_id", "listing_id", "shop_id", "company_id", "buyer_matrix_user_id", "seller_matrix_user_id", "price_credits", "status", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "buyer_ledger_status", "buyer_ledger_account_id", "buyer_ledger_entry_id", "buyer_ledger_balance_after", "buyer_ledger_error", "buyer_consume_status", "buyer_consume_entry_id", "buyer_consume_balance_after", "buyer_consume_error", "created_at"],
        &["purchase_id", "listing_id", "shop_id", "company_id", "buyer_matrix_user_id", "seller_matrix_user_id", "price_credits", "status", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "buyer_ledger_status", "buyer_ledger_account_id", "buyer_ledger_entry_id", "buyer_ledger_balance_after", "buyer_ledger_error", "buyer_consume_status", "buyer_consume_entry_id", "buyer_consume_balance_after", "buyer_consume_error", "to_timestamp(created_at_epoch)"],
        "purchase_id",
        &["listing_id", "shop_id", "company_id", "buyer_matrix_user_id", "seller_matrix_user_id", "price_credits", "status", "ledger_status", "ledger_account_id", "ledger_entry_id", "ledger_balance_after", "ledger_error", "buyer_ledger_status", "buyer_ledger_account_id", "buyer_ledger_entry_id", "buyer_ledger_balance_after", "buyer_ledger_error", "buyer_consume_status", "buyer_consume_entry_id", "buyer_consume_balance_after", "buyer_consume_error", "created_at"],
    )?);

    let work_orders = indexed_sorted(
        &world.world_work_orders,
        &indexes.sorted_work_order_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_orders",
        &work_orders,
        "work_order_id text, purchase_id text, listing_id text, buyer_matrix_user_id text, seller_matrix_user_id text, company_id text, status text, brief text, value_score bigint, created_at_epoch bigint",
        &["work_order_id", "purchase_id", "listing_id", "buyer_matrix_user_id", "seller_matrix_user_id", "company_id", "status", "brief", "value_score", "created_at"],
        &["work_order_id", "purchase_id", "listing_id", "buyer_matrix_user_id", "seller_matrix_user_id", "company_id", "status", "brief", "value_score", "to_timestamp(created_at_epoch)"],
        "work_order_id",
        &["purchase_id", "listing_id", "buyer_matrix_user_id", "seller_matrix_user_id", "company_id", "status", "brief", "value_score", "created_at"],
    )?);

    let factions: Vec<&WorldFaction> = indexes
        .sorted_faction_ids
        .iter()
        .filter_map(|faction_id| world.world_factions.get(faction_id))
        .collect();
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_factions",
        &factions,
        "faction_id text, zone_id text, name text, faction_kind text, status text, reputation_score bigint",
        &["faction_id", "zone_id", "name", "faction_kind", "status", "reputation_score"],
        &["faction_id", "zone_id", "name", "faction_kind", "status", "reputation_score"],
        "faction_id",
        &["zone_id", "name", "faction_kind", "status", "reputation_score"],
    )?);

    let standings = indexed_sorted(
        &world.world_faction_standings,
        &indexes.sorted_faction_standing_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_faction_standings",
        &standings,
        "standing_id text, matrix_user_id text, faction_id text, reputation_score bigint, rank text, updated_at_epoch bigint",
        &["standing_id", "matrix_user_id", "faction_id", "reputation_score", "rank", "updated_at"],
        &["standing_id", "matrix_user_id", "faction_id", "reputation_score", "rank", "to_timestamp(updated_at_epoch)"],
        "matrix_user_id, faction_id",
        &["standing_id", "reputation_score", "rank", "updated_at"],
    )?);

    let deliveries = indexed_sorted(
        &world.world_work_deliveries,
        &indexes.sorted_work_delivery_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_deliveries",
        &deliveries,
        "delivery_id text, work_order_id text, matrix_user_id text, body text, score double precision, judge_status text, status text, created_at_epoch bigint",
        &["delivery_id", "work_order_id", "matrix_user_id", "body", "score", "judge_status", "status", "created_at"],
        &["delivery_id", "work_order_id", "matrix_user_id", "body", "score", "judge_status", "status", "to_timestamp(created_at_epoch)"],
        "delivery_id",
        &["work_order_id", "matrix_user_id", "body", "score", "judge_status", "status", "created_at"],
    )?);

    let acceptances = indexed_sorted(
        &world.world_work_acceptances,
        &indexes.sorted_work_acceptance_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_acceptances",
        &acceptances,
        "acceptance_id text, work_order_id text, matrix_user_id text, body text, status text, reputation_delta bigint, created_at_epoch bigint",
        &["acceptance_id", "work_order_id", "matrix_user_id", "body", "status", "reputation_delta", "created_at"],
        &["acceptance_id", "work_order_id", "matrix_user_id", "body", "status", "reputation_delta", "to_timestamp(created_at_epoch)"],
        "acceptance_id",
        &["work_order_id", "matrix_user_id", "body", "status", "reputation_delta", "created_at"],
    )?);

    let rejections = indexed_sorted(
        &world.world_work_rejections,
        &indexes.sorted_work_rejection_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_rejections",
        &rejections,
        "rejection_id text, work_order_id text, matrix_user_id text, body text, status text, refund_status text, created_at_epoch bigint",
        &["rejection_id", "work_order_id", "matrix_user_id", "body", "status", "refund_status", "created_at"],
        &["rejection_id", "work_order_id", "matrix_user_id", "body", "status", "refund_status", "to_timestamp(created_at_epoch)"],
        "rejection_id",
        &["work_order_id", "matrix_user_id", "body", "status", "refund_status", "created_at"],
    )?);

    let reopens = indexed_sorted(
        &world.world_work_reopens,
        &indexes.sorted_work_reopen_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_reopens",
        &reopens,
        "reopen_id text, work_order_id text, matrix_user_id text, body text, status text, reserve_status text, created_at_epoch bigint",
        &["reopen_id", "work_order_id", "matrix_user_id", "body", "status", "reserve_status", "created_at"],
        &["reopen_id", "work_order_id", "matrix_user_id", "body", "status", "reserve_status", "to_timestamp(created_at_epoch)"],
        "reopen_id",
        &["work_order_id", "matrix_user_id", "body", "status", "reserve_status", "created_at"],
    )?);

    let cancellations = indexed_sorted(
        &world.world_work_cancellations,
        &indexes.sorted_work_cancellation_indices_by_id,
    );
    sql.push_str(&normalized_shadow_json_upsert_sql(
        "world_work_cancellations",
        &cancellations,
        "cancellation_id text, work_order_id text, matrix_user_id text, body text, status text, refund_status text, created_at_epoch bigint",
        &["cancellation_id", "work_order_id", "matrix_user_id", "body", "status", "refund_status", "created_at"],
        &["cancellation_id", "work_order_id", "matrix_user_id", "body", "status", "refund_status", "to_timestamp(created_at_epoch)"],
        "cancellation_id",
        &["work_order_id", "matrix_user_id", "body", "status", "refund_status", "created_at"],
    )?);

    Ok(sql)
}

pub(super) fn normalized_world_shadow_tables() -> Vec<&'static str> {
    vec![
        "world_zones",
        "world_locations",
        "world_entities",
        "world_map_nodes",
        "world_player_positions",
        "world_trillionnium_characters",
        "world_tactics_sessions",
        "world_tactics_simulation_ticks",
        "world_term_exchange_receipts",
        "world_assets",
        "world_events",
        "world_relationships",
        "world_contracts",
        "world_contract_completions",
        "world_asset_upgrades",
        "world_companies",
        "world_shops",
        "world_listings",
        "world_economy_events",
        "world_purchases",
        "world_work_orders",
        "world_factions",
        "world_faction_standings",
        "world_work_deliveries",
        "world_work_acceptances",
        "world_work_rejections",
        "world_work_reopens",
        "world_work_cancellations",
    ]
}

pub(super) fn normalized_world_shadow_sql_contract_json(generated_sql_bytes: usize) -> Value {
    let tables = normalized_world_shadow_tables();
    json!({
        "contract_version": "trillionnium_normalized_world_shadow_sql_v1",
        "mode": "json_snapshot_to_normalized_world_upserts",
        "index_layer": "WorldIndexes::normalized_shadow_sorted_ids_v1",
        "sorted_vector_index_layer": "WorldIndexes::normalized_shadow_sorted_vector_indices_v1",
        "term_exchange_receipt_tables": [
            "league_term_exchange_receipts",
            "world_term_exchange_receipts"
        ],
        "additional_repository_tables": ["league_term_exchange_receipts"],
        "table_count": tables.len(),
        "tables": tables,
        "generated_sql_bytes": generated_sql_bytes,
    })
}

pub(super) fn league_state_repository_write_set_for_command(command: &str) -> Option<Value> {
    league_state_repository_dual_write_plan_json()
        .get("write_sets")
        .and_then(Value::as_array)
        .and_then(|write_sets| {
            write_sets
                .iter()
                .find(|write_set| write_set.get("command").and_then(Value::as_str) == Some(command))
                .cloned()
        })
}

pub(super) fn normalized_shadow_statement_table(statement: &str) -> Option<&str> {
    let marker = "insert into ";
    let insert_pos = statement.rfind(marker)?;
    let after_insert = &statement[insert_pos + marker.len()..];
    after_insert.split_whitespace().next()
}

pub(super) fn normalized_shadow_sql_for_tables(
    full_sql: &str,
    include_tables: &HashSet<String>,
) -> String {
    let mut sql = String::new();
    for statement in full_sql.split(";\n") {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        let Some(table_name) = normalized_shadow_statement_table(statement) else {
            continue;
        };
        if include_tables.contains(table_name) {
            sql.push_str(statement);
            sql.push_str(";\n");
        }
    }
    sql
}

pub(super) fn add_world_table_dependency(tables: &mut HashSet<String>, table: &str) {
    tables.insert(table.to_string());
}

pub(super) fn normalized_repository_command_world_table_closure(
    declared_tables: &[String],
    normalized_world_tables: &HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let declared_world_tables: HashSet<String> = declared_tables
        .iter()
        .filter(|table| normalized_world_tables.contains(table.as_str()))
        .cloned()
        .collect();
    let mut include_tables = declared_world_tables.clone();

    loop {
        let before_len = include_tables.len();

        if include_tables.contains("world_locations") {
            add_world_table_dependency(&mut include_tables, "world_zones");
        }
        if include_tables.contains("world_entities")
            || include_tables.contains("world_assets")
            || include_tables.contains("world_events")
            || include_tables.contains("world_contracts")
            || include_tables.contains("world_companies")
            || include_tables.contains("world_shops")
            || include_tables.contains("world_map_nodes")
            || include_tables.contains("world_player_positions")
            || include_tables.contains("world_tactics_sessions")
        {
            add_world_table_dependency(&mut include_tables, "world_zones");
            add_world_table_dependency(&mut include_tables, "world_locations");
        }
        if include_tables.contains("world_player_positions") {
            add_world_table_dependency(&mut include_tables, "world_map_nodes");
        }
        if include_tables.contains("world_tactics_sessions") {
            add_world_table_dependency(&mut include_tables, "world_map_nodes");
        }
        if include_tables.contains("world_tactics_simulation_ticks") {
            add_world_table_dependency(&mut include_tables, "world_tactics_sessions");
        }
        if include_tables.contains("world_contracts") {
            add_world_table_dependency(&mut include_tables, "world_events");
        }
        if include_tables.contains("world_contract_completions") {
            add_world_table_dependency(&mut include_tables, "world_contracts");
        }
        if include_tables.contains("world_asset_upgrades") {
            add_world_table_dependency(&mut include_tables, "world_assets");
        }
        if include_tables.contains("world_companies") {
            add_world_table_dependency(&mut include_tables, "world_assets");
        }
        if include_tables.contains("world_shops") {
            add_world_table_dependency(&mut include_tables, "world_companies");
        }
        if include_tables.contains("world_listings") {
            add_world_table_dependency(&mut include_tables, "world_shops");
            add_world_table_dependency(&mut include_tables, "world_companies");
            add_world_table_dependency(&mut include_tables, "world_assets");
        }
        if include_tables.contains("world_purchases") {
            add_world_table_dependency(&mut include_tables, "world_listings");
            add_world_table_dependency(&mut include_tables, "world_shops");
            add_world_table_dependency(&mut include_tables, "world_companies");
        }
        if include_tables.contains("world_work_orders") {
            add_world_table_dependency(&mut include_tables, "world_purchases");
            add_world_table_dependency(&mut include_tables, "world_listings");
            add_world_table_dependency(&mut include_tables, "world_companies");
        }
        if include_tables.contains("world_factions") {
            add_world_table_dependency(&mut include_tables, "world_zones");
        }
        if include_tables.contains("world_faction_standings") {
            add_world_table_dependency(&mut include_tables, "world_factions");
        }
        if include_tables.contains("world_work_deliveries")
            || include_tables.contains("world_work_acceptances")
            || include_tables.contains("world_work_rejections")
            || include_tables.contains("world_work_reopens")
            || include_tables.contains("world_work_cancellations")
        {
            add_world_table_dependency(&mut include_tables, "world_work_orders");
        }

        if include_tables.len() == before_len {
            break;
        }
    }

    let table_order = normalized_world_shadow_tables();
    let ordered_tables: Vec<String> = table_order
        .iter()
        .filter(|table| include_tables.contains(**table))
        .map(|table| (*table).to_string())
        .collect();
    let dependency_tables: Vec<String> = table_order
        .iter()
        .filter(|table| {
            include_tables.contains(**table) && !declared_world_tables.contains(**table)
        })
        .map(|table| (*table).to_string())
        .collect();

    (ordered_tables, dependency_tables)
}

pub(super) fn normalized_repository_command_shadow_sql_from_full(
    full_sql: &str,
    command: &str,
) -> Result<Option<String>, serde_json::Error> {
    let Some(write_set) = league_state_repository_write_set_for_command(command) else {
        return Ok(None);
    };
    let declared_tables: Vec<String> = write_set
        .get("tables")
        .and_then(Value::as_array)
        .map(|tables| {
            tables
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let normalized_world_tables: HashSet<String> = normalized_world_shadow_tables()
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let write_set_world_tables: Vec<String> = declared_tables
        .iter()
        .filter(|table| normalized_world_tables.contains(table.as_str()))
        .cloned()
        .collect();
    let non_world_tables: Vec<String> = declared_tables
        .iter()
        .filter(|table| !normalized_world_tables.contains(table.as_str()))
        .cloned()
        .collect();
    let (world_tables, dependency_world_tables) = normalized_repository_command_world_table_closure(
        &declared_tables,
        &normalized_world_tables,
    );
    let include_tables: HashSet<String> = world_tables.iter().cloned().collect();
    let command_sql = normalized_shadow_sql_for_tables(full_sql, &include_tables);
    let contract = json!({
        "contract_version": "trillionnium_normalized_repository_command_shadow_sql_v1",
        "mode": "command_scoped_world_table_upserts_with_fk_dependency_closure",
        "command": command,
        "world_tables": world_tables,
        "write_set_world_tables": write_set_world_tables,
        "dependency_world_tables": dependency_world_tables,
        "non_world_tables": non_world_tables,
        "source_write_set": write_set,
    });
    let contract_json = serde_json::to_string(&contract)?;
    Ok(Some(format!(
        "-- Normalized repository command shadow SQL.\n\
         -- Contract: {contract_json}\n\
         {command_sql}"
    )))
}

#[allow(dead_code)]
pub(super) fn normalized_repository_command_shadow_sql(
    world: &WorldState,
    command: &str,
) -> Result<Option<String>, serde_json::Error> {
    let full_sql = world_state_normalized_shadow_sql(world)?;
    normalized_repository_command_shadow_sql_from_full(&full_sql, command)
}

pub(super) fn normalized_repository_direct_write_supported_commands() -> Vec<&'static str> {
    vec![
        "world_action",
        "world_contract_completion",
        "world_map_move",
        "world_tactics_command",
        "world_asset_upgrade",
        "world_company",
        "world_listing",
        "world_buy",
        "world_work_deliver",
        "world_work_accept",
        "world_work_reject",
        "world_work_reopen",
        "world_work_cancel",
    ]
}

pub(super) fn normalized_repository_direct_write_supports_command(command: &str) -> bool {
    matches!(
        command,
        "world_action"
            | "world_contract_completion"
            | "world_map_move"
            | "world_tactics_command"
            | "world_asset_upgrade"
            | "world_company"
            | "world_listing"
            | "world_buy"
            | "world_work_deliver"
            | "world_work_accept"
            | "world_work_reject"
            | "world_work_reopen"
            | "world_work_cancel"
    )
}

pub(super) fn normalized_repository_direct_write_contract_json() -> Value {
    json!({
        "contract_version": "trillionnium_normalized_repository_direct_write_v1",
        "phase": "direct_write_final_cutover",
        "purpose": "make explicit typed SQLx upsert helpers the primary normalized SQL write path for supported world commands while preserving JSON/snapshot only as export, audit, and rollback artifacts",
        "runtime_helper": "execute_normalized_repository_direct_command_write",
        "transaction_mode": "single_pg_transaction_direct_sql_primary_plus_snapshot_export",
        "index_reuse": "execute_normalized_repository_direct_command_write builds one WorldIndexes snapshot per hydrated repository state and reuses sorted vector indices across typed SQLx upserts",
        "supported_commands": normalized_repository_direct_write_supported_commands(),
        "fallback_helper": "snapshot_export_only",
        "fallback_mode": "unsupported world commands are rejected in final cutover instead of falling back to generated command-scoped SQL; non-world snapshot exports remain rollback/audit-only",
        "bridge": "league_state_snapshots, league_state_repository_snapshots, and league_state_repository_write_set_audits remain written only for rollback, parity, audit, and read-switch recovery gates",
        "transaction_boundary": "write_normalized_repository_snapshot_to_database commits direct typed SQLx upserts first, then writes JSON/snapshot export and audit artifacts, atomically in one PostgreSQL transaction",
        "command_helpers": [
            {
                "command": "world_action",
                "direct_tables": ["world_events", "world_contracts", "world_assets", "world_relationships"],
                "dependency_tables": ["world_zones", "world_locations"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_contract_completion",
                "direct_tables": ["world_contract_completions", "world_contracts", "world_assets", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_events"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_map_move",
                "direct_tables": ["world_player_positions", "world_economy_events", "world_trillionnium_characters"],
                "dependency_tables": ["world_zones", "world_locations", "world_map_nodes"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot",
                "runtime_mutation_contract": "trillionnium_world_resource_pressure_runtime_v1"
            },
            {
                "command": "world_tactics_command",
                "direct_tables": ["world_trillionnium_characters", "world_tactics_sessions", "world_tactics_simulation_ticks", "world_events", "world_relationships", "world_contracts", "world_contract_completions", "world_economy_events", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_map_nodes"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot",
                "storage_boundary_decision": "tw_4_8_tactics_json_snapshot_plus_normalized_shadow_tables"
            },
            {
                "command": "world_asset_upgrade",
                "direct_tables": ["world_asset_upgrades", "world_assets", "league_players"],
                "dependency_tables": ["world_zones", "world_locations"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_company",
                "direct_tables": ["world_companies", "world_shops", "world_listings", "world_relationships", "world_economy_events", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_listing",
                "direct_tables": ["world_listings", "world_shops", "world_companies", "world_economy_events", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_buy",
                "direct_tables": ["world_purchases", "world_work_orders", "world_relationships", "world_economy_events", "world_faction_standings", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_companies", "world_shops", "world_listings", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_work_deliver",
                "direct_tables": ["world_work_deliveries", "world_work_orders", "world_companies", "world_economy_events", "world_faction_standings", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_shops", "world_listings", "world_purchases", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_work_accept",
                "direct_tables": ["world_work_acceptances", "world_work_orders", "world_purchases", "world_companies", "world_economy_events", "world_faction_standings", "league_players"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_shops", "world_listings", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_work_reject",
                "direct_tables": ["world_work_rejections", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_companies", "world_shops", "world_listings", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_work_reopen",
                "direct_tables": ["world_work_reopens", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_companies", "world_shops", "world_listings", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            },
            {
                "command": "world_work_cancel",
                "direct_tables": ["world_work_cancellations", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings"],
                "dependency_tables": ["world_zones", "world_locations", "world_assets", "world_companies", "world_shops", "world_listings", "world_factions"],
                "helper_mode": "typed_sqlx_upsert_from_repository_snapshot"
            }
        ]
    })
}

pub(super) async fn upsert_normalized_league_players(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    league: &LeagueState,
) -> Result<usize, String> {
    let mut matrix_user_ids: Vec<&String> = league.players_by_matrix_user.keys().collect();
    matrix_user_ids.sort();
    let mut rows = 0;
    for matrix_user_id in matrix_user_ids {
        let Some(player) = league.players_by_matrix_user.get(matrix_user_id) else {
            continue;
        };
        sqlx::query(
            "insert into league_players (
                 matrix_user_id, display_name, class_tag, rank_tier, rating,
                 xp, reputation, battles, submissions, wins, earned_credits, created_at, updated_at
             ) values (
                 $1, $2, $3, $4, $5::integer,
                 $6::integer, $7::integer, $8::integer, $9::integer, $10::integer, $11::numeric,
                 to_timestamp($12::double precision), now()
             ) on conflict (matrix_user_id) do update set
                 display_name = excluded.display_name,
                 class_tag = excluded.class_tag,
                 rank_tier = excluded.rank_tier,
                 rating = excluded.rating,
                 xp = excluded.xp,
                 reputation = excluded.reputation,
                 battles = excluded.battles,
                 submissions = excluded.submissions,
                 wins = excluded.wins,
                 earned_credits = excluded.earned_credits,
                 updated_at = now()",
        )
        .bind(&player.matrix_user_id)
        .bind(&player.display_name)
        .bind(&player.class_tag)
        .bind(&player.rank_tier)
        .bind(player.rating)
        .bind(player.xp)
        .bind(player.reputation)
        .bind(player.battles)
        .bind(player.submissions)
        .bind(player.wins)
        .bind(player.earned_credits)
        .bind(player.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert league_players: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_zones(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for zone_id in &indexes.sorted_zone_ids {
        let Some(zone) = world.world_zones.get(zone_id) else {
            continue;
        };
        sqlx::query(
            "insert into world_zones (zone_id, name, status, theme, mirror_kind)
             values ($1, $2, $3, $4, $5)
             on conflict (zone_id) do update set
                 name = excluded.name,
                 status = excluded.status,
                 theme = excluded.theme,
                 mirror_kind = excluded.mirror_kind",
        )
        .bind(&zone.zone_id)
        .bind(&zone.name)
        .bind(&zone.status)
        .bind(&zone.theme)
        .bind(&zone.mirror_kind)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_zones: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_locations(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for location_id in &indexes.sorted_location_ids {
        let Some(location) = world.world_locations.get(location_id) else {
            continue;
        };
        sqlx::query(
            "insert into world_locations (location_id, zone_id, name, location_kind, description, status)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (location_id) do update set
                 zone_id = excluded.zone_id,
                 name = excluded.name,
                 location_kind = excluded.location_kind,
                 description = excluded.description,
                 status = excluded.status",
        )
        .bind(&location.location_id)
        .bind(&location.zone_id)
        .bind(&location.name)
        .bind(&location.location_kind)
        .bind(&location.description)
        .bind(&location.status)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_locations: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_map_nodes(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for node_id in &indexes.sorted_map_node_ids_by_id {
        let Some(node) = world.world_map_nodes.get(node_id) else {
            continue;
        };
        let exits = serde_json::to_string(&node.exits)
            .map_err(|err| format!("failed to serialize world_map_nodes.exits: {err}"))?;
        let interaction_tags = serde_json::to_string(&node.interaction_tags).map_err(|err| {
            format!("failed to serialize world_map_nodes.interaction_tags: {err}")
        })?;
        let freedom_hooks = serde_json::to_string(&node.freedom_hooks)
            .map_err(|err| format!("failed to serialize world_map_nodes.freedom_hooks: {err}"))?;
        sqlx::query(
            "insert into world_map_nodes (
                 node_id, location_id, zone_id, name, node_kind, description,
                 x, y, exits, interaction_tags, freedom_hooks, status
             ) values (
                 $1, $2, $3, $4, $5, $6,
                 $7::integer, $8::integer, $9::jsonb, $10::jsonb, $11::jsonb, $12
             ) on conflict (node_id) do update set
                 location_id = excluded.location_id,
                 zone_id = excluded.zone_id,
                 name = excluded.name,
                 node_kind = excluded.node_kind,
                 description = excluded.description,
                 x = excluded.x,
                 y = excluded.y,
                 exits = excluded.exits,
                 interaction_tags = excluded.interaction_tags,
                 freedom_hooks = excluded.freedom_hooks,
                 status = excluded.status",
        )
        .bind(&node.node_id)
        .bind(&node.location_id)
        .bind(&node.zone_id)
        .bind(&node.name)
        .bind(&node.node_kind)
        .bind(&node.description)
        .bind(node.x)
        .bind(node.y)
        .bind(exits)
        .bind(interaction_tags)
        .bind(freedom_hooks)
        .bind(&node.status)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_map_nodes: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_player_positions(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for matrix_user_id in &indexes.sorted_player_position_user_ids {
        let Some(position) = world.world_player_positions.get(matrix_user_id) else {
            continue;
        };
        sqlx::query(
            "insert into world_player_positions (matrix_user_id, node_id, location_id, updated_at)
             values ($1, $2, $3, to_timestamp($4::double precision))
             on conflict (matrix_user_id) do update set
                 node_id = excluded.node_id,
                 location_id = excluded.location_id,
                 updated_at = excluded.updated_at",
        )
        .bind(&position.matrix_user_id)
        .bind(&position.node_id)
        .bind(&position.location_id)
        .bind(position.updated_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_player_positions: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_trillionnium_characters(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for matrix_user_id in &indexes.sorted_trillionnium_character_user_ids {
        let Some(character) = world.world_trillionnium_characters.get(matrix_user_id) else {
            continue;
        };
        let attributes = serde_json::to_string(&character.attributes).map_err(|err| {
            format!("failed to serialize world_trillionnium_characters.attributes: {err}")
        })?;
        let skill_ids = serde_json::to_string(&character.skill_ids).map_err(|err| {
            format!("failed to serialize world_trillionnium_characters.skill_ids: {err}")
        })?;
        let inventory_items = serde_json::to_string(&character.inventory_items).map_err(|err| {
            format!("failed to serialize world_trillionnium_characters.inventory_items: {err}")
        })?;
        let equipment_slots = serde_json::to_string(&character.equipment_slots).map_err(|err| {
            format!("failed to serialize world_trillionnium_characters.equipment_slots: {err}")
        })?;
        let resource_pressure_state = serde_json::to_string(&character.resource_pressure_state)
            .map_err(|err| {
                format!(
                    "failed to serialize world_trillionnium_characters.resource_pressure_state: {err}"
                )
            })?;
        let region_story_unlock_state = serde_json::to_string(&character.region_story_unlock_state)
            .map_err(|err| {
                format!(
                    "failed to serialize world_trillionnium_characters.region_story_unlock_state: {err}"
                )
            })?;
        let combat_numerics_state = serde_json::to_string(&character.combat_numerics_state)
            .map_err(|err| {
                format!(
                    "failed to serialize world_trillionnium_characters.combat_numerics_state: {err}"
                )
            })?;
        sqlx::query(
            "insert into world_trillionnium_characters (
                 matrix_user_id, character_id, display_name, attributes, sect_id,
                 title, skill_ids, inventory_items, equipment_slots, resource_pressure_state, region_story_unlock_state, combat_numerics_state, updated_at
             ) values (
                 $1, $2, $3, $4::jsonb, $5,
                 $6, $7::jsonb, $8::jsonb, $9::jsonb, $10::jsonb, $11::jsonb, $12::jsonb, to_timestamp($13::double precision)
             ) on conflict (matrix_user_id) do update set
                 character_id = excluded.character_id,
                 display_name = excluded.display_name,
                 attributes = excluded.attributes,
                 sect_id = excluded.sect_id,
                 title = excluded.title,
                 skill_ids = excluded.skill_ids,
                 inventory_items = excluded.inventory_items,
                 equipment_slots = excluded.equipment_slots,
                 resource_pressure_state = excluded.resource_pressure_state,
                 region_story_unlock_state = excluded.region_story_unlock_state,
                 combat_numerics_state = excluded.combat_numerics_state,
                 updated_at = excluded.updated_at",
        )
        .bind(&character.matrix_user_id)
        .bind(&character.character_id)
        .bind(&character.display_name)
        .bind(attributes)
        .bind(&character.sect_id)
        .bind(&character.title)
        .bind(skill_ids)
        .bind(inventory_items)
        .bind(equipment_slots)
        .bind(resource_pressure_state)
        .bind(region_story_unlock_state)
        .bind(combat_numerics_state)
        .bind(character.updated_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_trillionnium_characters: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_tactics_sessions(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for session_id in &indexes.sorted_tactics_session_ids {
        let Some(session) = world.world_tactics_sessions.get(session_id) else {
            continue;
        };
        sqlx::query(
            "insert into world_tactics_sessions (
                 session_id, matrix_user_id, room_id, board_id, active_node_id,
                 active_overlay_id, active_unit_id, active_side, status, round,
                 action_points_remaining, current_tick, objective_id, objective_progress,
                 objective_goal, victory_state, reward_status, reward_event_id,
                 reward_credits_awarded, reward_xp_awarded, created_at, updated_at,
                 source_of_truth, persistence_owner
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8, $9, $10::integer,
                 $11::integer, $12::integer, $13, $14::integer,
                 $15::integer, $16, $17, $18,
                 $19::integer, $20::integer, to_timestamp($21::double precision), to_timestamp($22::double precision),
                 $23, $24
             ) on conflict (session_id) do update set
                 matrix_user_id = excluded.matrix_user_id,
                 room_id = excluded.room_id,
                 board_id = excluded.board_id,
                 active_node_id = excluded.active_node_id,
                 active_overlay_id = excluded.active_overlay_id,
                 active_unit_id = excluded.active_unit_id,
                 active_side = excluded.active_side,
                 status = excluded.status,
                 round = excluded.round,
                 action_points_remaining = excluded.action_points_remaining,
                 current_tick = excluded.current_tick,
                 objective_id = excluded.objective_id,
                 objective_progress = excluded.objective_progress,
                 objective_goal = excluded.objective_goal,
                 victory_state = excluded.victory_state,
                 reward_status = excluded.reward_status,
                 reward_event_id = excluded.reward_event_id,
                 reward_credits_awarded = excluded.reward_credits_awarded,
                 reward_xp_awarded = excluded.reward_xp_awarded,
                 created_at = excluded.created_at,
                 updated_at = excluded.updated_at,
                 source_of_truth = excluded.source_of_truth,
                 persistence_owner = excluded.persistence_owner",
        )
        .bind(&session.session_id)
        .bind(&session.matrix_user_id)
        .bind(&session.room_id)
        .bind(&session.board_id)
        .bind(&session.active_node_id)
        .bind(&session.active_overlay_id)
        .bind(&session.active_unit_id)
        .bind(&session.active_side)
        .bind(&session.status)
        .bind(session.round)
        .bind(session.action_points_remaining)
        .bind(session.current_tick)
        .bind(&session.objective_id)
        .bind(session.objective_progress)
        .bind(session.objective_goal)
        .bind(&session.victory_state)
        .bind(&session.reward_status)
        .bind(&session.reward_event_id)
        .bind(session.reward_credits_awarded)
        .bind(session.reward_xp_awarded)
        .bind(session.created_at_epoch as f64)
        .bind(session.updated_at_epoch as f64)
        .bind(&session.source_of_truth)
        .bind(&session.persistence_owner)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_tactics_sessions: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_tactics_simulation_ticks(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let ticks = indexed_sorted(
        &world.world_tactics_simulation_ticks,
        &indexes.sorted_tactics_simulation_tick_indices_by_id,
    );
    let mut rows = 0;
    for tick in ticks {
        sqlx::query(
            "insert into world_tactics_simulation_ticks (
                 tick_id, session_id, matrix_user_id, room_id, tick_index,
                 command, unit_id, target_tile, outcome_result, outcome_accepted,
                 simulation_effect, round_before, round_after, action_points_before,
                 action_points_after, objective_id, objective_progress_before,
                 objective_progress_after, objective_delta, victory_state_before,
                 victory_state_after, reward_status_after, active_unit_after,
                 generated_encounter_id, osm_game_overlay_id, created_at, source_of_truth
             ) values (
                 $1, $2, $3, $4, $5::integer,
                 $6, $7, $8, $9, $10,
                 $11, $12::integer, $13::integer, $14::integer,
                 $15::integer, $16, $17::integer,
                 $18::integer, $19::integer, $20,
                 $21, $22, $23,
                 $24, $25, to_timestamp($26::double precision), $27
             ) on conflict (tick_id) do update set
                 session_id = excluded.session_id,
                 matrix_user_id = excluded.matrix_user_id,
                 room_id = excluded.room_id,
                 tick_index = excluded.tick_index,
                 command = excluded.command,
                 unit_id = excluded.unit_id,
                 target_tile = excluded.target_tile,
                 outcome_result = excluded.outcome_result,
                 outcome_accepted = excluded.outcome_accepted,
                 simulation_effect = excluded.simulation_effect,
                 round_before = excluded.round_before,
                 round_after = excluded.round_after,
                 action_points_before = excluded.action_points_before,
                 action_points_after = excluded.action_points_after,
                 objective_id = excluded.objective_id,
                 objective_progress_before = excluded.objective_progress_before,
                 objective_progress_after = excluded.objective_progress_after,
                 objective_delta = excluded.objective_delta,
                 victory_state_before = excluded.victory_state_before,
                 victory_state_after = excluded.victory_state_after,
                 reward_status_after = excluded.reward_status_after,
                 active_unit_after = excluded.active_unit_after,
                 generated_encounter_id = excluded.generated_encounter_id,
                 osm_game_overlay_id = excluded.osm_game_overlay_id,
                 created_at = excluded.created_at,
                 source_of_truth = excluded.source_of_truth",
        )
        .bind(&tick.tick_id)
        .bind(&tick.session_id)
        .bind(&tick.matrix_user_id)
        .bind(&tick.room_id)
        .bind(tick.tick_index)
        .bind(&tick.command)
        .bind(&tick.unit_id)
        .bind(&tick.target_tile)
        .bind(&tick.outcome_result)
        .bind(tick.outcome_accepted)
        .bind(&tick.simulation_effect)
        .bind(tick.round_before)
        .bind(tick.round_after)
        .bind(tick.action_points_before)
        .bind(tick.action_points_after)
        .bind(&tick.objective_id)
        .bind(tick.objective_progress_before)
        .bind(tick.objective_progress_after)
        .bind(tick.objective_delta)
        .bind(&tick.victory_state_before)
        .bind(&tick.victory_state_after)
        .bind(&tick.reward_status_after)
        .bind(&tick.active_unit_after)
        .bind(&tick.generated_encounter_id)
        .bind(&tick.osm_game_overlay_id)
        .bind(tick.created_at_epoch as f64)
        .bind(&tick.source_of_truth)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_tactics_simulation_ticks: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_events(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let events = indexed_sorted(&world.world_events, &indexes.sorted_event_indices_by_id);
    let mut rows = 0;
    for event in events {
        sqlx::query(
            "insert into world_events (
                 event_id, actor_matrix_user_id, room_id, location_id, event_kind,
                 body, result, impact_score, cex_task_id, cex_status, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8::integer, $9, $10, to_timestamp($11::double precision)
             ) on conflict (event_id) do update set
                 actor_matrix_user_id = excluded.actor_matrix_user_id,
                 room_id = excluded.room_id,
                 location_id = excluded.location_id,
                 event_kind = excluded.event_kind,
                 body = excluded.body,
                 result = excluded.result,
                 impact_score = excluded.impact_score,
                 cex_task_id = excluded.cex_task_id,
                 cex_status = excluded.cex_status,
                 created_at = excluded.created_at",
        )
        .bind(&event.event_id)
        .bind(&event.actor_matrix_user_id)
        .bind(event.room_id.as_deref())
        .bind(&event.location_id)
        .bind(&event.event_kind)
        .bind(&event.body)
        .bind(&event.result)
        .bind(event.impact_score)
        .bind(event.cex_task_id.as_deref())
        .bind(event.cex_status.as_deref())
        .bind(event.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_events: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_relationships(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let relationships = indexed_sorted(
        &world.world_relationships,
        &indexes.sorted_relationship_indices_by_id,
    );
    let mut rows = 0;
    for relationship in relationships {
        sqlx::query(
            "insert into world_relationships (
                 relationship_id, from_id, to_id, relation_kind, strength, updated_at
             ) values (
                 $1, $2, $3, $4, $5::integer, to_timestamp($6::double precision)
             ) on conflict (relationship_id) do update set
                 from_id = excluded.from_id,
                 to_id = excluded.to_id,
                 relation_kind = excluded.relation_kind,
                 strength = excluded.strength,
                 updated_at = excluded.updated_at",
        )
        .bind(&relationship.relationship_id)
        .bind(&relationship.from_id)
        .bind(&relationship.to_id)
        .bind(&relationship.relation_kind)
        .bind(relationship.strength)
        .bind(relationship.updated_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_relationships: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_contracts(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let contracts = indexed_sorted(
        &world.world_contracts,
        &indexes.sorted_contract_indices_by_id,
    );
    let mut rows = 0;
    for contract in contracts {
        sqlx::query(
            "insert into world_contracts (
                 contract_id, event_id, actor_matrix_user_id, location_id, task_id,
                 title, body, status, cex_status, value_score, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8, $9, $10::integer, to_timestamp($11::double precision)
             ) on conflict (contract_id) do update set
                 event_id = excluded.event_id,
                 actor_matrix_user_id = excluded.actor_matrix_user_id,
                 location_id = excluded.location_id,
                 task_id = excluded.task_id,
                 title = excluded.title,
                 body = excluded.body,
                 status = excluded.status,
                 cex_status = excluded.cex_status,
                 value_score = excluded.value_score,
                 created_at = excluded.created_at",
        )
        .bind(&contract.contract_id)
        .bind(&contract.event_id)
        .bind(&contract.actor_matrix_user_id)
        .bind(&contract.location_id)
        .bind(&contract.task_id)
        .bind(&contract.title)
        .bind(&contract.body)
        .bind(&contract.status)
        .bind(contract.cex_status.as_deref())
        .bind(contract.value_score)
        .bind(contract.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_contracts: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_contract_completions(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let completions = indexed_sorted(
        &world.world_contract_completions,
        &indexes.sorted_contract_completion_indices_by_id,
    );
    let mut rows = 0;
    for completion in completions {
        let anti_cheat_flags =
            serde_json::to_string(&completion.anti_cheat_flags).map_err(|err| {
                format!("failed to serialize world_contract_completions.anti_cheat_flags: {err}")
            })?;
        let score_events = serde_json::to_string(&completion.score_events).map_err(|err| {
            format!("failed to serialize world_contract_completions.score_events: {err}")
        })?;
        sqlx::query(
            "insert into world_contract_completions (
                 completion_id, contract_id, matrix_user_id, body,
                 score, grade, reward_amount, judge_status, payout_status,
                 anti_cheat_flags, score_events, ledger_status, ledger_account_id,
                 ledger_entry_id, ledger_balance_after, ledger_error, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5::numeric, $6, $7::numeric, $8, $9,
                 $10::jsonb, $11::jsonb, $12, $13,
                 $14, $15::numeric, $16, to_timestamp($17::double precision)
             ) on conflict (completion_id) do update set
                 contract_id = excluded.contract_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 score = excluded.score,
                 grade = excluded.grade,
                 reward_amount = excluded.reward_amount,
                 judge_status = excluded.judge_status,
                 payout_status = excluded.payout_status,
                 anti_cheat_flags = excluded.anti_cheat_flags,
                 score_events = excluded.score_events,
                 ledger_status = excluded.ledger_status,
                 ledger_account_id = excluded.ledger_account_id,
                 ledger_entry_id = excluded.ledger_entry_id,
                 ledger_balance_after = excluded.ledger_balance_after,
                 ledger_error = excluded.ledger_error,
                 created_at = excluded.created_at",
        )
        .bind(&completion.completion_id)
        .bind(&completion.contract_id)
        .bind(&completion.matrix_user_id)
        .bind(&completion.body)
        .bind(completion.score)
        .bind(&completion.grade)
        .bind(completion.reward_amount)
        .bind(&completion.judge_status)
        .bind(&completion.payout_status)
        .bind(anti_cheat_flags)
        .bind(score_events)
        .bind(completion.ledger_status.as_deref())
        .bind(completion.ledger_account_id.as_deref())
        .bind(completion.ledger_entry_id.as_deref())
        .bind(completion.ledger_balance_after)
        .bind(completion.ledger_error.as_deref())
        .bind(completion.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_contract_completions: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_assets(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let assets = indexed_sorted(&world.world_assets, &indexes.sorted_asset_indices_by_id);
    let mut rows = 0;
    for asset in assets {
        sqlx::query(
            "insert into world_assets (
                 asset_id, owner_matrix_user_id, location_id, asset_kind, name,
                 status, value_score, upgrade_level, upgrade_points, last_upgrade_kind, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7::integer, $8::integer, $9::integer, $10, to_timestamp($11::double precision)
             ) on conflict (asset_id) do update set
                 owner_matrix_user_id = excluded.owner_matrix_user_id,
                 location_id = excluded.location_id,
                 asset_kind = excluded.asset_kind,
                 name = excluded.name,
                 status = excluded.status,
                 value_score = excluded.value_score,
                 upgrade_level = excluded.upgrade_level,
                 upgrade_points = excluded.upgrade_points,
                 last_upgrade_kind = excluded.last_upgrade_kind,
                 created_at = excluded.created_at",
        )
        .bind(&asset.asset_id)
        .bind(&asset.owner_matrix_user_id)
        .bind(&asset.location_id)
        .bind(&asset.asset_kind)
        .bind(&asset.name)
        .bind(&asset.status)
        .bind(asset.value_score)
        .bind(asset.upgrade_level)
        .bind(asset.upgrade_points)
        .bind(asset.last_upgrade_kind.as_deref())
        .bind(asset.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_assets: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_asset_upgrades(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let upgrades = indexed_sorted(
        &world.world_asset_upgrades,
        &indexes.sorted_asset_upgrade_indices_by_id,
    );
    let mut rows = 0;
    for upgrade in upgrades {
        sqlx::query(
            "insert into world_asset_upgrades (
                 upgrade_id, asset_id, matrix_user_id, body, upgrade_kind,
                 score, grade, judge_status, status, value_delta,
                 level_before, level_after, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6::numeric, $7, $8, $9, $10::integer,
                 $11::integer, $12::integer, to_timestamp($13::double precision)
             ) on conflict (upgrade_id) do update set
                 asset_id = excluded.asset_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 upgrade_kind = excluded.upgrade_kind,
                 score = excluded.score,
                 grade = excluded.grade,
                 judge_status = excluded.judge_status,
                 status = excluded.status,
                 value_delta = excluded.value_delta,
                 level_before = excluded.level_before,
                 level_after = excluded.level_after,
                 created_at = excluded.created_at",
        )
        .bind(&upgrade.upgrade_id)
        .bind(&upgrade.asset_id)
        .bind(&upgrade.matrix_user_id)
        .bind(&upgrade.body)
        .bind(&upgrade.upgrade_kind)
        .bind(upgrade.score)
        .bind(&upgrade.grade)
        .bind(&upgrade.judge_status)
        .bind(&upgrade.status)
        .bind(upgrade.value_delta)
        .bind(upgrade.level_before)
        .bind(upgrade.level_after)
        .bind(upgrade.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_asset_upgrades: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_companies(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let companies = indexed_sorted(
        &world.world_companies,
        &indexes.sorted_company_indices_by_id,
    );
    let mut rows = 0;
    for company in companies {
        sqlx::query(
            "insert into world_companies (
                 company_id, owner_matrix_user_id, asset_id, location_id, name,
                 company_kind, status, revenue_score, reputation_score, level, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8::integer, $9::integer, $10::integer, to_timestamp($11::double precision)
             ) on conflict (company_id) do update set
                 owner_matrix_user_id = excluded.owner_matrix_user_id,
                 asset_id = excluded.asset_id,
                 location_id = excluded.location_id,
                 name = excluded.name,
                 company_kind = excluded.company_kind,
                 status = excluded.status,
                 revenue_score = excluded.revenue_score,
                 reputation_score = excluded.reputation_score,
                 level = excluded.level,
                 created_at = excluded.created_at",
        )
        .bind(&company.company_id)
        .bind(&company.owner_matrix_user_id)
        .bind(&company.asset_id)
        .bind(&company.location_id)
        .bind(&company.name)
        .bind(&company.company_kind)
        .bind(&company.status)
        .bind(company.revenue_score)
        .bind(company.reputation_score)
        .bind(company.level)
        .bind(company.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_companies: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_shops(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let shops = indexed_sorted(&world.world_shops, &indexes.sorted_shop_indices_by_id);
    let mut rows = 0;
    for shop in shops {
        sqlx::query(
            "insert into world_shops (
                 shop_id, company_id, owner_matrix_user_id, location_id, name,
                 shop_kind, status, listing_count, gross_merchandise_score, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8::integer, $9::integer, to_timestamp($10::double precision)
             ) on conflict (shop_id) do update set
                 company_id = excluded.company_id,
                 owner_matrix_user_id = excluded.owner_matrix_user_id,
                 location_id = excluded.location_id,
                 name = excluded.name,
                 shop_kind = excluded.shop_kind,
                 status = excluded.status,
                 listing_count = excluded.listing_count,
                 gross_merchandise_score = excluded.gross_merchandise_score,
                 created_at = excluded.created_at",
        )
        .bind(&shop.shop_id)
        .bind(&shop.company_id)
        .bind(&shop.owner_matrix_user_id)
        .bind(&shop.location_id)
        .bind(&shop.name)
        .bind(&shop.shop_kind)
        .bind(&shop.status)
        .bind(shop.listing_count)
        .bind(shop.gross_merchandise_score)
        .bind(shop.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_shops: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_listings(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let listings = indexed_sorted(&world.world_listings, &indexes.sorted_listing_indices_by_id);
    let mut rows = 0;
    for listing in listings {
        sqlx::query(
            "insert into world_listings (
                 listing_id, shop_id, company_id, owner_matrix_user_id, asset_id,
                 title, listing_kind, status, price_credits, quality_score, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7, $8, $9::integer, $10::integer, to_timestamp($11::double precision)
             ) on conflict (listing_id) do update set
                 shop_id = excluded.shop_id,
                 company_id = excluded.company_id,
                 owner_matrix_user_id = excluded.owner_matrix_user_id,
                 asset_id = excluded.asset_id,
                 title = excluded.title,
                 listing_kind = excluded.listing_kind,
                 status = excluded.status,
                 price_credits = excluded.price_credits,
                 quality_score = excluded.quality_score,
                 created_at = excluded.created_at",
        )
        .bind(&listing.listing_id)
        .bind(&listing.shop_id)
        .bind(&listing.company_id)
        .bind(&listing.owner_matrix_user_id)
        .bind(&listing.asset_id)
        .bind(&listing.title)
        .bind(&listing.listing_kind)
        .bind(&listing.status)
        .bind(listing.price_credits)
        .bind(listing.quality_score)
        .bind(listing.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_listings: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_purchases(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let purchases = indexed_sorted(
        &world.world_purchases,
        &indexes.sorted_purchase_indices_by_id,
    );
    let mut rows = 0;
    for purchase in purchases {
        sqlx::query(
            "insert into world_purchases (
                 purchase_id, listing_id, shop_id, company_id, buyer_matrix_user_id,
                 seller_matrix_user_id, price_credits, status, ledger_status, ledger_account_id,
                 ledger_entry_id, ledger_balance_after, ledger_error, buyer_ledger_status,
                 buyer_ledger_account_id, buyer_ledger_entry_id, buyer_ledger_balance_after,
                 buyer_ledger_error, buyer_consume_status, buyer_consume_entry_id,
                 buyer_consume_balance_after, buyer_consume_error, created_at
             ) values (
                 $1, $2, $3, $4, $5,
                 $6, $7::integer, $8, $9, $10,
                 $11, $12::double precision, $13, $14,
                 $15, $16, $17::double precision,
                 $18, $19, $20,
                 $21::double precision, $22, to_timestamp($23::double precision)
             ) on conflict (purchase_id) do update set
                 listing_id = excluded.listing_id,
                 shop_id = excluded.shop_id,
                 company_id = excluded.company_id,
                 buyer_matrix_user_id = excluded.buyer_matrix_user_id,
                 seller_matrix_user_id = excluded.seller_matrix_user_id,
                 price_credits = excluded.price_credits,
                 status = excluded.status,
                 ledger_status = excluded.ledger_status,
                 ledger_account_id = excluded.ledger_account_id,
                 ledger_entry_id = excluded.ledger_entry_id,
                 ledger_balance_after = excluded.ledger_balance_after,
                 ledger_error = excluded.ledger_error,
                 buyer_ledger_status = excluded.buyer_ledger_status,
                 buyer_ledger_account_id = excluded.buyer_ledger_account_id,
                 buyer_ledger_entry_id = excluded.buyer_ledger_entry_id,
                 buyer_ledger_balance_after = excluded.buyer_ledger_balance_after,
                 buyer_ledger_error = excluded.buyer_ledger_error,
                 buyer_consume_status = excluded.buyer_consume_status,
                 buyer_consume_entry_id = excluded.buyer_consume_entry_id,
                 buyer_consume_balance_after = excluded.buyer_consume_balance_after,
                 buyer_consume_error = excluded.buyer_consume_error,
                 created_at = excluded.created_at",
        )
        .bind(&purchase.purchase_id)
        .bind(&purchase.listing_id)
        .bind(&purchase.shop_id)
        .bind(&purchase.company_id)
        .bind(&purchase.buyer_matrix_user_id)
        .bind(&purchase.seller_matrix_user_id)
        .bind(purchase.price_credits)
        .bind(&purchase.status)
        .bind(purchase.ledger_status.as_deref())
        .bind(purchase.ledger_account_id.as_deref())
        .bind(purchase.ledger_entry_id.as_deref())
        .bind(purchase.ledger_balance_after)
        .bind(purchase.ledger_error.as_deref())
        .bind(purchase.buyer_ledger_status.as_deref())
        .bind(purchase.buyer_ledger_account_id.as_deref())
        .bind(purchase.buyer_ledger_entry_id.as_deref())
        .bind(purchase.buyer_ledger_balance_after)
        .bind(purchase.buyer_ledger_error.as_deref())
        .bind(purchase.buyer_consume_status.as_deref())
        .bind(purchase.buyer_consume_entry_id.as_deref())
        .bind(purchase.buyer_consume_balance_after)
        .bind(purchase.buyer_consume_error.as_deref())
        .bind(purchase.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_purchases: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_orders(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let work_orders = indexed_sorted(
        &world.world_work_orders,
        &indexes.sorted_work_order_indices_by_id,
    );
    let mut rows = 0;
    for work_order in work_orders {
        sqlx::query(
            "insert into world_work_orders (
                 work_order_id, purchase_id, listing_id, buyer_matrix_user_id,
                 seller_matrix_user_id, company_id, status, brief, value_score, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5, $6, $7, $8, $9::integer, to_timestamp($10::double precision)
             ) on conflict (work_order_id) do update set
                 purchase_id = excluded.purchase_id,
                 listing_id = excluded.listing_id,
                 buyer_matrix_user_id = excluded.buyer_matrix_user_id,
                 seller_matrix_user_id = excluded.seller_matrix_user_id,
                 company_id = excluded.company_id,
                 status = excluded.status,
                 brief = excluded.brief,
                 value_score = excluded.value_score,
                 created_at = excluded.created_at",
        )
        .bind(&work_order.work_order_id)
        .bind(&work_order.purchase_id)
        .bind(&work_order.listing_id)
        .bind(&work_order.buyer_matrix_user_id)
        .bind(&work_order.seller_matrix_user_id)
        .bind(&work_order.company_id)
        .bind(&work_order.status)
        .bind(&work_order.brief)
        .bind(work_order.value_score)
        .bind(work_order.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_orders: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_deliveries(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let deliveries = indexed_sorted(
        &world.world_work_deliveries,
        &indexes.sorted_work_delivery_indices_by_id,
    );
    let mut rows = 0;
    for delivery in deliveries {
        sqlx::query(
            "insert into world_work_deliveries (
                 delivery_id, work_order_id, matrix_user_id, body,
                 score, judge_status, status, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5::double precision, $6, $7, to_timestamp($8::double precision)
             ) on conflict (delivery_id) do update set
                 work_order_id = excluded.work_order_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 score = excluded.score,
                 judge_status = excluded.judge_status,
                 status = excluded.status,
                 created_at = excluded.created_at",
        )
        .bind(&delivery.delivery_id)
        .bind(&delivery.work_order_id)
        .bind(&delivery.matrix_user_id)
        .bind(&delivery.body)
        .bind(delivery.score)
        .bind(&delivery.judge_status)
        .bind(&delivery.status)
        .bind(delivery.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_deliveries: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_acceptances(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let acceptances = indexed_sorted(
        &world.world_work_acceptances,
        &indexes.sorted_work_acceptance_indices_by_id,
    );
    let mut rows = 0;
    for acceptance in acceptances {
        sqlx::query(
            "insert into world_work_acceptances (
                 acceptance_id, work_order_id, matrix_user_id, body,
                 status, reputation_delta, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5, $6::integer, to_timestamp($7::double precision)
             ) on conflict (acceptance_id) do update set
                 work_order_id = excluded.work_order_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 status = excluded.status,
                 reputation_delta = excluded.reputation_delta,
                 created_at = excluded.created_at",
        )
        .bind(&acceptance.acceptance_id)
        .bind(&acceptance.work_order_id)
        .bind(&acceptance.matrix_user_id)
        .bind(&acceptance.body)
        .bind(&acceptance.status)
        .bind(acceptance.reputation_delta)
        .bind(acceptance.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_acceptances: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_rejections(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let rejections = indexed_sorted(
        &world.world_work_rejections,
        &indexes.sorted_work_rejection_indices_by_id,
    );
    let mut rows = 0;
    for rejection in rejections {
        sqlx::query(
            "insert into world_work_rejections (
                 rejection_id, work_order_id, matrix_user_id, body,
                 status, refund_status, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5, $6, to_timestamp($7::double precision)
             ) on conflict (rejection_id) do update set
                 work_order_id = excluded.work_order_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 status = excluded.status,
                 refund_status = excluded.refund_status,
                 created_at = excluded.created_at",
        )
        .bind(&rejection.rejection_id)
        .bind(&rejection.work_order_id)
        .bind(&rejection.matrix_user_id)
        .bind(&rejection.body)
        .bind(&rejection.status)
        .bind(&rejection.refund_status)
        .bind(rejection.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_rejections: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_reopens(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let reopens = indexed_sorted(
        &world.world_work_reopens,
        &indexes.sorted_work_reopen_indices_by_id,
    );
    let mut rows = 0;
    for reopen in reopens {
        sqlx::query(
            "insert into world_work_reopens (
                 reopen_id, work_order_id, matrix_user_id, body,
                 status, reserve_status, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5, $6, to_timestamp($7::double precision)
             ) on conflict (reopen_id) do update set
                 work_order_id = excluded.work_order_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 status = excluded.status,
                 reserve_status = excluded.reserve_status,
                 created_at = excluded.created_at",
        )
        .bind(&reopen.reopen_id)
        .bind(&reopen.work_order_id)
        .bind(&reopen.matrix_user_id)
        .bind(&reopen.body)
        .bind(&reopen.status)
        .bind(&reopen.reserve_status)
        .bind(reopen.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_reopens: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_work_cancellations(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let cancellations = indexed_sorted(
        &world.world_work_cancellations,
        &indexes.sorted_work_cancellation_indices_by_id,
    );
    let mut rows = 0;
    for cancellation in cancellations {
        sqlx::query(
            "insert into world_work_cancellations (
                 cancellation_id, work_order_id, matrix_user_id, body,
                 status, refund_status, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5, $6, to_timestamp($7::double precision)
             ) on conflict (cancellation_id) do update set
                 work_order_id = excluded.work_order_id,
                 matrix_user_id = excluded.matrix_user_id,
                 body = excluded.body,
                 status = excluded.status,
                 refund_status = excluded.refund_status,
                 created_at = excluded.created_at",
        )
        .bind(&cancellation.cancellation_id)
        .bind(&cancellation.work_order_id)
        .bind(&cancellation.matrix_user_id)
        .bind(&cancellation.body)
        .bind(&cancellation.status)
        .bind(&cancellation.refund_status)
        .bind(cancellation.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_work_cancellations: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_factions(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let mut rows = 0;
    for faction_id in &indexes.sorted_faction_ids {
        let Some(faction) = world.world_factions.get(faction_id) else {
            continue;
        };
        sqlx::query(
            "insert into world_factions (
                 faction_id, zone_id, name, faction_kind, status, reputation_score
             ) values (
                 $1, $2, $3, $4, $5, $6::integer
             ) on conflict (faction_id) do update set
                 zone_id = excluded.zone_id,
                 name = excluded.name,
                 faction_kind = excluded.faction_kind,
                 status = excluded.status,
                 reputation_score = excluded.reputation_score",
        )
        .bind(&faction.faction_id)
        .bind(&faction.zone_id)
        .bind(&faction.name)
        .bind(&faction.faction_kind)
        .bind(&faction.status)
        .bind(faction.reputation_score)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_factions: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_faction_standings(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let standings = indexed_sorted(
        &world.world_faction_standings,
        &indexes.sorted_faction_standing_indices_by_id,
    );
    let mut rows = 0;
    for standing in standings {
        sqlx::query(
            "insert into world_faction_standings (
                 standing_id, matrix_user_id, faction_id, reputation_score, rank, updated_at
             ) values (
                 $1, $2, $3, $4::integer, $5, to_timestamp($6::double precision)
             ) on conflict (matrix_user_id, faction_id) do update set
                 standing_id = excluded.standing_id,
                 reputation_score = excluded.reputation_score,
                 rank = excluded.rank,
                 updated_at = excluded.updated_at",
        )
        .bind(&standing.standing_id)
        .bind(&standing.matrix_user_id)
        .bind(&standing.faction_id)
        .bind(standing.reputation_score)
        .bind(&standing.rank)
        .bind(standing.updated_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_faction_standings: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn upsert_normalized_world_economy_events(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    world: &WorldState,
    indexes: &WorldIndexes,
) -> Result<usize, String> {
    let economy_events = indexed_sorted(
        &world.world_economy_events,
        &indexes.sorted_economy_event_indices_by_id,
    );
    let mut rows = 0;
    for economy_event in economy_events {
        sqlx::query(
            "insert into world_economy_events (
                 economy_event_id, matrix_user_id, event_kind, subject_id,
                 credits_delta, reputation_delta, created_at
             ) values (
                 $1, $2, $3, $4,
                 $5::integer, $6::integer, to_timestamp($7::double precision)
             ) on conflict (economy_event_id) do update set
                 matrix_user_id = excluded.matrix_user_id,
                 event_kind = excluded.event_kind,
                 subject_id = excluded.subject_id,
                 credits_delta = excluded.credits_delta,
                 reputation_delta = excluded.reputation_delta,
                 created_at = excluded.created_at",
        )
        .bind(&economy_event.economy_event_id)
        .bind(&economy_event.matrix_user_id)
        .bind(&economy_event.event_kind)
        .bind(&economy_event.subject_id)
        .bind(economy_event.credits_delta)
        .bind(economy_event.reputation_delta)
        .bind(economy_event.created_at_epoch as f64)
        .execute(&mut **conn)
        .await
        .map_err(|err| format!("failed to direct-upsert world_economy_events: {err}"))?;
        rows += 1;
    }
    Ok(rows)
}

pub(super) async fn execute_normalized_repository_direct_command_write(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    repository_snapshot: &LeagueStateRepositorySnapshot,
    command: &str,
) -> Result<Value, String> {
    let league = serde_json::from_str::<LeagueState>(&repository_snapshot.state_json)
        .map_err(|err| format!("failed to hydrate repository snapshot for direct write: {err}"))?;
    let world = &league.world;
    let indexes = build_world_indexes(world);
    match command {
        "world_action" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let event_rows = upsert_normalized_world_events(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let contract_rows = upsert_normalized_world_contracts(conn, world, &indexes).await?;
            let relationship_rows =
                upsert_normalized_world_relationships(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                },
                "direct_rows": {
                    "world_events": event_rows,
                    "world_assets": asset_rows,
                    "world_contracts": contract_rows,
                    "world_relationships": relationship_rows,
                }
            }))
        }
        "world_contract_completion" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let event_rows = upsert_normalized_world_events(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let contract_rows = upsert_normalized_world_contracts(conn, world, &indexes).await?;
            let completion_rows =
                upsert_normalized_world_contract_completions(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_events": event_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_assets": asset_rows,
                    "world_contracts": contract_rows,
                    "world_contract_completions": completion_rows,
                }
            }))
        }
        "world_map_move" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let map_node_rows = upsert_normalized_world_map_nodes(conn, world, &indexes).await?;
            let position_rows =
                upsert_normalized_world_player_positions(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let character_rows =
                upsert_normalized_world_trillionnium_characters(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "runtime_mutation_contract": "trillionnium_world_resource_pressure_runtime_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_map_nodes": map_node_rows,
                },
                "direct_rows": {
                    "world_player_positions": position_rows,
                    "world_economy_events": economy_event_rows,
                    "world_trillionnium_characters": character_rows,
                }
            }))
        }
        "world_tactics_command" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let map_node_rows = upsert_normalized_world_map_nodes(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let character_rows =
                upsert_normalized_world_trillionnium_characters(conn, world, &indexes).await?;
            let tactics_session_rows =
                upsert_normalized_world_tactics_sessions(conn, world, &indexes).await?;
            let tactics_tick_rows =
                upsert_normalized_world_tactics_simulation_ticks(conn, world, &indexes).await?;
            let event_rows = upsert_normalized_world_events(conn, world, &indexes).await?;
            let relationship_rows =
                upsert_normalized_world_relationships(conn, world, &indexes).await?;
            let contract_rows = upsert_normalized_world_contracts(conn, world, &indexes).await?;
            let completion_rows =
                upsert_normalized_world_contract_completions(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "storage_boundary_decision": "tw_4_8_tactics_json_snapshot_plus_normalized_shadow_tables",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_map_nodes": map_node_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_trillionnium_characters": character_rows,
                    "world_tactics_sessions": tactics_session_rows,
                    "world_tactics_simulation_ticks": tactics_tick_rows,
                    "world_events": event_rows,
                    "world_relationships": relationship_rows,
                    "world_contracts": contract_rows,
                    "world_contract_completions": completion_rows,
                    "world_economy_events": economy_event_rows,
                }
            }))
        }
        "world_asset_upgrade" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let asset_upgrade_rows =
                upsert_normalized_world_asset_upgrades(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_assets": asset_rows,
                    "world_asset_upgrades": asset_upgrade_rows,
                }
            }))
        }
        "world_company" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let relationship_rows =
                upsert_normalized_world_relationships(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_relationships": relationship_rows,
                    "world_economy_events": economy_event_rows,
                }
            }))
        }
        "world_listing" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_economy_events": economy_event_rows,
                }
            }))
        }
        "world_buy" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let relationship_rows =
                upsert_normalized_world_relationships(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_purchases": purchase_rows,
                    "world_work_orders": work_order_rows,
                    "world_relationships": relationship_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        "world_work_deliver" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let delivery_rows =
                upsert_normalized_world_work_deliveries(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_purchases": purchase_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_companies": company_rows,
                    "world_work_orders": work_order_rows,
                    "world_work_deliveries": delivery_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        "world_work_accept" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let player_rows = upsert_normalized_league_players(conn, &league).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let acceptance_rows =
                upsert_normalized_world_work_acceptances(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "league_players": player_rows,
                    "world_companies": company_rows,
                    "world_purchases": purchase_rows,
                    "world_work_orders": work_order_rows,
                    "world_work_acceptances": acceptance_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        "world_work_reject" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let rejection_rows =
                upsert_normalized_world_work_rejections(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "world_purchases": purchase_rows,
                    "world_work_orders": work_order_rows,
                    "world_work_rejections": rejection_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        "world_work_reopen" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let reopen_rows = upsert_normalized_world_work_reopens(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "world_purchases": purchase_rows,
                    "world_work_orders": work_order_rows,
                    "world_work_reopens": reopen_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        "world_work_cancel" => {
            let zone_rows = upsert_normalized_world_zones(conn, world, &indexes).await?;
            let location_rows = upsert_normalized_world_locations(conn, world, &indexes).await?;
            let asset_rows = upsert_normalized_world_assets(conn, world, &indexes).await?;
            let company_rows = upsert_normalized_world_companies(conn, world, &indexes).await?;
            let shop_rows = upsert_normalized_world_shops(conn, world, &indexes).await?;
            let listing_rows = upsert_normalized_world_listings(conn, world, &indexes).await?;
            let faction_rows = upsert_normalized_world_factions(conn, world, &indexes).await?;
            let purchase_rows = upsert_normalized_world_purchases(conn, world, &indexes).await?;
            let work_order_rows =
                upsert_normalized_world_work_orders(conn, world, &indexes).await?;
            let cancellation_rows =
                upsert_normalized_world_work_cancellations(conn, world, &indexes).await?;
            let economy_event_rows =
                upsert_normalized_world_economy_events(conn, world, &indexes).await?;
            let faction_standing_rows =
                upsert_normalized_world_faction_standings(conn, world, &indexes).await?;
            Ok(json!({
                "command": command,
                "helper": "execute_normalized_repository_direct_command_write",
                "mode": "typed_sqlx_upsert_from_repository_snapshot",
                "direct_write_contract": "trillionnium_normalized_repository_direct_write_v1",
                "dependency_rows": {
                    "world_zones": zone_rows,
                    "world_locations": location_rows,
                    "world_assets": asset_rows,
                    "world_companies": company_rows,
                    "world_shops": shop_rows,
                    "world_listings": listing_rows,
                    "world_factions": faction_rows,
                },
                "direct_rows": {
                    "world_purchases": purchase_rows,
                    "world_work_orders": work_order_rows,
                    "world_work_cancellations": cancellation_rows,
                    "world_economy_events": economy_event_rows,
                    "world_faction_standings": faction_standing_rows,
                }
            }))
        }
        _ => Err(format!(
            "normalized repository direct write helper is not declared for command={command}"
        )),
    }
}

pub(super) const TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR: &str =
    "0026_add_term_exchange_receipt_tables.sql";
const TRILLIONNIUM_REPOSITORY_FINAL_CUTOVER_PHASE: &str = "final_cutover";
const TRILLIONNIUM_CURRENT_REPOSITORY: &str = "json_file_with_sql_snapshot";
const TRILLIONNIUM_NEXT_REPOSITORY: &str = "normalized_sql_dual_write";
const TRILLIONNIUM_FINAL_REPOSITORY: &str = "normalized_sql_direct_write_final";

pub(super) fn league_state_hash(league: &LeagueState) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(league)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

pub(super) fn league_state_sql_cutover_table_json(
    table_name: &str,
    source_path: &str,
    primary_key: &str,
    row_count: usize,
) -> Value {
    league_state_sql_cutover_table_json_with_normalized_count(
        table_name,
        source_path,
        primary_key,
        row_count,
        row_count,
    )
}

pub(super) fn league_state_sql_cutover_table_json_with_normalized_count(
    table_name: &str,
    source_path: &str,
    primary_key: &str,
    source_row_count: usize,
    normalized_row_count: usize,
) -> Value {
    json!({
        "table_name": table_name,
        "source_path": source_path,
        "primary_key": primary_key,
        "source_row_count": source_row_count,
        "normalized_row_count": normalized_row_count,
        "duplicate_source_rows": source_row_count.saturating_sub(normalized_row_count),
        "row_count": normalized_row_count,
    })
}

pub(super) fn unique_key_count<'a, I>(keys: I) -> usize
where
    I: IntoIterator<Item = &'a str>,
{
    let mut seen = HashSet::new();
    for key in keys {
        seen.insert(key);
    }
    seen.len()
}

pub(super) fn unique_owned_key_count<I>(keys: I) -> usize
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    for key in keys {
        seen.insert(key);
    }
    seen.len()
}

pub(super) fn league_state_sql_cutover_plan_json(
    league: &LeagueState,
    state_hash: &str,
    generated_at: &str,
) -> Value {
    let world = &league.world;
    let tables =
        vec![
            league_state_sql_cutover_table_json(
                "league_players",
                "players_by_matrix_user",
                "matrix_user_id",
                league.players_by_matrix_user.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_matches",
                "matches",
                "match_id",
                league.matches.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_entries",
                "entries",
                "entry_id",
                league.entries.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_battles",
                "battles",
                "battle_id",
                league.battles.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_submissions",
                "submissions",
                "submission_id",
                league.submissions.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_rewards",
                "rewards",
                "reward_id",
                league.rewards.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_guilds",
                "guilds",
                "guild_id",
                league.guilds.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_raid_contributions",
                "raid_contributions",
                "contribution_id",
                league.raid_contributions.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_raid_rosters",
                "raid_rosters",
                "roster_id",
                league.raid_rosters.len(),
            ),
            league_state_sql_cutover_table_json(
                "league_inventory_items",
                "inventory_items",
                "inventory_item_id",
                league.inventory_items.len(),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "league_term_exchange_receipts",
                "term_exchange_receipts",
                "receipt_id",
                league.term_exchange_receipts.len(),
                unique_key_count(
                    league
                        .term_exchange_receipts
                        .keys()
                        .map(|receipt_id| receipt_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json(
                "world_zones",
                "world.world_zones",
                "zone_id",
                world.world_zones.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_locations",
                "world.world_locations",
                "location_id",
                world.world_locations.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_entities",
                "world.world_entities",
                "entity_id",
                world.world_entities.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_map_nodes",
                "world.world_map_nodes",
                "node_id",
                world.world_map_nodes.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_player_positions",
                "world.world_player_positions",
                "matrix_user_id",
                world.world_player_positions.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_trillionnium_characters",
                "world.world_trillionnium_characters",
                "matrix_user_id",
                world.world_trillionnium_characters.len(),
            ),
            league_state_sql_cutover_table_json(
                "world_tactics_sessions",
                "world.world_tactics_sessions",
                "session_id",
                world.world_tactics_sessions.len(),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_tactics_simulation_ticks",
                "world.world_tactics_simulation_ticks",
                "tick_id",
                world.world_tactics_simulation_ticks.len(),
                unique_key_count(
                    world
                        .world_tactics_simulation_ticks
                        .iter()
                        .map(|tick| tick.tick_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_term_exchange_receipts",
                "world.world_term_exchange_receipts",
                "receipt_id",
                world.world_term_exchange_receipts.len(),
                unique_key_count(
                    world
                        .world_term_exchange_receipts
                        .keys()
                        .map(|receipt_id| receipt_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_assets",
                "world.world_assets",
                "asset_id",
                world.world_assets.len(),
                unique_key_count(
                    world
                        .world_assets
                        .iter()
                        .map(|asset| asset.asset_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_asset_upgrades",
                "world.world_asset_upgrades",
                "upgrade_id",
                world.world_asset_upgrades.len(),
                unique_key_count(
                    world
                        .world_asset_upgrades
                        .iter()
                        .map(|upgrade| upgrade.upgrade_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_companies",
                "world.world_companies",
                "company_id",
                world.world_companies.len(),
                unique_key_count(
                    world
                        .world_companies
                        .iter()
                        .map(|company| company.company_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_shops",
                "world.world_shops",
                "shop_id",
                world.world_shops.len(),
                unique_key_count(world.world_shops.iter().map(|shop| shop.shop_id.as_str())),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_listings",
                "world.world_listings",
                "listing_id",
                world.world_listings.len(),
                unique_key_count(
                    world
                        .world_listings
                        .iter()
                        .map(|listing| listing.listing_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_economy_events",
                "world.world_economy_events",
                "economy_event_id",
                world.world_economy_events.len(),
                unique_key_count(
                    world
                        .world_economy_events
                        .iter()
                        .map(|event| event.economy_event_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_purchases",
                "world.world_purchases",
                "purchase_id",
                world.world_purchases.len(),
                unique_key_count(
                    world
                        .world_purchases
                        .iter()
                        .map(|purchase| purchase.purchase_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_orders",
                "world.world_work_orders",
                "work_order_id",
                world.world_work_orders.len(),
                unique_key_count(
                    world
                        .world_work_orders
                        .iter()
                        .map(|work_order| work_order.work_order_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_deliveries",
                "world.world_work_deliveries",
                "delivery_id",
                world.world_work_deliveries.len(),
                unique_key_count(
                    world
                        .world_work_deliveries
                        .iter()
                        .map(|delivery| delivery.delivery_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_acceptances",
                "world.world_work_acceptances",
                "acceptance_id",
                world.world_work_acceptances.len(),
                unique_key_count(
                    world
                        .world_work_acceptances
                        .iter()
                        .map(|acceptance| acceptance.acceptance_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_rejections",
                "world.world_work_rejections",
                "rejection_id",
                world.world_work_rejections.len(),
                unique_key_count(
                    world
                        .world_work_rejections
                        .iter()
                        .map(|rejection| rejection.rejection_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_reopens",
                "world.world_work_reopens",
                "reopen_id",
                world.world_work_reopens.len(),
                unique_key_count(
                    world
                        .world_work_reopens
                        .iter()
                        .map(|reopen| reopen.reopen_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_work_cancellations",
                "world.world_work_cancellations",
                "cancellation_id",
                world.world_work_cancellations.len(),
                unique_key_count(
                    world
                        .world_work_cancellations
                        .iter()
                        .map(|cancellation| cancellation.cancellation_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json(
                "world_factions",
                "world.world_factions",
                "faction_id",
                world.world_factions.len(),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_faction_standings",
                "world.world_faction_standings",
                "matrix_user_id:faction_id",
                world.world_faction_standings.len(),
                unique_owned_key_count(world.world_faction_standings.iter().map(|standing| {
                    format!("{}:{}", standing.matrix_user_id, standing.faction_id)
                })),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_events",
                "world.world_events",
                "event_id",
                world.world_events.len(),
                unique_key_count(
                    world
                        .world_events
                        .iter()
                        .map(|event| event.event_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_contracts",
                "world.world_contracts",
                "contract_id",
                world.world_contracts.len(),
                unique_key_count(
                    world
                        .world_contracts
                        .iter()
                        .map(|contract| contract.contract_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_contract_completions",
                "world.world_contract_completions",
                "completion_id",
                world.world_contract_completions.len(),
                unique_key_count(
                    world
                        .world_contract_completions
                        .iter()
                        .map(|completion| completion.completion_id.as_str()),
                ),
            ),
            league_state_sql_cutover_table_json_with_normalized_count(
                "world_relationships",
                "world.world_relationships",
                "relationship_id",
                world.world_relationships.len(),
                unique_key_count(
                    world
                        .world_relationships
                        .iter()
                        .map(|relationship| relationship.relationship_id.as_str()),
                ),
            ),
        ];
    let total_rows = tables
        .iter()
        .filter_map(|table| table.get("row_count").and_then(Value::as_u64))
        .sum::<u64>();

    json!({
        "schema": "trillionnium_normalized_world_v1",
        "source_hash": state_hash,
        "generated_at": generated_at,
        "current_repository": TRILLIONNIUM_CURRENT_REPOSITORY,
        "next_repository": TRILLIONNIUM_NEXT_REPOSITORY,
        "migration_floor": TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR,
        "repository_contract": league_state_repository_contract_json(),
        "cutover_mode": "shadow_snapshot_then_dual_write_then_read_switch_then_final_direct_write_cutover",
        "write_order": [
            "league_core",
            "world_reference",
            "world_map",
            "world_assets",
            "world_commerce",
            "term_exchange_receipts",
            "world_route_events",
            "league_progression"
        ],
        "table_count": tables.len(),
        "total_rows": total_rows,
        "tables": tables,
    })
}

pub(super) fn league_state_repository_write_set_json(
    command: &str,
    boundary: &str,
    tables: &[&str],
    idempotency_key: &str,
    validation: &[&str],
) -> Value {
    json!({
        "command": command,
        "boundary": boundary,
        "tables": tables,
        "idempotency_key": idempotency_key,
        "validation": validation,
    })
}

pub(super) fn league_state_repository_dual_write_plan_json() -> Value {
    json!({
        "plan_version": "trillionnium_repository_dual_write_plan_v1",
        "mode": "normalized_sql_final_cutover_with_json_export_rollback",
        "write_order": [
            "validate_command",
            "mutate_in_memory_world_state",
            "rebuild_world_indexes_once",
            "write_typed_normalized_sql_command_tables",
            "emit_json_snapshot_export_and_repository_audit",
            "validate_normalized_read_models",
            "keep_json_snapshot_as_rollback_artifact"
        ],
        "write_sets": [
            league_state_repository_write_set_json(
                "world_action",
                "WorldState.world_events + optional contract seed",
                &["world_events", "world_contracts", "world_assets", "world_relationships"],
                "world_events.event_id",
                &["world_events_count", "world_contracts_task_linkage"]
            ),
            league_state_repository_write_set_json(
                "world_map_move",
                "WorldState.world_player_positions + movement economy audit + resource pressure runtime",
                &["world_player_positions", "world_economy_events", "world_trillionnium_characters"],
                "world_player_positions.matrix_user_id",
                &["map_node_fk", "position_count", "economy_event_count", "resource_pressure_runtime"]
            ),
            league_state_repository_write_set_json(
                "world_tactics_command",
                "WorldState tactics character/session/tick state + optional task/combat rewards",
                &["world_trillionnium_characters", "world_tactics_sessions", "world_tactics_simulation_ticks", "world_events", "world_relationships", "world_contracts", "world_contract_completions", "world_economy_events", "world_term_exchange_receipts", "league_players"],
                "world_tactics_simulation_ticks.tick_id",
                &["tactics_session_fk", "tick_count", "objective_progress", "victory_reward_settlement", "task_reward_gate", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_contract_completion",
                "WorldState contract settlement",
                &["world_contract_completions", "world_contracts", "world_assets", "world_term_exchange_receipts", "league_rewards", "league_inventory_items"],
                "world_contract_completions.completion_id",
                &["completion_contract_fk", "ledger_status", "reward_count", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_asset_upgrade",
                "WorldState asset growth + player progression",
                &["world_asset_upgrades", "world_assets", "league_players"],
                "world_asset_upgrades.upgrade_id",
                &["asset_upgrade_fk", "asset_level_monotonic", "player_progression"]
            ),
            league_state_repository_write_set_json(
                "world_company",
                "WorldState company/shop launch + owner relationship",
                &["world_companies", "world_shops", "world_listings", "world_relationships", "world_economy_events", "league_players"],
                "world_companies.company_id",
                &["company_asset_fk", "shop_company_fk", "owner_relationship_fk"]
            ),
            league_state_repository_write_set_json(
                "world_listing",
                "WorldState offer publishing",
                &["world_listings", "world_shops", "world_companies", "world_economy_events", "league_players"],
                "world_listings.listing_id",
                &["listing_shop_fk", "listing_status", "shop_company_denormalized_totals"]
            ),
            league_state_repository_write_set_json(
                "world_buy",
                "WorldState buyer reserve + work order open",
                &["world_purchases", "world_work_orders", "world_relationships", "world_economy_events", "world_faction_standings", "world_term_exchange_receipts", "league_players"],
                "world_purchases.purchase_id",
                &["purchase_listing_fk", "work_order_purchase_fk", "buyer_reserve_status", "buyer_seller_standings", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_work_deliver",
                "WorldState seller delivery",
                &["world_work_deliveries", "world_work_orders", "world_companies", "world_economy_events", "world_faction_standings", "league_players"],
                "world_work_deliveries.delivery_id",
                &["delivery_work_order_fk", "work_order_status", "seller_reputation_delta"]
            ),
            league_state_repository_write_set_json(
                "world_work_accept",
                "WorldState buyer acceptance + settlement",
                &["world_work_acceptances", "world_work_orders", "world_purchases", "world_companies", "world_economy_events", "world_faction_standings", "world_term_exchange_receipts", "league_players"],
                "world_work_acceptances.acceptance_id",
                &["acceptance_work_order_fk", "seller_settlement_status", "buyer_consume_status", "buyer_seller_progression", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_work_reject",
                "WorldState buyer rejection + refund",
                &["world_work_rejections", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings", "world_term_exchange_receipts"],
                "world_work_rejections.rejection_id",
                &["rejection_work_order_fk", "refund_status", "buyer_standing", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_work_reopen",
                "WorldState refunded order reserve reopen",
                &["world_work_reopens", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings", "world_term_exchange_receipts"],
                "world_work_reopens.reopen_id",
                &["reopen_work_order_fk", "reserve_status", "buyer_standing", "term_exchange_receipt_progression_class"]
            ),
            league_state_repository_write_set_json(
                "world_work_cancel",
                "WorldState open order cancellation",
                &["world_work_cancellations", "world_work_orders", "world_purchases", "world_economy_events", "world_faction_standings", "world_term_exchange_receipts"],
                "world_work_cancellations.cancellation_id",
                &["cancellation_work_order_fk", "refund_status", "buyer_standing", "term_exchange_receipt_progression_class"]
            )
        ],
        "read_switch_requirements": [
            "all_write_sets_dual_written",
            "json_sql_row_count_parity_green",
            "repository_audit_green",
            "repository_write_set_audit_green",
            "sql_snapshot_gate_green",
            "normalized_runtime_dual_write_gate_green",
            "normalized_runtime_read_switch_gate_green",
            "normalized_world_home_read_model_green",
            "normalized_client_feed_read_model_green",
            "web_e2e_green",
            "matrix_live_e2e_green"
        ]
    })
}

#[allow(dead_code)]
pub(super) fn normalized_repository_world_home_read_model_sql() -> &'static str {
    "select jsonb_build_object(
  'read_model_version', 'trillionnium_normalized_world_home_read_model_v1',
  'source_tables', jsonb_build_array('world_events', 'world_relationships', 'world_map_nodes', 'world_contracts', 'world_work_orders', 'world_faction_standings'),
  'world_event_count', (select count(*) from world_events),
  'world_relationship_count', (select count(*) from world_relationships),
  'world_map_node_count', (select count(*) from world_map_nodes),
  'world_contract_count', (select count(*) from world_contracts),
  'world_work_order_count', (select count(*) from world_work_orders),
  'world_faction_standing_count', (select count(*) from world_faction_standings),
  'latest_event_ids', coalesce((select jsonb_agg(event_id order by created_at desc, event_id desc) from (select event_id, created_at from world_events order by created_at desc, event_id desc limit 6) recent_events), '[]'::jsonb),
  'latest_work_order_ids', coalesce((select jsonb_agg(work_order_id order by created_at desc, work_order_id desc) from (select work_order_id, created_at from world_work_orders order by created_at desc, work_order_id desc limit 6) recent_work_orders), '[]'::jsonb)
) as normalized_world_home_read_model"
}

#[allow(dead_code)]
pub(super) fn normalized_repository_client_feed_read_model_sql() -> &'static str {
    "select jsonb_build_object(
  'read_model_version', 'trillionnium_normalized_client_feed_read_model_v1',
  'source_tables', jsonb_build_array(
    'world_events',
    'world_contracts',
    'world_purchases',
    'world_work_orders',
    'world_work_deliveries',
    'world_work_acceptances',
    'world_work_rejections',
    'world_work_reopens',
    'world_work_cancellations',
    'world_economy_events'
  ),
  'world_event_count', (select count(*) from world_events),
  'world_contract_count', (select count(*) from world_contracts),
  'world_purchase_count', (select count(*) from world_purchases),
  'world_work_order_count', (select count(*) from world_work_orders),
  'world_work_delivery_count', (select count(*) from world_work_deliveries),
  'world_work_acceptance_count', (select count(*) from world_work_acceptances),
  'world_work_rejection_count', (select count(*) from world_work_rejections),
  'world_work_reopen_count', (select count(*) from world_work_reopens),
  'world_work_cancellation_count', (select count(*) from world_work_cancellations),
  'world_economy_event_count', (select count(*) from world_economy_events),
  'feed_item_count', (
    select count(*)
    from (
      select event_id as item_id from world_events
      union all select contract_id from world_contracts
      union all select purchase_id from world_purchases
      union all select work_order_id from world_work_orders
      union all select delivery_id from world_work_deliveries
      union all select acceptance_id from world_work_acceptances
      union all select rejection_id from world_work_rejections
      union all select reopen_id from world_work_reopens
      union all select cancellation_id from world_work_cancellations
      union all select economy_event_id from world_economy_events
    ) feed_items
  ),
  'latest_feed_items', coalesce((
    select jsonb_agg(jsonb_build_object('kind', feed_kind, 'id', item_id) order by created_at desc, item_id desc)
    from (
      select feed_kind, item_id, created_at
      from (
        select 'event' as feed_kind, event_id as item_id, created_at from world_events
        union all select 'contract', contract_id, created_at from world_contracts
        union all select 'purchase', purchase_id, created_at from world_purchases
        union all select 'work_order', work_order_id, created_at from world_work_orders
        union all select 'work_delivery', delivery_id, created_at from world_work_deliveries
        union all select 'work_acceptance', acceptance_id, created_at from world_work_acceptances
        union all select 'work_rejection', rejection_id, created_at from world_work_rejections
        union all select 'work_reopen', reopen_id, created_at from world_work_reopens
        union all select 'work_cancellation', cancellation_id, created_at from world_work_cancellations
        union all select 'economy_event', economy_event_id, created_at from world_economy_events
      ) raw_feed_items
      order by created_at desc, item_id desc
      limit 12
    ) latest_feed_items
  ), '[]'::jsonb)
) as normalized_client_feed_read_model"
}

pub(super) fn normalized_repository_read_model_contract_json() -> Value {
    json!({
        "contract_version": "trillionnium_normalized_repository_read_model_v1",
        "phase": "read_model_shadow",
        "purpose": "exercise normalized SQL read-model seams before switching projections away from the JSON export snapshot",
        "world_home": {
            "read_model_version": "trillionnium_normalized_world_home_read_model_v1",
            "sql_helper": "normalized_repository_world_home_read_model_sql",
            "source_tables": [
                "world_events",
                "world_relationships",
                "world_map_nodes",
                "world_contracts",
                "world_work_orders",
                "world_faction_standings"
            ],
            "parity_gate": "normalized_world_home_read_model_green",
            "startup_gate": "normalized_read_model_startup_gate_green"
        },
        "client_feed": {
            "read_model_version": "trillionnium_normalized_client_feed_read_model_v1",
            "sql_helper": "normalized_repository_client_feed_read_model_sql",
            "source_tables": [
                "world_events",
                "world_contracts",
                "world_purchases",
                "world_work_orders",
                "world_work_deliveries",
                "world_work_acceptances",
                "world_work_rejections",
                "world_work_reopens",
                "world_work_cancellations",
                "world_economy_events"
            ],
            "parity_gate": "normalized_client_feed_read_model_green",
            "startup_gate": "normalized_client_feed_read_model_startup_gate_green"
        }
    })
}

pub(super) fn league_state_repository_write_set_audit_contract_json() -> Value {
    let dual_write_plan = league_state_repository_dual_write_plan_json();
    let write_sets = dual_write_plan
        .get("write_sets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let commands: Vec<String> = write_sets
        .iter()
        .filter_map(|write_set| {
            write_set
                .get("command")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();

    json!({
        "audit_version": "trillionnium_repository_write_set_audit_v1",
        "table": "league_state_repository_write_set_audits",
        "migration_floor": TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR,
        "cutover_phase": TRILLIONNIUM_REPOSITORY_FINAL_CUTOVER_PHASE,
        "unique_key": ["state_hash", "cutover_phase", "command"],
        "write_set_count": write_sets.len(),
        "commands": commands,
        "purpose": "materialize every command write-set boundary after replacing snapshot-backed normalized dual-write with direct normalized SQL repository writes",
    })
}

pub(super) fn league_state_repository_contract_json() -> Value {
    json!({
        "contract_version": "trillionnium_repository_cutover_v1",
        "dual_write_plan": league_state_repository_dual_write_plan_json(),
        "write_set_audit": league_state_repository_write_set_audit_contract_json(),
        "direct_write_contract": normalized_repository_direct_write_contract_json(),
        "read_model_contract": normalized_repository_read_model_contract_json(),
        "state_boundary": {
            "league_state_owner": "LeagueState",
            "world_state_owner": "WorldState",
            "legacy_json_shape": "serde_flatten_world_state_inside_league_state",
            "world_read_boundary": [
                "WorldRouteProjectionContext",
                "WorldHomeProjectionContext",
                "WorldMapProjectionContext",
                "ClientFeedProjectionContext",
                "ClientAppProjectionContext"
            ],
            "world_index_boundary": "WorldIndexes rebuilt from WorldState per projection/command batch",
            "world_write_boundary": "world command handlers mutate in-memory LeagueState.world and persist supported world commands through normalized SQL typed direct writes as the primary repository path",
            "runtime_dual_write_seam": "persist_league_state applies direct typed SQLx command helpers for declared world commands to CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL when CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED=true, while JSON/snapshot artifacts remain export and rollback outputs",
            "runtime_read_switch_seam": "startup can hydrate LeagueState from the latest normalized repository league_state_snapshots JSON export after repository audit, write-set audit, normalized world-home read-model, and normalized client-feed read-model gates pass when CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=true",
            "runtime_command_write_sql_helper": "normalized_repository_command_shadow_sql is retained only as legacy rollback/export test support; final cutover rejects unsupported world commands instead of generated-SQL fallback",
            "runtime_direct_write_helper": "execute_normalized_repository_direct_command_write applies typed SQLx upserts for supported commands while snapshot/export/audit artifacts remain rollback-only",
            "runtime_read_model_sql_helper": "normalized_repository_world_home_read_model_sql and normalized_repository_client_feed_read_model_sql expose direct normalized SQL read-model seams for world home/feed parity before projections leave the JSON export snapshot"
        },
        "runtime_validation": {
            "script": "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
            "checks": [
                "launch_consumer_entry_api_with_normalized_dual_write",
                "post_world_action_mutation",
                "verify_league_state_snapshots",
                "verify_league_state_repository_snapshots",
                "verify_league_state_repository_write_set_audits",
                "verify_direct_world_action_write_helper",
                "verify_direct_world_contract_completion_write_helper",
                "verify_direct_world_map_move_write_helper",
                "verify_direct_world_tactics_command_write_helper",
                "verify_direct_world_asset_upgrade_write_helper",
                "verify_direct_world_company_write_helper",
                "verify_direct_world_listing_write_helper",
                "verify_direct_world_buy_write_helper",
                "verify_direct_world_work_deliver_write_helper",
                "verify_direct_world_work_accept_write_helper",
                "verify_direct_world_work_reject_write_helper",
                "verify_direct_world_work_reopen_write_helper",
                "verify_direct_world_work_cancel_write_helper",
                "verify_command_scoped_world_table_upserts",
                "verify_normalized_world_event_rows",
                "verify_normalized_world_home_read_model_sql",
                "verify_normalized_client_feed_read_model_sql",
                "relaunch_consumer_entry_api_with_normalized_read_switch",
                "verify_read_switch_hydrates_dual_written_world_state"
            ]
        },
        "repository_phases": [
            {
                "phase": "shadow_snapshot",
                "read_repository": "json_file",
                "write_repository": "json_file",
                "sql_behavior": "emit normalized snapshot metadata without changing runtime reads"
            },
            {
                "phase": "dual_write",
                "read_repository": "json_file",
                "write_repository": "json_file_and_normalized_sql",
                "sql_behavior": "persist every world/league command through the command-scoped normalized SQL dual-write seam, materialize command write-set audits, and keep the full snapshot SQL as the parity/audit artifact"
            },
            {
                "phase": "read_switch",
                "read_repository": "normalized_sql_snapshot_export",
                "write_repository": "json_file_and_normalized_sql",
                "sql_behavior": "hydrate startup state from normalized repository snapshots while keeping json snapshot as rollback/export artifact"
            },
            {
                "phase": "final_cutover",
                "read_repository": "normalized_sql_direct_write_with_snapshot_export",
                "write_repository": "normalized_sql_direct_write_primary",
                "sql_behavior": "supported world commands commit typed SQLx upserts first-class in the normalized repository; JSON and SQL snapshots are export, audit, and rollback artifacts only"
            }
        ],
        "read_switch_gates": [
            "all migrations through 0026_add_term_exchange_receipt_tables.sql applied",
            "league_and_world_term_exchange_receipt_tables_shadow_status_and_progression_class",
            "WorldState projection contexts read from repository snapshots without direct LeagueState coupling",
            "repository_audit_green",
            "repository_write_set_audit_green",
            "normalized_read_model_startup_gate_green",
            "normalized_client_feed_read_model_startup_gate_green",
            "sql_snapshot_gate_green",
            "normalized_runtime_dual_write_gate_green",
            "normalized_runtime_read_switch_gate_green",
            "normalized_world_home_read_model_green",
            "normalized_client_feed_read_model_green",
            "web_e2e_green",
            "matrix_live_e2e_green",
            "json_sql_row_count_parity_green"
        ]
    })
}

pub(super) fn league_state_sql_shadow_validation_json(cutover_plan: &Value) -> Value {
    let checks: Vec<Value> = cutover_plan
        .get("tables")
        .and_then(Value::as_array)
        .map(|tables| {
            tables
                .iter()
                .filter_map(|table| {
                    let table_name = table.get("table_name").and_then(Value::as_str)?;
                    let expected_rows = table.get("row_count").and_then(Value::as_u64)?;
                    Some(json!({
                        "table_name": table_name,
                        "expected_rows": expected_rows,
                        "check": "count_equals_expected",
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    let validation_sql = checks
        .iter()
        .filter_map(|check| {
            let table_name = check.get("table_name").and_then(Value::as_str)?;
            let expected_rows = check.get("expected_rows").and_then(Value::as_u64)?;
            Some(format!(
                "select '{}' as table_name, count(*)::bigint as actual_rows, {}::bigint as expected_rows from {}",
                sql_quote(table_name), expected_rows, table_name
            ))
        })
        .collect::<Vec<_>>()
        .join("\nunion all\n");

    json!({
        "validation_version": "trillionnium_sql_shadow_validation_v1",
        "mode": "row_count_parity",
        "check_count": checks.len(),
        "checks": checks,
        "sql": validation_sql,
    })
}

#[derive(Debug, Clone)]
pub(super) struct LeagueStateRepositoryCutoverAudit {
    state_hash: String,
    generated_at: String,
    cutover_phase: String,
    current_repository: String,
    next_repository: String,
    migration_floor: String,
    cutover_plan: Value,
    shadow_validation: Value,
}

impl LeagueStateRepositoryCutoverAudit {
    pub(super) fn from_snapshot_parts(
        state_hash: &str,
        generated_at: &str,
        cutover_plan: &Value,
        shadow_validation: &Value,
    ) -> Self {
        let current_repository = cutover_plan
            .get("current_repository")
            .and_then(Value::as_str)
            .unwrap_or(TRILLIONNIUM_CURRENT_REPOSITORY)
            .to_string();
        let next_repository = cutover_plan
            .get("next_repository")
            .and_then(Value::as_str)
            .unwrap_or(TRILLIONNIUM_NEXT_REPOSITORY)
            .to_string();
        let migration_floor = cutover_plan
            .get("migration_floor")
            .and_then(Value::as_str)
            .unwrap_or(TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR)
            .to_string();
        Self {
            state_hash: state_hash.to_string(),
            generated_at: generated_at.to_string(),
            cutover_phase: TRILLIONNIUM_REPOSITORY_FINAL_CUTOVER_PHASE.to_string(),
            current_repository,
            next_repository,
            migration_floor,
            cutover_plan: cutover_plan.clone(),
            shadow_validation: shadow_validation.clone(),
        }
    }

    pub(super) fn json(&self) -> Value {
        json!({
            "audit_version": "trillionnium_repository_cutover_audit_v1",
            "state_hash": &self.state_hash,
            "generated_at": &self.generated_at,
            "cutover_phase": &self.cutover_phase,
            "current_repository": &self.current_repository,
            "next_repository": &self.next_repository,
            "migration_floor": &self.migration_floor,
            "source_snapshot_kind": "consumer_entry_json_v1",
            "plan_contract_version": self.cutover_plan
                .get("repository_contract")
                .and_then(|contract| contract.get("contract_version"))
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_repository_cutover_v1"),
            "dual_write_plan_version": self.cutover_plan
                .get("repository_contract")
                .and_then(|contract| contract.get("dual_write_plan"))
                .and_then(|plan| plan.get("plan_version"))
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_repository_dual_write_plan_v1"),
            "shadow_validation_version": self.shadow_validation
                .get("validation_version")
                .and_then(Value::as_str)
                .unwrap_or("trillionnium_sql_shadow_validation_v1"),
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct LeagueStateRepositorySnapshot {
    pub(super) state_hash: String,
    pub(super) generated_at: String,
    pub(super) state_json: String,
    pub(super) normalized_world_shadow_sql: String,
    pub(super) sql_cutover_plan: Value,
    pub(super) sql_shadow_validation: Value,
    pub(super) repository_audit: LeagueStateRepositoryCutoverAudit,
}

impl LeagueStateRepositorySnapshot {
    pub(super) fn from_league(league: &LeagueState) -> Result<Self, serde_json::Error> {
        let state_json = serde_json::to_string_pretty(league)?;
        let mut normalized_world_shadow_sql = world_state_normalized_shadow_sql(&league.world)?;
        normalized_world_shadow_sql.push_str(&league_term_exchange_receipts_shadow_sql(league)?);
        let state_hash = league_state_hash(league)?;
        let generated_at = Utc::now().to_rfc3339();
        let sql_cutover_plan =
            league_state_sql_cutover_plan_json(league, &state_hash, &generated_at);
        let sql_shadow_validation = league_state_sql_shadow_validation_json(&sql_cutover_plan);
        let repository_audit = LeagueStateRepositoryCutoverAudit::from_snapshot_parts(
            &state_hash,
            &generated_at,
            &sql_cutover_plan,
            &sql_shadow_validation,
        );
        Ok(Self {
            state_hash,
            generated_at,
            state_json,
            normalized_world_shadow_sql,
            sql_cutover_plan,
            sql_shadow_validation,
            repository_audit,
        })
    }

    pub(super) fn repository_write_set_audit_sql(&self) -> Result<String, serde_json::Error> {
        let Some(write_sets) = self
            .sql_cutover_plan
            .get("repository_contract")
            .and_then(|contract| contract.get("dual_write_plan"))
            .and_then(|plan| plan.get("write_sets"))
            .and_then(Value::as_array)
        else {
            return Ok(String::new());
        };

        let mut sql = String::from(
            "-- Repository write-set audit upserts (generated from dual-write plan).\n",
        );
        let empty_validation = json!([]);
        for write_set in write_sets {
            let Some(command) = write_set.get("command").and_then(Value::as_str) else {
                continue;
            };
            let boundary = write_set
                .get("boundary")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let tables: Vec<String> = write_set
                .get("tables")
                .and_then(Value::as_array)
                .map(|tables| {
                    tables
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let idempotency_key = write_set
                .get("idempotency_key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let validation = write_set.get("validation").unwrap_or(&empty_validation);
            let validation_json = serde_json::to_string(validation)?;
            let write_set_json = serde_json::to_string(write_set)?;
            sql.push_str(&format!(
                "insert into league_state_repository_write_set_audits (\n\
                     state_hash, cutover_phase, command, boundary, tables,\n\
                     idempotency_key, validation, write_set, migration_floor\n\
                 ) values (\n\
                     '{}', '{}', '{}', '{}', {},\n\
                     '{}', '{}'::jsonb, '{}'::jsonb, '{}'\n\
                 ) on conflict (state_hash, cutover_phase, command) do update set\n\
                     boundary = excluded.boundary,\n\
                     tables = excluded.tables,\n\
                     idempotency_key = excluded.idempotency_key,\n\
                     validation = excluded.validation,\n\
                     write_set = excluded.write_set,\n\
                     migration_floor = excluded.migration_floor;\n",
                sql_quote(&self.state_hash),
                sql_quote(self.repository_audit.cutover_phase.as_str()),
                sql_quote(command),
                sql_quote(boundary),
                sql_text_array_literal(&tables),
                sql_quote(idempotency_key),
                sql_quote(&validation_json),
                sql_quote(&write_set_json),
                sql_quote(self.repository_audit.migration_floor.as_str()),
            ));
        }
        Ok(sql)
    }

    pub(super) fn sql_snapshot_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.sql_snapshot_bytes_with_normalized_sql(&self.normalized_world_shadow_sql)
    }

    pub(super) fn runtime_dual_write_sql_bytes(
        &self,
        command: Option<&str>,
    ) -> Result<Vec<u8>, serde_json::Error> {
        let normalized_world_shadow_sql = if let Some(command) = command {
            normalized_repository_command_shadow_sql_from_full(
                &self.normalized_world_shadow_sql,
                command,
            )?
            .unwrap_or_else(|| {
                format!(
                    "-- Trillionnium normalized repository command-scoped world upsert skipped: unknown write-set command={}\n",
                    command.replace('\n', " ")
                )
            })
        } else {
            self.normalized_world_shadow_sql.clone()
        };
        self.sql_snapshot_bytes_with_normalized_sql_without_transaction(
            &normalized_world_shadow_sql,
        )
    }

    pub(super) fn runtime_audit_bridge_sql_bytes(
        &self,
        command: Option<&str>,
    ) -> Result<Vec<u8>, serde_json::Error> {
        let normalized_world_shadow_sql = if let Some(command) = command {
            format!(
                "-- Trillionnium normalized repository direct write bridge.\n\
                 -- World table upserts for command={} are executed by execute_normalized_repository_direct_command_write.\n",
                command.replace('\n', " ")
            )
        } else {
            "-- Trillionnium normalized repository audit bridge without normalized world table upserts.\n"
                .to_string()
        };
        self.sql_snapshot_bytes_with_normalized_sql_without_transaction(
            &normalized_world_shadow_sql,
        )
    }

    pub(super) fn sql_snapshot_bytes_with_normalized_sql(
        &self,
        normalized_world_shadow_sql: &str,
    ) -> Result<Vec<u8>, serde_json::Error> {
        self.sql_snapshot_bytes_with_normalized_sql_inner(normalized_world_shadow_sql, true)
    }

    pub(super) fn sql_snapshot_bytes_with_normalized_sql_without_transaction(
        &self,
        normalized_world_shadow_sql: &str,
    ) -> Result<Vec<u8>, serde_json::Error> {
        self.sql_snapshot_bytes_with_normalized_sql_inner(normalized_world_shadow_sql, false)
    }

    fn sql_snapshot_bytes_with_normalized_sql_inner(
        &self,
        normalized_world_shadow_sql: &str,
        include_transaction_wrapper: bool,
    ) -> Result<Vec<u8>, serde_json::Error> {
        let cutover_plan_json = serde_json::to_string(&self.sql_cutover_plan)?;
        let shadow_validation_json = serde_json::to_string(&self.sql_shadow_validation)?;
        let write_set_audit_sql = self.repository_write_set_audit_sql()?;
        let migration_floor = self.repository_audit.migration_floor.as_str();
        let cutover_phase = self.repository_audit.cutover_phase.as_str();
        let current_repository = self.repository_audit.current_repository.as_str();
        let next_repository = self.repository_audit.next_repository.as_str();
        let transaction_begin = if include_transaction_wrapper {
            "begin;\n             "
        } else {
            ""
        };
        let transaction_commit = if include_transaction_wrapper {
            "commit;\n"
        } else {
            ""
        };
        let sql = format!(
            "-- Trillionnium League SQL-ready snapshot.\n\
             -- Generated by consumer-entry-api at {}.\n\
             -- Apply migrations through {migration_floor} before loading.\n\
             -- Normalized repository cutover plan: {cutover_plan_json}\n\
             -- SQL shadow validation: {shadow_validation_json}\n\
             {transaction_begin}\
             insert into league_state_snapshots (snapshot_kind, state_hash, state)\n\
             values ('consumer_entry_json_v1', '{}', '{}'::jsonb)\n\
             on conflict (state_hash) do update set state = excluded.state;\n\
             {normalized_world_shadow_sql}\
             insert into league_state_repository_snapshots (\n\
                 state_hash, source_snapshot_kind, cutover_phase, current_repository,\n\
                 next_repository, migration_floor, cutover_plan, shadow_validation\n\
             ) values (\n\
                 '{}', 'consumer_entry_json_v1', '{}',\n\
                 '{}', '{}',\n\
                 '{}', '{}'::jsonb, '{}'::jsonb\n\
             ) on conflict (state_hash, cutover_phase) do update set\n\
                 current_repository = excluded.current_repository,\n\
                 next_repository = excluded.next_repository,\n\
                 migration_floor = excluded.migration_floor,\n\
                 cutover_plan = excluded.cutover_plan,\n\
                 shadow_validation = excluded.shadow_validation;\n\
             {write_set_audit_sql}\
             {transaction_commit}",
            self.generated_at,
            sql_quote(&self.state_hash),
            sql_quote(&self.state_json),
            sql_quote(&self.state_hash),
            sql_quote(cutover_phase),
            sql_quote(current_repository),
            sql_quote(next_repository),
            sql_quote(migration_floor),
            sql_quote(&cutover_plan_json),
            sql_quote(&shadow_validation_json),
        );
        Ok(sql.into_bytes())
    }

    pub(super) fn endpoint_json(
        &self,
        league: &LeagueState,
        json_state_path_configured: bool,
        sql_snapshot_path_configured: bool,
        normalized_dual_write_enabled: bool,
        normalized_read_switch_enabled: bool,
        normalized_database_configured: bool,
    ) -> Value {
        json!({
            "kind": "league_state_snapshot",
            "league": "trillionnium_league",
            "repository": TRILLIONNIUM_CURRENT_REPOSITORY,
            "state_hash": self.state_hash,
            "generated_at": self.generated_at,
            "json_state_path_configured": json_state_path_configured,
            "sql_snapshot_path_configured": sql_snapshot_path_configured,
            "normalized_repository": {
                "dual_write_enabled": normalized_dual_write_enabled,
                "database_configured": normalized_database_configured,
                "dual_write_active": normalized_dual_write_enabled && normalized_database_configured,
                "read_switch_enabled": normalized_read_switch_enabled,
                "read_switch_active": normalized_read_switch_enabled && normalized_database_configured,
                "active": (normalized_dual_write_enabled || normalized_read_switch_enabled) && normalized_database_configured,
                "write_boundary": "persist_league_state emits command-scoped normalized WorldState table upserts for world commands, plus the JSON snapshot/export/audit bridge, into the configured SQL repository after JSON persistence",
                "write_mode": "command_scoped_normalized_world_upserts_with_full_snapshot_export",
                "command_scoped_helper": "normalized_repository_command_shadow_sql",
                "direct_write_contract": normalized_repository_direct_write_contract_json(),
                "direct_write_helper": "execute_normalized_repository_direct_command_write",
                "direct_write_supported_commands": normalized_repository_direct_write_supported_commands(),
                "direct_write_mode": "typed_sqlx_command_helpers_for_supported_commands_with_generated_sql_fallback",
                "unknown_command_mode": "audit_only_no_full_world_snapshot_fallback",
                "read_switch_gate": "latest_snapshot_requires_repository_audit_and_write_set_audit",
                "read_switch_source_of_truth_gate": "latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_and_client_feed_read_models",
                "read_model_contract": normalized_repository_read_model_contract_json(),
                "read_boundary": "startup can hydrate LeagueState from the latest normalized repository league_state_snapshots JSON export only after repository audit, write-set audit, normalized world-home read-model, and normalized client-feed read-model gates pass when CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=true",
            },
            "sql_cutover_plan": self.sql_cutover_plan,
            "sql_shadow_validation": self.sql_shadow_validation,
            "repository_cutover_audit": self.repository_audit.json(),
            "repository_write_set_audit": league_state_repository_write_set_audit_contract_json(),
            "normalized_world_shadow_sql": normalized_world_shadow_sql_contract_json(
                self.normalized_world_shadow_sql.len(),
            ),
            "counts": {
                "players": league.players_by_matrix_user.len(),
                "matches": league.matches.len(),
                "entries": league.entries.len(),
                "battles": league.battles.len(),
                "submissions": league.submissions.len(),
                "rewards": league.rewards.len(),
                "inventory_items": league.inventory_items.len(),
                "raid_contributions": league.raid_contributions.len(),
                "raid_rosters": league.raid_rosters.len(),
            },
        })
    }
}

pub(super) async fn write_normalized_repository_snapshot_to_database(
    config: &ConsumerEntryConfig,
    repository_snapshot: &LeagueStateRepositorySnapshot,
    command: Option<&str>,
) -> Result<(), Response> {
    let normalized_write_enabled = config.league_normalized_dual_write_enabled
        || config.league_normalized_final_cutover_enabled;
    if !normalized_write_enabled {
        return Ok(());
    }

    let Some(database_url) = config.league_normalized_database_url.as_deref() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "normalized repository write path is enabled but no normalized database URL is configured",
            })),
        )
            .into_response());
    };

    let direct_command =
        command.filter(|command| normalized_repository_direct_write_supports_command(command));
    if config.league_normalized_final_cutover_enabled
        && command.is_some_and(|command| command.starts_with("world_"))
        && direct_command.is_none()
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!(
                    "normalized repository final cutover is enabled but command={} has no typed direct-write helper",
                    command.unwrap_or_default()
                ),
            })),
        )
            .into_response());
    }

    let sql_bytes = if direct_command.is_some() || config.league_normalized_final_cutover_enabled {
        repository_snapshot.runtime_audit_bridge_sql_bytes(command)
    } else {
        repository_snapshot.runtime_dual_write_sql_bytes(command)
    }
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to serialize normalized repository runtime dual-write SQL: {err}") })),
        )
            .into_response()
    })?;
    let sql = String::from_utf8(sql_bytes).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("normalized repository snapshot is not UTF-8: {err}") })),
        )
            .into_response()
    })?;

    let database_url = database_url.to_string();
    let repository_snapshot = repository_snapshot.clone();
    let direct_command = direct_command.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                format!("failed to create normalized repository transaction runtime: {err}")
            })?;
        runtime.block_on(async move {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect(&database_url)
                .await
                .map_err(|err| {
                    format!("failed to connect normalized repository database: {err}")
                })?;

            let write_result = async {
                let mut conn = pool.acquire().await.map_err(|err| {
                    format!("failed to acquire normalized repository connection: {err}")
                })?;
                sqlx::raw_sql("begin")
                    .execute(&mut *conn)
                    .await
                    .map_err(|err| {
                        format!("failed to begin normalized repository transaction: {err}")
                    })?;

                let transaction_result = async {
                    if let Some(command) = direct_command.as_deref() {
                        execute_normalized_repository_direct_command_write(
                            &mut conn,
                            &repository_snapshot,
                            command,
                        )
                        .await
                        .map(|_| ())?;
                    }
                    sqlx::raw_sql(&sql)
                        .execute(&mut *conn)
                        .await
                        .map_err(|err| {
                            format!("failed to write normalized repository snapshot export/audit artifacts: {err}")
                        })?;
                    Ok::<(), String>(())
                }
                .await;

                match transaction_result {
                    Ok(()) => {
                        sqlx::raw_sql("commit")
                            .execute(&mut *conn)
                            .await
                            .map_err(|err| {
                                format!("failed to commit normalized repository transaction: {err}")
                            })?;
                        Ok::<(), String>(())
                    }
                    Err(err) => {
                        if let Err(rollback_err) =
                            sqlx::raw_sql("rollback").execute(&mut *conn).await
                        {
                            Err(format!("{err}; rollback failed: {rollback_err}"))
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            .await;
            pool.close().await;
            write_result
        })
    })
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                json!({ "error": format!("normalized repository transaction task failed: {err}") }),
            ),
        )
            .into_response()
    })?
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
            .into_response()
    })
}

pub(super) fn league_repository_runtime_json(config: &ConsumerEntryConfig) -> Value {
    let database_configured = config.league_normalized_database_url.is_some();
    let normalized_dual_write_active =
        config.league_normalized_dual_write_enabled && database_configured;
    let normalized_read_switch_active =
        config.league_normalized_read_switch_enabled && database_configured;
    let normalized_final_cutover_active =
        config.league_normalized_final_cutover_enabled && normalized_dual_write_active;
    let effective_repository = if normalized_final_cutover_active {
        TRILLIONNIUM_FINAL_REPOSITORY
    } else if normalized_read_switch_active {
        TRILLIONNIUM_NEXT_REPOSITORY
    } else {
        TRILLIONNIUM_CURRENT_REPOSITORY
    };
    json!({
        "current_repository": TRILLIONNIUM_CURRENT_REPOSITORY,
        "next_repository": TRILLIONNIUM_NEXT_REPOSITORY,
        "effective_repository": effective_repository,
        "repository_cutover_status": if normalized_final_cutover_active {
            "normalized_sql_direct_write_final_cutover_active"
        } else if normalized_read_switch_active {
            "normalized_sql_dual_write_read_switch_active"
        } else if normalized_dual_write_active {
            "normalized_sql_dual_write_dual_write_active"
        } else if database_configured {
            "normalized_sql_database_configured"
        } else {
            "json_file_with_sql_snapshot_active"
        },
        "migration_floor": TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR,
        "json_state_path_configured": config.league_state_path.is_some(),
        "sql_snapshot_path_configured": config.league_sql_snapshot_path.is_some(),
        "normalized_database_configured": database_configured,
        "normalized_dual_write_enabled": config.league_normalized_dual_write_enabled,
        "normalized_dual_write_active": normalized_dual_write_active,
        "normalized_read_switch_enabled": config.league_normalized_read_switch_enabled,
        "normalized_read_switch_active": normalized_read_switch_active,
        "normalized_final_cutover_enabled": config.league_normalized_final_cutover_enabled,
        "normalized_final_cutover_active": normalized_final_cutover_active,
        "normalized_source_of_truth_read_models": if normalized_read_switch_active {
            "world_home_and_client_feed"
        } else {
            "not_active"
        },
        "normalized_runtime_write_mode": if normalized_final_cutover_active {
            "normalized_sql_primary_world_command_writes_with_snapshot_export_rollback"
        } else {
            "command_scoped_normalized_world_upserts_with_full_snapshot_export"
        },
        "normalized_command_scoped_helper": if normalized_final_cutover_active {
            "legacy_rollback_export_only"
        } else {
            "normalized_repository_command_shadow_sql"
        },
        "normalized_direct_write_mode": if normalized_final_cutover_active {
            "typed_sqlx_command_helpers_primary_for_supported_world_commands"
        } else {
            "typed_sqlx_command_helpers_for_supported_commands_with_generated_sql_fallback"
        },
        "normalized_direct_write_helper": "execute_normalized_repository_direct_command_write",
        "normalized_direct_write_transaction_mode": if normalized_final_cutover_active {
            "single_pg_transaction_direct_sql_primary_plus_snapshot_export"
        } else {
            "single_pg_transaction_bridge_sql_plus_direct_upserts"
        },
        "normalized_direct_write_supported_commands": normalized_repository_direct_write_supported_commands(),
        "normalized_direct_write_contract": normalized_repository_direct_write_contract_json(),
        "normalized_unknown_command_mode": if normalized_final_cutover_active {
            "unsupported_world_commands_rejected_no_generated_sql_fallback"
        } else {
            "audit_only_no_full_world_snapshot_fallback"
        },
        "normalized_read_switch_gate": "latest_snapshot_requires_repository_audit_and_write_set_audit",
        "normalized_read_switch_source_of_truth_gate": "latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_and_client_feed_read_models",
        "normalized_read_model_contract": normalized_repository_read_model_contract_json(),
        "dual_write_env": "CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED",
        "read_switch_env": "CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED",
        "final_cutover_env": "CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED",
        "database_url_env": "CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL",
    })
}

pub(super) async fn write_file_with_parent(
    path: &str,
    bytes: Vec<u8>,
    label: &str,
) -> Result<(), Response> {
    if let Some(parent) = StdPath::new(path).parent() {
        if let Err(err) = tokio::fs::create_dir_all(parent).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to create {label} dir: {err}") })),
            )
                .into_response());
        }
    }
    tokio::fs::write(path, bytes).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to persist {label}: {err}") })),
        )
            .into_response()
    })
}

pub(super) async fn persist_league_state(
    state: &AppState,
    league: &LeagueState,
) -> Result<(), Response> {
    persist_league_state_with_command(state, league, None).await
}

pub(super) async fn persist_league_state_after_command(
    state: &AppState,
    league: &LeagueState,
    command: &'static str,
) -> Result<(), Response> {
    persist_league_state_with_command(state, league, Some(command)).await
}

pub(super) async fn persist_league_state_with_command(
    state: &AppState,
    league: &LeagueState,
    command: Option<&str>,
) -> Result<(), Response> {
    let needs_repository_snapshot = state.config().league_sql_snapshot_path.is_some()
        || state.config().league_normalized_dual_write_enabled;
    let repository_snapshot = if needs_repository_snapshot {
        Some(LeagueStateRepositorySnapshot::from_league(league).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to build league repository snapshot: {err}") })),
            )
                .into_response()
        })?)
    } else {
        None
    };

    if let Some(path) = state.config().league_state_path.as_deref() {
        let bytes = serde_json::to_vec_pretty(league).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to serialize league state: {err}") })),
            )
                .into_response()
        })?;
        write_file_with_parent(path, bytes, "league state").await?;
    }
    if let Some(path) = state.config().league_sql_snapshot_path.as_deref() {
        let repository_snapshot = repository_snapshot.as_ref().ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "league SQL snapshot path configured but repository snapshot was not built" })),
            )
                .into_response()
        })?;
        let bytes = repository_snapshot.sql_snapshot_bytes().map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to serialize league SQL snapshot: {err}") })),
            )
                .into_response()
        })?;
        write_file_with_parent(path, bytes, "league SQL snapshot").await?;
    }
    if let Some(repository_snapshot) = repository_snapshot.as_ref() {
        write_normalized_repository_snapshot_to_database(
            state.config(),
            repository_snapshot,
            command,
        )
        .await?;
    }
    state
        .inner
        .health_world_readiness_cache_generation
        .fetch_add(1, Ordering::Relaxed);
    Ok(())
}
