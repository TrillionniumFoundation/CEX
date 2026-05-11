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
pub(super) const TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION: &str =
    "trillionnium_world_objective_travel_v1";
pub(super) const TRILLIONNIUM_WORLD_SKILL_PRACTICE_LOOP_CONTRACT_VERSION: &str =
    "trillionnium_world_skill_practice_loop_v1";
pub(super) const TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION: &str =
    "trillionnium_world_combat_encounter_loop_v1";
pub(super) const TRILLIONNIUM_HERO_TAN_FULL_CONTENT_ALIGNMENT_CONTRACT_VERSION: &str =
    "trillionnium_hero_tan_full_content_alignment_v1";
pub(super) const TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION: &str =
    "trillionnium_world_item_equipment_runtime_v1";
pub(super) const TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION: &str =
    "trillionnium_world_resource_pressure_runtime_v1";

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
        "talk_npc" | "select_unit" | "inspect_osm_underlay" | "equip_item" | "end_turn" => 0,
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
        TrillionniumSkillDefinition {
            skill_id: "staff_and_polearm",
            family: "staff",
            name: "Route Guard Staff / 护路棍法",
            level: 1,
            xp: 70,
            unlock_condition: "escort_route_training",
            combat_effect: "extend_melee_zone_control",
            world_effect: "improve_escort_and_patrol_task_safety",
            training_anchor_role: "delivery_route",
        },
        TrillionniumSkillDefinition {
            skill_id: "evidence_packaging",
            family: "evidence",
            name: "Evidence Packaging / 证据封装",
            level: 1,
            xp: 110,
            unlock_condition: "submit_first_task_report",
            combat_effect: "preserve_objective_proof_under_pressure",
            world_effect: "improve_review_hold_release_quality",
            training_anchor_role: "quest_board",
        },
        TrillionniumSkillDefinition {
            skill_id: "healing_tonic_craft",
            family: "medicine",
            name: "Tonic Craft / 补剂调配",
            level: 1,
            xp: 85,
            unlock_condition: "meet_field_apothecary",
            combat_effect: "recover_minor_wounds_after_encounter",
            world_effect: "reduce_party_downtime_after_failed_tasks",
            training_anchor_role: "mentor_home",
        },
        TrillionniumSkillDefinition {
            skill_id: "route_scouting",
            family: "scouting",
            name: "Route Scouting / 路线侦察",
            level: 1,
            xp: 95,
            unlock_condition: "complete_first_delivery_route",
            combat_effect: "preview_enemy_position_before_entry",
            world_effect: "improve_objective_travel_next_step_quality",
            training_anchor_role: "delivery_route",
        },
        TrillionniumSkillDefinition {
            skill_id: "dispute_mediation",
            family: "mediation",
            name: "Dispute Mediation / 纠纷调停",
            level: 1,
            xp: 130,
            unlock_condition: "inspect_arbitration_desk",
            combat_effect: "convert_some_hostile_events_to_negotiation",
            world_effect: "increase_npc_trust_and_reputation_recovery",
            training_anchor_role: "arbitration_desk",
        },
        TrillionniumSkillDefinition {
            skill_id: "raid_coordination",
            family: "raid_command",
            name: "Raid Coordination / 会战号令",
            level: 1,
            xp: 160,
            unlock_condition: "enter_raid_hall",
            combat_effect: "improve_party_focus_on_objective_tiles",
            world_effect: "unlock_group_route_and_raid_hall_tasks",
            training_anchor_role: "raid_hall",
        },
        TrillionniumSkillDefinition {
            skill_id: "artifact_appraisal",
            family: "appraisal",
            name: "Artifact Appraisal / 器物鉴定",
            level: 1,
            xp: 105,
            unlock_condition: "inspect_workshop_or_market_item",
            combat_effect: "identify_item_quality_before_use",
            world_effect: "improve_market_listing_quality_and_repair_routes",
            training_anchor_role: "workshop",
        },
        TrillionniumSkillDefinition {
            skill_id: "terrain_reading",
            family: "terrain",
            name: "Terrain Reading / 地势辨读",
            level: 1,
            xp: 100,
            unlock_condition: "move_across_three_world_nodes",
            combat_effect: "reduce_forest_and_river_movement_penalty",
            world_effect: "improve_map_transition_and_blocked_terrain_guidance",
            training_anchor_role: "civic_square",
        },
        TrillionniumSkillDefinition {
            skill_id: "auction_sense",
            family: "auction",
            name: "Auction Sense / 拍卖眼力",
            level: 1,
            xp: 115,
            unlock_condition: "visit_market_listing_board",
            combat_effect: "turn_supply_tiles_into_temporary_focus",
            world_effect: "improve_bounty_pricing_and_seller_selection",
            training_anchor_role: "market",
        },
        TrillionniumSkillDefinition {
            skill_id: "camp_cooking",
            family: "survival",
            name: "Camp Cooking / 行灶术",
            level: 1,
            xp: 75,
            unlock_condition: "rest_after_long_route",
            combat_effect: "restore_focus_before_next_encounter",
            world_effect: "reduce_stamina_pressure_on_long_routes",
            training_anchor_role: "mentor_home",
        },
        TrillionniumSkillDefinition {
            skill_id: "shadow_messaging",
            family: "messaging",
            name: "Shadow Messaging / 暗信步",
            level: 1,
            xp: 125,
            unlock_condition: "complete_night_watch_patrol",
            combat_effect: "delay_enemy_reinforcement_signal",
            world_effect: "unlock_discreet_courier_and_witness_routes",
            training_anchor_role: "quest_board",
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
pub(super) struct WorldTrillionniumInventoryItem {
    pub(super) item_instance_id: String,
    pub(super) item_id: String,
    pub(super) slot: String,
    pub(super) family: String,
    pub(super) display_name: String,
    pub(super) quantity: u16,
    pub(super) quality: String,
    pub(super) equipped_slot: Option<String>,
    pub(super) acquired_from: String,
    pub(super) acquired_at_epoch: i64,
    pub(super) updated_at_epoch: i64,
}

fn trillionnium_catalog_item_field(item_id: &str, field: &str) -> Option<String> {
    trillionnium_item_equipment_catalog_json()
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("item_id")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate == item_id)
            })
        })
        .and_then(|item| item.get(field).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn trillionnium_inventory_item_for(
    matrix_user_id: &str,
    item_id: &str,
    acquired_from: &str,
    equipped_slot: Option<&str>,
    now_epoch: i64,
) -> Option<WorldTrillionniumInventoryItem> {
    let slot = trillionnium_catalog_item_field(item_id, "slot")?;
    let family = trillionnium_catalog_item_field(item_id, "family")?;
    let display_name = trillionnium_catalog_item_field(item_id, "display_name")?;
    Some(WorldTrillionniumInventoryItem {
        item_instance_id: league_hash_id(
            "world-trillionnium-inventory-item",
            &format!("{matrix_user_id}:{item_id}"),
        ),
        item_id: item_id.to_string(),
        slot,
        family,
        display_name,
        quantity: 1,
        quality: "starter".to_string(),
        equipped_slot: equipped_slot.map(ToString::to_string),
        acquired_from: acquired_from.to_string(),
        acquired_at_epoch: now_epoch,
        updated_at_epoch: now_epoch,
    })
}

fn default_trillionnium_inventory_items(
    matrix_user_id: &str,
    now_epoch: i64,
) -> Vec<WorldTrillionniumInventoryItem> {
    [
        ("route-guard-staff", Some("weapon")),
        ("street-compass-bracer", Some("wrist")),
        ("evidence-wrap-case", Some("pack")),
    ]
    .into_iter()
    .filter_map(|(item_id, equipped_slot)| {
        trillionnium_inventory_item_for(
            matrix_user_id,
            item_id,
            "rust_default_trillionnium_starter_loadout",
            equipped_slot,
            now_epoch,
        )
    })
    .collect()
}

fn default_trillionnium_equipment_slots(
    inventory_items: &[WorldTrillionniumInventoryItem],
) -> HashMap<String, String> {
    inventory_items
        .iter()
        .filter_map(|item| {
            item.equipped_slot
                .as_ref()
                .map(|slot| (slot.clone(), item.item_instance_id.clone()))
        })
        .collect()
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
pub(super) struct WorldTrillionniumResourcePressureMutation {
    pub(super) event_kind: String,
    pub(super) command: String,
    pub(super) time_delta_minutes: i64,
    pub(super) stamina_delta: i64,
    pub(super) injury_delta: i64,
    pub(super) evidence_integrity_delta: i64,
    pub(super) evidence_fragment_delta: i64,
    pub(super) source_of_truth: String,
    pub(super) created_at_epoch: i64,
}

impl WorldTrillionniumResourcePressureMutation {
    fn to_value(&self) -> Value {
        json!({
            "event_kind": &self.event_kind,
            "command": &self.command,
            "time_delta_minutes": self.time_delta_minutes,
            "stamina_delta": self.stamina_delta,
            "injury_delta": self.injury_delta,
            "evidence_integrity_delta": self.evidence_integrity_delta,
            "evidence_fragment_delta": self.evidence_fragment_delta,
            "source_of_truth": &self.source_of_truth,
            "created_at_epoch": self.created_at_epoch,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorldTrillionniumResourcePressureState {
    pub(super) day_index: i64,
    pub(super) minute_of_day: i64,
    pub(super) stamina_current: i64,
    pub(super) stamina_max: i64,
    pub(super) injury_level: i64,
    pub(super) evidence_integrity: i64,
    pub(super) evidence_fragments: i64,
    pub(super) mutation_count: i64,
    pub(super) last_mutation_command: Option<String>,
    pub(super) last_mutation_event: Option<String>,
    pub(super) last_mutation_result: Option<String>,
    pub(super) updated_at_epoch: i64,
    #[serde(default)]
    pub(super) recent_mutations: Vec<WorldTrillionniumResourcePressureMutation>,
}

impl Default for WorldTrillionniumResourcePressureState {
    fn default() -> Self {
        Self {
            day_index: 1,
            minute_of_day: 8 * 60,
            stamina_current: 100,
            stamina_max: 100,
            injury_level: 0,
            evidence_integrity: 72,
            evidence_fragments: 0,
            mutation_count: 0,
            last_mutation_command: None,
            last_mutation_event: None,
            last_mutation_result: None,
            updated_at_epoch: 0,
            recent_mutations: Vec::new(),
        }
    }
}

impl WorldTrillionniumResourcePressureState {
    fn ensure_defaults(&mut self) {
        if self.day_index <= 0 {
            self.day_index = 1;
        }
        if !(0..(24 * 60)).contains(&self.minute_of_day) {
            self.minute_of_day = 8 * 60;
        }
        if self.stamina_max <= 0 {
            self.stamina_max = 100;
        }
        self.stamina_current = self.stamina_current.clamp(0, self.stamina_max);
        self.injury_level = self.injury_level.clamp(0, 4);
        if self.evidence_integrity <= 0 {
            self.evidence_integrity = 72;
        }
        self.evidence_integrity = self.evidence_integrity.clamp(0, 100);
        self.evidence_fragments = self.evidence_fragments.max(0);
    }

    fn clock_label(&self) -> String {
        format!(
            "{:02}:{:02}",
            self.minute_of_day / 60,
            self.minute_of_day % 60
        )
    }

    fn stamina_status(&self) -> &'static str {
        if self.stamina_current <= 20 {
            "exhausted_risk"
        } else if self.stamina_current <= 45 {
            "strained"
        } else {
            "route_ready"
        }
    }

    fn injury_status(&self) -> &'static str {
        match self.injury_level {
            0 => "clear",
            1 => "bruised",
            2 => "wounded",
            3 => "downtime_recommended",
            _ => "must_recover_before_risk_route",
        }
    }

    fn evidence_status(&self) -> &'static str {
        if self.evidence_integrity >= 82 && self.evidence_fragments >= 3 {
            "review_ready"
        } else if self.evidence_integrity >= 62 {
            "draft_evidence_bundle"
        } else {
            "review_hold_risk"
        }
    }

    fn apply_mutation(
        &mut self,
        event_kind: &str,
        command: &str,
        result: Option<&str>,
        now_epoch: i64,
    ) -> Value {
        self.ensure_defaults();
        let (
            time_delta_minutes,
            stamina_delta,
            injury_delta,
            evidence_integrity_delta,
            evidence_fragment_delta,
        ) = match event_kind {
            "world_map_move" => (12, -4, 0, 1, 1),
            "tactics_attack" => {
                let injury_delta = if result == Some("defender_routed") {
                    0
                } else {
                    1
                };
                (8, -14, injury_delta, -2, 0)
            }
            "tactics_complete_task" => (18, -6, -1, 12, 3),
            _ => (4, -1, 0, 0, 0),
        };
        let old_minute = self.minute_of_day;
        let absolute_minute = self.minute_of_day + time_delta_minutes;
        self.day_index += absolute_minute.div_euclid(24 * 60);
        self.minute_of_day = absolute_minute.rem_euclid(24 * 60);
        self.stamina_current = (self.stamina_current + stamina_delta).clamp(0, self.stamina_max);
        self.injury_level = (self.injury_level + injury_delta).clamp(0, 4);
        self.evidence_integrity =
            (self.evidence_integrity + evidence_integrity_delta).clamp(0, 100);
        self.evidence_fragments = (self.evidence_fragments + evidence_fragment_delta).max(0);
        self.mutation_count += 1;
        self.last_mutation_command = Some(command.to_string());
        self.last_mutation_event = Some(event_kind.to_string());
        self.last_mutation_result = result.map(ToString::to_string);
        self.updated_at_epoch = now_epoch;
        let mutation = WorldTrillionniumResourcePressureMutation {
            event_kind: event_kind.to_string(),
            command: command.to_string(),
            time_delta_minutes,
            stamina_delta,
            injury_delta,
            evidence_integrity_delta,
            evidence_fragment_delta,
            source_of_truth: "rust_trillionnium_resource_pressure_runtime_state".to_string(),
            created_at_epoch: now_epoch,
        };
        self.recent_mutations.push(mutation.clone());
        if self.recent_mutations.len() > 8 {
            let overflow = self.recent_mutations.len() - 8;
            self.recent_mutations.drain(0..overflow);
        }
        json!({
            "contract_version": TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION,
            "source_of_truth": "rust_trillionnium_resource_pressure_runtime_state",
            "runtime_status": "rust_owned_time_stamina_injury_evidence_live",
            "mutation_event": event_kind,
            "command": command,
            "result": result,
            "mutation": mutation.to_value(),
            "previous_minute_of_day": old_minute,
            "resource_pressure_runtime": self.to_value(),
            "web_role": "visualization_input_only",
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION,
            "source_of_truth": "rust_trillionnium_resource_pressure_runtime_state",
            "persistence_owner": "world_state.world_trillionnium_characters.resource_pressure_state",
            "runtime_status": "rust_owned_time_stamina_injury_evidence_live",
            "tracked_domains": ["time", "stamina", "injury", "evidence_integrity"],
            "mutation_sources": ["world_map_move", "tactics_attack", "tactics_complete_task"],
            "time": {
                "day_index": self.day_index,
                "minute_of_day": self.minute_of_day,
                "clock_label": self.clock_label(),
            },
            "stamina": {
                "current": self.stamina_current,
                "max": self.stamina_max,
                "status": self.stamina_status(),
            },
            "injury": {
                "level": self.injury_level,
                "status": self.injury_status(),
            },
            "evidence_integrity": {
                "score": self.evidence_integrity,
                "fragments": self.evidence_fragments,
                "status": self.evidence_status(),
            },
            "mutation_count": self.mutation_count,
            "last_mutation_command": &self.last_mutation_command,
            "last_mutation_event": &self.last_mutation_event,
            "last_mutation_result": &self.last_mutation_result,
            "recent_mutations": self.recent_mutations.iter().map(WorldTrillionniumResourcePressureMutation::to_value).collect::<Vec<_>>(),
            "updated_at_epoch": self.updated_at_epoch,
            "web_role": "visualization_input_only",
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
    #[serde(default)]
    pub(super) inventory_items: Vec<WorldTrillionniumInventoryItem>,
    #[serde(default)]
    pub(super) equipment_slots: HashMap<String, String>,
    #[serde(default)]
    pub(super) resource_pressure_state: WorldTrillionniumResourcePressureState,
    pub(super) updated_at_epoch: i64,
}

impl WorldTrillionniumCharacter {
    pub(super) fn default_for(matrix_user_id: &str) -> Self {
        let inventory_items = default_trillionnium_inventory_items(matrix_user_id, 0);
        let equipment_slots = default_trillionnium_equipment_slots(&inventory_items);
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
            inventory_items,
            equipment_slots,
            resource_pressure_state: WorldTrillionniumResourcePressureState::default(),
            updated_at_epoch: 0,
        }
    }

    fn ensure_item_equipment_defaults(&mut self, now_epoch: i64) {
        if self.inventory_items.is_empty() {
            self.inventory_items =
                default_trillionnium_inventory_items(&self.matrix_user_id, now_epoch);
        }
        if self.equipment_slots.is_empty() {
            self.equipment_slots = default_trillionnium_equipment_slots(&self.inventory_items);
        }
    }

    fn ensure_resource_pressure_defaults(&mut self) {
        self.resource_pressure_state.ensure_defaults();
    }

    fn equip_item_by_id(&mut self, item_id: &str, now_epoch: i64) -> Option<(String, String)> {
        self.ensure_item_equipment_defaults(now_epoch);
        let (slot, item_instance_id) = {
            let item = self
                .inventory_items
                .iter_mut()
                .find(|candidate| candidate.item_id == item_id)?;
            item.updated_at_epoch = now_epoch;
            item.equipped_slot = Some(item.slot.clone());
            (item.slot.clone(), item.item_instance_id.clone())
        };
        self.equipment_slots
            .insert(slot.clone(), item_instance_id.clone());
        self.updated_at_epoch = now_epoch;
        Some((slot, item_instance_id))
    }

    fn item_equipment_runtime_json(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
            "source_of_truth": "rust_trillionnium_item_equipment_runtime_state",
            "persistence_owner": "world_state.world_trillionnium_characters.inventory_items_and_equipment_slots",
            "runtime_status": "rust_owned_inventory_and_equip_slots_live",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
            "inventory_count": self.inventory_items.len(),
            "equipped_slot_count": self.equipment_slots.len(),
            "inventory_items": &self.inventory_items,
            "equipment_slots": &self.equipment_slots,
            "allowed_mutation_commands": ["equip_item", "attack", "complete_task"],
            "web_role": "visualization_input_only",
        })
    }

    fn resource_pressure_runtime_json(&self) -> Value {
        let mut state = self.resource_pressure_state.clone();
        state.ensure_defaults();
        state.to_value()
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
            "item_equipment_runtime_contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
            "inventory_items": &self.inventory_items,
            "equipment_slots": &self.equipment_slots,
            "item_equipment_runtime": self.item_equipment_runtime_json(),
            "resource_pressure_runtime_contract_version": TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION,
            "resource_pressure_state": self.resource_pressure_runtime_json(),
            "resource_pressure_runtime": self.resource_pressure_runtime_json(),
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
                "streetwise_investigation",
                "staff_and_polearm",
                "evidence_packaging",
                "healing_tonic_craft",
                "route_scouting",
                "dispute_mediation",
                "raid_coordination",
                "artifact_appraisal",
                "terrain_reading",
                "auction_sense",
                "camp_cooking",
                "shadow_messaging"
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
    let mut character = world
        .world_trillionnium_characters
        .get(matrix_user_id)
        .cloned()
        .unwrap_or_else(|| WorldTrillionniumCharacter::default_for(matrix_user_id));
    character.ensure_item_equipment_defaults(0);
    character.ensure_resource_pressure_defaults();
    character.to_projection_json()
}

pub(super) fn apply_world_resource_pressure_mutation(
    world: &mut WorldState,
    matrix_user_id: &str,
    event_kind: &str,
    command: &str,
    result: Option<&str>,
    now_epoch: i64,
) -> Value {
    let character = world
        .world_trillionnium_characters
        .entry(matrix_user_id.to_string())
        .or_insert_with(|| WorldTrillionniumCharacter::default_for(matrix_user_id));
    character.ensure_item_equipment_defaults(now_epoch);
    character.ensure_resource_pressure_defaults();
    let mutation = character
        .resource_pressure_state
        .apply_mutation(event_kind, command, result, now_epoch);
    character.updated_at_epoch = now_epoch;
    mutation
}

pub(super) fn trillionnium_world_combat_encounter_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
    current_node: Option<&WorldMapNode>,
) -> Value {
    let encounter = current_node
        .map(world_combat_encounter_definition_for_node)
        .or_else(|| current_world_combat_encounter_definition(world, matrix_user_id));
    let latest_session = latest_world_tactics_session_for_user(world, matrix_user_id);
    let (return_state, session_id, victory_state, reward_status) = latest_session
        .map(|session| {
            let state = if session.status == "completed"
                || (session.victory_state == "victory" && session.reward_status == "settled")
            {
                "map_ready_after_resolution"
            } else if session.victory_state == "victory" {
                "reward_settlement_pending"
            } else if session.status == "active" {
                "encounter_active"
            } else {
                "map_ready"
            };
            (
                state,
                Some(session.session_id.clone()),
                session.victory_state.clone(),
                session.reward_status.clone(),
            )
        })
        .unwrap_or((
            "map_ready",
            None,
            "none".to_string(),
            "not_started".to_string(),
        ));
    let entry = encounter
        .as_ref()
        .map(WorldCombatEncounterDefinition::to_projection_json)
        .unwrap_or_else(|| {
            json!({
                "contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
                "available": false,
                "source_of_truth": "rust_world_combat_encounter_projection",
                "web_role": "intent_only_visualization_input",
            })
        });
    let (return_to_node_id, return_overlay_id) = encounter
        .as_ref()
        .map(|encounter| {
            (
                encounter.current_node_id.clone(),
                encounter.current_overlay_id.clone(),
            )
        })
        .unwrap_or_else(|| {
            (
                default_world_node_id().to_string(),
                format!("trillionnium-world-node:{}", default_world_node_id()),
            )
        });
    json!({
        "contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
        "source_of_truth": "rust_world_combat_encounter_projection",
        "validation_owner": "rust_world_combat_encounter_validator",
        "command_handler_owner": "rust_tactics_combat_handler",
        "return_state_owner": "rust_world_combat_encounter_return_state",
        "web_role": "intent_only_visualization_input",
        "entry": entry,
        "return_to_map": {
            "contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
            "return_state": return_state,
            "return_to_node_id": return_to_node_id,
            "return_overlay_id": return_overlay_id,
            "return_anchor": "world-keypad-adventure-shell",
            "latest_session_id": session_id,
            "victory_state": victory_state,
            "reward_status": reward_status,
            "source_of_truth": "rust_world_combat_encounter_return_state",
            "web_role": "visualization_only_intent_to_map_move",
        },
        "anti_cheese": {
            "contract_version": TRILLIONNIUM_TACTICS_REPEAT_FARMING_ANTI_CHEESE_CONTRACT_VERSION,
            "duplicate_settled_reward": "blocked_by_rust_tactics_repeat_farming_guard",
            "invalid_node_or_target": "fail_closed_by_rust_world_combat_encounter_validator",
            "web_role": "visualization_input_only",
        }
    })
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
        TrillionniumTrainingCommand {
            skill_id: "staff_and_polearm",
            mentor_npc_id: "npc-escort-captain-han",
            required_semantic_role: "delivery_route",
            cost_xp: 12,
            cooldown_seconds: 600,
        },
        TrillionniumTrainingCommand {
            skill_id: "evidence_packaging",
            mentor_npc_id: "npc-bounty-board-clerk",
            required_semantic_role: "quest_board",
            cost_xp: 15,
            cooldown_seconds: 840,
        },
        TrillionniumTrainingCommand {
            skill_id: "healing_tonic_craft",
            mentor_npc_id: "npc-field-apothecary",
            required_semantic_role: "mentor_home",
            cost_xp: 13,
            cooldown_seconds: 720,
        },
        TrillionniumTrainingCommand {
            skill_id: "route_scouting",
            mentor_npc_id: "npc-jade-route-scout",
            required_semantic_role: "delivery_route",
            cost_xp: 15,
            cooldown_seconds: 780,
        },
        TrillionniumTrainingCommand {
            skill_id: "dispute_mediation",
            mentor_npc_id: "npc-dispute-witness-lu",
            required_semantic_role: "arbitration_desk",
            cost_xp: 18,
            cooldown_seconds: 960,
        },
        TrillionniumTrainingCommand {
            skill_id: "raid_coordination",
            mentor_npc_id: "npc-raid-drum-sergeant",
            required_semantic_role: "raid_hall",
            cost_xp: 20,
            cooldown_seconds: 1200,
        },
        TrillionniumTrainingCommand {
            skill_id: "artifact_appraisal",
            mentor_npc_id: "npc-artifact-appraiser",
            required_semantic_role: "workshop",
            cost_xp: 14,
            cooldown_seconds: 780,
        },
        TrillionniumTrainingCommand {
            skill_id: "terrain_reading",
            mentor_npc_id: "npc-map-tile-surveyor",
            required_semantic_role: "civic_square",
            cost_xp: 12,
            cooldown_seconds: 600,
        },
        TrillionniumTrainingCommand {
            skill_id: "auction_sense",
            mentor_npc_id: "npc-warehouse-broker-xu",
            required_semantic_role: "market",
            cost_xp: 16,
            cooldown_seconds: 900,
        },
        TrillionniumTrainingCommand {
            skill_id: "camp_cooking",
            mentor_npc_id: "npc-camp-cook-lin",
            required_semantic_role: "mentor_home",
            cost_xp: 10,
            cooldown_seconds: 600,
        },
        TrillionniumTrainingCommand {
            skill_id: "shadow_messaging",
            mentor_npc_id: "npc-shadow-message-runner",
            required_semantic_role: "quest_board",
            cost_xp: 17,
            cooldown_seconds: 960,
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
        TrillionniumSectFixture {
            sect_id: "jade-route-agency",
            display_name: "Jade Route Agency / 玉路局",
            specialization: "route_scouting_patrol_and_caravan_escort",
            anchor_role: "delivery_route",
            mentor_npc_ids: vec!["npc-jade-route-scout", "npc-escort-captain-han"],
            entry_requirement: "complete_first_delivery_or_patrol_task",
            benefits: vec!["route_scouting_bonus", "escort_party_readiness"],
            title_ladder: vec!["route_runner", "jade_scout", "caravan_path_master"],
        },
        TrillionniumSectFixture {
            sect_id: "dispute-mirror-court",
            display_name: "Dispute Mirror Court / 明镜庭",
            specialization: "mediation_witness_handling_and_reputation_repair",
            anchor_role: "arbitration_desk",
            mentor_npc_ids: vec!["npc-dispute-witness-lu", "npc-sect-registrar-qin"],
            entry_requirement: "relationship_trust_or_dispute_mediation_training",
            benefits: vec!["npc_trust_recovery", "review_hold_release_hint"],
            title_ladder: vec!["case_listener", "mirror_clerk", "court_mediator"],
        },
        TrillionniumSectFixture {
            sect_id: "raid-signal-lodge",
            display_name: "Raid Signal Lodge / 号令楼",
            specialization: "combat_entry_party_coordination_and_raid_tasks",
            anchor_role: "raid_hall",
            mentor_npc_ids: vec!["npc-raid-drum-sergeant", "npc-arena-referee-du"],
            entry_requirement: "win_first_lightweight_encounter",
            benefits: vec!["raid_coordination_bonus", "combat_return_state_clarity"],
            title_ladder: vec!["signal_runner", "drum_captain", "raid_lodge_commander"],
        },
        TrillionniumSectFixture {
            sect_id: "field-remedy-garden",
            display_name: "Field Remedy Garden / 行药园",
            specialization: "medicine_recovery_and_failed_task_downtime_control",
            anchor_role: "mentor_home",
            mentor_npc_ids: vec!["npc-field-apothecary"],
            entry_requirement: "meet_field_apothecary_or_failed_encounter_recovery",
            benefits: vec!["minor_wound_recovery", "party_downtime_reduction"],
            title_ladder: vec!["herb_runner", "field_tonic_maker", "garden_healer"],
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
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "street_patrol",
            display_name: "Street Patrol / 街巡护路",
            source_semantic_roles: vec!["civic_square", "delivery_route"],
            command: "offer_task",
            completion_owner: "rust_world_action_handler",
            reward_gate: "patrol_report_review_hold_anti_cheese",
            log_style_key: "street_patrol",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "debt_recovery",
            display_name: "Debt Recovery / 清账追索",
            source_semantic_roles: vec!["ledger_hall", "market"],
            command: "offer_task",
            completion_owner: "rust_command_handler_ledger_progression",
            reward_gate: "ledger_settlement_and_dispute_evidence_required",
            log_style_key: "debt_recovery",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "map_survey",
            display_name: "Map Survey / 地图踏勘",
            source_semantic_roles: vec!["civic_square", "quest_board", "delivery_route"],
            command: "offer_task",
            completion_owner: "rust_world_graph_objective_travel",
            reward_gate: "route_evidence_required_before_reward",
            log_style_key: "map_survey",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "healing_supply",
            display_name: "Healing Supply / 行药补给",
            source_semantic_roles: vec!["mentor_home", "workshop"],
            command: "offer_task",
            completion_owner: "rust_world_action_handler",
            reward_gate: "supply_quality_review_required",
            log_style_key: "healing_supply",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "arbitrate_dispute",
            display_name: "Arbitrate Dispute / 调停纠纷",
            source_semantic_roles: vec!["arbitration_desk", "market"],
            command: "offer_task",
            completion_owner: "rust_trillionnium_task_completion_handler",
            reward_gate: "relationship_evidence_and_review_hold_gate",
            log_style_key: "arbitrate_dispute",
        },
        TrillionniumTaskArchetypeFixture {
            task_archetype_id: "raid_signal",
            display_name: "Raid Signal / 会战号令",
            source_semantic_roles: vec!["raid_hall", "arena"],
            command: "offer_task",
            completion_owner: "rust_tactics_turn_handler",
            reward_gate: "party_raid_resolution_then_ledger_gate",
            log_style_key: "raid_signal",
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

fn trillionnium_world_overlay_node_id(overlay_id: &str) -> Option<&str> {
    overlay_id.strip_prefix("trillionnium-world-node:")
}

fn world_graph_shortest_path(
    world: &WorldState,
    from_node_id: &str,
    to_node_id: &str,
) -> Vec<String> {
    if from_node_id == to_node_id && world.world_map_nodes.contains_key(from_node_id) {
        return vec![from_node_id.to_string()];
    }
    if !world.world_map_nodes.contains_key(from_node_id)
        || !world.world_map_nodes.contains_key(to_node_id)
    {
        return Vec::new();
    }

    let mut visited = HashSet::new();
    let mut previous: HashMap<String, String> = HashMap::new();
    let mut queue = VecDeque::new();
    visited.insert(from_node_id.to_string());
    queue.push_back(from_node_id.to_string());

    while let Some(node_id) = queue.pop_front() {
        if node_id == to_node_id {
            break;
        }
        let Some(node) = world.world_map_nodes.get(&node_id) else {
            continue;
        };
        let mut targets = node.exits.values().cloned().collect::<Vec<_>>();
        targets.sort();
        for target in targets {
            if !world.world_map_nodes.contains_key(&target) || visited.contains(&target) {
                continue;
            }
            visited.insert(target.clone());
            previous.insert(target.clone(), node_id.clone());
            queue.push_back(target);
        }
    }

    if !visited.contains(to_node_id) {
        return Vec::new();
    }
    let mut path = vec![to_node_id.to_string()];
    let mut cursor = to_node_id.to_string();
    while let Some(parent) = previous.get(&cursor) {
        path.push(parent.clone());
        cursor = parent.clone();
        if cursor == from_node_id {
            break;
        }
    }
    path.reverse();
    path
}

fn world_graph_direction_between(
    world: &WorldState,
    from_node_id: &str,
    to_node_id: &str,
) -> Option<String> {
    let node = world.world_map_nodes.get(from_node_id)?;
    let mut exits = node.exits.iter().collect::<Vec<_>>();
    exits.sort_by(|left, right| left.0.cmp(right.0));
    exits.into_iter().find_map(|(direction, target)| {
        if target == to_node_id {
            Some(direction.clone())
        } else {
            None
        }
    })
}

fn world_graph_path_nodes_json(world: &WorldState, path_node_ids: &[String]) -> Value {
    Value::Array(
        path_node_ids
            .iter()
            .filter_map(|node_id| world.world_map_nodes.get(node_id))
            .map(|node| {
                json!({
                    "node_id": node.node_id,
                    "name": node.name,
                    "node_kind": node.node_kind,
                    "location_id": node.location_id,
                    "zone_id": node.zone_id,
                    "x": node.x,
                    "y": node.y,
                    "osm_game_overlay_id": openstreetmap_game_overlay_id(node),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn latest_active_trillionnium_task_contract_for_travel<'a>(
    world: &'a WorldState,
    matrix_user_id: &str,
) -> Option<&'a WorldContract> {
    world.world_contracts.iter().rev().find(|contract| {
        let status = contract.status.as_str();
        contract.actor_matrix_user_id == matrix_user_id
            && contract.task_id.starts_with("trillionnium-task:")
            && (matches!(
                status,
                "trillionnium_task_offered"
                    | "trillionnium_task_completion_pending_settlement"
                    | "review_hold"
            ) || status.starts_with("completed_"))
    })
}

fn trillionnium_task_archetype_id_from_task_id(task_id: &str) -> Option<&str> {
    task_id.strip_prefix("trillionnium-task:")
}

fn world_contract_objective_overlay_id(contract: &WorldContract) -> Option<String> {
    let (_, after_objective) = contract.body.split_once("objective=")?;
    let overlay = after_objective
        .split(';')
        .next()
        .unwrap_or(after_objective)
        .trim();
    if overlay.starts_with("trillionnium-world-node:") {
        Some(overlay.to_string())
    } else {
        None
    }
}

fn task_candidate_target_node_id(
    task_candidates: &Value,
    task_archetype_id: &str,
    current_node_id: Option<&str>,
) -> Option<String> {
    let mut candidates = task_candidates
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|candidate| {
            candidate
                .get("task_archetype_id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == task_archetype_id)
        })
        .filter_map(|candidate| {
            candidate
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .and_then(trillionnium_world_overlay_node_id)
                .map(|node_id| node_id.to_string())
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
        .iter()
        .find(|node_id| Some(node_id.as_str()) != current_node_id)
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn choose_visible_objective_target(
    world: &WorldState,
    current_node: Option<&WorldMapNode>,
    osm_objectives: &Value,
) -> Option<(Value, String, Vec<String>)> {
    let current = current_node?;
    osm_objectives
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, objective)| {
            let target_node_id = objective
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .and_then(trillionnium_world_overlay_node_id)?
                .to_string();
            let target_node = world.world_map_nodes.get(&target_node_id)?;
            let path = world_graph_shortest_path(world, &current.node_id, &target_node_id);
            if path.is_empty() {
                return None;
            }
            let visible_in_keypad =
                (target_node.x - current.x).abs() <= 2 && (target_node.y - current.y).abs() <= 1;
            let target_priority = if target_node_id == current.node_id {
                1
            } else if visible_in_keypad {
                0
            } else {
                2
            };
            Some((
                target_priority,
                path.len(),
                index,
                objective,
                target_node_id,
                path,
            ))
        })
        .min_by(|left, right| (left.0, left.1, left.2).cmp(&(right.0, right.1, right.2)))
        .map(|(_, _, _, objective, target_node_id, path)| (objective, target_node_id, path))
}

fn route_json_for_world_path(
    world: &WorldState,
    route_id: String,
    route_kind: &str,
    current_node_id: &str,
    target_node_id: &str,
    path_node_ids: Vec<String>,
    route_context: Value,
) -> Value {
    let next_step_node_id = path_node_ids
        .get(1)
        .cloned()
        .or_else(|| path_node_ids.first().cloned())
        .unwrap_or_else(|| current_node_id.to_string());
    let next_step_direction = if next_step_node_id == current_node_id {
        Some("wait".to_string())
    } else {
        world_graph_direction_between(world, current_node_id, &next_step_node_id)
    };
    let target_node = world.world_map_nodes.get(target_node_id);
    json!({
        "contract_version": TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION,
        "route_id": route_id,
        "route_kind": route_kind,
        "source_of_truth": "rust_world_graph_objective_travel",
        "web_role": "visualization_only_intent_to_map_move",
        "current_node_id": current_node_id,
        "target_node_id": target_node_id,
        "target_node_name": target_node.map(|node| node.name.clone()).unwrap_or_else(|| target_node_id.to_string()),
        "target_overlay_id": target_node.map(openstreetmap_game_overlay_id).unwrap_or_else(|| format!("trillionnium-world-node:{target_node_id}")),
        "next_step_node_id": next_step_node_id,
        "next_step_direction": next_step_direction,
        "path_node_ids": path_node_ids.clone(),
        "path_nodes": world_graph_path_nodes_json(world, path_node_ids.as_slice()),
        "step_count": path_node_ids.len().saturating_sub(1),
        "travel_status": if current_node_id == target_node_id { "at_objective" } else { "en_route" },
        "action_hint": if current_node_id == target_node_id { "use_local_action_or_complete_task" } else { "move_to_next_world_node" },
        "route_context": route_context,
    })
}

fn npc_person_route_for_world_travel(
    world: &WorldState,
    current_node: Option<&WorldMapNode>,
    npcs: &Value,
) -> Option<Value> {
    let current = current_node?;
    npcs.as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|npc| {
            let npc_node_id = npc
                .get("osm_game_overlay_id")
                .and_then(Value::as_str)
                .and_then(trillionnium_world_overlay_node_id)?
                .to_string();
            let path = world_graph_shortest_path(world, &npc_node_id, &current.node_id);
            if path.is_empty() {
                return None;
            }
            Some((path.len(), npc, npc_node_id, path))
        })
        .min_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, npc, npc_node_id, path)| {
            json!({
                "contract_version": TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION,
                "person_id": npc.get("npc_id").cloned().unwrap_or_else(|| json!("local-npc")),
                "display_name": npc.get("display_name").cloned().unwrap_or_else(|| json!("Local NPC")),
                "person_kind": "npc",
                "current_node_id": npc_node_id,
                "target_node_id": current.node_id,
                "path_node_ids": path,
                "path_nodes": world_graph_path_nodes_json(world, path.as_slice()),
                "reaction_status": if npc_node_id == current.node_id { "available_here" } else { "traveling_to_player_node" },
                "source_of_truth": "rust_trillionnium_npc_spawn_anchor_plus_world_graph",
                "web_role": "visualization_only",
            })
        })
}

fn trillionnium_world_objective_travel_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
    current_node: Option<&WorldMapNode>,
    npcs: &Value,
    task_candidates: &Value,
    osm_objectives: &Value,
) -> Value {
    let current_node_id = current_node
        .map(|node| node.node_id.clone())
        .unwrap_or_else(|| default_world_node_id().to_string());
    let active_contract =
        latest_active_trillionnium_task_contract_for_travel(world, matrix_user_id);

    let (route_kind, target_node_id, path_node_ids, route_context) = if let Some(contract) =
        active_contract
    {
        let task_archetype_id = trillionnium_task_archetype_id_from_task_id(&contract.task_id)
            .unwrap_or("courier_letter");
        let target_node_id = world_contract_objective_overlay_id(contract)
            .and_then(|overlay| trillionnium_world_overlay_node_id(&overlay).map(str::to_string))
            .or_else(|| {
                task_candidate_target_node_id(
                    task_candidates,
                    task_archetype_id,
                    Some(&current_node_id),
                )
            })
            .unwrap_or_else(|| current_node_id.clone());
        let path = world_graph_shortest_path(world, &current_node_id, &target_node_id);
        (
            "active_task_route",
            target_node_id,
            if path.is_empty() {
                vec![current_node_id.clone()]
            } else {
                path
            },
            json!({
                "task_contract_id": contract.contract_id,
                "task_status": contract.status,
                "task_archetype_id": task_archetype_id,
                "task_title": contract.title,
                "cex_status": contract.cex_status,
            }),
        )
    } else if let Some((objective, target_node_id, path)) =
        choose_visible_objective_target(world, current_node, osm_objectives)
    {
        (
            "next_objective_route",
            target_node_id,
            path,
            json!({
                "objective_id": objective.get("objective_id").cloned().unwrap_or(Value::Null),
                "task_archetype_id": objective.get("task_archetype_id").cloned().unwrap_or(Value::Null),
                "objective_kind": objective.get("objective_kind").cloned().unwrap_or(Value::Null),
                "suggested_command": objective.get("suggested_command").cloned().unwrap_or(Value::Null),
            }),
        )
    } else {
        (
            "free_roam_route",
            current_node_id.clone(),
            vec![current_node_id.clone()],
            json!({ "objective_id": Value::Null, "suggested_command": "move" }),
        )
    };

    let active_route = route_json_for_world_path(
        world,
        league_hash_id(
            "trillionnium-world-objective-travel-route",
            &format!("{matrix_user_id}:{route_kind}:{current_node_id}:{target_node_id}"),
        ),
        route_kind,
        &current_node_id,
        &target_node_id,
        path_node_ids.clone(),
        route_context,
    );
    let npc_person_route = npc_person_route_for_world_travel(world, current_node, npcs);
    let next_step_node_id = active_route
        .get("next_step_node_id")
        .and_then(Value::as_str)
        .unwrap_or(&current_node_id)
        .to_string();
    let party_members = json!([
        {
            "member_id": "lord",
            "display_name": "Player hero / 玩家主角",
            "party_role": "player_character",
            "current_node_id": current_node_id.clone(),
            "target_node_id": target_node_id.clone(),
            "next_step_node_id": next_step_node_id.clone(),
            "path_node_ids": path_node_ids.clone(),
            "source_of_truth": "rust_world_player_positions",
            "web_role": "visualization_only",
        },
        {
            "member_id": "route-scout",
            "display_name": "Route Scout / 路线侦察",
            "party_role": "agent_party_scout",
            "current_node_id": current_node_id.clone(),
            "target_node_id": next_step_node_id.clone(),
            "next_step_node_id": next_step_node_id.clone(),
            "path_node_ids": [current_node_id.clone(), next_step_node_id.clone()],
            "source_of_truth": "rust_world_graph_objective_travel",
            "web_role": "visualization_only",
        },
        {
            "member_id": "ledger-closer",
            "display_name": "Ledger Closer / 结算信使",
            "party_role": "agent_party_reward_closer",
            "current_node_id": target_node_id.clone(),
            "target_node_id": target_node_id.clone(),
            "next_step_node_id": target_node_id.clone(),
            "path_node_ids": [target_node_id.clone()],
            "source_of_truth": "rust_reward_settlement_route_projection",
            "web_role": "visualization_only",
        }
    ]);
    json!({
        "contract_version": TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION,
        "source_of_truth": "rust_world_graph_objective_travel",
        "web_role": "visualization_only_intent_to_map_move",
        "matrix_user_id": matrix_user_id,
        "current_node_id": current_node_id.clone(),
        "current_overlay_id": current_node.map(openstreetmap_game_overlay_id).unwrap_or_else(|| format!("trillionnium-world-node:{}", default_world_node_id())),
        "graph_owner": "world_state.world_map_nodes.exits",
        "movement_endpoint": "/world/web/map-move",
        "movement_source_of_truth": "rust_world_map_move",
        "transition_source_of_truth": "rust_world_map_transition_rules",
        "active_route": active_route.clone(),
        "person_route": npc_person_route.clone(),
        "party_members": party_members,
        "route_tracks": [active_route, npc_person_route],
    })
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
        | "train_inner_power"
        | "train_staff"
        | "train_evidence_packaging"
        | "train_medicine"
        | "train_route_scouting"
        | "train_mediation"
        | "train_raid_coordination"
        | "train_appraisal"
        | "train_terrain_reading"
        | "train_auction_sense"
        | "train_camp_cooking"
        | "train_shadow_messaging" => vec!["sect_training_trial"],
        "review_contract_risk" | "review_evidence" => vec!["market_settlement", "find_item"],
        "repair_item" => vec!["find_item"],
        "price_bounty" => vec!["market_settlement"],
        "offer_escort_task" => vec!["escort_route"],
        "offer_patrol_loop" => vec!["street_patrol", "map_survey"],
        "recover_debt" => vec!["debt_recovery", "market_settlement"],
        "supply_medicine" => vec!["healing_supply", "find_item"],
        "survey_map" => vec!["map_survey", "courier_letter"],
        "mediate_dispute" => vec!["arbitrate_dispute", "market_settlement"],
        "coordinate_raid" => vec!["raid_signal", "escort_route", "defeat_bandit"],
        "run_arena_duel" => vec!["defeat_bandit", "raid_signal"],
        "register_sect_case" => vec!["sect_training_trial", "arbitrate_dispute"],
        "appraise_artifact" => vec!["find_item", "healing_supply"],
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
        TrillionniumNpcFixture {
            npc_id: "npc-contract-runner-mei",
            display_name: "Contract Runner Mei / 梅契跑",
            role: "courier_contract_runner",
            sect_id: "cloud-ledger-hall",
            anchor_role: "delivery_route",
            relationship_seed: 6,
            schedule: "route_morning_contract_evening_return",
            task_capabilities: vec!["offer_patrol_loop", "recover_debt"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-jade-route-scout",
            display_name: "Jade Route Scout / 玉路探",
            role: "mentor_route_scouting",
            sect_id: "jade-route-agency",
            anchor_role: "delivery_route",
            relationship_seed: 11,
            schedule: "dawn_route_scout_noon_report",
            task_capabilities: vec!["train_route_scouting", "survey_map", "offer_patrol_loop"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-bounty-board-clerk",
            display_name: "Bounty Clerk Bao / 鲍榜吏",
            role: "mentor_evidence_and_bounty_board",
            sect_id: "street-compass-society",
            anchor_role: "quest_board",
            relationship_seed: 5,
            schedule: "quest_board_open_hours",
            task_capabilities: vec![
                "train_evidence_packaging",
                "review_evidence",
                "offer_patrol_loop",
            ],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-escort-captain-han",
            display_name: "Escort Captain Han / 韩镖头",
            role: "mentor_staff_and_escort",
            sect_id: "jade-route-agency",
            anchor_role: "delivery_route",
            relationship_seed: 9,
            schedule: "caravan_departure_and_evening_drill",
            task_capabilities: vec!["train_staff", "offer_escort_task", "offer_patrol_loop"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-arena-referee-du",
            display_name: "Arena Referee Du / 杜校场",
            role: "arena_duel_referee",
            sect_id: "raid-signal-lodge",
            anchor_role: "arena",
            relationship_seed: 4,
            schedule: "arena_challenge_windows",
            task_capabilities: vec!["run_arena_duel", "coordinate_raid"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-field-apothecary",
            display_name: "Field Apothecary Yi / 易行药",
            role: "mentor_field_medicine",
            sect_id: "field-remedy-garden",
            anchor_role: "mentor_home",
            relationship_seed: 8,
            schedule: "midday_tonic_prep_night_recovery",
            task_capabilities: vec!["train_medicine", "supply_medicine", "review_evidence"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-dispute-witness-lu",
            display_name: "Witness Lu / 卢见证",
            role: "mentor_mediation_and_witness",
            sect_id: "dispute-mirror-court",
            anchor_role: "arbitration_desk",
            relationship_seed: 7,
            schedule: "case_hearing_and_witness_route",
            task_capabilities: vec!["train_mediation", "mediate_dispute", "review_evidence"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-warehouse-broker-xu",
            display_name: "Warehouse Broker Xu / 徐仓牙",
            role: "mentor_auction_and_inventory",
            sect_id: "market-wind-pavilion",
            anchor_role: "market",
            relationship_seed: 6,
            schedule: "market_auction_and_warehouse_close",
            task_capabilities: vec!["train_auction_sense", "price_bounty", "recover_debt"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-raid-drum-sergeant",
            display_name: "Raid Drum Sergeant / 鼓令军曹",
            role: "mentor_raid_coordination",
            sect_id: "raid-signal-lodge",
            anchor_role: "raid_hall",
            relationship_seed: 10,
            schedule: "raid_drill_and_signal_watch",
            task_capabilities: vec![
                "train_raid_coordination",
                "coordinate_raid",
                "offer_escort_task",
            ],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-sect-registrar-qin",
            display_name: "Sect Registrar Qin / 秦录事",
            role: "sect_registry_and_title_ladder",
            sect_id: "dispute-mirror-court",
            anchor_role: "sect_hall",
            relationship_seed: 3,
            schedule: "registry_open_midday",
            task_capabilities: vec!["register_sect_case", "mediate_dispute"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-map-tile-surveyor",
            display_name: "Map Surveyor Gao / 高图工",
            role: "mentor_terrain_and_map_survey",
            sect_id: "street-compass-society",
            anchor_role: "civic_square",
            relationship_seed: 8,
            schedule: "square_survey_and_evening_grid_notes",
            task_capabilities: vec!["train_terrain_reading", "survey_map", "offer_patrol_loop"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-artifact-appraiser",
            display_name: "Artifact Appraiser Yan / 颜鉴器",
            role: "mentor_artifact_appraisal",
            sect_id: "iron-workshop-gate",
            anchor_role: "workshop",
            relationship_seed: 7,
            schedule: "workshop_appraisal_and_market_walk",
            task_capabilities: vec!["train_appraisal", "appraise_artifact", "repair_item"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-camp-cook-lin",
            display_name: "Camp Cook Lin / 林行灶",
            role: "mentor_survival_cooking",
            sect_id: "field-remedy-garden",
            anchor_role: "mentor_home",
            relationship_seed: 5,
            schedule: "dawn_meal_prep_evening_recovery",
            task_capabilities: vec!["train_camp_cooking", "supply_medicine", "offer_patrol_loop"],
        },
        TrillionniumNpcFixture {
            npc_id: "npc-shadow-message-runner",
            display_name: "Shadow Message Runner / 暗信行者",
            role: "mentor_discreet_courier",
            sect_id: "night-watch-alliance",
            anchor_role: "quest_board",
            relationship_seed: 9,
            schedule: "night_message_route_and_board_drop",
            task_capabilities: vec![
                "train_shadow_messaging",
                "offer_patrol_loop",
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

fn trillionnium_item_equipment_catalog_json() -> Value {
    json!({
        "contract_version": "trillionnium_native_item_equipment_catalog_v1",
        "source_of_truth": "rust_trillionnium_item_equipment_catalog",
        "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        "runtime_status": "catalog_drives_rust_owned_inventory_and_equip_slots",
        "items": [
            {
                "item_id": "ledger-seal-token",
                "slot": "quest_proof",
                "family": "evidence",
                "display_name": "Ledger Seal Token / 账印令",
                "use_case": "binds submitted proof to settlement review",
                "combat_effect": "protects one objective proof bundle from interruption",
                "world_effect": "improves review-hold release quality"
            },
            {
                "item_id": "street-compass-bracer",
                "slot": "wrist",
                "family": "navigation",
                "display_name": "Street Compass Bracer / 街指南护腕",
                "use_case": "marks nearby exits and mentor anchors",
                "combat_effect": "adds one focus point on objective-entry turns",
                "world_effect": "reduces wrong-node route attempts"
            },
            {
                "item_id": "route-guard-staff",
                "slot": "weapon",
                "family": "staff",
                "display_name": "Route Guard Staff / 护路棍",
                "use_case": "escort and patrol starter weapon",
                "combat_effect": "extends melee zone control",
                "world_effect": "improves escort-route safety"
            },
            {
                "item_id": "iron-workshop-blade",
                "slot": "weapon",
                "family": "blade",
                "display_name": "Iron Workshop Blade / 铁坊短刀",
                "use_case": "workshop repair and armored encounter starter",
                "combat_effect": "improves attack against armored targets",
                "world_effect": "improves artifact repair task quality"
            },
            {
                "item_id": "market-wind-sword",
                "slot": "weapon",
                "family": "sword",
                "display_name": "Market Wind Sword / 集风剑",
                "use_case": "market negotiation duel starter",
                "combat_effect": "improves precision attack",
                "world_effect": "raises opening negotiation quality"
            },
            {
                "item_id": "night-watch-cloak",
                "slot": "cloak",
                "family": "lightness",
                "display_name": "Night Watch Cloak / 夜巡披风",
                "use_case": "patrol movement and evasion",
                "combat_effect": "raises evade on low-light tiles",
                "world_effect": "reduces travel friction on night routes"
            },
            {
                "item_id": "field-tonic-kit",
                "slot": "consumable",
                "family": "medicine",
                "display_name": "Field Tonic Kit / 行药包",
                "use_case": "recover from failed encounters and long patrols",
                "combat_effect": "recovers minor wounds after combat",
                "world_effect": "reduces party downtime"
            },
            {
                "item_id": "evidence-wrap-case",
                "slot": "pack",
                "family": "evidence",
                "display_name": "Evidence Wrap Case / 证据匣",
                "use_case": "holds task photos, reports, and review notes",
                "combat_effect": "prevents proof loss on objective tiles",
                "world_effect": "improves proof completeness scoring"
            },
            {
                "item_id": "auction-eye-lens",
                "slot": "tool",
                "family": "auction",
                "display_name": "Auction Eye Lens / 拍卖镜",
                "use_case": "inspect bounty price and item quality",
                "combat_effect": "reveals supply tile quality",
                "world_effect": "improves bounty pricing"
            },
            {
                "item_id": "raid-signal-drum",
                "slot": "party_tool",
                "family": "raid_command",
                "display_name": "Raid Signal Drum / 会战鼓",
                "use_case": "coordinate party entry and return state",
                "combat_effect": "focuses party objective attacks",
                "world_effect": "unlocks group route coordination"
            },
            {
                "item_id": "map-tile-rubbing",
                "slot": "map_note",
                "family": "terrain",
                "display_name": "Map Tile Rubbing / 地格拓片",
                "use_case": "records blocked terrain and transition rules",
                "combat_effect": "reduces movement penalty on known terrain",
                "world_effect": "improves route explanation"
            },
            {
                "item_id": "sect-registry-tag",
                "slot": "identity",
                "family": "sect",
                "display_name": "Sect Registry Tag / 门籍牌",
                "use_case": "tracks sect title ladder and mentor trust",
                "combat_effect": "adds morale on sect trial objectives",
                "world_effect": "improves relationship recovery"
            }
        ]
    })
}

fn trillionnium_resource_pressure_loops_json() -> Value {
    json!({
        "contract_version": "trillionnium_native_resource_pressure_loop_v1",
        "source_of_truth": "rust_trillionnium_resource_pressure_catalog",
        "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        "runtime_status": "catalog_projection_gate_runtime_mutation_pending",
        "loops": [
            {
                "loop_id": "daylight_route_window",
                "domain": "time",
                "pressure": "daylight_windows_change_patrol_and_delivery_risk",
                "player_choice": "depart_now_or_wait_for_lower_risk",
                "failure_mode": "late_report_review_hold"
            },
            {
                "loop_id": "stamina_travel_budget",
                "domain": "stamina",
                "pressure": "long_routes_reduce_combat_entry_focus",
                "player_choice": "rest_train_or_push_route",
                "failure_mode": "low_focus_encounter_penalty"
            },
            {
                "loop_id": "evidence_integrity",
                "domain": "proof",
                "pressure": "proof_bundle_can_be_incomplete_or_interrupted",
                "player_choice": "collect_more_evidence_or_submit_fast",
                "failure_mode": "review_hold_or_reward_delay"
            },
            {
                "loop_id": "injury_recovery",
                "domain": "health",
                "pressure": "failed_encounters_increase_downtime",
                "player_choice": "use_tonic_seek_mentor_or_continue",
                "failure_mode": "party_downtime_and_task_risk"
            },
            {
                "loop_id": "reputation_trust",
                "domain": "relationship",
                "pressure": "npc_trust_changes_task_access_and_dispute_outcomes",
                "player_choice": "mediate_dispute_pay_debt_or_train",
                "failure_mode": "locked_mentor_or_worse_reward_gate"
            },
            {
                "loop_id": "ledger_settlement_risk",
                "domain": "economy",
                "pressure": "rewards_are_held_until_settlement_review_passes",
                "player_choice": "improve_deliverable_or_accept_delay",
                "failure_mode": "reward_not_released"
            }
        ]
    })
}

fn trillionnium_story_arc_catalog_json() -> Value {
    json!({
        "contract_version": "trillionnium_native_story_arc_catalog_v1",
        "source_of_truth": "rust_trillionnium_story_arc_catalog",
        "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        "runtime_status": "catalog_projection_gate_runtime_mutation_pending",
        "arcs": [
            {
                "arc_id": "mirror_city_arrival",
                "theme": "first_route_first_mentor_first_reward",
                "entry_task_archetypes": ["courier_letter", "sect_training_trial"],
                "unlock_signal": "first_human_session_complete"
            },
            {
                "arc_id": "ledger_debt_storm",
                "theme": "contracts_debt_recovery_and_review_hold",
                "entry_task_archetypes": ["debt_recovery", "market_settlement"],
                "unlock_signal": "ledger_settlement_dispute_seen"
            },
            {
                "arc_id": "jade_route_patrol",
                "theme": "escort_patrol_and_map_survey",
                "entry_task_archetypes": ["street_patrol", "map_survey", "escort_route"],
                "unlock_signal": "route_scouting_known"
            },
            {
                "arc_id": "night_watch_dispute",
                "theme": "npc_relationship_witness_and_mediation",
                "entry_task_archetypes": ["arbitrate_dispute", "find_item"],
                "unlock_signal": "streetwise_investigation_or_mediation_known"
            },
            {
                "arc_id": "raid_signal_return",
                "theme": "combat_entry_party_raid_and_return_to_map",
                "entry_task_archetypes": ["raid_signal", "defeat_bandit"],
                "unlock_signal": "world_combat_encounter_return_loop_green"
            },
            {
                "arc_id": "field_remedy_supply",
                "theme": "medicine_supplies_recovery_and_failed_task_repair",
                "entry_task_archetypes": ["healing_supply", "find_item"],
                "unlock_signal": "healing_tonic_craft_known"
            }
        ]
    })
}

fn value_array_len(value: &Value) -> usize {
    value.as_array().map(Vec::len).unwrap_or(0)
}

fn nested_array_len(value: &Value, field: &str) -> usize {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn unique_string_field_count(items: &Value, field: &str) -> usize {
    items
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.get(field).and_then(Value::as_str).map(str::to_string))
        .collect::<HashSet<_>>()
        .len()
}

fn total_nested_array_field_count(items: &Value, field: &str) -> usize {
    items
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| nested_array_len(&item, field))
        .sum()
}

fn trillionnium_full_content_volume_alignment_json(
    world: &WorldState,
    trillionnium_character: &Value,
    skill_definitions: &Value,
    training_commands: &Value,
    sects: &Value,
    npcs: &Value,
    npc_command_descriptors: &Value,
    mentor_training_task_flows: &Value,
    task_archetypes: &Value,
    task_candidates: &Value,
    osm_objectives: &Value,
    map_overlay_identity_count: usize,
    combat_log: &Value,
    item_catalog: &Value,
    resource_pressure_runtime: &Value,
    resource_pressure_loops: &Value,
    story_arc_catalog: &Value,
) -> Value {
    let skill_count = value_array_len(skill_definitions);
    let skill_family_count = unique_string_field_count(skill_definitions, "family");
    let training_count = value_array_len(training_commands);
    let sect_count = value_array_len(sects);
    let npc_count = value_array_len(npcs);
    let npc_command_descriptor_count = value_array_len(npc_command_descriptors);
    let npc_task_capability_count = total_nested_array_field_count(npcs, "task_capabilities");
    let mentor_training_flow_count = value_array_len(mentor_training_task_flows);
    let task_archetype_count = value_array_len(task_archetypes);
    let task_candidate_count = value_array_len(task_candidates);
    let osm_objective_count = value_array_len(osm_objectives);
    let combat_log_beat_count = nested_array_len(combat_log, "beats");
    let item_count = nested_array_len(item_catalog, "items");
    let item_family_count = item_catalog
        .get("items")
        .map(|items| unique_string_field_count(items, "family"))
        .unwrap_or(0);
    let resource_loop_count = nested_array_len(resource_pressure_loops, "loops");
    let resource_runtime_tracked_domain_count = resource_pressure_runtime
        .get("tracked_domains")
        .map(value_array_len)
        .unwrap_or(0);
    let resource_runtime_mutation_source_count = resource_pressure_runtime
        .get("mutation_sources")
        .map(value_array_len)
        .unwrap_or(0);
    let resource_runtime_contract_green = resource_pressure_runtime
        .get("contract_version")
        .and_then(Value::as_str)
        == Some(TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION);
    let story_arc_count = nested_array_len(story_arc_catalog, "arcs");
    let runtime_inventory_item_count = nested_array_len(trillionnium_character, "inventory_items");
    let runtime_equipped_slot_count = trillionnium_character
        .get("equipment_slots")
        .and_then(Value::as_object)
        .map(Map::len)
        .unwrap_or(0);
    let map_node_count = world.world_map_nodes.len();
    let thresholds_green = skill_count >= 18
        && skill_family_count >= 14
        && training_count >= 18
        && sect_count >= 8
        && npc_count >= 18
        && npc_command_descriptor_count >= 28
        && npc_task_capability_count >= 36
        && mentor_training_flow_count >= 18
        && task_archetype_count >= 12
        && task_candidate_count >= 10
        && osm_objective_count >= 5
        && map_node_count >= 8
        && map_overlay_identity_count >= 8
        && combat_log_beat_count >= 4
        && item_count >= 12
        && item_family_count >= 8
        && runtime_inventory_item_count >= 3
        && runtime_equipped_slot_count >= 3
        && resource_loop_count >= 6
        && resource_runtime_contract_green
        && resource_runtime_tracked_domain_count >= 4
        && resource_runtime_mutation_source_count >= 3
        && story_arc_count >= 6;

    json!({
        "contract_version": TRILLIONNIUM_HERO_TAN_FULL_CONTENT_ALIGNMENT_CONTRACT_VERSION,
        "status": if thresholds_green { "content_volume_catalog_gate_green" } else { "content_volume_catalog_gate_blocked" },
        "scope": "full_content_volume_alignment_manifest",
        "reference_policy": {
            "reference_title": "bai_jin_hero_tan_shuo_scale_reference_only",
            "allowed_use": "mechanics_loops_content_breadth_and_coverage_shape_reference_only",
            "forbidden_use": [
                "original_text",
                "map_data",
                "sprites_or_assets",
                "source_code",
                "binary_tables",
                "npc_task_tables",
                "proprietary_names"
            ],
            "implementation_rule": "trillionnium_native_content_only",
            "copy_policy": "no_copied_hero_tan_text_assets_code_tables_or_data"
        },
        "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        "source_of_truth": "rust_trillionnium_full_content_volume_alignment_gate",
        "web_role": "visualization_input_only",
        "runtime_depth_status": "expanded_native_catalog_projection_gate; full_mmo_scale_authoring_and_runtime_mutation_remain_incremental",
        "minimum_thresholds": {
            "skill_definitions": 18,
            "skill_families": 14,
            "training_commands": 18,
            "sects": 8,
            "npcs": 18,
            "npc_command_descriptors": 28,
            "npc_task_capabilities": 36,
            "mentor_training_task_flows": 18,
            "task_archetypes": 12,
            "task_candidates": 10,
            "osm_objectives": 5,
            "world_map_nodes": 8,
            "map_overlay_identities": 8,
            "combat_log_beats": 4,
            "item_equipment_catalog": 12,
            "item_families": 8,
            "runtime_inventory_items": 3,
            "runtime_equipped_slots": 3,
            "resource_pressure_loops": 6,
            "resource_pressure_runtime_tracked_domains": 4,
            "resource_pressure_runtime_mutation_sources": 3,
            "story_arcs": 6
        },
        "coverage_counts": {
            "skill_definitions": skill_count,
            "skill_families": skill_family_count,
            "training_commands": training_count,
            "sects": sect_count,
            "npcs": npc_count,
            "npc_command_descriptors": npc_command_descriptor_count,
            "npc_task_capabilities": npc_task_capability_count,
            "mentor_training_task_flows": mentor_training_flow_count,
            "task_archetypes": task_archetype_count,
            "task_candidates": task_candidate_count,
            "osm_objectives": osm_objective_count,
            "world_map_nodes": map_node_count,
            "map_overlay_identities": map_overlay_identity_count,
            "combat_log_beats": combat_log_beat_count,
            "item_equipment_catalog": item_count,
            "item_families": item_family_count,
            "runtime_inventory_items": runtime_inventory_item_count,
            "runtime_equipped_slots": runtime_equipped_slot_count,
            "resource_pressure_loops": resource_loop_count,
            "resource_pressure_runtime_tracked_domains": resource_runtime_tracked_domain_count,
            "resource_pressure_runtime_mutation_sources": resource_runtime_mutation_source_count,
            "resource_pressure_runtime_contract_green": resource_runtime_contract_green,
            "story_arcs": story_arc_count
        },
        "thresholds_green": thresholds_green,
        "domains": [
            {"domain": "sects_and_title_ladders", "status": "native_catalog_expanded", "gate_field": "sects"},
            {"domain": "skill_families_and_training", "status": "native_catalog_expanded", "gate_field": "skill_definitions"},
            {"domain": "npc_social_relationships", "status": "native_catalog_expanded", "gate_field": "npcs"},
            {"domain": "quest_task_archetypes", "status": "native_catalog_expanded", "gate_field": "task_archetypes"},
            {"domain": "world_nodes_and_objective_travel", "status": "rust_runtime_backed", "gate_field": "world_objective_travel"},
            {"domain": "combat_entry_and_return", "status": "rust_runtime_backed", "gate_field": "world_combat_encounter"},
            {"domain": "items_and_equipment", "status": "rust_runtime_backed", "gate_field": "item_equipment_runtime"},
            {"domain": "survival_time_resource_pressure", "status": "rust_runtime_backed", "gate_field": "resource_pressure_runtime"},
            {"domain": "story_arcs", "status": "native_catalog_projection_gate", "gate_field": "story_arc_catalog"}
        ],
        "next_runtime_slices": [
            "persist_item_equipment_inventory_and_equip_slots",
            "expand_resource_pressure_recovery_and_camp_rest_loops",
            "expand_region_graph_and_story_arc_unlocks",
            "deepen_combat_numerics_without_copying_reference_data"
        ]
    })
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
                command_id: "equip_item",
                command: "equip_item",
                label: "装备道具",
                command_family: "item_equipment",
                validation_owner: "rust_trillionnium_item_equipment_runtime_state",
                web_target: "#trillionnium-equipment",
                required_skill_id: None,
                action_cost: 0,
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

#[derive(Debug, Clone)]
struct WorldCombatEncounterDefinition {
    encounter_id: String,
    encounter_kind: &'static str,
    current_node_id: String,
    current_node_name: String,
    current_overlay_id: String,
    semantic_role: &'static str,
    target_tile: &'static str,
    defender_unit_id: &'static str,
    defender_title: &'static str,
    recommended_skill_id: &'static str,
    source_of_truth: &'static str,
}

impl WorldCombatEncounterDefinition {
    fn to_projection_json(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
            "encounter_id": self.encounter_id,
            "encounter_kind": self.encounter_kind,
            "current_node_id": self.current_node_id,
            "current_node_name": self.current_node_name,
            "current_overlay_id": self.current_overlay_id,
            "semantic_role": self.semantic_role,
            "target_tile": self.target_tile,
            "defender_unit_id": self.defender_unit_id,
            "defender_title": self.defender_title,
            "recommended_skill_id": self.recommended_skill_id,
            "available": true,
            "entry_command": "attack",
            "return_anchor": "world-keypad-adventure-shell",
            "validation_owner": "rust_world_combat_encounter_validator",
            "command_handler_owner": "rust_tactics_combat_handler",
            "source_of_truth": self.source_of_truth,
            "web_role": "intent_only_visualization_input",
        })
    }
}

fn world_combat_encounter_definition_for_node(
    node: &WorldMapNode,
) -> WorldCombatEncounterDefinition {
    let semantic_role = openstreetmap_fixture_identity_for(node).semantic_role;
    let (encounter_kind, target_tile, defender_unit_id, defender_title, recommended_skill_id) =
        match semantic_role {
            "arena" | "raid_hall" => (
                "arena_duel_entry",
                "G7",
                "rival-warlord",
                "Rival Warlord",
                "basic_unarmed",
            ),
            "market" | "quest_board" | "delivery_route" | "arbitration_desk" => (
                "bounty_market_skirmish",
                "F5",
                "market-bandit",
                "Market Bandit",
                "basic_unarmed",
            ),
            _ => (
                "street_encounter_entry",
                "F5",
                "market-bandit",
                "Street Bandit",
                "basic_unarmed",
            ),
        };
    let current_overlay_id = openstreetmap_game_overlay_id(node);
    let encounter_id = league_hash_id(
        "world-combat-encounter",
        &format!("{}:{}:{}", node.node_id, semantic_role, target_tile),
    );
    WorldCombatEncounterDefinition {
        encounter_id,
        encounter_kind,
        current_node_id: node.node_id.clone(),
        current_node_name: node.name.clone(),
        current_overlay_id,
        semantic_role,
        target_tile,
        defender_unit_id,
        defender_title,
        recommended_skill_id,
        source_of_truth: "rust_world_combat_encounter_projection",
    }
}

fn current_world_combat_encounter_definition(
    world: &WorldState,
    matrix_user_id: &str,
) -> Option<WorldCombatEncounterDefinition> {
    let current_node_id = world_tactics_active_node_id(world, matrix_user_id);
    world
        .world_map_nodes
        .get(&current_node_id)
        .or_else(|| world.world_map_nodes.get(default_world_node_id()))
        .map(world_combat_encounter_definition_for_node)
}

fn world_combat_encounter_entry_rejection_json(
    command: &str,
    unit_id: &str,
    target_tile: Option<&str>,
    encounter: &WorldCombatEncounterDefinition,
    provided_overlay_id: Option<&str>,
    result: &str,
    rejection_reason: &str,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
        "world_combat_encounter_loop_contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
        "accepted": false,
        "command": command,
        "unit_id": unit_id,
        "target_tile": target_tile,
        "expected_target_tile": encounter.target_tile,
        "combat_encounter_id": encounter.encounter_id,
        "current_node_id": encounter.current_node_id,
        "required_osm_game_overlay_id": encounter.current_overlay_id,
        "provided_osm_game_overlay_id": provided_overlay_id,
        "result": result,
        "rejection_reason": rejection_reason,
        "source_of_truth": "rust_world_combat_encounter_validator",
        "web_role": "intent_only_visualization_input",
    })
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
    encounter_overlay_id: Option<&str>,
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
    let encounter_overlay_id = encounter_overlay_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| target.osm_game_overlay_id.clone());
    let seed = league_hash_id(
        "tactics-combat-seed",
        &format!(
            "{}:{}:{}:{}:{}:{}",
            matrix_user_id,
            attacker_unit_id,
            target.unit_id,
            target.tile_id,
            skill_id,
            encounter_overlay_id.as_deref().unwrap_or("no-osm-overlay")
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
        "osm_game_overlay_id": encounter_overlay_id,
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
    item_id: Option<&str>,
    target_slot: Option<&str>,
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

    let projected_combat_encounter = if command == "attack" {
        current_world_combat_encounter_definition(world, matrix_user_id)
    } else {
        None
    };
    let character = world
        .world_trillionnium_characters
        .entry(matrix_user_id.to_string())
        .or_insert_with(|| WorldTrillionniumCharacter::default_for(matrix_user_id));
    character.ensure_item_equipment_defaults(now_epoch);
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

    if command == "equip_item" {
        let requested_item_id = item_id
            .or(skill_id)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("route-guard-staff");
        if trillionnium_catalog_item_field(requested_item_id, "item_id").is_none() {
            return tactics_command_rejection_json(
                command,
                unit_id,
                target_tile,
                "unknown_item_id",
            );
        }
        if let Some(requested_slot) = target_slot.map(str::trim).filter(|value| !value.is_empty()) {
            let catalog_slot = trillionnium_catalog_item_field(requested_item_id, "slot")
                .unwrap_or_else(|| "inventory".to_string());
            if requested_slot != catalog_slot {
                return json!({
                    "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                    "item_equipment_runtime_contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
                    "accepted": false,
                    "command": command,
                    "unit_id": unit_id,
                    "target_tile": target_tile,
                    "item_id": requested_item_id,
                    "target_slot": requested_slot,
                    "expected_slot": catalog_slot,
                    "result": "equipment_slot_mismatch",
                    "rejection_reason": "equip_requires_matching_rust_catalog_slot",
                    "source_of_truth": "rust_trillionnium_item_equipment_runtime_state",
                    "web_role": "intent_only_visualization_input",
                });
            }
        }
        let Some((equipped_slot, item_instance_id)) =
            character.equip_item_by_id(requested_item_id, now_epoch)
        else {
            return json!({
                "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                "item_equipment_runtime_contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
                "accepted": false,
                "command": command,
                "unit_id": unit_id,
                "target_tile": target_tile,
                "item_id": requested_item_id,
                "result": "item_not_in_inventory",
                "rejection_reason": "equip_requires_item_in_rust_owned_inventory",
                "source_of_truth": "rust_trillionnium_item_equipment_runtime_state",
                "web_role": "intent_only_visualization_input",
            });
        };
        character.title = "整备行囊".to_string();
        character.updated_at_epoch = now_epoch;
        return json!({
            "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
            "item_equipment_runtime_contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
            "accepted": true,
            "command": command,
            "unit_id": unit_id,
            "target_tile": target_tile,
            "item_id": requested_item_id,
            "target_slot": target_slot,
            "item_instance_id": item_instance_id,
            "equipped_slot": equipped_slot,
            "equipment_slots": &character.equipment_slots,
            "item_equipment_runtime": character.item_equipment_runtime_json(),
            "validation_owner": descriptor.get("validation_owner").cloned().unwrap_or_else(|| json!("rust_trillionnium_item_equipment_runtime_state")),
            "result": "item_equipped",
            "state_mutation": "character_equipment_slot_updated",
            "source_of_truth": "rust_trillionnium_item_equipment_runtime_state",
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
            let provided_overlay_id = osm_game_overlay_id
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let projected_encounter = projected_combat_encounter;
            if let Some(encounter) = projected_encounter.as_ref() {
                if let Some(provided_overlay_id) = provided_overlay_id {
                    if provided_overlay_id != encounter.current_overlay_id {
                        return world_combat_encounter_entry_rejection_json(
                            command,
                            unit_id,
                            target_tile,
                            encounter,
                            Some(provided_overlay_id),
                            "combat_encounter_node_mismatch",
                            "combat_entry_requires_current_exploration_node_overlay",
                        );
                    }
                    let requested_target_tile = target_tile.unwrap_or(encounter.target_tile);
                    if requested_target_tile != encounter.target_tile {
                        return world_combat_encounter_entry_rejection_json(
                            command,
                            unit_id,
                            target_tile,
                            encounter,
                            Some(provided_overlay_id),
                            "combat_encounter_target_mismatch",
                            "combat_entry_target_must_match_rust_projected_node_encounter",
                        );
                    }
                }
            }
            let encounter_entry = projected_encounter.filter(|encounter| {
                provided_overlay_id.is_some()
                    || target_tile
                        .map(|requested| requested == encounter.target_tile)
                        .unwrap_or(true)
            });
            let effective_target_tile = encounter_entry
                .as_ref()
                .filter(|_| provided_overlay_id.is_some())
                .map(|encounter| encounter.target_tile)
                .or(target_tile);
            let effective_overlay_id = encounter_entry
                .as_ref()
                .map(|encounter| encounter.current_overlay_id.as_str())
                .or(provided_overlay_id);
            let character_attributes = character.attributes.clone();
            character.title = "街巷交锋".to_string();
            character.updated_at_epoch = now_epoch;
            let Some(combat_resolution) = tactics_combat_resolution_json(
                world,
                matrix_user_id,
                unit_id,
                effective_target_tile,
                effective_overlay_id,
                attack_skill_id,
                &character_attributes,
                now_epoch,
            ) else {
                return tactics_command_rejection_json(
                    command,
                    unit_id,
                    effective_target_tile,
                    "no_target_unit_at_tile",
                );
            };
            let combat_encounter = encounter_entry
                .as_ref()
                .map(WorldCombatEncounterDefinition::to_projection_json);
            let return_to_map = encounter_entry.as_ref().map(|encounter| {
                json!({
                    "contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
                    "return_state": "map_ready_after_resolution",
                    "return_to_node_id": encounter.current_node_id,
                    "return_overlay_id": encounter.current_overlay_id,
                    "return_anchor": "world-keypad-adventure-shell",
                    "source_of_truth": "rust_world_combat_encounter_return_state",
                    "web_role": "visualization_only_intent_to_map_move",
                })
            });
            if let Some(session) = latest_world_tactics_session_for_user(world, matrix_user_id) {
                if session.victory_state == "victory" && session.reward_status == "settled" {
                    return json!({
                        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                        "world_combat_encounter_loop_contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
                        "accepted": false,
                        "command": command,
                        "unit_id": unit_id,
                        "target_tile": effective_target_tile,
                        "required_skill_id": required_skill_id,
                        "combat_resolution_contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
                        "combat_resolution": combat_resolution,
                        "combat_encounter": combat_encounter.clone().unwrap_or(Value::Null),
                        "return_to_map": return_to_map.clone().unwrap_or(Value::Null),
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
                "world_combat_encounter_loop_contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
                "accepted": true,
                "command": command,
                "unit_id": unit_id,
                "target_tile": effective_target_tile,
                "required_skill_id": required_skill_id,
                "combat_resolution_contract_version": TRILLIONNIUM_TACTICS_COMBAT_RESOLUTION_CONTRACT_VERSION,
                "combat_resolution": combat_resolution,
                "combat_encounter": combat_encounter.unwrap_or(Value::Null),
                "return_to_map": return_to_map.unwrap_or(Value::Null),
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
    if let Some(provided_npc_id) = npc_id.map(str::trim).filter(|value| !value.is_empty()) {
        if provided_npc_id != training_command.mentor_npc_id {
            return json!({
                "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                "accepted": false,
                "command": command,
                "unit_id": unit_id,
                "skill_id": requested_skill_id,
                "mentor_npc_id": training_command.mentor_npc_id,
                "provided_npc_id": provided_npc_id,
                "mentor_training_task_contract_version": TRILLIONNIUM_MENTOR_TRAINING_TASK_CONTRACT_VERSION,
                "world_skill_practice_loop_contract_version": TRILLIONNIUM_WORLD_SKILL_PRACTICE_LOOP_CONTRACT_VERSION,
                "result": "mentor_mismatch",
                "rejection_reason": "mentor_training_requires_matching_npc",
                "source_of_truth": "rust_mentor_training_validator",
                "web_role": "intent_only_visualization_input",
            });
        }
    }
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
        "world_skill_practice_loop_contract_version": TRILLIONNIUM_WORLD_SKILL_PRACTICE_LOOP_CONTRACT_VERSION,
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
    let skill_definitions = trillionnium_skill_definitions_json();
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
    let world_objective_travel = trillionnium_world_objective_travel_projection_json(
        world,
        matrix_user_id,
        current_node,
        &npcs,
        &task_candidates,
        &osm_objectives,
    );
    let world_combat_encounter =
        trillionnium_world_combat_encounter_projection_json(world, matrix_user_id, current_node);
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
    let item_equipment_catalog = trillionnium_item_equipment_catalog_json();
    let item_equipment_runtime = trillionnium_character
        .get("item_equipment_runtime")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
                "source_of_truth": "rust_trillionnium_item_equipment_runtime_state",
                "runtime_status": "projected_default_until_character_mutation",
            })
        });
    let resource_pressure_runtime = trillionnium_character
        .get("resource_pressure_runtime")
        .cloned()
        .or_else(|| {
            trillionnium_character
                .get("resource_pressure_state")
                .cloned()
        })
        .unwrap_or_else(|| WorldTrillionniumResourcePressureState::default().to_value());
    let resource_pressure_loops = trillionnium_resource_pressure_loops_json();
    let story_arc_catalog = trillionnium_story_arc_catalog_json();
    let full_content_alignment = trillionnium_full_content_volume_alignment_json(
        world,
        &trillionnium_character,
        &skill_definitions,
        &training_commands,
        &sects,
        &npcs,
        &npc_command_descriptors,
        &mentor_training_task_flows,
        &task_archetypes,
        &task_candidates,
        &osm_objectives,
        map_overlay_identity_count,
        &combat_log,
        &item_equipment_catalog,
        &resource_pressure_runtime,
        &resource_pressure_loops,
        &story_arc_catalog,
    );
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
        "trillionnium_resource_pressure_runtime_contract_version": TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION,
        "trillionnium_npc_relationship_contract_version": TRILLIONNIUM_NPC_RELATIONSHIP_CONTRACT_VERSION,
        "trillionnium_osm_objective_contract_version": TRILLIONNIUM_OSM_OBJECTIVE_CONTRACT_VERSION,
        "world_objective_travel_contract_version": TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION,
        "world_skill_practice_loop_contract_version": TRILLIONNIUM_WORLD_SKILL_PRACTICE_LOOP_CONTRACT_VERSION,
        "world_combat_encounter_loop_contract_version": TRILLIONNIUM_WORLD_COMBAT_ENCOUNTER_LOOP_CONTRACT_VERSION,
        "full_content_alignment_contract_version": TRILLIONNIUM_HERO_TAN_FULL_CONTENT_ALIGNMENT_CONTRACT_VERSION,
        "item_equipment_runtime_contract_version": TRILLIONNIUM_WORLD_ITEM_EQUIPMENT_RUNTIME_CONTRACT_VERSION,
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
        "skill_definitions": skill_definitions,
        "training_commands": training_commands,
        "sects": sects,
        "npcs": npcs,
        "npc_spawn_anchors": npc_spawn_anchors,
        "npc_command_descriptors": npc_command_descriptors,
        "mentor_training_task_flows": mentor_training_task_flows,
        "task_archetypes": task_archetypes,
        "task_candidates": task_candidates,
        "osm_objectives": osm_objectives.clone(),
        "world_objective_travel": world_objective_travel,
        "world_combat_encounter": world_combat_encounter,
        "map_overlay_identity_index": map_overlay_identity_index,
        "game_session": game_session,
        "simulation_ticks": simulation_ticks,
        "battle_log_style": trillionnium_battle_log_style_json(),
        "combat_log": combat_log,
        "item_equipment_catalog": item_equipment_catalog,
        "item_equipment_runtime": item_equipment_runtime,
        "resource_pressure_runtime": resource_pressure_runtime,
        "resource_pressure_loops": resource_pressure_loops,
        "story_arc_catalog": story_arc_catalog,
        "full_content_alignment": full_content_alignment,
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
            "allowed_commands": ["select_unit", "move_unit", "attack", "use_skill", "equip_item", "train_skill", "talk_npc", "offer_task", "complete_task", "interact", "end_turn"],
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
