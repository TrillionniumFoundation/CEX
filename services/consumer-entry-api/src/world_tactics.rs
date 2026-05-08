use super::*;

pub(super) const TRILLIONNIUM_TACTICS_BOARD_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_board_v1";
pub(super) const TRILLIONNIUM_TACTICS_UNIT_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_unit_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMMAND_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_command_v1";
pub(super) const TRILLIONNIUM_JIANGHU_CHARACTER_CONTRACT_VERSION: &str =
    "trillionnium_jianghu_character_v1";
pub(super) const TRILLIONNIUM_JIANGHU_SKILL_CONTRACT_VERSION: &str =
    "trillionnium_jianghu_skill_v1";
pub(super) const TRILLIONNIUM_JIANGHU_TRAINING_CONTRACT_VERSION: &str =
    "trillionnium_jianghu_training_command_v1";
pub(super) const TRILLIONNIUM_JIANGHU_SECT_CONTRACT_VERSION: &str = "trillionnium_jianghu_sect_v1";
pub(super) const TRILLIONNIUM_JIANGHU_NPC_CONTRACT_VERSION: &str = "trillionnium_jianghu_npc_v1";
pub(super) const TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION: &str =
    "trillionnium_world_tactics_command_outcome_v1";

#[derive(Debug, Clone)]
pub(super) struct JianghuSkillDefinition {
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

impl JianghuSkillDefinition {
    fn to_value(&self) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_JIANGHU_SKILL_CONTRACT_VERSION,
            "skill_id": self.skill_id,
            "family": self.family,
            "name": self.name,
            "level": self.level,
            "xp": self.xp,
            "unlock_condition": self.unlock_condition,
            "combat_effect": self.combat_effect,
            "world_effect": self.world_effect,
            "training_anchor_role": self.training_anchor_role,
            "source_of_truth": "rust_jianghu_skill_definition",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn jianghu_fixture_skill_definitions() -> Vec<JianghuSkillDefinition> {
    vec![
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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
        JianghuSkillDefinition {
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

fn jianghu_skill_definition_by_id(skill_id: &str) -> Option<JianghuSkillDefinition> {
    jianghu_fixture_skill_definitions()
        .into_iter()
        .find(|skill| skill.skill_id == skill_id)
}

pub(super) fn jianghu_skill_definitions_json() -> Value {
    Value::Array(
        jianghu_fixture_skill_definitions()
            .into_iter()
            .map(|skill| skill.to_value())
            .collect::<Vec<_>>(),
    )
}

fn jianghu_known_skill_definitions_json(skill_ids: &[String]) -> Value {
    Value::Array(
        jianghu_fixture_skill_definitions()
            .into_iter()
            .filter(|skill| skill_ids.iter().any(|skill_id| skill_id == skill.skill_id))
            .map(|skill| skill.to_value())
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct JianghuAttributes {
    physique: u16,
    force: u16,
    agility: u16,
    insight: u16,
    resolve: u16,
    craft: u16,
    commerce: u16,
    reputation: i32,
}

impl Default for JianghuAttributes {
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

impl JianghuAttributes {
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
pub(super) struct WorldJianghuCharacter {
    matrix_user_id: String,
    character_id: String,
    display_name: String,
    attributes: JianghuAttributes,
    sect_id: Option<String>,
    title: String,
    skill_ids: Vec<String>,
    updated_at_epoch: i64,
}

impl WorldJianghuCharacter {
    fn default_for(matrix_user_id: &str) -> Self {
        Self {
            matrix_user_id: matrix_user_id.to_string(),
            character_id: league_hash_id("jianghu-character", matrix_user_id),
            display_name: "镜城游侠".to_string(),
            attributes: JianghuAttributes::default(),
            sect_id: None,
            title: "初入江湖".to_string(),
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
            "contract_version": TRILLIONNIUM_JIANGHU_CHARACTER_CONTRACT_VERSION,
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
            "known_skills": jianghu_known_skill_definitions_json(&self.skill_ids),
            "skill_definition_contract": TRILLIONNIUM_JIANGHU_SKILL_CONTRACT_VERSION,
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

pub(super) fn world_jianghu_character_projection_json(
    world: &WorldState,
    matrix_user_id: &str,
) -> Value {
    world
        .world_jianghu_characters
        .get(matrix_user_id)
        .cloned()
        .unwrap_or_else(|| WorldJianghuCharacter::default_for(matrix_user_id))
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

#[derive(Debug, Clone)]
struct JianghuTrainingCommand {
    skill_id: &'static str,
    mentor_npc_id: &'static str,
    required_semantic_role: &'static str,
    cost_xp: i64,
    cooldown_seconds: i64,
}

impl JianghuTrainingCommand {
    fn to_value(&self, features: &[Value]) -> Value {
        let anchor_overlay_id = feature_overlay_id_for_role(features, self.required_semantic_role)
            .unwrap_or_else(|| "trillionnium-world-node:mirror-city-square".to_string());
        json!({
            "contract_version": TRILLIONNIUM_JIANGHU_TRAINING_CONTRACT_VERSION,
            "command": "train_skill",
            "skill_id": self.skill_id,
            "mentor_npc_id": self.mentor_npc_id,
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

fn jianghu_training_command_fixtures() -> Vec<JianghuTrainingCommand> {
    vec![
        JianghuTrainingCommand {
            skill_id: "basic_inner_power",
            mentor_npc_id: "npc-cloud-ledger-mentor",
            required_semantic_role: "mentor_home",
            cost_xp: 12,
            cooldown_seconds: 600,
        },
        JianghuTrainingCommand {
            skill_id: "basic_unarmed",
            mentor_npc_id: "npc-street-compass-sifu",
            required_semantic_role: "civic_square",
            cost_xp: 8,
            cooldown_seconds: 420,
        },
        JianghuTrainingCommand {
            skill_id: "basic_blade",
            mentor_npc_id: "npc-iron-workshop-smith",
            required_semantic_role: "workshop",
            cost_xp: 10,
            cooldown_seconds: 480,
        },
        JianghuTrainingCommand {
            skill_id: "basic_sword",
            mentor_npc_id: "npc-market-wind-adviser",
            required_semantic_role: "market",
            cost_xp: 10,
            cooldown_seconds: 480,
        },
        JianghuTrainingCommand {
            skill_id: "merchant_routecraft",
            mentor_npc_id: "npc-market-wind-adviser",
            required_semantic_role: "market",
            cost_xp: 16,
            cooldown_seconds: 900,
        },
        JianghuTrainingCommand {
            skill_id: "artifact_crafting",
            mentor_npc_id: "npc-iron-workshop-smith",
            required_semantic_role: "workshop",
            cost_xp: 16,
            cooldown_seconds: 900,
        },
        JianghuTrainingCommand {
            skill_id: "streetwise_investigation",
            mentor_npc_id: "npc-night-watch-arbiter",
            required_semantic_role: "arbitration_desk",
            cost_xp: 14,
            cooldown_seconds: 720,
        },
    ]
}

pub(super) fn jianghu_training_commands_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        jianghu_training_command_fixtures()
            .into_iter()
            .map(|command| command.to_value(&features))
            .collect::<Vec<_>>(),
    )
}

fn jianghu_training_command_for_skill(skill_id: &str) -> Option<JianghuTrainingCommand> {
    jianghu_training_command_fixtures()
        .into_iter()
        .find(|command| command.skill_id == skill_id)
}

#[derive(Debug, Clone)]
struct JianghuSectFixture {
    sect_id: &'static str,
    display_name: &'static str,
    specialization: &'static str,
    anchor_role: &'static str,
    mentor_npc_ids: Vec<&'static str>,
    entry_requirement: &'static str,
    benefits: Vec<&'static str>,
    title_ladder: Vec<&'static str>,
}

impl JianghuSectFixture {
    fn to_value(&self, features: &[Value]) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_JIANGHU_SECT_CONTRACT_VERSION,
            "sect_id": self.sect_id,
            "display_name": self.display_name,
            "specialization": self.specialization,
            "anchor_semantic_role": self.anchor_role,
            "osm_game_overlay_id": feature_overlay_id_for_role(features, self.anchor_role),
            "mentor_npc_ids": self.mentor_npc_ids,
            "entry_requirement": self.entry_requirement,
            "benefits": self.benefits,
            "title_ladder": self.title_ladder,
            "source_of_truth": "rust_jianghu_sect_model",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn jianghu_sect_fixtures() -> Vec<JianghuSectFixture> {
    vec![
        JianghuSectFixture {
            sect_id: "cloud-ledger-hall",
            display_name: "Cloud Ledger Hall / 云账堂",
            specialization: "inner_power_contracts_and_settlement",
            anchor_role: "ledger_hall",
            mentor_npc_ids: vec!["npc-cloud-ledger-mentor"],
            entry_requirement: "reading_and_contracts_known",
            benefits: vec!["settlement_recovery_bonus", "contract_risk_preview"],
            title_ladder: vec!["outer_clerk", "ledger_runner", "cloud_ledger_keeper"],
        },
        JianghuSectFixture {
            sect_id: "street-compass-society",
            display_name: "Street Compass Society / 街指南社",
            specialization: "movement_investigation_and_routecraft",
            anchor_role: "civic_square",
            mentor_npc_ids: vec!["npc-street-compass-sifu"],
            entry_requirement: "basic_lightness_known",
            benefits: vec!["movement_range_hint", "street_event_preview"],
            title_ladder: vec!["street_walker", "route_scout", "compass_pathfinder"],
        },
        JianghuSectFixture {
            sect_id: "iron-workshop-gate",
            display_name: "Iron Workshop Gate / 铁坊门",
            specialization: "craft_blade_and_artifact_repair",
            anchor_role: "workshop",
            mentor_npc_ids: vec!["npc-iron-workshop-smith"],
            entry_requirement: "artifact_crafting_or_basic_blade_training",
            benefits: vec!["craft_quality_bonus", "item_repair_discount"],
            title_ladder: vec!["apprentice_smith", "artifact_mender", "iron_gate_master"],
        },
        JianghuSectFixture {
            sect_id: "market-wind-pavilion",
            display_name: "Market Wind Pavilion / 集风阁",
            specialization: "commerce_negotiation_and_bounty_quality",
            anchor_role: "market",
            mentor_npc_ids: vec!["npc-market-wind-adviser"],
            entry_requirement: "merchant_routecraft_training_available",
            benefits: vec!["listing_quality_hint", "negotiation_bonus"],
            title_ladder: vec!["stall_runner", "wind_broker", "market_pavilion_master"],
        },
        JianghuSectFixture {
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

pub(super) fn jianghu_sect_fixtures_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        jianghu_sect_fixtures()
            .into_iter()
            .map(|sect| sect.to_value(&features))
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone)]
struct JianghuNpcFixture {
    npc_id: &'static str,
    display_name: &'static str,
    role: &'static str,
    sect_id: &'static str,
    anchor_role: &'static str,
    relationship_seed: i64,
    schedule: &'static str,
    task_capabilities: Vec<&'static str>,
}

impl JianghuNpcFixture {
    fn to_value(&self, features: &[Value]) -> Value {
        json!({
            "contract_version": TRILLIONNIUM_JIANGHU_NPC_CONTRACT_VERSION,
            "npc_id": self.npc_id,
            "display_name": self.display_name,
            "role": self.role,
            "sect_id": self.sect_id,
            "anchor_semantic_role": self.anchor_role,
            "osm_game_overlay_id": feature_overlay_id_for_role(features, self.anchor_role),
            "relationship_seed": self.relationship_seed,
            "schedule": self.schedule,
            "task_capabilities": self.task_capabilities,
            "command_descriptors": ["talk", "train_skill", "offer_task"],
            "source_of_truth": "rust_jianghu_npc_model",
            "content_policy": "trillionnium_native_no_copied_hero_tan_text_assets_or_tables",
        })
    }
}

fn jianghu_npc_fixtures() -> Vec<JianghuNpcFixture> {
    vec![
        JianghuNpcFixture {
            npc_id: "npc-cloud-ledger-mentor",
            display_name: "Ledger Mentor Wen / 温账师",
            role: "mentor_contracts_inner_power",
            sect_id: "cloud-ledger-hall",
            anchor_role: "ledger_hall",
            relationship_seed: 12,
            schedule: "morning_ledger_evening_training",
            task_capabilities: vec!["train_inner_power", "review_contract_risk"],
        },
        JianghuNpcFixture {
            npc_id: "npc-street-compass-sifu",
            display_name: "Compass Sifu Luo / 罗街师",
            role: "mentor_movement_unarmed",
            sect_id: "street-compass-society",
            anchor_role: "civic_square",
            relationship_seed: 9,
            schedule: "daytime_square_patrol",
            task_capabilities: vec!["train_unarmed", "train_lightness", "offer_patrol_task"],
        },
        JianghuNpcFixture {
            npc_id: "npc-iron-workshop-smith",
            display_name: "Iron Smith Qiao / 乔铁匠",
            role: "mentor_blade_crafting",
            sect_id: "iron-workshop-gate",
            anchor_role: "workshop",
            relationship_seed: 7,
            schedule: "workshop_day_shift",
            task_capabilities: vec!["train_blade", "train_artifact_crafting", "repair_item"],
        },
        JianghuNpcFixture {
            npc_id: "npc-market-wind-adviser",
            display_name: "Market Adviser Lin / 林集风",
            role: "mentor_commerce_sword",
            sect_id: "market-wind-pavilion",
            anchor_role: "market",
            relationship_seed: 10,
            schedule: "market_open_hours",
            task_capabilities: vec!["train_sword", "train_routecraft", "price_bounty"],
        },
        JianghuNpcFixture {
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

pub(super) fn jianghu_npc_fixtures_json(openstreetmap_geodata: &Value) -> Value {
    let features = openstreetmap_geodata
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        jianghu_npc_fixtures()
            .into_iter()
            .map(|npc| npc.to_value(&features))
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
            "actor_matrix_user_id": self.actor_matrix_user_id,
            "character_source": self.character_source,
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
            "source_of_truth": "rust_tactics_command_model",
            "web_role": "intent_only_visualization_input",
        })
    }
}

fn tactics_units_json(
    matrix_user_id: &str,
    jianghu_character: &Value,
    arena_overlay_id: Option<String>,
    market_overlay_id: Option<String>,
) -> Value {
    let max_hp = jianghu_character["attributes"]["derived_stats"]["max_hp"]
        .as_i64()
        .unwrap_or(160);
    let energy = jianghu_character["attributes"]["derived_stats"]["inner_energy"]
        .as_i64()
        .unwrap_or(100);
    let move_range = jianghu_character["attributes"]["derived_stats"]["move_range"]
        .as_i64()
        .unwrap_or(4);
    Value::Array(
        vec![
            TacticsUnit {
                unit_id: "lord",
                owner: "player",
                side: "player",
                archetype: "jianghu_lord",
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
                character_source: "jianghu_character",
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
                command_family: "jianghu_skill",
                validation_owner: "rust_jianghu_skill_validator",
                web_target: "#trillionnium-jianghu-status",
                required_skill_id: Some("basic_inner_power"),
                action_cost: 1,
            },
            TacticsCommandDescriptor {
                command_id: "train_skill",
                command: "train_skill",
                label: "导师修炼",
                command_family: "mentor_training",
                validation_owner: "rust_mentor_training_validator",
                web_target: "#trillionnium-jianghu-training",
                required_skill_id: None,
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

pub(super) fn apply_world_tactics_command(
    world: &mut WorldState,
    matrix_user_id: &str,
    command: &str,
    unit_id: Option<&str>,
    target_tile: Option<&str>,
    skill_id: Option<&str>,
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
    let character = world
        .world_jianghu_characters
        .entry(matrix_user_id.to_string())
        .or_insert_with(|| WorldJianghuCharacter::default_for(matrix_user_id));
    let known_skills: HashSet<String> = character.skill_ids.iter().cloned().collect();

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
    let Some(skill) = jianghu_skill_definition_by_id(requested_skill_id) else {
        return tactics_command_rejection_json(command, unit_id, target_tile, "unknown_skill_id");
    };
    let Some(training_command) = jianghu_training_command_for_skill(requested_skill_id) else {
        return tactics_command_rejection_json(
            command,
            unit_id,
            target_tile,
            "skill_has_no_training_command",
        );
    };
    let nodes: Vec<WorldMapNode> = world.world_map_nodes.values().cloned().collect();
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);
    let training_commands = jianghu_training_commands_json(&geodata);
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
        "skill_contract_version": TRILLIONNIUM_JIANGHU_SKILL_CONTRACT_VERSION,
        "skill_family": skill.family,
        "mentor_npc_id": training_command.mentor_npc_id,
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
    let jianghu_character = world_jianghu_character_projection_json(world, matrix_user_id);
    let units = tactics_units_json(
        matrix_user_id,
        &jianghu_character,
        arena_overlay_id.clone(),
        market_overlay_id.clone(),
    );
    let available_commands = tactics_available_commands_json();
    let training_commands = jianghu_training_commands_json(openstreetmap_geodata);
    let sects = jianghu_sect_fixtures_json(openstreetmap_geodata);
    let npcs = jianghu_npc_fixtures_json(openstreetmap_geodata);
    json!({
        "contract_version": TRILLIONNIUM_TACTICS_BOARD_CONTRACT_VERSION,
        "source_of_truth": "rust_trillionnium_game_state",
        "web_role": "visualization_input_only",
        "unit_contract_version": TRILLIONNIUM_TACTICS_UNIT_CONTRACT_VERSION,
        "command_contract_version": TRILLIONNIUM_TACTICS_COMMAND_CONTRACT_VERSION,
        "command_outcome_contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
        "jianghu_skill_contract_version": TRILLIONNIUM_JIANGHU_SKILL_CONTRACT_VERSION,
        "jianghu_training_contract_version": TRILLIONNIUM_JIANGHU_TRAINING_CONTRACT_VERSION,
        "jianghu_sect_contract_version": TRILLIONNIUM_JIANGHU_SECT_CONTRACT_VERSION,
        "jianghu_npc_contract_version": TRILLIONNIUM_JIANGHU_NPC_CONTRACT_VERSION,
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
        "jianghu_character": jianghu_character,
        "skill_definitions": jianghu_skill_definitions_json(),
        "training_commands": training_commands,
        "sects": sects,
        "npcs": npcs,
        "npc_relationship_model": {
            "contract_version": "trillionnium_jianghu_npc_relationship_v1",
            "source_of_truth": "rust_jianghu_npc_model",
            "relationship_seed_owner": "rust_trillionnium_game_state",
            "web_role": "visualization_input_only"
        },
        "units": units,
        "objectives": [
            {"objective_id": "bounty-gate", "label": "赏", "title": "占领目标 / Objective", "grid_column": 7, "grid_row": 1, "source": "osm_feature", "osm_game_overlay_id": objective_overlay_id, "completion_owner": "rust_command_handler_ledger_progression"}
        ],
        "available_commands": available_commands,
        "turn_state": {
            "contract_version": "trillionnium_world_tactics_turn_state_v1",
            "active_side": "player",
            "active_unit_id": "lord",
            "round": 1,
            "action_points_remaining": 2,
            "source_of_truth": "rust_tactics_turn_handler",
            "allowed_commands": ["select_unit", "move_unit", "attack", "use_skill", "interact", "end_turn"],
        },
        "battle_log": [
            {"kind": "base", "text": "> 底座：tranchikhang/MedievalWar MIT · Phaser 3 地图/光标/回合/寻路/目标循环。"},
            {"kind": "map", "text": "> map: OpenClawStreetMap provides terrain, distance, events, and objectives."},
            {"kind": "jianghu", "text": "> jianghu: gmud/RMXP-Hero/yxts-llm inspire attributes, skills, sects, NPC tasks, and wuxia logs; Trillionnium owns the content."}
        ],
        "osm_objective_source": {
            "provider_contract": "OpenStreetMapDataProvider",
            "geodata_contract_version": OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
            "objective_overlay_id": objective_overlay_id,
            "osm_can_suggest_objectives": true,
            "rust_command_handler_decides_completion": true
        }
    })
}
