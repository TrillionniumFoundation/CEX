use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) enum RealWorldMapShellCardStyle {
    AppModule,
    WorldMini,
}

pub(super) fn real_world_map_runtime_bootstrap_js() -> &'static str {
    r#"      const createRealWorldMapAdapter = () => ({
        adapterId: (((engine.renderer_adapter || {}).adapter_id) || 'leaflet_renderer_adapter_v1'),
        createMap(targetElement, engineConfig, centerPoint) {
          return L.map(targetElement, { zoomControl: true }).setView([centerPoint.lat, centerPoint.lng], engineConfig.zoom || 15);
        },
        setBaseLayer(map, engineConfig) {
          return L.tileLayer(engineConfig.tile_url_template || 'https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
            maxZoom: engineConfig.max_zoom || 19,
            attribution: engineConfig.attribution || '© OpenStreetMap contributors'
          }).addTo(map);
        },
        createOverlayLayer(map) {
          return L.layerGroup().addTo(map);
        },
        clearOverlay(layer) {
          if (layer && layer.clearLayers) layer.clearLayers();
        },
        latLngBounds(points) {
          return L.latLngBounds(points);
        },
        renderRouteLine(map, fromPoint, toPoint, options = {}) {
          return L.polyline([[fromPoint.lat, fromPoint.lng], [toPoint.lat, toPoint.lng]], options).addTo(map);
        },
        renderPoiMarker(map, marker, popupHtml) {
          return L.marker([marker.lat, marker.lng], { title: marker.name || marker.node_id }).addTo(map).bindPopup(popupHtml);
        },
        renderDensityCircle(layer, centerPoint, radiusMeters, options = {}) {
          return L.circle([centerPoint.lat, centerPoint.lng], { radius: radiusMeters, ...options }).addTo(layer);
        },
        renderRegionAnchor(layer, centerPoint, options = {}) {
          return L.circleMarker([centerPoint.lat, centerPoint.lng], options).addTo(layer);
        },
        renderTileFrame(layer, bounds, options = {}) {
          return L.rectangle(bounds, options).addTo(layer);
        },
        renderEventPulse(layer, marker, options = {}) {
          return L.circleMarker([marker.lat, marker.lng], options).addTo(layer);
        },
        getCenter(map) {
          return map.getCenter();
        },
        getZoom(map) {
          return map.getZoom();
        },
        onViewportChange(map, callback) {
          map.on('moveend zoomend', callback);
        },
        setOverlayVisibility(map, layer, enabled) {
          if (!layer) return;
          const attached = map.hasLayer(layer);
          if (enabled && !attached) map.addLayer(layer);
          if (!enabled && attached) map.removeLayer(layer);
        },
        fitBounds(map, bounds) {
          if (bounds && bounds.isValid && bounds.isValid()) map.fitBounds(bounds, { padding: [22, 22] });
        },
        focus(map, focusPayload) {
          if (!focusPayload) return;
          const lat = Number(focusPayload.lat);
          const lng = Number(focusPayload.lng);
          const zoom = Number(focusPayload.zoom || map.getZoom());
          if (Number.isFinite(lat) && Number.isFinite(lng)) map.setView([lat, lng], Number.isFinite(zoom) ? zoom : map.getZoom());
        },
        invalidateSize(map) {
          map.invalidateSize();
        },
      });
      const mapAdapter = createRealWorldMapAdapter();
      const mapRuntime = mapAdapter.createMap(target, engine, center);
      mapAdapter.setBaseLayer(mapRuntime, engine);
      const escapeHtml = (value) => String(value ?? '').replace(/[&<>"']/g, (ch) => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      }[ch]));
      const markerById = new Map((engine.markers || []).map((marker) => [String(marker.node_id || ''), marker]));
      const markerByLocationId = new Map();
      (engine.markers || []).forEach((marker) => {
        const locationId = String(marker.location_id || '').trim();
        if (locationId && !markerByLocationId.has(locationId)) markerByLocationId.set(locationId, marker);
      });
      const markerLayerById = new Map();
      const overlayLayers = {
        density: mapAdapter.createOverlayLayer(mapRuntime),
        regions: mapAdapter.createOverlayLayer(mapRuntime),
        tiles: mapAdapter.createOverlayLayer(mapRuntime),
        prefetch: mapAdapter.createOverlayLayer(mapRuntime),
        events: mapAdapter.createOverlayLayer(mapRuntime),
      };
      const overlayState = { density: true, regions: true, tiles: true, prefetch: true, events: true };
      const overlayLabels = { density: 'density', regions: 'regions', tiles: 'tiles', prefetch: 'prefetch', events: 'live events' };"#
}

pub(super) fn real_world_map_runtime_primitives_js() -> &'static str {
    r#"      const tileToLatLng = (x, y, z) => {
        const scale = Math.pow(2, z);
        const lng = (x / scale) * 360 - 180;
        const latRadians = Math.atan(Math.sinh(Math.PI * (1 - (2 * y) / scale)));
        return { lat: (latRadians * 180) / Math.PI, lng };
      };
      const tileBoundsFromParts = (z, x, y) => {
        if (![z, x, y].every(Number.isFinite)) return null;
        const northWest = tileToLatLng(x, y, z);
        const southEast = tileToLatLng(x + 1, y + 1, z);
        return mapAdapter.latLngBounds([[northWest.lat, northWest.lng], [southEast.lat, southEast.lng]]);
      };
      const renderStreamHud = (viewport, focus = lastSelection) => {
        if (!streamHud) return;
        const density = (viewport.player_density || {}).mode || 'dense';
        const lens = buildStreamLens(viewport, focus);
        const chips = [
          `<span class="hud-chip"><strong>${escapeHtml(viewport.stream_region_count ?? 0)}</strong> region shards</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.marker_count ?? 0)}</strong> visible nodes</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.prefetch_count ?? 0)}</strong> prefetch tiles</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.live_event_count ?? 0)}</strong> live events · ${escapeHtml(density)}</span>`
        ];
        if (lens) {
          chips.push(`<span class="hud-chip"><strong>${escapeHtml(lens.count ?? 0)}</strong> stream lens · ${escapeHtml(lens.label || 'focus')}</span>`);
        }
        streamHud.innerHTML = chips.join('');
      };
      const setOverlayLayerVisibility = (name, enabled) => {
        const layer = overlayLayers[name];
        if (!layer) return;
        mapAdapter.setOverlayVisibility(mapRuntime, layer, enabled);
      };
      const renderOverlayStatus = () => {
        if (!overlayStatus) return;
        const active = Object.entries(overlayState)
          .filter(([, enabled]) => enabled)
          .map(([name]) => overlayLabels[name] || name);
        const activeRegion = (((lastViewport || {}).active_region || {}).name) || 'current region';
        overlayStatus.textContent = 'Active overlays: ' + (active.length ? active.join(', ') : 'none') + ' · quick focus: ' + activeRegion + ', nearest hotspot, hottest event.';
      };
      const refreshOverlayControls = () => {
        if (!overlayControls) return;
        overlayControls.querySelectorAll('.trillionnium-overlay-toggle').forEach((button) => {
          const enabled = !!overlayState[button.dataset.overlayTarget];
          button.setAttribute('aria-pressed', enabled ? 'true' : 'false');
          button.classList.toggle('is-off', !enabled);
        });
      };"#
}

pub(super) fn real_world_map_focus_core_js() -> &'static str {
    r#"      const buildDefaultFocus = () => {
        const activeRegion = (lastViewport || {}).active_region;
        if (activeRegion && activeRegion.center) {
          return {
            kind: 'region',
            lat: activeRegion.center.lat,
            lng: activeRegion.center.lng,
            zoom: activeRegion.zoom_max || 12,
          };
        }
        const hotspot = (((lastViewport || {}).poi_hotspots) || [])[0];
        if (hotspot) {
          return { kind: 'node', nodeId: hotspot.node_id };
        }
        return null;
      };
      const findLiveEventByFocus = (focus) => {
        if (!focus) return null;
        const eventId = String(focus.eventId || '').trim();
        const taskId = String(focus.taskId || '').trim();
        const locationId = String(focus.locationId || '').trim();
        const nodeId = String(focus.nodeId || '').trim();
        const stream = (((lastViewport || {}).live_event_stream) || []);
        return stream.find((eventItem) => eventId && String(eventItem.event_id || '').trim() === eventId)
          || stream.find((eventItem) => taskId && String(eventItem.cex_task_id || '').trim() === taskId && (!locationId || String(eventItem.location_id || '').trim() === locationId))
          || stream.find((eventItem) => locationId && String(eventItem.location_id || '').trim() === locationId && (!nodeId || String(eventItem.node_id || '').trim() === nodeId))
          || stream.find((eventItem) => nodeId && String(eventItem.node_id || '').trim() === nodeId)
          || null;
      };
      const buildEventFocus = (eventItem) => {
        const item = eventItem || {};
        const nodeId = String(item.node_id || '').trim();
        const marker = markerById.get(nodeId) || {};
        return {
          kind: 'event',
          nodeId,
          eventId: String(item.event_id || '').trim(),
          taskId: String(item.cex_task_id || '').trim(),
          locationId: String(item.location_id || marker.location_id || '').trim(),
          eventKind: String(item.event_kind || 'world_event'),
          nodeName: String(item.node_name || marker.name || item.location_id || 'POI'),
          eventBody: String(item.body || ''),
          eventResult: String(item.result || ''),
          suppressAction: true,
        };
      };
      const filterLiveEventStream = (stream, focus = lastSelection) => {
        const source = Array.isArray(stream) ? stream : [];
        if (!source.length) return source;
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return source;
        const selectionTaskId = String(selection.taskId || '').trim();
        const selectionLocationId = String(selection.locationId || '').trim();
        const selectionEventId = String(selection.eventId || '').trim();
        let filtered = source;
        if (selectionTaskId) {
          filtered = source.filter((eventItem) => String(eventItem.cex_task_id || '').trim() === selectionTaskId);
        }
        if (!filtered.length && selectionLocationId) {
          filtered = source.filter((eventItem) => String(eventItem.location_id || '').trim() === selectionLocationId);
        }
        if (!filtered.length && selectionEventId) {
          filtered = source.filter((eventItem) => String(eventItem.event_id || '').trim() === selectionEventId);
        }
        return filtered.length ? filtered : source;
      };
      const buildStreamLens = (viewport, focus = lastSelection) => {
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return null;
        const filtered = filterLiveEventStream((viewport || {}).live_event_stream || [], selection);
        if (!filtered.length) return null;
        const label = selection.kind === 'event'
          ? (selection.taskId ? ('task ' + selection.taskId) : (selection.title || 'selected event'))
          : (selection.title || selection.locationId || selection.nodeId || selection.kind || 'focus');
        return { count: filtered.length, label };
      };
      const findRegionByFocus = (focus) => {
        const focusLat = Number((focus || {}).lat);
        const focusLng = Number((focus || {}).lng);
        const regions = [
          ...((((lastViewport || {}).stream_region_shards) || [])),
          ((lastViewport || {}).active_region),
          ...((engine.region_shards) || []),
        ].filter(Boolean);
        return regions.find((region) => {
          const centerPoint = region.center || {};
          return Math.abs(Number(centerPoint.lat || 0) - focusLat) < 0.00001 && Math.abs(Number(centerPoint.lng || 0) - focusLng) < 0.00001;
        }) || null;
      };
"#
}

pub(super) fn real_world_map_selection_location_ids_js() -> &'static str {
    r#"      const resolveSelectionLocationIds = (focus) => {
        const locationIds = new Set();
        if (!focus) return locationIds;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          return locationIds;
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const locationId = String(eventItem.location_id || focus.locationId || '').trim();
          if (locationId) locationIds.add(locationId);
          const marker = markerById.get(String(eventItem.node_id || focus.nodeId || '').trim());
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          return locationIds;
        }
        if (focus.kind === 'tile') {
          const tile = buildSelectionFromFocus(focus) || {};
          (tile.nodeIds || []).forEach((nodeId) => {
            const marker = markerById.get(String(nodeId || ''));
            if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          });
          return locationIds;
        }
        ((((lastViewport || {}).visible_markers) || [])).forEach((marker) => {
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
        });
        return locationIds;
      };"#
}

pub(super) fn real_world_map_selection_builder_js() -> &'static str {
    r#"      const buildSelectionFromFocus = (focus) => {
        if (!focus) return null;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (!marker) return null;
          return {
            kind: 'node',
            title: marker.name || marker.node_id || 'POI',
            summary: (marker.node_kind || 'poi') + ' · ' + (((marker.interaction_tags || []).slice(0, 3)).join(' / ') || 'world interaction'),
            detail: marker.description || 'Move, inspect, trade, craft, or open world actions from this hotspot.',
            nodeId: marker.node_id,
            locationId: marker.location_id || '',
            actions: marker.primary_actions || [],
          };
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const nodeId = String(eventItem.node_id || focus.nodeId || '').trim();
          const marker = markerById.get(nodeId) || {};
          const taskId = String(eventItem.cex_task_id || focus.taskId || '').trim();
          const locationId = String(eventItem.location_id || focus.locationId || marker.location_id || '').trim();
          const impact = Number(eventItem.impact_score ?? focus.impact ?? 0);
          const eventKind = String(eventItem.event_kind || focus.eventKind || 'world_event');
          const nodeName = String(eventItem.node_name || focus.nodeName || marker.name || locationId || 'POI');
          return {
            kind: 'event',
            title: eventKind + ' · ' + nodeName,
            summary: (taskId ? ('task ' + taskId) : 'unlinked live event') + ' · impact ' + (Number.isFinite(impact) ? impact : 0),
            detail: String(eventItem.result || focus.eventResult || eventItem.body || focus.eventBody || 'Track this live event into the route cockpit and next world action.'),
            nodeId,
            locationId,
            taskId,
            eventId: String(eventItem.event_id || focus.eventId || '').trim(),
            eventBody: String(eventItem.body || focus.eventBody || '').trim(),
            eventResult: String(eventItem.result || focus.eventResult || '').trim(),
            actions: marker.primary_actions || [],
          };
        }
        if (focus.kind === 'region') {
          const region = findRegionByFocus(focus) || {};
          return {
            kind: 'region',
            title: region.name || 'Region focus',
            summary: (region.status || 'planned') + ' · ' + (region.coverage_kind || 'shard'),
            detail: 'Zoom ' + (region.zoom_min || focus.zoom || 12) + '-' + (region.zoom_max || focus.zoom || 12) + ' · density ' + (region.player_density_mode || (((lastViewport || {}).player_density || {}).mode) || 'mixed'),
            lat: focus.lat,
            lng: focus.lng,
            zoom: region.zoom_max || focus.zoom || 12,
          };
        }
        if (focus.kind === 'tile') {
          const tiles = [
            ...((((lastViewport || {}).visible_tile_shards) || [])),
            ...((((lastViewport || {}).prefetch_queue) || [])),
          ];
          const tile = tiles.find((item) => String(item.z || '') === String(focus.z || '') && String(item.x || '') === String(focus.x || '') && String(item.y || '') === String(focus.y || '')) || {};
          return {
            kind: 'tile',
            title: tile.tile_id || 'Tile shard',
            summary: (tile.tile_status || 'tile') + ' · ' + String(tile.marker_count ?? 0) + ' nodes',
            detail: (tile.lod_mode || 'street_nodes') + ' · tile ' + [focus.z, focus.x, focus.y].filter((value) => value !== undefined && value !== null && value !== '').join('/'),
            nodeIds: tile.node_ids || [],
            z: focus.z,
            x: focus.x,
            y: focus.y,
          };
        }
        return null;
      };
"#
}

pub(super) fn real_world_map_selection_signal_js() -> &'static str {
    r#"      const selectionEventSignalText = (selection) => {
        if (!selection || selection.kind !== 'event') return '';
        const result = String(selection.eventResult || '').trim();
        const body = String(selection.eventBody || '').trim();
        if (result && body) return 'Latest event signal: ' + result + ' · ' + body;
        if (result) return 'Latest event signal: ' + result;
        if (body) return 'Latest event signal: ' + body;
        return '';
      };
      const appendSelectionEventSignal = (body, selection) => {
        const eventSignal = selectionEventSignalText(selection);
        return eventSignal ? (body + ' ' + eventSignal) : body;
      };"#
}

pub(super) fn real_world_map_focus_panel_js() -> &'static str {
    r#"      const selectionActionButtonHtml = (attrs, label, extraAttrs = '') => {
        const attrHtml = Object.entries(attrs || {})
          .filter(([, value]) => value !== undefined && value !== null)
          .map(([name, value]) => ` data-${name}="${escapeHtml(value)}"`)
          .join('');
        return `<button type="button" class="focus-chip trillionnium-selection-action"${attrHtml}${extraAttrs}>${escapeHtml(label || 'Action')}</button>`;
      };
      const selectionCameraActionButtonHtml = (actionId, label) => selectionActionButtonHtml({ 'selection-kind': 'camera', 'camera-action': actionId }, label);
      const buildMapFocusActionButtonsHtml = (selection, options) => {
        const nodeButtonExtraAttrs = String(((options || {}).nodeButtonExtraAttrs) || '');
        const buttons = [];
        if (selection.kind === 'node' || selection.kind === 'event') {
          buttons.push(...(selection.actions || []).map((action) => selectionActionButtonHtml({ 'selection-kind': 'node', 'node-id': selection.nodeId || '', 'action-id': action.action_id || 'move_here' }, action.label || action.command || 'Action', nodeButtonExtraAttrs)));
        } else if (selection.kind === 'region') {
          buttons.push(selectionActionButtonHtml({ 'selection-kind': 'region', lat: selection.lat ?? '', lng: selection.lng ?? '', zoom: selection.zoom ?? 12 }, 'Center region'));
          buttons.push(selectionCameraActionButtonHtml('nearest_poi', 'Nearest hotspot'));
          buttons.push(selectionCameraActionButtonHtml('hottest_event', 'Hottest event'));
        } else if (selection.kind === 'tile') {
          buttons.push(selectionActionButtonHtml({ 'selection-kind': 'tile', 'tile-z': selection.z ?? '', 'tile-x': selection.x ?? '', 'tile-y': selection.y ?? '' }, 'Inspect tile'));
          buttons.push(selectionCameraActionButtonHtml('nearest_poi', 'Nearest hotspot'));
          buttons.push(selectionCameraActionButtonHtml('hottest_event', 'Hottest event'));
        }
        return buttons.join(' ');
      };
      const renderMapFocusPanel = (options) => {
        const focusSummaryNode = (options || {}).focusSummary;
        const focusDetailNode = (options || {}).focusDetail;
        const actionRailNode = (options || {}).actionRail;
        if (!focusSummaryNode || !focusDetailNode || !actionRailNode) return null;
        const focus = ((options || {}).focus) || buildDefaultFocus();
        const selection = buildSelectionFromFocus(focus);
        if (!selection) {
          focusSummaryNode.textContent = String(((options || {}).emptySummary) || 'Waiting for viewport focus…');
          focusDetailNode.textContent = String(((options || {}).emptyDetail) || 'Pick a region, tile, hotspot, or live event to steer movement and world actions.');
          actionRailNode.innerHTML = '';
          if (typeof (options || {}).onEmpty === 'function') options.onEmpty();
          return null;
        }
        focusSummaryNode.textContent = selection.title || 'Map focus';
        focusDetailNode.textContent = (selection.summary || 'world focus') + ' · ' + (selection.detail || '');
        actionRailNode.innerHTML = buildMapFocusActionButtonsHtml(selection, options);
        if (typeof (options || {}).onRendered === 'function') options.onRendered(selection);
        return selection;
      };"#
}

pub(super) fn real_world_map_focus_camera_js() -> &'static str {
    r#"      const focusMapSurface = (focus) => {
        if (!focus) return;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (!marker) return;
          mapAdapter.focus(mapRuntime, { lat: marker.lat, lng: marker.lng, zoom: Number(focus.zoom) || Math.max(mapAdapter.getZoom(mapRuntime), 16) });
          const markerLayer = markerLayerById.get(String(marker.node_id || ''));
          if (markerLayer) markerLayer.openPopup();
          if (!focus.suppressAction) {
            window.trillionniumApplyMarkerAction(marker.node_id || focus.nodeId, 'move_here');
          }
          return;
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const marker = markerById.get(String(eventItem.node_id || focus.nodeId || ''));
          if (!marker) return;
          mapAdapter.focus(mapRuntime, { lat: marker.lat, lng: marker.lng, zoom: Number(focus.zoom) || Math.max(mapAdapter.getZoom(mapRuntime), 16) });
          const markerLayer = markerLayerById.get(String(marker.node_id || ''));
          if (markerLayer) markerLayer.openPopup();
          if (!focus.suppressAction) {
            window.trillionniumApplyMarkerAction(marker.node_id || focus.nodeId, 'move_here');
          }
          return;
        }
        if (focus.kind === 'region') {
          const lat = Number(focus.lat);
          const lng = Number(focus.lng);
          if (!Number.isFinite(lat) || !Number.isFinite(lng)) return;
          mapAdapter.focus(mapRuntime, { lat, lng, zoom: Number(focus.zoom) || 12 });
          return;
        }
        if (focus.kind === 'tile') {
          const bounds = tileBoundsFromParts(Number(focus.z), Number(focus.x), Number(focus.y));
          if (!bounds) return;
          mapAdapter.fitBounds(mapRuntime, bounds.pad(0.18));
        }
      };
      const runCameraAction = (actionId) => {
        if (!lastViewport) return;
        if (actionId === 'active_region') {
          const region = lastViewport.active_region || (lastViewport.stream_region_shards || [])[0];
          if (!region) return;
          const centerPoint = region.center || {};
          const focus = { kind: 'region', lat: centerPoint.lat, lng: centerPoint.lng, zoom: region.zoom_max || 12 };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = 'Quick focus: active region · ' + (region.name || region.region_id || 'region');
          return;
        }
        if (actionId === 'nearest_poi') {
          const hotspot = (lastViewport.poi_hotspots || [])[0];
          if (!hotspot) return;
          const focus = { kind: 'node', nodeId: hotspot.node_id };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = 'Quick focus: nearest hotspot · ' + (hotspot.name || hotspot.node_id || 'poi');
          return;
        }
        if (actionId === 'hottest_event') {
          const hottestEvent = [...(lastViewport.live_event_stream || [])].sort((left, right) => Number(right.impact_score || 0) - Number(left.impact_score || 0))[0];
          if (!hottestEvent) return;
          const focus = { kind: 'event', nodeId: hottestEvent.node_id, eventId: hottestEvent.event_id, taskId: hottestEvent.cex_task_id, locationId: hottestEvent.location_id, eventKind: hottestEvent.event_kind, nodeName: hottestEvent.node_name, eventBody: hottestEvent.body, eventResult: hottestEvent.result, impact: hottestEvent.impact_score, suppressAction: true };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = 'Quick focus: hottest event · ' + (hottestEvent.event_kind || 'world_event') + ' · ' + (hottestEvent.node_name || hottestEvent.node_id || 'event');
        }
      };
"#
}

pub(super) fn real_world_map_static_marker_layers_js() -> &'static str {
    r#"      const mapMarkerActionButtonHtml = (marker, action) => `<button type="button" class="trillionnium-map-action" data-node-id="${escapeHtml(marker.node_id)}" data-action-id="${escapeHtml(action.action_id || 'move_here')}">${escapeHtml(action.label || action.command || 'Action')}</button>`;
      (engine.route_edges || []).forEach((edge) => {
        if (!edge.from || !edge.to) return;
        mapAdapter.renderRouteLine(mapRuntime, edge.from, edge.to, { color: '#64e3ff', weight: 2, opacity: 0.62 });
      });
      (engine.markers || []).forEach((marker) => {
        const actionButtons = (marker.primary_actions || []).map((action) => mapMarkerActionButtonHtml(marker, action)).join(' ');
        const actions = (marker.primary_actions || []).map((action) => `<br><code>${escapeHtml(action.command || action.label || '')}</code>`).join('');
        const popup = `<strong>${escapeHtml(marker.name)}</strong><br><code>${escapeHtml(marker.node_id)}</code><br>${escapeHtml(marker.description)}${actions}<br>${actionButtons}`;
        const markerLayer = mapAdapter.renderPoiMarker(mapRuntime, marker, popup);
        markerLayerById.set(String(marker.node_id || ''), markerLayer);
      });"#
}

pub(super) fn real_world_map_click_action_helpers_js() -> &'static str {
    r#"      const closestFromEvent = (event, selector) => event && event.target && event.target.closest ? event.target.closest(selector) : null;
      const mapClickSelectors = Object.freeze({
        action: '.trillionnium-map-action',
        overlay: '.trillionnium-overlay-toggle',
        selection: '.trillionnium-selection-action',
        camera: '.trillionnium-map-camera-action',
        focus: '.trillionnium-map-focus',
      });
      const handleMapActionButton = (button) => {
        if (!button) return false;
        window.trillionniumApplyMarkerAction(button.dataset.nodeId, button.dataset.actionId || 'move_here');
        return true;
      };
      const handleSelectionActionButton = (button, options = {}) => {
        if (!button) return false;
        const selectionKind = button.dataset.selectionKind;
        if (selectionKind === 'node') {
          const focus = buildSelectionFocusFromButton(button);
          focusMapSurface(focus);
          const handoff = window.trillionniumApplyMarkerAction(button.dataset.nodeId, button.dataset.actionId || 'move_here');
          if (typeof options.afterNodeAction === 'function') options.afterNodeAction(button, handoff);
          return true;
        }
        if (selectionKind === 'region' || selectionKind === 'tile') {
          const focus = buildSelectionFocusFromButton(button);
          focusMapSurface(focus);
          setFocusSelection(focus);
          return true;
        }
        if (selectionKind === 'camera') {
          runCameraAction(button.dataset.cameraAction);
          return true;
        }
        return false;
      };
      const handleMapCameraActionButton = (button) => {
        if (!button) return false;
        runCameraAction(button.dataset.cameraAction);
        return true;
      };
      const handleMapFocusButton = (button) => {
        if (!button) return false;
        const focus = buildMapFocusFromButton(button);
        focusMapSurface(focus);
        setFocusSelection(focus);
        return true;
      };"#
}

pub(super) fn real_world_map_overlay_render_js() -> &'static str {
    r#"      const renderViewportOverlays = (viewport) => {
        Object.values(overlayLayers).forEach((layer) => mapAdapter.clearOverlay(layer));
        const density = viewport.player_density || {};
        const densityRadiusMeters = density.mode === 'dense' ? 700 : density.mode === 'regional' ? 2800 : density.mode === 'sparse' ? 12000 : 30000;
        if (viewport.center && Number.isFinite(viewport.center.lat) && Number.isFinite(viewport.center.lng)) {
          mapAdapter.renderDensityCircle(overlayLayers.density, viewport.center, densityRadiusMeters, { color: '#64e3ff', weight: 1.2, fillColor: '#64e3ff', fillOpacity: 0.05 });
        }
        (viewport.stream_region_shards || []).forEach((region) => {
          const centerPoint = region.center || {};
          if (!Number.isFinite(centerPoint.lat) || !Number.isFinite(centerPoint.lng)) return;
          const stroke = region.status === 'active' ? '#f8c35b' : (region.status === 'warm' ? '#64e3ff' : '#8d97a6');
          mapAdapter.renderRegionAnchor(overlayLayers.regions, centerPoint, { radius: region.status === 'active' ? 9 : 7, color: stroke, fillColor: stroke, fillOpacity: 0.28, weight: 1.6 })
            .bindTooltip(`${region.name || 'Region'} · ${region.status || 'planned'}`);
        });
        (viewport.visible_tile_shards || []).forEach((tile) => {
          const bounds = tileBoundsFromParts(Number(tile.z), Number(tile.x), Number(tile.y));
          if (!bounds) return;
          const active = tile.tile_status === 'active';
          mapAdapter.renderTileFrame(overlayLayers.tiles, bounds, { color: active ? '#64e3ff' : '#34506d', weight: active ? 2 : 1, fillColor: active ? '#64e3ff' : '#203244', fillOpacity: active ? 0.12 : 0.02 })
            .bindTooltip(`${tile.tile_id || 'tile'} · ${tile.marker_count ?? 0} nodes`);
        });
        (viewport.prefetch_queue || []).forEach((tile) => {
          const bounds = tileBoundsFromParts(Number(tile.z), Number(tile.x), Number(tile.y));
          if (!bounds) return;
          mapAdapter.renderTileFrame(overlayLayers.prefetch, bounds, { color: '#f8c35b', weight: 2, dashArray: '6 6', fillColor: '#f8c35b', fillOpacity: 0.04 })
            .bindTooltip(`prefetch · ${tile.priority_label || 'warm'} · ${tile.tile_id || 'tile'}`);
        });
        const visibleMarkerById = new Map((viewport.visible_markers || []).map((marker) => [String(marker.node_id || ''), marker]));
        (viewport.live_event_stream || []).forEach((eventItem) => {
          const marker = visibleMarkerById.get(String(eventItem.node_id || '')) || markerById.get(String(eventItem.node_id || ''));
          if (!marker) return;
          const impact = Number(eventItem.impact_score || 0);
          const eventFocus = buildEventFocus(eventItem);
          mapAdapter.renderEventPulse(overlayLayers.events, marker, { radius: Math.max(6, Math.min(12, 5 + impact / 3)), color: '#ff8d4d', fillColor: '#ff8d4d', fillOpacity: 0.3, weight: 1.4 })
            .bindTooltip(`${eventItem.event_kind || 'world_event'} · ${eventItem.node_name || marker.name || eventItem.location_id || 'POI'}`)
            .on('click', () => {
              focusMapSurface(eventFocus);
              setFocusSelection(eventFocus);
            });
        });
        if (overlayLegend) {
          const regionName = ((viewport.active_region || {}).name) || 'Region';
          overlayLegend.textContent = 'Overlay legend: ' + regionName + ' anchors · ' + (viewport.tile_shard_count || 0) + ' tile frames · ' + (viewport.prefetch_count || 0) + ' prefetch warm ring · ' + (viewport.live_event_count || 0) + ' live event pulses.';
        }
        Object.keys(overlayLayers).forEach((name) => setOverlayLayerVisibility(name, overlayState[name] !== false));
      };
      const handleOverlayToggleButton = (button) => {
        const key = button && button.dataset ? button.dataset.overlayTarget : '';
        if (!Object.prototype.hasOwnProperty.call(overlayState, key)) return false;
        overlayState[key] = !overlayState[key];
        setOverlayLayerVisibility(key, overlayState[key]);
        refreshOverlayControls();
        renderOverlayStatus();
        return true;
      };
"#
}

pub(super) fn real_world_map_card_focus_helpers_js() -> &'static str {
    r#"      const mapFocusButtonAttrs = (attrs) => Object.entries(attrs || {})
        .filter(([, value]) => value !== undefined && value !== null)
        .map(([name, value]) => ` data-${name}="${escapeHtml(value)}"`)
        .join('');
      const mapRegionFocusButton = (item, label = 'Focus region') => {
        const centerPoint = (item && item.center) || {};
        return `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'region', lat: centerPoint.lat ?? '', lng: centerPoint.lng ?? '', zoom: (item || {}).zoom_max ?? 12 })}>${escapeHtml(label)}</button>`;
      };
      const mapTileFocusButton = (item, label = 'Inspect tile') => `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'tile', 'tile-z': (item || {}).z ?? '', 'tile-x': (item || {}).x ?? '', 'tile-y': (item || {}).y ?? '' })}>${escapeHtml(label)}</button>`;
      const mapNodeFocusButton = (item, label = 'Focus POI') => `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'node', 'node-id': (item || {}).node_id || '' })}>${escapeHtml(label)}</button>`;
      const mapEventFocusButton = (item, label = 'Track event') => {
        const eventItem = item || {};
        return `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'event', 'node-id': eventItem.node_id || '', 'event-id': eventItem.event_id || '', 'task-id': eventItem.cex_task_id || '', 'location-id': eventItem.location_id || '', 'event-kind': eventItem.event_kind || 'world_event', 'node-name': eventItem.node_name || eventItem.location_id || 'POI', 'event-body': eventItem.body || '', 'event-result': eventItem.result || '', 'suppress-action': 'true' })}>${escapeHtml(label)}</button>`;
      };
      const mapViewportCardModel = (item, kind, style = 'app') => {
        const source = item || {};
        const worldStyle = style === 'world';
        if (kind === 'region') {
          return {
            className: worldStyle ? 'mini shard' : 'module',
            title: source.name || 'Region',
            meta: worldStyle
              ? `${source.status || 'planned'} · ${source.coverage_kind || 'shard'} · ${source.distance_km ?? 0} km`
              : `${source.status || 'planned'} · ${source.distance_km ?? 0} km`,
            code: source.region_id || 'region',
            focusHtml: mapRegionFocusButton(source),
          };
        }
        if (kind === 'tile') {
          return {
            className: worldStyle ? 'mini tile' : 'module',
            title: source.tile_status || 'tile',
            meta: `${source.lod_mode || 'lod'} · ${source.marker_count ?? 0} nodes`,
            code: source.tile_id || 'tile',
            focusHtml: mapTileFocusButton(source, 'Inspect tile'),
          };
        }
        if (kind === 'prefetch') {
          return {
            className: worldStyle ? 'mini prefetch' : 'module',
            title: source.priority_label || 'warm',
            meta: `${source.prefetch_reason || 'neighbor_tile_warmup'} · ${source.marker_count ?? 0} nodes`,
            code: source.tile_id || 'tile',
            focusHtml: mapTileFocusButton(source, 'Warm tile'),
          };
        }
        if (kind === 'event') {
          return {
            className: worldStyle ? 'mini event' : 'module',
            title: source.event_kind || 'world_event',
            meta: `${source.node_name || source.location_id || 'POI'} · ${source.distance_km ?? 'global'} km`,
            code: source.event_id || 'event',
            focusHtml: mapEventFocusButton(source, 'Track event'),
          };
        }
        return {
          className: worldStyle ? 'mini poi' : 'module',
          title: source.name || 'POI',
          meta: `${source.node_kind || 'poi'} · ${source.distance_km ?? 0} km`,
          code: source.node_id || 'node',
          focusHtml: mapNodeFocusButton(source, 'Focus POI'),
        };
      };
      const mapViewportCardHtml = (item, kind, style = 'app') => {
        const card = mapViewportCardModel(item, kind, style);
        const focusStack = `<div class="focus-stack">${card.focusHtml}</div>`;
        if (style === 'world') {
          return `<article class="${escapeHtml(card.className)}"><strong>${escapeHtml(card.title)}</strong><span>${escapeHtml(card.meta)}</span><code>${escapeHtml(card.code)}</code>${focusStack}</article>`;
        }
        return `<article class="${escapeHtml(card.className)}"><strong>${escapeHtml(card.title)}</strong><span>${escapeHtml(card.meta)}</span><p><code>${escapeHtml(card.code)}</code></p>${focusStack}</article>`;
      };
      const buildMapFocusFromButton = (button) => {
        const dataset = (button && button.dataset) || {};
        return {
          kind: dataset.focusKind,
          nodeId: dataset.nodeId,
          eventId: dataset.eventId,
          taskId: dataset.taskId,
          locationId: dataset.locationId,
          eventKind: dataset.eventKind,
          nodeName: dataset.nodeName,
          eventBody: dataset.eventBody,
          eventResult: dataset.eventResult,
          suppressAction: dataset.suppressAction === 'true',
          lat: dataset.lat,
          lng: dataset.lng,
          zoom: dataset.zoom,
          z: dataset.tileZ,
          x: dataset.tileX,
          y: dataset.tileY,
        };
      };
      const buildSelectionFocusFromButton = (button) => {
        const dataset = (button && button.dataset) || {};
        const selectionKind = dataset.selectionKind;
        if (selectionKind === 'node') return { kind: 'node', nodeId: dataset.nodeId, suppressAction: true };
        if (selectionKind === 'region') return { kind: 'region', lat: dataset.lat, lng: dataset.lng, zoom: dataset.zoom };
        if (selectionKind === 'tile') return { kind: 'tile', z: dataset.tileZ, x: dataset.tileX, y: dataset.tileY };
        return null;
      };
"#
}

pub(super) fn real_world_map_render_cards_js(style: RealWorldMapShellCardStyle) -> String {
    let style_name = match style {
        RealWorldMapShellCardStyle::AppModule => "app",
        RealWorldMapShellCardStyle::WorldMini => "world",
    };
    format!(
        r#"      const renderCards = (targetNode, items, kind) => {{
        if (!targetNode) return;
        targetNode.innerHTML = items.map((item) => mapViewportCardHtml(item, kind, '{style_name}')).join('');
      }};
"#
    )
}

pub(super) fn real_world_map_route_target_resolution_js() -> &'static str {
    r#"      const resolveRouteTargetNodeId = (locationId, explicitNodeId, fallbackNodeId) => {
        const directNodeId = String(explicitNodeId || '').trim();
        if (directNodeId && markerById.has(directNodeId)) return directNodeId;
        const routeLocationId = String(locationId || '').trim();
        if (routeLocationId) {
          const marker = markerByLocationId.get(routeLocationId);
          if (marker && marker.node_id) return String(marker.node_id);
        }
        return String(fallbackNodeId || '').trim();
      };
"#
}

pub(super) fn real_world_map_route_status_js() -> &'static str {
    r#"      const routeOpportunitySegment = (task) => {
        const kind = String(((task || {}).next_opportunity_kind) || '').trim();
        return kind ? (' · opportunity ' + kind) : '';
      };
      const routeEventBriefText = (eventSignalText, fullRouteVisible) => {
        const normalized = String(eventSignalText || '').replace(/^Latest event signal:\s*/, '').trim();
        if (!normalized) return 'Focused event brief: no live event selected.';
        return 'Focused event brief: ' + normalized + (fullRouteVisible ? ' · full route visible.' : '');
      };
      const routeLinkStatusText = (context) => {
        const taskId = String((context || {}).taskId || '').trim();
        if (!taskId) return String((context || {}).emptyText || 'Linked task route: none yet.');
        const linkedEventCount = Number((context || {}).linkedEventCount || 0);
        const linkedContractCount = Number((context || {}).linkedContractCount || 0);
        const contractText = (context || {}).inFocus ? ' contracts in focus' : ' contracts';
        return 'Linked task route: ' + taskId + ' · ' + linkedEventCount + ' events · ' + linkedContractCount + contractText + routeOpportunitySegment((context || {}).opportunityTask) + '.';
      };
"#
}

pub(super) fn real_world_map_route_contract_js() -> String {
    let contract_json = world_route_ui_contract_json().to_string();
    format!(
        r#"      const routeUiContract = (() => {{
        const fallback = {contract_json};
        const source = (typeof app !== 'undefined' && app && app.route_contract)
          || (typeof payload !== 'undefined' && payload && payload.route_contract)
          || fallback;
        if (!source || typeof source !== 'object') return fallback;
        return {{
          ...fallback,
          ...source,
          panels: {{ ...(fallback.panels || {{}}), ...((source.panels) || {{}}) }},
          fields: {{ ...(fallback.fields || {{}}), ...((source.fields) || {{}}) }},
          panel_defaults: {{ ...(fallback.panel_defaults || {{}}), ...((source.panel_defaults) || {{}}) }},
          work_lanes: {{ ...(fallback.work_lanes || {{}}), ...((source.work_lanes) || {{}}) }},
          handoff: {{ ...(fallback.handoff || {{}}), ...((source.handoff) || {{}}) }},
        }};
      }})();
      const routeHandoffFields = () => routeUiContract.handoff || {{}};
      const routeHandoffFieldName = (fieldKey, fallback) => routeHandoffFields()[fieldKey] || fallback;
      const routeHandoffStorageKey = () => routeHandoffFieldName('storage_key', 'trillionnium-world-handoff');
      const routeHandoffRead = (handoff, fieldKey, fallback) => {{
        if (!handoff || typeof handoff !== 'object') return fallback;
        const fieldName = routeHandoffFieldName(fieldKey, fallback);
        if (Object.prototype.hasOwnProperty.call(handoff, fieldName) && handoff[fieldName] != null) {{
          return handoff[fieldName];
        }}
        if (fieldKey !== fieldName && Object.prototype.hasOwnProperty.call(handoff, fieldKey) && handoff[fieldKey] != null) {{
          return handoff[fieldKey];
        }}
        return fallback;
      }};
      const routeHandoffValue = (handoff, fieldKey, fallback = '') => {{
        const value = routeHandoffRead(handoff, fieldKey, fallback);
        return value == null ? fallback : value;
      }};
      const routeHandoffPanelId = (handoff, fallback = routeActionPanelId()) => routeHandoffValue(handoff, 'panel_id', fallback);
      const routeHandoffActionLabel = (handoff, fallback = 'Action') => {{
        const label = String(routeHandoffValue(handoff, 'action_label', '') || '').trim();
        if (label) return label;
        const actionId = String(routeHandoffValue(handoff, 'action_id', '') || '').trim();
        return actionId || fallback;
      }};
      const routeHandoffCommand = (handoff, fallback = 'prepared') => {{
        const command = String(routeHandoffValue(handoff, 'command', '') || '').trim();
        return command || fallback;
      }};
      const buildRouteHandoffState = (handoff, defaults = {{}}) => {{
        const fallback = defaults || {{}};
        return {{
          nodeId: routeHandoffValue(handoff, 'node_id', fallback.nodeId || ''),
          locationId: routeHandoffValue(handoff, 'location_id', fallback.locationId || ''),
          actionId: routeHandoffValue(handoff, 'action_id', fallback.actionId || ''),
          actionLabel: routeHandoffActionLabel(handoff, fallback.actionLabel || 'Action'),
          command: routeHandoffCommand(handoff, fallback.command || 'prepared'),
          panelId: routeHandoffPanelId(handoff, fallback.panelId || routeActionPanelId()),
          actionBody: routeHandoffValue(handoff, 'action_body', fallback.actionBody || ''),
          targetInputId: routeHandoffValue(handoff, 'target_input_id', fallback.targetInputId || ''),
          targetValue: routeHandoffValue(handoff, 'target_value', fallback.targetValue || ''),
          targetTextareaId: routeHandoffValue(handoff, 'target_textarea_id', fallback.targetTextareaId || ''),
          moveTarget: routeHandoffValue(handoff, 'move_target', fallback.moveTarget || ''),
          listingId: routeHandoffValue(handoff, 'listing_id', fallback.listingId || ''),
          workOrderId: routeHandoffValue(handoff, 'work_order_id', fallback.workOrderId || ''),
          contractId: routeHandoffValue(handoff, 'contract_id', fallback.contractId || ''),
          routeTaskId: routeHandoffValue(handoff, 'route_task_id', fallback.routeTaskId || ''),
          eventId: routeHandoffValue(handoff, 'event_id', fallback.eventId || ''),
          eventKind: routeHandoffValue(handoff, 'event_kind', fallback.eventKind || ''),
          eventBody: routeHandoffValue(handoff, 'event_body', fallback.eventBody || ''),
          eventResult: routeHandoffValue(handoff, 'event_result', fallback.eventResult || ''),
        }};
      }};
      const buildMarkerRouteActionState = (action, handoff, nodeId) => buildRouteHandoffState(handoff, {{
        actionLabel: ((action || {{}}).label) || ((action || {{}}).action_id) || 'Action',
        command: ((action || {{}}).command) || ('/go ' + nodeId),
        moveTarget: nodeId,
      }});
      const buildRouteHandoffRecord = (payload) => {{
        const source = payload || {{}};
        return {{
          [routeHandoffFieldName('node_id', 'node_id')]: source.node_id || null,
          [routeHandoffFieldName('location_id', 'location_id')]: source.location_id || null,
          [routeHandoffFieldName('action_id', 'action_id')]: source.action_id || null,
          [routeHandoffFieldName('action_label', 'action_label')]: source.action_label || null,
          [routeHandoffFieldName('command', 'command')]: source.command || null,
          [routeHandoffFieldName('panel_id', 'web_panel_id')]: source.web_panel_id || null,
          [routeHandoffFieldName('action_body', 'web_action_body')]: source.web_action_body || null,
          [routeHandoffFieldName('target_input_id', 'web_target_input_id')]: source.web_target_input_id || null,
          [routeHandoffFieldName('target_value', 'web_target_value')]: source.web_target_value || null,
          [routeHandoffFieldName('target_textarea_id', 'web_target_textarea_id')]: source.web_target_textarea_id || null,
          [routeHandoffFieldName('move_target', 'web_move_target')]: source.web_move_target || null,
          [routeHandoffFieldName('listing_id', 'web_listing_id')]: source.web_listing_id || null,
          [routeHandoffFieldName('work_order_id', 'web_work_order_id')]: source.web_work_order_id || null,
          [routeHandoffFieldName('contract_id', 'web_contract_id')]: source.web_contract_id || null,
          [routeHandoffFieldName('route_task_id', 'web_route_task_id')]: source.web_route_task_id || null,
          [routeHandoffFieldName('event_id', 'web_event_id')]: source.web_event_id || null,
          [routeHandoffFieldName('event_kind', 'web_event_kind')]: source.web_event_kind || null,
          [routeHandoffFieldName('event_body', 'web_event_body')]: source.web_event_body || null,
          [routeHandoffFieldName('event_result', 'web_event_result')]: source.web_event_result || null,
        }};
      }};
      const forceRouteFieldValue = (input, value, options = {{}}) => {{
        if (!input || value == null || value === '') return input;
        input.value = value;
        input.dataset.routeAutofilled = 'true';
        if (options.clearManual) input.dataset.routeManual = 'false';
        return input;
      }};
      const forceRouteFieldValueById = (inputId, value, options = {{}}) => {{
        if (!inputId) return null;
        const input = document.getElementById(inputId);
        return forceRouteFieldValue(input, value, options);
      }};
      const scrollRoutePanelIntoView = (panelId) => {{
        const panel = document.getElementById(panelId);
        if (panel) panel.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
        return panel;
      }};
"#
    )
}

pub(super) fn real_world_map_route_flow_buttons_js() -> &'static str {
    r#"      const buildRouteActionKey = (action) => {
        return [
          action && action.label || '',
          action && action.panelId || '',
          action && action.inputId || '',
          action && action.value || '',
          action && action.textareaId || '',
          action && action.locationId || '',
          action && action.targetNodeId || '',
          action && action.eventId || '',
          action && action.eventKind || '',
          action && action.eventTaskId || '',
          action && action.workOrderId || '',
          action && action.contractId || '',
          action && action.listingId || '',
          action && action.taskId || '',
          action && action.body || '',
        ].join('|');
      };
      const pushUniqueRouteAction = (items, action) => {
        if (!action || !action.label || !action.panelId || !Array.isArray(items)) return false;
        const key = buildRouteActionKey(action);
        if (items.some((item) => item && item.key === key)) return false;
        items.push({ ...action, key });
        return true;
      };
      const buildRouteOpportunityAction = (task, fallbackLocationId) => {
        if (!task || !(task.next_opportunity_command || task.next_opportunity_body)) return null;
        return buildSimpleRouteTargetAction({
          label: task.next_opportunity_action_label || 'Route next opportunity',
          panelId: task.next_opportunity_panel_id || routeActionPanelId(),
          inputId: task.next_opportunity_input_id || '',
          value: task.next_opportunity_input_value || '',
          textareaId: task.next_opportunity_textarea_id || routeActionTextareaId(),
          locationId: task.latest_location_id || fallbackLocationId || '',
          targetNodeId: task.next_opportunity_node_id || '',
          taskId: task.task_id || '',
          body: task.next_opportunity_body || task.next_opportunity_command || '',
        });
      };
      const routeFlowActionAttrs = (action) => ` data-target-panel="${escapeHtml(action.panelId)}" data-target-input-id="${escapeHtml(action.inputId || '')}" data-target-value="${escapeHtml(action.value || '')}" data-target-textarea-id="${escapeHtml(action.textareaId || routeActionTextareaId())}" data-target-location-id="${escapeHtml(action.locationId || '')}" data-target-node-id="${escapeHtml(action.targetNodeId || '')}" data-target-task-id="${escapeHtml(action.taskId || '')}" data-target-contract-id="${escapeHtml(action.contractId || '')}" data-target-listing-id="${escapeHtml(action.listingId || '')}" data-target-work-order-id="${escapeHtml(action.workOrderId || '')}" data-target-event-id="${escapeHtml(action.eventId || '')}" data-target-event-kind="${escapeHtml(action.eventKind || '')}" data-target-event-body="${escapeHtml(action.eventBody || '')}" data-target-event-result="${escapeHtml(action.eventResult || '')}" data-target-event-task-id="${escapeHtml(action.eventTaskId || '')}" data-target-body="${escapeHtml(action.body || '')}"`;
      const routeFlowActionButtonHtml = (action, className) => {
        if (!action || !action.label || !action.panelId) return '';
        const buttonClass = String(className || 'trillionnium-route-flow-action').trim() || 'trillionnium-route-flow-action';
        return `<button type="button" class="focus-chip ${escapeHtml(buttonClass)}"${routeFlowActionAttrs(action)}>${escapeHtml(action.label)}</button>`;
      };
      const pushRouteFlowActionButton = (buttons, keySet, action) => {
        if (!action || !action.label || !action.panelId) return;
        const key = buildRouteActionKey(action);
        if (keySet.has(key)) return;
        keySet.add(key);
        const html = routeFlowActionButtonHtml(action);
        if (html) buttons.push(html);
      };
      const routeTaskGraphActionButtonsHtml = (task, className) => {
        const source = task || {};
        const suggestedAction = buildTaskSuggestedAction(source);
        const opportunityAction = buildRouteOpportunityAction(source, source.latest_location_id || '');
        return routeFlowActionButtonHtml(suggestedAction, className) + routeFlowActionButtonHtml(opportunityAction, className);
      };
      const indexedRouteActionButtonHtml = (action, index, className = 'trillionnium-app-route-action') => `<button type="button" class="focus-chip ${escapeHtml(className)}" data-route-action-index="${escapeHtml(index)}">${escapeHtml((action || {}).label || 'Route action')}</button>`;
"#
}

pub(super) fn real_world_map_route_contract_accessors_js() -> &'static str {
    r#"      const inferConfiguredRouteNextStep = (selection, routeContext, config) => {
        const settings = config || {};
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || 'Focused route');
        const withEventSignal = (body) => appendSelectionEventSignal(body, selection);
        const locationId = routeContext.locationId || '';
        const taskId = routeContext.taskId || '';
        const workOrderId = routeContext.workOrderId || '';
        const contractId = routeContext.contractId || '';
        const listingId = routeContext.listingId || '';
        const latestWorkBucket = routeContext.latestWorkBucket || '';
        const statusPrefix = String(settings.statusPrefix || 'Recommended next step');
        const formatStatus = (message) => statusPrefix + ': ' + message;
        const buildAction = (action) => buildSimpleRouteTargetAction({
          locationId,
          taskId,
          workOrderId,
          contractId,
          listingId,
          ...action,
        });
        if (latestWorkBucket === 'rejection' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('reopen', workOrderId, {
            label: 'Route reopen',
            body: withEventSignal(settings.rejectionBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.rejectionStatus(workOrderId)),
          }));
        }
        if (latestWorkBucket === 'reopen' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('delivery', workOrderId, {
            label: 'Route redelivery',
            body: withEventSignal(settings.reopenBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.reopenStatus(workOrderId)),
          }));
        }
        if (latestWorkBucket === 'delivery' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('acceptance', workOrderId, {
            body: withEventSignal(settings.deliveryBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.deliveryStatus(workOrderId)),
          }));
        }
        if ((latestWorkBucket === 'work_order' || latestWorkBucket === 'purchase') && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('delivery', workOrderId, {
            body: withEventSignal(settings.openWorkBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.openWorkStatus(workOrderId)),
          }));
        }
        if ((latestWorkBucket === 'acceptance' || latestWorkBucket === 'cancellation') && workOrderId && settings.closedWorkBody && settings.closedWorkStatus) {
          return buildAction({
            label: settings.closedWorkLabel || 'Draft follow-up action',
            panelId: routeActionPanelId(),
            textareaId: routeActionTextareaId(),
            body: withEventSignal(settings.closedWorkBody(selectionTitle, workOrderId, latestWorkBucket)),
            status: formatStatus(settings.closedWorkStatus(workOrderId, latestWorkBucket)),
          });
        }
        if (contractId) {
          return buildAction(buildWorldContractLaneAction(contractId, {
            body: withEventSignal(settings.contractBody(selectionTitle, contractId)),
            status: formatStatus(settings.contractStatus(contractId)),
          }));
        }
        if (listingId) {
          return buildAction(buildWorldPurchaseLaneAction(listingId, {
            body: withEventSignal(settings.listingBody(selectionTitle, listingId)),
            status: formatStatus(settings.listingStatus(listingId)),
          }));
        }
        return buildAction({
          label: settings.defaultLabel || 'Draft world action',
          panelId: routeActionPanelId(),
          textareaId: routeActionTextareaId(),
          body: withEventSignal(settings.defaultBody(selectionTitle)),
          status: formatStatus(settings.defaultStatus()),
        });
      };
      const buildRouteDraftBody = (selection, routeContext, options) => {
        const settings = options || {};
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || 'Focused world route');
        const detail = [];
        if (!settings.omitContextDetails) {
          if (routeContext.locationId) detail.push('location ' + routeContext.locationId);
          if (routeContext.taskId) detail.push('task ' + routeContext.taskId);
          if (routeContext.eventLabel && routeContext.eventLabel !== 'no event') detail.push('event ' + routeContext.eventLabel);
          if (routeContext.workOrderId) detail.push('work order ' + routeContext.workOrderId);
          if (routeContext.contractId) detail.push('contract ' + routeContext.contractId);
          if (routeContext.listingId) detail.push('listing ' + routeContext.listingId);
          if (routeContext.recommendedLabel) detail.push('next step ' + String(routeContext.recommendedLabel).toLowerCase());
        }
        const leadIn = String(settings.leadIn || ': continue the world flow for ');
        const detailText = detail.join(' · ') || String(settings.emptyDetail || 'the active route');
        const suffix = String(settings.suffix || '. Align deliverable, evidence, acceptance, risks, and next action for this map focus.');
        return appendSelectionEventSignal(selectionTitle + leadIn + detailText + suffix, selection);
      };
      const buildTaskFollowUpDraftBody = (selection, taskId, detailText) => {
        return buildRouteDraftBody(selection, {}, {
          selectionTitleFallback: 'Focused route',
          omitContextDetails: true,
          leadIn: ': follow up on ',
          emptyDetail: 'task ' + taskId + ' ' + detailText,
          suffix: '.',
        });
      };
      const routePanels = () => routeUiContract.panels || {};
      const routeFields = () => routeUiContract.fields || {};
      const routePanelDefaults = (panelId) => ((routeUiContract.panel_defaults || {})[panelId]) || {};
      const routeActionPanelId = () => routePanels().action || 'world-action-console';
      const routeCommercePanelId = () => routePanels().commerce || 'world-commerce-panel';
      const routeContractsPanelId = () => routePanels().contracts || 'world-contracts-panel';
      const routeEventTimelinePanelId = () => routePanels().event_timeline || 'world-event-timeline';
      const routeMapMovePanelId = () => routePanels().map_move || 'world-map-move-panel';
      const routeMoveTargetId = () => routeFields().move_target || 'world-map-move-target';
      const routeActionLocationId = () => routeFields().action_location || 'world-action-location';
      const routePurchaseInputId = () => routeFields().purchase_input || 'world-buy-listing-id';
      const routePurchaseTextareaId = () => routeFields().purchase_textarea || 'world-buy-body';
      const routeContractInputId = () => routeFields().contract_input || 'world-contract-completion-id';
      const routeContractTextareaId = () => routeFields().contract_textarea || 'world-contract-completion-body';
      const routeActionTextareaId = () => routeFields().action_textarea || 'world-action-body';
      const routeWorkLaneKinds = () => {
        const laneOrder = routeUiContract.work_lane_order;
        if (Array.isArray(laneOrder) && laneOrder.length) return laneOrder;
        return ['delivery', 'acceptance', 'rejection', 'reopen', 'cancellation'];
      };
      const routeWorkLaneIds = (laneId) => {
        const normalizedLaneId = String(laneId || '').trim() || 'delivery';
        const workLanes = routeUiContract.work_lanes || {};
        return workLanes[normalizedLaneId] || workLanes.delivery || { inputId: 'world-work-deliver-id', textareaId: 'world-work-deliver-body' };
      };
      const routeWorkLaneInputId = (laneId) => routeWorkLaneIds(laneId).input_id || routeWorkLaneIds(laneId).inputId || '';
      const routeWorkLaneTextareaId = (laneId) => routeWorkLaneIds(laneId).textarea_id || routeWorkLaneIds(laneId).textareaId || '';
      const routePanelInputId = (panelId) => routePanelDefaults(panelId).input_id || routePanelDefaults(panelId).inputId || '';
      const routePanelTextareaId = (panelId) => routePanelDefaults(panelId).textarea_id || routePanelDefaults(panelId).textareaId || routeActionTextareaId();
      const routeWorkLaneInputIds = () => routeWorkLaneKinds().map((laneId) => routeWorkLaneInputId(laneId));
      const routeWorkLaneTextareaIds = () => routeWorkLaneKinds().map((laneId) => routeWorkLaneTextareaId(laneId));
      const routeManualTrackedFieldIds = () => [routePurchaseInputId(), ...routeWorkLaneInputIds(), routeContractInputId(), routePurchaseTextareaId(), ...routeWorkLaneTextareaIds(), routeContractTextareaId(), routeActionTextareaId()];
"#
}

pub(super) fn real_world_map_route_target_builders_js() -> &'static str {
    r#"      const buildTaskSuggestedAction = (task) => {
        const taskInfo = task || {};
        const panelId = taskInfo.suggested_panel_id || routeActionPanelId();
        const contractId = taskInfo.latest_contract_id || '';
        const inputId = taskInfo.suggested_input_id || routePanelInputId(panelId);
        const inputValue = taskInfo.suggested_input_value || (inputId === routeContractInputId() ? contractId : '');
        const textareaId = taskInfo.suggested_textarea_id || routePanelTextareaId(panelId);
        return buildSimpleRouteTargetAction({
          label: taskInfo.suggested_action_label || 'Draft task follow-up',
          panelId,
          inputId,
          value: inputValue,
          textareaId,
          locationId: taskInfo.latest_location_id || '',
          targetNodeId: taskInfo.suggested_node_id || '',
          contractId,
          taskId: taskInfo.task_id || '',
          body: taskInfo.suggested_body || '',
        });
      };
      const buildLinkedContractRouteAction = (selection, contractId, taskId, locationId, options) => {
        const settings = options || {};
        const effectiveContractId = String(contractId || '').trim();
        const effectiveTaskId = String(taskId || effectiveContractId).trim();
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || 'Focused route');
        return buildWorldContractLaneAction(effectiveContractId, {
          label: settings.label || 'Route linked contract',
          locationId: locationId || '',
          taskId: effectiveTaskId,
          body: appendSelectionEventSignal(selectionTitle + ': complete linked contract ' + effectiveContractId + ' for task ' + effectiveTaskId + (settings.bodySuffix || '.'), selection),
        });
      };
      const buildRouteEventTimelineAction = (label, eventContext) => {
        const event = eventContext || {};
        return {
          label: label || 'Open event timeline',
          panelId: routeEventTimelinePanelId(),
          locationId: String(event.locationId || '').trim(),
          eventId: String(event.eventId || '').trim(),
          eventKind: String(event.eventKind || 'world_event').trim(),
          eventBody: String(event.eventBody || '').trim(),
          eventResult: String(event.eventResult || '').trim(),
          eventTaskId: String(event.eventTaskId || '').trim(),
          body: String(event.body || ''),
        };
      };
      const buildRouteActionFromDataset = (dataset, fallbackLabel) => {
        const source = dataset || {};
        return buildSimpleRouteTargetAction({
          label: String(fallbackLabel || '').trim() || 'Route action',
          panelId: source.targetPanel || routeActionPanelId(),
          inputId: source.targetInputId || '',
          value: source.targetValue || '',
          textareaId: source.targetTextareaId || routeActionTextareaId(),
          locationId: source.targetLocationId || '',
          targetNodeId: source.targetNodeId || '',
          taskId: source.targetTaskId || '',
          contractId: source.targetContractId || '',
          listingId: source.targetListingId || '',
          workOrderId: source.targetWorkOrderId || '',
          eventId: source.targetEventId || '',
          eventKind: source.targetEventKind || '',
          eventBody: source.targetEventBody || '',
          eventResult: source.targetEventResult || '',
          eventTaskId: source.targetEventTaskId || '',
          body: source.targetBody || '',
        });
      };
      const buildRouteActionFromButton = (button, fallbackLabel) => {
        const label = String((button && button.textContent) || '').trim() || fallbackLabel || 'Route action';
        return buildRouteActionFromDataset((button && button.dataset) || {}, label);
      };
      const handleRouteActionButton = (button, opener, fallbackLabel) => {
        if (!button || typeof opener !== 'function') return false;
        opener(buildRouteActionFromButton(button, fallbackLabel));
        return true;
      };
      const handleIndexedRouteActionButton = (button, actions, opener) => {
        if (!button || typeof opener !== 'function') return false;
        const action = (actions || [])[Number(button.dataset.routeActionIndex || -1)];
        if (action) opener(action);
        return true;
      };
      const routeFilterModeFromButton = (button, fallbackMode = 'all') => {
        const value = String(((button && button.dataset) || {}).routeFilter || '').trim();
        if (value === 'all' || value === 'selection') return value;
        return fallbackMode;
      };
      const buildRouteHandoffPayload = (action, selection) => {
        const routeAction = action || {};
        const focus = selection || {};
        const fallbackNodeId = focus.kind === 'node' ? (focus.nodeId || '') : '';
        const targetNodeId = resolveRouteTargetNodeId(routeAction.locationId || focus.locationId || '', routeAction.targetNodeId || '', fallbackNodeId);
        const eventId = String(routeAction.eventId || ((focus.kind === 'event' && focus.eventId) ? focus.eventId : '') || '').trim();
        return {
          targetNodeId,
          eventId,
          payload: buildRouteHandoffRecord({
            node_id: targetNodeId || null,
            location_id: routeAction.locationId || focus.locationId || null,
            action_id: 'app_route_handoff',
            action_label: routeAction.label || 'World route handoff',
            command: '/world',
            web_panel_id: routeAction.panelId || routeActionPanelId(),
            web_action_body: routeAction.body || '',
            web_target_input_id: routeAction.inputId || null,
            web_target_value: routeAction.value || null,
            web_target_textarea_id: routeAction.textareaId || null,
            web_move_target: targetNodeId || null,
            web_listing_id: routeAction.listingId || null,
            web_work_order_id: routeAction.workOrderId || null,
            web_contract_id: routeAction.contractId || null,
            web_route_task_id: routeAction.taskId || null,
            web_event_id: eventId || null,
            web_event_kind: routeAction.eventKind || ((focus.kind === 'event' && focus.eventKind) ? focus.eventKind : null),
            web_event_body: routeAction.eventBody || ((focus.kind === 'event' && focus.eventBody) ? focus.eventBody : null),
            web_event_result: routeAction.eventResult || ((focus.kind === 'event' && focus.eventResult) ? focus.eventResult : null),
          }),
        };
      };
      const buildSimpleRouteTargetAction = (config) => {
        const target = config || {};
        return {
          label: target.label || 'Route action',
          panelId: target.panelId || routeActionPanelId(),
          inputId: target.inputId || '',
          value: target.value || '',
          textareaId: target.textareaId || routeActionTextareaId(),
          locationId: target.locationId || '',
          targetNodeId: target.targetNodeId || '',
          workOrderId: target.workOrderId || '',
          contractId: target.contractId || '',
          listingId: target.listingId || '',
          taskId: target.taskId || '',
          eventId: target.eventId || '',
          eventKind: target.eventKind || '',
          eventBody: target.eventBody || '',
          eventResult: target.eventResult || '',
          eventTaskId: target.eventTaskId || '',
          status: target.status || '',
          body: target.body || '',
        };
      };
      const buildWorldWorkLaneAction = (laneId, workOrderId, options) => {
        const target = options || {};
        const normalizedLaneId = String(laneId || '').trim();
        const defaultLabel = normalizedLaneId === 'acceptance'
          ? 'Route acceptance'
          : normalizedLaneId === 'rejection'
            ? 'Route rejection'
            : normalizedLaneId === 'reopen'
              ? 'Route reopen'
              : normalizedLaneId === 'cancellation'
                ? 'Route cancellation'
                : 'Route delivery';
        const effectiveWorkOrderId = String(workOrderId || target.value || target.workOrderId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || defaultLabel,
          panelId: routeCommercePanelId(),
          inputId: routeWorkLaneInputId(normalizedLaneId),
          value: effectiveWorkOrderId,
          textareaId: routeWorkLaneTextareaId(normalizedLaneId),
          workOrderId: target.workOrderId || effectiveWorkOrderId,
        });
      };
      const buildWorldPurchaseLaneAction = (listingId, options) => {
        const target = options || {};
        const effectiveListingId = String(listingId || target.value || target.listingId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || 'Route listing',
          panelId: routeCommercePanelId(),
          inputId: routePurchaseInputId(),
          value: effectiveListingId,
          textareaId: routePurchaseTextareaId(),
          listingId: target.listingId || effectiveListingId,
        });
      };
      const buildWorldContractLaneAction = (contractId, options) => {
        const target = options || {};
        const effectiveContractId = String(contractId || target.value || target.contractId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || 'Route contract',
          panelId: routeContractsPanelId(),
          inputId: routeContractInputId(),
          value: effectiveContractId,
          textareaId: routeContractTextareaId(),
          contractId: target.contractId || effectiveContractId,
        });
      };
      const buildDraftWorldAction = (locationId, taskId, body) => buildSimpleRouteTargetAction({
        label: 'Draft world action',
        panelId: routeActionPanelId(),
        textareaId: routeActionTextareaId(),
        locationId: locationId || '',
        taskId: taskId || '',
        body: body || '',
      });
      const buildTaskFollowUpAction = (selection, taskId, locationId, detailText) => buildSimpleRouteTargetAction({
        label: 'Draft task follow-up',
        panelId: routeActionPanelId(),
        textareaId: routeActionTextareaId(),
        locationId: locationId || '',
        taskId: taskId || '',
        body: buildTaskFollowUpDraftBody(selection, taskId, detailText),
      });
"#
}

pub(super) fn real_world_map_marker_action_handoff_js() -> &'static str {
    r#"      const resolveMarkerPrimaryAction = (marker, nodeId, actionId) => {
        const source = marker || {};
        const resolvedNodeId = String(source.node_id || nodeId || '').trim();
        return (source.primary_actions || []).find((item) => item.action_id === actionId) || {
          action_id: actionId || 'move_here',
          command: '/go ' + resolvedNodeId,
          label: 'Move here',
          web_panel_id: routeMapMovePanelId(),
          web_move_target: resolvedNodeId,
        };
      };
      const buildMarkerActionHandoff = (marker, nodeId, actionId) => {
        const source = marker || {};
        const resolvedNodeId = String(source.node_id || nodeId || '').trim();
        const action = resolveMarkerPrimaryAction(source, resolvedNodeId, actionId);
        return {
          marker: source,
          action,
          handoff: buildRouteHandoffRecord({
            node_id: resolvedNodeId || nodeId || null,
            location_id: source.location_id || null,
            action_id: action.action_id || actionId || 'move_here',
            action_label: action.label || action.action_id || 'Action',
            command: action.command || ('/go ' + resolvedNodeId),
            web_panel_id: action.web_panel_id || routeActionPanelId(),
            web_action_body: action.web_action_body || '',
            web_move_target: action.web_move_target || resolvedNodeId || null,
          }),
        };
      };
"#
}

pub(super) fn real_world_map_route_action_js() -> String {
    format!(
        "{}{}{}{}",
        real_world_map_route_flow_buttons_js(),
        real_world_map_route_contract_accessors_js(),
        real_world_map_route_target_builders_js(),
        real_world_map_marker_action_handoff_js(),
    )
}

pub(super) fn real_world_map_viewport_hydration_js() -> &'static str {
    r#"      const buildViewportUrl = (mapCenter, zoom) => {
        return viewportTemplate
          .replace('{{lat}}', mapCenter.lat.toFixed(6))
          .replace('{{lng}}', mapCenter.lng.toFixed(6))
          .replace('{{zoom}}', String(zoom))
          .replace('{{radius_km}}', zoom >= 14 ? '4.5' : (zoom >= 10 ? '22.0' : '120.0'))
          .replace('{{limit}}', zoom >= 14 ? '6' : '4');
      };
      const applyViewportSnapshot = (viewport, mapCenter, zoom) => {
        lastViewport = viewport;
        if (densitySummary) {
          densitySummary.textContent = ((viewport.player_density || {}).summary) || 'Map density booting…';
        }
        if (cameraSummary) {
          cameraSummary.textContent = 'Camera ' + mapCenter.lat.toFixed(4) + ', ' + mapCenter.lng.toFixed(4) + ' · zoom ' + zoom + ' · ' + (viewport.lod_mode || 'street_nodes') + ' · ' + (viewport.marker_count || 0) + ' visible nodes · ' + (((viewport.player_density || {}).mode) || 'dense') + ' density · ' + (viewport.live_event_count || 0) + ' live events';
        }
        renderStreamHud(viewport, lastSelection);
        renderCards(tileTarget, viewport.visible_tile_shards || [], 'tile');
        renderCards(regionTarget, viewport.stream_region_shards || [viewport.active_region || {}], 'region');
        renderCards(poiTarget, viewport.poi_hotspots || [], 'poi');
        renderCards(prefetchTarget, viewport.prefetch_queue || [], 'prefetch');
        renderCards(liveEventTarget, filterLiveEventStream(viewport.live_event_stream || [], lastSelection), 'event');
        renderViewportOverlays(viewport);
        refreshOverlayControls();
        renderOverlayStatus();
      };
      const fetchViewportSnapshot = async () => {
        const mapCenter = mapAdapter.getCenter(mapRuntime);
        const zoom = mapAdapter.getZoom(mapRuntime);
        const response = await fetch(buildViewportUrl(mapCenter, zoom), { credentials: 'same-origin' });
        if (!response.ok) return null;
        const viewport = await response.json();
        applyViewportSnapshot(viewport, mapCenter, zoom);
        return viewport;
      };
"#
}
