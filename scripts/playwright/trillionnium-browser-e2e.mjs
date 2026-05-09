import { createRequire } from 'node:module';
import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { chromium, devices } = require('playwright');

const baseUrl = (process.env.CONSUMER_ENTRY_BASE_URL || process.env.BASE_URL || 'http://127.0.0.1:8090').replace(/\/$/, '');
const rootDir = process.env.CEX_PROJECT_ROOT || process.cwd();
const outDir = process.env.TRILLIONNIUM_BROWSER_E2E_OUT_DIR || path.join(rootDir, 'run', 'league-browser');
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || process.env.CHROME_BIN || '/usr/bin/google-chrome-stable';
const expectFinalCutover = process.env.TRILLIONNIUM_BROWSER_E2E_EXPECT_FINAL_CUTOVER !== '0';
const ingressToken = (process.env.CONSUMER_ENTRY_INGRESS_TOKEN || '').trim();
const matrixUserId = process.env.TRILLIONNIUM_BROWSER_E2E_MATRIX_USER_ID || '@alice:local.dev';
const roomId = process.env.TRILLIONNIUM_BROWSER_E2E_ROOM_ID || '!browser-local:local.dev';
const sessionId = process.env.TRILLIONNIUM_BROWSER_E2E_SESSION_ID || 'browser-e2e-session';
const runId = `${Math.floor(Date.now() / 1000)}-${process.pid}`;
const summaryPath = path.join(outDir, `browser-e2e-summary-${runId}.json`);
const screenshotDir = path.join(outDir, `screenshots-${runId}`);

const leafletStub = String.raw`
(function(){
  const layerApi = () => ({
    addTo(target){ if (target && target.__layers) target.__layers.add(this); return this; },
    bindPopup(){ return this; },
    bindTooltip(){ return this; },
    openPopup(){ return this; },
    closePopup(){ return this; },
    on(){ return this; },
    setStyle(){ return this; },
    setLatLng(){ return this; },
    remove(){ return this; },
    clearLayers(){ return this; },
  });
  const makeMap = () => ({
    __layers: new Set(),
    __center: { lat: 31.230416, lng: 121.473701 },
    __zoom: 15,
    setView(point, zoom){ this.__center = { lat: Number(point[0]), lng: Number(point[1]) }; this.__zoom = Number(zoom || this.__zoom); return this; },
    getCenter(){ return this.__center; },
    getZoom(){ return this.__zoom; },
    on(){ return this; },
    off(){ return this; },
    addLayer(layer){ this.__layers.add(layer); return this; },
    removeLayer(layer){ this.__layers.delete(layer); return this; },
    hasLayer(layer){ return this.__layers.has(layer); },
    fitBounds(){ return this; },
    invalidateSize(){ return this; },
  });
  window.L = {
    map: makeMap,
    tileLayer: () => layerApi(),
    layerGroup: () => ({...layerApi(), __layers: new Set(), clearLayers(){ this.__layers.clear(); return this; }, addLayer(layer){ this.__layers.add(layer); return this; }, hasLayer(layer){ return this.__layers.has(layer); }}),
    latLngBounds: () => ({
      isValid: () => true,
      pad(){ return this; },
      extend(){ return this; },
      getCenter(){ return { lat: 31.230416, lng: 121.473701 }; },
    }),
    polyline: () => layerApi(),
    marker: () => layerApi(),
    divIcon: (options = {}) => ({ options }),
    circle: () => layerApi(),
    circleMarker: () => layerApi(),
    rectangle: () => layerApi(),
  };
})();
`;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function assert(condition, message, details = undefined) {
  if (!condition) {
    const error = new Error(message);
    if (details !== undefined) error.details = details;
    throw error;
  }
}

async function text(page, selector) {
  return (await page.locator(selector).first().textContent({ timeout: 10_000 })) || '';
}

async function count(page, selector) {
  return await page.locator(selector).count();
}

function isAllowedRouteRunnerNextRouteStatus(status) {
  return status === 'next_route_preview_locked_until_reward_claim' || status === 'next_route_ready_after_reward_claim';
}

function assertRouteRunnerHandoffContract(handoff, label) {
  assert(handoff?.contract_version === 'trillionnium_route_runner_handoff_v1', `${label} route-runner handoff contract missing`, handoff);
  assert(handoff?.supports_route_runner_reward_claim_actions === true, `${label} reward-claim handoff support missing`, handoff);
  assert(handoff?.supports_route_runner_next_route_actions === true, `${label} next-route handoff support missing`, handoff);
  assert(handoff?.supports_checkpoint_reward_history === true, `${label} checkpoint reward history support missing`, handoff);
  assert(handoff?.supports_route_runner_lifecycle === true, `${label} lifecycle support missing`, handoff);
  assert(handoff?.lifecycle_contract_version === 'trillionnium_route_runner_lifecycle_v1', `${label} lifecycle contract missing`, handoff);
  assert(handoff?.supports_route_mastery_progression === true, `${label} route mastery support missing`, handoff);
  assert(handoff?.route_mastery_contract_version === 'trillionnium_route_mastery_v1', `${label} route mastery contract missing`, handoff);
  assert(Number(handoff?.route_mastery_runner_count || 0) >= 1, `${label} route mastery runner count missing`, handoff);
  assert(Number(handoff?.runner_count || 0) >= 1, `${label} runner count missing`, handoff);
  assert(Number(handoff?.reward_claim_action_count || 0) >= 1, `${label} reward-claim action count missing`, handoff);
  assert(Number(handoff?.next_route_action_count || 0) >= 1, `${label} next-route action count missing`, handoff);
  assert(Boolean(handoff?.first_task_id), `${label} first route-runner task missing`, handoff);
  assert(Boolean(handoff?.first_progress_label), `${label} first route-runner progress label missing`, handoff);
  assert(Number(handoff?.first_route_mastery_xp || 0) >= 1, `${label} first route mastery XP missing`, handoff);
  assert(Boolean(handoff?.first_route_mastery_tier), `${label} first route mastery tier missing`, handoff);
  assert(Boolean(handoff?.first_route_mastery_next_goal), `${label} first route mastery next goal missing`, handoff);
  assert(Boolean(handoff?.first_reward_claim_status), `${label} reward-claim status missing`, handoff);
  assert(isAllowedRouteRunnerNextRouteStatus(handoff?.first_next_route_status), `${label} next-route status missing`, handoff);
  assert(Boolean(handoff?.first_lifecycle_source), `${label} lifecycle source missing`, handoff);
  assert(Boolean(handoff?.first_lifecycle_stage), `${label} lifecycle stage missing`, handoff);
  assert(Boolean(handoff?.first_lifecycle_status), `${label} lifecycle status missing`, handoff);
  assert(Boolean(handoff?.first_next_route_action_body), `${label} next-route action body missing`, handoff);
  assert(Boolean(handoff?.first_next_route_sequence_summary), `${label} next-route sequence summary missing`, handoff);
  assert(Boolean(handoff?.handoff_prompt), `${label} handoff prompt missing`, handoff);
  return {
    contract_version: handoff.contract_version,
    runner_count: handoff.runner_count,
    reward_claim_action_count: handoff.reward_claim_action_count,
    next_route_action_count: handoff.next_route_action_count,
    route_mastery_contract_version: handoff.route_mastery_contract_version,
    first_route_mastery_xp: handoff.first_route_mastery_xp,
    first_route_mastery_tier: handoff.first_route_mastery_tier,
    first_next_route_status: handoff.first_next_route_status,
    first_next_route_sequence_summary: handoff.first_next_route_sequence_summary,
  };
}

async function assertRouteRunnerHandoffDom(page, selector, label) {
  assert(await count(page, selector) >= 1, `${label} route-runner handoff DOM missing`);
  const dom = await page.locator(selector).first().evaluate((node) => ({
    id: node.id || null,
    nextRouteStatus: node.dataset.nextRouteStatus || null,
    runnerCount: Number.parseInt(node.dataset.runnerCount || '0', 10) || 0,
    rewardClaimCount: Number.parseInt(node.dataset.rewardClaimCount || '0', 10) || 0,
    nextRouteCount: Number.parseInt(node.dataset.nextRouteCount || '0', 10) || 0,
    routeMasteryContract: node.dataset.routeMasteryContract || null,
    routeMasteryTier: node.dataset.routeMasteryTier || null,
    routeMasteryXp: Number.parseInt(node.dataset.routeMasteryXp || '0', 10) || 0,
    text: String(node.innerText || node.textContent || '').replace(/\s+/g, ' ').trim(),
  }));
  assert(isAllowedRouteRunnerNextRouteStatus(dom.nextRouteStatus), `${label} DOM next-route status missing`, dom);
  assert(dom.runnerCount >= 1, `${label} DOM runner count missing`, dom);
  assert(dom.rewardClaimCount >= 1, `${label} DOM reward-claim count missing`, dom);
  assert(dom.nextRouteCount >= 1, `${label} DOM next-route count missing`, dom);
  assert(dom.routeMasteryContract === 'trillionnium_route_mastery_v1', `${label} DOM route mastery contract missing`, dom);
  assert(dom.routeMasteryXp >= 1, `${label} DOM route mastery XP missing`, dom);
  assert(Boolean(dom.routeMasteryTier), `${label} DOM route mastery tier missing`, dom);
  assert(/reward/i.test(dom.text) && /next[- ]route/i.test(dom.text), `${label} DOM handoff copy missing`, dom);
  return dom;
}

async function clickAndWaitForNavigationOrSettle(page, locator) {
  await Promise.all([
    page.waitForLoadState('domcontentloaded').catch(() => null),
    clickOrDomActivate(locator),
  ]);
  await page.waitForLoadState('networkidle', { timeout: 5_000 }).catch(() => null);
}

async function clickOrDomActivate(locator) {
  try {
    await locator.click({ timeout: 10_000 });
  } catch (error) {
    // The current mobile/game shells have dense HUD cards that can visually overlap
    // a target in WebKit/Chromium mobile emulation. For browser E2E we still want
    // the real DOM event path, so fall back to dispatching a click on the resolved
    // element instead of silently downgrading to an API-only check.
    await locator.dispatchEvent('click', {}, { timeout: 5_000 });
  }
}

function ingressHeaders() {
  return ingressToken ? { 'x-entry-token': ingressToken } : {};
}

function signedSessionHeaders() {
  const secret = process.env.CONSUMER_ENTRY_SESSION_AUTH_SECRET || process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET || '';
  if (!secret.trim()) return {};
  const issuer = process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER || 'matrix-entry-adapter';
  const audience = process.env.CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE || process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE || 'consumer-entry-api';
  const now = Math.floor(Date.now() / 1000);
  const requestFingerprint = `league-web-session:${matrixUserId}:${roomId}:${sessionId}`;
  const claims = {
    version: 1,
    issuer,
    key_id: null,
    subject: matrixUserId,
    source_kind: 'league_web_session',
    audience,
    request_fingerprint: requestFingerprint,
    room_id: roomId,
    session_id: sessionId,
    org_id: null,
    account_id: null,
    issued_at_epoch: now,
    expires_at_epoch: now + 300,
  };
  const assertion = Buffer.from(JSON.stringify(claims)).toString('base64url');
  const signature = crypto.createHmac('sha256', secret).update(assertion).digest('base64url');
  return {
    'x-cex-user-session': assertion,
    'x-cex-user-session-signature': signature,
  };
}

async function seedWebSession(context) {
  const response = await context.request.post('/league/web/session', {
    headers: { ...ingressHeaders(), ...signedSessionHeaders() },
    data: { matrix_user_id: matrixUserId, room_id: roomId, session_id: sessionId },
    timeout: 20_000,
  });
  const bodyText = await response.text();
  assert(response.ok(), `failed to seed browser web session: ${response.status()}`, bodyText);
  const body = JSON.parse(bodyText || '{}');
  assert(body.csrf, 'browser web session did not return csrf', body);
  return body;
}

async function activateTab(page, tab) {
  const tabButton = page.locator(`nav.app-bottom-tabs [data-app-tab="${tab}"]`).first();
  await clickOrDomActivate(tabButton);
  await page.waitForSelector(`#app-tab-${tab}.is-active`, { timeout: 10_000 });
}

async function assertNoVisibleBilingualSlashPair(page, label) {
  const text = await page.locator('body').innerText({ timeout: 10_000 });
  const slashPair = /(?:[A-Za-z][^\n]{0,120}\s\/\s[^\n]{0,120}[\u3400-\u9fff]|[\u3400-\u9fff][^\n]{0,120}\s\/\s[^\n]{0,120}[A-Za-z])/;
  const offenders = text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => slashPair.test(line));
  assert(offenders.length === 0, `${label} still shows bilingual slash-pair copy`, offenders.slice(0, 8));
}

async function assertEnglishSurfaceHasNoCoreChineseLeaks(page, label) {
  const pageText = await page.locator('body').innerText({ timeout: 10_000 });
  const forbidden = [
    '地图焦点可见',
    '世界事件已创建',
    '契约已开启或完成',
    '冒险委托已创建',
    '评级与返工路线可见',
    '奖励成长动态已更新',
    '路线下一步可见',
    '聚焦区域',
    '聚焦热点',
    '查看分片',
    '查看地图分片',
    '预热分片',
    '追踪事件',
    '最近热点',
    '高热事件',
  ];
  const offenders = forbidden.filter((needle) => pageText.includes(needle));
  assert(offenders.length === 0, `${label} leaked core Chinese map/onboarding labels in English mode`, offenders);
}

async function submitWorldForm(page, formSelector, marker, expectedUrlFragment) {
  const form = page.locator(formSelector).first();
  await form.evaluate((node) => {
    let current = node;
    while (current) {
      if (current.tagName === 'DETAILS') current.open = true;
      current = current.parentElement;
    }
  });
  await form.scrollIntoViewIfNeeded({ timeout: 10_000 });
  const textarea = form.locator('textarea').first();
  if (await textarea.count()) {
    const existing = await textarea.inputValue().catch(() => '');
    const nextValue = `${existing}\nBrowser E2E marker ${marker}: normalized SQL final-cutover path proof.`;
    if (await textarea.isVisible().catch(() => false)) {
      await textarea.fill(nextValue);
    } else {
      await textarea.evaluate((node, value) => {
        node.value = value;
        node.dispatchEvent(new Event('input', { bubbles: true }));
      }, nextValue);
    }
  }
  const [response] = await Promise.all([
    page.waitForNavigation({ waitUntil: 'domcontentloaded', timeout: 20_000 }).catch(() => null),
    form.evaluate((node) => {
      const submitter = node.querySelector('button[type="submit"], button');
      if (typeof node.requestSubmit === 'function') {
        node.requestSubmit(submitter || undefined);
      } else {
        node.submit();
      }
    }),
  ]);
  await page.waitForLoadState('networkidle', { timeout: 5_000 }).catch(() => null);
  if (!page.url().includes(expectedUrlFragment)) {
    const bodySnippet = await page.locator('body').innerText({ timeout: 2_000 }).catch(() => '');
    assert(false, `expected ${formSelector} navigation to include ${expectedUrlFragment}`, {
      url: page.url(),
      status: response ? response.status() : null,
      body: bodySnippet.slice(0, 800),
    });
  }
  return response ? response.status() : 200;
}

async function main() {
  await fs.mkdir(outDir, { recursive: true });
  await fs.mkdir(screenshotDir, { recursive: true });
  const consoleMessages = [];
  const pageErrors = [];
  const requestFailures = [];
  const steps = [];
  const routeRunnerHandoffCoverage = {};

  const browser = await chromium.launch({
    executablePath,
    headless: true,
    args: ['--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu'],
  });
  const context = await browser.newContext({
    ...devices['iPhone 13'],
    locale: 'zh-CN',
    timezoneId: 'Asia/Shanghai',
    baseURL: baseUrl,
    extraHTTPHeaders: ingressHeaders(),
  });

  const webSession = await seedWebSession(context);

  await context.route('https://unpkg.com/leaflet@1.9.4/dist/leaflet.css', (route) => route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
  await context.route('https://unpkg.com/leaflet@1.9.4/dist/leaflet.js', (route) => route.fulfill({ status: 200, contentType: 'application/javascript', body: leafletStub }));
  await context.route(/https:\/\/.*\.tile\.openstreetmap\.org\/.*/, (route) => route.fulfill({ status: 204, body: '' }));

  const page = await context.newPage();
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) {
      consoleMessages.push({ type: message.type(), text: message.text() });
    }
  });
  page.on('pageerror', (error) => pageErrors.push(String(error && error.stack || error)));
  page.on('requestfailed', (request) => {
    const url = request.url();
    if (!url.includes('favicon.ico')) {
      requestFailures.push({ url, failure: request.failure()?.errorText || 'requestfailed' });
    }
  });

  const marker = `browser-e2e-${Math.floor(Date.now() / 1000)}`;

  await page.goto('/app?lang=en', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#real-world-map', { timeout: 15_000 });
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'en', { timeout: 10_000 });
  assert((await page.title()).includes('Trillionnium World Mobile'), 'app title missing');
  const englishTabs = await page.$$eval('nav.app-bottom-tabs [data-app-tab]', (tabs) => tabs.map((tab) => tab.textContent.trim()));
  assert(JSON.stringify(englishTabs) === JSON.stringify(['Messages', 'World', 'Feed', 'Me']), `English system language tabs mismatch: ${JSON.stringify(englishTabs)}`);
  await assertNoVisibleBilingualSlashPair(page, '/app English system language');
  await assertEnglishSurfaceHasNoCoreChineseLeaks(page, '/app English system language');
  assert(await count(page, '[data-trillionnium-language-select]') >= 2, 'system language selectors missing');
  await activateTab(page, 'me');
  await page.locator('#trillionnium-app-language-select').selectOption('zh');
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'zh', { timeout: 10_000 });
  const chineseTabs = await page.$$eval('nav.app-bottom-tabs [data-app-tab]', (tabs) => tabs.map((tab) => tab.textContent.trim()));
  assert(JSON.stringify(chineseTabs) === JSON.stringify(['消息', '世界', '动态', '我']), `Chinese system language tabs mismatch: ${JSON.stringify(chineseTabs)}`);
  await assertNoVisibleBilingualSlashPair(page, '/app Chinese system language');
  assert((await page.locator('#trillionnium-system-language-settings').innerText()).includes('系统设置'), 'Chinese system settings copy missing');
  await page.locator('#trillionnium-app-language-select').selectOption('en');
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'en', { timeout: 10_000 });
  await activateTab(page, 'map');
  assert(await count(page, '[data-app-tab]') >= 4, 'mobile bottom tabs missing');
  assert(await count(page, '#app-tab-map.is-active') === 1, 'map tab not active by default');
  assert(await count(page, 'nav.app-bottom-tabs[role="tablist"]') === 1, 'accessible tablist missing');
  assert(await page.locator('nav.app-bottom-tabs [data-app-tab="map"]').first().getAttribute('aria-selected') === 'true', 'active map tab aria-selected missing');
  assert(await count(page, '#app-tab-map[role="tabpanel"]:not([hidden])') === 1, 'active map tabpanel not exposed');
  assert(await count(page, '#app-ux-live-status') === 1, 'UX live status missing');
  assert(await count(page, '#app-search-empty-state') === 1, 'search empty state missing');
  assert(await count(page, '#app-feed-items-live .app-feed-item, #app-feed-items-live article') >= 1, 'embedded feed cards missing');
  steps.push({ name: 'app_boot_mobile_map_feed_shell', ok: true });

  await page.locator('nav.app-bottom-tabs [data-app-tab="map"]').first().focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForSelector('#app-tab-feed.is-active', { timeout: 10_000 });
  assert(await page.locator('nav.app-bottom-tabs [data-app-tab="feed"]').first().getAttribute('aria-selected') === 'true', 'keyboard tab navigation did not update aria-selected');
  await page.locator('#app-global-search').fill(`no-match-${marker}`);
  await page.waitForSelector('#app-search-empty-state.is-visible', { timeout: 10_000 });
  const clearVisible = await page.locator('#app-search-clear').first().isVisible({ timeout: 5_000 }).catch(() => false);
  assert(clearVisible, 'search clear button not visible for active query');
  await clickOrDomActivate(page.locator('#app-search-clear').first());
  await page.waitForFunction(() => !(document.querySelector('#app-search-empty-state')?.classList.contains('is-visible')), { timeout: 10_000 });
  steps.push({ name: 'app_mobile_ux_a11y_keyboard_search', ok: true });

  const appJsonText = await page.locator('#trillionnium-app-data').first().textContent({ timeout: 10_000 });
  const appJson = JSON.parse(appJsonText || '{}');
  const appViewport = appJson?.map_hub?.viewport || {};
  assert(appJson?.mobile_shell_contract?.rum_slo_contract?.contract_version === 'trillionnium_world_map_rum_slo_v1', 'mobile shell RUM SLO contract missing from client app json', appJson?.mobile_shell_contract?.rum_slo_contract);
  assert(appJson?.mobile_shell_contract?.rum_slo_contract?.sample_matrix?.contract_version === 'trillionnium_world_map_real_user_rum_matrix_v1', 'mobile shell real-user RUM matrix contract missing', appJson?.mobile_shell_contract?.rum_slo_contract?.sample_matrix);
  assert(appJson?.mobile_shell_contract?.runtime_performance_budget?.density_scalability?.contract_version === 'trillionnium_world_map_density_scalability_v1', 'mobile shell density scalability contract missing', appJson?.mobile_shell_contract?.runtime_performance_budget?.density_scalability);
  assert(appJson?.mobile_shell_contract?.weak_network_resilience?.contract_version === 'trillionnium_world_map_weak_network_resilience_v1', 'mobile shell weak-network contract missing from client app json', appJson?.mobile_shell_contract?.weak_network_resilience);
  assert(appJson?.mobile_shell_contract?.weak_network_resilience?.offline_action_queue?.contract_version === 'trillionnium_world_map_offline_action_queue_v1', 'mobile shell offline action queue contract missing', appJson?.mobile_shell_contract?.weak_network_resilience?.offline_action_queue);
  assert(appJson?.mobile_shell_contract?.location_privacy_contract?.contract_version === 'trillionnium_world_map_location_privacy_v1', 'mobile shell location privacy contract missing from client app json', appJson?.mobile_shell_contract?.location_privacy_contract);
  assert(appJson?.mobile_shell_contract?.gameplay_accessibility_i18n?.contract_version === 'trillionnium_world_map_gameplay_accessibility_i18n_v1', 'mobile shell gameplay accessibility/i18n contract missing', appJson?.mobile_shell_contract?.gameplay_accessibility_i18n);
  assert(appViewport?.rum_slo_contract?.contract_version === 'trillionnium_world_map_rum_slo_v1', 'viewport RUM SLO contract missing from client app json', appViewport?.rum_slo_contract);
  assert(appViewport?.rum_slo_contract?.sample_matrix?.contract_version === 'trillionnium_world_map_real_user_rum_matrix_v1', 'viewport real-user RUM matrix contract missing from client app json', appViewport?.rum_slo_contract?.sample_matrix);
  assert(appViewport?.weak_network_resilience?.contract_version === 'trillionnium_world_map_weak_network_resilience_v1', 'viewport weak-network contract missing from client app json', appViewport?.weak_network_resilience);
  assert(appViewport?.weak_network_resilience?.offline_action_queue?.pending_action_queue?.sync_on_reconnect_required === true, 'viewport offline action queue sync contract missing', appViewport?.weak_network_resilience?.offline_action_queue);
  assert(appViewport?.location_privacy_contract?.contract_version === 'trillionnium_world_map_location_privacy_v1', 'viewport location privacy contract missing from client app json', appViewport?.location_privacy_contract);
  assert(appViewport?.gameplay_accessibility_i18n?.contract_version === 'trillionnium_world_map_gameplay_accessibility_i18n_v1', 'viewport gameplay accessibility/i18n contract missing', appViewport?.gameplay_accessibility_i18n);
  const appViewportDeltaCache = appViewport?.transport_delta_contract?.entity_delta_cache || {};
  assert(appViewport?.viewport_api?.not_modified_304_supported === true || appViewportDeltaCache?.not_modified_304_compatible === true, 'viewport API 304 support missing from client app json', { viewport_api: appViewport?.viewport_api, entity_delta_cache: appViewportDeltaCache });
  assert(appViewport?.viewport_api?.entity_delta_cache_contract === 'entity_group_versioned_delta_v1' || appViewportDeltaCache?.mode === 'entity_group_versioned_delta_v1', 'viewport entity delta cache contract missing from client app json', { viewport_api: appViewport?.viewport_api, entity_delta_cache: appViewportDeltaCache });
  assert(appViewportDeltaCache?.changed_group_rendering_required === true && appViewportDeltaCache?.visible_marker_delta_required === true && appViewportDeltaCache?.marker_cluster_delta_required === true, 'viewport changed-group marker/cluster delta contract missing from client app json', appViewportDeltaCache);
  assert(Array.isArray(appViewport?.marker_clusters) && appViewport.marker_clusters.length >= 1, 'viewport marker clusters missing from client app json', appViewport?.marker_clusters);
  assert(appViewport?.runtime_performance_budget?.degrade_strategy?.abort_previous_viewport_request === true && appViewport?.runtime_performance_budget?.degrade_strategy?.defer_noncritical_card_render === true && appViewport?.runtime_performance_budget?.degrade_strategy?.cluster_markers_before_hiding === true, 'viewport runtime P0/P1/P2 degrade strategy missing', appViewport?.runtime_performance_budget?.degrade_strategy);
  assert(appViewport?.runtime_performance_budget?.density_scalability?.frontend_virtualization?.virtualize_dense_cards_required === true && appViewport?.runtime_performance_budget?.density_scalability?.backend_projection?.spatial_tile_cache_required === true, 'viewport P1 density scalability contract missing', appViewport?.runtime_performance_budget?.density_scalability);
  assert(appViewport?.viewport_contract?.supports_location_privacy === true, 'viewport location privacy readiness missing', appViewport?.viewport_contract);
  assert(appViewport?.viewport_contract?.supports_weak_network_resilience === true, 'viewport weak-network readiness missing', appViewport?.viewport_contract);
  assert(appViewport?.viewport_contract?.supports_real_user_rum_matrix === true && appViewport?.viewport_contract?.supports_offline_action_queue === true && appViewport?.viewport_contract?.supports_density_scalability === true && appViewport?.viewport_contract?.supports_gameplay_accessibility_i18n === true, 'viewport P0-next/P1/P2 readiness flags missing', appViewport?.viewport_contract);
  assert(appViewport?.viewport_contract?.supports_visible_marker_delta === true && appViewport?.viewport_contract?.supports_marker_clusters === true, 'viewport marker delta/cluster readiness missing', appViewport?.viewport_contract);
  for (const selector of [
    '#app-map-rum-slo[data-contract-version="trillionnium_world_map_rum_slo_v1"][data-quantiles="p50,p95,p99"][data-surface-split="app,world"][data-device-split="mobile,desktop"][data-matrix-contract="trillionnium_world_map_real_user_rum_matrix_v1"][data-per-bucket-min-samples="1"]',
    '#app-map-weak-network[data-contract-version="trillionnium_world_map_weak_network_resilience_v1"][data-cache-key="trillionnium-world-map:last-good-viewport:v1"][data-delta-304-supported="true"][data-offline-banner-required="true"][data-pending-action-queue-required="true"]',
    '#app-map-location-privacy[data-contract-version="trillionnium_world_map_location_privacy_v1"][data-rum-excludes-lat-lng="true"][data-cache-control="private"]',
    '#app-map-performance-budget[data-spatial-cache-required="true"][data-virtualized-cards-required="true"][data-adaptive-density-required="true"]',
    '#app-map-readability-lod[data-semantic-legend-required="true"][data-avatar-feedback-required="true"][data-i18n-a11y-required="true"]',
  ]) {
    assert(await count(page, selector) === 1, `app map runtime safety DOM token missing: ${selector}`);
  }
  await page.waitForFunction(() => window.trillionniumMapLibreShadowProbe?.contract_version === 'trillionnium_world_map_renderer_shadow_v1' && window.trillionniumMapLibreShadowProbe?.shadow_engine_id === 'maplibre_gl_v1', { timeout: 15_000 });
  const browserShadowProbe = await page.evaluate(() => window.trillionniumMapLibreShadowProbe);
  assert(browserShadowProbe?.status === 'shadow_only_not_user_facing' && browserShadowProbe?.user_facing === false, 'MapLibre shadow probe must stay browser-exported and not user-facing', browserShadowProbe);
  assert(browserShadowProbe?.parity_contract_version === 'trillionnium_world_map_maplibre_shadow_parity_v1' && browserShadowProbe?.rollback_drill_required === true && browserShadowProbe?.canary_percent === 0, 'MapLibre shadow parity rollout/rollback contract missing', browserShadowProbe);
  assert(browserShadowProbe?.popup_semantics_match === true && browserShadowProbe?.focus_action_dataset_match === true && browserShadowProbe?.dom_contract_tokens_match === true, 'MapLibre popup/focus/action parity probe missing', browserShadowProbe);
  assert(await page.evaluate(() => window.trillionniumMapWeakNetworkContract) === 'trillionnium_world_map_weak_network_resilience_v1', 'weak-network runtime contract global missing');
  assert(await page.evaluate(() => window.trillionniumMapLocationPrivacyContract) === 'trillionnium_world_map_location_privacy_v1', 'location privacy runtime contract global missing');
  const forcedViewportRefresh = await page.evaluate(async () => {
    const viewport = await window.trillionniumRefreshMapViewport?.();
    return {
      viewport_cursor: viewport?.delta_cursor || viewport?.viewport_cursor || null,
      diagnostics: window.trillionniumMapRuntimeDiagnostics,
      cache: window.trillionniumMapWeakNetworkCacheStatus,
      rum: window.trillionniumMapRumLastSample,
      render_error: window.trillionniumMapRenderLastError || null,
      local_storage_keys: Object.keys(window.localStorage || {}).filter((key) => key.includes('trillionnium-world-map')).map((key) => [key, window.localStorage.getItem(key)?.length || 0]),
    };
  });
  assert(forcedViewportRefresh?.diagnostics?.cached_snapshot_available === true && forcedViewportRefresh?.diagnostics?.refresh_hook_available === true, 'map runtime diagnostics did not observe cached snapshot after forced refresh', forcedViewportRefresh);
  assert(!forcedViewportRefresh?.render_error, 'map viewport render recovered from an error instead of staying clean', forcedViewportRefresh);
  const runtimeDiagnostics = await page.evaluate(() => window.trillionniumMapRuntimeDiagnostics);
  assert(runtimeDiagnostics?.contract_version === 'trillionnium_world_map_runtime_diagnostics_v1', 'map runtime diagnostics contract missing', runtimeDiagnostics);
  assert(runtimeDiagnostics?.cache_key?.includes('trillionnium-world-map:last-good-viewport:v1'), 'map runtime diagnostics cache key missing', runtimeDiagnostics);
  assert(runtimeDiagnostics?.cache_shape === 'slim_last_good_viewport_snapshot_v1' && runtimeDiagnostics?.cache_store_ok === true, 'map runtime weak-network cache must store slim last-good snapshot', runtimeDiagnostics);
  assert(runtimeDiagnostics?.shadow_probe_exported === true, 'map runtime diagnostics shadow probe missing', runtimeDiagnostics);
  assert(runtimeDiagnostics?.changed_group_deferred_render_available === true && runtimeDiagnostics?.rum_focus_action_observer_installed === true, 'map runtime changed-group/focus RUM diagnostics missing', runtimeDiagnostics);
  assert(typeof runtimeDiagnostics?.viewport_abort_count === 'number' && runtimeDiagnostics?.marker_cluster_count >= 1, 'map runtime abort/cluster diagnostics missing', runtimeDiagnostics);
  const rumLastSample = await page.evaluate(() => window.trillionniumMapRumLastSample);
  assert(rumLastSample?.precise_lat_lng_excluded === true && !('lat' in rumLastSample) && !('lng' in rumLastSample), 'browser RUM diagnostic must exclude precise lat/lng', rumLastSample);
  const deltaTemplate = appViewport?.viewport_api?.web_session_delta_path_template || '/world/web/map-delta?lat={lat}&lng={lng}&zoom={zoom}&radius_km={radius_km}&limit={limit}&cursor={cursor}';
  const deltaCursor = runtimeDiagnostics?.last_viewport_cursor || appViewport?.delta_cursor || appViewport?.viewport_cursor;
  const deltaUrl = deltaTemplate
    .replaceAll('{lat}', '31.230400')
    .replaceAll('{lng}', '121.473700')
    .replaceAll('{zoom}', '15')
    .replaceAll('{radius_km}', '4.5')
    .replaceAll('{limit}', '6')
    .replaceAll('{cursor}', encodeURIComponent(deltaCursor || ''));
  const deltaNotModifiedProbe = await page.evaluate(async ({ deltaUrl }) => {
    const first = await fetch(deltaUrl, { credentials: 'same-origin', cache: 'no-store' });
    const etag = first.headers.get('etag');
    const second = await fetch(deltaUrl, { credentials: 'same-origin', cache: 'no-store', headers: etag ? { 'if-none-match': etag, 'x-trillionnium-map-if-none-match': etag } : {} });
    return { first_status: first.status, etag, server_timing: first.headers.get('server-timing'), server_ms: first.headers.get('x-trillionnium-world-map-server-ms'), second_status: second.status };
  }, { deltaUrl });
  assert(deltaNotModifiedProbe.first_status === 200 && Boolean(deltaNotModifiedProbe.etag) && Boolean(deltaNotModifiedProbe.server_timing) && Boolean(deltaNotModifiedProbe.server_ms) && deltaNotModifiedProbe.second_status === 304, 'browser map delta 304/server timing contract failed', deltaNotModifiedProbe);
  await page.route('**/world/web/map-delta**', (route) => route.abort('failed'));
  await page.route('**/world/web/map-viewport**', (route) => route.abort('failed'));
  const weakNetworkFallback = await page.evaluate(async () => {
    const viewport = await window.trillionniumRefreshMapViewport?.();
    return {
      viewport_cursor: viewport?.delta_cursor || viewport?.viewport_cursor || null,
      diagnostics: window.trillionniumMapRuntimeDiagnostics,
      rum: window.trillionniumMapRumLastSample,
    };
  });
  assert(Boolean(weakNetworkFallback?.viewport_cursor), 'weak-network fallback did not reuse cached viewport', weakNetworkFallback);
  assert(weakNetworkFallback?.diagnostics?.cached_snapshot_available === true, 'weak-network fallback diagnostics lost cached viewport', weakNetworkFallback);
  assert(weakNetworkFallback?.rum?.sample_kind === 'weak_network_cached_snapshot', 'weak-network fallback RUM sample missing', weakNetworkFallback);
  await page.unroute('**/world/web/map-delta**');
  await page.unroute('**/world/web/map-viewport**');
  routeRunnerHandoffCoverage.app_feed_contract = assertRouteRunnerHandoffContract(appJson?.feed?.route_runner_handoff, '/app feed JSON');
  routeRunnerHandoffCoverage.app_map_hub_contract = assertRouteRunnerHandoffContract(appJson?.map_hub?.route_runner_handoff, '/app map_hub JSON');
  routeRunnerHandoffCoverage.app_route_summary_dom = await assertRouteRunnerHandoffDom(page, '#app-route-runner-handoff-summary', '/app route summary');
  assert(await count(page, '#app-tactics-player-hud[data-contract-version="trillionnium_tactics_player_visible_surface_v1"]') === 1, 'app tactics player HUD contract missing');
  assert(await count(page, '#app-tactics-objective-card[data-session-contract="trillionnium_tactics_game_session_v1"]') === 1, 'app tactics objective card missing');
  assert(await count(page, '#app-tactics-current-session-card[data-tick-contract="trillionnium_tactics_simulation_tick_v1"]') === 1, 'app tactics current session card missing');
  assert(await count(page, '#app-tactics-reward-history-handoff[data-reward-history-contract="trillionnium_tactics_reward_history_v1"]') === 1, 'app tactics reward-history handoff missing');
  assert(await count(page, '#app-tactics-repeat-farming-copy[data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"]') === 1, 'app tactics repeat-farming copy missing');
  assert(await count(page, '#app-mobile-action-sheet[data-contract-version="trillionnium_mobile_single_primary_cta_v1"]') === 1, 'mobile bottom action sheet contract missing');
  assert(await count(page, '#app-mobile-action-sheet [data-primary-cta]') === 1, 'mobile bottom action sheet must expose exactly one primary CTA');
  assert(await page.locator('#app-mobile-primary-cta').first().getAttribute('data-primary-cta-target') === 'app-map-action-rail', 'mobile primary CTA target mismatch');
  assert(await count(page, '#app-map-copy-summary[data-contract-version="trillionnium_mobile_copy_layering_v1"]') === 1, 'mobile copy layering summary contract missing');
  assert(await count(page, '#app-map-copy-layer-details[data-contract-version="trillionnium_mobile_copy_layering_v1"][data-default-state="collapsed"]') === 1, 'mobile copy layering collapsed details contract missing');
  assert(await page.locator('#app-map-copy-layer-details').first().evaluate((el) => el.open) === false, 'mobile copy layering details must default collapsed');
  const mobileCopySummary = await page.locator('#app-map-copy-summary').first().innerText({ timeout: 10_000 });
  assert(mobileCopySummary.length <= 150 && mobileCopySummary.includes('Pick a nearby route'), 'mobile copy summary must be short and action-first', mobileCopySummary);
  const mobileCtaText = await page.locator('#app-mobile-action-sheet').first().innerText({ timeout: 10_000 });
  for (const needle of ['Continue Route', 'runner', 'reward', 'next-route']) {
    assert(mobileCtaText.includes(needle), `mobile bottom action sheet copy missing: ${needle}`, mobileCtaText);
  }
  const mobileShellChecks = appJson?.mobile_shell_contract?.readiness_checks || [];
  assert(appJson?.mobile_shell_contract?.contract_version === 'trillionnium_mobile_shell_ux_v1', 'mobile shell UX contract missing from client app json', appJson?.mobile_shell_contract);
  assert(appJson?.mobile_shell_contract?.primary_cta?.contract_version === 'trillionnium_mobile_single_primary_cta_v1', 'mobile single primary CTA JSON contract missing', appJson?.mobile_shell_contract?.primary_cta);
  assert(appJson?.mobile_shell_contract?.primary_cta?.single_primary_cta === true, 'mobile single primary CTA JSON single flag missing', appJson?.mobile_shell_contract?.primary_cta);
  assert(appJson?.mobile_shell_contract?.copy_layering?.contract_version === 'trillionnium_mobile_copy_layering_v1', 'mobile copy layering JSON contract missing', appJson?.mobile_shell_contract?.copy_layering);
  assert(appJson?.mobile_shell_contract?.copy_layering?.default_state === 'collapsed', 'mobile copy layering JSON default state missing', appJson?.mobile_shell_contract?.copy_layering);
  for (const expectedCheck of ['mobile_tablist_a11y_visible', 'keyboard_tab_navigation_visible', 'search_empty_state_visible', 'search_clear_and_escape_visible', 'aria_live_ux_status_visible', 'offline_feed_fallback_status_visible', 'web_session_feed_hydration_visible', 'mobile_bottom_sheet_single_primary_cta_visible', 'mobile_copy_layering_visible', 'map_rum_slo_quantiles_visible', 'map_weak_network_resilience_visible', 'map_location_privacy_visible']) {
    assert(mobileShellChecks.includes(expectedCheck), `mobile shell UX readiness check missing: ${expectedCheck}`, mobileShellChecks);
  }
  assert(appJson?.feed?.web_session_path === '/app/web/feed', 'web session feed hydration path missing', appJson?.feed);
  steps.push({ name: 'app_mobile_shell_ux_and_handoff_contract_json', ok: true });

  for (const tab of ['messages', 'feed', 'me', 'map']) {
    await activateTab(page, tab);
    steps.push({ name: `app_mobile_tab_${tab}`, ok: true });
  }

  await activateTab(page, 'feed');
  await page.waitForFunction(() => {
    const status = document.querySelector('#app-feed-api-status')?.textContent || '';
    const ux = document.querySelector('#app-ux-status-pill')?.textContent || '';
    return /动态已同步|动态备用快照|内置动态快照|Feed synced|Fallback feed snapshot|Feed API (synced|fallback)|Embedded feed snapshot/.test(status + ' ' + ux);
  }, { timeout: 15_000 });
  assert(await count(page, '#app-feed-items-live .app-feed-item, #app-feed-items-live article') >= 1, 'feed cards missing after API hydration');
  routeRunnerHandoffCoverage.app_feed_chip_dom = await assertRouteRunnerHandoffDom(page, '#app-feed-route-runner-handoff', '/app feed chip');
  assert(appJson?.feed?.web_session_path === '/app/web/feed', 'feed hydration did not expose web-session feed path');
  const filterCount = await count(page, '.trillionnium-app-feed-filter');
  if (filterCount > 0) {
    await clickOrDomActivate(page.locator('.trillionnium-app-feed-filter').first());
  }
  steps.push({ name: 'app_feed_api_hydration_and_filter', ok: true, filters: filterCount });

  await activateTab(page, 'map');
  const focusButtons = await count(page, '.trillionnium-map-focus');
  if (focusButtons > 0) {
    await clickOrDomActivate(page.locator('.trillionnium-map-focus').first());
    await page.waitForFunction(() => {
      const text = document.querySelector('#app-map-focus-summary')?.textContent || '';
      return text && !/Waiting|Pick/i.test(text);
    }, { timeout: 10_000 }).catch(() => null);
  }
  const appMapBox = await page.locator('#real-world-map').boundingBox({ timeout: 10_000 });
  const appMapPanelBox = await page.locator('#app-tab-map .map-panel').boundingBox({ timeout: 10_000 });
  const appOnboardingBox = await page.locator('#app-first-playable-onboarding').boundingBox({ timeout: 10_000 });
  assert(appMapBox && appMapPanelBox && appMapBox.y <= appMapPanelBox.y, '/app mobile should show the real map before dense map copy', { appMapBox, appMapPanelBox });
  assert(appMapBox && appOnboardingBox && appMapBox.y < appOnboardingBox.y, '/app mobile onboarding should not push the map below the first tab screen', { appMapBox, appOnboardingBox });
  steps.push({ name: 'app_real_map_focus_controls', ok: true, focus_buttons: focusButtons });
  await page.screenshot({ path: path.join(screenshotDir, 'app-mobile-feed-map.png'), fullPage: true }).catch((error) => {
    consoleMessages.push({ type: 'warning', text: `app screenshot skipped: ${error.message || error}` });
  });

  await page.goto('/league?lang=en', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'en', { timeout: 10_000 });
  assert(await count(page, '#league-language-switcher [data-trillionnium-language-select]') === 1, 'league visible language switcher missing');
  const leagueEnglishText = await page.locator('body').innerText({ timeout: 10_000 });
  for (const needle of ['Trillionnium League', 'Playable Now', 'Web Battle Console']) {
    assert(leagueEnglishText.includes(needle), `league English copy missing: ${needle}`);
  }
  await assertNoVisibleBilingualSlashPair(page, '/league English system language');
  await page.locator('#trillionnium-league-language-select').selectOption('zh');
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'zh', { timeout: 10_000 });
  const leagueChineseText = await page.locator('body').innerText({ timeout: 10_000 });
  for (const needle of ['当前可玩版本', '网页战斗台', '公会大厅']) {
    assert(leagueChineseText.includes(needle), `league Chinese copy missing: ${needle}`);
  }
  await assertNoVisibleBilingualSlashPair(page, '/league Chinese system language');
  steps.push({ name: 'league_global_language_switcher_runtime', ok: true });

  await page.goto('/world?lang=en', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-real-map', { timeout: 15_000 });
  assert((await page.title()).includes('Trillionnium World'), 'world title missing');
  const worldBodyText = await page.locator('body').innerText({ timeout: 10_000 });
  for (const needle of ['Global-first open world', 'World Action Console', 'Bounties', 'Submit']) {
    assert(worldBodyText.includes(needle), `world English/global-first copy missing: ${needle}`);
  }
  assert(await count(page, '#world-mobile-first-screen') === 1, 'world mobile-first hero missing');
  routeRunnerHandoffCoverage.world_route_summary_dom = await assertRouteRunnerHandoffDom(page, '#world-route-runner-handoff-summary', '/world route summary');
  assert(await count(page, '#world-language-switcher [data-trillionnium-language-select]') === 1, 'world visible language switcher missing');
  assert(await count(page, '#world-hero-mobile-actions[data-contract-version="trillionnium_mobile_single_primary_cta_v1"][data-primary-cta-count="1"]') === 1, 'world mobile single-primary CTA contract missing');
  assert(await count(page, '#world-mobile-primary-cta.cta') === 1, 'world mobile primary CTA missing');
  assert(await count(page, '#world-mobile-route-first-sheet .world-route-stepper span') === 3, 'world mobile route-first stepper missing');
  assert(await count(page, '#world-tactics-player-hud[data-contract-version="trillionnium_tactics_player_visible_surface_v1"]') === 1, 'world tactics player HUD contract missing');
  assert(await count(page, '#world-tactics-objective-card[data-session-contract="trillionnium_tactics_game_session_v1"]') === 1, 'world tactics objective card missing');
  assert(await count(page, '#world-tactics-current-session-card[data-tick-contract="trillionnium_tactics_simulation_tick_v1"]') === 1, 'world tactics current session card missing');
  assert(await count(page, '#world-tactics-reward-history-handoff[data-reward-history-contract="trillionnium_tactics_reward_history_v1"]') === 1, 'world tactics reward-history handoff missing');
  assert(await count(page, '#world-tactics-repeat-farming-copy[data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"]') === 1, 'world tactics repeat-farming copy missing');
  assert(await count(page, '#world-pulse-strip .pulse-card') === 5, 'world pulse strip should keep only compact primary counters visible');
  assert(await count(page, '#world-stats-compact-more .stat') >= 12, 'world compact stats drawer missing secondary counters');
  const pulseBox = await page.locator('#world-pulse-strip').boundingBox({ timeout: 10_000 });
  assert(pulseBox && pulseBox.height < 360, 'world mobile stats area is too tall', pulseBox);
  await assertNoVisibleBilingualSlashPair(page, '/world English system language');
  await assertEnglishSurfaceHasNoCoreChineseLeaks(page, '/world English system language');
  await page.goto('/world?lang=zh', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-real-map', { timeout: 15_000 });
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'zh', { timeout: 10_000 });
  const worldChineseText = await page.locator('body').innerText({ timeout: 10_000 });
  for (const needle of ['世界行动台', '悬赏', '提交成果']) {
    assert(worldChineseText.includes(needle), `world Chinese language copy missing: ${needle}`);
  }
  await assertNoVisibleBilingualSlashPair(page, '/world Chinese system language');
  await page.goto('/world?lang=en', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-real-map', { timeout: 15_000 });
  assert(await count(page, '#world-map-move-panel') === 1, 'world map move panel missing');
  assert(await count(page, '#world-buy-form') === 1, 'world buy form missing');
  steps.push({ name: 'world_boot_real_map_and_quest_forms', ok: true });

  const worldFocusButtons = await count(page, '.trillionnium-map-focus');
  if (worldFocusButtons > 0) {
    await clickOrDomActivate(page.locator('.trillionnium-map-focus').first());
  }
  await submitWorldForm(page, '#world-map-move-panel form', marker, 'map=moved');
  steps.push({ name: 'world_map_move_form_browser_submit', ok: true });

  await page.locator('#world-action-body').fill('打造一个现实委托可用的 AI 设计工坊道具：写清成果、证据包、风险控制、行动循环、下一步和自检记录，用于 browser adventure E2E。');
  await submitWorldForm(page, 'form[action="/world/web/action"]', marker, 'played=1');
  steps.push({ name: 'world_action_browser_submit', ok: true });

  await page.locator('#world-company-body').fill('建立一个 AI 设计工坊：写清客户交付方案、委托目标画像、可提交成果、证据来源包、风险控制、行动循环、下一步计划和自检记录。');
  await submitWorldForm(page, 'form[action="/world/web/company"]', marker, 'company=created');
  steps.push({ name: 'world_company_browser_submit', ok: true });

  await page.locator('#world-listing-body').fill('发布一个工坊任务牌：写清成果、赏金逻辑、证据包、承诺、风险控制、下一步行动和自检记录。');
  await submitWorldForm(page, 'form[action="/world/web/listing"]', marker, 'listing=created');
  steps.push({ name: 'world_listing_browser_submit', ok: true });

  await submitWorldForm(page, '#world-buy-form', marker, 'purchase=created');
  const purchaseCards = await count(page, '#world-purchase-cards-live article, #world-purchase-cards-live .mini');
  assert(purchaseCards >= 1, 'quest accept card missing after accepting quest board');
  steps.push({ name: 'world_quest_accept_browser_submit', ok: true, purchase_cards: purchaseCards });

  await page.locator('#world-work-deliver-body').evaluate((node) => {
    node.value = 'Browser quest delivery: deliver a customer-ready方案 with evidence/source data, risk controls, self-review, next action plan, acceptance checklist, and concrete result notes. 提交可交付方案：成果、证据包、风险控制、自评复盘、下一步计划、验收清单和真实结果记录。';
    node.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await submitWorldForm(page, '#world-work-deliver-form', marker, 'work=delivered');
  const deliveryCards = await count(page, '#world-work-deliveries-live article, #world-work-deliveries-live .mini');
  assert(deliveryCards >= 1, 'quest result card missing after submit');
  steps.push({ name: 'world_quest_result_submit_browser_submit', ok: true, delivery_cards: deliveryCards });

  await submitWorldForm(page, '#world-work-accept-form', marker, 'work=accepted');
  const acceptanceCards = await count(page, '#world-work-acceptances-live article, #world-work-acceptances-live .mini');
  assert(acceptanceCards >= 1, 'quest rating card missing after rating');
  steps.push({ name: 'world_quest_rating_browser_submit', ok: true, acceptance_cards: acceptanceCards });

  const health = await page.request.get(`${baseUrl}/health`, { timeout: 20_000 });
  assert(health.ok(), `health failed after browser flow: ${health.status()}`);
  const healthJson = await health.json();
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.contract_version === 'trillionnium_world_map_runtime_safety_gate_v1', 'health runtime safety gate contract missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.rum_slo_quantiles_visible === true, 'health runtime safety RUM SLO quantile gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.weak_network_cached_snapshot_visible === true, 'health runtime safety weak-network gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.rum_excludes_lat_lng === true, 'health runtime safety location privacy gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_rum_slo_gate?.contract_version === 'trillionnium_world_map_rum_slo_v1' && healthJson?.trillionnium_world_map_rum_slo_gate?.green === true, 'health RUM SLO metrics gate not green', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(typeof healthJson?.trillionnium_world_map_rum_slo_gate?.raw_split_green === 'boolean', 'health RUM SLO raw split verdict must stay visible during warmup', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(Number.isFinite(Number(healthJson?.trillionnium_world_map_rum_slo_gate?.sample_count)), 'health RUM SLO sample count missing', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(['warming_until_min_samples', 'enforced'].includes(healthJson?.trillionnium_world_map_rum_slo_gate?.enforcement_status), 'health RUM SLO enforcement status missing', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(healthJson?.trillionnium_world_map_delta_cache_gate?.entity_delta_cache_contract === 'entity_group_versioned_delta_v1' && healthJson?.trillionnium_world_map_delta_cache_gate?.failure_rate_within_target === true, 'health delta cache gate not green', healthJson?.trillionnium_world_map_delta_cache_gate);
  const metrics = await page.request.get(`${baseUrl}/metrics`, { timeout: 20_000 });
  assert(metrics.ok(), `metrics failed after browser flow: ${metrics.status()}`);
  const metricsText = await metrics.text();
  for (const needle of [
    'cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green 1',
    'cex_consumer_entry_trillionnium_world_map_rum_slo_raw_split_green ',
    'cex_consumer_entry_trillionnium_world_map_rum_slo_sample_count ',
    'cex_consumer_entry_trillionnium_world_map_rum_slo_enforcement_active ',
    'cex_consumer_entry_trillionnium_world_map_rum_slo_warming ',
    'cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green 1',
    'cex_consumer_entry_trillionnium_world_map_runtime_safety_gate_green 1',
    'cex_consumer_entry_trillionnium_world_map_weak_network_resilience_gate_green 1',
    'cex_consumer_entry_trillionnium_world_map_location_privacy_gate_green 1',
  ]) {
    assert(metricsText.includes(needle), `metrics runtime safety gauge missing: ${needle}`);
  }
  if (expectFinalCutover) {
    assert(healthJson?.league_repository_runtime?.effective_repository === 'normalized_sql_direct_write_final', 'browser e2e did not run against final repository', healthJson?.league_repository_runtime);
    assert(healthJson?.league_repository_runtime?.repository_cutover_status === 'normalized_sql_direct_write_final_cutover_active', 'browser e2e did not run against final cutover', healthJson?.league_repository_runtime);
    steps.push({ name: 'health_final_cutover_after_browser_flow', ok: true });
  } else {
    steps.push({
      name: 'health_final_cutover_after_browser_flow',
      ok: true,
      mode: 'not_required_by_TRILLIONNIUM_BROWSER_E2E_EXPECT_FINAL_CUTOVER',
      effective_repository: healthJson?.league_repository_runtime?.effective_repository,
      repository_cutover_status: healthJson?.league_repository_runtime?.repository_cutover_status,
    });
  }

  await page.screenshot({ path: path.join(screenshotDir, 'world-quest-rated.png'), fullPage: false, timeout: 15_000, animations: 'disabled' }).catch((error) => {
    consoleMessages.push({ type: 'warning', text: `world screenshot skipped: ${error.message || error}` });
  });

  const summary = {
    ok: pageErrors.length === 0,
    run_id: runId,
    checked_at_epoch: Math.floor(Date.now() / 1000),
    base_url: baseUrl,
    browser: 'playwright.chromium',
    executable_path: executablePath,
    viewport: 'iPhone 13',
    coverage: {
      app: true,
      world: true,
      mobile_tabs: true,
      mobile_tab_a11y: true,
      mobile_shell_contract: true,
      mobile_keyboard_tab_navigation: true,
      mobile_search_empty_and_clear_state: true,
      mobile_ux_live_status: true,
      real_world_map: true,
      feed_api_hydration: true,
      route_runner_handoff_contract: true,
      route_runner_handoff_dom: true,
      world_map_move: true,
      world_buy: true,
      world_work_deliver: true,
      world_work_accept: true,
      normalized_sql_final_cutover: true,
    },
    web_session: {
      seeded: true,
      matrix_user_id: matrixUserId,
      room_id: roomId,
      session_id: sessionId,
      expires_at_epoch: webSession.expires_at_epoch,
    },
    steps,
    route_runner_handoff: routeRunnerHandoffCoverage,
    console_messages: consoleMessages.slice(0, 20),
    page_errors: pageErrors,
    request_failures: requestFailures.slice(0, 20),
    screenshots_dir: screenshotDir,
    summary_path: summaryPath,
  };
  await fs.writeFile(summaryPath, JSON.stringify(summary, null, 2));
  await browser.close();
  console.log(JSON.stringify(summary, null, 2));
  if (!summary.ok) process.exit(2);
}

main().catch(async (error) => {
  const summary = {
    ok: false,
    checked_at_epoch: Math.floor(Date.now() / 1000),
    base_url: baseUrl,
    error: String(error && error.stack || error),
    details: error?.details,
    summary_path: summaryPath,
  };
  await fs.mkdir(outDir, { recursive: true });
  await fs.writeFile(summaryPath, JSON.stringify(summary, null, 2));
  console.error(JSON.stringify(summary, null, 2));
  process.exit(1);
});
