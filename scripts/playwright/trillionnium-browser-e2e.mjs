import { createRequire } from 'node:module';
import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { chromium, devices, request } = require('playwright');

const baseUrl = (process.env.CONSUMER_ENTRY_BASE_URL || process.env.BASE_URL || 'http://127.0.0.1:8090').replace(/\/$/, '');
const rootDir = process.env.CEX_PROJECT_ROOT || process.cwd();
const outDir = process.env.TRILLIONNIUM_BROWSER_E2E_OUT_DIR || path.join(rootDir, 'run', 'league-browser');
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || process.env.CHROME_BIN || '/usr/bin/google-chrome-stable';
const expectFinalCutover = process.env.TRILLIONNIUM_BROWSER_E2E_EXPECT_FINAL_CUTOVER !== '0';
const ingressToken = (process.env.CONSUMER_ENTRY_INGRESS_TOKEN || '').trim();
const e2eMode = process.env.TRILLIONNIUM_BROWSER_E2E_MODE || 'full-browser-e2e';
const runId = `${Math.floor(Date.now() / 1000)}-${process.pid}`;
const defaultMatrixUserId = e2eMode === 'first-human-session' ? `@browser-first-human-${runId}:local.dev` : '@alice:local.dev';
const matrixUserId = process.env.TRILLIONNIUM_BROWSER_E2E_MATRIX_USER_ID || defaultMatrixUserId;
const roomId = process.env.TRILLIONNIUM_BROWSER_E2E_ROOM_ID || '!browser-local:local.dev';
const sessionId = process.env.TRILLIONNIUM_BROWSER_E2E_SESSION_ID || (e2eMode === 'first-human-session' ? `browser-first-human-${runId}` : 'browser-e2e-session');
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
  const ensureAttribution = (target) => {
    const container = typeof target === 'string' ? document.getElementById(target) : target;
    if (!container || container.querySelector('.leaflet-control-attribution')) return;
    const attribution = document.createElement('div');
    attribution.className = 'leaflet-control-attribution';
    attribution.textContent = 'Leaflet | © OpenStreetMap contributors';
    container.appendChild(attribution);
  };
  const makeMap = (target) => {
    ensureAttribution(target);
    return ({
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
  };
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

function classifyRequestFailure(record) {
  const urlText = String(record?.url || '');
  const failure = String(record?.failure || 'requestfailed');
  let parsed = null;
  try {
    parsed = new URL(urlText);
  } catch (_error) {
    parsed = null;
  }
  const pathName = parsed?.pathname || '';
  const isLocalConsumer = parsed && parsed.origin === baseUrl;
  const isWorldMapDelta = isLocalConsumer && pathName === '/world/web/map-delta';
  const isWorldMapViewport = isLocalConsumer && pathName === '/world/web/map-viewport';
  if (isWorldMapDelta && failure === 'net::ERR_ABORTED') {
    return {
      ...record,
      allowed: true,
      classification: 'allowed_stale_map_delta_request_aborted',
      gate_reason: 'world map runtime intentionally aborts stale delta fetches when the viewport changes or the page navigates',
    };
  }
  if ((isWorldMapDelta || isWorldMapViewport) && ['net::ERR_ABORTED', 'net::ERR_FAILED'].includes(failure)) {
    return {
      ...record,
      allowed: true,
      classification: 'allowed_world_map_async_request_cancelled_during_route_transition',
      gate_reason: 'late map viewport refresh was cancelled during the scripted route transition after the runtime/health gates stayed green',
    };
  }
  return {
    ...record,
    allowed: false,
    classification: 'unclassified_browser_request_failure',
    gate_reason: 'not on the explicit Browser E2E request-failure allowlist',
  };
}

function buildRequestFailureGate(records) {
  const classified = (records || []).map(classifyRequestFailure);
  const unclassified = classified.filter((failure) => !failure.allowed);
  const allowed = classified.filter((failure) => failure.allowed);
  const allowedByClass = allowed.reduce((acc, failure) => {
    acc[failure.classification] = (acc[failure.classification] || 0) + 1;
    return acc;
  }, {});
  const maxAllowedWorldMapAsyncCancellations = 6;
  const allowedWorldMapAsyncCancellationCount = allowed.length;
  const allowedWithinBudget = allowedWorldMapAsyncCancellationCount <= maxAllowedWorldMapAsyncCancellations;
  return {
    contract_version: 'trillionnium_browser_request_failure_gate_v1',
    green: unclassified.length === 0 && allowedWithinBudget,
    policy: 'fail_on_unclassified_request_failures_allow_only_known_world_map_async_cancellations',
    total_count: classified.length,
    allowed_count: allowed.length,
    unclassified_count: unclassified.length,
    allowed_within_budget: allowedWithinBudget,
    allowed_world_map_async_cancellation_count: allowedWorldMapAsyncCancellationCount,
    max_allowed_world_map_async_cancellations: maxAllowedWorldMapAsyncCancellations,
    allowed_by_class: allowedByClass,
    classified_failures: classified.slice(0, 20),
    unclassified_failures: unclassified.slice(0, 20),
  };
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

function signedSessionHeaders({ matrixUserId: sessionMatrixUserId = matrixUserId, roomId: sessionRoomId = roomId, sessionId: webSessionId = sessionId } = {}) {
  const secret = process.env.CONSUMER_ENTRY_SESSION_AUTH_SECRET || process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET || '';
  if (!secret.trim()) return {};
  const issuer = process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER || 'matrix-entry-adapter';
  const audience = process.env.CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE || process.env.MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE || 'consumer-entry-api';
  const now = Math.floor(Date.now() / 1000);
  const requestFingerprint = `league-web-session:${sessionMatrixUserId}:${sessionRoomId}:${webSessionId}`;
  const claims = {
    version: 1,
    issuer,
    key_id: null,
    subject: sessionMatrixUserId,
    source_kind: 'league_web_session',
    audience,
    request_fingerprint: requestFingerprint,
    room_id: sessionRoomId,
    session_id: webSessionId,
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

async function seedWebSession(context, { matrixUserId: sessionMatrixUserId = matrixUserId, roomId: sessionRoomId = roomId, sessionId: webSessionId = sessionId } = {}) {
  const sessionHeaders = { ...ingressHeaders(), ...signedSessionHeaders({ matrixUserId: sessionMatrixUserId, roomId: sessionRoomId, sessionId: webSessionId }) };
  const sessionApiContext = await request.newContext({ baseURL: baseUrl, extraHTTPHeaders: sessionHeaders, userAgent: 'cex-browser-e2e-session-probe/1.0' });
  const response = await sessionApiContext.post('/league/web/session', {
    data: { matrix_user_id: sessionMatrixUserId, room_id: sessionRoomId, session_id: webSessionId },
    timeout: 20_000,
  });
  const bodyText = await response.text();
  assert(response.ok(), `failed to seed browser web session: ${response.status()}`, bodyText);
  const cookieHeaders = (await response.headersArray()).filter((header) => header.name.toLowerCase() === 'set-cookie');
  const cookies = cookieHeaders
    .map((header) => {
      const pair = String(header.value || '').split(';')[0] || '';
      const equalsIndex = pair.indexOf('=');
      if (equalsIndex <= 0) return null;
      return {
        name: pair.slice(0, equalsIndex),
        value: pair.slice(equalsIndex + 1),
        url: baseUrl,
      };
    })
    .filter(Boolean);
  if (cookies.length > 0) await context.addCookies(cookies);
  await sessionApiContext.dispose().catch(() => null);
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

async function setWorldInputValue(page, selector, value) {
  const input = page.locator(selector).first();
  await input.evaluate((node, nextValue) => {
    node.value = nextValue;
    node.dispatchEvent(new Event('input', { bubbles: true }));
    node.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

async function extractWorldListingIdByMarker(page, markerText) {
  const listingId = await page.$$eval('article.mini.listing', (nodes, marker) => {
    const match = nodes.find((node) => (node.textContent || '').includes(marker));
    return match?.querySelector('code')?.textContent?.trim() || null;
  }, markerText);
  assert(/^world-listing-/.test(String(listingId || '')), 'created world listing id not found for browser marker', { markerText, listingId });
  return listingId;
}

async function extractWorldWorkOrderIdByMarker(page, markerText) {
  const markerPrefix = String(markerText).slice(0, 48);
  const workOrderId = await page.$$eval('article.mini.work[data-work-order-id]', (nodes, marker) => {
    const match = nodes.find((node) => (node.textContent || '').includes(marker));
    return match?.dataset?.workOrderId || null;
  }, markerPrefix);
  assert(/^world-work-/.test(String(workOrderId || '')), 'created world work order id not found for browser marker', { markerText, markerPrefix, workOrderId });
  return workOrderId;
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

async function moveWorldKeypadToNode(page, targetNodeId) {
  await page.waitForFunction(() => {
    const runtime = window.trillionniumKeyboardMap?.getState?.();
    const domCurrent = document.querySelector('#world-keypad-map-grid')?.dataset?.currentNodeId;
    return Boolean(runtime?.currentNodeId && domCurrent && runtime.currentNodeId === domCurrent);
  }, undefined, { timeout: 10_000 });
  const path = await page.evaluate((target) => {
    const state = window.trillionniumKeyboardMap?.getState?.();
    const nodes = state?.nodes || {};
    const start = state?.currentNodeId;
    if (!start || !nodes[start] || !nodes[target]) return null;
    if (start === target) return [];
    const directionKeys = {
      east: '6', e: '6',
      west: '4', w: '4',
      north: '8', n: '8',
      south: '2', s: '2',
      'north-east': '9', northeast: '9', ne: '9',
      'north-west': '7', northwest: '7', nw: '7',
      'south-east': '3', southeast: '3', se: '3',
      'south-west': '1', southwest: '1', sw: '1',
    };
    const queue = [{ nodeId: start, steps: [] }];
    const seen = new Set([start]);
    while (queue.length > 0) {
      const current = queue.shift();
      const exits = nodes[current.nodeId]?.exits || {};
      for (const [direction, nextNodeId] of Object.entries(exits)) {
        const key = directionKeys[String(direction).toLowerCase()];
        if (!key || !nodes[nextNodeId] || seen.has(nextNodeId)) continue;
        const steps = [...current.steps, { key, direction, targetNodeId: nextNodeId }];
        if (nextNodeId === target) return steps;
        seen.add(nextNodeId);
        queue.push({ nodeId: nextNodeId, steps });
      }
    }
    return null;
  }, targetNodeId);
  assert(Array.isArray(path), `world keypad cannot route to ${targetNodeId}`, { targetNodeId, path });
  for (const step of path) {
    const ok = await page.evaluate((key) => window.trillionniumKeyboardMap?.move?.(key, 'browser-e2e-route'), step.key);
    assert(ok === true, 'world keypad route step failed', step);
    await page.waitForFunction((expected) => window.trillionniumKeyboardMap?.getState?.().currentNodeId === expected, step.targetNodeId, { timeout: 10_000 });
  }
  return path;
}

async function assertWorldFirstHumanScreen(page, label) {
  assert(await count(page, '#world-first-human-loop[data-contract-version="trillionnium_first_human_session_v1"][data-visible-question-count="4"]') === 1, `${label} first-human four-question card missing`);
  const questionIds = await page.$$eval('#world-first-human-loop [data-first-human-question]', (nodes) => nodes.map((node) => node.dataset.firstHumanQuestion));
  assert(JSON.stringify(questionIds) === JSON.stringify(['who', 'where', 'click', 'reward']), `${label} first-human question order drifted`, questionIds);
  const firstHumanText = await text(page, '#world-first-human-loop');
  for (const needle of ['Who', 'Where', 'Click', 'Reward']) {
    assert(firstHumanText.includes(needle), `${label} first-human cue missing: ${needle}`, firstHumanText);
  }
  const viewport = page.viewportSize() || { height: 844, width: 390 };
  const ctaBox = await page.locator('#world-mobile-primary-cta').boundingBox({ timeout: 10_000 });
  const firstHumanBox = await page.locator('#world-first-human-loop').boundingBox({ timeout: 10_000 });
  assert(firstHumanBox && firstHumanBox.y < viewport.height * 0.72, `${label} first-human card must appear inside the first mobile screen`, { firstHumanBox, viewport });
  assert(ctaBox && ctaBox.y < viewport.height * 0.92, `${label} primary CTA must stay reachable on the first mobile screen`, { ctaBox, viewport });
  return {
    contract_version: 'trillionnium_first_human_session_v1',
    first_screen_contract: 'trillionnium_world_first_screen_four_questions_v1',
    question_order: questionIds,
    cta_y: ctaBox?.y ?? null,
    first_human_y: firstHumanBox?.y ?? null,
    viewport_height: viewport.height,
  };
}

async function submitTacticsDraft(page, expectedUrlFragment) {
  const form = page.locator('#world-tactics-command-draft-form').first();
  await form.scrollIntoViewIfNeeded({ timeout: 10_000 });
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
  assert(page.url().includes(expectedUrlFragment), `expected tactics draft navigation to include ${expectedUrlFragment}`, { url: page.url(), status: response ? response.status() : null });
  return response ? response.status() : 200;
}

async function runFirstHumanSessionPath(page, marker, steps, consoleMessages) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/world?lang=en&first_human_session=1', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-real-map', { timeout: 15_000 });
  await page.waitForFunction(() => document.documentElement.getAttribute('data-ui-language') === 'en', { timeout: 10_000 });
  const firstScreen = await assertWorldFirstHumanScreen(page, 'first human session');
  steps.push({ name: 'first_human_session_first_screen_four_questions', ok: true, ...firstScreen });

  await clickOrDomActivate(page.locator('#world-mobile-primary-cta').first());
  await page.waitForFunction(() => {
    const shell = document.querySelector('#trillionnium-tactics-game-shell');
    if (!shell) return false;
    const rect = shell.getBoundingClientRect();
    return location.hash === '#trillionnium-tactics-game-shell' || rect.top < window.innerHeight;
  }, { timeout: 10_000 });
  steps.push({ name: 'first_human_session_enter_tactics_board', ok: true });

  await clickOrDomActivate(page.locator('.tactics-unit[data-unit="lord"][data-side="player"]').first());
  await clickOrDomActivate(page.locator('.tactics-tile[data-tile="G8"]').first());
  await clickOrDomActivate(page.locator('.tactics-command[data-command="train_skill"]').first());
  const trainingDraft = await page.evaluate(() => ({
    command: document.querySelector('#world-tactics-command-draft-form [name="command"]')?.value,
    targetTile: document.querySelector('#world-tactics-command-draft-form [name="target_tile"]')?.value,
    unitId: document.querySelector('#world-tactics-command-draft-form [name="unit_id"]')?.value,
    skillId: document.querySelector('#world-tactics-command-draft-form [name="skill_id"]')?.value,
    runtime: window.trillionniumTacticsIntentDraft?.getState?.(),
  }));
  assert(trainingDraft.command === 'train_skill' && trainingDraft.targetTile === 'G8' && trainingDraft.unitId === 'lord', 'first human training draft did not map visible clicks into Rust intent form', trainingDraft);
  await submitTacticsDraft(page, 'tactics=1');
  steps.push({ name: 'first_human_session_complete_training_task', ok: true, command: trainingDraft.command, target_tile: trainingDraft.targetTile, skill_id: trainingDraft.skillId });

  await page.waitForSelector('#trillionnium-tactics-game-shell', { timeout: 15_000 });
  await clickOrDomActivate(page.locator('.tactics-unit[data-unit="lord"][data-side="player"]').first());
  await clickOrDomActivate(page.locator('.tactics-tile[data-tile="F5"]').first());
  await clickOrDomActivate(page.locator('.tactics-command[data-command="attack"]').first());
  const attackDraft = await page.evaluate(() => ({
    command: document.querySelector('#world-tactics-command-draft-form [name="command"]')?.value,
    targetTile: document.querySelector('#world-tactics-command-draft-form [name="target_tile"]')?.value,
    unitId: document.querySelector('#world-tactics-command-draft-form [name="unit_id"]')?.value,
    runtime: window.trillionniumTacticsIntentDraft?.getState?.(),
  }));
  assert(attackDraft.command === 'attack' && attackDraft.targetTile === 'F5' && attackDraft.unitId === 'lord', 'first human attack draft did not map visible clicks into Rust intent form', attackDraft);
  await submitTacticsDraft(page, 'tactics=1');
  steps.push({ name: 'first_human_session_complete_tactics_battle', ok: true, command: attackDraft.command, target_tile: attackDraft.targetTile });

  await page.waitForSelector('#world-action-body', { timeout: 15_000 });
  await page.waitForFunction(() => document.querySelectorAll('.trillionnium-reward-claim-action').length >= 1 && document.querySelectorAll('.trillionnium-next-route-action').length >= 1, { timeout: 15_000 });
  await clickOrDomActivate(page.locator('.trillionnium-reward-claim-action').first());
  const rewardDraft = await page.locator('#world-action-body').inputValue({ timeout: 10_000 });
  assert(/reward|claim|rating|evidence|领取|奖励/i.test(rewardDraft), 'first human reward claim click did not draft a reward/proof action', rewardDraft);
  steps.push({ name: 'first_human_session_claim_reward_draft', ok: true, body_length: rewardDraft.length });

  await clickOrDomActivate(page.locator('.trillionnium-next-route-action').first());
  const nextRouteDraft = await page.locator('#world-action-body').inputValue({ timeout: 10_000 });
  assert(/next route|route|下一|路线/i.test(nextRouteDraft), 'first human next-route click did not draft the next route action', nextRouteDraft);
  await submitWorldForm(page, 'form[action="/world/web/action"]', marker, 'played=1');
  steps.push({ name: 'first_human_session_open_next_route', ok: true, body_length: nextRouteDraft.length });

  await page.screenshot({ path: path.join(screenshotDir, 'first-human-session.png'), fullPage: true, timeout: 15_000, animations: 'disabled' }).catch((error) => {
    consoleMessages.push({ type: 'warning', text: `first human session screenshot skipped: ${error.message || error}` });
  });
  return firstScreen;
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
  const localAdventureMatrixUserId = `@browser-e2e-adventure-${runId}:local.dev`;
  const localAdventureRoomId = `!browser-e2e-adventure-${runId}:local.dev`;
  const localAdventureSessionId = `browser-e2e-adventure-${runId}`;

  if (e2eMode === 'first-human-session') {
    await runFirstHumanSessionPath(page, marker, steps, consoleMessages);
    const requestFailureGate = buildRequestFailureGate(requestFailures);
    const summary = {
      ok: pageErrors.length === 0 && requestFailureGate.green,
      run_id: runId,
      checked_at_epoch: Math.floor(Date.now() / 1000),
      base_url: baseUrl,
      mode: e2eMode,
      browser: 'playwright.chromium',
      executable_path: executablePath,
      viewport: 'iPhone 13',
      coverage: {
        first_human_session: true,
        first_screen_four_questions: true,
        enter_tactics_board: true,
        complete_tactics_battle: true,
        reward_claim_draft: true,
        next_route_opened: true,
      },
      web_session: {
        seeded: true,
        matrix_user_id: matrixUserId,
        room_id: roomId,
        session_id: sessionId,
        expires_at_epoch: webSession.expires_at_epoch,
      },
      steps,
      request_failure_gate: requestFailureGate,
      console_messages: consoleMessages.slice(0, 20),
      page_errors: pageErrors,
      request_failures: requestFailureGate.classified_failures,
      screenshots_dir: screenshotDir,
      summary_path: summaryPath,
    };
    await fs.writeFile(summaryPath, JSON.stringify(summary, null, 2));
    await browser.close();
    console.log(JSON.stringify(summary, null, 2));
    if (!summary.ok) process.exit(2);
    return;
  }

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
  await activateTab(page, 'map');
  await page.waitForSelector('#app-tab-map.is-active', { timeout: 10_000 });

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
    '#app-openstreetmap-attribution[data-contract-version="openstreetmap_attribution_presence_v1"][data-attribution-required="true"][data-attribution-visible="true"][data-database-license="ODbL-1.0"]',
    '#app-map-performance-budget[data-spatial-cache-required="true"][data-virtualized-cards-required="true"][data-adaptive-density-required="true"]',
    '#app-map-readability-lod[data-semantic-legend-required="true"][data-avatar-feedback-required="true"][data-i18n-a11y-required="true"]',
  ]) {
    assert(await count(page, selector) === 1, `app map runtime safety DOM token missing: ${selector}`);
  }
  await page.waitForFunction(() => Array.from(document.querySelectorAll('.leaflet-control-attribution')).some((element) => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' && rect.width > 0 && rect.height > 0 && element.textContent.includes('OpenStreetMap');
  }), { timeout: 15_000 });
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
    const firstJson = await first.clone().json().catch(() => ({}));
    const currentCursor = firstJson?.next_cursor || firstJson?.delta_cursor || '';
    const noopUrl = new URL(deltaUrl, window.location.origin);
    if (currentCursor) noopUrl.searchParams.set('cursor', currentCursor);
    const noop = await fetch(noopUrl.toString(), { credentials: 'same-origin', cache: 'no-store' });
    const noopJson = await noop.clone().json().catch(() => ({}));
    let etag = noop.headers.get('etag');
    let secondStatus = 0;
    let conditionalAttempts = 0;
    for (let attempt = 0; attempt < 4; attempt += 1) {
      conditionalAttempts = attempt + 1;
      const conditional = await fetch(noopUrl.toString(), { credentials: 'same-origin', cache: 'no-store', headers: etag ? { 'if-none-match': etag, 'x-trillionnium-map-if-none-match': etag } : {} });
      secondStatus = conditional.status;
      if (conditional.status === 304) break;
      const refreshedEtag = conditional.headers.get('etag');
      if (!refreshedEtag || refreshedEtag === etag) break;
      etag = refreshedEtag;
    }
    return { first_status: first.status, first_changed: firstJson?.changed, noop_status: noop.status, noop_changed: noopJson?.changed, etag, server_timing: noop.headers.get('server-timing'), server_ms: noop.headers.get('x-trillionnium-world-map-server-ms'), second_status: secondStatus, conditional_attempts: conditionalAttempts };
  }, { deltaUrl });
  assert(deltaNotModifiedProbe.first_status === 200 && deltaNotModifiedProbe.noop_status === 200 && Boolean(deltaNotModifiedProbe.etag) && Boolean(deltaNotModifiedProbe.server_timing) && Boolean(deltaNotModifiedProbe.server_ms) && deltaNotModifiedProbe.second_status === 304, 'browser map delta 304/server timing contract failed', deltaNotModifiedProbe);
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
  assert(await count(page, '#app-tactics-intent-draft-card[data-contract-version="trillionnium_tactics_command_intent_draft_v1"][data-board-cell-interaction-contract="trillionnium_tactics_board_cell_interaction_v1"][data-unit-selection-contract="trillionnium_tactics_unit_selection_v1"][data-web-role="intent_only_visualization_input"]') === 1, 'app tactics intent draft card missing');
  assert(await count(page, '#app-tactics-intent-draft-card[data-command-handler-owner="rust_world_tactics_command_handler"][data-source-of-truth="rust_tactics_command_model"]') === 1, 'app tactics intent draft source contract missing');
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

  await page.emulateMedia({ reducedMotion: 'reduce' });
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
  const worldFirstHumanScreen = await assertWorldFirstHumanScreen(page, '/world English system language');
  steps.push({ name: 'world_first_human_screen_four_questions', ok: true, ...worldFirstHumanScreen });
  assert(await count(page, '#world-openstreetmap-provider-readiness[data-contract-version="openstreetmap_provider_readiness_v1"][data-fixture-mode-green="true"][data-live-modes-fail-closed="true"][data-live-network-ingestion-enabled="false"][data-production-ingestion-enabled="false"]') === 1, 'world OSM provider readiness/fail-closed contract missing');
  assert(await count(page, '#world-openstreetmap-geodata-freshness[data-contract-version="openstreetmap_geodata_freshness_v1"][data-fixture-static-snapshot="true"][data-wall-clock-freshness-applies="false"][data-live-data-freshness-applies="false"][data-staleness-gate-green="true"][data-stale-live-ingestion-blocked="true"][data-fixture-snapshot-age-seconds="0"]') === 1, 'world OSM geodata freshness/staleness contract missing');
  assert(await count(page, '#world-openstreetmap-attribution[data-contract-version="openstreetmap_attribution_presence_v1"][data-attribution-required="true"][data-attribution-visible="true"][data-database-license="ODbL-1.0"]') === 1, 'world OSM attribution presence contract missing');
  await page.waitForFunction(() => Array.from(document.querySelectorAll('.leaflet-control-attribution')).some((element) => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' && rect.width > 0 && rect.height > 0 && element.textContent.includes('OpenStreetMap');
  }), { timeout: 15_000 });
  assert(await count(page, '#world-tactics-player-hud[data-contract-version="trillionnium_tactics_player_visible_surface_v1"]') === 1, 'world tactics player HUD contract missing');
  assert(await count(page, '#world-tactics-objective-card[data-session-contract="trillionnium_tactics_game_session_v1"]') === 1, 'world tactics objective card missing');
  assert(await count(page, '#world-tactics-current-session-card[data-tick-contract="trillionnium_tactics_simulation_tick_v1"]') === 1, 'world tactics current session card missing');
  assert(await count(page, '#trillionnium-tactics-game-shell[data-tactics-accessibility-contract="trillionnium_tactics_accessibility_v1"][data-keyboard-traversal="roving_grid_focus"][data-low-motion-support="prefers_reduced_motion"][data-reduced-motion="true"]') === 1, 'world tactics accessibility/low-motion shell contract missing');
  assert(await count(page, '#world-tactics-keyboard-help[data-accessibility-contract="trillionnium_tactics_accessibility_v1"]') === 1, 'world tactics keyboard help missing');
  assert(await count(page, '#world-tactics-command-draft-panel[data-contract-version="trillionnium_tactics_command_intent_draft_v1"][data-board-cell-interaction-contract="trillionnium_tactics_board_cell_interaction_v1"][data-unit-selection-contract="trillionnium_tactics_unit_selection_v1"][data-web-role="intent_only_visualization_input"]') === 1, 'world tactics command draft panel missing');
  assert(await count(page, '#world-tactics-command-draft-form[data-contract-version="trillionnium_tactics_command_intent_draft_v1"][data-source-of-truth="rust_world_tactics_command_handler"][data-web-role="intent_only_visualization_input"]') === 1, 'world tactics command draft form missing');
  assert(await count(page, '.tactics-tile[role="gridcell"][data-board-cell-interaction-contract="trillionnium_tactics_board_cell_interaction_v1"][data-command-intent-draft-contract="trillionnium_tactics_command_intent_draft_v1"][data-accessibility-contract="trillionnium_tactics_accessibility_v1"][data-draft-input-name="target_tile"][data-roving-tabindex="tactics_board"]') >= 64, 'world tactics selectable/a11y board cells missing');
  assert(await count(page, '.tactics-unit[data-unit-selection-contract="trillionnium_tactics_unit_selection_v1"][data-command-intent-draft-contract="trillionnium_tactics_command_intent_draft_v1"][data-draft-input-name="unit_id"]') >= 2, 'world tactics selectable units missing');
  assert(await count(page, '.tactics-command[role="button"][data-command-intent-draft-contract="trillionnium_tactics_command_intent_draft_v1"][data-accessibility-contract="trillionnium_tactics_accessibility_v1"][data-draft-input-name="command"]') >= 3, 'world tactics draftable/a11y commands missing');
  assert(await page.evaluate(() => window.trillionniumTacticsIntentDraft?.web_role) === 'intent_only_visualization_input', 'world tactics intent draft runtime missing');
  assert(await page.evaluate(() => window.trillionniumTacticsIntentDraft?.accessibility_contract_version) === 'trillionnium_tactics_accessibility_v1', 'world tactics accessibility runtime missing');
  await clickOrDomActivate(page.locator('.tactics-unit[data-side="player"]').first());
  await clickOrDomActivate(page.locator('.tactics-tile[data-tile="C3"]').first());
  await clickOrDomActivate(page.locator('.tactics-command[data-command="move_unit"]').first());
  const draftState = await page.evaluate(() => ({
    runtime: window.trillionniumTacticsIntentDraft?.getState?.(),
    command: document.querySelector('#world-tactics-command-draft-form [name="command"]')?.value,
    unitId: document.querySelector('#world-tactics-command-draft-form [name="unit_id"]')?.value,
    targetTile: document.querySelector('#world-tactics-command-draft-form [name="target_tile"]')?.value,
    body: document.querySelector('#world-tactics-command-draft-form [name="body"]')?.value,
  }));
  assert(draftState.command === 'move_unit' && draftState.targetTile === 'C3' && draftState.unitId, 'world tactics draft form did not track selected intent', draftState);
  assert(String(draftState.body || '').includes('Rust validates legality and outcome'), 'world tactics draft body must preserve Rust validation copy', draftState);
  assert(draftState.runtime?.command === 'move_unit' && draftState.runtime?.targetTile === 'C3', 'world tactics draft runtime state mismatch', draftState);
  assert(await count(page, '.tactics-tile.is-selected[data-tile="C3"]') === 1, 'world tactics selected tile styling missing');
  assert(await count(page, '.tactics-command.is-selected[data-command="move_unit"]') === 1, 'world tactics selected command styling missing');
  await page.locator('.tactics-tile[data-tile="C3"]').focus();
  await page.keyboard.press('ArrowRight');
  const keyboardDraftState = await page.evaluate(() => ({
    activeTile: document.activeElement?.dataset?.tile,
    runtime: window.trillionniumTacticsIntentDraft?.getState?.(),
    targetTile: document.querySelector('#world-tactics-command-draft-form [name="target_tile"]')?.value,
    status: document.querySelector('#world-tactics-command-draft-status')?.textContent,
  }));
  assert(keyboardDraftState.activeTile === 'D3' && keyboardDraftState.targetTile === 'D3' && keyboardDraftState.runtime?.targetTile === 'D3', 'world tactics keyboard traversal did not move draft target right from C3', keyboardDraftState);
  assert(String(keyboardDraftState.status || '').includes('Rust remains source of truth'), 'world tactics keyboard traversal must update live status', keyboardDraftState);
  assert(await count(page, '#world-tactics-reward-history-handoff[data-reward-history-contract="trillionnium_tactics_reward_history_v1"]') === 1, 'world tactics reward-history handoff missing');
  assert(await count(page, '#world-tactics-repeat-farming-copy[data-anti-cheese-contract="trillionnium_tactics_repeat_farming_anti_cheese_v1"]') === 1, 'world tactics repeat-farming copy missing');
  assert(await count(page, '#world-pulse-strip .pulse-card') === 5, 'world pulse strip should keep only compact primary counters visible');
  assert(await count(page, '#world-stats-compact-more .stat') >= 12, 'world compact stats drawer missing secondary counters');
  const secondaryDashboardPanels = await count(page, '[data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1"][data-main-experience="false"]');
  assert(secondaryDashboardPanels >= 10, 'world secondary dashboard/detail panels contract missing', { secondaryDashboardPanels });
  assert(await count(page, '#world-stats-compact-more[data-secondary-dashboard-role="secondary_counter_drawer"][data-default-state="collapsed"]') === 1, 'world secondary counter drawer must stay collapsed');
  assert(await count(page, '#world-map-underlay-details[data-secondary-dashboard-role="supporting_engine_diagnostics"][data-default-state="collapsed"]') === 1, 'world map diagnostics must remain secondary collapsed details');
  assert(await count(page, '#world-map-move-panel[data-secondary-dashboard-role="secondary_detail_panel"][data-default-state="available_after_core_loop"]') === 1, 'world detailed map panel must be marked secondary detail');
  const pulseBox = await page.locator('#world-pulse-strip').boundingBox({ timeout: 10_000 });
  assert(pulseBox && pulseBox.height < 360, 'world mobile stats area is too tall', pulseBox);
  await assertNoVisibleBilingualSlashPair(page, '/world English system language');
  await assertEnglishSurfaceHasNoCoreChineseLeaks(page, '/world English system language');
  assert(await count(page, '#trillionnium-world-game-first-shell[data-contract-version="trillionnium_world_game_first_playable_shell_v1"][data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1"][data-ui-render-owner="rust_world_ui_renderer"][data-browser-ui-owner="input_only_event_bridge"][data-source-of-truth="rust_world_state_projection"][data-web-role="input_only_visualization"][data-heavy-panels-policy="secondary_collapsed_deferred"]') === 1, 'world game-first playable shell missing Rust-owned projection/UI contract');
  assert(await count(page, '#world-keypad-adventure-shell[data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1"][data-ui-render-owner="rust_world_ui_renderer"]') === 1, 'world keypad shell missing Rust-owned UI renderer contract');
  assert(await count(page, '#world-keypad-map-grid[data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1"][data-render-owner="rust_world_ui_renderer"], #world-keypad-numpad[data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1"][data-render-owner="rust_world_ui_renderer"]') >= 2, 'world keypad viewport/buttons are not Rust-rendered');
  assert(await count(page, '#world-keypad-numpad .world-keypad-button[data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1"][data-render-owner="rust_world_ui_renderer"][data-web-role="input_only"]') === 9, 'world keypad movement buttons must be Rust-rendered input-only controls');
  assert(await count(page, '#trillionnium-world-game-first-shell [data-action-kind="move"], #trillionnium-world-game-first-shell [data-action-kind="talk_npc"], #trillionnium-world-game-first-shell [data-action-kind="train_skill"], #trillionnium-world-game-first-shell [data-action-kind="task"], #trillionnium-world-game-first-shell [data-action-kind="combat"]') >= 5, 'world game-first shell missing immediate action list');
  assert(await count(page, '#trillionnium-world-game-first-shell [data-bar-kind="hp"], #trillionnium-world-game-first-shell [data-bar-kind="energy"], #trillionnium-world-game-first-shell [data-bar-kind="stamina"], #trillionnium-world-game-first-shell [data-bar-kind="guard"], #trillionnium-world-game-first-shell [data-bar-kind="focus"], #trillionnium-world-game-first-shell [data-bar-kind="survival"]') >= 6, 'world game-first shell missing compact survival/combat bars');
  const gameFirstOrder = await page.evaluate(() => ({
    gameFirst: document.querySelector('#trillionnium-world-game-first-shell')?.compareDocumentPosition(document.querySelector('#world-map-shell-panel')),
    heroTitleText: document.querySelector('#world-keypad-adventure-shell h2')?.textContent || '',
    secondaryDeferred: document.querySelectorAll('[data-secondary-dashboard-contract="trillionnium_secondary_dashboard_panels_v1"][data-deferred-payload="true"]').length,
  }));
  assert((gameFirstOrder.gameFirst & 4) !== 0, 'world game-first shell must appear before heavy map/dashboard panel', gameFirstOrder);
  assert(!String(gameFirstOrder.heroTitleText).includes('Platinum Hero'), 'world first playable title must use Trillionnium-native copy', gameFirstOrder);
  assert(gameFirstOrder.secondaryDeferred >= 5, 'world secondary dashboard payloads must be marked deferred', gameFirstOrder);
  assert(await count(page, '#world-keypad-adventure-shell[data-contract-version="trillionnium_text_adventure_keypad_movement_v1"][data-transition-contract-version="trillionnium_world_transition_semantics_v1"][data-interface-style="yingxiongtanshuo_keyboard_tile_map"][data-source-of-truth="rust_world_map_move"][data-transition-source-of-truth="rust_world_map_transition_rules"]') === 1, 'world keypad tile-map shell transition contract missing');
  assert(await count(page, '#world-keypad-adventure-shell[data-reference-project="albert10jp/yxts-gold-asm"][data-lcd-screen="160x80"][data-lcd-viewport="5x3"][data-lcd-palette="green_monochrome"]') === 1, 'world keypad must declare mechanics reference LCD contract');
  assert(await count(page, '#world-keypad-map-grid[role="grid"][data-source-of-truth="rust_world_map_nodes"]') === 1, 'world keypad map grid missing');
  assert(await count(page, '#world-keypad-map-grid[data-lcd-cols="5"][data-lcd-rows="3"][data-reference-project="albert10jp/yxts-gold-asm"]') === 1, 'world keypad LCD viewport dimensions drifted');
  assert(await count(page, '.world-keypad-cell[data-node-id][data-current="true"]') === 1, 'world keypad current player cell missing');
  assert(await count(page, '#world-keypad-numpad[data-transition-contract-version="trillionnium_world_transition_semantics_v1"][data-transition-source-of-truth="rust_world_map_transition_rules"] .world-keypad-button[data-keypad-key][data-transition-contract-version="trillionnium_world_transition_semantics_v1"][data-transition-source-of-truth="rust_world_map_transition_rules"]') === 9, 'world keypad numpad transition semantics missing');
  assert(await count(page, '#world-play-first-action-prompt[data-contract-version="trillionnium_world_play_first_exploration_loop_v1"][data-transition-contract-version="trillionnium_world_transition_semantics_v1"][data-objective-travel-contract-version="trillionnium_world_objective_travel_v1"][data-source-of-truth="rust_world_map_nodes_and_tactics_commands"]') === 1, 'world play-first exploration prompt transition/objective travel contract missing');
  assert(await count(page, '#trillionnium-tactics-game-shell[data-world-objective-travel-contract="trillionnium_world_objective_travel_v1"]') === 1, 'world tactics shell objective travel contract missing');
  assert(await count(page, '#world-objective-travel[data-contract-version="trillionnium_world_objective_travel_v1"][data-source-of-truth="rust_world_graph_objective_travel"][data-web-role="visualization_only_intent_to_map_move"]') === 1, 'world objective travel panel missing Rust graph contract');
  assert(await count(page, '.world-keypad-cell[data-objective-travel-contract-version="trillionnium_world_objective_travel_v1"][data-objective-travel-role="next_step"], .world-keypad-cell[data-objective-travel-contract-version="trillionnium_world_objective_travel_v1"][data-objective-travel-role="target"], .world-keypad-cell[data-objective-travel-contract-version="trillionnium_world_objective_travel_v1"][data-objective-travel-role="path"]') >= 1, 'world keypad cells must show active objective travel route roles');
  const objectiveTravelCoverage = await page.evaluate(() => ({
    runtimeContract: window.trillionniumKeyboardMap?.getState?.().objectiveTravelContractVersion,
    currentNodeId: window.trillionniumKeyboardMap?.getState?.().objectiveTravelCurrentNodeId,
    targetNodeId: window.trillionniumKeyboardMap?.getState?.().objectiveTravelTargetNodeId,
    nextStepNodeId: window.trillionniumKeyboardMap?.getState?.().objectiveTravelNextStepNodeId,
    roleCount: document.querySelectorAll('.world-keypad-cell[data-objective-travel-role]:not([data-objective-travel-role="none"])').length,
    partyCount: Number(document.querySelector('#world-objective-travel')?.dataset?.partyCount || 0),
  }));
  assert(objectiveTravelCoverage.runtimeContract === 'trillionnium_world_objective_travel_v1', 'world objective travel runtime contract missing', objectiveTravelCoverage);
  assert(objectiveTravelCoverage.currentNodeId && objectiveTravelCoverage.targetNodeId && objectiveTravelCoverage.nextStepNodeId, 'world objective travel route endpoints missing', objectiveTravelCoverage);
  assert(objectiveTravelCoverage.roleCount >= 1 && objectiveTravelCoverage.partyCount >= 2, 'world objective travel route/party projection missing', objectiveTravelCoverage);
  assert(await count(page, '#world-current-location-card') === 1, 'world current location card missing');
  assert(await count(page, '#world-current-exits .world-local-exit-form[data-source-of-truth="rust_world_map_move"][data-transition-source-of-truth="rust_world_map_transition_rules"][data-transition-contract-version="trillionnium_world_transition_semantics_v1"]') >= 1, 'world current exits must expose Rust-owned movement transition intents');
  assert(await count(page, '#world-local-actions [data-action-kind]') >= 1, 'world local actions missing');
  assert(await count(page, '#world-local-npc-talk[data-command="talk_npc"]') === 1, 'world NPC talk affordance missing');
  assert(await count(page, '#world-local-skill-practice[data-contract-version="trillionnium_world_skill_practice_loop_v1"][data-practice-command="train_skill"][data-source-of-truth="rust_mentor_training_validator"][data-web-role="intent_only_visualization_input"]') === 1, 'world local skill practice mentor affordance missing');
  assert(await count(page, '#world-local-combat-encounter[data-contract-version="trillionnium_world_combat_encounter_loop_v1"][data-entry-command="attack"][data-source-of-truth="rust_world_combat_encounter_projection"][data-web-role="intent_only_visualization_input"]') === 1, 'world local combat encounter affordance missing');
  assert(await count(page, '#trillionnium-full-content-alignment[data-full-content-alignment-contract="trillionnium_hero_tan_full_content_alignment_v1"][data-thresholds-green="true"][data-source-of-truth="rust_trillionnium_full_content_volume_alignment_gate"][data-content-policy="trillionnium_native_no_copied_hero_tan_text_assets_or_tables"][data-web-role="visualization_input_only"]') === 1, 'world full content volume alignment gate missing');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="items_and_equipment"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content item/equipment runtime domain missing');
  assert(await count(page, '#trillionnium-equipment[data-item-equipment-runtime-contract="trillionnium_world_item_equipment_runtime_v1"]') === 1, 'world item/equipment runtime panel missing');
  assert(await count(page, '.trillionnium-equipment-form[data-command="equip_item"][data-source-of-truth="rust_trillionnium_item_equipment_runtime_state"]') >= 1, 'world item/equipment equip intent form missing');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="survival_time_resource_pressure"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content survival/resource domain missing');
  assert(await count(page, '#trillionnium-resource-pressure-runtime[data-resource-pressure-runtime-contract="trillionnium_world_resource_pressure_runtime_v1"][data-source-of-truth="rust_trillionnium_resource_pressure_runtime_state"][data-persistence-owner="world_state.world_trillionnium_characters.resource_pressure_state"][data-web-role="visualization_input_only"]') === 1, 'world resource pressure runtime panel missing Rust-owned metadata');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="food_water_age_survival"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content food/water/age survival domain must be Rust runtime backed');
  assert(await count(page, '#trillionnium-food-water-age-survival[data-survival-runtime-contract="trillionnium_world_food_water_age_survival_v1"][data-source-of-truth="rust_trillionnium_food_water_age_survival_state"][data-persistence-owner="world_state.world_trillionnium_characters.resource_pressure_state.food_water_age"][data-web-role="visualization_input_only"]') === 1, 'world food/water/age survival runtime panel missing Rust-owned metadata');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="npc_social_relationships"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content NPC social simulation domain must be Rust runtime backed');
  assert(await count(page, '#trillionnium-dynamic-social-simulation[data-dynamic-social-simulation-contract="trillionnium_world_dynamic_social_simulation_v1"][data-source-of-truth="rust_world_relationships_dynamic_social_state"][data-persistence-owner="world_state.world_relationships"][data-web-role="visualization_input_only"]') === 1, 'world dynamic social simulation panel missing Rust-owned metadata');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="authored_quest_chains"][data-domain-status="native_catalog_expanded"]') === 1, 'world full content authored quest chain domain must be native expanded');
  assert(await count(page, '#trillionnium-authored-quest-chains[data-authored-quest-chain-contract="trillionnium_world_authored_quest_chain_v1"][data-source-of-truth="rust_trillionnium_authored_quest_chain_catalog"][data-graph-owner="world_state.world_map_nodes.exits"][data-relationship-owner="world_state.world_relationships"][data-web-role="visualization_input_only"]') === 1, 'world authored quest chain catalog missing Rust-owned metadata');
  assert(await count(page, '#trillionnium-authored-quest-chains .trillionnium-authored-chain-card[data-chain-id="cistern_ration_relief"][data-survival-pressure="food_water_decay_visible"]') === 1, 'world authored quest chain catalog missing survival-linked native route');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="combat_numerics"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content combat numerics domain must be Rust runtime backed');
  assert(await count(page, '#trillionnium-combat-numerics-runtime[data-combat-numerics-runtime-contract="trillionnium_world_combat_numerics_runtime_v1"][data-source-of-truth="rust_trillionnium_combat_numerics_runtime_state"][data-persistence-owner="world_state.world_trillionnium_characters.combat_numerics_state"][data-runtime-status="rust_owned_hp_energy_guard_focus_hitcrit_live"][data-web-role="visualization_input_only"]') === 1, 'world combat numerics runtime panel missing Rust-owned metadata');
  assert(await count(page, '#trillionnium-full-content-alignment [data-content-domain="story_arcs"][data-domain-status="rust_runtime_backed"]') === 1, 'world full content story arcs domain must be Rust runtime backed');
  assert(await count(page, '#trillionnium-region-story-unlocks[data-region-story-unlock-runtime-contract="trillionnium_world_region_story_unlock_runtime_v1"][data-source-of-truth="rust_trillionnium_region_story_unlock_runtime_state"][data-persistence-owner="world_state.world_trillionnium_characters.region_story_unlock_state"][data-runtime-status="rust_owned_region_graph_story_arc_unlocks_live"][data-web-role="visualization_input_only"]') === 1, 'world region/story unlock runtime panel missing Rust-owned metadata');
  assert(await count(page, '#world-local-task-loop[data-pickup-command="offer_task"][data-completion-command="complete_task"]') === 1, 'world task pickup/completion affordance missing');
  const worldKeypadBox = await page.locator('#world-keypad-adventure-shell').boundingBox({ timeout: 10_000 });
  const worldKeypadGridBox = await page.locator('#world-keypad-map-grid').boundingBox({ timeout: 10_000 });
  const worldKeypadNumpadBox = await page.locator('#world-keypad-numpad').boundingBox({ timeout: 10_000 });
  const worldViewport = page.viewportSize() || { height: 844, width: 390 };
  assert(worldKeypadBox && worldKeypadBox.y < worldViewport.height * 0.36, 'world keypad adventure shell must be the first-screen game interface', { worldKeypadBox, worldViewport });
  assert(worldKeypadGridBox && worldKeypadGridBox.y < worldViewport.height * 0.46, 'world keypad character grid must be visible in the first screen', { worldKeypadGridBox, worldViewport });
  assert(worldKeypadNumpadBox && worldKeypadNumpadBox.y < worldViewport.height * 0.50, 'world keypad controls must be visible in the first screen', { worldKeypadNumpadBox, worldViewport });
  assert(await page.evaluate(() => window.trillionniumKeyboardMap?.source_of_truth) === 'rust_world_map_move', 'world keypad runtime source-of-truth missing');
  assert(await page.evaluate(() => window.trillionniumKeyboardMap?.transition_source_of_truth) === 'rust_world_map_transition_rules', 'world keypad transition runtime source-of-truth missing');
  const keypadTransitionCoverage = await page.evaluate(() => {
    const buttons = Array.from(document.querySelectorAll('#world-keypad-numpad .world-keypad-button[data-keypad-key]'));
    return {
      contract: window.trillionniumKeyboardMap?.getState?.().transitionContractVersion,
      statuses: [...new Set(buttons.map((button) => button.dataset.transitionStatus || ''))],
      kinds: [...new Set(buttons.map((button) => button.dataset.transitionKind || ''))],
      results: [...new Set(buttons.map((button) => button.dataset.transitionResult || ''))],
      blockedReasons: [...new Set(buttons.map((button) => button.dataset.blockedReason || '').filter(Boolean))],
      blockedCount: buttons.filter((button) => button.dataset.transitionStatus !== 'accepted').length,
      acceptedCount: buttons.filter((button) => button.dataset.transitionStatus === 'accepted').length,
    };
  });
  assert(keypadTransitionCoverage.contract === 'trillionnium_world_transition_semantics_v1', 'world keypad runtime transition contract missing', keypadTransitionCoverage);
  assert(keypadTransitionCoverage.acceptedCount >= 1 && keypadTransitionCoverage.blockedCount >= 1, 'world keypad must expose accepted and blocked transition semantics', keypadTransitionCoverage);
  assert(keypadTransitionCoverage.kinds.includes('blocked_terrain'), 'world keypad blocked-terrain transition kind missing', keypadTransitionCoverage);
  const firstKeypadMove = await page.evaluate(() => {
    const state = window.trillionniumKeyboardMap?.getState?.();
    const buttons = Array.from(document.querySelectorAll('#world-keypad-numpad .world-keypad-button[data-keypad-key]'));
    const preferredKeys = ['6', '2', '8', '4', '3', '1', '9', '7'];
    for (const key of preferredKeys) {
      const button = buttons.find((candidate) => candidate.dataset.keypadKey === key && candidate.dataset.transitionStatus === 'accepted' && candidate.dataset.targetNodeId);
      if (button) return { key, direction: button.dataset.moveDirection, targetNodeId: button.dataset.targetNodeId, fromNodeId: state?.currentNodeId, transitionKind: button.dataset.transitionKind };
    }
    return null;
  });
  assert(firstKeypadMove && firstKeypadMove.targetNodeId, 'world keypad had no adjacent movement target', firstKeypadMove);
  await clickOrDomActivate(page.locator(`#world-keypad-${firstKeypadMove.key}`));
  await page.waitForFunction((expected) => window.trillionniumKeyboardMap?.getState?.().currentNodeId === expected, firstKeypadMove.targetNodeId, { timeout: 10_000 });
  const afterButtonMove = await page.evaluate(() => ({
    runtime: window.trillionniumKeyboardMap?.getState?.(),
    domCurrent: document.querySelector('.world-keypad-cell[data-current="true"]')?.dataset?.nodeId,
    objectiveTravelCurrentNodeId: window.trillionniumKeyboardMap?.getState?.().objectiveTravelCurrentNodeId,
    status: document.querySelector('#world-keypad-live-status')?.textContent || '',
    source: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastInputSource || '',
    transitionKind: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastTransitionKind || '',
    transitionSourceOfTruth: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastTransitionSourceOfTruth || '',
    gridRenderOwner: document.querySelector('#world-keypad-map-grid')?.dataset?.renderOwner || '',
    numpadRenderOwner: document.querySelector('#world-keypad-numpad')?.dataset?.renderOwner || '',
    rustOwnedUiContract: document.querySelector('#world-keypad-map-grid')?.dataset?.rustOwnedUiContract || '',
  }));
  assert(afterButtonMove.runtime?.currentNodeId === firstKeypadMove.targetNodeId && afterButtonMove.domCurrent === firstKeypadMove.targetNodeId, 'world keypad button movement did not update persisted projection', afterButtonMove);
  assert(afterButtonMove.gridRenderOwner === 'rust_world_ui_renderer' && afterButtonMove.numpadRenderOwner === 'rust_world_ui_renderer' && afterButtonMove.rustOwnedUiContract === 'trillionnium_world_rust_owned_ui_shell_v1', 'world keypad button move must swap Rust-rendered UI fragments, not browser-built UI', afterButtonMove);
  assert(afterButtonMove.objectiveTravelCurrentNodeId === firstKeypadMove.targetNodeId, 'world objective travel did not refresh from Rust move response', afterButtonMove);
  assert(afterButtonMove.source === 'button' && /Moved|已移动/.test(afterButtonMove.status), 'world keypad button move status missing', afterButtonMove);
  assert(['local_exit', 'room_transition', 'zone_transition', 'wait'].includes(afterButtonMove.transitionKind), 'world keypad button move transition kind missing', afterButtonMove);
  assert(afterButtonMove.transitionSourceOfTruth === 'rust_world_map_transition_rules', 'world keypad button move transition source missing', afterButtonMove);
  const keyboardMove = await page.evaluate((previousNodeId) => {
    const buttons = Array.from(document.querySelectorAll('#world-keypad-numpad .world-keypad-button[data-keypad-key]'))
      .filter((button) => button.dataset.transitionStatus === 'accepted' && button.dataset.targetNodeId && button.dataset.keypadKey !== '5');
    const backtrack = buttons.find((button) => button.dataset.targetNodeId === previousNodeId);
    if (backtrack) return { key: backtrack.dataset.keypadKey, direction: backtrack.dataset.moveDirection, targetNodeId: backtrack.dataset.targetNodeId, transitionKind: backtrack.dataset.transitionKind };
    const next = buttons[0];
    if (next) return { key: next.dataset.keypadKey, direction: next.dataset.moveDirection, targetNodeId: next.dataset.targetNodeId, transitionKind: next.dataset.transitionKind };
    return null;
  }, firstKeypadMove.fromNodeId);
  assert(keyboardMove && keyboardMove.targetNodeId, 'world keypad had no keyboard movement target after button move', keyboardMove);
  await page.locator('#world-keypad-adventure-shell').focus();
  await page.keyboard.press(`Numpad${keyboardMove.key}`);
  await page.waitForFunction((expected) => window.trillionniumKeyboardMap?.getState?.().currentNodeId === expected, keyboardMove.targetNodeId, { timeout: 10_000 });
  const afterKeyboardMove = await page.evaluate(() => ({
    runtime: window.trillionniumKeyboardMap?.getState?.(),
    domCurrent: document.querySelector('.world-keypad-cell[data-current="true"]')?.dataset?.nodeId,
    source: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastInputSource || '',
    direction: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastMoveDirection || '',
    transitionStatus: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastTransitionStatus || '',
    transitionKind: document.querySelector('#world-keypad-adventure-shell')?.dataset?.lastTransitionKind || '',
    gridRenderOwner: document.querySelector('#world-keypad-map-grid')?.dataset?.renderOwner || '',
    rustOwnedUiContract: document.querySelector('#world-keypad-map-grid')?.dataset?.rustOwnedUiContract || '',
  }));
  assert(afterKeyboardMove.runtime?.currentNodeId === keyboardMove.targetNodeId && afterKeyboardMove.domCurrent === keyboardMove.targetNodeId, 'world keypad keyboard/numpad movement did not update map position', afterKeyboardMove);
  assert(afterKeyboardMove.gridRenderOwner === 'rust_world_ui_renderer' && afterKeyboardMove.rustOwnedUiContract === 'trillionnium_world_rust_owned_ui_shell_v1', 'world keypad keyboard move must keep Rust-rendered UI fragments current', afterKeyboardMove);
  assert(afterKeyboardMove.source === 'keyboard' && afterKeyboardMove.direction === keyboardMove.direction, 'world keypad keyboard movement source/direction missing', afterKeyboardMove);
  assert(afterKeyboardMove.transitionStatus === 'accepted' && ['local_exit', 'room_transition', 'zone_transition', 'wait'].includes(afterKeyboardMove.transitionKind), 'world keypad keyboard movement transition semantics missing', afterKeyboardMove);
  steps.push({ name: 'world_keypad_tile_map_button_and_numpad_movement', ok: true, button_move: firstKeypadMove, keyboard_move: keyboardMove });

  await seedWebSession(context, { matrixUserId: localAdventureMatrixUserId, roomId: localAdventureRoomId, sessionId: localAdventureSessionId });
  await page.goto(`/world?lang=en&e2e_reload=${encodeURIComponent(runId)}-local-adventure#world-keypad-adventure-shell`, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-keypad-adventure-shell', { state: 'attached', timeout: 15_000 });

  let routeToNpcHub = await moveWorldKeypadToNode(page, 'mirror-city-square');
  await page.goto(`/world?lang=en&e2e_reload=${encodeURIComponent(runId)}-npc-1#world-play-first-action-prompt`, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('#world-play-first-action-prompt', { state: 'attached', timeout: 15_000 });
  let promptNodeId = await page.locator('#world-play-first-action-prompt').first().getAttribute('data-current-node-id');
  if (promptNodeId !== 'mirror-city-square') {
    routeToNpcHub = await moveWorldKeypadToNode(page, 'mirror-city-square');
    await page.goto(`/world?lang=en&e2e_reload=${encodeURIComponent(runId)}-npc-2-${Date.now()}#world-play-first-action-prompt`, { waitUntil: 'domcontentloaded', timeout: 30_000 });
    await page.waitForSelector('#world-play-first-action-prompt', { state: 'attached', timeout: 15_000 });
    promptNodeId = await page.locator('#world-play-first-action-prompt').first().getAttribute('data-current-node-id');
  }
  assert(promptNodeId === 'mirror-city-square', 'world play-first prompt did not reload at Mirror City Square after keypad route', { promptNodeId, routeToNpcHub });
  assert(await count(page, '#world-local-skill-practice .world-local-skill-practice-form[data-command="train_skill"][data-skill-id="basic_unarmed"][data-mentor-npc-id="npc-street-compass-sifu"][data-skill-practice-contract-version="trillionnium_world_skill_practice_loop_v1"][data-source-of-truth="rust_mentor_training_validator"][data-web-role="intent_only_visualization_input"]') >= 1, 'world local mentor skill-practice form missing at Mirror City Square');
  await submitWorldForm(page, '#world-local-skill-practice .world-local-skill-practice-form[data-command="train_skill"][data-skill-id="basic_unarmed"][data-mentor-npc-id="npc-street-compass-sifu"]', marker, 'skill=trained');
  await page.waitForSelector('#world-local-skill-practice .world-local-skill-practice-card[data-skill-id="basic_unarmed"][data-known-skill="true"]', { state: 'attached', timeout: 15_000 });
  const localSkillPracticeState = await page.evaluate(() => ({
    promptNodeId: document.querySelector('#world-play-first-action-prompt')?.dataset?.currentNodeId,
    knownSkillCount: Number(document.querySelector('#world-local-skill-practice-feedback')?.dataset?.knownSkillCount || 0),
    knownSkills: document.querySelector('#world-local-skill-practice-feedback span')?.textContent || '',
    sourceOfTruth: document.querySelector('#world-local-skill-practice')?.dataset?.sourceOfTruth || '',
    formWebRole: document.querySelector('#world-local-skill-practice .world-local-skill-practice-form[data-skill-id="basic_unarmed"]')?.dataset?.webRole || '',
  }));
  assert(localSkillPracticeState.promptNodeId === 'mirror-city-square' && localSkillPracticeState.knownSkills.includes('basic_unarmed'), 'world local mentor skill practice did not mutate Rust character projection', localSkillPracticeState);
  assert(localSkillPracticeState.sourceOfTruth === 'rust_mentor_training_validator' && localSkillPracticeState.formWebRole === 'intent_only_visualization_input', 'world local mentor practice source/web-role metadata drifted', localSkillPracticeState);
  assert(await count(page, '#world-local-combat-encounter-form[data-contract-version="trillionnium_world_combat_encounter_loop_v1"][data-command="attack"][data-validation-owner="rust_world_combat_encounter_validator"][data-command-handler-owner="rust_tactics_combat_handler"][data-return-state-owner="rust_world_combat_encounter_return_state"][data-web-role="intent_only_visualization_input"]') === 1, 'world local combat encounter form missing Rust-owned entry/return metadata');
  await submitWorldForm(page, '#world-local-combat-encounter-form', marker, 'combat=resolved');
  await page.waitForSelector('#world-local-combat-return[data-contract-version="trillionnium_world_combat_encounter_loop_v1"][data-return-state="map_ready_after_resolution"][data-source-of-truth="rust_world_combat_encounter_return_state"]', { state: 'attached', timeout: 15_000 });
  const localCombatEncounterState = await page.evaluate(() => ({
    promptNodeId: document.querySelector('#world-play-first-action-prompt')?.dataset?.currentNodeId,
    encounterContract: document.querySelector('#world-local-combat-encounter')?.dataset?.contractVersion || '',
    formTargetTile: document.querySelector('#world-local-combat-encounter-form')?.dataset?.targetTile || '',
    formOverlayId: document.querySelector('#world-local-combat-encounter-form')?.dataset?.currentOverlayId || '',
    returnState: document.querySelector('#world-local-combat-return')?.dataset?.returnState || '',
    returnNodeId: document.querySelector('#world-local-combat-return')?.dataset?.returnToNodeId || '',
    rewardStatus: document.querySelector('#world-local-combat-return')?.dataset?.rewardStatus || '',
    resourceContract: document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.resourcePressureRuntimeContract || '',
    resourceLastMutation: document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.lastMutationEvent || '',
    resourceMutationCount: Number(document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.mutationCount || 0),
    survivalContract: document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.survivalRuntimeContract || '',
    survivalFoodStatus: document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.foodStatus || '',
    survivalWaterStatus: document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.waterStatus || '',
    survivalAgeStage: document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.ageStage || '',
    dynamicSocialContract: document.querySelector('#trillionnium-dynamic-social-simulation')?.dataset?.dynamicSocialSimulationContract || '',
    dynamicSocialEventCount: Number(document.querySelector('#trillionnium-dynamic-social-simulation')?.dataset?.relationshipEventCount || 0),
    dynamicSocialFactionCount: Number(document.querySelector('#trillionnium-dynamic-social-simulation')?.dataset?.factionCount || 0),
    authoredQuestChainContract: document.querySelector('#trillionnium-authored-quest-chains')?.dataset?.authoredQuestChainContract || '',
    authoredQuestChainCount: Number(document.querySelector('#trillionnium-authored-quest-chains')?.dataset?.chainCount || 0),
    authoredQuestNodeCoverage: Number(document.querySelector('#trillionnium-authored-quest-chains')?.dataset?.coveredNodeCount || 0),
    combatNumericsContract: document.querySelector('#trillionnium-combat-numerics-runtime')?.dataset?.combatNumericsRuntimeContract || '',
    combatNumericsSource: document.querySelector('#trillionnium-combat-numerics-runtime')?.dataset?.sourceOfTruth || '',
    combatNumericsLastMutation: document.querySelector('#trillionnium-combat-numerics-runtime')?.dataset?.lastMutationEvent || '',
    combatNumericsMutationCount: Number(document.querySelector('#trillionnium-combat-numerics-runtime')?.dataset?.mutationCount || 0),
    combatNumericsHealthStatus: document.querySelector('#trillionnium-combat-numerics-runtime')?.dataset?.healthStatus || '',
    combatNumericsExchangeCount: document.querySelectorAll('#trillionnium-combat-numerics-runtime .trillionnium-combat-exchange[data-result]:not([data-result="none"])').length,
    regionStoryContract: document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.regionStoryUnlockRuntimeContract || '',
    regionStoryLastMutation: document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.lastMutationEvent || '',
    regionStoryArcCount: Number(document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.unlockedStoryArcCount || 0),
  }));
  assert(localCombatEncounterState.promptNodeId === 'mirror-city-square' && localCombatEncounterState.returnNodeId === 'mirror-city-square', 'world local combat did not return to the current exploration node', localCombatEncounterState);
  assert(localCombatEncounterState.encounterContract === 'trillionnium_world_combat_encounter_loop_v1' && localCombatEncounterState.returnState === 'map_ready_after_resolution' && localCombatEncounterState.rewardStatus === 'settled', 'world local combat return projection did not expose Rust settlement/map state', localCombatEncounterState);
  assert(localCombatEncounterState.resourceContract === 'trillionnium_world_resource_pressure_runtime_v1' && localCombatEncounterState.resourceLastMutation === 'tactics_attack' && localCombatEncounterState.resourceMutationCount >= 1, 'world local combat did not surface Rust-owned resource-pressure mutation state', localCombatEncounterState);
  assert(localCombatEncounterState.survivalContract === 'trillionnium_world_food_water_age_survival_v1' && localCombatEncounterState.survivalFoodStatus && localCombatEncounterState.survivalWaterStatus && localCombatEncounterState.survivalAgeStage, 'world local combat did not surface Rust-owned food/water/age survival state', localCombatEncounterState);
  assert(localCombatEncounterState.dynamicSocialContract === 'trillionnium_world_dynamic_social_simulation_v1' && localCombatEncounterState.dynamicSocialEventCount >= 1 && localCombatEncounterState.dynamicSocialFactionCount >= 8, 'world local combat did not surface Rust-owned dynamic social simulation state', localCombatEncounterState);
  assert(localCombatEncounterState.authoredQuestChainContract === 'trillionnium_world_authored_quest_chain_v1' && localCombatEncounterState.authoredQuestChainCount >= 6 && localCombatEncounterState.authoredQuestNodeCoverage >= 16, 'world local combat did not preserve authored quest-chain breadth catalog', localCombatEncounterState);
  assert(localCombatEncounterState.combatNumericsContract === 'trillionnium_world_combat_numerics_runtime_v1' && localCombatEncounterState.combatNumericsSource === 'rust_trillionnium_combat_numerics_runtime_state' && localCombatEncounterState.combatNumericsLastMutation === 'tactics_attack' && localCombatEncounterState.combatNumericsMutationCount >= 1 && localCombatEncounterState.combatNumericsExchangeCount >= 1 && localCombatEncounterState.combatNumericsHealthStatus, 'world local combat did not surface Rust-owned combat numerics mutation state', localCombatEncounterState);
  assert(localCombatEncounterState.regionStoryContract === 'trillionnium_world_region_story_unlock_runtime_v1' && localCombatEncounterState.regionStoryLastMutation === 'tactics_attack' && localCombatEncounterState.regionStoryArcCount >= 2, 'world local combat did not surface Rust-owned region/story unlock mutation state', localCombatEncounterState);
  assert(await count(page, '#world-local-npc-talk .world-local-npc-form[data-command="talk_npc"][data-npc-id="npc-street-compass-sifu"]') >= 1, 'world local NPC talk form missing at Mirror City Square');
  assert(await count(page, '#world-local-npc-talk .world-local-npc-form[data-command="offer_task"][data-npc-id="npc-street-compass-sifu"]') >= 1, 'world local NPC offer_task form missing at Mirror City Square');
  await submitWorldForm(page, '#world-local-npc-talk .world-local-npc-form[data-command="talk_npc"][data-npc-id="npc-street-compass-sifu"]', marker, 'npc=talked');
  assert(await count(page, '#world-play-first-action-prompt[data-current-node-id="mirror-city-square"]') === 1, 'world play-first prompt did not remain anchored after NPC talk');
  await submitWorldForm(page, '#world-local-npc-talk .world-local-npc-form[data-command="offer_task"][data-npc-id="npc-street-compass-sifu"]', marker, 'task=offered');
  await page.waitForSelector('#world-local-active-task-card[data-active-task="true"][data-lifecycle-step="complete_task"][data-task-archetype-id="courier_letter"]', { state: 'attached', timeout: 15_000 });
  assert(await count(page, '#world-local-task-complete-form[data-command="complete_task"][data-lifecycle-contract-version="trillionnium_world_local_task_lifecycle_v1"]') === 1, 'world local task completion form missing after offer_task');
  await page.locator('#world-local-task-complete-form input[name="body"]').evaluate((node, markerValue) => {
    node.value = `Trillionnium local task report ${markerValue}: current room checked, NPC lead confirmed, evidence package attached, route risk reviewed, next step ready, self-review complete.`;
    node.dispatchEvent(new Event('input', { bubbles: true }));
  }, marker);
  await submitWorldForm(page, '#world-local-task-complete-form', marker, 'task=completed');
  await page.waitForSelector('#world-local-task-completion-feedback[data-completion-present="true"][data-source-of-truth="rust_world_contract_completions"]', { state: 'attached', timeout: 15_000 });
  await page.waitForSelector('#trillionnium-resource-pressure-runtime[data-last-mutation-event="tactics_complete_task"]', { state: 'attached', timeout: 15_000 });
  await page.waitForSelector('#trillionnium-food-water-age-survival[data-survival-runtime-contract="trillionnium_world_food_water_age_survival_v1"]', { state: 'attached', timeout: 15_000 });
  await page.waitForSelector('#trillionnium-dynamic-social-simulation[data-dynamic-social-simulation-contract="trillionnium_world_dynamic_social_simulation_v1"]', { state: 'attached', timeout: 15_000 });
  await page.waitForSelector('#trillionnium-region-story-unlocks[data-last-mutation-event="tactics_complete_task"]', { state: 'attached', timeout: 15_000 });
  const localResourcePressureAfterTask = await page.evaluate(() => ({
    contract: document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.resourcePressureRuntimeContract || '',
    lastMutation: document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.lastMutationEvent || '',
    mutationCount: Number(document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.mutationCount || 0),
    evidenceStatus: document.querySelector('#trillionnium-resource-pressure-runtime')?.dataset?.evidenceStatus || '',
    survivalContract: document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.survivalRuntimeContract || '',
    survivalMutationCount: Number(document.querySelector('#trillionnium-food-water-age-survival')?.dataset?.mutationCount || 0),
    dynamicSocialContract: document.querySelector('#trillionnium-dynamic-social-simulation')?.dataset?.dynamicSocialSimulationContract || '',
    dynamicSocialEventCount: Number(document.querySelector('#trillionnium-dynamic-social-simulation')?.dataset?.relationshipEventCount || 0),
  }));
  assert(localResourcePressureAfterTask.contract === 'trillionnium_world_resource_pressure_runtime_v1' && localResourcePressureAfterTask.lastMutation === 'tactics_complete_task' && localResourcePressureAfterTask.mutationCount >= localCombatEncounterState.resourceMutationCount + 1, 'world local task completion did not advance Rust-owned resource-pressure runtime', localResourcePressureAfterTask);
  assert(localResourcePressureAfterTask.survivalContract === 'trillionnium_world_food_water_age_survival_v1' && localResourcePressureAfterTask.survivalMutationCount >= localResourcePressureAfterTask.mutationCount, 'world local task completion did not advance Rust-owned survival runtime', localResourcePressureAfterTask);
  assert(localResourcePressureAfterTask.dynamicSocialContract === 'trillionnium_world_dynamic_social_simulation_v1' && localResourcePressureAfterTask.dynamicSocialEventCount >= localCombatEncounterState.dynamicSocialEventCount + 2, 'world local NPC/task loop did not advance Rust-owned dynamic social simulation', localResourcePressureAfterTask);
  const localRegionStoryAfterTask = await page.evaluate(() => ({
    contract: document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.regionStoryUnlockRuntimeContract || '',
    sourceOfTruth: document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.sourceOfTruth || '',
    lastMutation: document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.lastMutationEvent || '',
    mutationCount: Number(document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.mutationCount || 0),
    unlockedRegionCount: Number(document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.unlockedRegionCount || 0),
    unlockedStoryArcCount: Number(document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.unlockedStoryArcCount || 0),
    visitedNodeCount: Number(document.querySelector('#trillionnium-region-story-unlocks')?.dataset?.visitedNodeCount || 0),
  }));
  assert(localRegionStoryAfterTask.contract === 'trillionnium_world_region_story_unlock_runtime_v1' && localRegionStoryAfterTask.sourceOfTruth === 'rust_trillionnium_region_story_unlock_runtime_state' && localRegionStoryAfterTask.lastMutation === 'tactics_complete_task' && localRegionStoryAfterTask.mutationCount >= 2 && localRegionStoryAfterTask.unlockedRegionCount >= 1 && localRegionStoryAfterTask.unlockedStoryArcCount >= localCombatEncounterState.regionStoryArcCount, 'world local task completion did not advance Rust-owned region/story unlock runtime', localRegionStoryAfterTask);
  const localTaskStatusAfterCompletion = await page.locator('#world-local-active-task-card').first().getAttribute('data-status');
  const localTaskLifecycleStepAfterCompletion = await page.locator('#world-local-active-task-card').first().getAttribute('data-lifecycle-step');
  assert(/^(trillionnium_task_completion_pending_settlement|completed_|review_hold)/.test(String(localTaskStatusAfterCompletion || '')), 'world local task status did not advance after complete_task', { localTaskStatusAfterCompletion, localTaskLifecycleStepAfterCompletion });
  assert(['settlement_pending', 'settlement_feedback', 'review_hold'].includes(String(localTaskLifecycleStepAfterCompletion || '')), 'world local task lifecycle did not expose settlement/review feedback after complete_task', { localTaskStatusAfterCompletion, localTaskLifecycleStepAfterCompletion });
  assert(await count(page, '#world-local-task-complete-form') === 0, 'world local completion form must disappear while settlement is pending');
  steps.push({ name: 'world_local_skill_practice_mentor_loop', ok: true, route_steps_to_npc_hub: routeToNpcHub.length, known_skill_count: localSkillPracticeState.knownSkillCount });
  steps.push({ name: 'world_local_combat_encounter_return_loop', ok: true, return_state: localCombatEncounterState.returnState, reward_status: localCombatEncounterState.rewardStatus });
  steps.push({ name: 'world_combat_numerics_runtime_loop', ok: true, last_mutation: localCombatEncounterState.combatNumericsLastMutation, mutation_count: localCombatEncounterState.combatNumericsMutationCount, exchange_count: localCombatEncounterState.combatNumericsExchangeCount });
  steps.push({ name: 'world_resource_pressure_runtime_loop', ok: true, last_mutation: localResourcePressureAfterTask.lastMutation, mutation_count: localResourcePressureAfterTask.mutationCount });
  steps.push({ name: 'world_food_water_age_survival_runtime_loop', ok: true, mutation_count: localResourcePressureAfterTask.survivalMutationCount });
  steps.push({ name: 'world_dynamic_social_simulation_loop', ok: true, relationship_event_count: localResourcePressureAfterTask.dynamicSocialEventCount, faction_count: localCombatEncounterState.dynamicSocialFactionCount });
  steps.push({ name: 'world_authored_quest_chain_catalog', ok: true, chain_count: localCombatEncounterState.authoredQuestChainCount, node_coverage: localCombatEncounterState.authoredQuestNodeCoverage });
  steps.push({ name: 'world_region_story_unlock_runtime_loop', ok: true, last_mutation: localRegionStoryAfterTask.lastMutation, mutation_count: localRegionStoryAfterTask.mutationCount, unlocked_story_arc_count: localRegionStoryAfterTask.unlockedStoryArcCount });
  steps.push({ name: 'world_local_npc_task_pickup_completion_loop', ok: true, route_steps_to_npc_hub: routeToNpcHub.length });

  await seedWebSession(context);
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

  await page.locator('#world-action-body').fill('Craft a browser E2E AI design studio asset: define the deliverable, evidence package, risk controls, action loop, next step, and self-review record for a reusable Trillionnium workshop item.');
  await submitWorldForm(page, 'form[action="/world/web/action"]', marker, 'played=1');
  steps.push({ name: 'world_action_browser_submit', ok: true });

  await page.locator('#world-company-body').fill('建立一个 AI 设计工坊：写清客户交付方案、委托目标画像、可提交成果、证据来源包、风险控制、行动循环、下一步计划和自检记录。');
  await submitWorldForm(page, 'form[action="/world/web/company"]', marker, 'company=created');
  steps.push({ name: 'world_company_browser_submit', ok: true });

  const browserListingMarker = `Browser quest board ${marker}`;
  await page.locator('#world-listing-body').fill(`${browserListingMarker}: publish a studio bounty card with deliverables, reward logic, evidence package, commitments, risk controls, next action, rating rubric, revision policy, and self-review record.`);
  await submitWorldForm(page, 'form[action="/world/web/listing"]', marker, 'listing=created');
  const browserListingId = await extractWorldListingIdByMarker(page, browserListingMarker);
  steps.push({ name: 'world_listing_browser_submit', ok: true, listing_id: browserListingId });

  const browserBuyMarker = `Browser quest accept ${marker}`;
  await setWorldInputValue(page, '#world-buy-listing-id', browserListingId);
  await page.locator('#world-buy-body').evaluate((node, bodyText) => {
    node.value = bodyText;
    node.dispatchEvent(new Event('input', { bubbles: true }));
  }, `${browserBuyMarker}: accept this quest card and open an adventure commission with deliverables, evidence package, rating standards, risk controls, next action, and self-review.`);
  await submitWorldForm(page, '#world-buy-form', marker, 'purchase=created');
  const browserWorkOrderId = await extractWorldWorkOrderIdByMarker(page, browserBuyMarker);
  const purchaseCards = await count(page, '#world-purchase-cards-live article, #world-purchase-cards-live .mini');
  assert(purchaseCards >= 1, 'quest accept card missing after accepting quest board');
  steps.push({ name: 'world_quest_accept_browser_submit', ok: true, purchase_cards: purchaseCards, listing_id: browserListingId, work_order_id: browserWorkOrderId });

  await setWorldInputValue(page, '#world-work-deliver-id', browserWorkOrderId);
  await page.locator('#world-work-deliver-body').evaluate((node) => {
    node.value = 'Browser quest delivery: deliver a customer-ready方案 with evidence/source data, risk controls, self-review, next action plan, acceptance checklist, and concrete result notes. 提交可交付方案：成果、证据包、风险控制、自评复盘、下一步计划、验收清单和真实结果记录。';
    node.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await submitWorldForm(page, '#world-work-deliver-form', marker, 'work=delivered');
  const deliveryCards = await count(page, '#world-work-deliveries-live article, #world-work-deliveries-live .mini');
  assert(deliveryCards >= 1, 'quest result card missing after submit');
  steps.push({ name: 'world_quest_result_submit_browser_submit', ok: true, delivery_cards: deliveryCards, work_order_id: browserWorkOrderId });

  await setWorldInputValue(page, '#world-work-accept-id', browserWorkOrderId);
  await submitWorldForm(page, '#world-work-accept-form', marker, 'work=accepted');
  const acceptanceCards = await count(page, '#world-work-acceptances-live article, #world-work-acceptances-live .mini');
  assert(acceptanceCards >= 1, 'quest rating card missing after rating');
  steps.push({ name: 'world_quest_rating_browser_submit', ok: true, acceptance_cards: acceptanceCards, work_order_id: browserWorkOrderId });

  const rumMatrixWarmup = await page.evaluate(async ({ marker, deltaCursor, matrixUserId }) => {
    const results = [];
    const rumMatrixSurfaces = ['app', 'world'];
    const rumMatrixDevices = ['mobile', 'desktop'];
    const rumMatrixSampleKinds = ['first_map_interactive_runtime_ready', 'delta_not_modified_304_fast_path', 'weak_network_cached_snapshot'];
    for (const surfaceId of rumMatrixSurfaces) {
      for (const userAgentClass of rumMatrixDevices) {
        for (const sampleKind of rumMatrixSampleKinds) {
          const response = await fetch('/world/web/map-rum', {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              matrix_user_id: matrixUserId,
              surface_id: surfaceId,
              session_id: `browser-e2e-rum-${marker}`,
              viewport_cursor: deltaCursor || 'browser-e2e-current-cursor',
              sample_kind: sampleKind,
              user_agent_class: userAgentClass,
              first_map_interactive_ms: sampleKind.startsWith('first_map_interactive') ? 140 : 0,
              viewport_refresh_ms: sampleKind.startsWith('first_map_interactive') ? 0 : 24,
              focus_to_action_rail_ms: 18,
              main_thread_long_task_ms: 0,
              tile_error_count: 0,
            }),
          });
          results.push({ surfaceId, userAgentClass, sampleKind, status: response.status, ok: response.ok });
        }
      }
    }
    return results;
  }, { marker, deltaCursor, matrixUserId });
  assert(rumMatrixWarmup.every((result) => result.ok), 'RUM matrix warmup failed', rumMatrixWarmup);
  steps.push({ name: 'world_map_rum_matrix_browser_warmup', ok: true, samples: 12 });

  const healthApiContext = await request.newContext({ baseURL: baseUrl, extraHTTPHeaders: ingressHeaders(), userAgent: 'cex-browser-e2e-health-probe/1.0' });
  const health = await healthApiContext.get('/health', { timeout: 180_000 });
  assert(health.ok(), `health failed after browser flow: ${health.status()}`);
  const healthJson = await health.json();
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.contract_version === 'trillionnium_world_map_runtime_safety_gate_v1', 'health runtime safety gate contract missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.rum_slo_quantiles_visible === true, 'health runtime safety RUM SLO quantile gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.weak_network_cached_snapshot_visible === true, 'health runtime safety weak-network gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_world_map_runtime_safety_gate?.rum_excludes_lat_lng === true, 'health runtime safety location privacy gate missing', healthJson?.trillionnium_world_map_runtime_safety_gate);
  assert(healthJson?.trillionnium_openstreetmap_provider_readiness_gate?.contract_version === 'trillionnium_openstreetmap_provider_readiness_gate_v1', 'health OSM provider readiness gate contract missing', healthJson?.trillionnium_openstreetmap_provider_readiness_gate);
  assert(healthJson?.trillionnium_openstreetmap_provider_readiness_gate?.readiness_contract_version === 'openstreetmap_provider_readiness_v1', 'health OSM provider readiness contract missing', healthJson?.trillionnium_openstreetmap_provider_readiness_gate);
  assert(healthJson?.trillionnium_openstreetmap_provider_readiness_gate?.fixture_mode_green === true, 'health OSM fixture mode must be green', healthJson?.trillionnium_openstreetmap_provider_readiness_gate);
  assert(healthJson?.trillionnium_openstreetmap_provider_readiness_gate?.live_modes_fail_closed === true, 'health OSM live modes must fail closed', healthJson?.trillionnium_openstreetmap_provider_readiness_gate);
  assert(healthJson?.trillionnium_openstreetmap_provider_readiness_gate?.network_ingestion_disabled === true, 'health OSM network ingestion must stay disabled', healthJson?.trillionnium_openstreetmap_provider_readiness_gate);
  assert(healthJson?.trillionnium_openstreetmap_geodata_freshness_gate?.contract_version === 'trillionnium_openstreetmap_geodata_freshness_gate_v1', 'health OSM geodata freshness gate contract missing', healthJson?.trillionnium_openstreetmap_geodata_freshness_gate);
  assert(healthJson?.trillionnium_openstreetmap_geodata_freshness_gate?.freshness_contract_version === 'openstreetmap_geodata_freshness_v1', 'health OSM geodata freshness contract missing', healthJson?.trillionnium_openstreetmap_geodata_freshness_gate);
  assert(healthJson?.trillionnium_openstreetmap_geodata_freshness_gate?.freshness_status === 'fixture_static_fresh_live_stale_blocked', 'health OSM freshness status drifted', healthJson?.trillionnium_openstreetmap_geodata_freshness_gate);
  assert(healthJson?.trillionnium_openstreetmap_geodata_freshness_gate?.fixture_snapshot_age_seconds === 0, 'health OSM fixture age sentinel must stay zero', healthJson?.trillionnium_openstreetmap_geodata_freshness_gate);
  assert(healthJson?.trillionnium_openstreetmap_geodata_freshness_gate?.stale_live_ingestion_blocked === true, 'health OSM stale live ingestion must fail closed', healthJson?.trillionnium_openstreetmap_geodata_freshness_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.contract_version === 'trillionnium_openstreetmap_attribution_presence_gate_v1', 'health OSM attribution presence gate contract missing', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.attribution_presence_contract_version === 'openstreetmap_attribution_presence_v1', 'health OSM attribution presence contract missing', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.attribution === '© OpenStreetMap contributors', 'health OSM attribution copy drifted', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.database_license === 'ODbL-1.0', 'health OSM database license drifted', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.attribution_visible_required === true, 'health OSM attribution visible requirement missing', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.odbl_database_obligations_visible === true, 'health OSM ODbL obligation visibility missing', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_openstreetmap_attribution_presence_gate?.attribution_presence_green === true, 'health OSM attribution presence gate must be green', healthJson?.trillionnium_openstreetmap_attribution_presence_gate);
  assert(healthJson?.trillionnium_world_map_rum_slo_gate?.contract_version === 'trillionnium_world_map_rum_slo_v1' && healthJson?.trillionnium_world_map_rum_slo_gate?.green === true, 'health RUM SLO metrics gate not green', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(typeof healthJson?.trillionnium_world_map_rum_slo_gate?.raw_split_green === 'boolean', 'health RUM SLO raw split verdict must stay visible during warmup', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(Number.isFinite(Number(healthJson?.trillionnium_world_map_rum_slo_gate?.sample_count)), 'health RUM SLO sample count missing', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(['warming_until_min_samples', 'enforced'].includes(healthJson?.trillionnium_world_map_rum_slo_gate?.enforcement_status), 'health RUM SLO enforcement status missing', healthJson?.trillionnium_world_map_rum_slo_gate);
  assert(healthJson?.trillionnium_world_map_delta_cache_gate?.entity_delta_cache_contract === 'entity_group_versioned_delta_v1' && healthJson?.trillionnium_world_map_delta_cache_gate?.failure_rate_within_target === true, 'health delta cache gate not green', healthJson?.trillionnium_world_map_delta_cache_gate);
  const metrics = await healthApiContext.get('/metrics', { timeout: 180_000 });
  assert(metrics.ok(), `metrics failed after browser flow: ${metrics.status()}`);
  const metricsText = await metrics.text();
  await healthApiContext.dispose().catch(() => null);
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
    'cex_consumer_entry_trillionnium_openstreetmap_provider_readiness_gate_green 1',
    'cex_consumer_entry_trillionnium_openstreetmap_provider_fail_closed_mode_count ',
    'cex_consumer_entry_trillionnium_openstreetmap_geodata_freshness_gate_green 1',
    'cex_consumer_entry_trillionnium_openstreetmap_geodata_fixture_snapshot_age_seconds 0',
    'cex_consumer_entry_trillionnium_openstreetmap_geodata_staleness_alarm_active 0',
    'cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_gate_green 1',
    'cex_consumer_entry_trillionnium_openstreetmap_attribution_visible_required 1',
    'cex_consumer_entry_trillionnium_openstreetmap_odbl_obligations_visible 1',
    'cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_check_count 4',
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

  const requestFailureGate = buildRequestFailureGate(requestFailures);
  const summary = {
    ok: pageErrors.length === 0 && requestFailureGate.green,
    run_id: runId,
    checked_at_epoch: Math.floor(Date.now() / 1000),
    base_url: baseUrl,
    mode: e2eMode,
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
      world_transition_semantics: true,
      world_local_skill_practice_mentor_loop: true,
      world_local_combat_encounter_return_loop: true,
      world_combat_numerics_runtime_loop: true,
      world_resource_pressure_runtime_loop: true,
      world_food_water_age_survival_runtime_loop: true,
      world_dynamic_social_simulation_loop: true,
      world_authored_quest_chain_catalog: true,
      world_region_story_unlock_runtime_loop: true,
      world_local_npc_task_loop: true,
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
    request_failure_gate: requestFailureGate,
    console_messages: consoleMessages.slice(0, 20),
    page_errors: pageErrors,
    request_failures: requestFailureGate.classified_failures,
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
