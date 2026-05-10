use super::*;

pub(super) const TRILLIONNIUM_WORLD_TRANSITION_SEMANTICS_CONTRACT_VERSION: &str =
    "trillionnium_world_transition_semantics_v1";

#[derive(Debug, Clone, Serialize)]
pub(super) struct WorldMapTransitionDecision {
    pub contract_version: &'static str,
    pub source_of_truth: &'static str,
    pub web_role: &'static str,
    pub accepted: bool,
    pub result: String,
    pub transition_status: String,
    pub transition_kind: String,
    pub direction: String,
    pub target: String,
    pub current_node_id: String,
    pub target_node_id: Option<String>,
    pub to_node_id: Option<String>,
    pub current_location_id: String,
    pub target_location_id: Option<String>,
    pub current_zone_id: String,
    pub target_zone_id: Option<String>,
    pub changes_location: bool,
    pub changes_zone: bool,
    pub requires_interaction: bool,
    pub blocked_reason: Option<String>,
    pub user_message: String,
}

impl WorldMapTransitionDecision {
    pub(super) fn http_status(&self) -> StatusCode {
        if self.accepted {
            StatusCode::OK
        } else {
            match self.result.as_str() {
                "unknown_target" => StatusCode::NOT_FOUND,
                "locked_route" | "interaction_required" | "non_adjacent_route" => {
                    StatusCode::CONFLICT
                }
                _ => StatusCode::CONFLICT,
            }
        }
    }

    pub(super) fn error_message(&self) -> &'static str {
        match self.result.as_str() {
            "blocked_terrain" => "world map direction is blocked terrain",
            "unknown_target" => {
                "world map target node not found or not reachable by that direction"
            }
            "locked_route" => "world map route is locked",
            "interaction_required" => "world map route requires interaction before movement",
            "non_adjacent_route" => "world map target is not adjacent",
            _ => "world map movement rejected",
        }
    }
}

pub(super) fn world_transition_aliases(target: &str) -> Vec<&'static str> {
    match target.trim().to_ascii_lowercase().as_str() {
        "8" | "n" | "north" => vec!["north", "n"],
        "2" | "s" | "south" => vec!["south", "s"],
        "4" | "w" | "west" => vec!["west", "w"],
        "6" | "e" | "east" => vec!["east", "e"],
        "7" | "nw" | "northwest" | "north-west" => {
            vec!["north-west", "northwest", "nw"]
        }
        "9" | "ne" | "northeast" | "north-east" => {
            vec!["north-east", "northeast", "ne"]
        }
        "1" | "sw" | "southwest" | "south-west" => {
            vec!["south-west", "southwest", "sw"]
        }
        "3" | "se" | "southeast" | "south-east" => {
            vec!["south-east", "southeast", "se"]
        }
        "5" | "wait" | "stay" | "here" => vec!["wait", "stay"],
        _ => Vec::new(),
    }
}

pub(super) fn world_transition_primary_direction(target: &str) -> String {
    world_transition_aliases(target)
        .first()
        .copied()
        .unwrap_or_else(|| target.trim())
        .to_string()
}

fn world_transition_kind(current: &WorldMapNode, target: &WorldMapNode) -> String {
    if current.node_id == target.node_id {
        "wait".to_string()
    } else if current.zone_id != target.zone_id {
        "zone_transition".to_string()
    } else if current.location_id != target.location_id {
        "room_transition".to_string()
    } else {
        "local_exit".to_string()
    }
}

fn world_transition_status_for_target(
    target: &WorldMapNode,
) -> Option<(&'static str, &'static str)> {
    let status = target.status.trim();
    if status.eq_ignore_ascii_case("open") || status.is_empty() {
        None
    } else if status.eq_ignore_ascii_case("interaction_required")
        || status.starts_with("interaction_required")
        || status.starts_with("requires_")
    {
        Some(("interaction_required", "interaction_required"))
    } else {
        Some(("locked_route", "locked_route"))
    }
}

fn world_transition_decision_base(
    current: &WorldMapNode,
    target: &str,
    accepted: bool,
    result: &str,
    transition_status: &str,
    transition_kind: &str,
    direction: &str,
    target_node: Option<&WorldMapNode>,
    blocked_reason: Option<String>,
    user_message: String,
) -> WorldMapTransitionDecision {
    WorldMapTransitionDecision {
        contract_version: TRILLIONNIUM_WORLD_TRANSITION_SEMANTICS_CONTRACT_VERSION,
        source_of_truth: "rust_world_map_transition_rules",
        web_role: "intent_only_visualization_input",
        accepted,
        result: result.to_string(),
        transition_status: transition_status.to_string(),
        transition_kind: transition_kind.to_string(),
        direction: direction.to_string(),
        target: target.to_string(),
        current_node_id: current.node_id.clone(),
        target_node_id: target_node.map(|node| node.node_id.clone()),
        to_node_id: target_node.map(|node| node.node_id.clone()),
        current_location_id: current.location_id.clone(),
        target_location_id: target_node.map(|node| node.location_id.clone()),
        current_zone_id: current.zone_id.clone(),
        target_zone_id: target_node.map(|node| node.zone_id.clone()),
        changes_location: target_node
            .map(|node| node.location_id != current.location_id)
            .unwrap_or(false),
        changes_zone: target_node
            .map(|node| node.zone_id != current.zone_id)
            .unwrap_or(false),
        requires_interaction: result == "interaction_required",
        blocked_reason,
        user_message,
    }
}

pub(super) fn world_map_transition_decision(
    world: &WorldState,
    current: &WorldMapNode,
    target: &str,
) -> WorldMapTransitionDecision {
    let target_trimmed = target.trim();
    let aliases = world_transition_aliases(target_trimmed);
    let direction = world_transition_primary_direction(target_trimmed);

    if aliases.contains(&"wait") || target_trimmed == current.node_id {
        return world_transition_decision_base(
            current,
            target_trimmed,
            true,
            "wait",
            "accepted",
            "wait",
            "wait",
            Some(current),
            None,
            "Wait in the current room.".to_string(),
        );
    }

    let mut resolved_direction = direction.clone();
    let mut target_node_id = None;
    for candidate in aliases
        .iter()
        .copied()
        .chain(std::iter::once(target_trimmed))
    {
        if let Some(node_id) = current.exits.get(candidate) {
            resolved_direction = candidate.to_string();
            target_node_id = Some(node_id.clone());
            break;
        }
    }

    if target_node_id.is_none()
        && current
            .exits
            .values()
            .any(|node_id| node_id == target_trimmed)
    {
        target_node_id = Some(target_trimmed.to_string());
        resolved_direction = current
            .exits
            .iter()
            .find_map(|(exit_direction, node_id)| {
                if node_id == target_trimmed {
                    Some(exit_direction.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| target_trimmed.to_string());
    }

    if target_node_id.is_none() && !aliases.is_empty() {
        return world_transition_decision_base(
            current,
            target_trimmed,
            false,
            "blocked_terrain",
            "blocked",
            "blocked_terrain",
            &direction,
            None,
            Some("no_exit_for_direction".to_string()),
            "That direction has no open exit from the current room.".to_string(),
        );
    }

    let target_node_id = target_node_id.unwrap_or_else(|| target_trimmed.to_string());
    let Some(target_node) = world.world_map_nodes.get(&target_node_id) else {
        return world_transition_decision_base(
            current,
            target_trimmed,
            false,
            "unknown_target",
            "blocked",
            "unknown_target",
            &resolved_direction,
            None,
            Some("target_node_missing".to_string()),
            "The target room does not exist in the Rust world graph.".to_string(),
        );
    };

    let is_direct_exit = target_node.node_id == current.node_id
        || current
            .exits
            .values()
            .any(|node_id| node_id == &target_node.node_id);
    if !is_direct_exit {
        return world_transition_decision_base(
            current,
            target_trimmed,
            false,
            "non_adjacent_route",
            "locked",
            "locked_route",
            &resolved_direction,
            Some(target_node),
            Some("target_not_in_current_exits".to_string()),
            "That room is visible in the world graph but is not adjacent from here.".to_string(),
        );
    }

    if let Some((result, transition_kind)) = world_transition_status_for_target(target_node) {
        return world_transition_decision_base(
            current,
            target_trimmed,
            false,
            result,
            "locked",
            transition_kind,
            &resolved_direction,
            Some(target_node),
            Some(format!("target_status:{}", target_node.status)),
            if result == "interaction_required" {
                "That route needs a local interaction before the player can enter.".to_string()
            } else {
                "That route is locked by the Rust world state.".to_string()
            },
        );
    }

    let transition_kind = world_transition_kind(current, target_node);
    world_transition_decision_base(
        current,
        target_trimmed,
        true,
        "open_exit",
        "accepted",
        &transition_kind,
        &resolved_direction,
        Some(target_node),
        None,
        if transition_kind == "zone_transition" || transition_kind == "room_transition" {
            "Move accepted; this crosses into another room/zone projection.".to_string()
        } else {
            "Move accepted through an adjacent local exit.".to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_transition_test_node(
        world: &mut WorldState,
        current_id: &str,
        direction: &str,
        node_id: &str,
        location_id: &str,
        zone_id: &str,
        status: &str,
    ) {
        world
            .world_map_nodes
            .get_mut(current_id)
            .expect("current test node")
            .exits
            .insert(direction.to_string(), node_id.to_string());
        world.world_map_nodes.insert(
            node_id.to_string(),
            WorldMapNode {
                node_id: node_id.to_string(),
                location_id: location_id.to_string(),
                zone_id: zone_id.to_string(),
                name: format!("{node_id} test room"),
                node_kind: "test_room".to_string(),
                description: "transition semantics fixture".to_string(),
                x: 9,
                y: 9,
                exits: HashMap::new(),
                interaction_tags: Vec::new(),
                freedom_hooks: Vec::new(),
                status: status.to_string(),
            },
        );
    }

    #[test]
    fn classifies_world_map_transition_semantics_from_rust_world_graph() {
        let mut league = default_league_state();
        let current_id = default_world_node_id().to_string();
        let base_current = league
            .world
            .world_map_nodes
            .get(&current_id)
            .cloned()
            .expect("default current node");
        add_transition_test_node(
            &mut league.world,
            &current_id,
            "north-east",
            "transition-side-room",
            "transition-side-room",
            &base_current.zone_id,
            "open",
        );
        add_transition_test_node(
            &mut league.world,
            &current_id,
            "north-west",
            "transition-locked-room",
            &base_current.location_id,
            &base_current.zone_id,
            "locked",
        );
        add_transition_test_node(
            &mut league.world,
            &current_id,
            "south-west",
            "transition-interaction-room",
            &base_current.location_id,
            &base_current.zone_id,
            "interaction_required",
        );
        let current = league
            .world
            .world_map_nodes
            .get(&current_id)
            .cloned()
            .expect("updated current node");

        let blocked = world_map_transition_decision(&league.world, &current, "south-east");
        assert_eq!(
            blocked.contract_version,
            TRILLIONNIUM_WORLD_TRANSITION_SEMANTICS_CONTRACT_VERSION
        );
        assert_eq!(blocked.source_of_truth, "rust_world_map_transition_rules");
        assert_eq!(blocked.web_role, "intent_only_visualization_input");
        assert!(!blocked.accepted);
        assert_eq!(blocked.result, "blocked_terrain");
        assert_eq!(blocked.transition_status, "blocked");
        assert_eq!(blocked.transition_kind, "blocked_terrain");
        assert_eq!(blocked.http_status(), StatusCode::CONFLICT);
        assert_eq!(
            blocked.blocked_reason.as_deref(),
            Some("no_exit_for_direction")
        );

        let unknown = world_map_transition_decision(&league.world, &current, "ghost-room");
        assert!(!unknown.accepted);
        assert_eq!(unknown.result, "unknown_target");
        assert_eq!(unknown.http_status(), StatusCode::NOT_FOUND);

        let locked = world_map_transition_decision(&league.world, &current, "north-west");
        assert!(!locked.accepted);
        assert_eq!(locked.result, "locked_route");
        assert_eq!(locked.transition_status, "locked");
        assert_eq!(locked.transition_kind, "locked_route");
        assert_eq!(locked.to_node_id.as_deref(), Some("transition-locked-room"));
        assert_eq!(
            locked.blocked_reason.as_deref(),
            Some("target_status:locked")
        );

        let interaction = world_map_transition_decision(&league.world, &current, "south-west");
        assert!(!interaction.accepted);
        assert_eq!(interaction.result, "interaction_required");
        assert_eq!(interaction.transition_kind, "interaction_required");
        assert!(interaction.requires_interaction);

        let room = world_map_transition_decision(&league.world, &current, "north-east");
        assert!(room.accepted);
        assert_eq!(room.result, "open_exit");
        assert_eq!(room.transition_status, "accepted");
        assert_eq!(room.transition_kind, "room_transition");
        assert!(room.changes_location);
        assert!(!room.changes_zone);

        let zone = world_map_transition_decision(&league.world, &current, "north");
        assert!(zone.accepted);
        assert_eq!(zone.transition_kind, "zone_transition");
        assert!(zone.changes_zone);

        let wait = world_map_transition_decision(&league.world, &current, "5");
        assert!(wait.accepted);
        assert_eq!(wait.result, "wait");
        assert_eq!(wait.transition_kind, "wait");
        assert_eq!(wait.to_node_id.as_deref(), Some(current_id.as_str()));
    }
}
