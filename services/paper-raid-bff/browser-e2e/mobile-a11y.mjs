import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const credentialsPath = process.env.PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE;
const evidenceDir = process.env.PAPER_RAID_BFF_BROWSER_EVIDENCE_DIR;
const sourceRevision = process.env.PAPER_RAID_BFF_BROWSER_SOURCE_REVISION;
const sourceTree = process.env.PAPER_RAID_BFF_BROWSER_SOURCE_TREE;
const sourceState = process.env.PAPER_RAID_BFF_BROWSER_SOURCE_STATE;
const sourceSnapshotSha256 = process.env.PAPER_RAID_BFF_BROWSER_SOURCE_SNAPSHOT_SHA256;
const candidateBinarySha256 = process.env.PAPER_RAID_BFF_BROWSER_CANDIDATE_BINARY_SHA256;
const baseUrl = new URL(process.env.PAPER_RAID_BFF_BROWSER_BASE_URL || "http://127.0.0.1:7020");
const expectedCsp = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";
const viewports = Object.freeze([
  Object.freeze({ width: 390, height: 844 }),
  Object.freeze({ width: 430, height: 932 }),
]);

assert.ok(credentialsPath && evidenceDir);
assert.match(sourceRevision || "", /^[0-9a-f]{40}$/);
assert.match(sourceTree || "", /^[0-9a-f]{40}$/);
assert.match(sourceSnapshotSha256 || "", /^[0-9a-f]{64}$/);
assert.match(candidateBinarySha256 || "", /^[0-9a-f]{64}$/);
assert.ok(["clean", "dirty-development-only"].includes(sourceState));
assert.equal(baseUrl.origin, "http://127.0.0.1:7020");
assert.equal(baseUrl.pathname, "/");
assert.equal(baseUrl.search, "");
assert.equal(baseUrl.hash, "");

const credentialStat = await stat(credentialsPath);
assert.equal(credentialStat.isFile(), true);
assert.equal(credentialStat.mode & 0o077, 0, "credentials must be mode 0600 or stricter");
assert.ok(credentialStat.size > 0 && credentialStat.size <= 16 * 1024);
const credentials = JSON.parse(await readFile(credentialsPath, "utf8"));
assert.deepEqual(Object.keys(credentials).sort(), ["login_key", "schema"]);
assert.equal(credentials.schema, "hepta.paper_raid.browser_mobile_a11y.credentials.v1");
assert.equal(typeof credentials.login_key, "string");
assert.ok(credentials.login_key.length >= 32 && credentials.login_key.length <= 4096);
await mkdir(evidenceDir, { recursive: true, mode: 0o700 });

function targetUrl(path) {
  return new URL(path, baseUrl).href;
}

function axValue(node, key) {
  if (key === "role" || key === "name") return node[key]?.value;
  return node.properties?.find(property => property.name === key)?.value?.value;
}

function safeName(value) {
  return String(value || "").replaceAll(/[^a-z0-9_-]+/gi, "-").replaceAll(/^-+|-+$/g, "").toLowerCase();
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function relativeLuminance([red, green, blue]) {
  const channel = value => {
    const normalized = value / 255;
    return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
}

function contrastRatio(first, second) {
  const high = Math.max(relativeLuminance(first), relativeLuminance(second));
  const low = Math.min(relativeLuminance(first), relativeLuminance(second));
  return (high + 0.05) / (low + 0.05);
}

function parseRgb(value) {
  const hex = String(value).trim().match(/^#([0-9a-f]{6})$/i);
  if (hex) return hex[1].match(/../g).map(channel => Number.parseInt(channel, 16));
  const match = String(value).match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
  assert.ok(match, `unsupported computed color: ${value}`);
  return match.slice(1, 4).map(Number);
}

async function waitForBindings(page) {
  await page.waitForFunction(() => document.documentElement.dataset.paperRaidBindingsReady === "true");
}

async function focusByKeyboard(page, selector, { backwards = false } = {}) {
  const found = await page.evaluate(() => {
    document.body.setAttribute("tabindex", "-1");
    document.body.focus({ preventScroll: true });
    return document.activeElement === document.body;
  });
  assert.equal(found, true);
  for (let index = 0; index < 160; index += 1) {
    await page.keyboard.press(backwards ? "Shift+Tab" : "Tab");
    if (await page.locator(selector).evaluateAll((nodes, active) => nodes.includes(active), await page.evaluateHandle(() => document.activeElement))) {
      await page.evaluate(() => document.body.removeAttribute("tabindex"));
      return;
    }
  }
  await page.evaluate(() => document.body.removeAttribute("tabindex"));
  assert.fail(`keyboard traversal did not reach ${selector}`);
}

async function assertFocusedStyle(page) {
  const style = await page.evaluate(() => {
    const element = document.activeElement;
    if (!(element instanceof HTMLElement)) return null;
    const computed = getComputedStyle(element);
    return {
      focusVisible: element.matches(":focus-visible"),
      outlineStyle: computed.outlineStyle,
      outlineWidth: Number.parseFloat(computed.outlineWidth),
      outlineOffset: Number.parseFloat(computed.outlineOffset),
      outlineColor: computed.outlineColor,
      focusBackdrop: computed.getPropertyValue("--focus-backdrop").trim(),
      boxShadow: computed.boxShadow,
      label: `${element.tagName.toLowerCase()}#${element.id}.${Array.from(element.classList).join(".")}`,
    };
  });
  assert.ok(style, "focused element is missing");
  assert.equal(style.focusVisible, true, `${style.label} is not :focus-visible`);
  assert.notEqual(style.outlineStyle, "none", `${style.label} has no outline`);
  assert.ok(style.outlineWidth >= 3, `${style.label} outline is thinner than 3px`);
  assert.ok(style.outlineOffset >= 3, `${style.label} outline offset is less than 3px`);
  assert.notEqual(style.boxShadow, "none", `${style.label} has no fixed focus backdrop`);
  assert.match(style.boxShadow, /\b9px\b/, `${style.label} focus backdrop does not cover the outline adjacency`);
  assert.deepEqual(parseRgb(style.boxShadow), parseRgb(style.focusBackdrop));
  assert.ok(
    contrastRatio(parseRgb(style.outlineColor), parseRgb(style.focusBackdrop)) >= 3,
    `${style.label} focus indicator contrast is below 3:1`,
  );
}

async function fullAxTree(cdp) {
  return (await cdp.send("Accessibility.getFullAXTree", { depth: -1 })).nodes;
}

async function partialAxTree(cdp, selector) {
  const documentNode = await cdp.send("DOM.getDocument", { depth: 0 });
  const { nodeId } = await cdp.send("DOM.querySelector", {
    nodeId: documentNode.root.nodeId,
    selector,
  });
  assert.notEqual(nodeId, 0, `missing DOM node for ${selector}`);
  const described = await cdp.send("DOM.describeNode", { nodeId });
  return (await cdp.send("Accessibility.getPartialAXTree", {
    backendNodeId: described.node.backendNodeId,
    fetchRelatives: false,
  })).nodes;
}

async function backendNodeId(cdp, selector) {
  const documentNode = await cdp.send("DOM.getDocument", { depth: 0 });
  const { nodeId } = await cdp.send("DOM.querySelector", {
    nodeId: documentNode.root.nodeId,
    selector,
  });
  assert.notEqual(nodeId, 0, `missing DOM node for ${selector}`);
  return (await cdp.send("DOM.describeNode", { nodeId })).node.backendNodeId;
}

async function backendNodeIds(cdp, selector) {
  const documentNode = await cdp.send("DOM.getDocument", { depth: 0 });
  const { nodeIds } = await cdp.send("DOM.querySelectorAll", {
    nodeId: documentNode.root.nodeId,
    selector,
  });
  return Promise.all(nodeIds.map(async nodeId =>
    (await cdp.send("DOM.describeNode", { nodeId })).node.backendNodeId,
  ));
}

function axBoolean(node, key) {
  const value = axValue(node, key);
  assert.ok([true, false, "true", "false"].includes(value), `${key} is not an AX boolean`);
  return value === true || value === "true";
}

async function assertAxIgnoredOrAbsent(cdp, selector) {
  const expectedBackendId = await backendNodeId(cdp, selector);
  const matches = (await fullAxTree(cdp))
    .filter(candidate => candidate.backendDOMNodeId === expectedBackendId);
  assert.ok(
    matches.length === 0 || matches.every(node => node.ignored),
    `${selector} unexpectedly appears as a nonignored AX node`,
  );
}

function collectBackendNodeIds(node, collected) {
  if (node.backendNodeId) collected.add(node.backendNodeId);
  for (const child of node.children || []) collectBackendNodeIds(child, collected);
  for (const shadowRoot of node.shadowRoots || []) collectBackendNodeIds(shadowRoot, collected);
  if (node.contentDocument) collectBackendNodeIds(node.contentDocument, collected);
}

async function assertClosedDetailsSubtreeIgnored(cdp, selector) {
  const documentNode = await cdp.send("DOM.getDocument", { depth: 0 });
  const { nodeId } = await cdp.send("DOM.querySelector", {
    nodeId: documentNode.root.nodeId,
    selector,
  });
  assert.notEqual(nodeId, 0, `missing closed details for ${selector}`);
  const described = await cdp.send("DOM.describeNode", { nodeId, depth: -1, pierce: true });
  assert.equal(String(described.node.nodeName).toLowerCase(), "details");
  const descendantBackendIds = new Set();
  for (const child of described.node.children || []) {
    if (String(child.nodeName).toLowerCase() === "summary") continue;
    collectBackendNodeIds(child, descendantBackendIds);
  }
  assert.ok(descendantBackendIds.size > 0, `${selector} has no non-summary descendants to audit`);
  const exposed = (await fullAxTree(cdp)).filter(node =>
    descendantBackendIds.has(node.backendDOMNodeId) && !node.ignored,
  );
  assert.deepEqual(
    exposed.map(node => ({
      backendDOMNodeId: node.backendDOMNodeId,
      role: axValue(node, "role"),
      name: axValue(node, "name"),
    })),
    [],
    `${selector} exposes descendants from its closed subtree`,
  );
}

async function assertFocusedAx(page, cdp) {
  const nodes = await partialAxTree(cdp, ":focus");
  const focused = nodes.find(node => !node.ignored && axValue(node, "focused") === true);
  assert.ok(focused, "focused DOM node is not focused in the accessibility tree");
}

async function assertDomAccessibility(page) {
  const result = await page.evaluate(() => {
    const visible = element => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return style.display !== "none" && style.visibility !== "hidden" && rect.width > 0 && rect.height > 0;
    };
    const selectorFor = element => {
      if (element.id) return `#${CSS.escape(element.id)}`;
      const classes = Array.from(element.classList).slice(0, 3).map(value => `.${CSS.escape(value)}`).join("");
      return `${element.tagName.toLowerCase()}${classes}`;
    };
    const actionableSelector = "a[href],button,input,select,textarea,summary,[tabindex]";
    const actionables = Array.from(document.querySelectorAll(actionableSelector)).filter(visible);
    const overflow = actionables.flatMap(element => {
      const rect = element.getBoundingClientRect();
      if (rect.left >= -1 && rect.right <= document.documentElement.clientWidth + 1) return [];
      return [{ selector: selectorFor(element), left: rect.left, right: rect.right, text: (element.textContent || "").trim().slice(0, 120) }];
    });
    const undersized = actionables.flatMap(element => {
      let inlineLinkException = false;
      if (element.matches('a[href]:not(.button)') && getComputedStyle(element).display === "inline") {
        const container = element.closest("p,li,dd,dt,figcaption,label");
        if (container) {
          const clone = container.cloneNode(true);
          clone.querySelectorAll("a").forEach(node => node.remove());
          inlineLinkException = /\S/u.test(clone.textContent || "");
        }
      }
      if (inlineLinkException) return [];
      let target = element;
      if (element.matches('input[type="radio"],input[type="checkbox"]')) target = element.closest("label") || element;
      const rect = target.getBoundingClientRect();
      if (rect.width >= 24 && rect.height >= 24) return [];
      return [{ selector: selectorFor(element), width: rect.width, height: rect.height }];
    });
    const ids = Array.from(document.querySelectorAll("[id]")).map(element => element.id);
    const duplicateIds = ids.filter((id, index) => ids.indexOf(id) !== index);
    const brokenReferences = [];
    for (const element of document.querySelectorAll("[aria-labelledby],[aria-describedby],[aria-controls]")) {
      for (const attribute of ["aria-labelledby", "aria-describedby", "aria-controls"]) {
        for (const id of (element.getAttribute(attribute) || "").split(/\s+/).filter(Boolean)) {
          if (!document.getElementById(id)) brokenReferences.push({ selector: selectorFor(element), attribute, id });
        }
      }
    }
    const positiveTabindex = Array.from(document.querySelectorAll("[tabindex]"))
      .filter(element => Number(element.getAttribute("tabindex")) > 0)
      .map(selectorFor);
    const hanWithoutLanguage = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!/[\u3400-\u9fff]/u.test(node.nodeValue || "")) continue;
      const parent = node.parentElement;
      if (!parent || parent.closest("script,style,[hidden],[aria-hidden='true']")) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      if (range.getClientRects().length === 0) continue;
      const languageOwner = parent.closest("[lang]");
      if (languageOwner?.getAttribute("lang") !== "zh-Hans") {
        hanWithoutLanguage.push({ selector: selectorFor(parent), text: (node.nodeValue || "").trim().slice(0, 120), lang: languageOwner?.getAttribute("lang") || null });
      }
    }
    return {
      innerWidth: window.innerWidth,
      visualViewportWidth: window.visualViewport?.width ?? null,
      clientWidth: document.documentElement.clientWidth,
      rootScrollWidth: document.documentElement.scrollWidth,
      bodyScrollWidth: document.body.scrollWidth,
      overflow,
      undersized,
      duplicateIds: [...new Set(duplicateIds)],
      brokenReferences,
      positiveTabindex,
      hanWithoutLanguage,
      htmlLang: document.documentElement.lang,
    };
  });
  assert.equal(result.htmlLang, "en");
  assert.equal(result.innerWidth, result.clientWidth);
  assert.equal(result.visualViewportWidth, result.clientWidth);
  assert.ok(result.rootScrollWidth <= result.clientWidth, JSON.stringify(result));
  assert.ok(result.bodyScrollWidth <= result.clientWidth, JSON.stringify(result));
  assert.deepEqual(result.overflow, []);
  assert.deepEqual(result.undersized, []);
  assert.deepEqual(result.duplicateIds, []);
  assert.deepEqual(result.brokenReferences, []);
  assert.deepEqual(result.positiveTabindex, []);
  assert.deepEqual(result.hanWithoutLanguage, []);
  return result;
}

async function assertAxStructure(page, cdp, state) {
  const nodes = await fullAxTree(cdp);
  const active = nodes.filter(node => !node.ignored);
  const roleCount = role => active.filter(node => axValue(node, "role") === role).length;
  assert.equal(roleCount("RootWebArea"), 1);
  assert.equal(roleCount("banner"), 1);
  assert.equal(roleCount("main"), 1);
  assert.equal(roleCount("contentinfo"), 1);
  assert.equal(active.filter(node => axValue(node, "role") === "heading" && axValue(node, "level") === 1).length, 1);
  for (const node of active) {
    if (["button", "link", "textbox", "radio", "combobox", "DisclosureTriangle"].includes(axValue(node, "role"))) {
      assert.ok(String(axValue(node, "name") || "").trim(), `unnamed ${axValue(node, "role")} in ${state}`);
    }
  }
  const bilingualControl = active.find(node =>
    ["button", "link", "radio"].includes(axValue(node, "role"))
      && /[A-Za-z]/u.test(String(axValue(node, "name") || ""))
      && /[\u3400-\u9fff]/u.test(String(axValue(node, "name") || "")),
  );
  assert.ok(bilingualControl, `${state} exposes no representative bilingual control name in AX`);
  await assertAxIgnoredOrAbsent(cdp, "#toast");
  if (state !== "login") {
    await assertClosedDetailsSubtreeIgnored(cdp, ".key-vault details:not([open])");
    if (await page.locator(".practice-leave:not([open])").count()) {
      await assertClosedDetailsSubtreeIgnored(cdp, ".practice-leave:not([open])");
    }
  }
  if (["captain", "evidence"].includes(state)) {
    const groupBackendId = await backendNodeId(cdp, ".practice-advance-form fieldset");
    const group = active.find(node => node.backendDOMNodeId === groupBackendId);
    assert.ok(group, `${state} choice fieldset is absent from AX`);
    assert.equal(axValue(group, "role"), "group");
    assert.match(String(axValue(group, "name") || ""), /Choose one/u);
    assert.match(String(axValue(group, "name") || ""), /请选择一项/u);
    const radioBackendIds = await backendNodeIds(cdp, '.practice-advance-form input[type="radio"]');
    const domChecked = await page.locator('.practice-advance-form input[type="radio"]').evaluateAll(radios =>
      radios.map(radio => radio.checked),
    );
    assert.equal(radioBackendIds.length, state === "captain" ? 2 : 3);
    for (let index = 0; index < radioBackendIds.length; index += 1) {
      const matches = active.filter(node => node.backendDOMNodeId === radioBackendIds[index]);
      assert.equal(matches.length, 1, `${state} radio ${index} lacks one exact nonignored AX node`);
      assert.equal(axValue(matches[0], "role"), "radio");
      assert.equal(axBoolean(matches[0], "checked"), domChecked[index]);
    }
  }
  if (state !== "login") {
    const progress = (await partialAxTree(cdp, ".practice-progress ol")).find(node => !node.ignored && axValue(node, "role") === "list");
    assert.ok(progress, "practice progress does not expose an AX list");
    const byId = new Map(nodes.map(node => [node.nodeId, node]));
    const descendants = [];
    const pending = [...(progress.childIds || [])];
    while (pending.length > 0) {
      const node = byId.get(pending.shift());
      if (!node) continue;
      descendants.push(node);
      pending.push(...(node.childIds || []));
    }
    assert.equal(descendants.filter(node => !node.ignored && axValue(node, "role") === "listitem").length, 5);
    const current = await page.locator('[aria-current="step"]').count();
    if (["not-started", "abandoned"].includes(state)) {
      assert.equal(current, 0);
    } else {
      assert.equal(current, 1);
      const currentText = await page.locator('[aria-current="step"]').innerText();
      assert.match(currentText, /Current\s*\/\s*当前/);
      assert.equal(await page.locator('[aria-current="step"]').getAttribute("data-practice-step-state"), "current");
      const currentBackendId = await backendNodeId(cdp, '[aria-current="step"]');
      const currentAx = nodes.find(node => node.backendDOMNodeId === currentBackendId && !node.ignored);
      assert.ok(currentAx, "current Practice step is absent from the AX tree");
      assert.equal(axValue(currentAx, "role"), "listitem");
      assert.match(String(axValue(currentAx, "name") || ""), /Current\s*\/\s*当前/);
    }
  }
  return nodes;
}

async function assertFocusableAxMappings(page, cdp) {
  const count = await page.locator("a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])").count();
  for (let index = 0; index < count; index += 1) {
    const token = `a11y-map-${index}`;
    const locator = page.locator("a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])").nth(index);
    if (!(await locator.isVisible())) continue;
    await locator.evaluate((element, value) => element.setAttribute("data-a11y-map", value), token);
    const nodes = await partialAxTree(cdp, `[data-a11y-map="${token}"]`);
    assert.ok(nodes.some(node => !node.ignored && axValue(node, "focusable") === true), `${token} lacks a focusable AX node`);
  }
  await page.locator("[data-a11y-map]").evaluateAll(nodes => nodes.forEach(node => node.removeAttribute("data-a11y-map")));
}

async function assertKeyboardCycle(page, cdp) {
  const expected = await page.evaluate(() => {
    const selector = "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])";
    const radioTabStops = new Map();
    for (const radio of document.querySelectorAll('input[type="radio"]:not([disabled])')) {
      const key = `${radio.form?.className || ""}:${radio.name}`;
      const existing = radioTabStops.get(key);
      if (!existing || radio.checked) radioTabStops.set(key, radio);
    }
    let sequence = 0;
    return Array.from(document.querySelectorAll(selector)).flatMap(element => {
      if (!(element instanceof HTMLElement) || element.offsetParent === null || element.tabIndex < 0) return [];
      const closedDetails = element.closest("details:not([open])");
      if (closedDetails && element !== closedDetails.querySelector(":scope > summary")) return [];
      if (element instanceof HTMLInputElement && element.type === "radio") {
        const key = `${element.form?.className || ""}:${element.name}`;
        if (radioTabStops.get(key) !== element) return [];
      }
      const token = `a11y-tab-${sequence++}`;
      element.setAttribute("data-a11y-tab", token);
      return [token];
    });
  });
  assert.ok(expected.length > 0);
  await page.evaluate(() => {
    document.body.setAttribute("tabindex", "-1");
    document.body.focus({ preventScroll: true });
  });
  const visited = [];
  for (let index = 0; index < expected.length; index += 1) {
    await page.keyboard.press("Tab");
    const token = await page.evaluate(() => document.activeElement?.getAttribute("data-a11y-tab"));
    assert.ok(token, `Tab ${index + 1} did not reach an expected control`);
    visited.push(token);
    await assertFocusedStyle(page);
    await assertFocusedAx(page, cdp);
  }
  assert.deepEqual(visited, expected, "Tab traversal order differs from DOM keyboard order");
  const cycleTo = async (key, wanted) => {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      await page.keyboard.press(key);
      const token = await page.evaluate(() => document.activeElement?.getAttribute("data-a11y-tab"));
      if (token === wanted) return;
      assert.equal(token, null, `${key} escaped the expected keyboard cycle at ${token}`);
    }
    assert.fail(`${key} did not cycle to ${wanted}`);
  };
  await cycleTo("Tab", expected[0]);
  await cycleTo("Shift+Tab", expected.at(-1));
  await page.evaluate(() => {
    document.body.removeAttribute("tabindex");
    document.querySelectorAll("[data-a11y-tab]").forEach(node => node.removeAttribute("data-a11y-tab"));
  });
}

async function audit(page, cdp, state, viewport) {
  await page.setViewportSize(viewport);
  await waitForBindings(page);
  const dom = await assertDomAccessibility(page);
  assert.equal(dom.clientWidth, viewport.width, `DOM client width is not ${viewport.width}px`);
  const ax = await assertAxStructure(page, cdp, state);
  await assertFocusableAxMappings(page, cdp);
  await assertKeyboardCycle(page, cdp);
  const metrics = await cdp.send("Page.getLayoutMetrics");
  assert.equal(metrics.cssLayoutViewport.clientWidth, viewport.width, `CDP layout width is not ${viewport.width}px`);
  assert.equal(metrics.cssVisualViewport.clientWidth, viewport.width, `CDP visual width is not ${viewport.width}px`);
  const stem = `${safeName(state)}-${viewport.width}x${viewport.height}`;
  const screenshotPath = `${evidenceDir}/${stem}.png`;
  const jsonPath = `${evidenceDir}/${stem}.json`;
  await page.screenshot({ path: screenshotPath, fullPage: true });
  await writeFile(jsonPath, `${JSON.stringify({
    schema: "hepta.paper_raid.browser_mobile_a11y.page_evidence.v1",
    state,
    viewport,
    path: new URL(page.url()).pathname,
    dom,
    layout: metrics.cssLayoutViewport,
    ax,
  }, null, 2)}\n`, { mode: 0o600 });
  return { state, viewport, screenshotPath, jsonPath };
}

async function auditBoth(page, cdp, state) {
  const evidence = [];
  for (const viewport of viewports) evidence.push(await audit(page, cdp, state, viewport));
  return evidence;
}

async function assertLiveAnnouncement(page, cdp, formSelector, expectedText) {
  const output = page.locator(`${formSelector} output`);
  await output.filter({ hasText: expectedText }).waitFor({ timeout: 5_000 });
  assert.equal(await output.getAttribute("aria-live"), "polite");
  assert.equal(await output.locator('span[lang="zh-Hans"]').count(), 1);
  assert.equal(await output.locator('span[aria-hidden="true"]').count(), 1);
  const outputBackendId = await backendNodeId(cdp, `${formSelector} output`);
  const nodes = (await fullAxTree(cdp)).filter(node => !node.ignored);
  const byId = new Map(nodes.map(node => [node.nodeId, node]));
  const subtreeText = root => {
    const values = [];
    const pending = [root];
    while (pending.length > 0) {
      const node = pending.shift();
      values.push(String(axValue(node, "name") || ""));
      for (const id of node.childIds || []) {
        const child = byId.get(id);
        if (child) pending.push(child);
      }
    }
    return values.join(" ");
  };
  const announcements = nodes.filter(node =>
    node.backendDOMNodeId === outputBackendId
      && axValue(node, "role") === "status"
      && subtreeText(node).includes(expectedText),
  );
  assert.equal(announcements.length, 1, `expected one live announcement containing ${expectedText}`);
  await assertAxIgnoredOrAbsent(cdp, "#toast");
}

async function submitAndReload(page, cdp, formSelector, expectedText, nextSelector) {
  await focusByKeyboard(page, `${formSelector} button[type="submit"]`);
  await assertFocusedStyle(page);
  const navigation = page.waitForNavigation({ waitUntil: "domcontentloaded", timeout: 10_000 })
    .catch(error => error);
  await page.keyboard.press("Enter");
  let announcementError = null;
  try {
    await assertLiveAnnouncement(page, cdp, formSelector, expectedText);
  } catch (error) {
    announcementError = error;
  }
  const navigationResult = await navigation;
  if (announcementError) throw announcementError;
  if (navigationResult instanceof Error) throw navigationResult;
  await waitForBindings(page);
  const next = page.locator(nextSelector).first();
  await next.waitFor();
  assert.equal(await next.evaluate((node, active) => node === active, await page.evaluateHandle(() => document.activeElement)), true, `${nextSelector} did not receive transition focus`);
  await assertFocusedStyle(page);
  await assertFocusedAx(page, cdp);
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: viewports[0],
  locale: "en-US",
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
  reducedMotion: "reduce",
  serviceWorkers: "block",
});
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
await cdp.send("Accessibility.enable");
await cdp.send("DOM.enable");
await cdp.send("Page.enable");
const browserVersion = await cdp.send("Browser.getVersion");
const consoleErrors = [];
const pageErrors = [];
const requestFailures = [];
page.on("console", message => { if (message.type() === "error") consoleErrors.push(message.text()); });
page.on("pageerror", error => pageErrors.push(error.message));
page.on("requestfailed", request => {
  const failure = request.failure()?.errorText || "";
  if (!(request.resourceType() === "document" && /ERR_ABORTED/.test(failure))) {
    requestFailures.push({ path: new URL(request.url()).pathname, failure });
  }
});

const evidence = [];
try {
  const loginResponse = await page.goto(targetUrl("/login"), { waitUntil: "domcontentloaded" });
  assert.ok(loginResponse);
  assert.equal(loginResponse.status(), 200);
  assert.equal(loginResponse.headers()["content-security-policy"], expectedCsp);
  await waitForBindings(page);
  evidence.push(...await auditBoth(page, cdp, "login"));

  await focusByKeyboard(page, 'input[name="login_key"]');
  await assertFocusedStyle(page);
  await page.keyboard.type(credentials.login_key);
  await page.keyboard.press("Tab");
  await assertFocusedStyle(page);
  await page.keyboard.press("Enter");
  await page.waitForURL(current => new URL(current).pathname === "/league");
  await waitForBindings(page);

  const session = await context.request.get(targetUrl("/api/session"), { headers: { accept: "application/json" } });
  assert.equal(session.status(), 200);
  const sessionValue = await session.json();
  assert.match(sessionValue.player_id, /^[0-9a-f-]{36}$/);

  await page.goto(targetUrl("/league/practice"), { waitUntil: "domcontentloaded" });
  await waitForBindings(page);
  assert.equal(await page.locator(".practice-prerequisite").count(), 0, "prequalified binding was not accepted");
  evidence.push(...await auditBoth(page, cdp, "not-started"));

  await submitAndReload(page, cdp, ".practice-start-form", "Practice ready.", '.practice-advance-form input[type="radio"]');
  evidence.push(...await auditBoth(page, cdp, "captain"));

  await focusByKeyboard(page, '.practice-advance-form input[type="radio"]');
  await page.keyboard.press("ArrowDown");
  const selectedValue = await page.locator('.practice-advance-form input[type="radio"]:checked').inputValue();
  const reload = page.waitForNavigation({ waitUntil: "domcontentloaded" });
  await page.reload({ waitUntil: "domcontentloaded" });
  await reload.catch(() => {});
  await waitForBindings(page);
  assert.equal(await page.locator('.practice-advance-form input[type="radio"]').evaluateAll((nodes, active) => nodes.includes(active), await page.evaluateHandle(() => document.activeElement)), true, "same-stage reload did not restore radio focus");
  assert.equal(await page.locator('.practice-advance-form input[type="radio"]:focus').inputValue(), selectedValue);
  await page.keyboard.press("ArrowUp");
  await submitAndReload(page, cdp, ".practice-advance-form", "Choice saved.", '.practice-advance-form input[type="radio"]');
  evidence.push(...await auditBoth(page, cdp, "evidence"));

  await focusByKeyboard(page, '.practice-advance-form input[type="radio"]');
  await page.keyboard.press("Space");
  await submitAndReload(page, cdp, ".practice-advance-form", "Choice saved.", ".practice-agent-wait .practice-primary-action");
  evidence.push(...await auditBoth(page, cdp, "waiting-for-bridge"));

  await focusByKeyboard(page, ".practice-leave summary");
  await page.keyboard.press("Enter");
  assert.equal(await page.locator(".practice-leave").getAttribute("open"), "");
  await submitAndReload(page, cdp, ".practice-abandon-form", "Practice left.", ".practice-abandoned .practice-primary-action");
  evidence.push(...await auditBoth(page, cdp, "abandoned"));

  const storage = await page.evaluate(async () => ({
    localStorageKeys: Object.keys(localStorage),
    indexedDatabases: typeof indexedDB.databases === "function" ? (await indexedDB.databases()).map(value => value.name).filter(Boolean) : [],
  }));
  assert.deepEqual(storage.localStorageKeys, []);
  assert.deepEqual(storage.indexedDatabases, []);
  assert.deepEqual(consoleErrors, []);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(requestFailures, []);

  const artifacts = [];
  for (const item of evidence) {
    for (const path of [item.screenshotPath, item.jsonPath]) {
      const bytes = await readFile(path);
      artifacts.push({ file: path.split("/").at(-1), bytes: bytes.length, sha256: sha256(bytes) });
    }
  }
  artifacts.sort((left, right) => left.file.localeCompare(right.file));
  const result = {
    schema: "hepta.paper_raid.browser_mobile_a11y.result.v1",
    source_revision: sourceRevision,
    source_tree: sourceTree,
    source_state: sourceState,
    source_snapshot_sha256: sourceSnapshotSha256,
    runtime_kind: "pinned_host_toolchain_container_test_only",
    candidate_binary_sha256: candidateBinarySha256,
    browser_product: browserVersion.product,
    browser_revision: browserVersion.revision,
    protocol_version: browserVersion.protocolVersion,
    viewports,
    audited_states: ["login", "not-started", "captain", "evidence", "waiting-for-bridge", "abandoned"],
    developer_json_fallback_used: false,
    production_bridge_pairing_proved: false,
    production_bridge_execution_proved: false,
    post_agent_focus_transition_real_e2e_proved: false,
    manual_screen_reader_certification: false,
    artifacts,
    passed: true,
  };
  await writeFile(`${evidenceDir}/result.json`, `${JSON.stringify(result, null, 2)}\n`, { mode: 0o600 });
  process.stdout.write(`${JSON.stringify(result)}\n`);
} finally {
  await cdp.send("Accessibility.disable").catch(() => {});
  await context.close();
  await browser.close();
}
