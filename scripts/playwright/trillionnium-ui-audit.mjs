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
      app: { maxScrollH: 5200, maxMapY: 700, maxRouteY: 900, maxActionY: 1300, maxOnboardingY: 1900 },
      world: { maxScrollH: 12500, maxMapY: 900, maxPulseY: 1200, maxActionY: 1700 },
      league: { maxScrollH: 4300, maxStatsY: 750, maxModesY: 1100, maxConsoleY: 1400 },
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
    map: '#real-world-map',
    route: '#app-map-route-panel',
    action: '#app-map-action-panel',
    onboarding: '#app-first-playable-onboarding',
    denseCopy: '#app-tab-map .map-panel',
    tiles: '#app-tile-shards-live',
  },
  world: {
    header: 'header',
    map: '#world-real-map',
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
}

function checkMobile(result, limits) {
  assertMetric(result.scrollH <= limits.maxScrollH, `mobile/${result.name} scroll height regressed`, { scrollH: result.scrollH, limit: limits.maxScrollH });
  if (result.name === 'app') {
    const map = yOf(result, 'map');
    const route = yOf(result, 'route');
    const action = yOf(result, 'action');
    const onboarding = yOf(result, 'onboarding');
    const denseCopy = yOf(result, 'denseCopy');
    assertMetric(map < route && route < action && action < onboarding && onboarding < denseCopy, 'mobile/app must keep map → route → action → onboarding → dense copy order', { map, route, action, onboarding, denseCopy });
    assertMetric(map <= limits.maxMapY && route <= limits.maxRouteY && action <= limits.maxActionY && onboarding <= limits.maxOnboardingY, 'mobile/app key modules are too deep', { map, route, action, onboarding, limits });
  } else if (result.name === 'world') {
    const map = yOf(result, 'map');
    const pulse = yOf(result, 'pulse');
    const action = yOf(result, 'action');
    assertMetric(map < pulse && pulse < action, 'mobile/world must keep map → pulse → action order', { map, pulse, action });
    assertMetric(map <= limits.maxMapY && pulse <= limits.maxPulseY && action <= limits.maxActionY, 'mobile/world key modules are too deep', { map, pulse, action, limits });
  } else if (result.name === 'league') {
    const stats = yOf(result, 'stats');
    const modes = yOf(result, 'modes');
    const console = yOf(result, 'console');
    const progression = yOf(result, 'progression');
    assertMetric(stats < modes && modes < console && console < progression, 'mobile/league must keep stats → modes → console → progression order', { stats, modes, console, progression });
    assertMetric(stats <= limits.maxStatsY && modes <= limits.maxModesY && console <= limits.maxConsoleY, 'mobile/league key modules are too deep', { stats, modes, console, limits });
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
    const firstViewportButtons = Array.from(document.querySelectorAll('button,a,input,select,textarea'))
      .filter(visible)
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.top < vh && r.bottom > 0;
      })
      .map((el) => {
        const r = el.getBoundingClientRect();
        return { tag: el.tagName.toLowerCase(), id: el.id, y: Math.round(r.top), bottom: Math.round(r.bottom), txt: text(el).slice(0, 90) || el.getAttribute('aria-label') || el.getAttribute('placeholder') || '' };
      })
      .slice(0, 40);
    return {
      profile: profileName,
      name: targetName,
      viewport: { vw, vh },
      scrollH: Math.round(document.documentElement.scrollHeight),
      visibleCjk,
      overflowRaw,
      actionableOverflow,
      key,
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
        if (profile.name === 'mobile') checkMobile(result, profile.limits[target.name]);
        else checkDesktop(result, profile.limits[target.name]);
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
