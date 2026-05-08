import { createRequire } from 'node:module';
import fs from 'node:fs/promises';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const baseUrl = (process.env.CONSUMER_ENTRY_BASE_URL || process.env.BASE_URL || 'http://127.0.0.1:8090').replace(/\/$/, '');
const rootDir = process.env.CEX_PROJECT_ROOT || process.cwd();
const outDir = process.env.TRILLIONNIUM_UI_AUDIT_OUT_DIR || path.join(rootDir, 'run', 'trillionnium-ui-audit');
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || process.env.CHROME_BIN || '/usr/bin/google-chrome-stable';
const runId = `${Math.floor(Date.now() / 1000)}-${process.pid}`;
const summaryPath = path.join(outDir, `ui-audit-summary-${runId}.json`);

const profiles = [
  {
    name: 'mobile',
    context: { viewport: { width: 390, height: 844 }, isMobile: true, deviceScaleFactor: 3 },
    limits: {
      app: { maxScrollH: 5220, maxMapY: 700, maxRouteY: 900, maxActionY: 1300, maxOnboardingY: 1900 },
      world: { maxScrollH: 12500, maxMapY: 900, maxPulseY: 1200, maxActionY: 1700 },
      league: { maxScrollH: 4300, maxStatsY: 750, maxModesY: 1100, maxConsoleY: 1400 },
    },
  },
  {
    name: 'tablet',
    context: { viewport: { width: 768, height: 1024 }, isMobile: true, deviceScaleFactor: 2 },
    limits: {
      app: { maxScrollH: 5400, maxMapY: 700, maxRouteY: 1000, maxActionY: 1400, maxOnboardingY: 2000 },
      world: { maxScrollH: 12500, maxMapY: 850, maxPulseY: 1500, maxActionY: 2000 },
      league: { maxScrollH: 3900, maxStatsY: 650, maxModesY: 950, maxConsoleY: 1300 },
    },
  },
  {
    name: 'desktop',
    context: { viewport: { width: 1280, height: 800 } },
    limits: {
      app: { maxScrollH: 6200, maxMapY: 900, maxRouteY: 1100, maxOnboardingY: 2400 },
      world: { maxScrollH: 12500, maxMapY: 1200, maxActionY: 1800, maxActionH: 700 },
      league: { maxScrollH: 3400, maxModesY: 1300, maxConsoleY: 1600, maxConsoleH: 900 },
    },
  },
];

const targets = [
  { name: 'app', route: '/app?lang=en' },
  { name: 'world', route: '/world?lang=en' },
  { name: 'league', route: '/league?lang=en' },
];

const selectorSets = {
  app: {
    header: 'header',
    tabs: '.app-bottom-tabs',
    stickyStatus: '.app-map-product-strip',
    mobileSheet: '#app-mobile-action-sheet',
    map: '#real-world-map',
    route: '#app-map-route-panel',
    routeHandoff: '#app-route-runner-handoff-summary',
    feedHandoff: '#app-feed-route-runner-handoff',
    action: '#app-map-action-panel',
    onboarding: '#app-first-playable-onboarding',
    denseCopy: '#app-tab-map .map-panel',
    tiles: '#app-tile-shards-live',
  },
  world: {
    header: 'header',
    map: '#world-real-map',
    routeHandoff: '#world-route-runner-handoff-summary',
    pulse: '#world-pulse-strip',
    action: '#world-action-console',
    contracts: '#world-contracts-panel',
    mapCopy: '.map-copy',
    commands: 'section.panel:last-of-type',
  },
  league: {
    header: 'header',
    stats: '.stats',
    modes: '#league-playable-modes',
    console: '#league-battle-console',
    progression: '#league-progression',
    worldBridge: '#league-world-bridge',
    guilds: 'section.panel:has(h2[data-i18n-en="Guild Halls"])',
    leaderboard: 'table',
    commands: 'section.panel:has(h2[data-i18n-en="Playable Commands"])',
  },
};

function assertMetric(condition, message, details = undefined) {
  if (!condition) {
    const error = new Error(message);
    if (details !== undefined) error.details = details;
    throw error;
  }
}

function yOf(result, key) {
  return result.key?.[key]?.y;
}

function hOf(result, key) {
  return result.key?.[key]?.h;
}

function checkCommon(result) {
  assertMetric(result.visibleCjk.length === 0, `${result.profile}/${result.name} visible English UI leaks CJK`, result.visibleCjk.slice(0, 10));
  assertMetric(result.actionableOverflow.length === 0, `${result.profile}/${result.name} has actionable horizontal overflow`, result.actionableOverflow.slice(0, 10));
  assertMetric(result.coveredActionables.length === 0, `${result.profile}/${result.name} has covered first-viewport actionables`, result.coveredActionables.slice(0, 10));
  assertMetric(result.smallTouchTargets.length === 0, `${result.profile}/${result.name} has small first-viewport touch targets`, result.smallTouchTargets.slice(0, 10));
}

function isAllowedRouteRunnerNextRouteStatus(status) {
  return status === 'next_route_preview_locked_until_reward_claim' || status === 'next_route_ready_after_reward_claim';
}

function checkRouteRunnerHandoff(result) {
  if (!['app', 'world'].includes(result.name)) return;
  const handoff = result.routeRunnerHandoff || {};
  const summaryRequired =
    result.name === 'app'
      ? handoff.appRouteSummaryPresent && handoff.appFeedSummaryPresent
      : handoff.worldSummaryPresent;
  assertMetric(summaryRequired, `${result.profile}/${result.name} route-runner handoff summaries missing`, handoff);
  assertMetric(handoff.contractVersionPresent === true, `${result.profile}/${result.name} route-runner handoff contract missing`, handoff);
  assertMetric(handoff.sourcePresent === true, `${result.profile}/${result.name} route-runner handoff feed source missing`, handoff);
  assertMetric(isAllowedRouteRunnerNextRouteStatus(handoff.nextRouteStatus), `${result.profile}/${result.name} route-runner next-route status missing`, handoff);
  assertMetric(Number(handoff.runnerCount || 0) >= 1, `${result.profile}/${result.name} route-runner count missing`, handoff);
  assertMetric(Number(handoff.rewardClaimCount || 0) >= 1, `${result.profile}/${result.name} route-runner reward-claim count missing`, handoff);
  assertMetric(Number(handoff.nextRouteCount || 0) >= 1, `${result.profile}/${result.name} route-runner next-route count missing`, handoff);
  assertMetric(handoff.hasRewardCopy === true && handoff.hasNextRouteCopy === true, `${result.profile}/${result.name} route-runner handoff copy missing`, handoff);
}

function checkMobile(result, limits) {
  assertMetric(result.scrollH <= limits.maxScrollH, `${result.profile}/${result.name} scroll height regressed`, { scrollH: result.scrollH, limit: limits.maxScrollH });
  if (result.name === 'app') {
    const cta = result.mobilePrimaryCta || {};
    assertMetric(cta.sheetPresent === true, `${result.profile}/app mobile bottom action sheet missing`, cta);
    assertMetric(cta.contractVersion === 'trillionnium_mobile_single_primary_cta_v1', `${result.profile}/app mobile primary CTA contract missing`, cta);
    assertMetric(cta.bottomSheetMode === 'fixed_above_bottom_tabs_on_mobile', `${result.profile}/app mobile bottom sheet mode missing`, cta);
    assertMetric(Number(cta.primaryCtaCount || 0) === 1, `${result.profile}/app must expose exactly one mobile primary CTA`, cta);
    assertMetric(cta.primaryCtaTarget === 'app-map-action-rail', `${result.profile}/app mobile primary CTA must target the next action rail`, cta);
    assertMetric(cta.primaryCtaVisible === true, `${result.profile}/app mobile primary CTA must be visible`, cta);
    if (result.profile === 'mobile') {
      assertMetric(cta.sheetBottom <= result.viewport.vh + 2, `${result.profile}/app mobile bottom sheet must stay in viewport`, cta);
      assertMetric(cta.sheetY >= Math.floor(result.viewport.vh * 0.58), `${result.profile}/app mobile bottom sheet must stay docked near bottom`, cta);
    }
    assertMetric((cta.text || '').includes('Continue Route'), `${result.profile}/app mobile primary CTA copy missing`, cta);
    assertMetric((cta.text || '').includes('runner'), `${result.profile}/app mobile bottom sheet must keep runner context`, cta);
    assertMetric((cta.text || '').includes('next-route'), `${result.profile}/app mobile bottom sheet must keep next-route context`, cta);
    assertMetric((cta.text || '').includes('reward'), `${result.profile}/app mobile bottom sheet must keep reward context`, cta);
    const copy = result.mobileCopyLayering || {};
    assertMetric(copy.summaryPresent === true, `${result.profile}/app mobile copy summary missing`, copy);
    assertMetric(copy.summaryContractVersion === 'trillionnium_mobile_copy_layering_v1', `${result.profile}/app mobile copy layering contract missing`, copy);
    assertMetric(copy.detailsPresent === true, `${result.profile}/app mobile copy details missing`, copy);
    assertMetric(copy.detailsContractVersion === 'trillionnium_mobile_copy_layering_v1', `${result.profile}/app mobile copy details contract missing`, copy);
    assertMetric(copy.defaultState === 'collapsed' && copy.detailsOpen === false, `${result.profile}/app mobile copy details must default collapsed`, copy);
    assertMetric(Number(copy.summaryLength || 0) > 0 && Number(copy.summaryLength || 0) <= 150, `${result.profile}/app mobile copy summary is too dense`, copy);
    assertMetric((copy.summaryText || '').includes('Pick a nearby route'), `${result.profile}/app mobile copy summary must be action-first`, copy);
    const readability = result.mapReadabilityLod || {};
    assertMetric(readability.present === true, `${result.profile}/app map readability LOD contract missing`, readability);
    assertMetric(readability.contractVersion === 'trillionnium_world_map_readability_lod_v1', `${result.profile}/app map readability LOD contract version missing`, readability);
    assertMetric(readability.semanticLayerContract === 'trillionnium_world_map_game_layer_semantics_v1', `${result.profile}/app map semantic layer contract missing`, readability);
    assertMetric(readability.firstScreenMode === 'route_first_street_detail', `${result.profile}/app map readability first-screen mode missing`, readability);
    assertMetric(Number(readability.primaryCtaBudget || 0) === 1, `${result.profile}/app map readability must keep one primary CTA`, readability);
    assertMetric(Number(readability.visibleMarkerBudget || 0) <= 18, `${result.profile}/app visible marker clutter budget drifted`, readability);
    assertMetric(Number(readability.avatarRunnerBudget || 0) <= 6, `${result.profile}/app avatar runner clutter budget drifted`, readability);
    assertMetric(Number(readability.copySummaryBudget || 0) <= 150, `${result.profile}/app copy budget drifted`, readability);
    assertMetric(readability.detailsDefaultState === 'collapsed', `${result.profile}/app dense map details must default collapsed`, readability);
    assertMetric(String(readability.semanticLegendRequired) === 'true' && String(readability.avatarFeedbackRequired) === 'true' && String(readability.i18nA11yRequired) === 'true', `${result.profile}/app P2 semantic/avatar/i18n-a11y polish contract missing`, readability);
    assertMetric((readability.text || '').includes('One route first'), `${result.profile}/app readability copy must stay route-first`, readability);
    const perf = result.mapPerformanceBudget || {};
    assertMetric(perf.present === true, `${result.profile}/app map performance budget missing`, perf);
    assertMetric(perf.contractVersion === 'trillionnium_world_map_runtime_performance_budget_v1', `${result.profile}/app map performance budget contract missing`, perf);
    assertMetric(Number(perf.firstMapInteractiveTargetMs || 0) <= 2000, `${result.profile}/app first map interactive target drifted`, perf);
    assertMetric(Number(perf.viewportRefreshP95TargetMs || 0) <= 250, `${result.profile}/app viewport refresh budget drifted`, perf);
    assertMetric(Number(perf.focusToActionRailTargetMs || 0) <= 300, `${result.profile}/app focus-to-action budget drifted`, perf);
    assertMetric(String(perf.deltaViewportUpdatesRequired) === 'true', `${result.profile}/app delta viewport requirement missing`, perf);
    assertMetric(String(perf.abortPreviousViewportRequest) === 'true', `${result.profile}/app stale viewport abort requirement missing`, perf);
    assertMetric(String(perf.deferNoncriticalCardRender) === 'true', `${result.profile}/app deferred card render requirement missing`, perf);
    assertMetric(String(perf.clusterMarkersBeforeHiding) === 'true', `${result.profile}/app marker cluster density policy missing`, perf);
    assertMetric(String(perf.spatialCacheRequired) === 'true' && String(perf.virtualizedCardsRequired) === 'true' && String(perf.adaptiveDensityRequired) === 'true', `${result.profile}/app P1 density scalability DOM contract missing`, perf);
    const transport = result.mapTransportDelta || {};
    assertMetric(transport.present === true, `${result.profile}/app map transport delta contract missing`, transport);
    assertMetric(transport.contractVersion === 'trillionnium_world_map_transport_delta_v1', `${result.profile}/app map transport delta version missing`, transport);
    assertMetric(transport.subsystemContract === 'trillionnium_world_map_subsystem_v1', `${result.profile}/app map subsystem contract missing`, transport);
    assertMetric(String(transport.presenceDeltaRequired) === 'true', `${result.profile}/app presence delta requirement missing`, transport);
    assertMetric(String(transport.snapshotFallbackRequired) === 'true', `${result.profile}/app snapshot fallback requirement missing`, transport);
    assertMetric(String(transport.changedGroupRenderingRequired) === 'true', `${result.profile}/app changed-group rendering requirement missing`, transport);
    assertMetric(String(transport.visibleMarkerDeltaRequired) === 'true', `${result.profile}/app visible-marker delta requirement missing`, transport);
    assertMetric(String(transport.markerClusterDeltaRequired) === 'true', `${result.profile}/app marker-cluster delta requirement missing`, transport);
    const rumSlo = result.mapRumSlo || {};
    assertMetric(rumSlo.present === true, `${result.profile}/app map RUM SLO contract missing`, rumSlo);
    assertMetric(rumSlo.contractVersion === 'trillionnium_world_map_rum_slo_v1', `${result.profile}/app map RUM SLO contract version missing`, rumSlo);
    assertMetric((rumSlo.quantiles || '').includes('p50') && (rumSlo.quantiles || '').includes('p95') && (rumSlo.quantiles || '').includes('p99'), `${result.profile}/app RUM SLO quantiles missing`, rumSlo);
    assertMetric((rumSlo.surfaceSplit || '').includes('app') && (rumSlo.surfaceSplit || '').includes('world'), `${result.profile}/app RUM SLO surface split missing`, rumSlo);
    assertMetric((rumSlo.deviceSplit || '').includes('mobile') && (rumSlo.deviceSplit || '').includes('desktop'), `${result.profile}/app RUM SLO device split missing`, rumSlo);
    assertMetric(rumSlo.matrixContract === 'trillionnium_world_map_real_user_rum_matrix_v1' && (rumSlo.sampleKinds || '').includes('cold_cache_interactive') && (rumSlo.sampleKinds || '').includes('weak_network_cached_snapshot') && Number(rumSlo.perBucketMinSamples || 0) >= 1, `${result.profile}/app real-user RUM matrix missing`, rumSlo);
    const weakNetwork = result.mapWeakNetwork || {};
    assertMetric(weakNetwork.present === true, `${result.profile}/app weak-network contract missing`, weakNetwork);
    assertMetric(weakNetwork.contractVersion === 'trillionnium_world_map_weak_network_resilience_v1', `${result.profile}/app weak-network contract version missing`, weakNetwork);
    assertMetric(weakNetwork.cacheKey === 'trillionnium-world-map:last-good-viewport:v1', `${result.profile}/app weak-network cache key missing`, weakNetwork);
    assertMetric(String(weakNetwork.delta304Supported) === 'true', `${result.profile}/app weak-network 304 support missing`, weakNetwork);
    assertMetric(String(weakNetwork.offlineBannerRequired) === 'true' && String(weakNetwork.pendingActionQueueRequired) === 'true' && String(weakNetwork.conflictSyncRequired) === 'true', `${result.profile}/app weak-network productization contract missing`, weakNetwork);
    const privacy = result.mapLocationPrivacy || {};
    assertMetric(privacy.present === true, `${result.profile}/app location privacy contract missing`, privacy);
    assertMetric(privacy.contractVersion === 'trillionnium_world_map_location_privacy_v1', `${result.profile}/app location privacy contract version missing`, privacy);
    assertMetric(String(privacy.rumExcludesLatLng) === 'true', `${result.profile}/app RUM location privacy flag missing`, privacy);
    assertMetric(privacy.cacheControl === 'private', `${result.profile}/app personalized map cache must stay private`, privacy);
    if (result.profile === 'mobile') {
      const sheet = yOf(result, 'mobileSheet');
      assertMetric(sheet >= Math.floor(result.viewport.vh * 0.58), `${result.profile}/app mobile bottom sheet key selector is not docked`, { sheet, viewport: result.viewport });
    }
    const map = yOf(result, 'map');
    const route = yOf(result, 'route');
    const action = yOf(result, 'action');
    const onboarding = yOf(result, 'onboarding');
    const denseCopy = yOf(result, 'denseCopy');
    assertMetric(map < route && route < action && action < onboarding && onboarding < denseCopy, `${result.profile}/app must keep map → route → action → onboarding → dense copy order`, { map, route, action, onboarding, denseCopy });
    assertMetric(map <= limits.maxMapY && route <= limits.maxRouteY && action <= limits.maxActionY && onboarding <= limits.maxOnboardingY, `${result.profile}/app key modules are too deep`, { map, route, action, onboarding, limits });
  } else if (result.name === 'world') {
    const cta = result.worldMobilePrimaryCta || {};
    assertMetric(cta.sheetPresent === true, `${result.profile}/world mobile route-first sheet missing`, cta);
    assertMetric(cta.contractVersion === 'trillionnium_mobile_single_primary_cta_v1', `${result.profile}/world mobile primary CTA contract missing`, cta);
    assertMetric(cta.paritySource === 'app-mobile-primary-cta', `${result.profile}/world mobile CTA must declare /app parity`, cta);
    assertMetric(Number(cta.primaryCtaCount || 0) === 1, `${result.profile}/world must expose exactly one route-first primary CTA`, cta);
    assertMetric(cta.primaryCtaVisible === true, `${result.profile}/world mobile primary CTA must be visible`, cta);
    assertMetric(cta.firstScreenLoop === 'pick_route_submit_proof_claim_reward', `${result.profile}/world route-first loop contract missing`, cta);
    for (const token of ['Current route', 'Next action', 'Reward', 'Continue route', 'Pick route', 'Submit proof', 'Claim reward']) {
      assertMetric((cta.text || '').includes(token), `${result.profile}/world route-first mobile copy missing ${token}`, cta);
    }
    assertMetric(Number(cta.routeMasteryXp || 0) > 0 && Boolean(cta.routeMasteryTier), `${result.profile}/world route-first mobile reward/XP data missing`, cta);
    const readability = result.worldMapReadability || {};
    assertMetric(readability.present === true, `${result.profile}/world map readability LOD contract missing`, readability);
    assertMetric(readability.contractVersion === 'trillionnium_world_map_readability_lod_v1', `${result.profile}/world map readability LOD contract version missing`, readability);
    assertMetric(readability.semanticLayerContract === 'trillionnium_world_map_game_layer_semantics_v1', `${result.profile}/world map semantic layer contract missing`, readability);
    assertMetric(readability.paritySource === 'app-map-readability-lod', `${result.profile}/world map readability must declare /app parity`, readability);
    assertMetric(Number(readability.primaryCtaBudget || 0) === 1, `${result.profile}/world map readability must keep one primary CTA`, readability);
    assertMetric(Number(readability.visibleMarkerBudget || 0) <= 18, `${result.profile}/world visible marker clutter budget drifted`, readability);
    assertMetric(Number(readability.avatarRunnerBudget || 0) <= 6, `${result.profile}/world avatar runner clutter budget drifted`, readability);
    assertMetric(readability.detailsDefaultState === 'collapsed', `${result.profile}/world dense map details must default collapsed`, readability);
    assertMetric(String(readability.semanticLegendRequired) === 'true' && String(readability.avatarFeedbackRequired) === 'true' && String(readability.i18nA11yRequired) === 'true', `${result.profile}/world P2 semantic/avatar/i18n-a11y polish contract missing`, readability);
    const perf = result.mapPerformanceBudget || {};
    assertMetric(perf.present === true, `${result.profile}/world map performance budget missing`, perf);
    assertMetric(perf.contractVersion === 'trillionnium_world_map_runtime_performance_budget_v1', `${result.profile}/world map performance budget contract missing`, perf);
    assertMetric(Number(perf.focusToActionRailTargetMs || 0) <= 300, `${result.profile}/world focus-to-action budget drifted`, perf);
    assertMetric(String(perf.deltaViewportUpdatesRequired) === 'true', `${result.profile}/world delta viewport requirement missing`, perf);
    assertMetric(String(perf.abortPreviousViewportRequest) === 'true', `${result.profile}/world stale viewport abort requirement missing`, perf);
    assertMetric(String(perf.deferNoncriticalCardRender) === 'true', `${result.profile}/world deferred card render requirement missing`, perf);
    assertMetric(String(perf.spatialCacheRequired) === 'true' && String(perf.virtualizedCardsRequired) === 'true' && String(perf.adaptiveDensityRequired) === 'true', `${result.profile}/world P1 density scalability DOM contract missing`, perf);
    assertMetric(String(perf.clusterMarkersBeforeHiding) === 'true', `${result.profile}/world marker cluster density policy missing`, perf);
    const transport = result.mapTransportDelta || {};
    assertMetric(transport.contractVersion === 'trillionnium_world_map_transport_delta_v1', `${result.profile}/world map transport delta version missing`, transport);
    assertMetric(transport.subsystemContract === 'trillionnium_world_map_subsystem_v1', `${result.profile}/world map subsystem contract missing`, transport);
    assertMetric(transport.paritySource === 'app-map-transport-delta', `${result.profile}/world map transport must declare /app parity`, transport);
    assertMetric(String(transport.presenceDeltaRequired) === 'true', `${result.profile}/world presence delta requirement missing`, transport);
    assertMetric(String(transport.changedGroupRenderingRequired) === 'true', `${result.profile}/world changed-group rendering requirement missing`, transport);
    assertMetric(String(transport.visibleMarkerDeltaRequired) === 'true', `${result.profile}/world visible-marker delta requirement missing`, transport);
    assertMetric(String(transport.markerClusterDeltaRequired) === 'true', `${result.profile}/world marker-cluster delta requirement missing`, transport);
    const rumSlo = result.mapRumSlo || {};
    assertMetric(rumSlo.contractVersion === 'trillionnium_world_map_rum_slo_v1', `${result.profile}/world map RUM SLO contract version missing`, rumSlo);
    assertMetric(rumSlo.paritySource === 'app-map-rum-slo', `${result.profile}/world map RUM SLO must declare /app parity`, rumSlo);
    assertMetric((rumSlo.quantiles || '').includes('p95') && (rumSlo.deviceSplit || '').includes('mobile'), `${result.profile}/world map RUM SLO dimensions missing`, rumSlo);
    assertMetric(rumSlo.matrixContract === 'trillionnium_world_map_real_user_rum_matrix_v1' && Number(rumSlo.perBucketMinSamples || 0) >= 1, `${result.profile}/world real-user RUM matrix missing`, rumSlo);
    const weakNetwork = result.mapWeakNetwork || {};
    assertMetric(weakNetwork.contractVersion === 'trillionnium_world_map_weak_network_resilience_v1', `${result.profile}/world weak-network contract version missing`, weakNetwork);
    assertMetric(weakNetwork.paritySource === 'app-map-weak-network', `${result.profile}/world weak-network must declare /app parity`, weakNetwork);
    assertMetric(weakNetwork.cacheKey === 'trillionnium-world-map:last-good-viewport:v1' && String(weakNetwork.delta304Supported) === 'true', `${result.profile}/world weak-network cache/304 missing`, weakNetwork);
    assertMetric(String(weakNetwork.offlineBannerRequired) === 'true' && String(weakNetwork.pendingActionQueueRequired) === 'true' && String(weakNetwork.conflictSyncRequired) === 'true', `${result.profile}/world weak-network productization contract missing`, weakNetwork);
    const privacy = result.mapLocationPrivacy || {};
    assertMetric(privacy.contractVersion === 'trillionnium_world_map_location_privacy_v1', `${result.profile}/world location privacy contract version missing`, privacy);
    assertMetric(privacy.paritySource === 'app-map-location-privacy', `${result.profile}/world privacy must declare /app parity`, privacy);
    assertMetric(String(privacy.rumExcludesLatLng) === 'true' && privacy.cacheControl === 'private', `${result.profile}/world privacy cache/RUM flags missing`, privacy);
    const shadow = result.shadowRenderer || {};
    assertMetric(shadow.contractVersion === 'trillionnium_world_map_renderer_shadow_v1', `${result.profile}/world shadow renderer contract missing`, shadow);
    assertMetric(shadow.parityContract === 'trillionnium_world_map_maplibre_shadow_parity_v1' && String(shadow.rollbackDrillRequired) === 'true' && Number(shadow.canaryPercent || -1) === 0, `${result.profile}/world MapLibre shadow parity/canary/rollback contract missing`, shadow);
    assertMetric(shadow.activeEngine === 'leaflet_openstreetmap_v1' && shadow.shadowEngine === 'maplibre_gl_v1', `${result.profile}/world shadow renderer engine ids missing`, shadow);
    assertMetric(shadow.status === 'shadow_only_not_user_facing', `${result.profile}/world MapLibre must stay shadow-only`, shadow);
    const map = yOf(result, 'map');
    const pulse = yOf(result, 'pulse');
    const action = yOf(result, 'action');
    assertMetric(map < pulse && pulse < action, `${result.profile}/world must keep map → pulse → action order`, { map, pulse, action });
    assertMetric(map <= limits.maxMapY && pulse <= limits.maxPulseY && action <= limits.maxActionY, `${result.profile}/world key modules are too deep`, { map, pulse, action, limits });
  } else if (result.name === 'league') {
    const stats = yOf(result, 'stats');
    const modes = yOf(result, 'modes');
    const console = yOf(result, 'console');
    const progression = yOf(result, 'progression');
    assertMetric(stats < modes && modes < console && console < progression, `${result.profile}/league must keep stats → modes → console → progression order`, { stats, modes, console, progression });
    assertMetric(stats <= limits.maxStatsY && modes <= limits.maxModesY && console <= limits.maxConsoleY, `${result.profile}/league key modules are too deep`, { stats, modes, console, limits });
  }
}

function checkDesktop(result, limits) {
  assertMetric(result.scrollH <= limits.maxScrollH, `desktop/${result.name} scroll height regressed`, { scrollH: result.scrollH, limit: limits.maxScrollH });
  if (result.name === 'app') {
    const map = yOf(result, 'map');
    const route = yOf(result, 'route');
    const onboarding = yOf(result, 'onboarding');
    assertMetric(map < onboarding && route < onboarding, 'desktop/app must expose map and route before onboarding', { map, route, onboarding });
    assertMetric(map <= limits.maxMapY && route <= limits.maxRouteY && onboarding <= limits.maxOnboardingY, 'desktop/app key modules are too deep', { map, route, onboarding, limits });
  } else if (result.name === 'world') {
    const map = yOf(result, 'map');
    const action = yOf(result, 'action');
    const actionH = hOf(result, 'action');
    assertMetric(map < action, 'desktop/world must expose map before action console', { map, action });
    assertMetric(map <= limits.maxMapY && action <= limits.maxActionY && actionH <= limits.maxActionH, 'desktop/world action console is too deep or too tall', { map, action, actionH, limits });
  } else if (result.name === 'league') {
    const modes = yOf(result, 'modes');
    const console = yOf(result, 'console');
    const consoleH = hOf(result, 'console');
    assertMetric(modes < console, 'desktop/league must expose modes before battle console', { modes, console });
    assertMetric(modes <= limits.maxModesY && console <= limits.maxConsoleY && consoleH <= limits.maxConsoleH, 'desktop/league battle console is too deep or too tall', { modes, console, consoleH, limits });
  }
}

async function auditPage(page, profile, target) {
  const routeUrl = `${baseUrl}${target.route}`;
  await page.goto(routeUrl, { waitUntil: 'networkidle', timeout: 60_000 });
  await page.waitForTimeout(700);
  const screenshotPath = path.join(outDir, `${profile.name}-${target.name}-${runId}.png`);
  await page.screenshot({ path: screenshotPath, fullPage: false });
  const selectors = selectorSets[target.name];
  const data = await page.evaluate(({ profileName, targetName, selectors }) => {
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    const cjk = /[\u3400-\u9fff\uf900-\ufaff]/;
    const text = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
    const visible = (el) => {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      return r.width > 1 && r.height > 1 && r.bottom > 0 && r.top < document.documentElement.scrollHeight && cs.visibility !== 'hidden' && cs.display !== 'none' && !el.closest('[hidden]');
    };
    const isLeafletArtifact = (el) => {
      for (let current = el; current && current !== document.body; current = current.parentElement) {
        const cls = String(current.className || '');
        if (cls.includes('leaflet-')) return true;
      }
      return false;
    };
    const info = (sel) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return {
        selector: sel,
        x: Math.round(r.left),
        y: Math.round(r.top),
        right: Math.round(r.right),
        bottom: Math.round(r.bottom),
        w: Math.round(r.width),
        h: Math.round(r.height),
        txt: text(el).slice(0, 240),
      };
    };
    const visibleElements = Array.from(document.querySelectorAll('body *')).filter(visible);
    const isAuditedActionable = (el) => {
      if (!el.matches('button,a[href],input,select,textarea,[role="button"],summary')) return false;
      if (isLeafletArtifact(el)) return false;
      if (el.closest('[aria-hidden="true"],[inert]')) return false;
      if (el.disabled || el.getAttribute('aria-disabled') === 'true') return false;
      return true;
    };
    const actionableInfo = (el) => {
      const r = el.getBoundingClientRect();
      return {
        tag: el.tagName.toLowerCase(),
        id: el.id,
        cls: String(el.className || ''),
        x: Math.round(r.left),
        y: Math.round(r.top),
        right: Math.round(r.right),
        bottom: Math.round(r.bottom),
        w: Math.round(r.width),
        h: Math.round(r.height),
        txt: text(el).slice(0, 120) || el.getAttribute('aria-label') || el.getAttribute('placeholder') || '',
      };
    };
    const firstViewportActionables = visibleElements
      .filter(isAuditedActionable)
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.top < vh && r.bottom > 0;
      });
    const coveredActionables = firstViewportActionables
      .map((el) => {
        const r = el.getBoundingClientRect();
        const x = Math.min(Math.max(r.left + r.width / 2, 1), vw - 1);
        const y = Math.min(Math.max(r.top + r.height / 2, 1), vh - 1);
        const top = document.elementFromPoint(x, y);
        return { el, top, info: actionableInfo(el), topInfo: top ? actionableInfo(top) : null };
      })
      .filter(({ el, top }) => !top || (top !== el && !el.contains(top) && !top.contains(el)))
      .map(({ info, topInfo }) => ({ ...info, coveredBy: topInfo }))
      .slice(0, 30);
    const smallTouchTargets = firstViewportActionables
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.width < 40 || r.height < 40;
      })
      .map(actionableInfo)
      .slice(0, 30);
    const visibleCjk = visibleElements
      .filter((el) => !el.closest('.language-switcher'))
      .filter((el) => !['OPTION', 'SCRIPT', 'STYLE', 'NOSCRIPT'].includes(el.tagName))
      .filter((el) => cjk.test(text(el)))
      .map((el) => {
        const r = el.getBoundingClientRect();
        return { tag: el.tagName.toLowerCase(), id: el.id, cls: String(el.className || ''), y: Math.round(r.top), bottom: Math.round(r.bottom), txt: text(el).slice(0, 180) };
      })
      .slice(0, 30);
    const overflowRaw = visibleElements
      .map((el) => ({ el, r: el.getBoundingClientRect(), txt: text(el) }))
      .filter(({ r }) => r.right > vw + 1 || r.left < -1)
      .map(({ el, r, txt }) => ({ tag: el.tagName.toLowerCase(), id: el.id, cls: String(el.className || ''), x: Math.round(r.left), right: Math.round(r.right), y: Math.round(r.top), txt: txt.slice(0, 160), leaflet: isLeafletArtifact(el), svg: ['svg', 'g', 'path'].includes(el.tagName.toLowerCase()) }))
      .slice(0, 50);
    const actionableOverflow = overflowRaw.filter((item) => !item.leaflet && !item.svg);
    const key = {};
    for (const [keyName, selector] of Object.entries(selectors)) key[keyName] = info(selector);
    const firstViewportButtons = firstViewportActionables
      .map(actionableInfo)
      .slice(0, 40);
    const html = document.documentElement.innerHTML;
    const pickMaxCount = (elements, datasetKey) => Math.max(0, ...elements.map((el) => Number.parseInt(el?.dataset?.[datasetKey] || '0', 10) || 0));
    const appRouteHandoff = document.getElementById('app-route-runner-handoff-summary');
    const appFeedHandoff = document.getElementById('app-feed-route-runner-handoff');
    const worldRouteHandoff = document.getElementById('world-route-runner-handoff-summary');
    const appMapReadabilityLod = document.getElementById('app-map-readability-lod');
    const worldMapReadabilityLod = document.getElementById('world-map-readability-lod');
    const mapPerformanceBudgetElement = document.getElementById(targetName === 'world' ? 'world-map-performance-budget' : 'app-map-performance-budget');
    const mapTransportDeltaElement = document.getElementById(targetName === 'world' ? 'world-map-transport-delta' : 'app-map-transport-delta');
    const mapRumSloElement = document.getElementById(targetName === 'world' ? 'world-map-rum-slo' : 'app-map-rum-slo');
    const mapWeakNetworkElement = document.getElementById(targetName === 'world' ? 'world-map-weak-network' : 'app-map-weak-network');
    const mapLocationPrivacyElement = document.getElementById(targetName === 'world' ? 'world-map-location-privacy' : 'app-map-location-privacy');
    const worldMapShadowRenderer = document.getElementById('world-map-shadow-renderer');
    const handoffElements = [appRouteHandoff, appFeedHandoff, worldRouteHandoff].filter(Boolean);
    const handoffText = handoffElements.map((el) => text(el)).join(' · ');
    const mobileSheet = document.getElementById('app-mobile-action-sheet');
    const worldMobileSheet = document.getElementById('world-hero-mobile-actions');
    const worldRewardXp = document.getElementById('world-mobile-reward-xp');
    const copySummary = document.getElementById('app-map-copy-summary');
    const copyDetails = document.getElementById('app-map-copy-layer-details');
    const mobilePrimaryCtas = Array.from(document.querySelectorAll('#app-mobile-action-sheet [data-primary-cta]')).filter(visible);
    const primaryCta = mobilePrimaryCtas[0] || null;
    const sheetRect = mobileSheet?.getBoundingClientRect();
    const mobilePrimaryCta = {
      sheetPresent: Boolean(mobileSheet),
      contractVersion: mobileSheet?.dataset.contractVersion || null,
      bottomSheetMode: mobileSheet?.dataset.bottomSheetMode || null,
      declaredPrimaryCtaCount: mobileSheet?.dataset.primaryCtaCount || null,
      primaryCtaCount: mobilePrimaryCtas.length,
      primaryCtaVisible: Boolean(primaryCta),
      primaryCtaTarget: primaryCta?.dataset.primaryCtaTarget || mobileSheet?.dataset.primaryCtaTarget || null,
      sheetY: sheetRect ? Math.round(sheetRect.top) : null,
      sheetBottom: sheetRect ? Math.round(sheetRect.bottom) : null,
      text: text(mobileSheet).slice(0, 360),
    };
    const worldMobilePrimaryCtas = Array.from(document.querySelectorAll('#world-hero-mobile-actions #world-mobile-primary-cta')).filter(visible);
    const worldPrimaryCta = worldMobilePrimaryCtas[0] || null;
    const worldMobilePrimaryCta = {
      sheetPresent: Boolean(worldMobileSheet),
      contractVersion: worldMobileSheet?.dataset.contractVersion || null,
      paritySource: worldMobileSheet?.dataset.paritySource || null,
      declaredPrimaryCtaCount: worldMobileSheet?.dataset.primaryCtaCount || null,
      firstScreenLoop: worldMobileSheet?.dataset.firstScreenLoop || null,
      primaryCtaCount: worldMobilePrimaryCtas.length,
      primaryCtaVisible: Boolean(worldPrimaryCta),
      routeMasteryXp: worldRewardXp?.dataset.routeMasteryXp || null,
      routeMasteryTier: worldRewardXp?.dataset.routeMasteryTier || null,
      text: text(worldMobileSheet).slice(0, 520),
    };
    const mobileCopyLayering = {
      summaryPresent: Boolean(copySummary),
      summaryContractVersion: copySummary?.dataset.contractVersion || null,
      summaryText: text(copySummary),
      summaryLength: text(copySummary).length,
      detailsPresent: Boolean(copyDetails),
      detailsContractVersion: copyDetails?.dataset.contractVersion || null,
      defaultState: copyDetails?.dataset.defaultState || null,
      detailsOpen: Boolean(copyDetails?.open),
      detailsVisibleText: text(copyDetails).slice(0, 260),
    };
    const mapReadabilityLod = {
      present: Boolean(appMapReadabilityLod),
      contractVersion: appMapReadabilityLod?.dataset.contractVersion || null,
      semanticLayerContract: appMapReadabilityLod?.dataset.semanticLayerContract || null,
      firstScreenMode: appMapReadabilityLod?.dataset.firstScreenMode || null,
      primaryCtaBudget: appMapReadabilityLod?.dataset.primaryCtaBudget || null,
      visibleMarkerBudget: appMapReadabilityLod?.dataset.visibleMarkerBudget || null,
      avatarRunnerBudget: appMapReadabilityLod?.dataset.avatarRunnerBudget || null,
      copySummaryBudget: appMapReadabilityLod?.dataset.copySummaryBudget || null,
      detailsDefaultState: appMapReadabilityLod?.dataset.detailsDefaultState || null,
      semanticLegendRequired: appMapReadabilityLod?.dataset.semanticLegendRequired || null,
      avatarFeedbackRequired: appMapReadabilityLod?.dataset.avatarFeedbackRequired || null,
      i18nA11yRequired: appMapReadabilityLod?.dataset.i18nA11yRequired || null,
      text: text(appMapReadabilityLod).slice(0, 260),
    };
    const worldMapReadability = {
      present: Boolean(worldMapReadabilityLod),
      contractVersion: worldMapReadabilityLod?.dataset.contractVersion || null,
      semanticLayerContract: worldMapReadabilityLod?.dataset.semanticLayerContract || null,
      firstScreenMode: worldMapReadabilityLod?.dataset.firstScreenMode || null,
      primaryCtaBudget: worldMapReadabilityLod?.dataset.primaryCtaBudget || null,
      visibleMarkerBudget: worldMapReadabilityLod?.dataset.visibleMarkerBudget || null,
      avatarRunnerBudget: worldMapReadabilityLod?.dataset.avatarRunnerBudget || null,
      copySummaryBudget: worldMapReadabilityLod?.dataset.copySummaryBudget || null,
      detailsDefaultState: worldMapReadabilityLod?.dataset.detailsDefaultState || null,
      semanticLegendRequired: worldMapReadabilityLod?.dataset.semanticLegendRequired || null,
      avatarFeedbackRequired: worldMapReadabilityLod?.dataset.avatarFeedbackRequired || null,
      i18nA11yRequired: worldMapReadabilityLod?.dataset.i18nA11yRequired || null,
      paritySource: worldMapReadabilityLod?.dataset.paritySource || null,
      text: text(worldMapReadabilityLod).slice(0, 260),
    };
    const mapPerformanceBudget = {
      present: Boolean(mapPerformanceBudgetElement),
      contractVersion: mapPerformanceBudgetElement?.dataset.contractVersion || null,
      firstMapInteractiveTargetMs: mapPerformanceBudgetElement?.dataset.firstMapInteractiveTargetMs || null,
      viewportRefreshP95TargetMs: mapPerformanceBudgetElement?.dataset.viewportRefreshP95TargetMs || null,
      focusToActionRailTargetMs: mapPerformanceBudgetElement?.dataset.focusToActionRailTargetMs || null,
      mainThreadLongTaskBudgetMs: mapPerformanceBudgetElement?.dataset.mainThreadLongTaskBudgetMs || null,
      lowEndMobileFpsFloor: mapPerformanceBudgetElement?.dataset.lowEndMobileFpsFloor || null,
      deltaViewportUpdatesRequired: mapPerformanceBudgetElement?.dataset.deltaViewportUpdatesRequired || null,
      abortPreviousViewportRequest: mapPerformanceBudgetElement?.dataset.abortPreviousViewportRequest || null,
      deferNoncriticalCardRender: mapPerformanceBudgetElement?.dataset.deferNoncriticalCardRender || null,
      clusterMarkersBeforeHiding: mapPerformanceBudgetElement?.dataset.clusterMarkersBeforeHiding || null,
      spatialCacheRequired: mapPerformanceBudgetElement?.dataset.spatialCacheRequired || null,
      virtualizedCardsRequired: mapPerformanceBudgetElement?.dataset.virtualizedCardsRequired || null,
      adaptiveDensityRequired: mapPerformanceBudgetElement?.dataset.adaptiveDensityRequired || null,
      text: text(mapPerformanceBudgetElement).slice(0, 260),
    };
    const mapTransportDelta = {
      present: Boolean(mapTransportDeltaElement),
      contractVersion: mapTransportDeltaElement?.dataset.contractVersion || null,
      subsystemContract: mapTransportDeltaElement?.dataset.subsystemContract || null,
      presenceDeltaRequired: mapTransportDeltaElement?.dataset.presenceDeltaRequired || null,
      snapshotFallbackRequired: mapTransportDeltaElement?.dataset.snapshotFallbackRequired || null,
      changedGroupRenderingRequired: mapTransportDeltaElement?.dataset.changedGroupRenderingRequired || null,
      visibleMarkerDeltaRequired: mapTransportDeltaElement?.dataset.visibleMarkerDeltaRequired || null,
      markerClusterDeltaRequired: mapTransportDeltaElement?.dataset.markerClusterDeltaRequired || null,
      paritySource: mapTransportDeltaElement?.dataset.paritySource || null,
      text: text(mapTransportDeltaElement).slice(0, 260),
    };
    const mapRumSlo = {
      present: Boolean(mapRumSloElement),
      contractVersion: mapRumSloElement?.dataset.contractVersion || null,
      quantiles: mapRumSloElement?.dataset.quantiles || null,
      surfaceSplit: mapRumSloElement?.dataset.surfaceSplit || null,
      deviceSplit: mapRumSloElement?.dataset.deviceSplit || null,
      sampleKinds: mapRumSloElement?.dataset.sampleKinds || null,
      perBucketMinSamples: mapRumSloElement?.dataset.perBucketMinSamples || null,
      matrixContract: mapRumSloElement?.dataset.matrixContract || null,
      paritySource: mapRumSloElement?.dataset.paritySource || null,
      text: text(mapRumSloElement).slice(0, 260),
    };
    const mapWeakNetwork = {
      present: Boolean(mapWeakNetworkElement),
      contractVersion: mapWeakNetworkElement?.dataset.contractVersion || null,
      cacheKey: mapWeakNetworkElement?.dataset.cacheKey || null,
      delta304Supported: mapWeakNetworkElement?.getAttribute('data-delta-304-supported') || null,
      offlineBannerRequired: mapWeakNetworkElement?.dataset.offlineBannerRequired || null,
      pendingActionQueueRequired: mapWeakNetworkElement?.dataset.pendingActionQueueRequired || null,
      conflictSyncRequired: mapWeakNetworkElement?.dataset.conflictSyncRequired || null,
      paritySource: mapWeakNetworkElement?.dataset.paritySource || null,
      text: text(mapWeakNetworkElement).slice(0, 260),
    };
    const mapLocationPrivacy = {
      present: Boolean(mapLocationPrivacyElement),
      contractVersion: mapLocationPrivacyElement?.dataset.contractVersion || null,
      rumExcludesLatLng: mapLocationPrivacyElement?.dataset.rumExcludesLatLng || null,
      cacheControl: mapLocationPrivacyElement?.dataset.cacheControl || null,
      paritySource: mapLocationPrivacyElement?.dataset.paritySource || null,
      text: text(mapLocationPrivacyElement).slice(0, 260),
    };
    const shadowRenderer = {
      present: Boolean(worldMapShadowRenderer),
      contractVersion: worldMapShadowRenderer?.dataset.contractVersion || null,
      parityContract: worldMapShadowRenderer?.dataset.parityContract || null,
      activeEngine: worldMapShadowRenderer?.dataset.activeEngine || null,
      shadowEngine: worldMapShadowRenderer?.dataset.shadowEngine || null,
      status: worldMapShadowRenderer?.dataset.status || null,
      canaryPercent: worldMapShadowRenderer?.dataset.canaryPercent || null,
      rollbackDrillRequired: worldMapShadowRenderer?.dataset.rollbackDrillRequired || null,
      text: text(worldMapShadowRenderer).slice(0, 260),
    };
    const routeRunnerHandoff = {
      appRouteSummaryPresent: Boolean(appRouteHandoff),
      appFeedSummaryPresent: Boolean(appFeedHandoff),
      worldSummaryPresent: Boolean(worldRouteHandoff),
      contractVersionPresent: html.includes('trillionnium_route_runner_handoff_v1'),
      sourcePresent: html.includes('route_runner_handoff'),
      nextRouteStatus: handoffElements.map((el) => el.dataset.nextRouteStatus).find(Boolean) || null,
      runnerCount: pickMaxCount(handoffElements, 'runnerCount'),
      rewardClaimCount: pickMaxCount(handoffElements, 'rewardClaimCount'),
      nextRouteCount: pickMaxCount(handoffElements, 'nextRouteCount'),
      hasRewardCopy: /reward/i.test(handoffText),
      hasNextRouteCopy: /next[- ]route/i.test(handoffText),
      text: handoffText.slice(0, 320),
    };
    return {
      profile: profileName,
      name: targetName,
      viewport: { vw, vh },
      scrollH: Math.round(document.documentElement.scrollHeight),
      visibleCjk,
      overflowRaw,
      actionableOverflow,
      coveredActionables,
      smallTouchTargets,
      key,
      routeRunnerHandoff,
      mobilePrimaryCta,
      worldMobilePrimaryCta,
      mobileCopyLayering,
      mapReadabilityLod,
      worldMapReadability,
      mapPerformanceBudget,
      mapTransportDelta,
      mapRumSlo,
      mapWeakNetwork,
      mapLocationPrivacy,
      shadowRenderer,
      firstViewportButtons,
      firstViewportText: text(document.body).slice(0, 1400),
    };
  }, { profileName: profile.name, targetName: target.name, selectors });
  return { ...data, route: target.route, screenshot: screenshotPath };
}

await fs.mkdir(outDir, { recursive: true });
const browser = await chromium.launch({ headless: true, executablePath });
const results = [];
const failures = [];
try {
  for (const profile of profiles) {
    const context = await browser.newContext(profile.context);
    for (const target of targets) {
      const page = await context.newPage();
      let result = null;
      try {
        result = await auditPage(page, profile, target);
        checkCommon(result);
        checkRouteRunnerHandoff(result);
        if (profile.name === 'desktop') checkDesktop(result, profile.limits[target.name]);
        else checkMobile(result, profile.limits[target.name]);
        result.ok = true;
        results.push(result);
      } catch (error) {
        failures.push({ profile: profile.name, surface: target.name, message: error.message, details: error.details });
        if (result) {
          result.ok = false;
          result.error = error.message;
          result.details = error.details;
          results.push(result);
        } else {
          results.push({ profile: profile.name, name: target.name, route: target.route, ok: false, error: error.message, details: error.details });
        }
      } finally {
        await page.close().catch(() => null);
      }
    }
    await context.close().catch(() => null);
  }
} finally {
  await browser.close().catch(() => null);
}

const summary = {
  ok: failures.length === 0,
  run_id: runId,
  checked_at_epoch: Math.floor(Date.now() / 1000),
  base_url: baseUrl,
  browser: 'playwright.chromium',
  executable_path: executablePath,
  summary_path: summaryPath,
  failures,
  results,
};
await fs.writeFile(summaryPath, `${JSON.stringify(summary, null, 2)}\n`);

for (const result of results) {
  const keySummary = result.key
    ? Object.entries(result.key)
        .map(([key, value]) => `${key}=${value ? `${value.y}-${value.bottom}/h${value.h}` : 'null'}`)
        .join(' ')
    : '';
  console.log(`${result.ok ? 'ok' : 'FAIL'} ${result.profile}/${result.name} scrollH=${result.scrollH ?? 'n/a'} cjk=${result.visibleCjk?.length ?? 'n/a'} overflow=${result.actionableOverflow?.length ?? 'n/a'} ${keySummary}`);
}
console.log(JSON.stringify({ ok: summary.ok, summary_path: summaryPath, failures }, null, 2));

if (!summary.ok) process.exit(1);
