# Trillionnium Real-world Map Engine Evaluation v1

## Decision

**Keep `leaflet_openstreetmap_v1` as the live engine for the current map-first product slices.**

Do **not** jump straight to a MapLibre rewrite yet.

The next execution step should be:

1. keep Leaflet + OpenStreetMap in production,
2. extract an engine-adapter seam so `/app` and `/world` stop hardcoding renderer details twice,
3. only introduce `maplibre_gl_v1` after the product actually needs vector/WebGL behavior.

## Current Repo Reality

The current implementation is explicitly Leaflet + OpenStreetMap:

- `services/consumer-entry-api/src/lib.rs`
  - `real_world_map_engine_json(...)` emits:
    - `engine_id = leaflet_openstreetmap_v1`
    - `engine = Leaflet`
    - `tile_provider = OpenStreetMap`
    - `tile_url_template = https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png`
  - `/app` shell loads Leaflet CSS/JS and builds the map with:
    - `L.map(...)`
    - `L.tileLayer(...)`
    - `L.layerGroup()`
    - `L.marker(...)`
    - `L.circleMarker(...)`
    - `L.polyline(...)`
  - `/world` shell mirrors the same Leaflet pattern.

So the current engine is not abstract yet; it is a concrete Leaflet renderer embedded twice inside the web shell output.

## Why Leaflet Is Still The Right Live Choice Now

Leaflet is still a good fit for the current shape of Trillionnium World because the product is presently:

- **map-first, but not yet vector-tile-first**
- **interaction-heavy**, not shader-heavy
- **overlay-driven**, with POIs / live events / route focus / panels as the main value
- **fast-moving in product shape**, where iteration speed matters more than graphical sophistication

Leaflet currently matches the shipped product well:

- OSM raster base is already working.
- Current overlays are simple and legible.
- Focus / event / route / handoff behavior is already verified in web E2E.
- DOM/SVG-style layers are easy to patch quickly while the product is still changing every day.
- Migration cost would be real because `/app` and `/world` both currently encode Leaflet behavior directly.

## Where Leaflet Will Start To Hurt

Leaflet will become the bottleneck when one or more of these become core requirements:

1. **hundreds to thousands of simultaneous visible overlays**
2. **smooth continuous animation of dense event fields**
3. **vector-tile restyling by zoom / faction / ownership / world mode**
4. **camera pitch / bearing / 3D-ish scene language**
5. **GPU-first rendering for routes, pulses, heat, territorial fills**
6. **large-scale clustering / decluttering that must stay fluid on mobile**

That is the point where MapLibre GL starts paying for itself.

## Why Not Migrate To MapLibre Right Now

A direct migration today would mostly buy **technical churn**, not immediate product leverage.

Main reasons:

- Current base map is raster OSM, not a vector style pipeline.
- Current UX value comes from route/event/focus coupling, not from tilt/bearing/fancy cartography.
- The bigger immediate code smell is **duplicated renderer logic between `/app` and `/world`**, not Leaflet itself.
- Moving to MapLibre before extracting a renderer seam would force a messy double rewrite.

So the wrong order is:

- rewrite engine first,
- then clean structure later.

The right order is:

- clean structure first,
- then swap/add engines when the product pressure justifies it.

## Recommended Architecture Move

### Phase 1 — do now

Keep `leaflet_openstreetmap_v1`, but refactor the browser-side renderer around a shared adapter contract.

Target contract:

- `createMap(target, engineConfig)`
- `setBaseLayer(map, engineConfig)`
- `renderRegions(map, regions)`
- `renderTiles(map, tiles)`
- `renderPrefetch(map, queue)`
- `renderPois(map, markers)`
- `renderEvents(map, events)`
- `renderRoutes(map, edges)`
- `focus(map, focusPayload)`
- `setOverlayVisibility(map, key, enabled)`
- `destroy(map)`

Goal: `/app` and `/world` share one rendering model while staying on Leaflet.

### Phase 2 — prepare but do not switch by default

Add a second planned engine id:

- `maplibre_gl_v1`

But keep it behind an explicit feature flag / experimental route until it proves real value.

### Phase 3 — switch only on trigger

Promote MapLibre when at least one of these becomes true:

- world viewport regularly needs **300+ visible interactive objects**,
- event overlays become **continuously animated** instead of sparse pulses,
- product needs **vector style layers** or **territory fills**,
- mobile interaction quality on Leaflet becomes a measurable blocker.

## Concrete Recommendation

**Recommendation: stay on Leaflet now, refactor toward an engine adapter next.**

This gives the best order of operations:

- preserve the already-working map-first product,
- reduce duplicated `/app` + `/world` rendering code,
- create a clean seam for a future MapLibre engine,
- avoid paying a WebGL migration tax before the product needs it.

## Next Implementable Slice

The next code slice should be:

1. extract duplicated `/app` + `/world` Leaflet rendering helpers into shared Rust-generated JS blocks or shared template helpers,
2. centralize overlay/layer/focus/event rendering behind engine-neutral function names,
3. keep `leaflet_openstreetmap_v1` as the active engine id,
4. optionally add a placeholder `planned_upgrade_engine = maplibre_gl_v1` metadata field later, once the adapter seam exists.

That is the lowest-risk path that still moves the architecture forward.

## Progress Note — 2026-04-28

The first adapter seam is now present while Leaflet remains active:

- `real_world_map_engine_json(...)` declares `renderer_adapter.adapter_id = leaflet_renderer_adapter_v1` and `planned_upgrade_engine.engine_id = maplibre_gl_v1`.
- `/app` and `/world` both emit the shared `createRealWorldMapAdapter()` browser runtime.
- The shared adapter now owns map creation, base layer setup, overlay layer creation/visibility, overlay clearing, route line rendering, POI marker rendering, density/region/tile/event overlay primitives, camera center/zoom reads, viewport-change event binding, focus, fit bounds, and invalidate-size calls. Browser shell code now treats the map handle as the engine-neutral `mapRuntime` instead of a Leaflet-named variable.

This does **not** switch engines yet; it just moves the current Leaflet runtime behind a named adapter seam so a future MapLibre path has a cleaner insertion point.

The adapter contract metadata is now generated from a shared backend helper rather than being hand-shaped inline in the engine payload. The payload includes `adapter_contract.supports_future_engine_swap = true` and a `planned_upgrade_engine.gating_contract`, so future MapLibre experiments have a clear condition to key off without changing the current active engine.

Matrix/mobile fallback cards now carry the same seam: `/app`, `/world`, and `/map` expose `map_renderer_adapter_id=leaflet_renderer_adapter_v1`, `map_runtime_handle_name=mapRuntime`, `map_renderer_future_engine_candidate=maplibre_gl_v1`, `map_renderer_supports_future_engine_swap=true`, and planned upgrade gate metadata. This keeps chat/mobile consumers aligned with the web runtime contract instead of binding them to Leaflet-specific names.

## Companion Reference

For a broader layered comparison beyond just the map renderer — including world modeling, social/realtime backend, and future 3D geospatial references — see:

- `docs/trillionnium-open-source-stack-reference-v1.md`

That companion note makes the current repo stance explicit:

- **Leaflet** stays live now
- **MapLibre** is the first future engine to prepare for
- **Bevy** is the most relevant architectural reference for ECS-like world state modeling
- **Nakama** is a later backend reference for realtime/presence/social loops
- **CesiumJS** is only for a future 3D geospatial world mode, not the current web shell
