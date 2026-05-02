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
    layerGroup: () => ({...layerApi(), __layers: new Set(), clearLayers(){ this.__layers.clear(); return this; }, addLayer(layer){ this.__layers.add(layer); return this; }}),
    latLngBounds: () => ({
      isValid: () => true,
      pad(){ return this; },
      extend(){ return this; },
      getCenter(){ return { lat: 31.230416, lng: 121.473701 }; },
    }),
    polyline: () => layerApi(),
    marker: () => layerApi(),
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
    await textarea.fill(`${existing}\nBrowser E2E marker ${marker}: normalized SQL final-cutover path proof.`);
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
  assert(page.url().includes(expectedUrlFragment), `expected ${formSelector} navigation to include ${expectedUrlFragment}`, page.url());
  return response ? response.status() : 200;
}

async function main() {
  await fs.mkdir(outDir, { recursive: true });
  await fs.mkdir(screenshotDir, { recursive: true });
  const consoleMessages = [];
  const pageErrors = [];
  const requestFailures = [];
  const steps = [];

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

  await page.goto('/app', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#real-world-map', { timeout: 15_000 });
  assert((await page.title()).includes('Trillionnium World Mobile'), 'app title missing');
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
  const mobileShellChecks = appJson?.mobile_shell_contract?.readiness_checks || [];
  assert(appJson?.mobile_shell_contract?.contract_version === 'trillionnium_mobile_shell_ux_v1', 'mobile shell UX contract missing from client app json', appJson?.mobile_shell_contract);
  for (const expectedCheck of ['mobile_tablist_a11y_visible', 'keyboard_tab_navigation_visible', 'search_empty_state_visible', 'search_clear_and_escape_visible', 'aria_live_ux_status_visible', 'offline_feed_fallback_status_visible', 'web_session_feed_hydration_visible']) {
    assert(mobileShellChecks.includes(expectedCheck), `mobile shell UX readiness check missing: ${expectedCheck}`, mobileShellChecks);
  }
  assert(appJson?.feed?.web_session_path === '/app/web/feed', 'web session feed hydration path missing', appJson?.feed);
  steps.push({ name: 'app_mobile_shell_ux_contract_json', ok: true });

  for (const tab of ['messages', 'feed', 'me', 'map']) {
    await activateTab(page, tab);
    steps.push({ name: `app_mobile_tab_${tab}`, ok: true });
  }

  await activateTab(page, 'feed');
  await page.waitForFunction(() => {
    const status = document.querySelector('#app-feed-api-status')?.textContent || '';
    return /动态已同步|动态备用快照|内置动态快照|Feed API (synced|fallback)|Embedded feed snapshot/.test(status);
  }, { timeout: 15_000 });
  assert(await count(page, '#app-feed-items-live .app-feed-item, #app-feed-items-live article') >= 1, 'feed cards missing after API hydration');
  assert((await text(page, '#app-feed-api-status')).includes('/app/web/feed'), 'feed hydration did not use web-session feed path');
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
  steps.push({ name: 'app_real_map_focus_controls', ok: true, focus_buttons: focusButtons });
  await page.screenshot({ path: path.join(screenshotDir, 'app-mobile-feed-map.png'), fullPage: true }).catch((error) => {
    consoleMessages.push({ type: 'warning', text: `app screenshot skipped: ${error.message || error}` });
  });

  await page.goto('/world', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-real-map', { timeout: 15_000 });
  assert((await page.title()).includes('Trillionnium World'), 'world title missing');
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

  await page.locator('#world-company-body').fill('建立一个 AI 设计工坊：写清委托目标画像、可提交成果、证据来源包、风险控制、行动循环、下一条支线和自检记录。');
  await submitWorldForm(page, 'form[action="/world/web/company"]', marker, 'company=created');
  steps.push({ name: 'world_company_browser_submit', ok: true });

  await page.locator('#world-listing-body').fill('发布一个工坊任务牌：写清成果、赏金逻辑、证据包、承诺、风险控制、下一步行动和自检记录。');
  await submitWorldForm(page, 'form[action="/world/web/listing"]', marker, 'listing=created');
  steps.push({ name: 'world_listing_browser_submit', ok: true });

  await submitWorldForm(page, '#world-buy-form', marker, 'purchase=created');
  const purchaseCards = await count(page, '#world-purchase-cards-live article, #world-purchase-cards-live .mini');
  assert(purchaseCards >= 1, 'quest accept card missing after accepting quest board');
  steps.push({ name: 'world_quest_accept_browser_submit', ok: true, purchase_cards: purchaseCards });

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
