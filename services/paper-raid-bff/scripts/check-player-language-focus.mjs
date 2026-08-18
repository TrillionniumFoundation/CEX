import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const browserUrl = new URL("../src/browser.js", import.meta.url);
const source = await readFile(browserUrl, "utf8");

class FakeClassList {
  constructor(values = []) {
    this.values = new Set(values);
  }

  contains(value) {
    return this.values.has(value);
  }

  toggle(value, enabled) {
    if (enabled) this.values.add(value);
    else this.values.delete(value);
  }

  [Symbol.iterator]() {
    return this.values[Symbol.iterator]();
  }
}

class FakeElement {
  constructor(tagName, { id = "", classes = [], attributes = {}, dataset = {} } = {}) {
    this.tagName = tagName.toUpperCase();
    this.id = id;
    this.attributes = new Map(Object.entries(attributes));
    this.dataset = { ...dataset };
    this.classList = new FakeClassList(classes);
    this.children = [];
    this.parentElement = null;
    this.textContent = "";
    this.hidden = false;
    this.disabled = false;
    this.open = false;
    this.focusCalls = [];
    this.scrollCalls = [];
  }

  get className() {
    return Array.from(this.classList).join(" ");
  }

  set className(value) {
    this.classList = new FakeClassList(String(value).split(/\s+/).filter(Boolean));
  }

  get nextElementSibling() {
    if (!this.parentElement) return null;
    const index = this.parentElement.children.indexOf(this);
    return index >= 0 ? this.parentElement.children[index + 1] || null : null;
  }

  append(...children) {
    for (const child of children) {
      child.parentElement = this;
      this.children.push(child);
    }
  }

  insertAdjacentElement(position, element) {
    assert.equal(position, "afterend");
    assert.ok(this.parentElement);
    const index = this.parentElement.children.indexOf(this);
    element.parentElement = this.parentElement;
    this.parentElement.children.splice(index + 1, 0, element);
    return element;
  }

  remove() {
    if (!this.parentElement) return;
    const index = this.parentElement.children.indexOf(this);
    if (index >= 0) this.parentElement.children.splice(index, 1);
    this.parentElement = null;
  }

  getAttribute(name) {
    if (name === "id") return this.id || null;
    if (name === "class") return this.className || null;
    return this.attributes.get(name) ?? null;
  }

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
  }

  matches(selector) {
    if (selector === "form,article,section,details,[data-paper-id]") {
      return ["FORM", "ARTICLE", "SECTION", "DETAILS"].includes(this.tagName)
        || typeof this.dataset.paperId === "string";
    }
    if (selector === "[hidden],[aria-hidden='true']") {
      return this.hidden || this.getAttribute("aria-hidden") === "true";
    }
    if (selector.includes("a[href]") && selector.includes("button:not([disabled])")) {
      if (this.disabled) return false;
      return ["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "SUMMARY"].includes(this.tagName)
        || (this.getAttribute("tabindex") !== null && this.getAttribute("tabindex") !== "-1");
    }
    return false;
  }

  closest(selector) {
    for (let node = this; node; node = node.parentElement) {
      if (node.matches(selector)) return node;
    }
    return null;
  }

  querySelector(selector) {
    if (selector.startsWith(".")) {
      const className = selector.slice(1);
      const pending = [...this.children];
      while (pending.length > 0) {
        const candidate = pending.shift();
        if (candidate.classList.contains(className)) return candidate;
        pending.push(...candidate.children);
      }
    }
    return null;
  }

  focus(options) {
    this.focusCalls.push(options);
  }

  scrollIntoView(options) {
    this.scrollCalls.push(options);
  }
}

class FakeDetailsElement extends FakeElement {
  constructor(options = {}) {
    super("details", options);
  }
}

const sessionValues = new Map();
let focusCandidates = [];
const body = new FakeElement("body");
const document = {
  body,
  addEventListener() {},
  createElement(tagName) {
    return tagName === "details" ? new FakeDetailsElement() : new FakeElement(tagName);
  },
  querySelector() { return null; },
  querySelectorAll(selector) {
    return selector.includes("button:not([disabled])") ? focusCandidates : [];
  },
};
const context = vm.createContext({
  AbortSignal,
  Blob,
  CSS: { escape: value => String(value) },
  DOMParser: class {},
  Headers,
  HTMLDetailsElement: FakeDetailsElement,
  HTMLElement: FakeElement,
  Response,
  TextDecoder,
  TextEncoder,
  URL,
  atob: value => Buffer.from(value, "base64").toString("binary"),
  btoa: value => Buffer.from(value, "binary").toString("base64"),
  crypto: webcrypto,
  document,
  fetch: async () => { throw new Error("unexpected_fetch"); },
  performance: { getEntriesByType: () => [{ type: "reload" }] },
  sessionStorage: {
    getItem(key) { return sessionValues.get(key) ?? null; },
    setItem(key, value) { sessionValues.set(key, value); },
  },
  window: {
    location: { pathname: "/league/papers/11111111-1111-4111-8111-111111111111", assign() {} },
    matchMedia: () => ({ matches: true }),
    setTimeout() { return 1; },
    clearTimeout() {},
  },
});
vm.runInContext(source, context, { filename: browserUrl.pathname });

const errorEntries = vm.runInContext("Object.entries(PLAYER_ERROR_MESSAGES)", context);
assert.equal(errorEntries.length, 20, "the bounded player-facing dictionary must remain exactly 20 entries");
for (const [code, message] of errorEntries) {
  assert.match(code, /^[a-z][a-z0-9_]+$/);
  assert.match(message, / \/ .*?[\u3400-\u9fff]/, `${code} is not concise bilingual player language`);
  assert.ok(message.length <= 220, `${code} player language is not concise`);
}
for (const publicCode of [
  "authentication_required",
  "forbidden",
  "csrf_replayed",
  "conflict",
  "invalid_request",
  "dependency_unavailable",
  "rate_limited",
  "upstream_rejected",
  "not_found",
  "internal_error",
]) {
  assert.ok(errorEntries.some(([code]) => code === publicCode), `missing public BFF code ${publicCode}`);
}

const rawConflict = { error: "conflict", current_version: 7, retry: false };
const conflict = context.playerErrorPresentation(rawConflict);
assert.equal(conflict.code, "conflict");
assert.match(conflict.message, /authoritative state changed/i);
assert.equal(conflict.raw, JSON.stringify(rawConflict, null, 2));
assert.equal(context.playerErrorPresentation("unmapped_machine_error"), null);
assert.equal(
  context.playerErrorPresentation("state may have advanced: paper_authority_refresh_is_pending").code,
  "paper_authority_refresh_is_pending",
);

const errorParent = new FakeElement("form");
const errorOutput = new FakeElement("output");
errorParent.append(errorOutput);
context.show(errorOutput, rawConflict, false);
assert.equal(errorOutput.dataset.playerErrorCode, "conflict");
assert.equal(errorOutput.dataset.playerMessage, "friendly");
assert.equal(errorOutput.getAttribute("aria-live"), "polite");
assert.match(errorOutput.textContent, /\u6743\u5a01\u72b6\u6001\u5df2\u53d8\u66f4/);
assert.equal(errorOutput.textContent.includes("conflict"), false);
assert.equal(errorOutput.textContent.includes("current_version"), false);
const disclosure = errorOutput.nextElementSibling;
assert.ok(disclosure instanceof FakeDetailsElement);
assert.equal(disclosure.open, false);
assert.equal(disclosure.children[0].tagName, "SUMMARY", "native summary remains keyboard accessible");
assert.equal(disclosure.children[0].textContent, "Advanced error details / 高级错误详情");
assert.equal(disclosure.querySelector(".player-error-raw").textContent, JSON.stringify(rawConflict, null, 2));
context.show(errorOutput, "Action completed / 操作已完成", true);
assert.equal(errorOutput.nextElementSibling, null);
assert.equal(Object.hasOwn(errorOutput.dataset, "playerErrorCode"), false);

function focusTree() {
  const details = new FakeDetailsElement({ classes: ["paper-room-advanced"] });
  const form = new FakeElement("form", {
    classes: ["work-item-status-form"],
    dataset: {
      paperId: "11111111-1111-4111-8111-111111111111",
      workItemId: "22222222-2222-4222-8222-222222222222",
    },
  });
  const button = new FakeElement("button", {
    classes: ["primary-action-button"],
    attributes: { type: "submit", name: "next_status", value: "accepted" },
  });
  body.append(details);
  details.append(form);
  form.append(button);
  return { details, form, button };
}

const original = focusTree();
const secretInput = new FakeElement("input", {
  attributes: { type: "password", name: "passphrase" },
});
secretInput.value = "LOGIN-VALUE PASSPHRASE-VALUE CSRF-VALUE PRIVATE-KEY-VALUE";
original.form.append(secretInput);
assert.equal(context.savePlayerFocusContext(secretInput), true);
const secretContext = sessionValues.get("hepta.paper-raid.player-focus-context.v1");
for (const forbidden of [
  "LOGIN-VALUE",
  "PASSPHRASE-VALUE",
  "CSRF-VALUE",
  "PRIVATE-KEY-VALUE",
]) assert.equal(secretContext.includes(forbidden), false, `focus context stored ${forbidden}`);
assert.equal(context.savePlayerFocusContext(original.button), true);
const stored = sessionValues.get("hepta.paper-raid.player-focus-context.v1");
assert.ok(stored);
assert.equal(stored.includes("accepted"), true);
assert.equal(stored.includes("password"), false);
original.details.remove();
const replacement = focusTree();
focusCandidates = [replacement.button];
assert.equal(context.restorePlayerFocusContext("navigate"), false, "normal navigation must not restore focus");
assert.equal(replacement.button.focusCalls.length, 0);
assert.equal(context.restorePlayerFocusContext("reload"), true);
assert.equal(replacement.details.open, true, "closed Advanced disclosure opens before focus restoration");
assert.equal(replacement.button.focusCalls.length, 1);
assert.equal(replacement.button.focusCalls[0].preventScroll, true);
assert.equal(replacement.button.scrollCalls.length, 1);
assert.equal(replacement.button.scrollCalls[0].behavior, "auto");
assert.equal(replacement.button.scrollCalls[0].block, "center");
assert.equal(typeof replacement.button.click, "undefined", "focus restoration must not replay an action");

const duplicate = focusTree();
focusCandidates = [replacement.button, duplicate.button];
assert.equal(context.restorePlayerFocusContext("reload"), false, "ambiguous context must fail closed");
assert.equal(duplicate.button.focusCalls.length, 0);

const focusSource = source.slice(
  source.indexOf("function restorePlayerFocusContext("),
  source.indexOf("function bindPlayerFocusContext()"),
);
for (const forbidden of [".click(", ".submit(", "requestSubmit(", "dispatchEvent("]) {
  assert.equal(focusSource.includes(forbidden), false, `focus restore replays an action via ${forbidden}`);
}
const disclosureSource = source.slice(
  source.indexOf("function renderPlayerErrorDisclosure("),
  source.indexOf("function show("),
);
assert.ok(disclosureSource.includes('document.createElement("details")'));
assert.ok(disclosureSource.includes('document.createElement("summary")'));
assert.ok(disclosureSource.includes("textContent"));
assert.equal(disclosureSource.includes("innerHTML"), false);
const focusCaptureSource = source.slice(
  source.indexOf("function focusNodeDescriptor("),
  source.indexOf("function reloadNavigationType()"),
);
assert.equal(focusCaptureSource.includes("node.value"), false);
assert.equal(focusCaptureSource.includes("csrfToken"), false);
assert.equal(focusCaptureSource.includes("humanSigner"), false);

console.log("paper-raid-bff player language and focus continuity: ok");
