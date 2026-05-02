use super::*;

pub(super) async fn get_world_web_shell(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Html<String> {
    let web_session = authorize_league_web_session_readonly(&state, &headers, true)
        .ok()
        .flatten();
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
        "Authenticated web session: world actions are CSRF-protected and bound to the signed player."
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "Local-dev World shell: create ventures, craft assets, recruit Agents, and mirror real opportunities without exposing tokens to the browser."
    } else {
        "Read-only World shell: request a signed /league/web/session before submitting world actions."
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
                escape_html_text(&zone.status),
                escape_html_text(&zone.name),
                escape_html_text(&zone.theme),
                escape_html_text(&zone.zone_id),
                escape_html_text(&zone.mirror_kind),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let location_cards = locations
        .iter()
        .map(|location| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&location.name),
                escape_html_text(&location.description),
                escape_html_text(&location.location_id),
                escape_html_text(&location.location_kind),
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
                escape_html_text(&location.name),
                escape_html_text(&location.location_kind),
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
                escape_html_text(&node.name),
                escape_html_text(&node.node_kind),
                node.x,
                node.y,
                escape_html_text(&node.node_id),
                escape_html_text(&node.freedom_hooks.join(" / ")),
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
                node.name,
                node.description,
                node.exits.len()
            )
        })
        .unwrap_or_else(|| "Map booting".to_string());
    let world_map = world_map_json(&league, "@alice:local.dev");
    let world_viewport = world_map_viewport_json(
        &league.world,
        "@alice:local.dev",
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
        .unwrap_or("/v1/world/map/@alice:local.dev/viewport");
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
                map_region_focus_button_html(center_lat, center_lng, zoom_focus, "Focus region");
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
            let focus_button = map_node_focus_button_html(node_id, "Focus POI");
            format!(
                "<article class=\"mini poi\"><strong>{}</strong><span>{} · {} km</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(name),
                escape_html_text(node_kind),
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
                map_tile_focus_button_html(tile_z, tile_x, tile_y, "Inspect tile");
            format!(
                "<article class=\"mini tile\"><strong>{}</strong><span>{} · {} nodes</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(tile_status),
                escape_html_text(lod_mode),
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
            let focus_button = map_tile_focus_button_html(tile_z, tile_x, tile_y, "Warm tile");
            format!(
                "<article class=\"mini prefetch\"><strong>{}</strong><span>{} · {} nodes</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(priority),
                escape_html_text(reason),
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
                event_kind,
                node_name,
                event_body,
                event_result,
                "Track event",
            );
            format!(
                "<article class=\"mini event\"><strong>{}</strong><span>{} · {} km</span><code>{}</code><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(event_kind),
                escape_html_text(node_name),
                escape_html_text(&distance_km.to_string()),
                escape_html_text(event_id),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_density_summary = world_viewport
        .get("player_density")
        .and_then(|density| density.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("Map density booting.");
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
    let map_player_density_mode = world_viewport
        .get("player_density")
        .and_then(|density| density.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let world_map_data_json = serde_json::to_string(&world_map)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
    let latest_asset_id = world_indexes
        .latest_asset_index_for_owner("@alice:local.dev")
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
                "<article class=\"mini asset\"><strong>{}</strong><span>{} · Lv {} · value {}</span><code>{}</code><small>{}</small></article>",
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
        "<article class=\"mini asset\"><strong>No player assets yet</strong><span>Create a venture or craft build to mint the first asset.</span><code>/world action</code></article>".to_string()
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
                "<article class=\"mini company\"><strong>{}</strong><span>{} · Lv {} · revenue {}</span><code>{}</code><small>asset {} · rep {}</small></article>",
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
        "<article class=\"mini company\"><strong>No companies yet</strong><span>Use /company latest to turn an asset into an operating company.</span><code>/company latest</code></article>".to_string()
    } else {
        company_cards
    };
    let latest_company_id = world_indexes
        .latest_company_index_for_owner("@alice:local.dev")
        .and_then(|index| league.world.world_companies.get(index))
        .map(|company| company.company_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let shop_cards = indexed_recent(&league.world.world_shops, &world_indexes.recent_shop_indices, 8)
        .map(|shop| {
            format!(
                "<article class=\"mini shop\"><strong>{}</strong><span>{} · listings {} · GMV {}</span><code>{}</code><small>company {} · {}</small></article>",
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
        "<article class=\"mini shop\"><strong>No shops yet</strong><span>Launch a company to open the first storefront.</span><code>/company latest</code></article>".to_string()
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
                "<article class=\"mini listing\"><strong>{}</strong><span>{} · {} credits · quality {}</span><code>{}</code><small>shop {} · {}</small></article>",
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
        "<article class=\"mini listing\"><strong>No listings yet</strong><span>Use /sell latest to publish an offer.</span><code>/sell latest</code></article>".to_string()
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
                "<article class=\"mini purchase world-route-filter-item\" data-route-bucket=\"purchase\" data-location-id=\"{}\" data-purchase-id=\"{}\" data-listing-id=\"{}\" data-company-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} credits · {}</span><code>{}</code><small>listing {} · seller {} · buyer {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&purchase.purchase_id),
                escape_html_text(&purchase.listing_id),
                escape_html_text(&purchase.company_id),
                escape_html_text(&purchase.status),
                purchase.created_at_epoch,
                escape_html_text(&purchase.buyer_matrix_user_id),
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
        "<article class=\"mini purchase\"><strong>No purchases yet</strong><span>Buy a listing to create seller revenue and a work order.</span><code>/buy latest</code></article>".to_string()
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
                "<article class=\"mini work world-route-filter-item\" data-route-bucket=\"work_order\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-purchase-id=\"{}\" data-company-id=\"{}\" data-listing-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · value {}</span><code>{}</code><small>buyer {} · seller {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&work_order.work_order_id),
                escape_html_text(&work_order.purchase_id),
                escape_html_text(&work_order.company_id),
                escape_html_text(&work_order.listing_id),
                escape_html_text(&work_order.status),
                work_order.created_at_epoch,
                escape_html_text(&work_order.status),
                escape_html_text(&work_order.brief.chars().take(48).collect::<String>()),
                work_order.value_score,
                escape_html_text(&work_order.work_order_id),
                escape_html_text(&work_order.buyer_matrix_user_id),
                escape_html_text(&work_order.seller_matrix_user_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_order_cards = if work_order_cards.is_empty() {
        "<article class=\"mini work\"><strong>No work orders yet</strong><span>Purchases open jobs that sellers can fulfill next.</span><code>/work</code></article>".to_string()
    } else {
        work_order_cards
    };
    let latest_work_order_id = world_indexes
        .latest_work_order_index_for_actor("@alice:local.dev")
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
                "<article class=\"mini delivery world-route-filter-item\" data-route-bucket=\"delivery\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{:.1} · {}</span><code>{}</code><small>work {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&delivery.work_order_id),
                escape_html_text(&delivery.status),
                delivery.created_at_epoch,
                escape_html_text(&delivery.matrix_user_id),
                delivery.score,
                escape_html_text(&delivery.status),
                escape_html_text(&delivery.delivery_id),
                escape_html_text(&delivery.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_delivery_cards = if work_delivery_cards.is_empty() {
        "<article class=\"mini delivery\"><strong>No deliveries yet</strong><span>Sellers can deliver work after a listing is purchased.</span><code>/work deliver latest</code></article>".to_string()
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
                "<article class=\"mini acceptance world-route-filter-item\" data-route-bucket=\"acceptance\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · rep +{}</span><code>{}</code><small>work {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&acceptance.work_order_id),
                escape_html_text(&acceptance.status),
                acceptance.created_at_epoch,
                escape_html_text(&acceptance.matrix_user_id),
                escape_html_text(&acceptance.status),
                acceptance.reputation_delta,
                escape_html_text(&acceptance.acceptance_id),
                escape_html_text(&acceptance.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_acceptance_cards = if work_acceptance_cards.is_empty() {
        "<article class=\"mini acceptance\"><strong>No acceptances yet</strong><span>Buyers can accept delivered work to finish the service loop.</span><code>/work accept latest</code></article>".to_string()
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
                "<article class=\"mini rejection world-route-filter-item\" data-route-bucket=\"rejection\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · refund {}</span><code>{}</code><small>work {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&rejection.work_order_id),
                escape_html_text(&rejection.status),
                rejection.created_at_epoch,
                escape_html_text(&rejection.matrix_user_id),
                escape_html_text(&rejection.status),
                escape_html_text(&rejection.refund_status),
                escape_html_text(&rejection.rejection_id),
                escape_html_text(&rejection.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_rejection_cards = if work_rejection_cards.is_empty() {
        "<article class=\"mini rejection\"><strong>No rejections yet</strong><span>Buyers can reject delivered work to refund reserved funds.</span><code>/work reject latest</code></article>".to_string()
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
                "<article class=\"mini reopen world-route-filter-item\" data-route-bucket=\"reopen\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · reserve {}</span><code>{}</code><small>work {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&reopen.work_order_id),
                escape_html_text(&reopen.status),
                reopen.created_at_epoch,
                escape_html_text(&reopen.matrix_user_id),
                escape_html_text(&reopen.status),
                escape_html_text(&reopen.reserve_status),
                escape_html_text(&reopen.reopen_id),
                escape_html_text(&reopen.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_reopen_cards = if work_reopen_cards.is_empty() {
        "<article class=\"mini reopen\"><strong>No reopens yet</strong><span>Buyers can reopen a rejected work order, reserve funds again, and allow redelivery.</span><code>/work reopen latest</code></article>".to_string()
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
                "<article class=\"mini cancellation world-route-filter-item\" data-route-bucket=\"cancellation\" data-location-id=\"{}\" data-work-order-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · refund {}</span><code>{}</code><small>work {}</small></article>",
                escape_html_text(location_id),
                escape_html_text(&cancellation.work_order_id),
                escape_html_text(&cancellation.status),
                cancellation.created_at_epoch,
                escape_html_text(&cancellation.matrix_user_id),
                escape_html_text(&cancellation.status),
                escape_html_text(&cancellation.refund_status),
                escape_html_text(&cancellation.cancellation_id),
                escape_html_text(&cancellation.work_order_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let work_cancellation_cards = if work_cancellation_cards.is_empty() {
        "<article class=\"mini cancellation\"><strong>No cancellations yet</strong><span>Buyers can cancel open work before delivery and refund reserved funds.</span><code>/work cancel latest</code></article>".to_string()
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
                "<article class=\"mini faction\"><strong>{}</strong><span>{} · rep {}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&faction.name),
                escape_html_text(&faction.faction_kind),
                faction.reputation_score,
                escape_html_text(&faction.faction_id),
                escape_html_text(&faction.zone_id),
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
                "<article class=\"mini standing\"><strong>{}</strong><span>{} rep · {}</span><code>{}</code><small>{}</small></article>",
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
        "<article class=\"mini standing\"><strong>No faction standings yet</strong><span>Commerce and work will build reputation with city factions.</span><code>/factions</code></article>".to_string()
    } else {
        standing_cards
    };
    let latest_contract_id = world_indexes
        .latest_contract_index_for_actor("@alice:local.dev")
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
                "<article class=\"mini contract world-route-filter-item\" data-route-bucket=\"contract\" data-location-id=\"{}\" data-contract-id=\"{}\" data-task-id=\"{}\" data-route-status=\"{}\" data-created-at=\"{}\"><strong>{}</strong><span>{} · value {}</span><code>{}</code><small>task {} · {}</small></article>",
                escape_html_text(&contract.location_id),
                escape_html_text(&contract.contract_id),
                escape_html_text(&contract.task_id),
                escape_html_text(&contract.status),
                contract.created_at_epoch,
                escape_html_text(&contract.title),
                escape_html_text(&contract.status),
                contract.value_score,
                escape_html_text(&contract.location_id),
                escape_html_text(&contract.task_id),
                escape_html_text(contract.cex_status.as_deref().unwrap_or("created")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let contract_cards = if contract_cards.is_empty() {
        "<article class=\"mini contract\"><strong>No World contracts yet</strong><span>Use /contract to mirror a real need into CEX execution.</span><code>/contract</code></article>".to_string()
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
                &event.event_kind,
                &event.location_id,
                &event.body,
                &event.result,
                "Track event",
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
                escape_html_text(&event.event_kind),
                escape_html_text(&event.body),
                escape_html_text(&event.location_id),
                event.impact_score,
                escape_html_text(event.cex_task_id.as_deref().unwrap_or("no-task")),
                escape_html_text(&event.result),
                focus_button,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let event_items = if event_items.is_empty() {
        "<li><b>🌍 World is waiting</b><span>Use the action console to create the first world event.</span><small>/world action</small><em>Reality mirror booting.</em></li>".to_string()
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
        "Filter activity by focus",
        "Show all activity",
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
    @media (max-width:1050px) {{ header,.play {{ grid-template-columns:1fr; }} .grid,.stats,.mini-grid {{ grid-template-columns:1fr; }} }}
  </style>
</head>
<body>
  <header>
    <section>
      <div class="pill">Reality Mirror Sandbox</div>
      <h1>Trillionnium World</h1>
      <p class="subtitle">开放世界总层：现实镜像城市、Craft 工坊、Market、League 竞技场、Agent 居民、资产、关系与自由行动。League 是竞技模块，Craft 是建造模块，Ledger 是结算层。</p>
    </section>
    <aside class="hero-card">
      <strong>World Shell Online</strong>
      <p class="subtitle">Build a company, craft an asset, recruit Agents, enter markets, or jump into League competition.</p>
      <a id="world-league-link" class="cta" href="/league">Enter League Arena</a>
    </aside>
  </header>
  <main>
    <section class="stats">
      <div class="stat"><span>Zones</span><b>{zones}</b></div>
      <div class="stat"><span>Locations</span><b>{locations}</b></div>
      <div class="stat"><span>Agents</span><b>{entities}</b></div>
      <div class="stat"><span>Map Nodes</span><b>{map_nodes}</b></div>
      <div class="stat"><span>Assets</span><b>{assets}</b></div>
      <div class="stat"><span>Upgrades</span><b>{asset_upgrades}</b></div>
      <div class="stat"><span>Companies</span><b>{companies}</b></div>
      <div class="stat"><span>Shops</span><b>{shops}</b></div>
      <div class="stat"><span>Listings</span><b>{listings}</b></div>
      <div class="stat"><span>Purchases</span><b>{purchases}</b></div>
      <div class="stat"><span>Work</span><b>{work_orders}</b></div>
      <div class="stat"><span>Rejected</span><b>{work_rejections}</b></div>
      <div class="stat"><span>Reopened</span><b>{work_reopens}</b></div>
      <div class="stat"><span>Cancelled</span><b>{work_cancellations}</b></div>
      <div class="stat"><span>Factions</span><b>{factions}</b></div>
      <div class="stat"><span>Contracts</span><b>{contracts}</b></div>
      <div class="stat"><span>Done</span><b>{completions}</b></div>
      <div class="stat"><span>Events</span><b>{events}</b></div>
      <div class="stat"><span>Relations</span><b>{relationships}</b></div>
    </section>
    <section class="panel">
      <div class="map-shell">
        <div>
          <div class="pill">Global Real-world Map Engine</div>
          <h2>{map_engine_name} + {tile_provider}</h2>
          <p class="subtitle">地图引擎现在就是 UI 主入口：底层以 <code>{mirror_scope}</code> 作为全量真实世界镜像，策略为 <code>{full_mirror_strategy}</code>，表现层采用 <code>{simplification_style}</code>，目标是 <code>{scaling_goal}</code>。</p>
          <div id="world-tile-shards-live" class="mini-grid">{tile_shard_cards}</div>
          <div id="world-region-shards-live" class="mini-grid">{region_shard_cards}</div>
          <div class="mini-grid" style="margin-top:12px">{lod_layer_cards}</div>
          <div id="world-poi-hotspots-live" class="mini-grid" style="margin-top:12px">{hotspot_cards}</div>
          <div id="world-prefetch-queue-live" class="mini-grid" style="margin-top:12px">{prefetch_cards}</div>
          <div id="world-live-events-live" class="mini-grid" style="margin-top:12px">{live_event_cards}</div>
          <p class="subtitle">Viewport API: <code>{viewport_path}</code></p>
          <p class="subtitle">Web Viewport: <code>{web_session_viewport_path}</code></p>
          <p id="world-map-density-summary" class="subtitle">{map_density_summary}</p>
          <p id="world-map-camera-summary" class="subtitle">Camera booting…</p>
          <div id="world-map-stream-hud" class="map-stream-hud">
            <span class="hud-chip"><strong>{map_stream_region_count}</strong> region shards</span>
            <span class="hud-chip"><strong>{map_visible_marker_count}</strong> visible nodes</span>
            <span class="hud-chip"><strong>{map_prefetch_count}</strong> prefetch tiles</span>
            <span class="hud-chip"><strong>{map_live_event_count}</strong> live events · {map_player_density_mode}</span>
          </div>
          <div id="world-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
          </div>
          <div id="world-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
          </div>
          <p id="world-map-overlay-status" class="subtitle">Active overlays: density, regions, tiles, prefetch, live events.</p>
          <div class="mini" style="margin-top:14px;">
            <strong>Map focus action rail</strong>
            <span id="world-map-focus-summary">Waiting for viewport focus…</span>
            <small id="world-map-focus-detail">Pick a region, tile, hotspot, or live event to steer movement and world actions.</small>
            <div id="world-map-action-rail" class="focus-stack"></div>
          </div>
          <p id="world-map-route-filter-status" class="subtitle">Focused route filter: showing all world activity.</p>
          <div id="world-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <p id="world-map-route-flow-status" class="subtitle">Focused world flow: waiting for a map-driven route.</p>
          <p id="world-map-route-next-step-status" class="subtitle">Recommended next step: pick a map focus first.</p>
          <p id="world-map-route-event-brief-status" class="subtitle">Focused event brief: waiting for a live event focus.</p>
          <p id="world-map-route-link-status" class="subtitle">Linked task route: none yet.</p>
          <div id="world-map-route-flow-actions" class="focus-stack"></div>
          <p id="world-map-overlay-legend" class="subtitle">Overlay legend: region anchors · active tile frames · prefetch warm ring · live event pulses.</p>
        </div>
        <div id="world-real-map" data-engine="{map_engine_id}" data-provider="{tile_provider}" aria-label="World real-world map engine"></div>
      </div>
    </section>
    <section>
      <h2>World Zones</h2>
      <div class="grid">{zone_cards}</div>
    </section>
    <section id="world-map-move-panel" class="panel">
      <h2>Detailed World Map</h2>
      <p class="subtitle">{current_map_summary}</p>
      <div class="mini-grid">{map_cards}</div>
      <form method="post" action="/world/web/map-move" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <select id="world-map-move-target" name="target">{map_exit_options}</select>
        <button type="submit">Move on Map</button>
      </form>
    </section>
    <section class="play">
      <div id="world-action-console" class="panel">
        <h2>World Action Console</h2>
        <p id="world-action-console-status" class="subtitle">{console_note}</p>
        <form method="post" action="/world/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
          <select id="world-action-location" name="location_id">{location_options}</select>
          <textarea id="world-action-body" name="body">我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户，并把客户需求转成 League 任务。</textarea>
          <button type="submit">Commit World Action</button>
        </form>
      </div>
      <div class="panel">
        <h2>World Event Timeline</h2>
        <ul id="world-event-timeline" class="timeline">{event_items}</ul>
      </div>
    </section>
    <section class="panel">
      <h2>Locations</h2>
      <div class="mini-grid">{location_cards}</div>
    </section>
    <section class="panel">
      <h2>Agent Residents / NPCs</h2>
      <div class="mini-grid">{entity_cards}</div>
    </section>
    <section id="world-assets-panel" class="panel">
      <h2>Player Assets</h2>
      <div class="mini-grid">{asset_cards}</div>
      <form method="post" action="/world/web/asset" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="latest or world-asset-id" />
        <textarea id="world-asset-body" name="body">Upgrade this World asset with a stronger offer, proof, risk control, operating loop, and next customer path.</textarea>
        <button type="submit">Upgrade Asset</button>
      </form>
    </section>
    <section id="world-companies-panel" class="panel">
      <h2>Companies / Shops</h2>
      <div class="mini-grid">{company_cards}</div>
      <form method="post" action="/world/web/company" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-company-asset-id" name="asset_id" value="{latest_asset_id}" placeholder="latest or world-asset-id" />
        <textarea id="world-company-body" name="body">Launch a shop/company from this asset with offer, customer segment, operating loop, proof, and first revenue path.</textarea>
        <button type="submit">Launch Company</button>
      </form>
    </section>
    <section id="world-listings-panel" class="panel">
      <h2>Shops / Listings</h2>
      <div class="mini-grid">{shop_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{listing_cards}</div>
      <form method="post" action="/world/web/listing" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-listing-company-id" name="company_id" value="{latest_company_id}" placeholder="latest or world-company-id" />
        <textarea id="world-listing-body" name="body">Publish a service listing with clear deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.</textarea>
        <button type="submit">Publish Listing</button>
      </form>
    </section>
    <section id="world-commerce-panel" class="panel">
      <h2>Commerce / Work Orders</h2>
      <div id="world-purchase-cards-live" class="mini-grid">{purchase_cards}</div>
      <div id="world-work-orders-live" class="mini-grid" style="margin-top:12px">{work_order_cards}</div>
      <form id="world-buy-form" method="post" action="/world/web/buy" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-buy-listing-id" name="listing_id" value="{latest_listing_id}" placeholder="latest or world-listing-id" />
        <textarea id="world-buy-body" name="body">Buy this service and open a work order with deliverable, evidence package, acceptance standard, risk controls, and next action.</textarea>
        <button type="submit">Buy / Hire Listing</button>
      </form>
      <div id="world-work-deliveries-live" class="mini-grid" style="margin-top:12px">{work_delivery_cards}</div>
      <form id="world-work-deliver-form" method="post" action="/world/web/work-deliver" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-work-deliver-id" name="work_order_id" value="{latest_work_order_id}" placeholder="latest or world-work-id" />
        <textarea id="world-work-deliver-body" name="body">Work delivery package: deliverable, evidence package, acceptance checklist, risk review, next action, and self-review.</textarea>
        <button type="submit">Deliver Work Order</button>
      </form>
      <div id="world-work-acceptances-live" class="mini-grid" style="margin-top:12px">{work_acceptance_cards}</div>
      <form id="world-work-accept-form" method="post" action="/world/web/work-accept" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-work-accept-id" name="work_order_id" value="{latest_work_order_id}" placeholder="latest or world-work-id" />
        <textarea id="world-work-accept-body" name="body">Buyer acceptance: delivered work accepted with proof, quality note, next collaboration, and reputation confirmation.</textarea>
        <button type="submit">Accept Work Order</button>
      </form>
      <div id="world-work-rejections-live" class="mini-grid" style="margin-top:12px">{work_rejection_cards}</div>
      <form id="world-work-reject-form" method="post" action="/world/web/work-reject" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-work-reject-id" name="work_order_id" value="{latest_work_order_id}" placeholder="latest or world-work-id" />
        <textarea id="world-work-reject-body" name="body">Buyer rejection: delivery is not accepted, refund the reserved buyer funds, reopen with revision requirements, evidence gaps, and next action.</textarea>
        <button type="submit">Reject / Refund Work Order</button>
      </form>
      <div id="world-work-reopens-live" class="mini-grid" style="margin-top:12px">{work_reopen_cards}</div>
      <form id="world-work-reopen-form" method="post" action="/world/web/work-reopen" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-work-reopen-id" name="work_order_id" value="{latest_work_order_id}" placeholder="latest or world-work-id" />
        <textarea id="world-work-reopen-body" name="body">Buyer reopen: reserve funds again, list revision requirements, evidence gaps, acceptance standard, and next redelivery action.</textarea>
        <button type="submit">Reopen / Reserve Again</button>
      </form>
      <div id="world-work-cancellations-live" class="mini-grid" style="margin-top:12px">{work_cancellation_cards}</div>
      <form id="world-work-cancel-form" method="post" action="/world/web/work-cancel" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-work-cancel-id" name="work_order_id" value="{latest_work_order_id}" placeholder="latest or world-work-id" />
        <textarea id="world-work-cancel-body" name="body">Buyer cancel: cancel this open work before delivery, refund reserved buyer funds, record reason, and close the work order.</textarea>
        <button type="submit">Cancel / Refund Work Order</button>
      </form>
    </section>
    <section class="panel">
      <h2>Faction Reputation Map</h2>
      <div class="mini-grid">{faction_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{standing_cards}</div>
    </section>
    <section id="world-contracts-panel" class="panel">
      <h2>World Contracts</h2>
      <div id="world-contract-cards-live" class="mini-grid">{contract_cards}</div>
      <form id="world-contract-completion-form" method="post" action="/world/web/contract" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input id="world-contract-completion-id" name="contract_id" value="{latest_contract_id}" placeholder="world-contract-id" />
        <textarea id="world-contract-completion-body" name="body">World contract delivery: deliverable, evidence, risk review, next step, acceptance standard.</textarea>
        <button type="submit">Complete Contract</button>
      </form>
    </section>
    <section class="panel">
      <h2>Task-linked Route Graph</h2>
      <div id="world-route-task-graph-live" class="mini-grid">{world_route_task_graph_cards}</div>
    </section>
    <section class="panel">
      <h2>Playable Commands</h2>
      <p class="subtitle"><code>/world</code> <code>/world action 我要开一家 AI 设计公司</code> <code>/league</code> <code>/arena</code> <code>/guild</code> <code>/raid</code></p>
    </section>
  </main>
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
          cameraSummary.textContent = 'World handoff: ' + state.actionLabel + ' · ' + state.command + (handoffEventId ? (' · event ' + handoffEventId) : (handoffNodeId ? (' · focus ' + handoffNodeId) : '')) + (state.routeTaskId ? (' · task ' + state.routeTaskId) : '');
        }}
        if (focusDetail) {{
          focusDetail.textContent = 'Prepared from /app: ' + state.actionLabel + ' · panel ' + state.panelId + ' · ' + state.actionBody + (handoffEventId ? (' · event ' + handoffEventId) : '') + (state.routeTaskId ? (' · task ' + state.routeTaskId) : '');
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
          cameraSummary.textContent = 'Route focus prepared: ' + (focusedEventId ? ('event ' + focusedEventId) : focusedNodeId) + ' · panel ' + (target.panelId || routeActionPanelId());
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
      const buildRouteActionDraft = (selection, routeContext) => buildRouteDraftBody(selection, routeContext, {{ selectionTitleFallback: 'Focused world route' }});
      const renderWorldTaskGraph = (tasks) => {{
        if (!routeTaskGraphTarget) return;
        const visible = (tasks && tasks.length ? tasks : routeTaskGraphItems).slice(0, 6);
        routeTaskGraphTarget.innerHTML = visible.length ? visible.map((task) => {{
          const actionButtons = routeTaskGraphActionButtonsHtml(task);
          return `<article class="mini task-graph"><strong>${{escapeHtml(task.task_id || 'task')}}</strong><span>${{escapeHtml(task.latest_bucket || 'event')}} · ${{escapeHtml(task.latest_status || 'pending')}} · opportunity ${{escapeHtml(task.next_opportunity_kind || 'contract_capture')}}</span><code>${{escapeHtml(task.latest_location_id || task.task_id || 'route')}}</code><small>${{escapeHtml(task.event_count ?? 0)}} events · ${{escapeHtml(task.contract_count ?? 0)}} contracts · ${{escapeHtml(task.completion_count ?? 0)}} completions</small><small>${{escapeHtml(task.outcome_summary || 'Outcome summary pending.')}}</small><small><strong>Opportunity lane</strong> · ${{escapeHtml(task.next_opportunity_hint || 'Opportunity hint pending.')}}</small><div class="focus-stack"><code>${{escapeHtml(task.next_opportunity_command || '/world action 继续推进下一步机会。')}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
        }}).join('') : '<article class="mini task-graph"><strong>No task-linked routes yet</strong><span>Create a world contract or task-linked event to grow the graph.</span><code>task graph</code></article>';
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
        statusPrefix: 'Recommended next step',
        rejectionBody: (selectionTitle, workOrderId) => selectionTitle + ': reopen work order ' + workOrderId + ' with revision requirements, evidence gaps, renewed reserve, and next redelivery step.',
        rejectionStatus: (workOrderId) => 'reopen work order ' + workOrderId + ' for this focused route.',
        reopenBody: (selectionTitle, workOrderId) => selectionTitle + ': redeliver work order ' + workOrderId + ' with revised deliverable, evidence package, acceptance checklist, and risk review.',
        reopenStatus: (workOrderId) => 'redeliver work order ' + workOrderId + ' after reopen.',
        deliveryBody: (selectionTitle, workOrderId) => selectionTitle + ': review delivery for work order ' + workOrderId + ', confirm proof and quality, then accept or reject with concrete next action.',
        deliveryStatus: (workOrderId) => 'review the latest delivery for work order ' + workOrderId + '.',
        openWorkBody: (selectionTitle, workOrderId) => selectionTitle + ': prepare delivery for work order ' + workOrderId + ' with deliverable, evidence, acceptance checklist, and next action.',
        openWorkStatus: (workOrderId) => 'deliver the active work order ' + workOrderId + '.',
        closedWorkLabel: 'Draft follow-up action',
        closedWorkBody: (selectionTitle, workOrderId, latestWorkBucket) => selectionTitle + ': follow up after ' + latestWorkBucket + ' on work order ' + workOrderId + '. Capture outcome, next collaboration, and world-state consequences for this route.',
        closedWorkStatus: (workOrderId, latestWorkBucket) => 'draft a follow-up world action after ' + latestWorkBucket + ' on work order ' + workOrderId + '.',
        contractBody: (selectionTitle, contractId) => selectionTitle + ': complete contract ' + contractId + ' with deliverable, evidence, risk review, acceptance standard, and next step.',
        contractStatus: (contractId) => 'complete contract ' + contractId + ' for this route.',
        listingBody: (selectionTitle, listingId) => selectionTitle + ': hire listing ' + listingId + ' and define deliverable, evidence, acceptance, risk control, and next action.',
        listingStatus: (listingId) => 'route the active listing ' + listingId + ' into a hire/workflow.',
        defaultBody: (selectionTitle) => selectionTitle + ': draft the next world action from this focused route, including evidence, risk, and next operational move.',
        defaultStatus: () => 'draft a world action for this focused route.',
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
          routeFilterStatus.textContent = 'Focused route filter: showing all world activity.';
        }} else {{
          const workflowCount = (counts.purchase || 0) + (counts.work_order || 0) + (counts.delivery || 0) + (counts.acceptance || 0) + (counts.rejection || 0) + (counts.reopen || 0) + (counts.cancellation || 0);
          routeFilterStatus.textContent = 'Focused route filter: ' + (selection.title || 'focus') + (selectedTaskId ? (' · task ' + selectedTaskId) : '') + ' · ' + (counts.event || 0) + ' events · ' + (counts.contract || 0) + ' contracts · ' + workflowCount + ' commerce/work items.';
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
            pushRouteFlowActionButton(actions, actionKeys, buildTaskFollowUpAction(selection, activeTaskId, locationId, 'with linked event/contract context, current world-state evidence, risks, and next action'));
          }}
          if (eventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction('Open event timeline', {{
              eventId: String(eventItem.dataset.eventId || ''),
              eventKind: String(eventItem.dataset.eventKind || 'world_event'),
              eventBody: String(eventItem.dataset.eventBody || ''),
              eventResult: String(eventItem.dataset.eventResult || ''),
              eventTaskId: String(eventItem.dataset.taskId || ''),
              locationId: String(eventItem.dataset.locationId || ''),
            }}));
          }}
          if (activeTaskId && linkedEventItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildRouteEventTimelineAction('Open linked event', {{
              eventId: String(linkedEventItem.dataset.eventId || ''),
              eventKind: String(linkedEventItem.dataset.eventKind || 'world_event'),
              eventBody: String(linkedEventItem.dataset.eventBody || ''),
              eventResult: String(linkedEventItem.dataset.eventResult || ''),
              eventTaskId: String(linkedEventItem.dataset.taskId || ''),
              locationId: String(linkedEventItem.dataset.locationId || ''),
            }}));
          }}
          if (workOrderId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('delivery', workOrderId, {{ label: 'Route work order', locationId }}));
          }}
          if (contractId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldContractLaneAction(contractId, {{ locationId }}));
          }}
          if (activeTaskId && linkedContractItem) {{
            pushRouteFlowActionButton(actions, actionKeys, buildLinkedContractRouteAction(selection, String(linkedContractItem.dataset.contractId || ''), activeTaskId, locationId, {{ bodySuffix: ' with evidence, acceptance standard, and next step.' }}));
          }}
          if (listingId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldPurchaseLaneAction(listingId, {{ locationId }}));
          }}
          if (((nextStep || {{}}).label) === 'Route acceptance' && workOrderId) {{
            pushRouteFlowActionButton(actions, actionKeys, buildWorldWorkLaneAction('rejection', workOrderId, {{
              locationId,
              body: appendSelectionEventSignal((selection && selection.title ? selection.title : 'Focused route') + ': reject delivery for work order ' + workOrderId + ' with evidence gaps, refund logic, and revision path.', selection),
            }}));
          }}
          routeFlowActions.innerHTML = actions.join(' ');
        }}
        if (!routeFlowStatus) return;
        if (routeFilterMode === 'all' || !selection) {{
          routeFlowStatus.textContent = 'Focused world flow: waiting for a map-driven route.';
          if (routeNextStepStatus) routeNextStepStatus.textContent = 'Recommended next step: pick a map focus first.';
          if (routeEventBriefStatus) routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, true);
          if (routeLinkStatus) routeLinkStatus.textContent = routeLinkStatusText({{ emptyText: 'Linked task route: none yet.' }});
          if (actionConsoleStatus) actionConsoleStatus.textContent = defaultActionConsoleStatus || 'Use the world action console to commit world actions.';
          return;
        }}
        const contractLabel = contractId || 'no contract';
        const workLabel = workOrderId || 'no work order';
        routeFlowStatus.textContent = 'Focused world flow: ' + (selection.title || 'focus') + ' → event ' + eventLabel + ' · work ' + workLabel + ' · contract ' + contractLabel + routeOpportunitySegment(opportunityTask) + '.';
        if (routeNextStepStatus) {{
          routeNextStepStatus.textContent = ((nextStep || {{}}).status) || 'Recommended next step: draft a world action for this focused route.';
        }}
        if (routeEventBriefStatus) {{
          routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, false);
        }}
        if (routeLinkStatus) {{
          routeLinkStatus.textContent = routeLinkStatusText({{ taskId: activeTaskId, linkedEventCount, linkedContractCount, opportunityTask, inFocus: true, emptyText: 'Linked task route: no event/contract task link in the current focus.' }});
        }}
        if (actionConsoleStatus) {{
          actionConsoleStatus.textContent = 'Focused world action: ' + (selection.title || 'focus') + ' · ' + (locationId || 'no location') + ' · event ' + eventLabel + ' · work ' + workLabel + ' · contract ' + contractLabel + routeOpportunitySegment(opportunityTask) + ' · next ' + (((nextStep || {{}}).label) || 'draft world action') + '.';
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
          emptyDetail: 'Pick a region, tile, hotspot, or live event to steer movement and world actions.',
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
        if (cameraSummary) {{ cameraSummary.textContent = 'Selected map action: ' + state.actionLabel + ' · ' + state.command; }}
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
        if (handleRouteActionButton(routeFlowButton, openRouteFlowAction, 'Route action')) return;
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
        map_density_summary = escape_html_text(map_density_summary),
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
        event_items = event_items,
        console_note = escape_html_text(console_note),
        csrf_input = csrf_input,
        world_map_data_json = world_map_data_json,
    ))
}
