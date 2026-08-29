import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const root = new URL("../", import.meta.url);
const browserUrl = new URL("src/browser.js", root);
const [browser, html] = await Promise.all([
  readFile(browserUrl, "utf8"),
  readFile(new URL("src/html.rs", root), "utf8"),
]);

function cssColor(name) {
  const match = html.match(new RegExp(`--${name}:(#[0-9a-fA-F]{6})`));
  assert.ok(match, `missing CSS color --${name}`);
  return match[1];
}

function luminance(hex) {
  const channels = hex.slice(1).match(/../g).map(value => Number.parseInt(value, 16) / 255);
  const linear = channels.map(value => value <= 0.04045
    ? value / 12.92
    : ((value + 0.055) / 1.055) ** 2.4);
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

function contrast(left, right) {
  const light = Math.max(luminance(left), luminance(right));
  const dark = Math.min(luminance(left), luminance(right));
  return (light + 0.05) / (dark + 0.05);
}

const componentSurfaces = [
  cssColor("bg"),
  "#142036",
  "#0d1421",
  "#07101d",
  "#12334a",
  "#0b2637",
];
for (const surface of componentSurfaces) {
  assert.ok(
    contrast(cssColor("line"), surface) >= 3,
    `component boundary ${cssColor("line")} is below 3:1 against ${surface}`,
  );
  assert.ok(
    contrast(cssColor("amber"), surface) >= 3,
    `focus indicator ${cssColor("amber")} is below 3:1 against ${surface}`,
  );
}
for (const foreground of ["text", "muted", "cyan", "amber", "pink"]) {
  for (const surface of ["#142036", "#0d1421"]) {
    assert.ok(
      contrast(cssColor(foreground), surface) >= 4.5,
      `text color --${foreground} is below 4.5:1 against ${surface}`,
    );
  }
}

for (const marker of [
  '<html lang="en">',
  'lang="zh-Hans"',
  'data-practice-step-state="{}"{}',
  'aria-current="step"',
  'class="practice-step-status"',
  '<ol role="list">',
  ':where(a,button,input,textarea,select,summary,[tabindex]):focus-visible',
  '<aside id="toast" aria-hidden="true" hidden>',
]) assert.ok(html.includes(marker), `missing static accessibility marker: ${marker}`);
assert.equal(html.includes('<aside id="toast" aria-live="polite"'), false);

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
  constructor(tagName, { classes = [], attributes = {}, dataset = {} } = {}) {
    this.tagName = tagName.toUpperCase();
    this.id = "";
    this.attributes = new Map(Object.entries(attributes));
    this.dataset = { ...dataset };
    this.classList = new FakeClassList(classes);
    this.children = [];
    this.parentElement = null;
    this._textContent = "";
    this.disabled = false;
    this.hidden = false;
    this.open = false;
    this.selectorResults = new Map();
  }

  get textContent() {
    return this.children.length > 0
      ? this.children.map(child => child.textContent).join("")
      : this._textContent;
  }

  set textContent(value) {
    this._textContent = String(value);
    for (const child of this.children) child.parentElement = null;
    this.children = [];
  }

  get nextElementSibling() {
    if (!this.parentElement) return null;
    const index = this.parentElement.children.indexOf(this);
    return this.parentElement.children[index + 1] || null;
  }

  append(...children) {
    for (const child of children) {
      child.parentElement = this;
      this.children.push(child);
    }
  }

  replaceChildren(...children) {
    for (const child of this.children) child.parentElement = null;
    this.children = [];
    this._textContent = "";
    this.append(...children);
  }

  remove() {
    if (!this.parentElement) return;
    const index = this.parentElement.children.indexOf(this);
    if (index >= 0) this.parentElement.children.splice(index, 1);
    this.parentElement = null;
  }

  getAttribute(name) {
    if (name === "id") return this.id || null;
    if (name === "class") return Array.from(this.classList).join(" ") || null;
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
    if (selector === "[tabindex]") return this.getAttribute("tabindex") !== null;
    if (selector.includes("a[href]") && selector.includes("button:not([disabled])")) {
      return !this.disabled && ["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "SUMMARY"].includes(this.tagName);
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
    if (this.selectorResults.has(selector)) return this.selectorResults.get(selector);
    if (!selector.startsWith(".")) return null;
    const className = selector.slice(1);
    const pending = [...this.children];
    while (pending.length > 0) {
      const candidate = pending.shift();
      if (candidate.classList.contains(className)) return candidate;
      pending.push(...candidate.children);
    }
    return null;
  }

  focus() {}
  scrollIntoView() {}
}

class FakeDetailsElement extends FakeElement {
  constructor(options = {}) {
    super("details", options);
  }
}

const body = new FakeElement("body");
const toast = new FakeElement("aside", { attributes: { "aria-hidden": "true" } });
const document = {
  body,
  addEventListener() {},
  createElement(tagName) {
    return tagName === "details" ? new FakeDetailsElement() : new FakeElement(tagName);
  },
  querySelector(selector) {
    return selector === "#toast" ? toast : null;
  },
  querySelectorAll() {
    return [];
  },
};
const sessionValues = new Map();
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
    location: { pathname: "/league/practice", assign() {}, reload() {} },
    matchMedia: () => ({ matches: true }),
    requestAnimationFrame(callback) { callback(); },
    setTimeout() { return 1; },
    clearTimeout() {},
  },
});
vm.runInContext(browser, context, { filename: browserUrl.pathname });

const output = new FakeElement("output");
context.show(output, "Choice saved / 选择已保存", true);
assert.equal(output.getAttribute("aria-live"), "polite");
assert.equal(output.children.length, 3);
assert.equal(output.children[1].getAttribute("aria-hidden"), "true");
assert.equal(output.children[2].getAttribute("lang"), "zh-Hans");
assert.equal(output.children[2].textContent, "选择已保存");
assert.equal(toast.getAttribute("aria-live"), null, "visual toast must not become a second live region");
assert.equal(toast.getAttribute("aria-hidden"), "true");

const field = new FakeElement("input");
const submit = new FakeElement("button", { attributes: { type: "submit" } });
const action = new FakeElement("form");
action.selectorResults.set(
  "input:not([type=hidden]):not([disabled]), select:not([disabled]), textarea:not([disabled])",
  field,
);
action.selectorResults.set(
  "button[type=submit]:not([disabled]), a.button[href], button:not([disabled])",
  submit,
);
assert.equal(context.paperRoomFocusTarget(action), field, "progressive action must focus data entry before submit");

function practiceFocusSignature(practiceAction, practiceVersion) {
  const form = new FakeElement("form", {
    classes: ["practice-advance-form", "primary-action"],
    dataset: { practiceAction, practiceVersion },
  });
  const button = new FakeElement("button", {
    classes: ["practice-primary-action"],
    attributes: { type: "submit" },
  });
  body.replaceChildren(form);
  form.append(button);
  return context.playerFocusSignature(button);
}

const captain = practiceFocusSignature("captain_plan", "1");
const sameCaptain = practiceFocusSignature("captain_plan", "1");
const evidence = practiceFocusSignature("evidence_assessment", "2");
assert.equal(captain, sameCaptain, "same-step reload signature must remain stable");
assert.notEqual(captain, evidence, "a new practice step must not inherit the prior submit focus");
assert.match(captain, /"practiceAction":"captain_plan"/);
assert.match(captain, /"practiceVersion":"1"/);

console.log("paper-raid-bff P1 static accessibility gate: ok");
