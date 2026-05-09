use super::*;

pub(super) const OPENSTREETMAP_GEODATA_CONTRACT_VERSION: &str = "openstreetmap_geodata_v1";
pub(super) const OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION: &str =
    "openstreetmap_fixture_layers_v1";
pub(super) const OPENSTREETMAP_DERIVED_DATABASE_METADATA_CONTRACT_VERSION: &str =
    "openstreetmap_derived_database_metadata_v1";
pub(super) const OPENSTREETMAP_PROVIDER_MODE_CONTRACT_VERSION: &str =
    "openstreetmap_provider_mode_v1";
pub(super) const OPENSTREETMAP_PROVIDER_READINESS_CONTRACT_VERSION: &str =
    "openstreetmap_provider_readiness_v1";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum OpenStreetMapProviderMode {
    Fixture,
    OverpassBboxCache,
    GeofabrikExtractImport,
    VendorTileCache,
    Unknown,
}

impl OpenStreetMapProviderMode {
    pub(super) fn from_str(value: &str) -> Self {
        match value {
            "fixture" => Self::Fixture,
            "overpass_bbox_cache" => Self::OverpassBboxCache,
            "geofabrik_extract_import" => Self::GeofabrikExtractImport,
            "vendor_tile_cache" => Self::VendorTileCache,
            _ => Self::Unknown,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Fixture => "fixture",
            Self::OverpassBboxCache => "overpass_bbox_cache",
            Self::GeofabrikExtractImport => "geofabrik_extract_import",
            Self::VendorTileCache => "vendor_tile_cache",
            Self::Unknown => "unknown",
        }
    }

    pub(super) fn enabled(self) -> bool {
        matches!(self, Self::Fixture)
    }

    pub(super) fn fail_closed_reason(self) -> &'static str {
        match self {
            Self::Fixture => "fixture_provider_allowed_without_network_ingestion",
            Self::OverpassBboxCache => {
                "blocked_until_bbox_cache_rate_limit_and_odbl_tracking_exist"
            }
            Self::GeofabrikExtractImport => {
                "blocked_until_extract_import_pipeline_and_derived_database_manifest_exist"
            }
            Self::VendorTileCache => {
                "blocked_until_vendor_contract_cache_and_attribution_manifest_exist"
            }
            Self::Unknown => "unknown_provider_mode_fail_closed",
        }
    }
}

pub(super) fn openstreetmap_provider_mode_status_json(mode: &str) -> Value {
    let parsed = OpenStreetMapProviderMode::from_str(mode);
    json!({
        "contract_version": OPENSTREETMAP_PROVIDER_MODE_CONTRACT_VERSION,
        "mode": parsed.as_str(),
        "requested_mode": mode,
        "enabled": parsed.enabled(),
        "fail_closed": !parsed.enabled(),
        "network_ingestion_enabled": false,
        "reason": parsed.fail_closed_reason(),
        "source_of_truth": "rust_openstreetmap_data_provider",
    })
}

fn openstreetmap_provider_modes_json() -> Value {
    Value::Array(
        [
            "fixture",
            "overpass_bbox_cache",
            "geofabrik_extract_import",
            "vendor_tile_cache",
            "unknown",
        ]
        .into_iter()
        .map(openstreetmap_provider_mode_status_json)
        .collect::<Vec<_>>(),
    )
}

fn openstreetmap_layer_feature_count(fixture_layers: &Value) -> u64 {
    fixture_layers
        .get("layer_feature_counts")
        .and_then(Value::as_object)
        .map(|counts| counts.values().filter_map(Value::as_u64).sum::<u64>())
        .unwrap_or(0)
}

fn openstreetmap_provider_readiness_json(
    nodes: &[WorldMapNode],
    fixture_layers: &Value,
    stable_fixture_count: usize,
) -> Value {
    let fixture_status = openstreetmap_provider_mode_status_json("fixture");
    let live_mode_statuses = [
        "overpass_bbox_cache",
        "geofabrik_extract_import",
        "vendor_tile_cache",
        "unknown",
    ]
    .into_iter()
    .map(openstreetmap_provider_mode_status_json)
    .collect::<Vec<_>>();
    let live_modes_fail_closed = live_mode_statuses.iter().all(|status| {
        status
            .get("fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && !status
                .get("network_ingestion_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
    });
    let fail_closed_mode_count = live_mode_statuses
        .iter()
        .filter(|status| {
            status
                .get("fail_closed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let fixture_layer_feature_count = openstreetmap_layer_feature_count(fixture_layers);
    let stable_fixture_identity_coverage_complete = stable_fixture_count == nodes.len();
    let fixture_mode_green = fixture_status
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !fixture_status
            .get("network_ingestion_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && stable_fixture_identity_coverage_complete
        && fixture_layer_feature_count > 0;
    json!({
        "contract_version": OPENSTREETMAP_PROVIDER_READINESS_CONTRACT_VERSION,
        "provider_contract": "OpenStreetMapDataProvider",
        "provider_id": "fixture_openstreetmap_data_provider_v1",
        "provider_mode_contract_version": OPENSTREETMAP_PROVIDER_MODE_CONTRACT_VERSION,
        "mode": "fixture",
        "readiness_status": "fixture_ready_live_fail_closed",
        "green": fixture_mode_green && live_modes_fail_closed,
        "source_of_truth": "rust_openstreetmap_data_provider",
        "web_role": "visualization_input_only",
        "fixture_mode_green": fixture_mode_green,
        "fixture_provider_enabled": fixture_status.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        "fixture_provider_fail_closed": fixture_status.get("fail_closed").and_then(Value::as_bool).unwrap_or(true),
        "fixture_network_ingestion_enabled": fixture_status.get("network_ingestion_enabled").and_then(Value::as_bool).unwrap_or(true),
        "fixture_node_count": nodes.len(),
        "fixture_layer_feature_count": fixture_layer_feature_count,
        "stable_fixture_identity_count": stable_fixture_count,
        "stable_fixture_identity_coverage_complete": stable_fixture_identity_coverage_complete,
        "live_modes_fail_closed": live_modes_fail_closed,
        "live_network_ingestion_enabled": false,
        "production_ingestion_enabled": false,
        "provider_modes_observable": true,
        "fail_closed_mode_count": fail_closed_mode_count,
        "expected_fail_closed_mode_count": live_mode_statuses.len(),
        "overpass_bbox_cache_fail_closed": live_mode_statuses.iter().any(|status| status.get("mode").and_then(Value::as_str) == Some("overpass_bbox_cache") && status.get("fail_closed").and_then(Value::as_bool).unwrap_or(false)),
        "geofabrik_extract_import_fail_closed": live_mode_statuses.iter().any(|status| status.get("mode").and_then(Value::as_str) == Some("geofabrik_extract_import") && status.get("fail_closed").and_then(Value::as_bool).unwrap_or(false)),
        "vendor_tile_cache_fail_closed": live_mode_statuses.iter().any(|status| status.get("mode").and_then(Value::as_str) == Some("vendor_tile_cache") && status.get("fail_closed").and_then(Value::as_bool).unwrap_or(false)),
        "unknown_mode_fail_closed": live_mode_statuses.iter().any(|status| status.get("mode").and_then(Value::as_str) == Some("unknown") && status.get("fail_closed").and_then(Value::as_bool).unwrap_or(false)),
        "public_tile_server_production_traffic_allowed": false,
        "public_tile_server_policy": "do_not_use_public_osm_tile_servers_for_production_traffic",
        "odbl_tracking_required_before_live": true,
        "derived_database_metadata_required_before_live": true,
        "live_provider_preconditions": [
            "bbox_cache_rate_limit_and_retry_budget",
            "geofabrik_extract_import_pipeline",
            "derived_database_manifest_and_odbl_tracking",
            "self_hosted_or_vendor_tile_cache_contract",
            "fresh_production_signoff_before_live_ingestion"
        ],
        "fixture_readiness_checks": [
            "fixture_provider_enabled",
            "stable_fixture_identity_coverage_complete",
            "fixture_layers_non_empty",
            "live_modes_fail_closed",
            "network_ingestion_disabled",
            "odbl_tracking_required_before_live"
        ],
        "fixture_status": fixture_status,
        "live_mode_statuses": live_mode_statuses,
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OpenStreetMapSemanticRoleMapping {
    pub(super) semantic_role: &'static str,
    pub(super) source_layer: &'static str,
    pub(super) game_system_role: &'static str,
    pub(super) objective_kind: &'static str,
    pub(super) completion_owner: &'static str,
}

const OPENSTREETMAP_SEMANTIC_ROLE_MAPPINGS: &[OpenStreetMapSemanticRoleMapping] = &[
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "civic_square",
        source_layer: "pois",
        game_system_role: "spawn_hub",
        objective_kind: "orientation_checkpoint",
        completion_owner: "rust_command_handler_progression",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "mentor_home",
        source_layer: "buildings",
        game_system_role: "mentor_training_anchor",
        objective_kind: "training_visit",
        completion_owner: "rust_mentor_training_validator",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "ledger_hall",
        source_layer: "pois",
        game_system_role: "ledger_settlement_anchor",
        objective_kind: "settlement_review",
        completion_owner: "rust_ledger_progression_gate",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "sect_hall",
        source_layer: "areas",
        game_system_role: "sect_membership_anchor",
        objective_kind: "sect_entry_or_training",
        completion_owner: "rust_sect_progression_validator",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "workshop",
        source_layer: "pois",
        game_system_role: "crafting_station",
        objective_kind: "artifact_crafting",
        completion_owner: "rust_inventory_crafting_gate",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "inventory_yard",
        source_layer: "areas",
        game_system_role: "asset_storage_anchor",
        objective_kind: "asset_upgrade",
        completion_owner: "rust_inventory_progression_gate",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "market",
        source_layer: "pois",
        game_system_role: "bounty_market_anchor",
        objective_kind: "accept_or_publish_bounty",
        completion_owner: "rust_market_command_handler",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "quest_board",
        source_layer: "pois",
        game_system_role: "quest_discovery_anchor",
        objective_kind: "inspect_bounty_board",
        completion_owner: "rust_task_generator",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "delivery_route",
        source_layer: "roads",
        game_system_role: "route_delivery_edge",
        objective_kind: "courier_delivery",
        completion_owner: "rust_work_delivery_handler",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "arbitration_desk",
        source_layer: "pois",
        game_system_role: "dispute_review_anchor",
        objective_kind: "evidence_review",
        completion_owner: "rust_review_hold_gate",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "arena",
        source_layer: "areas",
        game_system_role: "combat_rating_anchor",
        objective_kind: "arena_duel",
        completion_owner: "rust_tactics_combat_handler",
    },
    OpenStreetMapSemanticRoleMapping {
        semantic_role: "raid_hall",
        source_layer: "admin_boundaries",
        game_system_role: "multi_party_raid_anchor",
        objective_kind: "guild_raid_route",
        completion_owner: "rust_raid_progression_gate",
    },
];

#[derive(Debug, Clone, Copy)]
pub(super) struct OpenStreetMapFixtureIdentity {
    pub(super) osm_type: &'static str,
    pub(super) osm_id: i64,
    pub(super) semantic_role: &'static str,
    pub(super) source_layer: &'static str,
}

pub(super) fn openstreetmap_semantic_role_mappings_json() -> Value {
    Value::Array(
        OPENSTREETMAP_SEMANTIC_ROLE_MAPPINGS
            .iter()
            .map(|mapping| {
                json!({
                    "semantic_role": mapping.semantic_role,
                    "source_layer": mapping.source_layer,
                    "game_system_role": mapping.game_system_role,
                    "objective_kind": mapping.objective_kind,
                    "completion_owner": mapping.completion_owner,
                    "layer_owner": "OpenStreetMapDataProvider",
                    "game_truth_owner": "trillionnium_rust_world_state",
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn openstreetmap_semantic_role_mapping_for(role: &str) -> OpenStreetMapSemanticRoleMapping {
    OPENSTREETMAP_SEMANTIC_ROLE_MAPPINGS
        .iter()
        .copied()
        .find(|mapping| mapping.semantic_role == role)
        .unwrap_or(OpenStreetMapSemanticRoleMapping {
            semantic_role: "street_encounter",
            source_layer: "tags",
            game_system_role: "street_encounter_anchor",
            objective_kind: "free_roam_encounter",
            completion_owner: "rust_world_action_handler",
        })
}

pub(super) trait OpenStreetMapDataProvider {
    fn provider_id(&self) -> &'static str;
    fn source_mode(&self) -> &'static str;
    fn feature_json(&self, node: &WorldMapNode) -> Value;

    fn features_json(&self, nodes: &[WorldMapNode]) -> Vec<Value> {
        nodes.iter().map(|node| self.feature_json(node)).collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FixtureOpenStreetMapDataProvider;

impl OpenStreetMapDataProvider for FixtureOpenStreetMapDataProvider {
    fn provider_id(&self) -> &'static str {
        "fixture_openstreetmap_data_provider_v1"
    }

    fn source_mode(&self) -> &'static str {
        "local_fixture_mock_first_no_live_overpass"
    }

    fn feature_json(&self, node: &WorldMapNode) -> Value {
        openstreetmap_geodata_feature_json(self, node)
    }
}

pub(super) fn real_world_node_coordinates(node: &WorldMapNode) -> (f64, f64) {
    // Deterministic local projection for the early Trillionnium fixture world. The OSM
    // provider owns the identity/geodata contract; the visual map engine only consumes this.
    const BASE_LAT: f64 = 31.230416;
    const BASE_LNG: f64 = 121.473701;
    const LAT_STEP: f64 = 0.0048;
    const LNG_STEP: f64 = 0.0065;
    let lat = BASE_LAT - (node.y as f64 * LAT_STEP);
    let lng = BASE_LNG + (node.x as f64 * LNG_STEP);
    (lat, lng)
}

pub(super) fn openstreetmap_fixture_identity_for(
    node: &WorldMapNode,
) -> OpenStreetMapFixtureIdentity {
    match node.node_id.as_str() {
        "mirror-city-square" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_001,
            semantic_role: "civic_square",
            source_layer: "pois",
        },
        "agent-dormitory" => OpenStreetMapFixtureIdentity {
            osm_type: "node",
            osm_id: 31_230_416_002,
            semantic_role: "mentor_home",
            source_layer: "buildings",
        },
        "ledger-office" => OpenStreetMapFixtureIdentity {
            osm_type: "node",
            osm_id: 31_230_416_003,
            semantic_role: "ledger_hall",
            source_layer: "pois",
        },
        "starter-studio" => OpenStreetMapFixtureIdentity {
            osm_type: "relation",
            osm_id: 31_230_416_101,
            semantic_role: "sect_hall",
            source_layer: "areas",
        },
        "forge-workbench" => OpenStreetMapFixtureIdentity {
            osm_type: "node",
            osm_id: 31_230_416_102,
            semantic_role: "workshop",
            source_layer: "pois",
        },
        "asset-yard" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_103,
            semantic_role: "inventory_yard",
            source_layer: "areas",
        },
        "zbj-market-gate" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_201,
            semantic_role: "market",
            source_layer: "pois",
        },
        "client-board" => OpenStreetMapFixtureIdentity {
            osm_type: "node",
            osm_id: 31_230_416_202,
            semantic_role: "quest_board",
            source_layer: "pois",
        },
        "delivery-dock" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_203,
            semantic_role: "delivery_route",
            source_layer: "roads",
        },
        "dispute-desk" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_204,
            semantic_role: "arbitration_desk",
            source_layer: "pois",
        },
        "league-coliseum" => OpenStreetMapFixtureIdentity {
            osm_type: "way",
            osm_id: 31_230_416_301,
            semantic_role: "arena",
            source_layer: "areas",
        },
        "raid-hall" => OpenStreetMapFixtureIdentity {
            osm_type: "relation",
            osm_id: 31_230_416_302,
            semantic_role: "raid_hall",
            source_layer: "admin_boundaries",
        },
        _ => OpenStreetMapFixtureIdentity {
            osm_type: fallback_openstreetmap_osm_type(node),
            osm_id: fallback_openstreetmap_osm_id(node),
            semantic_role: fallback_openstreetmap_semantic_role(node),
            source_layer: "tags",
        },
    }
}

fn fallback_openstreetmap_osm_type(node: &WorldMapNode) -> &'static str {
    if node
        .interaction_tags
        .iter()
        .any(|tag| tag == "guild" || tag == "raid")
    {
        "relation"
    } else if matches!(
        node.node_kind.as_str(),
        "hub_square" | "market_gate" | "arena_gate" | "delivery_dock" | "dispute_desk"
    ) {
        "way"
    } else {
        "node"
    }
}

fn fallback_openstreetmap_osm_id(node: &WorldMapNode) -> i64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in node.node_id.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    1_770_000_000_i64 + (hash % 8_000_000_000) as i64
}

fn fallback_openstreetmap_semantic_role(node: &WorldMapNode) -> &'static str {
    if node.interaction_tags.iter().any(|tag| tag == "ledger") {
        "ledger_hall"
    } else if node
        .interaction_tags
        .iter()
        .any(|tag| tag == "market" || tag == "listing" || tag == "bounty")
    {
        "market"
    } else if node
        .interaction_tags
        .iter()
        .any(|tag| tag == "guild" || tag == "raid")
    {
        "sect_hall"
    } else if node
        .interaction_tags
        .iter()
        .any(|tag| tag == "arena" || tag == "ranking")
    {
        "arena"
    } else if node
        .interaction_tags
        .iter()
        .any(|tag| tag == "craft" || tag == "upgrade")
    {
        "workshop"
    } else {
        "street_encounter"
    }
}

pub(super) fn openstreetmap_osm_type(node: &WorldMapNode) -> &'static str {
    openstreetmap_fixture_identity_for(node).osm_type
}

pub(super) fn openstreetmap_osm_id(node: &WorldMapNode) -> i64 {
    openstreetmap_fixture_identity_for(node).osm_id
}

pub(super) fn openstreetmap_game_overlay_id(node: &WorldMapNode) -> String {
    format!("trillionnium-world-node:{}", node.node_id)
}

fn openstreetmap_amenity_tag(node: &WorldMapNode) -> &'static str {
    let identity = openstreetmap_fixture_identity_for(node);
    match identity.semantic_role {
        "ledger_hall" => "bank",
        "market" | "quest_board" => "marketplace",
        "arena" | "raid_hall" => "events_venue",
        "workshop" | "sect_hall" => "workshop",
        "delivery_route" => "courier",
        "arbitration_desk" => "courthouse",
        "mentor_home" => "community_centre",
        _ => "community_centre",
    }
}

fn openstreetmap_node_tags_json(node: &WorldMapNode) -> Value {
    let identity = openstreetmap_fixture_identity_for(node);
    let mapping = openstreetmap_semantic_role_mapping_for(identity.semantic_role);
    let mut tags = Map::new();
    tags.insert("name".to_string(), json!(node.name.as_str()));
    tags.insert("name:zh".to_string(), json!(node.name.as_str()));
    tags.insert(
        "amenity".to_string(),
        json!(openstreetmap_amenity_tag(node)),
    );
    tags.insert("source".to_string(), json!("trillionnium_fixture"));
    tags.insert(
        "trillionnium:node_id".to_string(),
        json!(node.node_id.as_str()),
    );
    tags.insert(
        "trillionnium:location_id".to_string(),
        json!(node.location_id.as_str()),
    );
    tags.insert(
        "trillionnium:zone_id".to_string(),
        json!(node.zone_id.as_str()),
    );
    tags.insert(
        "trillionnium:node_kind".to_string(),
        json!(node.node_kind.as_str()),
    );
    tags.insert(
        "trillionnium:game_overlay_id".to_string(),
        json!(openstreetmap_game_overlay_id(node)),
    );
    tags.insert(
        "trillionnium:semantic_role".to_string(),
        json!(identity.semantic_role),
    );
    tags.insert(
        "trillionnium:source_layer".to_string(),
        json!(identity.source_layer),
    );
    tags.insert(
        "trillionnium:game_system_role".to_string(),
        json!(mapping.game_system_role),
    );
    tags.insert(
        "trillionnium:objective_kind".to_string(),
        json!(mapping.objective_kind),
    );
    tags.insert(
        "trillionnium:completion_owner".to_string(),
        json!(mapping.completion_owner),
    );
    tags.insert(
        "trillionnium:interaction_tags".to_string(),
        json!(node.interaction_tags.join(",")),
    );
    Value::Object(tags)
}

fn openstreetmap_geodata_feature_json(
    provider: &dyn OpenStreetMapDataProvider,
    node: &WorldMapNode,
) -> Value {
    let (lat, lng) = real_world_node_coordinates(node);
    let identity = openstreetmap_fixture_identity_for(node);
    let mapping = openstreetmap_semantic_role_mapping_for(identity.semantic_role);
    let osm_type = identity.osm_type;
    let osm_id = identity.osm_id;
    let game_overlay_id = openstreetmap_game_overlay_id(node);
    json!({
        "feature_id": format!("{osm_type}/{osm_id}"),
        "contract_version": OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
        "provider_id": provider.provider_id(),
        "source_mode": provider.source_mode(),
        "identity_source": if identity.source_layer == "tags" { "deterministic_hash_fallback" } else { "stable_fixture_table" },
        "stable_fixture_identity": identity.source_layer != "tags",
        "osm_type": osm_type,
        "osm_id": osm_id,
        "semantic_role": identity.semantic_role,
        "source_layer": identity.source_layer,
        "game_system_role": mapping.game_system_role,
        "objective_kind": mapping.objective_kind,
        "completion_owner": mapping.completion_owner,
        "objective_seed": format!("{}:{}:{}", node.zone_id, osm_type, osm_id),
        "lat": lat,
        "lng": lng,
        "lat_string": format!("{lat:.6}"),
        "lng_string": format!("{lng:.6}"),
        "geometry": {
            "type": "Point",
            "coordinates": [lng, lat],
            "projection": "EPSG:4326",
        },
        "tags": openstreetmap_node_tags_json(node),
        "game_overlay_id": game_overlay_id,
        "game_binding": {
            "source_of_truth": "rust_world_state",
            "projection_owner": "OpenStreetMapDataProvider",
            "web_role": "visualization_input_only",
            "node_id": &node.node_id,
            "location_id": &node.location_id,
            "zone_id": &node.zone_id,
            "node_kind": &node.node_kind,
            "x": node.x,
            "y": node.y,
            "interaction_tags": &node.interaction_tags,
            "freedom_hooks": &node.freedom_hooks,
        }
    })
}

fn openstreetmap_fixture_polygon_coordinates(node: &WorldMapNode, radius: f64) -> Value {
    let (lat, lng) = real_world_node_coordinates(node);
    json!([[
        [lng - radius, lat - radius],
        [lng + radius, lat - radius],
        [lng + radius, lat + radius],
        [lng - radius, lat + radius],
        [lng - radius, lat - radius]
    ]])
}

fn openstreetmap_layer_feature_for_node(
    layer: &str,
    node: &WorldMapNode,
    geometry_type: &str,
    coordinates: Value,
) -> Value {
    let identity = openstreetmap_fixture_identity_for(node);
    let mapping = openstreetmap_semantic_role_mapping_for(identity.semantic_role);
    json!({
        "layer": layer,
        "contract_version": OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION,
        "feature_id": format!("fixture-layer/{layer}/{}/{}", identity.osm_type, identity.osm_id),
        "osm_type": identity.osm_type,
        "osm_id": identity.osm_id,
        "semantic_role": identity.semantic_role,
        "game_system_role": mapping.game_system_role,
        "objective_kind": mapping.objective_kind,
        "completion_owner": mapping.completion_owner,
        "game_overlay_id": openstreetmap_game_overlay_id(node),
        "node_id": &node.node_id,
        "geometry": {
            "type": geometry_type,
            "coordinates": coordinates,
            "projection": "EPSG:4326",
        },
        "tags": openstreetmap_node_tags_json(node),
        "source_of_truth": "rust_openstreetmap_data_provider",
        "game_truth_owner": "trillionnium_rust_world_state",
    })
}

fn openstreetmap_fixture_road_features_json(nodes: &[WorldMapNode]) -> Vec<Value> {
    let node_by_id: HashMap<&str, &WorldMapNode> = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect();
    let mut roads = Vec::new();
    for node in nodes {
        for target_id in node.exits.values() {
            if node.node_id.as_str() >= target_id.as_str() {
                continue;
            }
            if let Some(target) = node_by_id.get(target_id.as_str()) {
                let (from_lat, from_lng) = real_world_node_coordinates(node);
                let (to_lat, to_lng) = real_world_node_coordinates(target);
                let identity = openstreetmap_fixture_identity_for(node);
                roads.push(json!({
                    "layer": "roads",
                    "contract_version": OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION,
                    "feature_id": format!("fixture-road/{}--{}", node.node_id, target.node_id),
                    "osm_type": "way",
                    "osm_id": fallback_openstreetmap_osm_id(node).saturating_add(fallback_openstreetmap_osm_id(target)),
                    "semantic_role": if identity.semantic_role == "delivery_route" { "delivery_route" } else { "street_route" },
                    "game_system_role": if identity.semantic_role == "delivery_route" { "route_delivery_edge" } else { "movement_edge" },
                    "objective_kind": if identity.semantic_role == "delivery_route" { "courier_delivery" } else { "free_roam_travel" },
                    "completion_owner": "rust_world_movement_handler",
                    "from_node_id": &node.node_id,
                    "to_node_id": &target.node_id,
                    "from_game_overlay_id": openstreetmap_game_overlay_id(node),
                    "to_game_overlay_id": openstreetmap_game_overlay_id(target),
                    "geometry": {
                        "type": "LineString",
                        "coordinates": [[from_lng, from_lat], [to_lng, to_lat]],
                        "projection": "EPSG:4326",
                    },
                    "tags": {
                        "highway": "service",
                        "source": "trillionnium_fixture",
                        "trillionnium:layer": "roads",
                        "trillionnium:from_node_id": &node.node_id,
                        "trillionnium:to_node_id": &target.node_id,
                    },
                    "source_of_truth": "rust_openstreetmap_data_provider",
                    "game_truth_owner": "trillionnium_rust_world_state",
                }));
            }
        }
    }
    roads
}

fn openstreetmap_fixture_building_features_json(nodes: &[WorldMapNode]) -> Vec<Value> {
    nodes
        .iter()
        .filter(|node| {
            let identity = openstreetmap_fixture_identity_for(node);
            identity.source_layer == "buildings"
                || matches!(
                    identity.semantic_role,
                    "mentor_home" | "ledger_hall" | "workshop" | "arbitration_desk"
                )
        })
        .map(|node| {
            openstreetmap_layer_feature_for_node(
                "buildings",
                node,
                "Polygon",
                openstreetmap_fixture_polygon_coordinates(node, 0.00055),
            )
        })
        .collect()
}

fn openstreetmap_fixture_area_features_json(nodes: &[WorldMapNode]) -> Vec<Value> {
    nodes
        .iter()
        .filter(|node| {
            let identity = openstreetmap_fixture_identity_for(node);
            identity.source_layer == "areas"
                || matches!(
                    identity.semantic_role,
                    "civic_square" | "arena" | "inventory_yard"
                )
        })
        .map(|node| {
            openstreetmap_layer_feature_for_node(
                "areas",
                node,
                "Polygon",
                openstreetmap_fixture_polygon_coordinates(node, 0.00115),
            )
        })
        .collect()
}

fn openstreetmap_fixture_admin_boundary_features_json(nodes: &[WorldMapNode]) -> Vec<Value> {
    let mut boundaries: Vec<Value> = nodes
        .iter()
        .filter(|node| openstreetmap_fixture_identity_for(node).source_layer == "admin_boundaries")
        .map(|node| {
            openstreetmap_layer_feature_for_node(
                "admin_boundaries",
                node,
                "Polygon",
                openstreetmap_fixture_polygon_coordinates(node, 0.0018),
            )
        })
        .collect();

    if !nodes.is_empty() {
        let min_lat = nodes
            .iter()
            .map(|node| real_world_node_coordinates(node).0)
            .fold(f64::INFINITY, f64::min);
        let max_lat = nodes
            .iter()
            .map(|node| real_world_node_coordinates(node).0)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lng = nodes
            .iter()
            .map(|node| real_world_node_coordinates(node).1)
            .fold(f64::INFINITY, f64::min);
        let max_lng = nodes
            .iter()
            .map(|node| real_world_node_coordinates(node).1)
            .fold(f64::NEG_INFINITY, f64::max);
        boundaries.push(json!({
            "layer": "admin_boundaries",
            "contract_version": OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION,
            "feature_id": "fixture-admin-boundary/cn-shanghai-core",
            "osm_type": "relation",
            "osm_id": 31_230_416_900_i64,
            "semantic_role": "neighborhood_boundary",
            "game_system_role": "faction_territory_boundary",
            "objective_kind": "territory_patrol",
            "completion_owner": "rust_faction_progression_gate",
            "zone_id": "cn-shanghai-core",
            "geometry": {
                "type": "Polygon",
                "coordinates": [[
                    [min_lng - 0.003, min_lat - 0.003],
                    [max_lng + 0.003, min_lat - 0.003],
                    [max_lng + 0.003, max_lat + 0.003],
                    [min_lng - 0.003, max_lat + 0.003],
                    [min_lng - 0.003, min_lat - 0.003]
                ]],
                "projection": "EPSG:4326",
            },
            "tags": {
                "boundary": "administrative",
                "admin_level": "10",
                "source": "trillionnium_fixture",
                "trillionnium:layer": "admin_boundaries",
                "trillionnium:zone_id": "cn-shanghai-core",
            },
            "source_of_truth": "rust_openstreetmap_data_provider",
            "game_truth_owner": "trillionnium_rust_world_state",
        }));
    }
    boundaries
}

pub(super) fn openstreetmap_fixture_layers_json(nodes: &[WorldMapNode]) -> Value {
    let roads = openstreetmap_fixture_road_features_json(nodes);
    let buildings = openstreetmap_fixture_building_features_json(nodes);
    let areas = openstreetmap_fixture_area_features_json(nodes);
    let admin_boundaries = openstreetmap_fixture_admin_boundary_features_json(nodes);
    let road_count = roads.len();
    let building_count = buildings.len();
    let area_count = areas.len();
    let admin_boundary_count = admin_boundaries.len();
    json!({
        "contract_version": OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION,
        "source_of_truth": "rust_openstreetmap_data_provider",
        "web_role": "visualization_input_only",
        "live_ingestion_enabled": false,
        "layers": {
            "roads": roads,
            "buildings": buildings,
            "areas": areas,
            "admin_boundaries": admin_boundaries,
        },
        "layer_feature_counts": {
            "roads": road_count,
            "buildings": building_count,
            "areas": area_count,
            "admin_boundaries": admin_boundary_count,
        },
        "semantic_role_mapping": openstreetmap_semantic_role_mappings_json(),
        "objective_rule": "OSM layers can seed candidate objectives; Rust command handlers and ledger/progression gates own completion.",
    })
}

fn openstreetmap_derived_database_metadata_json(
    nodes: &[WorldMapNode],
    fixture_layers: &Value,
) -> Value {
    let layer_counts = fixture_layers
        .get("layer_feature_counts")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let total_layer_features = layer_counts
        .as_object()
        .map(|counts| counts.values().filter_map(Value::as_u64).sum::<u64>())
        .unwrap_or(0);
    json!({
        "contract_version": OPENSTREETMAP_DERIVED_DATABASE_METADATA_CONTRACT_VERSION,
        "source_of_truth": "rust_openstreetmap_data_provider",
        "provider_mode": "fixture",
        "fixture_source": "trillionnium_world_default_fixture_nodes",
        "fixture_source_revision": "openstreetmap_fixture_layers_v1",
        "imported_at_epoch": 0,
        "import_timestamp_policy": "stable_fixture_epoch_until_real_import_pipeline_exists",
        "transform_version": "trillionnium_osm_fixture_transform_v1",
        "derived_database_snapshot_id": format!(
            "osm-fixture-v1-n{}-l{}",
            nodes.len(), total_layer_features
        ),
        "node_feature_count": nodes.len(),
        "layer_feature_count": total_layer_features,
        "layer_feature_counts": layer_counts,
        "odbl": {
            "attribution": "© OpenStreetMap contributors",
            "database_license": "ODbL-1.0",
            "share_alike_note": "Track and publish derived database metadata before enabling live/imported OSM databases.",
            "derived_database_tracking_required": true,
            "public_tile_server_policy": "do_not_use_public_osm_tile_servers_for_production_traffic",
        },
        "readiness_checks": [
            "fixture_source_declared",
            "transform_version_declared",
            "odbl_share_alike_note_present",
            "live_import_disabled_until_tracking_exists"
        ],
    })
}

pub(super) fn openstreetmap_geodata_v1_json(
    nodes: &[WorldMapNode],
    current_node: Option<&WorldMapNode>,
) -> Value {
    let provider = FixtureOpenStreetMapDataProvider;
    let features = provider.features_json(nodes);
    let stable_fixture_count = features
        .iter()
        .filter(|feature| {
            feature
                .get("stable_fixture_identity")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let current_feature = current_node
        .map(|node| provider.feature_json(node))
        .unwrap_or(Value::Null);
    let fixture_layers = openstreetmap_fixture_layers_json(nodes);
    let derived_database_metadata =
        openstreetmap_derived_database_metadata_json(nodes, &fixture_layers);
    let provider_readiness =
        openstreetmap_provider_readiness_json(nodes, &fixture_layers, stable_fixture_count);
    json!({
        "kind": OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
        "contract_version": OPENSTREETMAP_GEODATA_CONTRACT_VERSION,
        "provider_contract": "OpenStreetMapDataProvider",
        "provider_id": provider.provider_id(),
        "provider_mode": "fixture",
        "provider_mode_contract_version": OPENSTREETMAP_PROVIDER_MODE_CONTRACT_VERSION,
        "provider_mode_status": openstreetmap_provider_mode_status_json("fixture"),
        "provider_modes": openstreetmap_provider_modes_json(),
        "provider_readiness_contract_version": OPENSTREETMAP_PROVIDER_READINESS_CONTRACT_VERSION,
        "provider_readiness": provider_readiness,
        "source_mode": provider.source_mode(),
        "source_of_truth": "rust_openstreetmap_data_provider",
        "web_role": "visualization_input_only",
        "gameplay_owner": "trillionnium_rust_world_state",
        "ingestion_stage": "fixture_mock_before_overpass_or_geofabrik",
        "fixture_identity_mode": "stable_fixture_table_with_deterministic_hash_fallback",
        "stable_fixture_count": stable_fixture_count,
        "feature_count": features.len(),
        "feature_identity_fields": ["osm_id", "osm_type", "lat", "lng", "tags", "game_overlay_id", "semantic_role", "objective_seed"],
        "osm_layers": ["roads", "pois", "buildings", "areas", "admin_boundaries", "tags"],
        "fixture_layers_contract_version": OPENSTREETMAP_FIXTURE_LAYERS_CONTRACT_VERSION,
        "fixture_layers": fixture_layers,
        "derived_database_metadata": derived_database_metadata,
        "derived_database_metadata_contract_version": OPENSTREETMAP_DERIVED_DATABASE_METADATA_CONTRACT_VERSION,
        "semantic_role_mapping": openstreetmap_semantic_role_mappings_json(),
        "production_ingestion_plan": {
            "live_overpass_enabled": false,
            "geofabrik_import_enabled": false,
            "cache_or_self_host_required_before_production_traffic": true,
            "public_tile_server_policy": "do_not_use_public_osm_tile_servers_for_production_traffic",
            "next_provider_modes": ["overpass_bbox_cache", "geofabrik_extract_import", "vendor_tile_cache"],
        },
        "legal": {
            "attribution": "© OpenStreetMap contributors",
            "database_license": "ODbL-1.0",
            "attribution_required": true,
            "odbl_database_obligations": true,
            "derived_database_tracking_required": true,
        },
        "current_feature": current_feature,
        "features": features,
        "readiness_checks": [
            "rust_provider_owns_geodata_projection",
            "web_shell_only_reads_projection_json",
            "osm_identity_fields_visible",
            "fixture_path_precedes_live_ingestion",
            "production_cache_required_before_osm_traffic",
            "stable_fixture_identity_table_present"
        ]
    })
}
