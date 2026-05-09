use super::*;

pub(super) const TRILLIONNIUM_TACTICS_BOARD_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_board_v1";
pub(super) const TRILLIONNIUM_TACTICS_UNIT_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_unit_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMMAND_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_command_v1";
pub(super) const TRILLIONNIUM_CHARACTER_CONTRACT_VERSION: &str = "trillionnium_character_v1";
pub(super) const TRILLIONNIUM_SKILL_CONTRACT_VERSION: &str = "trillionnium_skill_v1";
pub(super) const TRILLIONNIUM_TRAINING_CONTRACT_VERSION: &str = "trillionnium_training_command_v1";
pub(super) const TRILLIONNIUM_SECT_CONTRACT_VERSION: &str = "trillionnium_sect_v1";
pub(super) const TRILLIONNIUM_NPC_CONTRACT_VERSION: &str = "trillionnium_npc_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_command_outcome_v1";
pub(super) const TRILLIONNIUM_SECT_OSM_BINDING_CONTRACT_VERSION: &str =
    "trillionnium_sect_osm_binding_v1";
pub(super) const TRILLIONNIUM_NPC_SPAWN_CONTRACT_VERSION: &str = "trillionnium_npc_spawn_anchor_v1";
pub(super) const TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION: &str =
    "trillionnium_npc_command_descriptor_v1";
pub(super) const TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION: &str =
    "trillionnium_mentor_training_task_v1";
pub(super) const TRILLIONNIUM_TASK_ARCHETYPE_CONTRACT_VERSION: &str =
    "trillionnium_task_archetype_v1";
pub(super) const TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION: &str =
    "trillionnium_task_completion_v1";
pub(super) const TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION: &str = "trillionnium_reward_gate_v1";
pub(super) const TRILLIONNIUM_BATTLE_LOG_STYLE_CONTRACT_VERSION: &str =
    "trillionnium_battle_log_style_v1";
pub(super) const TRILLIONNIUM_COMBAT_LOG_CONTRACT_VERSION: &str = "trillionnium_combat_log_v1";
pub(super) const TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION: &str =
    "trillionnium_npc_relationship_v1";
pub(super) const TRILLIONNIUM_OSM_OBJECTIVE_CONTRACT_VERSION: &str =
    "trillionnium_osm_objective_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION: &str =
    "trillionnium_tactics_combat_resolution_v1";
pub(super) const TRILLIONNIUM_TACTICS_GAME_SESSION_CONTRACT_VERSION: &str =
    "trillionnium_tactics_game_session_v1";
pub(super) const TRILLIONNIUM_TACTICS_SIMULATION_TICK_CONTRACT_VERSION: &str =
    "trillionnium_tactics_simulation_tick_v1";
pub(super) const TRILLIONNIUM_TACTICS_REWARD_SETTLEMENT_CONTRACT_VERSION: &str =
    "trillionnium_tactics_reward_settlement_v1";
pub(super) const TRILLIONNIUM_TACTICS_REPEAT_FARMING_ANTI_CHEESE_CONTRACT_VERSION: &str =
    "trillionnium_tactics_repeat_farming_anti_cheese_v1";
pub(super) const TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION: &str =
    "trillionnium_tactics_board_cell_interaction_v1";
pub(super) const TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION: &str =
    "trillionnium_tactics_unit_selection_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION: &str =
    "trillionnium_tactics_command_intent_draft_v1";
pub(super) const TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION: &str =
    "trillionnium_tactics_accessibility_v1";
pub(super) const TRILLIONNIUM_MAP_OVERLAY_IDENTITY_CONTRACT_VERSION: &str =
    "trillionnium_map_overlay_identity_v1";

fn default_tactics_objective_id() -> String {
    "defeat_market_bandit".to_string()
}

fn default_tactics_objective_goal() -> i64 {
    1
}

fn default_tactics_victory_state() -> String {
    "active".to_string()
}

fn default_tactics_reward_status() -> String {
    "not_eligible".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorldTacticsGameSession {
    pub(super) contract_version: String,
    pub(super) session_id: String,
    pub(super) matrix_user_id: String,
    pub(super) room_id: Option<String>,
    pub(super) board_id: String,
    pub(super) active_node_id: String,
    pub(super) active_overlay_id: String,
    pub(super) active_unit_id: String,
    pub(super) active_side: String,
    pub(super) status: String,
    pub(super) round: i64,
    pub(super) action_points_remaining: i64,
    pub(super) current_tick: i64,
    #[serde(default = "default_tactics_objective_id")]
    pub(super) objective_id: String,
    #[serde(default)]
    pub(super) objective_progress: i64,
    #[serde(default = "default_tactics_objective_goal")]
    pub(super) objective_goal: i64,
    #[serde(default = "default_tactics_victory_state")]
    pub(super) victory_state: String,
    #[serde(default = "default_tactics_reward_status")]
    pub(super) reward_status: String,
    #[serde(default)]
    pub(super) reward_event_id: Option<String>,
    #[serde(default)]
    pub(super) reward_credits_awarded: i64,
    #[serde(default)]
    pub(super) reward_xp_awarded: i64,
    pub(super) created_at_epoch: i64,
    pub(super) updated_at_epoch: i64,
    pub(super) source_of_truth: String,
    pub(super) persistence_owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorldTacticsSimulationTick {
    pub(super) contract_version: String,
    pub(super) tick_id: String,
    pub(super) session_id: String,
    pub(super) matrix_user_id: String,
    pub(super) room_id: Option<String>,
    pub(super) tick_index: i64,
    pub(super) command: String,
    pub(super) unit_id: String,
    pub(super) target_tile: Option<String>,
    pub(super) outcome_result: String,
    pub(super) outcome_accepted: bool,
    pub(super) simulation_effect: String,
    pub(super) round_before: i64,
    pub(super) round_after: i64,
    pub(super) action_points_before: i64,
    pub(super) action_points_after: i64,
    #[serde(default = "default_tactics_objective_id")]
    pub(super) objective_id: String,
    #[serde(default)]
    pub(super) objective_progress_before: i64,
    #[serde(default)]
    pub(super) objective_progress_after: i64,
    #[serde(default)]
    pub(super) objective_delta: i64,
    #[serde(default = "default_tactics_victory_state")]
    pub(super) victory_state_before: String,
    #[serde(default = "default_tactics_victory_state")]
    pub(super) victory_state_after: String,
    #[serde(default = "default_tactics_reward_status")]
    pub(super) reward_status_after: String,
    pub(super) active_unit_after: String,
    pub(super) generated_encounter_id: Option<String>,
    pub(super) osm_game_overlay_id: Option<String>,
    pub(super) created_at_epoch: i64,
    pub(super) source_of_truth: String,
}

fn world_tactics_session_id(matrix_user_id: &str, room_id: Option<&str>) -> String {
    league_hash_id(
        "world-tactics-session",
        &format!("{}:{}", matrix_user_id, room_id.unwrap_or("world-web")),
    )
}

fn world_tactics_active_node_id(world: &WorldState, matrix_user_id: &str) -> String {
    world
        .world_player_positions
        .get(matrix_user_id)
        .map(|position| position.node_id.clone())
        .unwrap_or_else(|| default_world_node_id().to_string())
}

fn world_tactics_active_overlay_id(world: &WorldState, matrix_user_id: &str) -> String {
    format!(
        "trillionnium-world-node:{}",
        world_tactics_active_node_id(world, matrix_user_id)
    )
}

fn world_tactics_command_action_cost(command: &str) -> i64 {
    match command {
        "talk_npc" | "select_unit" | "inspect_osm_underlay" | "end_turn" => 0,
        _ => 1,
    }
}

fn world_tactics_simulation_effect(command: &str, accepted: bool) -> &'static str {
    if !accepted {
        "rejected_no_state_advance"
    } else if command == "end_turn" {
        "round_advanced"
    } else if command == "attack" {
        "deterministic_combat_resolved"
    } else {
        "command_resolved_state_advanced"
    }
}

fn world_tactics_default_session(
    world: &WorldState,
    matrix_user_id: &str,
    room_id: Option<&str>,
    now_epoch: i64,
) -> WorldTacticsGameSession {
    WorldTacticsGameSession {
        contract_version: TRILLIONNIUM_TACTICS_GAME_SESSION_CONTRACT_VERSION.to_string(),
        session_id: world_tactics_session_id(matrix_user_id, room_id),
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.map(ToString::to_string),
        board_id: "mirror-street-tactics-board-v1".to_string(),
        active_node_id: world_tactics_active_node_id(world, matrix_user_id),
        active_overlay_id: world_tactics_active_overlay_id(world, matrix_user_id),
        active_unit_id: "lord".to_string(),
        active_side: "player".to_string(),
        status: "active".to_string(),
        round: 1,
        action_points_remaining: 2,
        current_tick: 0,
        objective_id: default_tactics_objective_id(),
        objective_progress: 0,
        objective_goal: default_tactics_objective_goal(),
        victory_state: default_tactics_victory_state(),
        reward_status: default_tactics_reward_status(),
        reward_event_id: None,
        reward_credits_awarded: 0,
        reward_xp_awarded: 0,
        created_at_epoch: now_epoch,
        updated_at_epoch: now_epoch,
        source_of_truth: "rust_world_tactics_game_session".to_string(),
        persistence_owner: "world_state.world_tactics_sessions".to_string(),
    }
}

pub(super) fn record_world_tactics_simulation_tick(
    world: &mut WorldState,
    matrix_user_id: &str,
    room_id: Option<&str>,
    command: &str,
    unit_id: Option<&str>,
    target_tile: Option<&str>,
    osm_game_overlay_id: Option<&str>,
    outcome: &Value,
    now_epoch: i64,
) -> (Value, Value) {
    let session_id = world_tactics_session_id(matrix_user_id, room_id);
    let current_node_id = world_tactics_active_node_id(world, matrix_user_id);
    let current_overlay_id = format!("trillionnium-world-node:{current_node_id}");
    let default_session = world_tactics_default_session(world, matrix_user_id, room_id, now_epoch);
    let session = world
        .world_tactics_sessions
        .entry(session_id.clone())
        .or_insert(default_session);
    let round_before = session.round;
    let ap_before = session.action_points_remaining;
    let accepted = outcome
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let result = outcome
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or(if accepted {
            "tactics_command_accepted"
        } else {
            "tactics_command_rejected"
        })
        .to_string();
    let tick_index = session.current_tick + 1;
    let action_cost = world_tactics_command_action_cost(command);
    let objective_progress_before = session.objective_progress;
    let victory_state_before = session.victory_state.clone();
    if accepted && command == "end_turn" {
        session.round += 1;
        session.action_points_remaining = 2;
        session.active_side = "player".to_string();
        session.active_unit_id = "lord".to_string();
    } else if accepted {
        session.action_points_remaining =
            session.action_points_remaining.saturating_sub(action_cost);
        session.active_unit_id = unit_id.unwrap_or("lord").to_string();
        session.active_side = if session.action_points_remaining == 0 {
            "enemy_pending".to_string()
        } else {
            "player".to_string()
        };
    }
    let objective_delta = if accepted
        && command == "attack"
        && outcome
            .get("combat_resolution")
            .and_then(|combat| combat.get("result"))
            .and_then(Value::as_str)
            == Some("defender_routed")
        && session.victory_state == "active"
    {
        1
    } else {
        0
    };
    if objective_delta > 0 {
        session.objective_progress =
            (session.objective_progress + objective_delta).clamp(0, session.objective_goal.max(1));
        if session.objective_progress >= session.objective_goal.max(1) {
            session.victory_state = "victory".to_string();
            session.status = "victory_pending_reward".to_string();
            if session.reward_status != "settled" {
                session.reward_status = "pending_settlement".to_string();
            }
        }
    } else if accepted
        && command == "end_turn"
        && session.round > 6
        && session.victory_state == "active"
    {
        session.victory_state = "failure".to_string();
        session.status = "failed_objective_timeout".to_string();
        session.reward_status = "not_eligible".to_string();
    }
    session.current_tick = tick_index;
    session.updated_at_epoch = now_epoch;
    session.active_node_id = current_node_id;
    session.active_overlay_id = osm_game_overlay_id
        .map(ToString::to_string)
        .or_else(|| {
            outcome
                .get("combat_resolution")
                .and_then(|combat| combat.get("osm_game_overlay_id"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or(current_overlay_id);
    let generated_encounter_id = outcome
        .get("combat_resolution")
        .and_then(|combat| combat.get("combat_resolution_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let tick = WorldTacticsSimulationTick {
        contract_version: TRILLIONNIUM_TACTICS_SIMULATION_TICK_CONTRACT_VERSION.to_string(),
        tick_id: league_hash_id(
            "world-tactics-tick",
            &format!("{}:{}:{}:{}", session_id, tick_index, command, now_epoch),
        ),
        session_id: session_id.clone(),
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.map(ToString::to_string),
        tick_index,
        command: command.to_string(),
        unit_id: unit_id.unwrap_or("lord").to_string(),
        target_tile: target_tile.map(ToString::to_string),
        outcome_result: result,
        outcome_accepted: accepted,
        simulation_effect: world_tactics_simulation_effect(command, accepted).to_string(),
        round_before,
        round_after: session.round,
        action_points_before: ap_before,
        action_points_after: session.action_points_remaining,
        objective_id: session.objective_id.clone(),
        objective_progress_before,
        objective_progress_after: session.objective_progress,
        objective_delta,
        victory_state_before,
        victory_state_after: session.victory_state.clone(),
        reward_status_after: session.reward_status.clone(),
        active_unit_after: session.active_unit_id.clone(),
        generated_encounter_id,
        osm_game_overlay_id: Some(session.active_overlay_id.clone()),
        created_at_epoch: now_epoch,
        source_of_truth: "rust_tactics_simulation_tick".to_string(),
    };
    world.world_tactics_simulation_ticks.push(tick.clone());
    if world.world_tactics_simulation_ticks.len() > 512 {
        let overflow = world.world_tactics_simulation_ticks.len() - 512;
        world.world_tactics_simulation_ticks.drain(0..overflow);
    }
    (json!(session), json!(tick))
}

pub(super) fn mark_world_tactics_reward_settled(
    world: &mut WorldState,
    session_id: &str,
    reward_event_id: &str,
    reward_credits: i64,
    reward_xp: i64,
    now_epoch: i64,
) -> Option<Value> {
    let session = world.world_tactics_sessions.get_mut(session_id)?;
    session.reward_status = "settled".to_string();
    session.status = "completed".to_string();
    session.reward_event_id = Some(reward_event_id.to_string());
    session.reward_credits_awarded = reward_credits;
    session.reward_xp_awarded = reward_xp;
    session.updated_at_epoch = now_epoch;
    Some(json!(session))
}

fn latest_world_tactics_session_for_user<'a>(
    world: &'a WorldState,
    matrix_user_id: &str,
) -> Option<&'a WorldTacticsGameSession> {
    world
        .world_tactics_sessions
        .values()
        .filter(|session| session.matrix_user_id == matrix_user_id)
        .max_by_key(|session| session.updated_at_epoch)
}

fn world_tactics_game_session_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
    current_node: Option<&WorldMapNode>,
) -> Value {
    if let Some(session) = latest_world_tactics_session_for_user(world, matrix_user_id) {
        let mut value = json!(session);
        value["persistence_status"] = json!("persisted");
        value["repository_boundary"] = json!("world_state.world_tactics_sessions");
        return value;
    }
    let now_epoch = 0;
    let mut session = world_tactics_default_session(world, matrix_user_id, None, now_epoch);
    if let Some(node) = current_node {
        session.active_node_id = node.node_id.clone();
        session.active_overlay_id = openstreetmap_game_overlay_id(node);
    }
    let mut value = json!(session);
    value["persistence_status"] = json!("projected_default_until_first_command");
    value["repository_boundary"] = json!("world_state.world_tactics_sessions");
    value
}

fn world_tactics_simulation_tick_log_json(world: &WorldState, matrix_user_id: &str) -> Value {
    let latest_session_id = latest_world_tactics_session_for_user(world, matrix_user_id)
        .map(|session| session.session_id.as_str());
    Value::Array(
        world
            .world_tactics_simulation_ticks
            .iter()
            .filter(|tick| {
                tick.matrix_user_id == matrix_user_id
                    && latest_session_id
                        .map(|session_id| tick.session_id == session_id)
                        .unwrap_or(true)
            })
            .rev()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|tick| json!(tick))
            .collect::<Vec<_>>(),
    )
}

fn world_map_overlay_identity_index_json(openstreetmap_geodata: &Value) -> Value {
    let mut identities = Vec::new();
    for feature in openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let game_overlay_id = feature
            .get("game_overlay_id")
            .and_then(Value::as_str)
            .unwrap_or("trillionnium-world-node:unknown");
        let game_binding = feature.get("game_binding").cloned().unwrap_or(Value::Null);
        identities.push(json!({
            "contract_version": TRILLIONNIUM_MAP_OVERLAY_IDENTITY_CONTRACT_VERSION,
            "overlay_identity_id": game_overlay_id,
            "game_overlay_id": game_overlay_id,
            "feature_id": feature.get("feature_id").cloned().unwrap_or(Value::Null),
            "osm_type": feature.get("osm_type").cloned().unwrap_or(Value::Null),
            "osm_id": feature.get("osm_id").cloned().unwrap_or(Value::Null),
            "semantic_role": feature.get("semantic_role").cloned().unwrap_or(Value::Null),
            "objective_seed": feature.get("objective_seed").cloned().unwrap_or(Value::Null),
            "node_id": game_binding.get("node_id").cloned().unwrap_or(Value::Null),
            "location_id": game_binding.get("location_id").cloned().unwrap_or(Value::Null),
            "zone_id": game_binding.get("zone_id").cloned().unwrap_or(Value::Null),
            "source_of_truth": "rust_openstreetmap_data_provider",
            "normalization_owner": "rust_map_overlay_identity_index",
            "web_role": "visualization_input_only",
        }));
    }
    identities.sort_by(|left, right| {
        left.get("game_overlay_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .get("game_overlay_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });
    Value::Array(identities)
}

#[derive(Debug, Clone)]
pub(super) struct TrillionniumSkillDefinition {
    skill_id: &'static str,
    family: &'static str,
    name: &'static str,
    level: u16,
    xp: u32,
    unlock_condition: &'static str,
    combat_effect: &'static str,
    world_effect: &'static str,
    training_anchor_role: &'static str,
}

impl TrillionniumSkillDefinition {
    fn to_value(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_SKILL_CONTRACT_VERSION,
            "skill_id": self.skill_id,
            "family": self.family,
            "name": self.name,
            "level": self.level,
            "xp": self.xp,
            "unlock_condition": self.unlock_condition,
            "combat_effect": self.combat_effect,
            "world_effect": self.world_effect,
            "training_anchor_role": self.training_anchor_role,
            "source_of_truth": "rust_trillionnium_skill_definition",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn trillionnium_fixture_skill_definitions() -> Vec<TrillionniumSkillDefinition> {
    vec![
        TrillionniumSkillDefinition {
            skill_id: "basic_inner_power",
            family: "inner_power",
            name: "Cloud Ledger Breathing / 云账吐纳",
            level: 1,
            xp: 120,
            unlock_condition: "default_character_seed",
            combat_effect: "raise_inner_energy_and_guard",
            world_effect: "improve_settlement_recovery_focus",
            training_anchor_role: "mentor_home",
        },
        TrillionniumSkillDefinition {
            skill_id: "basic_unarmed",
            family: "unarmed",
            name: "Street Compass Palm / 街指南掌",
            level: 1,
            xp: 60,
            unlock_condition: "inspect_civic_square",
            combat_effect: "enable_melee_attack",
            world_effect: "resolve_minor_street_encounters",
            training_anchor_role: "civic_square",
        },
        TrillionniumSkillDefinition {
            skill_id: "basic_blade",
            family: "blade",
            name: "Iron Workshop Blade / 铁坊刀法",
            level: 1,
            xp: 40,
            unlock_condition: "visit_workshop_anchor",
            combat_effect: "increase_attack_against_armored_targets",
            world_effect: "improve_artifact_repair_tasks",
            training_anchor_role: "workshop",
        },
        TrillionniumSkillDefinition {
            skill_id: "basic_sword",
            family: "sword",
            name: "Market Wind Sword / 集风剑式",
            level: 1,
            xp: 40,
            unlock_condition: "market_route_training",
            combat_effect: "increase_precision_attack",
            world_effect: "improve_negotiation_opening_move",
            training_anchor_role: "market",
        },
        TrillionniumSkillDefinition {
            skill_id: "basic_lightness",
            family: "lightness",
            name: "Night Watch Steps / 夜巡步",
            level: 1,
            xp: 100,
            unlock_condition: "default_character_seed",
            combat_effect: "increase_move_range_and_evade",
            world_effect: "reduce_route_travel_friction",
            training_anchor_role: "delivery_route",
        },
        TrillionniumSkillDefinition {
            skill_id: "reading_and_contracts",
            family: "civil",
            name: "Contract Reading / 契约读法",
            level: 1,
            xp: 140,
            unlock_condition: "default_character_seed",
            combat_effect: "reveal_risk_marker_before_attack",
            world_effect: "improve_contract_capture_and_review_hold_recovery",
            training_anchor_role: "ledger_hall",
        },
        TrillionniumSkillDefinition {
            skill_id: "merchant_routecraft",
            family: "commerce",
            name: "Merchant Routecraft / 商路术",
            level: 1,
            xp: 80,
            unlock_condition: "accept_market_bounty",
            combat_effect: "convert_market_tile_to_supply_bonus",
            world_effect: "improve_bounty_pricing_and_route_choice",
            training_anchor_role: "market",
        },
        TrillionniumSkillDefinition {
            skill_id: "artifact_crafting",
            family: "craft",
            name: "Artifact Crafting / 器作法",
            level: 1,
            xp: 80,
            unlock_condition: "visit_workshop_anchor",
            combat_effect: "improve_item_use_quality",
            world_effect: "increase_asset_upgrade_quality_bonus",
            training_anchor_role: "workshop",
        },
        TrillionniumSkillDefinition {
            skill_id: "streetwise_investigation",
            family: "investigation",
            name: "Streetwise Investigation / 街察术",
            level: 1,
            xp: 90,
            unlock_condition: "inspect_quest_board_or_dispute_desk",
            combat_effect: "reveal_hidden_enemy_intent",
            world_effect: "improve_evidence_gathering_and_dispute_routes",
            training_anchor_role: "arbitration_desk",
        },
    ]
}

fn trillionnium_skill_definition_by_id(skill_id: &str) -> Option<TrillionniumSkillDefinition> {
    trillionnium_fixture_skill_definitions()
        .into_iter()
        .find(|skill| skill.skill_id == skill_id)
}

pub(super) fn trillionnium_skill_definitions_json() -> Value {
    Value::Array(
        trillionnium_fixture_skill_definitions()
            .into_iter()
            .map(|skill| skill.to_value())
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_known_skill_definitions_json(skill_ids: &[String]) -> Value {
    Value::Array(
        trillionnium_fixture_skill_definitions()
            .into_iter()
            .filter(|skill| skill_ids.iter().any(|skill_id| skill_id == skill.skill_id))
            .map(|skill| skill.to_value())
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TrillionniumAttributes {
    pub(super) physique: u16,
    pub(super) force: u16,
    pub(super) agility: u16,
    pub(super) insight: u16,
    pub(super) resolve: u16,
    pub(super) craft: u16,
    pub(super) commerce: u16,
    pub(super) reputation: i32,
}

impl Default for TrillionniumAttributes {
    fn default() -> Self {
        Self {
            physique: 12,
            force: 11,
            agility: 12,
            insight: 13,
            resolve: 12,
            craft: 10,
            commerce: 10,
            reputation: 0,
        }
    }
}

impl TrillionniumAttributes {
    fn derived_stats_json(&self) -> Value {
        let max_hp = 80 + self.physique as i64 * 6 + self.resolve as i64 * 2;
        let inner_energy = 40 + self.resolve as i64 * 5 + self.insight as i64 * 2;
        let move_range = 3 + (self.agility / 8).clamp(0, 3);
        let learning_speed = 100 + self.insight as i64 * 4;
        let negotiation_bonus = self.commerce as i64 + (self.reputation / 10) as i64;
        json!({
            "max_hp": max_hp.clamp(80, 260),
            "inner_energy": inner_energy.clamp(40, 220),
            "move_range": move_range,
            "learning_speed": learning_speed.clamp(100, 220),
            "negotiation_bonus": negotiation_bonus.clamp(-25, 80),
            "craft_quality_bonus": (self.craft as i64 / 2).clamp(0, 50),
            "combat_power_hint": (self.force as i64 * 2 + self.agility as i64 + self.resolve as i64).clamp(0, 160),
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "physique": self.physique,
            "force": self.force,
            "agility": self.agility,
            "insight": self.insight,
            "resolve": self.resolve,
            "craft": self.craft,
            "commerce": self.commerce,
            "reputation": self.reputation,
            "derived_stats": self.derived_stats_json(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorldTrillionniumCharacter {
    pub(super) matrix_user_id: String,
    pub(super) character_id: String,
    pub(super) display_name: String,
    pub(super) attributes: TrillionniumAttributes,
    pub(super) sect_id: Option<String>,
    pub(super) title: String,
    pub(super) skill_ids: Vec<String>,
    pub(super) updated_at_epoch: i64,
}

impl WorldTrillionniumCharacter {
    fn default_for(matrix_user_id: &str) -> Self {
        Self {
            matrix_user_id: matrix_user_id.to_string(),
            character_id: league_hash_id("trillionnium-character", matrix_user_id),
            display_name: "镜城游侠".to_string(),
            attributes: TrillionniumAttributes::default(),
            sect_id: None,
            title: "初入Trillionnium".to_string(),
            skill_ids: vec![
                "basic_inner_power".to_string(),
                "basic_lightness".to_string(),
                "reading_and_contracts".to_string(),
            ],
            updated_at_epoch: 0,
        }
    }

    fn to_projection_json(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_CHARACTER_CONTRACT_VERSION,
            "source_of_truth": "rust_trillionnium_game_state",
            "mechanics_reference_layer": "gmud_rmxp_hero_yxts_llm_reference_only",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
            "matrix_user_id": &self.matrix_user_id,
            "character_id": &self.character_id,
            "display_name": &self.display_name,
            "title": &self.title,
            "sect_id": &self.sect_id,
            "attributes": self.attributes.to_value(),
            "skill_ids": &self.skill_ids,
            "known_skills": trillionnium_known_skill_definitions_json(&self.skill_ids),
            "skill_definition_contract": TRILLIONNIUM_SKILL_CONTRACT_VERSION,
            "skill_families": [
                "basic_inner_power",
                "basic_unarmed",
                "basic_blade",
                "basic_sword",
                "basic_lightness",
                "reading_and_contracts",
                "merchant_routecraft",
                "artifact_crafting",
                "streetwise_investigation"
            ],
            "next_development_hooks": [
                "sect_hall_osm_overlay_binding",
                "mentor_training_command",
                "npc_relationship_model",
                "wuxia_combat_log_generator"
            ],
            "updated_at_epoch": self.updated_at_epoch,
        })
    }
}

pub(super) fn world_trillionnium_character_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
) -> Value {
    world
        .world_trillionnium_characters
        .get(matrix_user_id)
        .cloned()
        .unwrap_or_else(|| WorldTrillionniumCharacter::default_for(matrix_user_id))
        .to_projection_json()
}

fn tactics_terrain_for(row: usize, col: usize) -> &'static str {
    match (row, col) {
        (0, 6) | (1, 5) | (2, 6) => "objective",
        (1, 1) | (2, 2) | (3, 3) | (4, 4) | (5, 5) => "road",
        (2, 0) | (3, 0) | (5, 2) | (6, 2) => "forest",
        (0, 3) | (1, 3) | (2, 3) | (3, 4) | (4, 5) => "river",
        (6, 0) | (7, 1) => "camp",
        (3, 6) | (4, 6) => "market",
        _ => "plain",
    }
}

fn tactics_tile_label(row: usize, col: usize) -> String {
    format!(
        "{}{}",
        (b'A' + col as u8) as char,
        8usize.saturating_sub(row)
    )
}

fn feature_for_role<'a>(features: &'a [Value], role: &str) -> Option<&'a Value> {
    features.iter().find(|feature| {
        feature
            .get("semantic_role")
            .and_then(Value::as_str)
            .is_some_and(|value| value == role)
    })
}

fn feature_overlay_id(feature: Option<&Value>) -> Option<String> {
    feature
        .and_then(|feature| feature.get("game_overlay_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn feature_overlay_id_for_role(features: &[Value], role: &str) -> Option<String> {
    feature_overlay_id(feature_for_role(features, role))
}

fn feature_str(feature: &Value, key: &str) -> Option<String> {
    feature
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn feature_i64(feature: &Value, key: &str) -> Option<i64> {
    feature.get(key).and_then(Value::as_i64)
}

fn osm_anchor_binding_json(features: &[Value], role: &str, binding_kind: &str) -> Value {
    let feature = feature_for_role(features, role);
    let overlay_id = feature_overlay_id(feature);
    let feature_id = feature.and_then(|value| feature_str(value, "feature_id"));
    let node_id = feature
        .and_then(|value| value.get("game_binding"))
        .and_then(|binding| binding.get("node_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    json!({
        "contract_version": match binding_kind {
            "sect_hall" => TRILLIONNIUM_SECT_OSM_BINDING_CONTRACT_VERSION,
            "npc_spawn" => TRILLIONNIUM_NPC_SPAWN_CONTRACT_VERSION,
            _ => "trillionnium_osm_anchor_binding_v1",
        },
        "binding_kind": binding_kind,
        "semantic_role": role,
        "osm_game_overlay_id": overlay_id,
        "feature_id": feature_id,
        "node_id": node_id,
        "osm_type": feature.and_then(|value| feature_str(value, "osm_type")),
        "osm_id": feature.and_then(|value| feature_i64(value, "osm_id")),
        "lat_string": feature.and_then(|value| feature_str(value, "lat_string")),
        "lng_string": feature.and_then(|value| feature_str(value, "lng_string")),
        "stable_fixture_identity": feature
            .and_then(|value| value.get("stable_fixture_identity"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "source_of_truth": "rust_openstreetmap_data_provider",
        "state_binding_owner": "rust_trillionnium_game_state",
        "web_role": "visualization_input_only",
        "fail_closed_if_missing": true,
    })
}

fn osm_anchor_overlay_id(anchor: &Value) -> Option<String> {
    anchor
        .get("osm_game_overlay_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[derive(Debug, Clone)]
struct TrillionniumTrainingCommand {
    skill_id: &'static str,
    mentor_npc_id: &'static str,
    required_semantic_role: &'static str,
    cost_xp: i64,
    cooldown_seconds: i64,
}

impl TrillionniumTrainingCommand {
    fn to_value(&self, features: &[Value]) -> Value {
        let anchor_overlay_id = feature_overlay_id_for_role(features, self.required_semantic_role)
            .unwrap_or_else(|| "trillionnium-world-node:mirror-city-square".to_string());
        json!({
            "contract_version": TRILLIONNIUM_TRAINING_CONTRACT_VERSION,
            "command": "train_skill",
            "skill_id": self.skill_id,
            "mentor_npc_id": self.mentor_npc_id,
            "mentor_training_task_flow_id": format!("mentor-training:{}", self.skill_id),
            "mentor_training_task_contract_version": TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION,
            "required_semantic_role": self.required_semantic_role,
            "required_osm_game_overlay_id": anchor_overlay_id,
            "cost_xp": self.cost_xp,
            "cooldown_seconds": self.cooldown_seconds,
            "validation_owner": "rust_mentor_training_validator",
            "state_mutation_owner": "rust_trillionnium_game_state",
            "web_role": "intent_only_visualization_input",
        })
    }
}

fn trillionnium_training_command_fixtures() -> Vec<TrillionniumTrainingCommand> {
    vec![
        TrillionniumTrainingCommand {
            skill_id: "basic_inner_power",
            mentor_npc_id: "npc-cloud-ledger-mentor",
            required_semantic_role: "mentor_home",
            cost_xp: 12,
            cooldown_seconds: 600,
        },
        TrillionniumTrainingCommand {
            skill_id: "basic_unarmed",
            mentor_npc_id: "npc-street-compass-sifu",
            required_semantic_role: "civic_square",
            cost_xp: 8,
            cooldown_seconds: 420,
        },
        TrillionniumTrainingCommand {
            skill_id: "basic_blade",
            mentor_npc_id: "npc-iron-workshop-smith",
            required_semantic_role: "workshop",
            cost_xp: 10,
            cooldown_seconds: 480,
        },
        TrillionniumTrainingCommand {
            skill_id: "basic_sword",
            mentor_npc_id: "npc-market-wind-adviser",
            required_semantic_role: "market",
            cost_xp: 10,
            cooldown_seconds: 480,
        },
        TrillionniumTrainingCommand {
            skill_id: "merchant_routecraft",
            mentor_npc_id: "npc-market-wind-adviser",
            required_semantic_role: "market",
            cost_xp: 16,
            cooldown_seconds: 900,
        },
        TrillionniumTrainingCommand {
            skill_id: "artifact_crafting",
            mentor_npc_id: "npc-iron-workshop-smith",
            required_semantic_role: "workshop",
            cost_xp: 16,
            cooldown_seconds: 900,
        },
        TrillionniumTrainingCommand {
            skill_id: "streetwise_investigation",
            mentor_npc_id: "npc-night-watch-arbiter",
            required_semantic_role: "arbitration_desk",
            cost_xp: 14,
            cooldown_seconds: 720,
        },
    ]
}

pub(super) fn trillionnium_training_commands_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        trillionnium_training_command_fixtures()
            .into_iter()
            .map(|command| command.to_value(&features))
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_training_command_for_skill(skill_id: &str) -> Option<TrillionniumTrainingCommand> {
    trillionnium_training_command_fixtures()
        .into_iter()
        .find(|command| command.skill_id == skill_id)
}

#[derive(Debug, Clone)]
struct TrillionniumSectFixture {
    sect_id: &'static str,
    display_name: &'static str,
    specialization: &'static str,
    anchor_role: &'static str,
    mentor_npc_ids: Vec<&'static str>,
    entry_requirement: &'static str,
    benefits: Vec<&'static str>,
    title_ladder: Vec<&'static str>,
}

impl TrillionniumSectFixture {
    fn to_value(&self, features: &[Value]) -> Value {
        let anchor_binding = osm_anchor_binding_json(features, self.anchor_role, "sect_hall");
        json!({
            "contract_version": TRILLIONNIUM_SECT_CONTRACT_VERSION,
            "osm_binding_contract_version": TRILLIONNIUM_SECT_OSM_BINDING_CONTRACT_VERSION,
            "sect_id": self.sect_id,
            "display_name": self.display_name,
            "specialization": self.specialization,
            "anchor_semantic_role": self.anchor_role,
            "osm_game_overlay_id": osm_anchor_overlay_id(&anchor_binding),
            "osm_anchor_binding": anchor_binding,
            "sect_hall_binding_owner": "rust_openstreetmap_data_provider",
            "game_overlay_binding_required": true,
            "mentor_npc_ids": self.mentor_npc_ids,
            "entry_requirement": self.entry_requirement,
            "benefits": self.benefits,
            "title_ladder": self.title_ladder,
            "source_of_truth": "rust_trillionnium_sect_model",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn trillionnium_sect_fixtures() -> Vec<TrillionniumSectFixture> {
    vec![
        TrillionniumSectFixture {
            sect_id: "cloud-ledger-hall",
            display_name: "Cloud Ledger Hall / 云账堂",
            specialization: "inner_power_contracts_and_settlement",
            anchor_role: "ledger_hall",
            mentor_npc_ids: vec!["npc-cloud-ledger-mentor"],
            entry_requirement: "reading_and_contracts_known",
            benefits: vec!["settlement_recovery_bonus", "contract_risk_preview"],
            title_ladder: vec!["outer_clerk", "ledger_runner", "cloud_ledger_keeper"],
        },
        TrillionniumSectFixture {
            sect_id: "street-compass-society",
            display_name: "Street Compass Society / 街指南社",
            specialization: "movement_investigation_and_routecraft",
            anchor_role: "civic_square",
            mentor_npc_ids: vec!["npc-street-compass-sifu"],
            entry_requirement: "basic_lightness_known",
            benefits: vec!["movement_range_hint", "street_event_preview"],
            title_ladder: vec!["street_walker", "route_scout", "compass_pathfinder"],
        },
        TrillionniumSectFixture {
            sect_id: "iron-workshop-gate",
            display_name: "Iron Workshop Gate / 铁坊门",
            specialization: "craft_blade_and_artifact_repair",
            anchor_role: "workshop",
            mentor_npc_ids: vec!["npc-iron-workshop-smith"],
            entry_requirement: "artifact_crafting_or_basic_blade_training",
            benefits: vec!["craft_quality_bonus", "item_repair_discount"],
            title_ladder: vec!["apprentice_smith", "artifact_mender", "iron_gate_master"],
        },
        TrillionniumSectFixture {
            sect_id: "market-wind-pavilion",
            display_name: "Market Wind Pavilion / 集风阁",
            specialization: "commerce_negotiation_and_bounty_quality",
            anchor_role: "market",
            mentor_npc_ids: vec!["npc-market-wind-adviser"],
            entry_requirement: "merchant_routecraft_training_available",
            benefits: vec!["listing_quality_hint", "negotiation_bonus"],
            title_ladder: vec!["stall_runner", "wind_broker", "market_pavilion_master"],
        },
        TrillionniumSectFixture {
            sect_id: "night-watch-alliance",
            display_name: "Night Watch Alliance / 夜巡盟",
            specialization: "risk_control_disputes_and_escort_tasks",
            anchor_role: "arbitration_desk",
            mentor_npc_ids: vec!["npc-night-watch-arbiter"],
            entry_requirement: "streetwise_investigation_training_available",
            benefits: vec!["dispute_evidence_bonus", "escort_risk_reduction"],
            title_ladder: vec!["watch_runner", "risk_warden", "night_watch_captain"],
        },
    ]
}

pub(super) fn trillionnium_sect_fixtures_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        trillionnium_sect_fixtures()
            .into_iter()
            .map(|sect| sect.to_value(&features))
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone)]
struct TrillionniumTaskArchetypeFixture {
    task_archetype_id: &'static str,
    display_name: &'static str,
    source_semantic_roles: Vec<&'static str>,
    command: &'static str,
    completion_owner: &'static str,
    reward_gate: &'static str,
    log_style_key: &'static str,
}

impl TrillionniumTaskArchetypeFixture {
    fn to_value(&self, features: &[Value]) -> Value {
        let candidates = self
            .source_semantic_roles
            .iter()
            .filter_map(|role| feature_for_role(features, role).map(|feature| (*role, feature)))
            .map(|(role, feature)| {
                json!({
                    "candidate_id": format!(
                        "trillionnium-task:{}:{}",
                        self.task_archetype_id,
                        feature
                            .get("game_overlay_id")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    ),
                    "task_archetype_id": self.task_archetype_id,
                    "source_semantic_role": role,
                    "osm_game_overlay_id": feature.get("game_overlay_id").cloned().unwrap_or(Value::Null),
                    "feature_id": feature.get("feature_id").cloned().unwrap_or(Value::Null),
                    "objective_seed": feature.get("objective_seed").cloned().unwrap_or(Value::Null),
                    "completion_owner": self.completion_owner,
                    "completion_contract_version": TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION,
                    "completion_command": "complete_task",
                    "reward_gate": self.reward_gate,
                    "reward_gate_contract_version": TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION,
                    "ledger_reward_requires_settlement": true,
                    "review_hold_gate_enforced": true,
                    "anti_cheese_gate_enforced": true,
                    "source_of_truth": "rust_openstreetmap_data_provider",
                })
            })
            .collect::<Vec<_>>();
        json!({
            "contract_version": TRILLIONNIUM_TASK_ARCHETYPE_CONTRACT_VERSION,
            "task_archetype_id": self.task_archetype_id,
            "display_name": self.display_name,
            "source_semantic_roles": self.source_semantic_roles,
            "command": self.command,
            "completion_owner": self.completion_owner,
            "reward_gate": self.reward_gate,
            "reward_gate_contract_version": TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION,
            "completion_contract_version": TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION,
            "completion_command": "complete_task",
            "rust_command_handler_decides_completion": true,
            "log_style_key": self.log_style_key,
            "candidate_generation_owner": "rust_openstreetmap_data_provider",
            "web_role": "visualization_input_only",
            "generated_candidates": candidates,
        })
    }
}

fn trillionnium_task_archetype_fixtures() -> Vec<TrillionniumTaskArchetypeFixture> {
    vec![
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "courier_letter",
            display_name: "Courier Letter / 飞笺传书",
            source_semantic_roles: vec!["delivery_route", "civic_square"],
            command: "offer_task",
            completion_owner: "rust_command_handler_ledger_progression",
            reward_gate: "ledger_settlement_review_hold_anti_cheese",
            log_style_key: "street_courier",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "find_item",
            display_name: "Find Item / 寻物探查",
            source_semantic_roles: vec!["workshop", "market"],
            command: "offer_task",
            completion_owner: "rust_world_action_handler",
            reward_gate: "evidence_required_before_reward",
            log_style_key: "street_investigation",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "escort_route",
            display_name: "Escort Route / 护送路线",
            source_semantic_roles: vec!["delivery_route", "arbitration_desk"],
            command: "offer_task",
            completion_owner: "rust_tactics_turn_handler",
            reward_gate: "proof_gated_route_completion",
            log_style_key: "escort_clash",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "market_settlement",
            display_name: "Market Settlement / 市集结算",
            source_semantic_roles: vec!["market", "ledger_hall"],
            command: "offer_task",
            completion_owner: "rust_market_command_handler",
            reward_gate: "ledger_release_required",
            log_style_key: "market_parley",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "defeat_bandit",
            display_name: "Defeat Bandit / 平定流寇",
            source_semantic_roles: vec!["arena", "market"],
            command: "attack",
            completion_owner: "rust_tactics_combat_handler",
            reward_gate: "combat_resolution_then_ledger_review_gate",
            log_style_key: "escort_clash",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "sect_training_trial",
            display_name: "Sect Training Trial / 门内试炼",
            source_semantic_roles: vec![
                "mentor_home",
                "civic_square",
                "workshop",
                "market",
                "arbitration_desk",
            ],
            command: "train_skill",
            completion_owner: "rust_mentor_training_validator",
            reward_gate: "mentor_place_cost_cooldown_required",
            log_style_key: "mentor_trial",
        },
    ]
}

fn trillionnium_task_archetype_by_id(
    task_archetype_id: &str,
) -> Option<TrillionniumTaskArchetypeFixture> {
    trillionnium_task_archetype_fixtures()
        .into_iter()
        .find(|task| task.task_archetype_id == task_archetype_id)
}

fn trillionnium_task_archetypes_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        trillionnium_task_archetype_fixtures()
            .into_iter()
            .map(|task| task.to_value(&features))
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_task_candidates_json(task_archetypes: &Value) -> Value {
    Value::Array(
        task_archetypes
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|task| {
                task.get("generated_candidates")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
            })
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_task_candidate_for(
    world: &WorldState,
    task_archetype_id: &str,
    osm_game_overlay_id: Option<&str>,
) -> Option<Value> {
    let nodes: Vec<WorldMapNode> = world.world_map_nodes.values().cloned().collect();
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);
    let task_archetypes = trillionnium_task_archetypes_json(&geodata);
    let candidates = trillionnium_task_candidates_json(&task_archetypes);
    candidates
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|candidate| {
            candidate
                .get("task_archetype_id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == task_archetype_id)
                && match osm_game_overlay_id {
                    Some(overlay_id) => candidate
                        .get("osm_game_overlay_id")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == overlay_id),
                    None => true,
                }
        })
}

fn trillionnium_objective_task_for_semantic_role(role: &str) -> Option<&'static str> {
    match role {
        "civic_square" | "mentor_home" | "sect_hall" => Some("sect_training_trial"),
        "ledger_hall" | "market" => Some("market_settlement"),
        "quest_board" | "workshop" | "arbitration_desk" => Some("find_item"),
        "delivery_route" => Some("courier_letter"),
        "arena" => Some("defeat_bandit"),
        "raid_hall" => Some("escort_route"),
        _ => None,
    }
}

fn trillionnium_objective_label_for_role(role: &str) -> &'static str {
    match role {
        "civic_square" => "集",
        "mentor_home" => "师",
        "ledger_hall" => "账",
        "sect_hall" => "门",
        "workshop" => "器",
        "market" => "市",
        "quest_board" => "榜",
        "delivery_route" => "路",
        "arbitration_desk" => "判",
        "arena" => "战",
        "raid_hall" => "盟",
        _ => "遇",
    }
}

fn trillionnium_objective_priority_for_role(role: &str) -> i64 {
    match role {
        "market" => 100,
        "arena" => 96,
        "delivery_route" => 92,
        "quest_board" => 88,
        "civic_square" => 84,
        "mentor_home" => 80,
        "ledger_hall" => 76,
        "workshop" => 72,
        "arbitration_desk" => 68,
        "sect_hall" => 64,
        "raid_hall" => 60,
        _ => 10,
    }
}

fn trillionnium_osm_objectives_json(
    world: &WorldState,
    matrix_user_id: &str,
    openstreetmap_geodata: &Value,
) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let user_contract_count = world
        .world_contracts
        .iter()
        .filter(|contract| contract.actor_matrix_user_id == matrix_user_id)
        .count();
    let user_relationship_count = world
        .world_relationships
        .iter()
        .filter(|relationship| relationship.from_id == matrix_user_id)
        .count();
    let mut objectives = Vec::new();
    for feature in features {
        let role = feature
            .get("semantic_role")
            .and_then(Value::as_str)
            .unwrap_or("street_encounter");
        let Some(task_archetype_id) = trillionnium_objective_task_for_semantic_role(role) else {
            continue;
        };
        let overlay_id = feature
            .get("game_overlay_id")
            .and_then(Value::as_str)
            .unwrap_or("trillionnium-world-node:unknown");
        let feature_id = feature
            .get("feature_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown-feature");
        let objective_kind = feature
            .get("objective_kind")
            .and_then(Value::as_str)
            .unwrap_or("free_roam_encounter");
        let completion_owner = feature
            .get("completion_owner")
            .and_then(Value::as_str)
            .unwrap_or("rust_world_action_handler");
        let task_completion_owner = trillionnium_task_archetype_by_id(task_archetype_id)
            .map(|task| task.completion_owner)
            .unwrap_or(completion_owner);
        let objective_seed_input = format!(
            "{}:{}:{}:{}:{}:{}",
            feature
                .get("objective_seed")
                .and_then(Value::as_str)
                .unwrap_or(feature_id),
            task_archetype_id,
            matrix_user_id,
            user_contract_count,
            user_relationship_count,
            OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
        );
        let deterministic_seed =
            league_hash_id("trillionnium-objective-seed", &objective_seed_input);
        let objective_id = league_hash_id(
            "trillionnium-osm-objective",
            &format!("{feature_id}:{task_archetype_id}:{deterministic_seed}"),
        );
        let grid_column = feature
            .get("game_binding")
            .and_then(|binding| binding.get("x"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .saturating_add(1)
            .clamp(1, 8);
        let grid_row = feature
            .get("game_binding")
            .and_then(|binding| binding.get("y"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .saturating_add(1)
            .clamp(1, 8);
        let priority = trillionnium_objective_priority_for_role(role);
        let suggested_command = if task_archetype_id == "defeat_bandit" {
            "attack"
        } else if task_archetype_id == "sect_training_trial" {
            "train_skill"
        } else {
            "complete_task"
        };
        objectives.push((
            priority,
            objective_id.clone(),
            json!({
                "contract_version": TRILLIONNIUM_OSM_OBJECTIVE_CONTRACT_VERSION,
                "objective_id": objective_id,
                "label": trillionnium_objective_label_for_role(role),
                "title": format!("{} · {}", objective_kind, task_archetype_id),
                "grid_column": grid_column,
                "grid_row": grid_row,
                "source": "osm_feature",
                "source_feature_id": feature_id,
                "source_semantic_role": role,
                "task_archetype_id": task_archetype_id,
                "osm_game_overlay_id": overlay_id,
                "overlay_identity_ref": overlay_id,
                "objective_kind": objective_kind,
                "objective_seed": deterministic_seed,
                "seed_inputs": {
                    "provider_objective_seed": feature.get("objective_seed").cloned().unwrap_or(Value::Null),
                    "matrix_user_id": matrix_user_id,
                    "user_contract_count": user_contract_count,
                    "user_relationship_count": user_relationship_count,
                },
                "suggested_command": suggested_command,
                "osm_feature_completion_owner": completion_owner,
                "completion_owner": task_completion_owner,
                "osm_can_suggest_objectives": true,
                "rust_command_handler_decides_completion": true,
                "requires_open_task_contract": suggested_command == "complete_task",
                "source_of_truth": "rust_trillionnium_osm_objective_generator",
                "web_role": "visualization_input_only",
            }),
        ));
    }
    objectives.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    Value::Array(
        objectives
            .into_iter()
            .map(|(_, _, objective)| objective)
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_task_archetype_ids_for_capability(capability: &str) -> Vec<&'static str> {
    match capability {
        "offer_patrol_task" => vec!["courier_letter", "escort_route"],
        "train_unarmed"
        | "train_lightness"
        | "train_blade"
        | "train_sword"
        | "train_routecraft"
        | "train_artifact_crafting"
        | "train_investigation"
        | "train_inner_power" => vec!["sect_training_trial"],
        "review_contract_risk" | "review_evidence" => vec!["market_settlement", "find_item"],
        "repair_item" => vec!["find_item"],
        "price_bounty" => vec!["market_settlement"],
        "offer_escort_task" => vec!["escort_route"],
        _ => Vec::new(),
    }
}

fn trillionnium_npc_relationship_delta(relation_kind: &str, strength: i64) -> i64 {
    match relation_kind {
        "trillionnium_npc_talk_npc" | "tactics_talk_npc" => 3,
        "trillionnium_npc_offer_task" | "tactics_offer_task" => 5,
        "trillionnium_npc_train_skill" | "tactics_train_skill" => 4,
        "trillionnium_npc_complete_task" | "tactics_complete_task" => 2,
        _ => strength.clamp(-3, 3),
    }
}

fn trillionnium_npc_relationship_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
    npc: &TrillionniumNpcFixture,
) -> Value {
    let mut event_count = 0_i64;
    let mut relationship_delta = 0_i64;
    let mut last_relation_kind: Option<String> = None;
    let mut last_updated_at_epoch = 0_i64;
    for relationship in world.world_relationships.iter().filter(|relationship| {
        relationship.from_id == matrix_user_id && relationship.to_id == npc.npc_id
    }) {
        event_count += 1;
        relationship_delta +=
            trillionnium_npc_relationship_delta(&relationship.relation_kind, relationship.strength);
        if relationship.updated_at_epoch >= last_updated_at_epoch {
            last_updated_at_epoch = relationship.updated_at_epoch;
            last_relation_kind = Some(relationship.relation_kind.clone());
        }
    }
    let relationship_score = (npc.relationship_seed + relationship_delta).clamp(-100, 100);
    let trust = (relationship_score / 2 + event_count * 2).clamp(0, 100);
    let risk_posture = if relationship_score >= 24 {
        "trusted"
    } else if relationship_score <= -20 {
        "hostile"
    } else {
        "watchful"
    };
    json!({
        "contract_version": TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION,
        "source_of_truth": "rust_world_relationships_persistent_state",
        "relationship_owner": "rust_trillionnium_npc_model",
        "matrix_user_id": matrix_user_id,
        "npc_id": npc.npc_id,
        "seed_relationship": npc.relationship_seed,
        "relationship_delta": relationship_delta,
        "relationship_score": relationship_score,
        "trust": trust,
        "risk_posture": risk_posture,
        "event_count": event_count,
        "last_relation_kind": last_relation_kind,
        "last_updated_at_epoch": last_updated_at_epoch,
        "web_role": "visualization_input_only",
    })
}

#[derive(Debug, Clone)]
struct TrillionniumNpcFixture {
    npc_id: &'static str,
    display_name: &'static str,
    role: &'static str,
    sect_id: &'static str,
    anchor_role: &'static str,
    relationship_seed: i64,
    schedule: &'static str,
    task_capabilities: Vec<&'static str>,
}

impl TrillionniumNpcFixture {
    fn to_value(&self, features: &[Value], relationship_state: Value) -> Value {
        let spawn_anchor = osm_anchor_binding_json(features, self.anchor_role, "npc_spawn");
        let command_descriptors = trillionnium_npc_command_descriptors_json(self, &spawn_anchor);
        let task_archetype_ids = trillionnium_npc_task_archetype_ids(self);
        let relationship_score = relationship_state
            .get("relationship_score")
            .and_then(Value::as_i64)
            .unwrap_or(self.relationship_seed);
        let trust = relationship_state
            .get("trust")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let risk_posture = relationship_state
            .get("risk_posture")
            .and_then(Value::as_str)
            .unwrap_or("watchful")
            .to_string();
        json!({
            "contract_version": TRILLIONNIUM_NPC_CONTRACT_VERSION,
            "spawn_contract_version": TRILLIONNIUM_NPC_SPAWN_CONTRACT_VERSION,
            "command_descriptor_contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
            "relationship_contract_version": TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION,
            "npc_id": self.npc_id,
            "display_name": self.display_name,
            "role": self.role,
            "sect_id": self.sect_id,
            "anchor_semantic_role": self.anchor_role,
            "osm_game_overlay_id": osm_anchor_overlay_id(&spawn_anchor),
            "spawn_anchor": spawn_anchor,
            "npc_spawn_owner": "rust_trillionnium_npc_model",
            "relationship_seed": self.relationship_seed,
            "relationship": relationship_score,
            "trust": trust,
            "risk_posture": risk_posture,
            "relationship_state": relationship_state,
            "schedule": self.schedule,
            "task_capabilities": self.task_capabilities,
            "task_archetype_ids": task_archetype_ids,
            "command_ids": ["talk_npc", "train_skill", "offer_task"],
            "command_descriptors": command_descriptors,
            "source_of_truth": "rust_trillionnium_npc_model",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn trillionnium_npc_training_skill_ids(npc_id: &str) -> Vec<&'static str> {
    trillionnium_training_command_fixtures()
        .into_iter()
        .filter(|command| command.mentor_npc_id == npc_id)
        .map(|command| command.skill_id)
        .collect::<Vec<_>>()
}

fn trillionnium_npc_task_archetype_ids(npc: &TrillionniumNpcFixture) -> Vec<&'static str> {
    let mut ids = npc
        .task_capabilities
        .iter()
        .flat_map(|capability| trillionnium_task_archetype_ids_for_capability(capability))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn trillionnium_npc_command_descriptors_json(
    npc: &TrillionniumNpcFixture,
    spawn_anchor: &Value,
) -> Value {
    let overlay_id = osm_anchor_overlay_id(spawn_anchor)
        .unwrap_or_else(|| "trillionnium-world-node:mirror-city-square".to_string());
    let training_skill_ids = trillionnium_npc_training_skill_ids(npc.npc_id);
    let task_archetype_ids = trillionnium_npc_task_archetype_ids(npc);
    json!([
        {
            "contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
            "command_id": format!("talk:{}", npc.npc_id),
            "command": "talk_npc",
            "label": "交谈 / Talk",
            "npc_id": npc.npc_id,
            "required_osm_game_overlay_id": overlay_id,
            "validation_owner": "rust_trillionnium_npc_interaction_validator",
            "state_mutation_owner": "rust_trillionnium_game_state",
            "body_template": format!("talk to {} about current route evidence", npc.display_name),
            "web_role": "intent_only_visualization_input",
        },
        {
            "contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
            "command_id": format!("train:{}", npc.npc_id),
            "command": "train_skill",
            "label": "拜师修炼 / Train",
            "npc_id": npc.npc_id,
            "skill_ids": training_skill_ids,
            "required_osm_game_overlay_id": overlay_id,
            "validation_owner": "rust_mentor_training_validator",
            "state_mutation_owner": "rust_trillionnium_game_state",
            "web_role": "intent_only_visualization_input",
        },
        {
            "contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
            "command_id": format!("offer-task:{}", npc.npc_id),
            "command": "offer_task",
            "label": "接任务 / Offer task",
            "npc_id": npc.npc_id,
            "task_archetype_ids": task_archetype_ids,
            "required_osm_game_overlay_id": overlay_id,
            "validation_owner": "rust_trillionnium_task_offer_validator",
            "state_mutation_owner": "rust_command_handler_ledger_progression",
            "body_template": format!("ask {} for a local Trillionnium task", npc.display_name),
            "web_role": "intent_only_visualization_input",
        }
    ])
}

fn trillionnium_npc_fixtures() -> Vec<TrillionniumNpcFixture> {
    vec![
        TrillionniumNpcFixture {
            npc_id: "npc-cloud-ledger-mentor",
            display_name: "Ledger Mentor Wen / 温账师",
            role: "mentor_contracts_inner_power",
            sect_id: "cloud-ledger-hall",
            anchor_role: "ledger_hall",
            relationship_seed: 12,
            schedule: "morning_ledger_evening_training",
            task_capabilities: vec!["train_inner_power", "review_contract_risk"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-street-compass-sifu",
            display_name: "Compass Sifu Luo / 罗街师",
            role: "mentor_movement_unarmed",
            sect_id: "street-compass-society",
            anchor_role: "civic_square",
            relationship_seed: 9,
            schedule: "daytime_square_patrol",
            task_capabilities: vec!["train_unarmed", "train_lightness", "offer_patrol_task"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-iron-workshop-smith",
            display_name: "Iron Smith Qiao / 乔铁匠",
            role: "mentor_blade_crafting",
            sect_id: "iron-workshop-gate",
            anchor_role: "workshop",
            relationship_seed: 7,
            schedule: "workshop_day_shift",
            task_capabilities: vec!["train_blade", "train_artifact_crafting", "repair_item"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-market-wind-adviser",
            display_name: "Market Adviser Lin / 林集风",
            role: "mentor_commerce_sword",
            sect_id: "market-wind-pavilion",
            anchor_role: "market",
            relationship_seed: 10,
            schedule: "market_open_hours",
            task_capabilities: vec!["train_sword", "train_routecraft", "price_bounty"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-night-watch-arbiter",
            display_name: "Night Arbiter Shen / 沈夜判",
            role: "mentor_investigation_disputes",
            sect_id: "night-watch-alliance",
            anchor_role: "arbitration_desk",
            relationship_seed: 8,
            schedule: "evening_dispute_watch",
            task_capabilities: vec![
                "train_investigation",
                "offer_escort_task",
                "review_evidence",
            ],
        },
    ]
}

pub(super) fn trillionnium_npc_fixtures_json(
    world: &WorldState,
    matrix_user_id: &str,
    openstreetmap_geodata: &Value,
) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        trillionnium_npc_fixtures()
            .into_iter()
            .map(|npc| {
                let relationship_state =
                    trillionnium_npc_relationship_projection_json(world, matrix_user_id, &npc);
                npc.to_value(&features, relationship_state)
            })
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_npc_spawn_anchors_json(npcs: &Value) -> Value {
    Value::Array(
        npcs.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|npc| npc.get("spawn_anchor").cloned())
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_npc_command_descriptors_from_npcs_json(npcs: &Value) -> Value {
    Value::Array(
        npcs.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|npc| {
                npc.get("command_descriptors")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
            })
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_mentor_training_task_flows_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        trillionnium_training_command_fixtures()
            .into_iter()
            .map(|command| {
                let anchor = osm_anchor_binding_json(
                    &features,
                    command.required_semantic_role,
                    "mentor_training",
                );
                json!({
                    "contract_version": TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION,
                    "task_flow_id": format!("mentor-training:{}", command.skill_id),
                    "task_archetype_id": "sect_training_trial",
                    "skill_id": command.skill_id,
                    "mentor_npc_id": command.mentor_npc_id,
                    "required_semantic_role": command.required_semantic_role,
                    "required_osm_game_overlay_id": osm_anchor_overlay_id(&anchor),
                    "osm_anchor_binding": anchor,
                    "cost_xp": command.cost_xp,
                    "cooldown_seconds": command.cooldown_seconds,
                    "steps": [
                        "travel_to_required_osm_anchor",
                        "talk_to_mentor_npc",
                        "submit_train_skill_intent",
                        "rust_validate_skill_mentor_place_cost_cooldown",
                        "mutate_world_trillionnium_character",
                        "record_world_event_and_relationship"
                    ],
                    "validation_owner": "rust_mentor_training_validator",
                    "completion_owner": "rust_command_handler_ledger_progression",
                    "web_role": "visualization_input_only",
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn trillionnium_battle_log_style_json() -> Value {
    json!({
        "contract_version": TRILLIONNIUM_BATTLE_LOG_STYLE_CONTRACT_VERSION,
        "style_id": "trillionnium_native_street_wuxia_log_v1",
        "source_of_truth": "rust_trillionnium_battle_log_generator",
        "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        "template_keys": [
            "street_courier",
            "street_investigation",
            "escort_clash",
            "market_parley",
            "mentor_trial"
        ],
        "inputs": ["skill_family", "terrain", "npc_role", "task_archetype", "outcome"],
        "web_role": "visualization_input_only",
    })
}

fn trillionnium_combat_log_json(
    objective_overlay_id: &str,
    trillionnium_character: &Value,
    task_candidates: &Value,
) -> Value {
    let display_name = trillionnium_character
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("镜城游侠");
    let skill_family = trillionnium_character
        .get("known_skills")
        .and_then(Value::as_array)
        .and_then(|skills| skills.first())
        .and_then(|skill| skill.get("family"))
        .and_then(Value::as_str)
        .unwrap_or("inner_power");
    let task_archetype_id = task_candidates
        .as_array()
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("task_archetype_id"))
        .and_then(Value::as_str)
        .unwrap_or("courier_letter");
    let log_id = league_hash_id(
        "trillionnium-combat-log",
        &format!("{display_name}:{skill_family}:{task_archetype_id}:{objective_overlay_id}"),
    );
    let beats = vec![
        json!({
            "kind": "stance",
            "text": format!("镜城风从巷口压低，{display_name}稳住气息，把任务封签收进袖中。"),
            "skill_family": skill_family,
            "delta_hp": 0,
        }),
        json!({
            "kind": "exchange",
            "text": "青石路面映出一瞬虚影，夜巡步斜切半格，证据袋没有离手。",
            "skill_family": "basic_lightness",
            "delta_hp": -3,
        }),
        json!({
            "kind": "task_gate",
            "text": format!("真实街格只提供锚点 {objective_overlay_id}；任务真相、复核和奖励门禁由 Rust 判定。"),
            "task_archetype_id": task_archetype_id,
            "osm_game_overlay_id": objective_overlay_id,
        }),
        json!({
            "kind": "result",
            "text": "战报封存：先验 deliverable，再查 evidence、risk controls、next action 与 self-review；账本结算通过后才释放奖励。",
            "outcome": "objective_secured_pending_reward_gate",
        }),
    ];
    json!({
        "contract_version": TRILLIONNIUM_COMBAT_LOG_CONTRACT_VERSION,
        "log_id": log_id,
        "style": "trillionnium_wuxia_log_v1",
        "style_contract": TRILLIONNIUM_BATTLE_LOG_STYLE_CONTRACT_VERSION,
        "template_pack": "trillionnium_native_combat_task_templates_v1",
        "source_of_truth": "rust_trillionnium_combat_log_generator",
        "content_policy": "trillionnium_native_no_copied_reference_text_assets_or_tables",
        "source_reference_safety": {
            "mechanics_reference_only": true,
            "generated_text_policy": "native_templates_only_no_verbatim_source_reference_strings",
            "test_gate": "forbid_source_reference_strings_in_generated_beats"
        },
        "inputs": {
            "skill_family": skill_family,
            "terrain": "market_street",
            "npc_role": "street_compass_mentor",
            "task_archetype": task_archetype_id,
            "outcome": "objective_secured_pending_reward_gate"
        },
        "beats": beats,
        "matrix_projection": {
            "enabled": true,
            "card_field": "trillionnium_combat_log",
            "summary_line": "native_wuxia_task_log_visible"
        },
        "app_projection": {
            "enabled": true,
            "json_field": "trillionnium_combat_log",
            "surface": "client_app.map.tactics_board.combat_log"
        }
    })
}

fn trillionnium_battle_log_lines_json(combat_log: &Value) -> Value {
    let style_contract = combat_log
        .get("style_contract")
        .and_then(Value::as_str)
        .unwrap_or(TRILLIONNIUM_BATTLE_LOG_STYLE_CONTRACT_VERSION);
    Value::Array(
        combat_log
            .get("beats")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|beat| {
                json!({
                    "kind": beat.get("kind").and_then(Value::as_str).unwrap_or("log"),
                    "style_contract": style_contract,
                    "text": beat.get("text").and_then(Value::as_str).unwrap_or(""),
                    "source_of_truth": "rust_trillionnium_combat_log_generator",
                })
            })
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone)]
pub(super) struct TacticsUnit {
    unit_id: &'static str,
    owner: &'static str,
    side: &'static str,
    archetype: &'static str,
    label: &'static str,
    title: &'static str,
    grid_column: i64,
    grid_row: i64,
    hp: i64,
    max_hp: i64,
    energy: i64,
    move_range: i64,
    attack_range: i64,
    status_effects: Vec<&'static str>,
    osm_game_overlay_id: Option<String>,
    actor_matrix_user_id: Option<String>,
    character_source: &'static str,
}

impl TacticsUnit {
    fn to_value(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_TACTICS_UNIT_CONTRACT_VERSION,
            "unit_id": self.unit_id,
            "owner": self.owner,
            "side": self.side,
            "class": self.archetype,
            "archetype": self.archetype,
            "label": self.label,
            "title": self.title,
            "grid_column": self.grid_column,
            "grid_row": self.grid_row,
            "hp": self.hp,
            "max_hp": self.max_hp,
            "energy": self.energy,
            "position": {
                "grid_column": self.grid_column,
                "grid_row": self.grid_row,
                "tile_id": tactics_tile_label((self.grid_row - 1).max(0) as usize, (self.grid_column - 1).max(0) as usize),
            },
            "move": self.move_range,
            "move_range": self.move_range,
            "attack_range": self.attack_range,
            "status_effects": self.status_effects,
            "osm_game_overlay_id": self.osm_game_overlay_id,
            "overlay_identity_ref": self.osm_game_overlay_id,
            "actor_matrix_user_id": self.actor_matrix_user_id,
            "character_source": self.character_source,
            "unit_selection_contract_version": TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION,
            "command_intent_draft_contract_version": TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION,
            "accessibility_contract_version": TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION,
            "selectable_unit": true,
            "selection_role": "active_unit",
            "keyboard_focus_role": "active_unit_button",
            "draft_input_name": "unit_id",
            "aria_role": "button",
            "validation_owner": "rust_tactics_command_validator",
            "web_role": "intent_only_visualization_input",
            "source_of_truth": "rust_tactics_unit_model",
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct TacticsCommandDescriptor {
    command_id: &'static str,
    command: &'static str,
    label: &'static str,
    command_family: &'static str,
    validation_owner: &'static str,
    web_target: &'static str,
    required_skill_id: Option<&'static str>,
    action_cost: i64,
}

impl TacticsCommandDescriptor {
    fn to_value(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_CONTRACT_VERSION,
            "command_id": self.command_id,
            "command": self.command,
            "label": self.label,
            "command_family": self.command_family,
            "validation_owner": self.validation_owner,
            "web_target": self.web_target,
            "required_skill_id": self.required_skill_id,
            "action_cost": self.action_cost,
            "command_intent_draft_contract_version": TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION,
            "accessibility_contract_version": TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION,
            "draft_input_name": "command",
            "keyboard_focus_role": "command_button",
            "aria_role": "button",
            "target_tile_required": matches!(self.command, "move_unit" | "attack" | "use_skill" | "interact"),
            "unit_selection_required": true,
            "draft_owner": "browser_tactics_intent_builder",
            "source_of_truth": "rust_tactics_command_model",
            "web_role": "intent_only_visualization_input",
        })
    }
}

fn tactics_units_json(
    matrix_user_id: &str,
    trillionnium_character: &Value,
    arena_overlay_id: Option<String>,
    market_overlay_id: Option<String>,
) -> Value {
    let max_hp = trillionnium_character["attributes"]["derived_stats"]["max_hp"]
        .as_i64()
        .unwrap_or(160);
    let energy = trillionnium_character["attributes"]["derived_stats"]["inner_energy"]
        .as_i64()
        .unwrap_or(100);
    let move_range = trillionnium_character["attributes"]["derived_stats"]["move_range"]
        .as_i64()
        .unwrap_or(4);
    Value::Array(
        vec![
            TacticsUnit {
                unit_id: "lord",
                owner: "player",
                side: "player",
                archetype: "trillionnium_lord",
                label: "主",
                title: "主公 / Lord",
                grid_column: 2,
                grid_row: 7,
                hp: (max_hp / 4).clamp(32, 80),
                max_hp,
                energy,
                move_range,
                attack_range: 1,
                status_effects: vec!["ready", "player_controlled"],
                osm_game_overlay_id: None,
                actor_matrix_user_id: Some(matrix_user_id.to_string()),
                character_source: "trillionnium_character",
            },
            TacticsUnit {
                unit_id: "strategist",
                owner: "ally",
                side: "ally",
                archetype: "route_strategist",
                label: "策",
                title: "军师 / Strategist",
                grid_column: 3,
                grid_row: 6,
                hp: 24,
                max_hp: 80,
                energy: 70,
                move_range: 3,
                attack_range: 2,
                status_effects: vec!["support", "route_reader"],
                osm_game_overlay_id: None,
                actor_matrix_user_id: None,
                character_source: "route_runner_support",
            },
            TacticsUnit {
                unit_id: "agent-squad",
                owner: "ally",
                side: "ally",
                archetype: "agent_scout",
                label: "斥",
                title: "Agent 斥候 / Scout",
                grid_column: 1,
                grid_row: 8,
                hp: 28,
                max_hp: 90,
                energy: 85,
                move_range: 5,
                attack_range: 1,
                status_effects: vec!["scout", "evidence_runner"],
                osm_game_overlay_id: None,
                actor_matrix_user_id: None,
                character_source: "agent_party",
            },
            TacticsUnit {
                unit_id: "rival-warlord",
                owner: "enemy",
                side: "enemy",
                archetype: "rival_commander",
                label: "敌",
                title: "敌将 / Rival",
                grid_column: 7,
                grid_row: 2,
                hp: 30,
                max_hp: 96,
                energy: 60,
                move_range: 3,
                attack_range: 1,
                status_effects: vec!["guarding_objective"],
                osm_game_overlay_id: arena_overlay_id,
                actor_matrix_user_id: None,
                character_source: "rust_fixture_enemy",
            },
            TacticsUnit {
                unit_id: "market-bandit",
                owner: "enemy",
                side: "enemy",
                archetype: "market_bandit",
                label: "寇",
                title: "流寇 / Bandit",
                grid_column: 6,
                grid_row: 4,
                hp: 18,
                max_hp: 64,
                energy: 48,
                move_range: 4,
                attack_range: 1,
                status_effects: vec!["threatens_market_route"],
                osm_game_overlay_id: market_overlay_id,
                actor_matrix_user_id: None,
                character_source: "rust_fixture_enemy",
            },
        ]
        .into_iter()
        .map(|unit| unit.to_value())
        .collect::<Vec<_>>(),
    )
}

fn tactics_available_commands_json() -> Value {
    Value::Array(
        vec![
            TacticsCommandDescriptor {
                command_id: "select_unit",
                command: "select_unit",
                label: "选中单位",
                command_family: "selection",
                validation_owner: "rust_tactics_command_validator",
                web_target: "#trillionnium-tactics-game-shell",
                required_skill_id: None,
                action_cost: 0,
            },
            TacticsCommandDescriptor {
                command_id: "move_unit",
                command: "move_unit",
                label: "行军路线",
                command_family: "movement",
                validation_owner: "rust_tactics_command_validator",
                web_target: "#world-map-route-flow-actions",
                required_skill_id: Some("basic_lightness"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "attack",
                command: "attack",
                label: "发起攻击",
                command_family: "combat",
                validation_owner: "rust_tactics_combat_handler",
                web_target: "#trillionnium-tactics-game-shell",
                required_skill_id: Some("basic_unarmed"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "use_skill",
                command: "use_skill",
                label: "施展技能",
                command_family: "trillionnium_skill",
                validation_owner: "rust_trillionnium_skill_validator",
                web_target: "#trillionnium-status",
                required_skill_id: Some("basic_inner_power"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "train_skill",
                command: "train_skill",
                label: "导师修炼",
                command_family: "mentor_training",
                validation_owner: "rust_mentor_training_validator",
                web_target: "#trillionnium-training",
                required_skill_id: None,
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "talk_npc",
                command: "talk_npc",
                label: "交谈问路",
                command_family: "npc_society",
                validation_owner: "rust_trillionnium_npc_interaction_validator",
                web_target: "#trillionnium-npcs",
                required_skill_id: None,
                action_cost: 0,
            },
            TacticsCommandDescriptor {
                command_id: "offer_task",
                command: "offer_task",
                label: "接取Trillionnium任务",
                command_family: "trillionnium_task_offer",
                validation_owner: "rust_trillionnium_task_offer_validator",
                web_target: "#trillionnium-npcs",
                required_skill_id: Some("reading_and_contracts"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "complete_task",
                command: "complete_task",
                label: "提交任务战报",
                command_family: "trillionnium_task_completion",
                validation_owner: "rust_trillionnium_task_completion_handler",
                web_target: "#trillionnium-task-candidates",
                required_skill_id: Some("reading_and_contracts"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "interact",
                command: "interact",
                label: "接取悬赏",
                command_family: "world_interaction",
                validation_owner: "rust_world_action_handler",
                web_target: "#world-commerce-panel",
                required_skill_id: Some("reading_and_contracts"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "inspect_osm_underlay",
                command: "interact",
                label: "查看底图",
                command_family: "map_inspection",
                validation_owner: "rust_openstreetmap_data_provider",
                web_target: "#world-real-map",
                required_skill_id: Some("streetwise_investigation"),
                action_cost: 0,
            },
            TacticsCommandDescriptor {
                command_id: "end_turn",
                command: "end_turn",
                label: "结束回合",
                command_family: "turn_control",
                validation_owner: "rust_tactics_turn_handler",
                web_target: "#trillionnium-tactics-game-shell",
                required_skill_id: None,
                action_cost: 0,
            },
        ]
        .into_iter()
        .map(|command| command.to_value())
        .collect::<Vec<_>>(),
    )
}

fn tactics_command_descriptor_json(command: &str) -> Option<Value> {
    tactics_available_commands_json()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|descriptor| {
            descriptor
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|value| value == command)
        })
}

fn tactics_command_rejection_json(
    command: &str,
    unit_id: &str,
    target_tile: Option<&str>,
    reason: &str,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
        "accepted": false,
        "command": command,
        "unit_id": unit_id,
        "target_tile": target_tile,
        "result": "tactics_command_rejected",
        "rejection_reason": reason,
        "source_of_truth": "rust_tactics_command_validator",
        "web_role": "intent_only_visualization_input",
    })
}

#[derive(Debug, Clone)]
struct TacticsCombatTarget {
    unit_id: &'static str,
    title: &'static str,
    tile_id: &'static str,
    hp_before: i64,
    guard: i64,
    terrain: &'static str,
    osm_game_overlay_id: Option<String>,
}

fn tactics_combat_target_for_tile(
    target_tile: Option<&str>,
    arena_overlay_id: Option<String>,
    market_overlay_id: Option<String>,
) -> Option<TacticsCombatTarget> {
    match target_tile.unwrap_or("F5") {
        "F5" => Some(TacticsCombatTarget {
            unit_id: "market-bandit",
            title: "流寇 / Bandit",
            tile_id: "F5",
            hp_before: 18,
            guard: 3,
            terrain: "market",
            osm_game_overlay_id: market_overlay_id,
        }),
        "G7" => Some(TacticsCombatTarget {
            unit_id: "rival-warlord",
            title: "敌将 / Rival",
            tile_id: "G7",
            hp_before: 30,
            guard: 7,
            terrain: "objective",
            osm_game_overlay_id: arena_overlay_id,
        }),
        _ => None,
    }
}

fn deterministic_tactics_damage(
    seed: &str,
    attributes: &TrillionniumAttributes,
    skill_id: &str,
    guard: i64,
) -> i64 {
    let seed_roll = seed.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(1_099_511_628_211)
            .wrapping_add(byte as u64)
    });
    let skill_bonus = match skill_id {
        "basic_unarmed" => 7,
        "basic_blade" | "basic_sword" => 9,
        "basic_inner_power" => 5,
        _ => 3,
    };
    let base =
        attributes.force as i64 + attributes.agility as i64 / 2 + attributes.resolve as i64 / 3;
    (base + skill_bonus + (seed_roll % 8) as i64 - guard).clamp(3, 64)
}

fn tactics_combat_resolution_json(
    world: &WorldState,
    matrix_user_id: &str,
    attacker_unit_id: &str,
    target_tile: Option<&str>,
    skill_id: &str,
    attributes: &TrillionniumAttributes,
    now_epoch: i64,
) -> Option<Value> {
    let nodes: Vec<WorldMapNode> = world.world_map_nodes.values().cloned().collect();
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);
    let features = geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let arena_overlay_id = feature_overlay_id(feature_for_role(&features, "arena"));
    let market_overlay_id = feature_overlay_id(feature_for_role(&features, "market"));
    let target = tactics_combat_target_for_tile(target_tile, arena_overlay_id, market_overlay_id)?;
    let seed = league_hash_id(
        "tactics-combat-seed",
        &format!(
            "{}:{}:{}:{}:{}:{}",
            matrix_user_id,
            attacker_unit_id,
            target.unit_id,
            target.tile_id,
            skill_id,
            target
                .osm_game_overlay_id
                .as_deref()
                .unwrap_or("no-osm-overlay")
        ),
    );
    let damage = deterministic_tactics_damage(&seed, attributes, skill_id, target.guard);
    let hp_after = target.hp_before.saturating_sub(damage).max(0);
    let result = if hp_after == 0 {
        "defender_routed"
    } else {
        "hit_landed"
    };
    Some(json!({
        "contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
        "combat_resolution_id": league_hash_id("tactics-combat-resolution", &format!("{seed}:{now_epoch}")),
        "deterministic_seed": seed,
        "attacker_unit_id": attacker_unit_id,
        "defender_unit_id": target.unit_id,
        "defender_title": target.title,
        "target_tile": target.tile_id,
        "terrain": target.terrain,
        "skill_id": skill_id,
        "damage": damage,
        "defender_hp_before": target.hp_before,
        "defender_hp_after": hp_after,
        "result": result,
        "osm_game_overlay_id": target.osm_game_overlay_id,
        "source_of_truth": "rust_tactics_combat_handler",
        "state_persistence": "world_event_log_now_game_session_hp_state_in_tw4_4",
        "osm_can_place_encounter": true,
        "rust_combat_handler_decides_resolution": true,
        "web_role": "intent_only_visualization_input",
    }))
}

pub(super) fn apply_world_tactics_command(
    world: &mut WorldState,
    matrix_user_id: &str,
    command: &str,
    unit_id: Option<&str>,
    target_tile: Option<&str>,
    skill_id: Option<&str>,
    npc_id: Option<&str>,
    task_archetype_id: Option<&str>,
    osm_game_overlay_id: Option<&str>,
    now_epoch: i64,
) -> Value {
    let unit_id = unit_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("lord");
    let valid_units = [
        "lord",
        "strategist",
        "agent-squad",
        "rival-warlord",
        "market-bandit",
    ];
    if !valid_units.contains(&unit_id) {
        return tactics_command_rejection_json(command, unit_id, target_tile, "unknown_unit_id");
    }
    let Some(descriptor) = tactics_command_descriptor_json(command) else {
        return tactics_command_rejection_json(command, unit_id, target_tile, "unknown_command");
    };
    let required_skill_id = descriptor
        .get("required_skill_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    if command == "complete_task" {
        let known_skills: HashSet<String> = world
            .world_trillionnium_characters
            .entry(matrix_user_id.to_string())
            .or_insert_with(|| WorldTrillionniumCharacter::default_for(matrix_user_id))
            .skill_ids
            .iter()
            .cloned()
            .collect();
        if let Some(required_skill_id) = required_skill_id.as_deref() {
            if !known_skills.contains(required_skill_id) {
                return json!({
                    "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                    "accepted": false,
                    "command": command,
                    "unit_id": unit_id,
                    "target_tile": target_tile,
                    "required_skill_id": required_skill_id,
                    "result": "skill_locked",
                    "rejection_reason": "required_skill_not_known",
                    "source_of_truth": "rust_trillionnium_task_completion_handler",
                    "web_role": "intent_only_visualization_input",
                });
            }
        }
        let selected_task_archetype_id = task_archetype_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("courier_letter");
        let Some(task_archetype) = trillionnium_task_archetype_by_id(selected_task_archetype_id)
        else {
            return tactics_command_rejection_json(
                command,
                unit_id,
                target_tile,
                "unknown_task_archetype_id",
            );
        };
        let provided_overlay_id = osm_game_overlay_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(candidate) =
            trillionnium_task_candidate_for(world, selected_task_archetype_id, provided_overlay_id)
        else {
            return json!({
                "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                "accepted": false,
                "command": command,
                "unit_id": unit_id,
                "target_tile": target_tile,
                "task_archetype_id": selected_task_archetype_id,
                "provided_osm_game_overlay_id": provided_overlay_id,
                "result": "task_objective_mismatch",
                "rejection_reason": "task_completion_requires_osm_generated_candidate",
                "source_of_truth": "rust_trillionnium_task_completion_handler",
                "web_role": "intent_only_visualization_input",
            });
        };
        let character = world
            .world_trillionnium_characters
            .entry(matrix_user_id.to_string())
            .or_insert_with(|| WorldTrillionniumCharacter::default_for(matrix_user_id));
        character.title = "提交Trillionnium战报".to_string();
        character.updated_at_epoch = now_epoch;
        return json!({
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
            "accepted": true,
            "command": command,
            "unit_id": unit_id,
            "target_tile": target_tile,
            "task_archetype_id": selected_task_archetype_id,
            "task_archetype_contract_version": TRILLIONNIUM_TASK_ARCHETYPE_CONTRACT_VERSION,
            "completion_contract_version": TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION,
            "reward_gate_contract_version": TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION,
            "reward_gate": task_archetype.reward_gate,
            "completion_owner": task_archetype.completion_owner,
            "required_osm_game_overlay_id": candidate.get("osm_game_overlay_id").cloned().unwrap_or(Value::Null),
            "task_candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
            "validation_owner": descriptor.get("validation_owner").cloned().unwrap_or_else(|| json!("rust_trillionnium_task_completion_handler")),
            "result": "task_completion_validated",
            "state_mutation": "world_trillionnium_task_completion_pending_ledger_settlement",
            "ledger_reward_requires_settlement": true,
            "review_hold_gate_enforced": true,
            "anti_cheese_gate_enforced": true,
            "source_of_truth": "rust_trillionnium_task_completion_handler",
            "web_role": "intent_only_visualization_input",
            "updated_at_epoch": now_epoch,
        });
    }

    let character = world
        .world_trillionnium_characters
        .entry(matrix_user_id.to_string())
        .or_insert_with(|| WorldTrillionniumCharacter::default_for(matrix_user_id));
    let known_skills: HashSet<String> = character.skill_ids.iter().cloned().collect();

    if matches!(command, "talk_npc" | "offer_task") {
        if command == "offer_task" {
            if let Some(required_skill_id) = required_skill_id.as_deref() {
                if !known_skills.contains(required_skill_id) {
                    return json!({
                        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                        "accepted": false,
                        "command": command,
                        "unit_id": unit_id,
                        "target_tile": target_tile,
                        "required_skill_id": required_skill_id,
                        "result": "skill_locked",
                        "rejection_reason": "required_skill_not_known",
                        "source_of_truth": "rust_trillionnium_task_offer_validator",
                        "web_role": "intent_only_visualization_input",
                    });
                }
            }
        }
        let requested_npc_id = npc_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("npc-street-compass-sifu");
        let Some(npc) = trillionnium_npc_fixtures()
            .into_iter()
            .find(|candidate| candidate.npc_id == requested_npc_id)
        else {
            return tactics_command_rejection_json(command, unit_id, target_tile, "unknown_npc_id");
        };
        let nodes: Vec<WorldMapNode> = world.world_map_nodes.values().cloned().collect();
        let geodata = openstreetmap_geodata_v1_json(&nodes, None);
        let features = geodata
            .get("features")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let spawn_anchor = osm_anchor_binding_json(&features, npc.anchor_role, "npc_spawn");
        let required_overlay_id = osm_anchor_overlay_id(&spawn_anchor)
            .unwrap_or_else(|| "trillionnium-world-node:mirror-city-square".to_string());
        if let Some(provided_overlay_id) = osm_game_overlay_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if provided_overlay_id != required_overlay_id {
                return json!({
                    "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                    "accepted": false,
                    "command": command,
                    "unit_id": unit_id,
                    "npc_id": requested_npc_id,
                    "required_osm_game_overlay_id": required_overlay_id,
                    "provided_osm_game_overlay_id": provided_overlay_id,
                    "result": "npc_place_mismatch",
                    "rejection_reason": "npc_interaction_requires_matching_osm_spawn_anchor",
                    "source_of_truth": "rust_trillionnium_npc_interaction_validator",
                    "web_role": "intent_only_visualization_input",
                });
            }
        }
        let allowed_task_archetype_ids = trillionnium_npc_task_archetype_ids(&npc);
        let selected_task_archetype_id = task_archetype_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| allowed_task_archetype_ids.first().copied());
        if command == "offer_task" {
            let Some(selected_task_archetype_id) = selected_task_archetype_id else {
                return tactics_command_rejection_json(
                    command,
                    unit_id,
                    target_tile,
                    "npc_has_no_task_archetype",
                );
            };
            if trillionnium_task_archetype_by_id(selected_task_archetype_id).is_none() {
                return tactics_command_rejection_json(
                    command,
                    unit_id,
                    target_tile,
                    "unknown_task_archetype_id",
                );
            }
            if !allowed_task_archetype_ids
                .iter()
                .any(|allowed| allowed == &selected_task_archetype_id)
            {
                return json!({
                    "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                    "accepted": false,
                    "command": command,
                    "unit_id": unit_id,
                    "npc_id": requested_npc_id,
                    "task_archetype_id": selected_task_archetype_id,
                    "allowed_task_archetype_ids": allowed_task_archetype_ids,
                    "result": "task_not_offered_by_npc",
                    "rejection_reason": "npc_task_capability_mismatch",
                    "source_of_truth": "rust_trillionnium_task_offer_validator",
                    "web_role": "intent_only_visualization_input",
                });
            }
        }
        character.title = if command == "offer_task" {
            "受领Trillionnium任务".to_string()
        } else {
            "Trillionnium有约".to_string()
        };
        character.updated_at_epoch = now_epoch;
        return json!({
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
            "accepted": true,
            "command": command,
            "unit_id": unit_id,
            "target_tile": target_tile,
            "npc_id": requested_npc_id,
            "npc_contract_version": TRILLIONNIUM_NPC_CONTRACT_VERSION,
            "npc_spawn_contract_version": TRILLIONNIUM_NPC_SPAWN_CONTRACT_VERSION,
            "npc_command_descriptor_contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
            "task_archetype_id": selected_task_archetype_id,
            "task_archetype_contract_version": TRILLIONNIUM_TASK_ARCHETYPE_CONTRACT_VERSION,
            "required_osm_game_overlay_id": required_overlay_id,
            "validation_owner": descriptor.get("validation_owner").cloned().unwrap_or_else(|| json!("rust_trillionnium_npc_interaction_validator")),
            "result": if command == "offer_task" { "task_offer_recorded" } else { "npc_talk_recorded" },
            "state_mutation": if command == "offer_task" { "world_task_offer_event_recorded" } else { "npc_relationship_event_recorded" },
            "source_of_truth": if command == "offer_task" { "rust_trillionnium_task_offer_validator" } else { "rust_trillionnium_npc_interaction_validator" },
            "web_role": "intent_only_visualization_input",
            "updated_at_epoch": now_epoch,
        });
    }

    if command != "train_skill" {
        if let Some(required_skill_id) = required_skill_id.as_deref() {
            if !known_skills.contains(required_skill_id) {
                return json!({
                    "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                    "accepted": false,
                    "command": command,
                    "unit_id": unit_id,
                    "target_tile": target_tile,
                    "required_skill_id": required_skill_id,
                    "result": "skill_locked",
                    "rejection_reason": "required_skill_not_known",
                    "source_of_truth": "rust_tactics_command_validator",
                    "web_role": "intent_only_visualization_input",
                });
            }
        }
        if command == "attack" {
            let attack_skill_id = skill_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(required_skill_id.as_deref())
                .unwrap_or("basic_unarmed");
            let character_attributes = character.attributes.clone();
            character.title = "街巷交锋".to_string();
            character.updated_at_epoch = now_epoch;
            let Some(combat_resolution) = tactics_combat_resolution_json(
                world,
                matrix_user_id,
                unit_id,
                target_tile,
                attack_skill_id,
                &character_attributes,
                now_epoch,
            ) else {
                return tactics_command_rejection_json(
                    command,
                    unit_id,
                    target_tile,
                    "no_target_unit_at_tile",
                );
            };
            if let Some(session) = latest_world_tactics_session_for_user(world, matrix_user_id) {
                if session.victory_state == "victory" && session.reward_status == "settled" {
                    return json!({
                        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                        "accepted": false,
                        "command": command,
                        "unit_id": unit_id,
                        "target_tile": target_tile,
                        "required_skill_id": required_skill_id,
                        "combat_resolution_contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
                        "combat_resolution": combat_resolution,
                        "result": "repeat_farming_blocked",
                        "rejection_reason": "tactics_objective_reward_already_settled",
                        "anti_cheese_contract_version": TRILLIONNIUM_TACTICS_REPEAT_FARMING_ANTI_CHEESE_CONTRACT_VERSION,
                        "anti_cheese_gate_enforced": true,
                        "session_id": session.session_id,
                        "objective_id": session.objective_id,
                        "victory_state": session.victory_state,
                        "reward_status": session.reward_status,
                        "source_of_truth": "rust_tactics_repeat_farming_guard",
                        "web_role": "intent_only_visualization_input",
                    });
                }
            }
            return json!({
                "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                "accepted": true,
                "command": command,
                "unit_id": unit_id,
                "target_tile": target_tile,
                "required_skill_id": required_skill_id,
                "combat_resolution_contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
                "combat_resolution": combat_resolution,
                "validation_owner": descriptor.get("validation_owner").cloned().unwrap_or_else(|| json!("rust_tactics_combat_handler")),
                "result": "tactics_combat_resolved",
                "state_mutation": "world_tactics_combat_event_recorded",
                "source_of_truth": "rust_tactics_combat_handler",
                "web_role": "intent_only_visualization_input",
                "updated_at_epoch": now_epoch,
            });
        }
        character.updated_at_epoch = now_epoch;
        return json!({
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
            "accepted": true,
            "command": command,
            "unit_id": unit_id,
            "target_tile": target_tile,
            "required_skill_id": required_skill_id,
            "validation_owner": descriptor.get("validation_owner").cloned().unwrap_or_else(|| json!("rust_tactics_command_validator")),
            "result": "tactics_command_accepted",
            "state_mutation": "world_tactics_event_recorded",
            "source_of_truth": "rust_tactics_command_validator",
            "web_role": "intent_only_visualization_input",
            "updated_at_epoch": now_epoch,
        });
    }

    let requested_skill_id = skill_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("basic_unarmed");
    let Some(skill) = trillionnium_skill_definition_by_id(requested_skill_id) else {
        return tactics_command_rejection_json(command, unit_id, target_tile, "unknown_skill_id");
    };
    let Some(training_command) = trillionnium_training_command_for_skill(requested_skill_id) else {
        return tactics_command_rejection_json(
            command,
            unit_id,
            target_tile,
            "skill_has_no_training_command",
        );
    };
    let nodes: Vec<WorldMapNode> = world.world_map_nodes.values().cloned().collect();
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);
    let training_commands = trillionnium_training_commands_json(&geodata);
    let training_descriptor = training_commands
        .as_array()
        .and_then(|commands| {
            commands.iter().find(|candidate| {
                candidate
                    .get("skill_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == requested_skill_id)
            })
        })
        .cloned()
        .unwrap_or_else(|| training_command.to_value(&[]));
    let required_overlay_id = training_descriptor
        .get("required_osm_game_overlay_id")
        .and_then(Value::as_str)
        .unwrap_or("trillionnium-world-node:mirror-city-square");
    if let Some(provided_overlay_id) = osm_game_overlay_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if provided_overlay_id != required_overlay_id {
            return json!({
                "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                "accepted": false,
                "command": command,
                "unit_id": unit_id,
                "skill_id": requested_skill_id,
                "required_osm_game_overlay_id": required_overlay_id,
                "provided_osm_game_overlay_id": provided_overlay_id,
                "result": "training_place_mismatch",
                "rejection_reason": "mentor_training_requires_matching_osm_place",
                "source_of_truth": "rust_mentor_training_validator",
                "web_role": "intent_only_visualization_input",
            });
        }
    }

    let already_known = character
        .skill_ids
        .iter()
        .any(|known| known == requested_skill_id);
    if !already_known {
        character.skill_ids.push(requested_skill_id.to_string());
    }
    character.title = if character.sect_id.is_some() {
        "门内行走".to_string()
    } else {
        "得授新艺".to_string()
    };
    character.updated_at_epoch = now_epoch;
    json!({
        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
        "accepted": true,
        "command": command,
        "unit_id": unit_id,
        "target_tile": target_tile,
        "skill_id": requested_skill_id,
        "skill_contract_version": TRILLIONNIUM_SKILL_CONTRACT_VERSION,
        "skill_family": skill.family,
        "mentor_npc_id": training_command.mentor_npc_id,
        "mentor_training_task_flow_id": format!("mentor-training:{}", requested_skill_id),
        "mentor_training_task_contract_version": TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION,
        "required_semantic_role": training_command.required_semantic_role,
        "required_osm_game_overlay_id": required_overlay_id,
        "cost_xp": training_command.cost_xp,
        "cooldown_seconds": training_command.cooldown_seconds,
        "result": if already_known { "skill_already_known" } else { "skill_trained" },
        "state_mutation": if already_known { "training_event_recorded" } else { "character_skill_added" },
        "source_of_truth": "rust_mentor_training_validator",
        "web_role": "intent_only_visualization_input",
        "updated_at_epoch": now_epoch,
    })
}

pub(super) fn world_tactics_board_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
    current_node: Option<&WorldMapNode>,
    openstreetmap_geodata: &Value,
) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let current_overlay_id = current_node.map(openstreetmap_game_overlay_id);
    let market_overlay_id = feature_overlay_id(feature_for_role(&features, "market"));
    let arena_overlay_id = feature_overlay_id(feature_for_role(&features, "arena"));
    let mentor_overlay_id = feature_overlay_id(feature_for_role(&features, "mentor_home"));
    let objective_overlay_id = market_overlay_id
        .clone()
        .or_else(|| current_overlay_id.clone())
        .unwrap_or_else(|| "trillionnium-world-node:mirror-city-square".to_string());
    let mut cells = Vec::with_capacity(64);
    for row in 0..8 {
        for col in 0..8 {
            let tile_label = tactics_tile_label(row, col);
            let terrain = tactics_terrain_for(row, col);
            let overlay_id = match terrain {
                "objective" => Some(objective_overlay_id.as_str()),
                "market" => market_overlay_id.as_deref(),
                "camp" => mentor_overlay_id.as_deref(),
                "road" => current_overlay_id.as_deref(),
                _ => None,
            };
            cells.push(json!({
                "tile_id": tile_label,
                "row": row + 1,
                "col": col + 1,
                "grid_row": row + 1,
                "grid_column": col + 1,
                "terrain": terrain,
                "source_of_truth": "rust_tactics_board_projection",
                "osm_game_overlay_id": overlay_id,
                "overlay_identity_ref": overlay_id,
                "board_cell_interaction_contract_version": TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION,
                "command_intent_draft_contract_version": TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION,
                "accessibility_contract_version": TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION,
                "selectable": true,
                "selection_role": "target_tile",
                "keyboard_focus_role": "target_tile_gridcell",
                "draft_input_name": "target_tile",
                "aria_role": "gridcell",
                "validation_owner": "rust_tactics_command_validator",
                "web_role": "intent_only_visualization_input",
                "movement_cost": match terrain {
                    "road" => 1,
                    "plain" => 1,
                    "market" => 1,
                    "camp" => 1,
                    "objective" => 1,
                    "forest" => 2,
                    "river" => 3,
                    _ => 1,
                },
                "blocks_line_of_sight": terrain == "forest",
            }));
        }
    }
    let trillionnium_character =
        world_trillionnium_character_projection_json(world, matrix_user_id);
    let units = tactics_units_json(
        matrix_user_id,
        &trillionnium_character,
        arena_overlay_id.clone(),
        market_overlay_id.clone(),
    );
    let available_commands = tactics_available_commands_json();
    let training_commands = trillionnium_training_commands_json(openstreetmap_geodata);
    let sects = trillionnium_sect_fixtures_json(openstreetmap_geodata);
    let npcs = trillionnium_npc_fixtures_json(world, matrix_user_id, openstreetmap_geodata);
    let npc_spawn_anchors = trillionnium_npc_spawn_anchors_json(&npcs);
    let npc_command_descriptors = trillionnium_npc_command_descriptors_from_npcs_json(&npcs);
    let mentor_training_task_flows =
        trillionnium_mentor_training_task_flows_json(openstreetmap_geodata);
    let task_archetypes = trillionnium_task_archetypes_json(openstreetmap_geodata);
    let task_candidates = trillionnium_task_candidates_json(&task_archetypes);
    let osm_objectives =
        trillionnium_osm_objectives_json(world, matrix_user_id, openstreetmap_geodata);
    let osm_objective_count = osm_objectives
        .as_array()
        .map(|objectives| objectives.len())
        .unwrap_or(0);
    let map_overlay_identity_index = world_map_overlay_identity_index_json(openstreetmap_geodata);
    let map_overlay_identity_count = map_overlay_identity_index
        .as_array()
        .map(|identities| identities.len())
        .unwrap_or(0);
    let game_session =
        world_tactics_game_session_projection_json(world, matrix_user_id, current_node);
    let simulation_ticks = world_tactics_simulation_tick_log_json(world, matrix_user_id);
    let simulation_tick_count = simulation_ticks
        .as_array()
        .map(|ticks| ticks.len())
        .unwrap_or(0);
    let combat_log = trillionnium_combat_log_json(
        &objective_overlay_id,
        &trillionnium_character,
        &task_candidates,
    );
    let battle_log = trillionnium_battle_log_lines_json(&combat_log);
    json!({
        "contract_version": TRILLIONNIUM_TACTICS_BOARD_CONTRACT_VERSION,
        "source_of_truth": "rust_trillionnium_game_state",
        "web_role": "visualization_input_only",
        "unit_contract_version": TRILLIONNIUM_TACTICS_UNIT_CONTRACT_VERSION,
        "command_contract_version": TRILLIONNIUM_TACTICS_COMMAND_CONTRACT_VERSION,
        "command_outcome_contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
        "trillionnium_skill_contract_version": TRILLIONNIUM_SKILL_CONTRACT_VERSION,
        "trillionnium_training_contract_version": TRILLIONNIUM_TRAINING_CONTRACT_VERSION,
        "trillionnium_sect_contract_version": TRILLIONNIUM_SECT_CONTRACT_VERSION,
        "trillionnium_npc_contract_version": TRILLIONNIUM_NPC_CONTRACT_VERSION,
        "trillionnium_sect_osm_binding_contract_version": TRILLIONNIUM_SECT_OSM_BINDING_CONTRACT_VERSION,
        "trillionnium_npc_spawn_contract_version": TRILLIONNIUM_NPC_SPAWN_CONTRACT_VERSION,
        "trillionnium_npc_command_descriptor_contract_version": TRILLIONNIUM_NPC_COMMAND_DESCRIPTOR_CONTRACT_VERSION,
        "mentor_training_task_contract_version": TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION,
        "trillionnium_task_archetype_contract_version": TRILLIONNIUM_TASK_ARCHETYPE_CONTRACT_VERSION,
        "trillionnium_task_completion_contract_version": TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION,
        "trillionnium_reward_gate_contract_version": TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION,
        "trillionnium_battle_log_style_contract_version": TRILLIONNIUM_BATTLE_LOG_STYLE_CONTRACT_VERSION,
        "trillionnium_combat_log_contract_version": TRILLIONNIUM_COMBAT_LOG_CONTRACT_VERSION,
        "trillionnium_npc_relationship_contract_version": TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION,
        "trillionnium_osm_objective_contract_version": TRILLIONNIUM_OSM_OBJECTIVE_CONTRACT_VERSION,
        "tactics_combat_resolution_contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
        "tactics_game_session_contract_version": TRILLIONNIUM_TACTICS_GAME_SESSION_CONTRACT_VERSION,
        "tactics_simulation_tick_contract_version": TRILLIONNIUM_TACTICS_SIMULATION_TICK_CONTRACT_VERSION,
        "tactics_reward_settlement_contract_version": TRILLIONNIUM_TACTICS_REWARD_SETTLEMENT_CONTRACT_VERSION,
        "tactics_repeat_farming_anti_cheese_contract_version": TRILLIONNIUM_TACTICS_REPEAT_FARMING_ANTI_CHEESE_CONTRACT_VERSION,
        "tactics_board_cell_interaction_contract_version": TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION,
        "tactics_unit_selection_contract_version": TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION,
        "tactics_command_intent_draft_contract_version": TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION,
        "tactics_accessibility_contract_version": TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION,
        "map_overlay_identity_contract_version": TRILLIONNIUM_MAP_OVERLAY_IDENTITY_CONTRACT_VERSION,
        "intent_draft_policy": {
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_INTENT_DRAFT_CONTRACT_VERSION,
            "board_cell_interaction_contract_version": TRILLIONNIUM_TACTICS_BOARD_CELL_INTERACTION_CONTRACT_VERSION,
            "unit_selection_contract_version": TRILLIONNIUM_TACTICS_UNIT_SELECTION_CONTRACT_VERSION,
            "draft_owner": "browser_tactics_intent_builder",
            "validation_owner": "rust_tactics_command_validator",
            "command_handler_owner": "rust_world_tactics_command_handler",
            "web_role": "intent_only_visualization_input",
            "browser_may_select": ["unit_id", "target_tile", "command"],
            "browser_may_not_resolve": ["movement_legality", "combat_result", "reward_status", "objective_completion"],
            "source_of_truth": "rust_trillionnium_game_state"
        },
        "accessibility_policy": {
            "contract_version": TRILLIONNIUM_TACTICS_ACCESSIBILITY_CONTRACT_VERSION,
            "keyboard_traversal": "roving_grid_focus",
            "keyboard_keys": ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "Enter", "Space"],
            "low_motion_support": "prefers_reduced_motion",
            "live_region_owner": "world_tactics_command_draft_status",
            "focus_owner": "browser_tactics_keyboard_controller",
            "source_of_truth": "rust_trillionnium_game_state",
            "web_role": "accessibility_view_contract_only"
        },
        "open_source_base": {
            "repo": "tranchikhang/MedievalWar",
            "license": "MIT",
            "engine": "Phaser 3",
            "borrowed_patterns": ["map", "cursor", "control", "turn_system", "pathfinding", "context_menu", "objectives", "ai"],
            "asset_policy": "no_proprietary_assets_css_tokens_first"
        },
        "board": {
            "board_id": "mirror-street-tactics-board-v1",
            "width": 8,
            "height": 8,
            "coordinate_system": "A1_to_H8",
            "cells": cells,
        },
        "trillionnium_character": trillionnium_character,
        "skill_definitions": trillionnium_skill_definitions_json(),
        "training_commands": training_commands,
        "sects": sects,
        "npcs": npcs,
        "npc_spawn_anchors": npc_spawn_anchors,
        "npc_command_descriptors": npc_command_descriptors,
        "mentor_training_task_flows": mentor_training_task_flows,
        "task_archetypes": task_archetypes,
        "task_candidates": task_candidates,
        "osm_objectives": osm_objectives.clone(),
        "map_overlay_identity_index": map_overlay_identity_index,
        "game_session": game_session,
        "simulation_ticks": simulation_ticks,
        "battle_log_style": trillionnium_battle_log_style_json(),
        "combat_log": combat_log,
        "npc_relationship_model": {
            "contract_version": TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION,
            "source_of_truth": "rust_world_relationships_persistent_state",
            "relationship_seed_owner": "rust_trillionnium_game_state",
            "relationship_event_owner": "world_relationships",
            "web_role": "visualization_input_only"
        },
        "units": units,
        "objectives": osm_objectives,
        "available_commands": available_commands,
        "turn_state": {
            "contract_version": "trillionnium_world_tactics_turn_state_v1",
            "active_side": "player",
            "active_unit_id": "lord",
            "round": 1,
            "action_points_remaining": 2,
            "source_of_truth": "rust_tactics_turn_handler",
            "allowed_commands": ["select_unit", "move_unit", "attack", "use_skill", "train_skill", "talk_npc", "offer_task", "complete_task", "interact", "end_turn"],
        },
        "battle_log": battle_log,
        "osm_objective_source": {
            "provider_contract": "OpenStreetMapDataProvider",
            "geodata_contract_version": OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
            "objective_overlay_id": objective_overlay_id,
            "objective_contract_version": TRILLIONNIUM_OSM_OBJECTIVE_CONTRACT_VERSION,
            "objective_count": osm_objective_count,
            "map_overlay_identity_contract_version": TRILLIONNIUM_MAP_OVERLAY_IDENTITY_CONTRACT_VERSION,
            "map_overlay_identity_count": map_overlay_identity_count,
            "osm_can_suggest_objectives": true,
            "rust_command_handler_decides_completion": true
        },
        "simulation_tick_source": {
            "session_contract_version": TRILLIONNIUM_TACTICS_GAME_SESSION_CONTRACT_VERSION,
            "tick_contract_version": TRILLIONNIUM_TACTICS_SIMULATION_TICK_CONTRACT_VERSION,
            "tick_count": simulation_tick_count,
            "source_of_truth": "rust_tactics_simulation_tick",
            "persistence_owner": "world_state.world_tactics_simulation_ticks"
        },
        "repeat_farming_anti_cheese_policy": {
            "contract_version": TRILLIONNIUM_TACTICS_REPEAT_FARMING_ANTI_CHEESE_CONTRACT_VERSION,
            "gate_owner": "rust_tactics_repeat_farming_guard",
            "policy": "one objective reward per settled tactics session until a new route/objective is issued",
            "repeat_attack_after_settlement": "blocked",
            "reward_history_owner": "route_task_graph_and_route_runner_history",
            "web_role": "visualization_input_only"
        }
    })
}
