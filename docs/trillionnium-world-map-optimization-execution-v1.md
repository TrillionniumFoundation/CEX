# Trillionnium World Map Optimization Execution v1

This note records the next full P0/P1/P2 push after the 100% beta/commercial gates. The map was already green; this phase is about making the green gates more product-real.

## P0 — truthful product loop + performance headroom

- Treat raw world/league counts as evidence, not direct cohort truth.
- Expose bounded cohort rates for route started → proof → reward → next route so percentages cannot exceed 100%.
- Keep raw counts visible for diagnosis, but require a denominator/integrity contract before product decisions.
- Make reward → next-route blockers first-class: no next route after reward, CTA not salient, reward unclear, proof/review friction, or insufficient sample.
- Add a runtime performance budget contract: first map interactive target, viewport refresh p95, focus-to-action rail latency, long-task budget, low-end FPS floor, delta update requirement, and explicit degradation when avatar runners hit budget.

## P1 — parity, commercial routing, shadow renderer

- `/app` and `/world` must both expose the same map readability / LOD / semantic-layer contract.
- Route recommendations should prefer completion quality and lower dispute risk, not just geography or object density.
- MapLibre remains shadow-only until product telemetry and WebGL pressure justify promotion.
- The active user-facing renderer remains Leaflet/OpenStreetMap.

## P2 — map as a subsystem

The map should be operated as a product subsystem, not as one page component:

1. `WorldMapDomain`: persisted world objects, routes, events, commerce risk.
2. `WorldMapProjection`: viewport payload, LOD, semantic roles, route-first copy.
3. `WorldMapTransport`: snapshot/delta/cache contracts.
4. `WorldMapRendererContract`: Leaflet live, MapLibre shadow candidate, rollback.
5. `WorldMapTelemetry`: funnel integrity, performance, CTA, and retention events.

Success for this phase is not “more markers”; it is a lighter, more truthful, more retention-aware map loop.

## Implemented gate surface

- `trillionnium_route_runner_funnel_integrity_v1`: separates raw event counts from bounded cohort decision rates, requires denominator consistency, and surfaces reward → next-route blockers.
- `trillionnium_world_map_runtime_performance_budget_v1`: hardens first-interactive, viewport refresh, focus-to-action, FPS, long-task, and delta-update budgets.
- `trillionnium_world_map_first_screen_decision_v1`: makes the first screen route-first: current route, next action, reward/XP, one primary CTA, dense detail collapsed.
- `trillionnium_world_route_recommendation_policy_v1`: folds completion quality, dispute risk, and reward-to-next-route lift into recommendation readiness.
- `trillionnium_world_map_renderer_shadow_v1`: keeps MapLibre as a shadow renderer with parity checks and promotion blockers while Leaflet/OpenStreetMap stays live.
- `trillionnium_world_map_subsystem_v1` + `trillionnium_world_map_transport_delta_v1`: define domain/projection/transport/renderer/telemetry boundaries, region deltas, presence deltas, snapshot fallback, and payload budgets.

Latest validation from this push: playability scorecard, web E2E, UI audit, browser E2E, real-user beta, public-commercial, consumer/matrix/ledger tests, syntax checks, and diff whitespace checks all passed under `CEX_ENV_FILE=run/local-production/.env`.
