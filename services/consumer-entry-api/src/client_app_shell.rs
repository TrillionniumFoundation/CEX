use super::*;

pub(super) async fn get_client_app_web_shell(
    State(state): State<AppState>,
    _headers: HeaderMap,
) -> Html<String> {
    let league = state.inner.league_state.lock().await;
    let app = client_app_json(&league, "@alice:local.dev");
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
                escape_html_text(name),
                escape_html_text(style),
                escape_html_text(summary),
                escape_html_text(command),
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
                "<article class=\"module\"><strong>{}</strong><span>{} · {}</span><p>消息、协作、合同推进与 world action 的联系人入口。</p><code>{}</code></article>",
                escape_html_text(name),
                escape_html_text(kind),
                escape_html_text(role),
                escape_html_text(location),
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
                "<article class=\"module\"><strong>Task Thread</strong><span>{} · {} / {}</span><p>{}</p><code>{}</code></article>",
                escape_html_text(&task.task_id),
                escape_html_text(&task.latest_bucket),
                escape_html_text(&task.latest_status),
                escape_html_text(&task.outcome_summary),
                escape_html_text(&task.next_opportunity_hint),
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
        "<article class=\"module\"><strong>消息</strong><span>WeChat / Telegram loop</span><p>这里会显示联系人、Agent、系统通知、合同线程和任务协作入口。</p><code>/social</code></article>".to_string()
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
                escape_html_text(summary),
                escape_html_text(command),
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
                map_region_focus_button_html(center_lat, center_lng, zoom_focus, "Focus region");
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
            let focus_button = map_node_focus_button_html(node_id, "Focus POI");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{}</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(name),
                escape_html_text(node_kind),
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
                map_tile_focus_button_html(tile_z, tile_x, tile_y, "Inspect tile");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} · {} nodes</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(tile_status),
                escape_html_text(lod_mode),
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
            let focus_button = map_tile_focus_button_html(tile_z, tile_x, tile_y, "Warm tile");
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} nodes</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(priority),
                escape_html_text(reason),
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
                event_kind,
                node_name,
                event_body,
                event_result,
                "Track event",
            );
            format!(
                "<article class=\"module\"><strong>{}</strong><span>{} · {} km</span><p><code>{}</code></p><div class=\"focus-stack\">{}</div></article>",
                escape_html_text(event_kind),
                escape_html_text(node_name),
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
    let map_player_density_mode = map_hub
        .and_then(|hub| hub.get("player_density_mode"))
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let onboarding = app.get("onboarding");
    let onboarding_label = onboarding
        .and_then(|rail| rail.get("rail_label"))
        .and_then(Value::as_str)
        .unwrap_or("新手主线：从地图到成交");
    let onboarding_goal = onboarding
        .and_then(|rail| rail.get("primary_goal"))
        .and_then(Value::as_str)
        .unwrap_or("把地图焦点推进成 world action、contract、commerce work order、delivery、acceptance 和 reward。");
    let onboarding_completion_target = onboarding
        .and_then(|rail| rail.get("completion_target"))
        .and_then(Value::as_str)
        .unwrap_or("first_playable_loop_100");
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
                "<article class=\"module onboarding-step\" data-onboarding-step=\"{}\"><strong>{}</strong><span>{} · {}</span><p>{}</p><code>{}</code><p class=\"subtitle\">Success: <code>{}</code></p></article>",
                escape_html_text(step_id),
                escape_html_text(label),
                escape_html_text(surface),
                escape_html_text(status),
                escape_html_text(description),
                escape_html_text(command),
                escape_html_text(success_signal),
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
                escape_html_text(&check)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let app_data_json = serde_json::to_string(&app)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
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
        "Filter route by focus",
        "Show full route",
    );
    let shared_map_route_target_resolution_js = real_world_map_route_target_resolution_js();
    let shared_map_route_status_js = real_world_map_route_status_js();
    let shared_map_route_contract_js = real_world_map_route_contract_js();
    let shared_map_route_action_js = real_world_map_route_action_js();
    let shared_map_viewport_hydration_js = real_world_map_viewport_hydration_js();
    let shared_map_render_cards_js =
        real_world_map_render_cards_js(RealWorldMapShellCardStyle::AppModule);
    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium Client App</title>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
  <style>
    :root {{ color-scheme: dark; --bg:#070814; --panel:#14182d; --gold:#f8c35b; --cyan:#64e3ff; --text:#f6f7fb; --muted:#a6adbb; }}
    body {{ margin:0; min-height:100vh; font-family:Inter, ui-sans-serif, system-ui, sans-serif; background:radial-gradient(circle at 20% 0%, #153f58, transparent 32rem), var(--bg); color:var(--text); padding-bottom:88px; }}
    header {{ position:sticky; top:0; z-index:20; padding:18px min(5vw,32px) 16px; backdrop-filter:blur(18px); background:linear-gradient(180deg, rgba(7,8,20,.96), rgba(7,8,20,.78)); border-bottom:1px solid rgba(255,255,255,.08); }}
    main {{ padding:18px min(5vw,32px) 34px; }}
    h1 {{ margin:0; font-size:clamp(28px,5.6vw,52px); letter-spacing:-.06em; }}
    .subtitle {{ color:var(--muted); max-width:850px; line-height:1.55; }}
    .grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .map-shell {{ display:grid; grid-template-columns:minmax(260px,.8fr) minmax(320px,1.2fr); gap:18px; align-items:stretch; margin-bottom:22px; }}
    .map-panel {{ border:1px solid rgba(255,255,255,.12); border-radius:26px; background:rgba(255,255,255,.07); padding:22px; box-shadow:0 20px 70px rgba(0,0,0,.35); }}
    #real-world-map {{ min-height:430px; border-radius:26px; overflow:hidden; border:1px solid rgba(100,227,255,.28); box-shadow:0 24px 90px rgba(0,0,0,.45); background:#0b1220; }}
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
    .module p,.subtitle {{ color:var(--muted); }}
    .map-panel p {{ color:var(--muted); line-height:1.55; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    a {{ color:var(--gold); }}
    .app-mobile-shell {{ display:grid; gap:18px; }}
    .app-topbar-meta {{ display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; }}
    .app-search-shell {{ position:relative; display:flex; gap:12px; align-items:center; }}
    .app-search-input {{ width:100%; border-radius:18px; border:1px solid rgba(255,255,255,.12); background:rgba(255,255,255,.08); color:var(--text); padding:14px 16px; font-size:15px; box-shadow:0 10px 30px rgba(0,0,0,.18) inset; }}
    .app-search-input::placeholder {{ color:rgba(246,247,251,.56); }}
    .app-search-clear {{ flex:0 0 auto; border:1px solid rgba(248,195,91,.3); background:rgba(248,195,91,.1); color:var(--gold); border-radius:14px; padding:11px 12px; font-weight:900; cursor:pointer; }}
    .app-search-clear[hidden] {{ display:none; }}
    .app-ux-status {{ display:flex; align-items:center; gap:8px; margin-top:10px; min-height:28px; }}
    .app-ux-pill {{ display:inline-flex; align-items:center; max-width:100%; border:1px solid rgba(100,227,255,.22); background:rgba(100,227,255,.08); color:var(--cyan); border-radius:999px; padding:6px 10px; font-size:12px; font-weight:900; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
    .app-ux-pill[data-state="loading"] {{ color:var(--gold); border-color:rgba(248,195,91,.32); background:rgba(248,195,91,.1); }}
    .app-ux-pill[data-state="offline"], .app-ux-pill[data-state="fallback"] {{ color:#ffb48a; border-color:rgba(255,180,138,.32); background:rgba(255,120,70,.1); }}
    .app-search-empty {{ display:none; margin-top:10px; border:1px dashed rgba(255,255,255,.16); border-radius:16px; padding:10px 12px; color:var(--muted); background:rgba(255,255,255,.04); }}
    .app-search-empty.is-visible {{ display:block; }}
    .sr-only {{ position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0; }}
    .app-tab-panel {{ display:none; gap:16px; }}
    .app-tab-panel.is-active {{ display:grid; }}
    .app-tab-header {{ display:grid; gap:6px; margin-bottom:4px; }}
    .app-bottom-tabs {{ position:fixed; left:0; right:0; bottom:0; z-index:30; display:grid; grid-template-columns:repeat(4,1fr); gap:8px; padding:10px min(4vw,24px) calc(10px + env(safe-area-inset-bottom, 0px)); border-top:1px solid rgba(255,255,255,.08); background:rgba(8,10,24,.92); backdrop-filter:blur(18px); }}
    .app-bottom-tab {{ border:1px solid rgba(255,255,255,.1); background:rgba(255,255,255,.05); color:var(--muted); border-radius:16px; padding:10px 8px; font-weight:800; cursor:pointer; }}
    .app-bottom-tab.is-active {{ color:var(--text); background:rgba(100,227,255,.12); border-color:rgba(100,227,255,.32); }}
    .app-bottom-tab:focus-visible, .focus-chip:focus-visible, .overlay-toggle:focus-visible, .app-search-input:focus-visible, .app-search-clear:focus-visible {{ outline:2px solid var(--cyan); outline-offset:2px; }}
    .app-me-grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }}
    .app-search-hidden {{ display:none !important; }}
    @media (max-width: 820px) {{ .map-shell {{ grid-template-columns:1fr; }} #real-world-map {{ min-height:360px; }} main {{ padding:14px 16px 34px; }} header {{ padding:14px 16px 12px; }} }}
  </style>
</head>
<body>
  <header>
    <div class="app-topbar-meta">
      <p><code>client app shell v1</code></p>
      <p><a href="/world">World</a> · <a href="/league">League</a></p>
    </div>
    <h1>Trillionnium Client App</h1>
    <p class="subtitle">一个手机端超级入口：顶部全局搜索，底部四栏——消息 / 世界 / 动态 / 我。当前位置：<strong>{}</strong></p>
    <div class="app-search-shell">
      <input id="app-global-search" class="app-search-input" type="search" inputmode="search" placeholder="搜索地点、联系人、任务、动态" aria-label="全局搜索" />
      <button id="app-search-clear" class="app-search-clear" type="button" aria-label="清空全局搜索" hidden>清空</button>
    </div>
    <div id="app-ux-status" class="app-ux-status" aria-live="polite">
      <span id="app-ux-status-pill" class="app-ux-pill" data-state="ready">UX ready · 世界 tab active</span>
      <span id="app-ux-live-status" class="sr-only">Client UX ready</span>
    </div>
    <div id="app-search-empty-state" class="app-search-empty" role="status" aria-live="polite">无匹配结果 · 换个关键词或切换底部 Tab。</div>
  </header>
  <main class="app-mobile-shell">
    <section id="app-first-playable-onboarding" class="module" aria-label="First playable onboarding rail">
      <span class="badge">First playable onboarding</span>
      <h2>{}</h2>
      <p class="subtitle">{} 目标：<code>{}</code></p>
      <div id="app-first-playable-checks" class="map-stream-hud">{}</div>
      <section id="app-first-playable-steps" class="grid">{}</section>
    </section>
    <section id="app-tab-messages" class="app-tab-panel" data-app-panel="messages" role="tabpanel" aria-labelledby="app-tab-button-messages" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2>消息</h2>
        <p class="subtitle">Telegram / 微信风格的消息首页，承接联系人、Agent 协作、系统通知与任务线程。</p>
      </div>
      <section id="app-message-cards" class="grid">{}</section>
    </section>
    <section id="app-tab-map" class="app-tab-panel is-active" data-app-panel="map" role="tabpanel" aria-labelledby="app-tab-button-map" aria-hidden="false">
      <div class="app-tab-header">
        <h2>世界</h2>
        <p class="subtitle">World-first 主舞台：focus、route、live event、world action 都从这里展开。</p>
      </div>
      <section class="map-shell" aria-label="Real-world map engine shell">
      <div class="map-panel">
        <span class="badge">Real-world map engine</span>
        <h2>{} + {}</h2>
        <p>这里不再只是文字地图：客户端以 OpenStreetMap 的全球真实世界瓦片做全量镜像底图，再把 Trillionnium World 的英雄坛说/Gather 节点、路线、LOD 分片和当前位置叠加成轻量 marker。Matrix 文字地图保留为低配和聊天 fallback，用于支撑更多玩家同时在线。</p>
        <p><strong>Mirror</strong>: <code>{}</code> · <strong>Active Region</strong>: <code>{}</code> · <strong>Shards</strong>: {} · <strong>LOD Layers</strong>: {}</p>
        <p><strong>Viewport API</strong>: <code>{}</code></p>
        <p><strong>Web Viewport</strong>: <code>{}</code></p>
        <p><code>{}</code></p>
        <p><strong>Map-first Hub</strong>: the World Map module is the primary super-app entry, with nearby POIs and region shards surfaced before other modules.</p>
        <p id="app-map-density-summary" class="subtitle">{}</p>
        <p id="app-map-camera-summary" class="subtitle">Camera booting…</p>
        <div id="app-map-stream-hud" class="map-stream-hud">
          <span class="hud-chip"><strong>{}</strong> region shards</span>
          <span class="hud-chip"><strong>{}</strong> visible nodes</span>
          <span class="hud-chip"><strong>{}</strong> prefetch tiles</span>
          <span class="hud-chip"><strong>{}</strong> live events · {}</span>
        </div>
        <div id="app-map-overlay-controls" class="overlay-toggle-bar">
{shared_map_overlay_controls_html}
        </div>
        <div id="app-map-camera-actions" class="overlay-toggle-bar">
{shared_map_camera_actions_html}
        </div>
        <p id="app-map-overlay-status" class="subtitle">Active overlays: density, regions, tiles, prefetch, live events.</p>
        <div class="module" style="margin-top:14px; padding:16px 18px;">
          <strong>Map focus action rail</strong>
          <span id="app-map-focus-summary">Waiting for viewport focus…</span>
          <p id="app-map-focus-detail">Pick a region, tile, hotspot, or live event to turn the map into an action surface.</p>
          <div id="app-map-action-rail" class="focus-stack"></div>
        </div>
        <div class="module" style="margin-top:14px; padding:16px 18px;">
          <strong>World route cockpit</strong>
          <span id="app-map-route-status">Focused route cockpit: waiting for a map focus…</span>
          <p id="app-map-route-next-step-status">Recommended world handoff: pick a map focus first.</p>
          <p id="app-map-route-event-brief-status">Focused event brief: waiting for a live event focus.</p>
          <p id="app-map-route-link-status">Linked task route: none yet.</p>
          <div id="app-map-route-filter-actions" class="focus-stack">
            {shared_route_filter_buttons_html}
          </div>
          <div id="app-map-route-actions" class="focus-stack"></div>
        </div>
        <p id="app-map-overlay-legend" class="subtitle">Overlay legend: region anchors · active tile frames · prefetch warm ring · live event pulses.</p>
      </div>
      <div id="real-world-map" data-engine="{}" data-provider="{}" aria-label="Leaflet OpenStreetMap real-world map engine"></div>
    </section>
    <section>
      <h2>Tile Shards</h2>
      <section id="app-tile-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Region Shards</h2>
      <section id="app-region-shards-live" class="grid">{}</section>
    </section>
    <section>
      <h2>POI Hotspots</h2>
      <section id="app-poi-hotspots-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Prefetch Queue</h2>
      <section id="app-prefetch-queue-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Live Event Stream</h2>
      <section id="app-live-events-live" class="grid">{}</section>
    </section>
    </section>
    <section id="app-tab-feed" class="app-tab-panel" data-app-panel="feed" role="tabpanel" aria-labelledby="app-tab-button-feed" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2>动态</h2>
        <p class="subtitle">小红书式流式浏览，但内容核心是 live events、contracts、completions、commerce 与 social updates。</p>
        <p><strong>Feed API</strong>: <code>{}</code></p>
        <p id="app-feed-api-status" class="subtitle">Feed boot from <code>{}</code> · active region <code>{}</code> · {} items ready.</p>
      </div>
      <div id="app-feed-filter-actions" class="focus-stack">{}</div>
      <div id="app-feed-summary" class="map-stream-hud">{}</div>
      <section>
        <h2>Unified Feed Timeline</h2>
        <section id="app-feed-items-live" class="grid">{}</section>
      </section>
    <section>
      <h2>World Route Preview</h2>
      <section id="app-route-preview-live" class="grid">{}</section>
    </section>
    <section>
      <h2>Task-linked Route Graph</h2>
      <section id="app-route-task-graph-live" class="grid">{}</section>
    </section>
    </section>
    <section id="app-tab-me" class="app-tab-panel" data-app-panel="me" role="tabpanel" aria-labelledby="app-tab-button-me" aria-hidden="true" hidden>
      <div class="app-tab-header">
        <h2>我</h2>
        <p class="subtitle">钱包支付、成长、资产、设置与系统能力统一归到个人中心。</p>
      </div>
      <section class="app-me-grid">{}</section>
      <section>
        <h2>System Modules</h2>
        <section class="grid">{}</section>
      </section>
    </section>
  </main>
  <nav class="app-bottom-tabs" aria-label="Client mobile tabs" role="tablist">
    <button id="app-tab-button-messages" type="button" class="app-bottom-tab" data-app-tab="messages" role="tab" aria-controls="app-tab-messages" aria-selected="false" tabindex="-1">消息</button>
    <button id="app-tab-button-map" type="button" class="app-bottom-tab is-active" data-app-tab="map" role="tab" aria-controls="app-tab-map" aria-selected="true" tabindex="0">世界</button>
    <button id="app-tab-button-feed" type="button" class="app-bottom-tab" data-app-tab="feed" role="tab" aria-controls="app-tab-feed" aria-selected="false" tabindex="-1">动态</button>
    <button id="app-tab-button-me" type="button" class="app-bottom-tab" data-app-tab="me" role="tab" aria-controls="app-tab-me" aria-selected="false" tabindex="-1">我</button>
  </nav>
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
      const appTabLabels = {{ messages: '消息', map: '世界', feed: '动态', me: '我' }};
      const appTabPlaceholders = {{
        messages: '搜索联系人、群组、Agent、任务对话',
        map: '搜索世界地点、公司、任务、事件',
        feed: '搜索动态、话题、事件、成交案例',
        me: '搜索订单、账单、资产、设置',
      }};
      let activeAppTab = 'map';
      {shared_map_runtime_bootstrap_js}

      const routePreviewItems = ((((app.map_hub || {{}}).route_preview) || {{}}).items) || [];
      const routeTaskGraphItems = ((((app.map_hub || {{}}).route_task_graph) || {{}}).tasks) || [];
      const feedApiPath = ((((app.feed || {{}}).api_path) || '')) || '/v1/client/feed/@alice:local.dev';
      const feedWebSessionPath = ((((app.feed || {{}}).web_session_path) || '')) || '/app/web/feed';
      const feedFilterLabels = {};
      let lastViewport = null;
      let lastFeed = app.feed || {{}};
      let lastSelection = null;
      let lastRouteActions = [];
      let routeFilterMode = 'selection';
      let feedFilterMode = 'all';
      let feedLoadedViaApi = false;
      let feedRequestInFlight = null;
      const announceUxStatus = (message, state = 'ready') => {{
        const text = String(message || '').trim() || 'Client UX ready';
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
            ? ('无匹配结果 · “' + String(query || '').trim() + '” 没有命中当前 ' + (appTabLabels[activeAppTab] || activeAppTab) + ' 页，换个关键词或切换底部 Tab。')
            : '无匹配结果 · 换个关键词或切换底部 Tab。';
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
        if (appSearchInput) appSearchInput.placeholder = appTabPlaceholders[activeAppTab] || '搜索地点、联系人、任务、动态';
        announceUxStatus('UX ready · ' + (appTabLabels[activeAppTab] || activeAppTab) + ' tab active', 'ready');
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

      const inferAppRouteNextStep = (selection, context) => inferConfiguredRouteNextStep(selection, context, {{
        statusPrefix: 'Recommended world handoff',
        rejectionBody: (selectionTitle, workOrderId) => selectionTitle + ': reopen work order ' + workOrderId + ' with revision requirements, renewed reserve, and redelivery plan.',
        rejectionStatus: (workOrderId) => 'reopen work order ' + workOrderId + '.',
        reopenBody: (selectionTitle, workOrderId) => selectionTitle + ': redeliver work order ' + workOrderId + ' with revised deliverable, proof, and acceptance checklist.',
        reopenStatus: (workOrderId) => 'redeliver work order ' + workOrderId + '.',
        deliveryBody: (selectionTitle, workOrderId) => selectionTitle + ': review delivery for work order ' + workOrderId + ' and decide accept vs reject with concrete proof gaps.',
        deliveryStatus: (workOrderId) => 'review the latest delivery for work order ' + workOrderId + '.',
        openWorkBody: (selectionTitle, workOrderId) => selectionTitle + ': prepare delivery for work order ' + workOrderId + ' with deliverable, evidence, and next action.',
        openWorkStatus: (workOrderId) => 'deliver the active work order ' + workOrderId + '.',
        contractBody: (selectionTitle, contractId) => selectionTitle + ': complete linked contract ' + contractId + ' with evidence, acceptance standard, and next step.',
        contractStatus: (contractId) => 'complete contract ' + contractId + '.',
        listingBody: (selectionTitle, listingId) => selectionTitle + ': hire listing ' + listingId + ' and define deliverable, acceptance, and risk controls.',
        listingStatus: (listingId) => 'route listing ' + listingId + ' into world commerce.',
        defaultBody: (selectionTitle) => selectionTitle + ': draft the next world action for this map focus with current evidence, risks, and next operational step.',
        defaultStatus: () => 'draft a world action from this focused route.',
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
        routePreviewTarget.innerHTML = visible.map((item) => `<article class="module"><strong>${{escapeHtml(item.title || 'Route item')}}</strong><span>${{escapeHtml(item.detail || item.route_bucket || 'route')}}</span><p>${{escapeHtml(item.summary || item.route_status || 'waiting')}}</p><div class="focus-stack"><code>${{escapeHtml(item.task_id || item.location_id || item.route_bucket || 'route')}}</code></div></article>`).join('');
      }};
      const renderAppTaskGraph = (tasks) => {{
        if (!routeTaskGraphTarget) return;
        const visible = tasks.length ? tasks.slice(0, 6) : routeTaskGraphItems.slice(0, 6);
        routeTaskGraphTarget.innerHTML = visible.map((task) => {{
          const actionButtons = routeTaskGraphActionButtonsHtml(task, 'trillionnium-app-route-flow-action');
          return `<article class="module app-route-task-graph-item"><strong>${{escapeHtml(task.task_id || 'task')}}</strong><span>${{escapeHtml(task.latest_bucket || 'event')}} · ${{escapeHtml(task.latest_status || 'pending')}} · opportunity ${{escapeHtml(task.next_opportunity_kind || 'contract_capture')}}</span><p>${{escapeHtml(task.event_count ?? 0)}} events · ${{escapeHtml(task.contract_count ?? 0)}} contracts · ${{escapeHtml(task.completion_count ?? 0)}} completions</p><p>${{escapeHtml(task.outcome_summary || 'Outcome summary pending.')}}</p><p><strong>Opportunity lane</strong> · ${{escapeHtml(task.next_opportunity_hint || 'Opportunity hint pending.')}}</p><p>${{escapeHtml(task.next_opportunity_playbook || 'Opportunity playbook pending.')}}</p><div class="focus-stack"><code>${{escapeHtml(task.next_opportunity_command || '/world action 继续推进下一步机会。')}}</code></div><div class="focus-stack">${{actionButtons}}</div></article>`;
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
        pushUniqueRouteAction(actions, buildDraftWorldAction(locationId, activeTaskId, buildRouteDraftBody(selection, {{}}, {{ selectionTitleFallback: 'Focused route', omitContextDetails: true, leadIn: ': continue ', emptyDetail: 'this map-driven route', suffix: ' with evidence, risk review, and next operational move.' }})));
        if (activeTaskId) pushUniqueRouteAction(actions, buildTaskFollowUpAction(selection, activeTaskId, locationId, 'across linked event/contract state, evidence, blockers, and next action'));
        if (contractId) pushUniqueRouteAction(actions, buildLinkedContractRouteAction(selection, contractId, activeTaskId, locationId));
        if (linkedEventItem) pushUniqueRouteAction(actions, buildRouteEventTimelineAction('Open linked event', {{
          locationId,
          eventId: String(linkedEventItem.event_id || '').trim(),
          eventKind: String(linkedEventItem.title || 'world_event'),
          eventBody: String(linkedEventItem.summary || '').trim(),
          eventResult: String(linkedEventItem.route_status || '').trim(),
          eventTaskId: activeTaskId,
          body: appendSelectionEventSignal(((routeSelection && routeSelection.title) || 'Focused route') + ': review linked event ' + String(linkedEventItem.title || 'event') + ' before the next world action.', selection),
        }}));
        lastRouteActions = actions;
        if (routeActionRail) {{
          routeActionRail.innerHTML = actions.map((action, index) => indexedRouteActionButtonHtml(action, index)).join(' ');
        }}
        if (routeStatus) routeStatus.textContent = routeFilterMode === 'all'
          ? ('Focused route cockpit: showing full route overview' + (selection ? (' while focus stays on ' + (selection.title || 'focus')) : '') + ' · ' + filteredItems.length + ' route items' + (activeTaskId ? (' · task ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.')
          : (routeSelection ? ('Focused route cockpit: ' + (routeSelection.title || 'focus') + ' · ' + (locationId || 'no location') + ' · ' + filteredItems.length + ' linked route items' + (activeTaskId ? (' · task ' + activeTaskId) : '') + routeOpportunitySegment(opportunityTask) + '.') : 'Focused route cockpit: no active map focus, showing latest route items.');
        if (routeNextStepStatus) routeNextStepStatus.textContent = (nextStep && nextStep.status) || 'Recommended world handoff: draft a world action from this focused route.';
        if (routeEventBriefStatus) routeEventBriefStatus.textContent = routeEventBriefText(eventSignalText, routeFilterMode === 'all');
        if (routeLinkStatus) routeLinkStatus.textContent = routeLinkStatusText({{ taskId: activeTaskId, linkedEventCount, linkedContractCount, opportunityTask, emptyText: 'Linked task route: no task-linked event/contract in the current focus.' }});
      }};
      const renderFocusPanel = () => {{
        renderMapFocusPanel({{
          focusSummary,
          focusDetail,
          actionRail,
          focus: lastSelection || buildDefaultFocus(),
          emptyDetail: 'Pick a region, tile, hotspot, or live event to turn the map into an action surface.',
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
          focusDetail.textContent = 'World handoff ready: ' + state.actionLabel + ' · open ' + state.panelId + ' in /world.';
        }}
        if (cameraSummary) {{ cameraSummary.textContent = 'Selected map action: ' + state.actionLabel + ' · ' + state.command; }}
        announceUxStatus('Map handoff ready · ' + state.actionLabel, 'ready');
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
        if (handleRouteActionButton(appRouteFlowButton, openWorldRouteAction, 'Route action')) return;
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
        const chips = [
          `<span class="hud-chip"><strong>${{escapeHtml((visibleItems || []).length)}}</strong> visible items</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(payload.item_count ?? 0)}}</strong> total feed items</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(contracts)}}</strong> contracts · <strong>${{escapeHtml(completions)}}</strong> completions</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(purchaseCount)}}</strong> purchases · <strong>${{escapeHtml(workOrderCount)}}</strong> work orders</span>`,
          `<span class="hud-chip"><strong>${{escapeHtml(nearbyAgents)}}</strong> nearby agents · ${{escapeHtml(payload.active_region_id || 'global')}}</span>`
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
          feedItemTarget.innerHTML = '<article class="module app-feed-item"><strong>动态暂时安静</strong><span>feed waiting</span><p>切到世界选择一个 live event，或者稍后再拉一次 Feed API。</p><div class="focus-stack"><code>/v1/client/feed/@alice:local.dev</code></div></article>';
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
          return `<article class="module app-feed-item" data-feed-kind="${{escapeHtml(kind)}}" data-feed-group="${{escapeHtml(group)}}"><strong>${{escapeHtml(item.title || 'Feed item')}}</strong><span>${{escapeHtml(detail)}}</span><p>${{escapeHtml(summary)}}</p><div class="focus-stack"><code>${{escapeHtml(source)}}</code>${{actionButtons ? ' ' + actionButtons : ''}}</div></article>`;
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
          const sourceLabel = feedLoadedViaApi ? 'Feed API synced' : 'Embedded feed snapshot';
          const visibleFeedPath = payload.web_session_path || feedWebSessionPath || payload.api_path || feedApiPath;
          feedApiStatus.textContent = sourceLabel + ' · ' + visibleFeedPath + ' · active region ' + (payload.active_region_id || 'global') + ' · ' + String(payload.item_count ?? sourceItems.length ?? 0) + ' items.';
        }}
      }};
      const loadFeedSurface = async (reason = 'manual') => {{
        const hydrationPath = feedWebSessionPath || feedApiPath;
        if (!hydrationPath || feedRequestInFlight) return feedRequestInFlight;
        feedRequestInFlight = (async () => {{
          try {{
            announceUxStatus('Feed loading · ' + reason, 'loading');
            const response = await fetch(hydrationPath, {{ credentials: 'same-origin' }});
            if (!response.ok) throw new Error('feed_http_' + response.status);
            const payload = await response.json();
            if (payload && typeof payload === 'object') {{
              lastFeed = payload;
              feedLoadedViaApi = true;
              renderFeedSurface(lastFeed, lastSelection);
              applyAppSearchFilter();
              if (feedApiStatus) feedApiStatus.textContent = 'Feed API synced · ' + hydrationPath + ' · reason ' + reason + ' · ' + String(payload.item_count ?? 0) + ' items.';
              announceUxStatus('Feed API synced · ' + String(payload.item_count ?? 0) + ' items', 'ready');
            }}
          }} catch (_error) {{
            feedLoadedViaApi = false;
            renderFeedSurface(lastFeed, lastSelection);
            if (feedApiStatus) feedApiStatus.textContent = 'Feed API fallback · ' + hydrationPath + ' · using embedded snapshot.';
            announceUxStatus('Feed API fallback · using embedded snapshot', navigator.onLine === false ? 'offline' : 'fallback');
          }} finally {{
            feedRequestInFlight = null;
          }}
        }})();
        return feedRequestInFlight;
      }};
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
            announceUxStatus('Search cleared', 'ready');
          }}
        }});
      }}
      if (appSearchClearButton && appSearchInput) {{
        appSearchClearButton.addEventListener('click', () => {{
          appSearchInput.value = '';
          applyAppSearchFilter();
          appSearchInput.focus();
          announceUxStatus('Search cleared', 'ready');
        }});
      }}
      window.addEventListener('offline', () => announceUxStatus('Offline mode · embedded snapshot ready', 'offline'));
      window.addEventListener('online', () => {{
        announceUxStatus('Back online · refreshing feed', 'loading');
        if (activeAppTab === 'feed') loadFeedSurface('online');
      }});
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
        escape_html_text(current_node),
        escape_html_text(onboarding_label),
        escape_html_text(onboarding_goal),
        escape_html_text(onboarding_completion_target),
        onboarding_acceptance_chips,
        onboarding_step_cards,
        message_cards,
        escape_html_text(map_engine_name),
        escape_html_text(tile_provider),
        escape_html_text(mirror_scope),
        escape_html_text(active_region_id),
        region_shard_count,
        lod_layer_count,
        escape_html_text(viewport_path_template),
        escape_html_text(web_session_viewport_path_template),
        escape_html_text(map_engine_id),
        escape_html_text(map_density_summary),
        map_stream_region_count,
        map_visible_marker_count,
        map_prefetch_count,
        map_live_event_count,
        escape_html_text(map_player_density_mode),
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
        me_primary_cards,
        module_cards,
        app_data_json,
        feed_filter_labels_js,
    ))
}
