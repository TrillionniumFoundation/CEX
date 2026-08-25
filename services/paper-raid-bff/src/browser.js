"use strict";

let csrfToken = null;
let humanSigner = null;

const KEY_BUNDLE_SCHEMA = "hepta.paper_raid.human_key_bundle.v1";
const KEY_BUNDLE_AAD = new TextEncoder().encode(KEY_BUNDLE_SCHEMA);
const KEY_BUNDLE_ITERATIONS = 310000;

function bytesToBase64(value) {
  const bytes = value instanceof Uint8Array ? value : new Uint8Array(value);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(value) {
  if (typeof value !== "string" || value.length > 100000) throw new Error("invalid_base64");
  const binary = atob(value);
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
  if (bytesToBase64(bytes) !== value) throw new Error("non_canonical_base64");
  return bytes;
}

function updateHumanKeyStatus(message) {
  for (const output of document.querySelectorAll(".human-key-status")) output.textContent = message;
}

async function deriveBundleKey(passphrase, salt) {
  const material = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(passphrase),
    "PBKDF2",
    false,
    ["deriveKey"]
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations: KEY_BUNDLE_ITERATIONS },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

async function nonExtractableSigner(privatePkcs8, publicRaw) {
  const privateKey = await crypto.subtle.importKey(
    "pkcs8",
    privatePkcs8,
    { name: "Ed25519" },
    false,
    ["sign"]
  );
  const publicKey = await crypto.subtle.importKey(
    "raw",
    publicRaw,
    { name: "Ed25519" },
    false,
    ["verify"]
  );
  const probe = crypto.getRandomValues(new Uint8Array(32));
  const signature = await crypto.subtle.sign("Ed25519", privateKey, probe);
  if (!await crypto.subtle.verify("Ed25519", publicKey, signature, probe)) {
    throw new Error("key_bundle_public_private_mismatch");
  }
  return Object.freeze({ privateKey, publicKeyBase64: bytesToBase64(publicRaw) });
}

async function createEncryptedBundle(passphrase) {
  if (!crypto.subtle || typeof crypto.subtle.generateKey !== "function") {
    throw new Error("webcrypto_unavailable");
  }
  const generated = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const publicRaw = new Uint8Array(await crypto.subtle.exportKey("raw", generated.publicKey));
  const privatePkcs8 = new Uint8Array(await crypto.subtle.exportKey("pkcs8", generated.privateKey));
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  try {
    const key = await deriveBundleKey(passphrase, salt);
    const encrypted = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv, additionalData: KEY_BUNDLE_AAD, tagLength: 128 },
      key,
      privatePkcs8
    );
    const signer = await nonExtractableSigner(privatePkcs8, publicRaw);
    const bundle = {
      schema: KEY_BUNDLE_SCHEMA,
      public_key: signer.publicKeyBase64,
      kdf: {
        name: "PBKDF2",
        hash: "SHA-256",
        iterations: KEY_BUNDLE_ITERATIONS,
        salt: bytesToBase64(salt)
      },
      cipher: {
        name: "AES-GCM",
        tag_length: 128,
        iv: bytesToBase64(iv)
      },
      encrypted_pkcs8: bytesToBase64(encrypted),
      created_at_unix: Math.floor(Date.now() / 1000)
    };
    return { bundle, signer };
  } finally {
    privatePkcs8.fill(0);
  }
}

function exactKeys(value, expected) {
  if (!value || Array.isArray(value) || typeof value !== "object") return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

async function decryptBundle(bundle, passphrase) {
  if (!exactKeys(bundle, ["schema", "public_key", "kdf", "cipher", "encrypted_pkcs8", "created_at_unix"]) ||
      bundle.schema !== KEY_BUNDLE_SCHEMA ||
      !exactKeys(bundle.kdf, ["name", "hash", "iterations", "salt"]) ||
      bundle.kdf.name !== "PBKDF2" || bundle.kdf.hash !== "SHA-256" ||
      bundle.kdf.iterations !== KEY_BUNDLE_ITERATIONS ||
      !exactKeys(bundle.cipher, ["name", "tag_length", "iv"]) ||
      bundle.cipher.name !== "AES-GCM" || bundle.cipher.tag_length !== 128) {
    throw new Error("unsupported_or_malformed_key_bundle");
  }
  const publicRaw = base64ToBytes(bundle.public_key);
  const salt = base64ToBytes(bundle.kdf.salt);
  const iv = base64ToBytes(bundle.cipher.iv);
  const encrypted = base64ToBytes(bundle.encrypted_pkcs8);
  if (publicRaw.length !== 32 || salt.length !== 16 || iv.length !== 12 || encrypted.length > 4096) {
    throw new Error("invalid_key_bundle_lengths");
  }
  const key = await deriveBundleKey(passphrase, salt);
  const decrypted = new Uint8Array(await crypto.subtle.decrypt(
    { name: "AES-GCM", iv, additionalData: KEY_BUNDLE_AAD, tagLength: 128 },
    key,
    encrypted
  ));
  try {
    return await nonExtractableSigner(decrypted, publicRaw);
  } finally {
    decrypted.fill(0);
  }
}

function downloadBundle(bundle) {
  const blob = new Blob([`${JSON.stringify(bundle, null, 2)}\n`], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `hepta-paper-raid-human-key-${Date.now()}.json`;
  link.rel = "noopener";
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function uuid() {
  return crypto.randomUUID();
}

const PLAYER_ERROR_MESSAGES = Object.freeze({
  authentication_required: "Your session expired. Sign in again. / 会话已过期，请重新登录。",
  forbidden: "This identity cannot perform that action. / 当前身份无权执行此操作。",
  csrf_replayed: "This page safety token was already used. Reload, then try again. / 页面安全令牌已使用，请刷新后重试。",
  conflict: "The authoritative state changed. Reload and review the current task. / 权威状态已变更，请刷新并检查当前任务。",
  invalid_request: "Some submitted information is invalid. Check the highlighted action. / 提交信息有误，请检查当前操作。",
  dependency_unavailable: "A required service is temporarily unavailable. Try again shortly. / 所需服务暂时不可用，请稍后重试。",
  rate_limited: "Too many actions were sent. Wait a moment, then retry once. / 操作过于频繁，请稍候再重试一次。",
  upstream_rejected: "The research authority rejected this action. Reload before retrying. / 研究权威系统拒绝了此操作，请刷新后再试。",
  not_found: "This task is no longer available. Return to the current Raid view. / 此任务已不可用，请返回当前远征页面。",
  internal_error: "The service could not finish the action. Nothing is auto-submitted; try again later. / 服务未能完成操作；系统不会自动重提，请稍后再试。",
  network_retry_exhausted: "The network did not recover. Check your connection, then try again. / 网络仍未恢复，请检查连接后重试。",
  mutation_retry_exhausted: "The action could not be confirmed safely. Reload before trying again. / 无法安全确认此操作，请刷新后再试。",
  csrf_refresh_failed: "This page could not refresh its safety token. Reload, then try again. / 页面安全令牌刷新失败，请刷新后重试。",
  login_failed: "That Alpha access key was not accepted. Check it or contact the operator. / Alpha 访问密钥未通过，请核对或联系运营者。",
  webcrypto_unavailable: "This browser cannot create the required signing key. Use a supported current browser. / 此浏览器无法创建签名密钥，请使用受支持的新版浏览器。",
  passphrases_must_match_and_contain_at_least_16_characters: "Use matching recovery passphrases of at least 16 characters. / 两次恢复口令必须一致，且至少 16 个字符。",
  generate_a_key_or_import_the_original_encrypted_bundle_first: "Create a key or import your original encrypted recovery bundle first. / 请先创建密钥，或导入原始加密恢复包。",
  key_bundle_or_passphrase_is_invalid: "The recovery bundle or passphrase is not valid. Check both and retry. / 恢复包或口令无效，请核对后重试。",
  queue_requires_exactly_one_active_agent_binding: "Pair exactly one active Agent Bridge before joining this queue. / 加入队列前，请仅保留一个有效 Agent Bridge 配对。",
  paper_authority_refresh_is_pending: "The Raid is loading newer authoritative state. Wait for the reload. / 远征正在加载最新权威状态，请等待页面刷新。",
});

function rawPlayerResult(value) {
  if (typeof value === "string") return value;
  try {
    const encoded = JSON.stringify(value, null, 2);
    return typeof encoded === "string" ? encoded : String(value);
  } catch (_) {
    return String(value);
  }
}

function playerErrorPresentation(value) {
  const raw = rawPlayerResult(value);
  const directCode = value && typeof value === "object" && !Array.isArray(value)
    && typeof value.error === "string"
    ? value.error
    : null;
  const candidates = directCode
    ? [directCode]
    : (raw.match(/[a-z][a-z0-9_]{2,}/g) || []);
  const code = candidates.find(candidate => Object.hasOwn(PLAYER_ERROR_MESSAGES, candidate));
  return code ? { code, message: PLAYER_ERROR_MESSAGES[code], raw } : null;
}

function renderPlayerErrorDisclosure(output, presentation) {
  if (!output || !output.parentElement) return;
  let details = output.nextElementSibling;
  if (!details || !details.classList.contains("player-error-details")) details = null;
  if (!presentation) {
    if (details) details.remove();
    delete output.dataset.playerErrorCode;
    delete output.dataset.playerMessage;
    return;
  }
  if (!details) {
    details = document.createElement("details");
    details.className = "player-error-details";
    const summary = document.createElement("summary");
    summary.textContent = "Advanced error details / 高级错误详情";
    const raw = document.createElement("code");
    raw.className = "player-error-raw";
    details.append(summary, raw);
    output.insertAdjacentElement("afterend", details);
  }
  details.open = false;
  details.querySelector(".player-error-raw").textContent = presentation.raw;
  output.dataset.playerErrorCode = presentation.code;
  output.dataset.playerMessage = "friendly";
}

function renderPlayerMessage(output, value) {
  if (!output) return;
  const rendered = String(value);
  const separator = rendered.indexOf(" / ");
  const chinese = separator >= 0 ? rendered.slice(separator + 3) : "";
  if (!/[\u3400-\u9fff]/u.test(chinese)) {
    output.textContent = rendered;
    return;
  }
  const english = document.createElement("span");
  english.textContent = rendered.slice(0, separator);
  const divider = document.createElement("span");
  divider.setAttribute("aria-hidden", "true");
  divider.textContent = " / ";
  const translation = document.createElement("span");
  translation.setAttribute("lang", "zh-Hans");
  translation.textContent = chinese;
  output.replaceChildren(english, divider, translation);
}

function show(output, value, ok = true) {
  const presentation = ok ? null : playerErrorPresentation(value);
  const rendered = presentation ? presentation.message : rawPlayerResult(value);
  if (output) {
    if (typeof output.setAttribute === "function") output.setAttribute("aria-live", "polite");
    renderPlayerMessage(output, rendered);
    output.classList.toggle("result-ok", ok);
    output.classList.toggle("result-error", !ok);
    renderPlayerErrorDisclosure(output, presentation);
  }
  const toast = document.querySelector("#toast");
  if (toast) {
    toast.textContent = rendered;
    toast.hidden = false;
    window.setTimeout(() => { toast.hidden = true; }, 5000);
  }
}

const PLAYER_FOCUS_CONTEXT_SCHEMA = "hepta.paper_raid.player_focus_context.v1";
const PLAYER_FOCUS_CONTEXT_KEY = "hepta.paper-raid.player-focus-context.v1";
const PLAYER_FOCUS_CONTEXT_MAX_AGE_MS = 30 * 60 * 1000;
const PLAYER_FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "summary",
  "[tabindex]:not([tabindex='-1'])",
].join(",");
const PLAYER_FOCUS_DATA_KEYS = Object.freeze([
  "paperId",
  "teamId",
  "challengeId",
  "workItemId",
  "ticketId",
  "proposalId",
  "assignmentId",
  "evaluationId",
  "reviewId",
  "revisionId",
  "sectionRevisionId",
  "runRecordId",
  "resourceId",
  "manifestId",
  "receiptId",
  "bindingId",
  "sessionId",
  "sectionKey",
  "participantSlot",
  "slot",
  "role",
  "kind",
  "actionKind",
  "command",
  "queueAction",
  "paperRoomReveal",
  "practiceAction",
  "practiceVersion",
]);

function safeFocusText(value) {
  const text = String(value || "");
  return text.length <= 128 && /^[A-Za-z0-9_.:\/-]*$/.test(text) ? text : "";
}

function focusNodeDescriptor(node, target = false) {
  const descriptor = { tag: node.tagName.toLowerCase() };
  const id = safeFocusText(node.id);
  if (id) descriptor.id = id;
  const name = safeFocusText(node.getAttribute("name"));
  if (name) descriptor.name = name;
  const type = safeFocusText(node.getAttribute("type"));
  if (type) descriptor.type = type;
  const classes = Array.from(node.classList || [])
    .map(safeFocusText)
    .filter(Boolean)
    .sort()
    .slice(0, 8);
  if (classes.length > 0) descriptor.classes = classes;
  const data = {};
  for (const key of PLAYER_FOCUS_DATA_KEYS) {
    const value = safeFocusText(node.dataset && node.dataset[key]);
    if (value) data[key] = value;
  }
  if (Object.keys(data).length > 0) descriptor.data = data;
  if (target && ["button", "submit", "reset", "radio", "checkbox"].includes(type || descriptor.tag)) {
    const actionValue = safeFocusText(node.getAttribute("value"));
    if (actionValue) descriptor.actionValue = actionValue;
  }
  return descriptor;
}

function playerFocusSignature(element) {
  if (!(element instanceof HTMLElement) || !element.matches(PLAYER_FOCUSABLE_SELECTOR)) return null;
  const scopes = [];
  for (let node = element.parentElement; node && node !== document.body; node = node.parentElement) {
    if (!node.matches("form,article,section,details,[data-paper-id]")) continue;
    const descriptor = focusNodeDescriptor(node);
    if (Object.keys(descriptor).length > 1) scopes.push(descriptor);
    if (scopes.length >= 8) break;
  }
  return JSON.stringify({ target: focusNodeDescriptor(element, true), scopes });
}

function savePlayerFocusContext(element) {
  const signature = playerFocusSignature(element);
  if (!signature || signature.length > 4096) return false;
  const payload = {
    schema: PLAYER_FOCUS_CONTEXT_SCHEMA,
    path: window.location.pathname,
    signature,
    saved_at_ms: Date.now(),
  };
  try {
    sessionStorage.setItem(PLAYER_FOCUS_CONTEXT_KEY, JSON.stringify(payload));
    return true;
  } catch (_) {
    return false;
  }
}

function reloadNavigationType() {
  if (typeof performance === "undefined" || typeof performance.getEntriesByType !== "function") {
    return null;
  }
  const entries = performance.getEntriesByType("navigation");
  return entries.length === 1 ? entries[0].type : null;
}

function recentPlayerFocusContext(navigationType = reloadNavigationType()) {
  if (navigationType !== "reload" && navigationType !== "navigate") return false;
  let payload;
  try {
    const encoded = sessionStorage.getItem(PLAYER_FOCUS_CONTEXT_KEY);
    if (!encoded || encoded.length > 8192) return null;
    payload = JSON.parse(encoded);
  } catch (_) {
    return null;
  }
  if (!exactKeys(payload, ["schema", "path", "signature", "saved_at_ms"])) return null;
  const ageMs = Date.now() - payload.saved_at_ms;
  if (
      payload.schema !== PLAYER_FOCUS_CONTEXT_SCHEMA ||
      payload.path !== window.location.pathname ||
      typeof payload.signature !== "string" || payload.signature.length < 1 ||
      payload.signature.length > 4096 || !Number.isSafeInteger(payload.saved_at_ms) ||
      ageMs < 0 || ageMs > PLAYER_FOCUS_CONTEXT_MAX_AGE_MS) {
    return null;
  }
  if (
      navigationType === "navigate" &&
      (payload.path !== "/league/practice" || !payload.signature.includes("practice-agent-wait"))) {
    return null;
  }
  return payload;
}

function restorePlayerFocusContext(navigationType = reloadNavigationType()) {
  const payload = recentPlayerFocusContext(navigationType);
  if (!payload) return false;
  const matches = Array.from(document.querySelectorAll(PLAYER_FOCUSABLE_SELECTOR))
    .filter(candidate => playerFocusSignature(candidate) === payload.signature);
  if (matches.length !== 1) return false;
  const target = matches[0];
  if (target.disabled || target.closest("[hidden],[aria-hidden='true']")) return false;
  for (let node = target.parentElement; node && node !== document.body; node = node.parentElement) {
    if (node instanceof HTMLDetailsElement) node.open = true;
  }
  target.focus({ preventScroll: true });
  if (typeof target.scrollIntoView === "function") {
    const reduceMotion = window.matchMedia
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    target.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "center" });
  }
  return true;
}

function focusProgressedPracticeAction(navigationType = reloadNavigationType()) {
  const payload = recentPlayerFocusContext(navigationType);
  if (
      !payload || window.location.pathname !== "/league/practice" ||
      (!payload.signature.includes("practice-start-form") &&
       !payload.signature.includes("practiceAction") &&
       !payload.signature.includes("practice-agent-wait") &&
       !payload.signature.includes("practice-abandon-form"))) {
    return false;
  }
  const action = document.querySelector([
    ".practice-advance-form.primary-action",
    ".practice-agent-wait.primary-action",
    ".practice-start-form.primary-action",
    ".practice-complete.primary-action",
    ".practice-abandoned.primary-action",
  ].join(","));
  const target = paperRoomFocusTarget(action);
  if (!(target instanceof HTMLElement) || target.closest("[hidden],[aria-hidden='true']")) {
    return false;
  }
  target.focus({ preventScroll: true });
  if (typeof target.scrollIntoView === "function") {
    const reduceMotion = window.matchMedia
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    target.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "center" });
  }
  return true;
}

function bindPlayerFocusContext() {
  document.addEventListener("focusin", event => {
    savePlayerFocusContext(event.target);
  });
}

async function responseValue(response) {
  const text = await response.text();
  if (!text) return null;
  try { return JSON.parse(text); } catch (_) { return { error: "non_json_response" }; }
}

async function refreshCsrf() {
  const response = await fetch("/session/refresh", {
    method: "POST",
    credentials: "same-origin",
    headers: { "accept": "application/json" }
  });
  if (response.status === 401) {
    window.location.assign("/login");
    throw new Error("authentication_required");
  }
  const value = await responseValue(response);
  if (!response.ok || !value || typeof value.csrf !== "string") {
    throw new Error(value && value.error ? value.error : "csrf_refresh_failed");
  }
  csrfToken = value.csrf;
  return csrfToken;
}

async function mutation(url, init) {
  let networkFailure = false;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    if (!csrfToken) await refreshCsrf();
    const headers = new Headers(init.headers || {});
    headers.set("x-paper-raid-csrf", csrfToken);
    try {
      const response = await fetch(url, {
        ...init,
        headers,
        credentials: "same-origin"
      });
      const rotated = response.headers.get("x-paper-raid-csrf");
      if (rotated) csrfToken = rotated;
      if (response.status === 401) {
        window.location.assign("/login");
        return response;
      }
      if (response.status === 409 && attempt === 0) {
        csrfToken = null;
        await refreshCsrf();
        continue;
      }
      return response;
    } catch (error) {
      networkFailure = true;
      if (attempt === 0) {
        csrfToken = null;
        await refreshCsrf();
        continue;
      }
      throw error;
    }
  }
  throw new Error(networkFailure ? "network_retry_exhausted" : "mutation_retry_exhausted");
}

async function sendCommand(command, resourceId, childId, payload, sessionId = null) {
  const supplied = typeof payload.idempotency_key === "string" ? payload.idempotency_key : "";
  const idempotencyKey = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(supplied)
    ? supplied
    : uuid();
  payload.idempotency_key = idempotencyKey;
  const envelope = {
    command,
    resource_id: resourceId || null,
    child_id: childId || null,
    session_id: sessionId || null,
    idempotency_key: idempotencyKey,
    payload
  };
  return mutation("/api/hepta/commands", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify(envelope)
  });
}

async function currentSession() {
  const response = await fetch("/api/session", {
    method: "GET",
    credentials: "same-origin",
    headers: { "accept": "application/json" }
  });
  const value = await responseValue(response);
  if (!response.ok || !value || typeof value.player_id !== "string" || !Array.isArray(value.scopes)) {
    throw new Error(value && value.error ? value.error : "session_read_failed");
  }
  return value;
}

async function requireActiveAgentBinding(bindingId) {
  const expected = canonicalUuid(bindingId, "binding_id");
  const response = await fetch("/api/agent-bindings", {
    method: "GET",
    credentials: "same-origin",
    headers: { "accept": "application/json" }
  });
  const value = await responseValue(response);
  if (!response.ok || !Array.isArray(value)) {
    throw new Error(value && value.error ? value.error : "agent_binding_read_failed");
  }
  const binding = value.find(candidate => candidate &&
    String(candidate.binding_id || "").toLowerCase() === expected &&
    candidate.status === "active");
  if (!binding) throw new Error("frozen_team_agent_binding_is_not_currently_active");
  return binding;
}

const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/;
const LOGICAL_SESSION_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;

function positiveInteger(value, field) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) throw new Error(`${field}_must_be_a_positive_integer`);
  return parsed;
}

function canonicalDigest(value, field) {
  const digest = String(value || "").trim().toLowerCase();
  if (!DIGEST_PATTERN.test(digest)) throw new Error(`${field}_must_be_a_sha256_digest`);
  return digest;
}

function phaseTransitionPayload(version, nextPhase) {
  const allowed = new Set([
    "preregistering", "researching", "experimenting", "drafting",
    "integrity_review", "reproducing", "author_approval"
  ]);
  if (!allowed.has(nextPhase)) throw new Error("invalid_next_paper_phase");
  return { expected_version: positiveInteger(version, "paper_version"), next_phase: nextPhase };
}

function workItemPayload(version, kind, title, playerId = null, bindingId = null) {
  const cleanTitle = String(title || "").trim();
  const cleanKind = String(kind || "").trim();
  if (cleanTitle.length < 3 || !cleanKind) throw new Error("work_item_title_and_kind_are_required");
  if ((playerId === null) !== (bindingId === null)) throw new Error("work_item_assignment_must_be_complete");
  return {
    work_item_id: uuid(),
    expected_paper_version: positiveInteger(version, "paper_version"),
    kind: cleanKind,
    title: cleanTitle,
    assigned_player_id: playerId,
    assigned_binding_id: bindingId
  };
}

function workItemTransitionPayload(version, nextStatus, artifactManifestHash) {
  const allowed = new Set(["in_progress", "review", "accepted", "rejected", "cancelled"]);
  if (!allowed.has(nextStatus)) throw new Error("invalid_next_work_item_status");
  return {
    expected_version: positiveInteger(version, "work_item_version"),
    next_status: nextStatus,
    artifact_manifest_hash: nextStatus === "accepted"
      ? canonicalDigest(artifactManifestHash, "artifact_manifest_hash")
      : null
  };
}

function paperRevisionPayload(version, parentRevisionId, values) {
  return {
    revision_id: uuid(),
    expected_paper_version: positiveInteger(version, "paper_version"),
    parent_revision_id: parentRevisionId || null,
    source_manifest_hash: canonicalDigest(values.sourceManifestHash, "source_manifest_hash"),
    artifact_manifest_hash: canonicalDigest(values.artifactManifestHash, "artifact_manifest_hash"),
    bibliography_hash: canonicalDigest(values.bibliographyHash, "bibliography_hash"),
    claim_evidence_graph_hash: canonicalDigest(values.claimEvidenceGraphHash, "claim_evidence_graph_hash")
  };
}

function canonicalUuid(value, field) {
  const clean = String(value || "").trim().toLowerCase();
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(clean)) {
    throw new Error(`${field}_must_be_a_canonical_uuid`);
  }
  return clean;
}

const PAPER_WORKFLOW_STALE_MAX_WAIT_MS = 120_000;
const paperWorkflowStates = new Map();

function paperWorkflowState(paperId) {
  const canonicalPaperId = canonicalUuid(paperId, "paper_id");
  let state = paperWorkflowStates.get(canonicalPaperId);
  if (!state) {
    state = {
      active: 0,
      pendingReloadDelayMs: null,
      pendingStaleApply: null,
      reloadDeadlineMs: null,
      reloadStarted: false,
      reloadTimer: null,
      staleWatchdogTimer: null,
    };
    paperWorkflowStates.set(canonicalPaperId, state);
  }
  return { canonicalPaperId, state };
}

function forcePaperReload(state) {
  if (state.reloadStarted) return;
  if (state.reloadTimer !== null) window.clearTimeout(state.reloadTimer);
  state.pendingReloadDelayMs = null;
  state.reloadDeadlineMs = null;
  state.reloadTimer = null;
  state.reloadStarted = true;
  window.location.reload();
}

function armPaperReload(canonicalPaperId, state, delayMs) {
  if (state.reloadStarted) return;
  const deadlineMs = Date.now() + delayMs;
  if (state.reloadDeadlineMs !== null && state.reloadDeadlineMs <= deadlineMs) return;
  if (state.reloadTimer !== null) window.clearTimeout(state.reloadTimer);
  state.reloadDeadlineMs = deadlineMs;
  state.reloadTimer = window.setTimeout(() => {
    if (state.reloadStarted) return;
    state.reloadTimer = null;
    state.reloadDeadlineMs = null;
    if (state.active > 0) {
      state.pendingReloadDelayMs = 0;
      return;
    }
    state.reloadStarted = true;
    window.location.reload();
  }, Math.max(0, deadlineMs - Date.now()));
  paperWorkflowStates.set(canonicalPaperId, state);
}

function requestPaperReload(paperId, delayMs) {
  if (!Number.isSafeInteger(delayMs) || delayMs < 0 || delayMs > 10_000) {
    throw new Error("paper_reload_delay_is_invalid");
  }
  const { canonicalPaperId, state } = paperWorkflowState(paperId);
  if (state.reloadStarted) return;
  if (state.active > 0) {
    state.pendingReloadDelayMs = state.pendingReloadDelayMs === null
      ? delayMs
      : Math.min(state.pendingReloadDelayMs, delayMs);
    return;
  }
  armPaperReload(canonicalPaperId, state, delayMs);
}

function requestStaleAuthorityRefresh(paperId, applyInvalidation) {
  if (typeof applyInvalidation !== "function") {
    throw new Error("stale_authority_invalidation_must_be_a_function");
  }
  const { canonicalPaperId, state } = paperWorkflowState(paperId);
  if (!state.pendingStaleApply) state.pendingStaleApply = applyInvalidation;
  if (state.active > 0) {
    if (state.staleWatchdogTimer === null) {
      state.staleWatchdogTimer = window.setTimeout(() => {
        state.staleWatchdogTimer = null;
        const apply = state.pendingStaleApply;
        state.pendingStaleApply = null;
        try {
          if (apply) apply();
        } finally {
          forcePaperReload(state);
        }
      }, PAPER_WORKFLOW_STALE_MAX_WAIT_MS);
    }
    return true;
  }
  const apply = state.pendingStaleApply;
  state.pendingStaleApply = null;
  try {
    apply();
  } catch (error) {
    forcePaperReload(state);
    throw error;
  }
  return false;
}

function beginPaperWorkflow(paperId) {
  const { canonicalPaperId, state } = paperWorkflowState(paperId);
  if (state.pendingStaleApply || state.pendingReloadDelayMs !== null ||
      state.reloadTimer !== null || state.reloadStarted) {
    throw new Error("paper_authority_refresh_is_pending");
  }
  state.active += 1;
  let finished = false;
  return () => {
    if (finished) throw new Error("paper_workflow_finished_more_than_once");
    finished = true;
    state.active -= 1;
    if (state.active < 0) throw new Error("paper_workflow_active_count_is_invalid");
    if (state.active > 0) return;
    if (state.pendingStaleApply) {
      if (state.staleWatchdogTimer !== null) {
        window.clearTimeout(state.staleWatchdogTimer);
        state.staleWatchdogTimer = null;
      }
      const apply = state.pendingStaleApply;
      state.pendingStaleApply = null;
      try {
        apply();
      } catch (error) {
        forcePaperReload(state);
        throw error;
      }
    }
    if (state.pendingReloadDelayMs === null) {
      if (state.reloadTimer === null && !state.reloadStarted) {
        paperWorkflowStates.delete(canonicalPaperId);
      }
      return;
    }
    const delayMs = state.pendingReloadDelayMs;
    state.pendingReloadDelayMs = null;
    armPaperReload(canonicalPaperId, state, delayMs);
  };
}

function formMutationMustStayDisabled(form) {
  if (!form || !form.dataset) return true;
  if (form.dataset.authorityState === "stale" ||
      form.dataset.reworkLeaseState === "expired") {
    return true;
  }
  const paperId = form.dataset.paperId;
  if (!paperId) return false;
  try {
    const state = paperWorkflowStates.get(canonicalUuid(paperId, "paper_id"));
    return Boolean(state && (
      state.pendingStaleApply ||
      state.pendingReloadDelayMs !== null ||
      state.reloadTimer !== null ||
      state.reloadStarted
    ));
  } catch (_) {
    return true;
  }
}

function restoreAuthoritativeFormControls(form, controls) {
  const disabled = formMutationMustStayDisabled(form);
  for (const control of controls) control.disabled = disabled;
}

function nonNegativeInteger(value, field) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`${field}_must_be_a_non_negative_integer`);
  }
  return parsed;
}

function acquireSectionLeasePayload(values) {
  const sectionKey = String(values.sectionKey || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(sectionKey)) throw new Error("section_key_must_be_a_safe_logical_identifier");
  canonicalUuid(values.currentRevisionId, "current_revision_id");
  return {
    lease_id: uuid(),
    section_key: sectionKey,
    holder_binding_id: canonicalUuid(values.holderBindingId, "holder_binding_id"),
    expected_previous_fencing_token: nonNegativeInteger(
      values.previousFencingToken,
      "previous_fencing_token"
    ),
    ttl_seconds: positiveInteger(values.ttlSeconds, "ttl_seconds")
  };
}

async function humanDecisionPayload(values) {
  const decision = String(values.decision || "");
  if (!new Set(["accept", "rework", "reject"]).has(decision)) {
    throw new Error("invalid_human_decision");
  }
  return {
    decision_id: uuid(),
    proposal_id: canonicalUuid(values.proposalId, "proposal_id"),
    expected_proposal_version: positiveInteger(values.proposalVersion, "proposal_version"),
    decision,
    reason_hash: await semanticDigest(values.reason, "decision_reason")
  };
}

function sectionRevisionPayload(values) {
  const sectionKey = String(values.sectionKey || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(sectionKey)) throw new Error("section_key_must_be_a_safe_logical_identifier");
  return {
    section_revision_id: uuid(),
    section_key: sectionKey,
    parent_revision_id: canonicalUuid(values.parentRevisionId, "parent_revision_id"),
    proposal_id: canonicalUuid(values.proposalId, "proposal_id"),
    lease_id: canonicalUuid(values.leaseId, "lease_id"),
    fencing_token: positiveInteger(values.fencingToken, "fencing_token"),
    patch_manifest_id: canonicalUuid(values.patchManifestId, "patch_manifest_id"),
    patch_hash: canonicalDigest(values.patchHash, "patch_hash")
  };
}

async function sectionReviewPayload(values) {
  const verdict = String(values.verdict || "");
  if (!new Set(["approve", "rework", "reject"]).has(verdict)) {
    throw new Error("invalid_section_review_verdict");
  }
  return {
    review_id: uuid(),
    expected_revision_version: positiveInteger(values.revisionVersion, "revision_version"),
    verdict,
    review_hash: await semanticDigest(values.review, "section_review")
  };
}

function sectionMergePayload(values) {
  const sectionRevisionId = canonicalUuid(values.sectionRevisionId, "section_revision_id");
  return {
    merge_id: uuid(),
    section_revision_id: sectionRevisionId,
    expected_revision_version: positiveInteger(values.revisionVersion, "revision_version"),
    parent_revision_id: canonicalUuid(values.parentRevisionId, "parent_revision_id"),
    merged_section_revision_id: sectionRevisionId,
    lease_id: canonicalUuid(values.leaseId, "lease_id"),
    fencing_token: positiveInteger(values.fencingToken, "fencing_token")
  };
}

function safeInteger(value, field) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new Error(`${field}_must_be_a_safe_integer`);
  return parsed;
}

function canonicalHttpsUrl(value, field) {
  const clean = String(value || "").trim();
  let parsed;
  try { parsed = new URL(clean); } catch (_) { throw new Error(`${field}_must_be_a_canonical_https_url`); }
  if (parsed.protocol !== "https:" || parsed.username || parsed.password || parsed.hash) {
    throw new Error(`${field}_must_be_a_credential_free_https_url_without_fragment`);
  }
  return clean;
}

async function semanticDigest(value, field) {
  const clean = String(value || "").trim();
  if (!clean) throw new Error(`${field}_is_required`);
  return sha256Label(new TextEncoder().encode(clean));
}

async function preserveDisclosure(form, paperId, hashField, textField) {
  const existing = form.elements[hashField];
  const existingHash = String(existing && existing.value || "").trim();
  if (existingHash) return canonicalDigest(existingHash, hashField);
  const input = form.elements[textField];
  const plainText = String(input && input.value || "").trim();
  if (!plainText) throw new Error(`${textField}_is_required`);
  const mediaType = "text/plain; charset=utf-8";
  const receipt = await uploadCasArtifact(
    paperId,
    new Blob([plainText], { type: mediaType }),
    mediaType
  );
  return canonicalDigest(receipt.digest, hashField);
}

function selectedValues(select) {
  if (!select) return [];
  return Array.from(select.selectedOptions, option => option.value).filter(Boolean);
}

async function experimentPlanPayload(values) {
  const manifestIds = [values.codeManifestId, values.datasetManifestId, values.environmentManifestId];
  if (manifestIds.some(value => !value) || new Set(manifestIds).size !== 3) {
    throw new Error("experiment_plan_requires_three_distinct_manifests");
  }
  return {
    experiment_plan_id: uuid(),
    protocol_snapshot_hash: await semanticDigest(values.protocol, "protocol_snapshot"),
    code_manifest_id: manifestIds[0],
    dataset_manifest_id: manifestIds[1],
    environment_manifest_id: manifestIds[2],
    seed_policy_hash: await semanticDigest(values.seedPolicy, "seed_policy"),
    stopping_rule_hash: await semanticDigest(values.stoppingRule, "stopping_rule")
  };
}

function evidenceVerificationPayload(values) {
  const locator = String(values.locator || "").trim();
  const license = String(values.license || "").trim();
  if (!locator || !license) throw new Error("evidence_locator_and_license_are_required");
  return {
    evidence_card_id: uuid(),
    source_uri: canonicalHttpsUrl(values.sourceUri, "source_uri"),
    source_hash: canonicalDigest(values.sourceHash, "source_hash"),
    locator,
    license
  };
}

function citationVerificationPayload(values) {
  if (!values.evidenceCardId) throw new Error("verified_evidence_is_required");
  const doi = String(values.doi || "").trim() || null;
  const canonicalUrl = String(values.canonicalUrl || "").trim();
  if (!doi && !canonicalUrl) throw new Error("citation_requires_a_doi_or_canonical_url");
  if (doi && (!doi.startsWith("10.") || !doi.includes("/") || /\s/.test(doi))) {
    throw new Error("doi_must_use_the_canonical_10_registrant_suffix_form");
  }
  return {
    citation_id: uuid(),
    evidence_card_id: values.evidenceCardId,
    doi,
    canonical_url: canonicalUrl ? canonicalHttpsUrl(canonicalUrl, "canonical_url") : null
  };
}

async function runRecordPayload(values) {
  const status = String(values.status || "");
  if (!new Set(["succeeded", "failed", "cancelled"]).has(status)) {
    throw new Error("invalid_run_status");
  }
  if (!values.experimentPlanId || !values.logsManifestId) {
    throw new Error("run_requires_an_experiment_plan_and_logs_manifest");
  }
  const succeeded = status === "succeeded";
  if (succeeded && (!values.outputsManifestId || values.outputsManifestId === values.logsManifestId)) {
    throw new Error("successful_run_requires_a_distinct_outputs_manifest");
  }
  const metricsHash = succeeded
    ? (values.metricsHash
      ? canonicalDigest(values.metricsHash, "metrics_hash")
      : await semanticDigest(values.metrics, "metrics"))
    : null;
  return {
    run_record_id: uuid(),
    experiment_plan_id: values.experimentPlanId,
    status,
    seed: safeInteger(values.seed, "seed"),
    parameters_hash: await semanticDigest(values.parameters, "parameters"),
    logs_manifest_id: values.logsManifestId,
    outputs_manifest_id: succeeded ? values.outputsManifestId : null,
    metrics_hash: metricsHash,
    failure_hash: succeeded ? null : await semanticDigest(values.failure, "failure")
  };
}

function figureLineagePayload(values) {
  const figureKey = String(values.figureKey || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(figureKey)) {
    throw new Error("figure_key_must_be_a_safe_logical_identifier");
  }
  const runRecordIds = [...new Set(
    values.runRecordIds.map(value => canonicalUuid(value, "run_record_id"))
  )];
  if (runRecordIds.length === 0 || runRecordIds.length !== values.runRecordIds.length) {
    throw new Error("figure_lineage_requires_distinct_authoritative_runs");
  }
  return {
    figure_lineage_id: uuid(),
    figure_key: figureKey,
    figure_manifest_id: canonicalUuid(values.figureManifestId, "figure_manifest_id"),
    run_record_ids: runRecordIds,
    transform_hash: canonicalDigest(values.transformHash, "transform_hash")
  };
}

async function claimRecordPayload(values) {
  const kind = String(values.claimKind || "");
  if (!new Set(["main", "numeric", "figure", "supporting", "limitation"]).has(kind)) {
    throw new Error("invalid_claim_kind");
  }
  const lineageCount = values.evidenceCardIds.length + values.runRecordIds.length + values.figureLineageIds.length;
  if (new Set(["main", "numeric", "figure"]).has(kind) && lineageCount === 0) {
    throw new Error("this_claim_kind_requires_evidence_run_or_figure_lineage");
  }
  const claimKey = String(values.claimKey || "").trim();
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(claimKey)) {
    throw new Error("claim_key_must_be_a_safe_logical_identifier");
  }
  return {
    claim_id: uuid(),
    claim_key: claimKey,
    claim_kind: kind,
    statement_hash: await semanticDigest(values.statement, "statement"),
    evidence_card_ids: values.evidenceCardIds,
    run_record_ids: values.runRecordIds,
    figure_lineage_ids: values.figureLineageIds
  };
}

function canonicalJson(value) {
  const sort = item => {
    if (Array.isArray(item)) return item.map(sort);
    if (item && typeof item === "object") {
      return Object.fromEntries(
        Object.keys(item).sort().map(key => [key, sort(item[key])]),
      );
    }
    return item;
  };
  return JSON.stringify(sort(value));
}

const CONTRIBUTION_LEDGER_NAMESPACE = "0870ff5a-7cfb-5bf0-8511-3f08c0626152";
const CONTRIBUTION_LEDGER_SCHEMA = "hepta.paper_raid.contribution_ledger.v1";
const ACCEPTED_ARTIFACT_MILESTONE_XP = 100;
const ACCEPTED_REVIEW_MILESTONE_XP = 150;
const CREDIT_ROLES = new Set([
  "conceptualization", "data_curation", "formal_analysis", "funding_acquisition",
  "investigation", "methodology", "project_administration", "resources", "software",
  "supervision", "validation", "visualization", "writing_original_draft",
  "writing_review_editing"
]);

function uuidBytes(value, field) {
  const canonical = canonicalUuid(value, field);
  return Uint8Array.from(
    canonical.replaceAll("-", "").match(/.{2}/g),
    byte => Number.parseInt(byte, 16)
  );
}

function bytesUuid(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 16) throw new Error("uuid_bytes_must_be_16_bytes");
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function uuidV5(namespace, name) {
  const namespaceBytes = uuidBytes(namespace, "uuid_namespace");
  const nameBytes = new TextEncoder().encode(String(name));
  const material = new Uint8Array(namespaceBytes.length + nameBytes.length);
  material.set(namespaceBytes);
  material.set(nameBytes, namespaceBytes.length);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-1", material));
  const result = digest.slice(0, 16);
  result[6] = (result[6] & 0x0f) | 0x50;
  result[8] = (result[8] & 0x3f) | 0x80;
  return bytesUuid(result);
}

function normalizedCreditRoles(values) {
  if (!Array.isArray(values) || values.length < 1 || values.length > 32) {
    throw new Error("credit_roles_must_contain_1_to_32_values");
  }
  const roles = values.map(value => String(value || "").trim());
  if (roles.some(role => !CREDIT_ROLES.has(role)) || new Set(roles).size !== roles.length) {
    throw new Error("credit_roles_must_be_distinct_and_non_empty");
  }
  return roles.sort();
}

function normalizedContributionRefs(values, field) {
  if (!Array.isArray(values) || values.length > 256) {
    throw new Error(`${field}_must_contain_at_most_256_values`);
  }
  const refs = values.map(value => canonicalUuid(value, field)).sort();
  if (refs.some((value, index) => index > 0 && refs[index - 1] === value)) {
    throw new Error(`${field}_must_not_contain_duplicates`);
  }
  return refs;
}

function contributionRefs(node) {
  return {
    accepted_artifact_manifest_ids: Array.from(
      node.querySelectorAll(".contribution-artifact-ref"),
      reference => reference.dataset.manifestId
    ),
    accepted_section_review_ids: Array.from(
      node.querySelectorAll(".contribution-review-ref"),
      reference => reference.dataset.reviewId
    )
  };
}

async function contributionLedgerBudget(paperId, revisionId, authors) {
  const canonicalPaperId = canonicalUuid(paperId, "paper_id");
  const canonicalRevisionId = canonicalUuid(revisionId, "revision_id");
  if (!Array.isArray(authors) || authors.length < 3 || authors.length > 5) {
    throw new Error("contribution_ledger_requires_the_exact_3_to_5_author_roster");
  }
  const seen = new Set();
  const requestEntries = authors.map(author => {
    const playerId = canonicalUuid(author.player_id, "author_player_id");
    if (seen.has(playerId)) throw new Error("contribution_ledger_author_is_duplicated");
    seen.add(playerId);
    const artifacts = normalizedContributionRefs(
      author.accepted_artifact_manifest_ids,
      "accepted_artifact_manifest_id"
    );
    const reviews = normalizedContributionRefs(
      author.accepted_section_review_ids,
      "accepted_section_review_id"
    );
    return {
      player_id: playerId,
      credit_roles: normalizedCreditRoles(author.credit_roles),
      accepted_artifact_manifest_ids: artifacts,
      accepted_section_review_ids: reviews
    };
  }).sort((left, right) => left.player_id < right.player_id ? -1 : left.player_id > right.player_id ? 1 : 0);
  const contributionLedgerId = await uuidV5(
    CONTRIBUTION_LEDGER_NAMESPACE,
    `${CONTRIBUTION_LEDGER_SCHEMA}\n${canonicalPaperId}\n${canonicalRevisionId}`
  );
  const frozenEntries = requestEntries.map(entry => ({
    ...entry,
    contribution_points:
      (entry.accepted_artifact_manifest_ids.length > 0 ? ACCEPTED_ARTIFACT_MILESTONE_XP : 0) +
      (entry.accepted_section_review_ids.length > 0 ? ACCEPTED_REVIEW_MILESTONE_XP : 0)
  }));
  const ledgerRecord = {
    schema: CONTRIBUTION_LEDGER_SCHEMA,
    contribution_ledger_id: contributionLedgerId,
    paper_project_id: canonicalPaperId,
    entries: frozenEntries
  };
  return Object.freeze({
    contributionLedgerId,
    ledgerHash: await sha256Label(new TextEncoder().encode(canonicalJson(ledgerRecord))),
    requestEntries,
    frozenEntries
  });
}

function frozenLedgerAuthors(form) {
  const authors = Array.from(form.querySelectorAll(".ledger-author"), author => ({
    player_id: author.dataset.playerId,
    credit_roles: Array.from(
      author.querySelectorAll(".ledger-credit-role"),
      role => role.dataset.role
    ),
    ...contributionRefs(author)
  }));
  if (authors.some(author => author.credit_roles.length === 0)) {
    throw new Error("frozen_release_credit_roles_are_missing");
  }
  return authors;
}

function promoteReleaseAuthors(form) {
  const authors = Array.from(form.querySelectorAll(".release-author"), (fieldset, index) => {
    const displayName = fieldset.elements.display_name.value.trim();
    if (!displayName) throw new Error("registered_display_name_is_required");
    return {
      author_order: index + 1,
      participant_slot: positiveInteger(fieldset.dataset.participantSlot, "participant_slot"),
      player_id: canonicalUuid(fieldset.dataset.playerId, "author_player_id"),
      display_name: displayName,
      credit_roles: normalizedCreditRoles(
        fieldset.elements.credit_roles.value.split(",").map(value => value.trim()).filter(Boolean)
      ),
      ...contributionRefs(fieldset)
    };
  });
  if (authors.length < 3 || authors.length > 5 ||
      new Set(authors.map(author => author.player_id)).size !== authors.length ||
      new Set(authors.map(author => author.participant_slot)).size !== authors.length) {
    throw new Error("release_authors_must_be_the_exact_unique_3_to_5_member_roster");
  }
  return authors;
}

async function neutralBundleRawSha256(bundle) {
  const digest = await sha256Label(new TextEncoder().encode(`${canonicalJson(bundle)}\n`));
  return digest.slice("sha256:".length);
}

function safeLogicalFilename(name, fallback) {
  const leaf = String(name || "").split(/[\\/]/).pop() || fallback;
  const safe = leaf
    .normalize("NFKC")
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 120);
  return safe && safe !== "." && safe !== ".." ? safe : fallback;
}

async function uploadCasArtifact(paperId, file, mediaType, allowEmpty = false) {
  if (!file || (!allowEmpty && file.size < 1) || file.size > 32 * 1024 * 1024) {
    throw new Error(allowEmpty
      ? "artifact_size_must_be_0_to_33554432_bytes"
      : "artifact_size_must_be_1_to_33554432_bytes");
  }
  const digest = await sha256Label(await file.arrayBuffer());
  const response = await mutation(
    `/api/papers/${encodeURIComponent(paperId)}/artifacts/${encodeURIComponent(digest)}`,
    {
      method: "PUT",
      headers: { "content-type": mediaType, "accept": "application/json" },
      body: file
    }
  );
  const value = await responseValue(response);
  if (!response.ok) throw new Error(value && value.error ? value.error : "artifact_upload_failed");
  const raw = digest.slice("sha256:".length);
  if (!value || value.digest !== digest || value.artifact_sha256 !== raw ||
      value.uri !== `cas://sha256/${raw}` || value.media_type !== mediaType ||
      value.size !== file.size || value.acl !== "team") {
    throw new Error("artifact_upload_receipt_is_not_authoritative_or_exact");
  }
  return value;
}

async function uploadAndRegisterManifest(values) {
  const requiredRunIds = [...new Set(values.requiredRunIds.map(value => String(value).trim()).filter(Boolean))];
  if (requiredRunIds.length === 0) throw new Error("artifact_manifest_requires_run_lineage");
  const uploaded = [];
  for (const item of values.objects) {
    const stored = await uploadCasArtifact(
      values.paperId,
      item.file,
      item.mediaType,
      item.allowEmpty === true
    );
    uploaded.push({ ...item, stored });
  }
  const objects = uploaded
    .map(item => ({
      canonical_json: false,
      dependencies: [],
      logical_path: item.logicalPath,
      media_type: item.mediaType,
      role: item.role,
      sha256: item.stored.artifact_sha256,
      size: item.stored.size
    }))
    .sort((left, right) => left.logical_path < right.logical_path ? -1 : left.logical_path > right.logical_path ? 1 : 0);
  if (new Set(objects.map(object => object.logical_path)).size !== objects.length) {
    throw new Error("artifact_logical_paths_must_be_distinct");
  }
  const sourceBundle = {
    artifact_root: {
      algorithm: "sha256-canonical-manifest-v1",
      digest_file: "artifact-bundle.v1.sha256"
    },
    bundle_id: `browser-${values.bundleKind}-${uuid()}`,
    challenge_id: values.challengeId,
    created_at: new Date().toISOString(),
    hepta_binding_status: "unbound",
    human_authority_materialized: false,
    object_count: objects.length,
    objects,
    required_run_ids: requiredRunIds,
    schema: "paper-raid.artifact-bundle.v1"
  };
  const manifestId = uuid();
  const expectedSourceManifestSha256 = await neutralBundleRawSha256(sourceBundle);
  const payload = {
    manifest_id: manifestId,
    expected_paper_version: positiveInteger(values.paperVersion, "paper_version"),
    expected_source_manifest_sha256: expectedSourceManifestSha256,
    source_bundle: sourceBundle,
    storage_locations: objects.map(object => {
      const stored = uploaded.find(item => item.logicalPath === object.logical_path).stored;
      return {
        logical_path: object.logical_path,
        sha256: object.sha256,
        uri: stored.uri,
        acl: "team"
      };
    })
  };
  const response = await sendCommand("register_artifact", values.paperId, null, payload);
  const result = await responseValue(response);
  if (!response.ok) throw new Error(result && result.error ? result.error : "artifact_manifest_registration_failed");
  return authoritativeArtifactRegistration(result, {
    manifestId,
    paperId: values.paperId,
    sourceBundleId: sourceBundle.bundle_id,
    expectedSourceManifestSha256,
    requiredRunIds,
    uploaded
  });
}

function authoritativeArtifactRegistration(result, expected) {
  if (!result || typeof result !== "object") {
    throw new Error("artifact_manifest_registration_receipt_is_missing");
  }
  const manifestId = canonicalUuid(result.manifest_id, "registered_manifest_id");
  const paperId = canonicalUuid(result.paper_project_id, "registered_paper_id");
  const expectedPaperId = canonicalUuid(expected.paperId, "paper_id");
  const manifestHash = canonicalDigest(result.manifest_hash, "registered_manifest_hash");
  const expectedManifestHash = `sha256:${expected.expectedSourceManifestSha256}`;
  if (manifestId !== canonicalUuid(expected.manifestId, "expected_manifest_id") ||
      paperId !== expectedPaperId ||
      result.source_bundle_id !== expected.sourceBundleId ||
      result.source_manifest_sha256 !== expected.expectedSourceManifestSha256 ||
      manifestHash !== expectedManifestHash ||
      result.object_count !== expected.uploaded.length ||
      result.version !== 1 ||
      !Array.isArray(result.required_run_ids) ||
      result.required_run_ids.length !== expected.requiredRunIds.length ||
      result.required_run_ids.some((value, index) => value !== expected.requiredRunIds[index])) {
    throw new Error("artifact_manifest_registration_receipt_is_not_authoritative_or_exact");
  }
  return {
    manifestId,
    manifestHash,
    sourceBundleId: result.source_bundle_id,
    requiredRunIds: [...expected.requiredRunIds],
    objects: expected.uploaded.map(item => ({
      logicalPath: item.logicalPath,
      role: item.role,
      digest: canonicalDigest(item.stored.digest, `${item.role}_digest`),
      uri: item.stored.uri,
      size: item.stored.size,
      mediaType: item.mediaType
    }))
  };
}

async function registerReviewReadyManifest(values) {
  const sources = [values.draft, values.frozenEvaluator, values.dataset, values.candidate];
  if (new Set(sources.map(source => source.manifestId)).size !== 4 ||
      new Set(sources.map(source => source.manifestHash)).size !== 4) {
    throw new Error("review_ready_sources_must_be_four_distinct_registered_manifests");
  }
  const requiredRunIds = [...new Set(
    sources.flatMap(source => source.requiredRunIds).map(String).filter(Boolean)
  )].sort();
  if (requiredRunIds.length === 0) throw new Error("review_ready_manifest_requires_run_lineage");
  const sourceObjects = sources.flatMap(source => source.objects);
  if (new Set(sourceObjects.map(object => object.logicalPath)).size !== sourceObjects.length ||
      new Set(sourceObjects.map(object => object.digest)).size !== sourceObjects.length) {
    throw new Error("review_ready_source_objects_must_have_distinct_paths_and_digests");
  }
  const objects = sourceObjects.map(object => ({
    canonical_json: false,
    dependencies: [],
    logical_path: object.logicalPath,
    media_type: object.mediaType,
    role: object.role,
    sha256: canonicalDigest(object.digest, `${object.role}_digest`).slice("sha256:".length),
    size: object.size
  })).sort((left, right) => left.logical_path < right.logical_path ? -1 :
    left.logical_path > right.logical_path ? 1 : 0);
  const manifestId = uuid();
  const sourceBundle = {
    artifact_root: {
      algorithm: "sha256-canonical-manifest-v1",
      digest_file: "artifact-bundle.v1.sha256"
    },
    bundle_id: `review-ready-${manifestId}`,
    challenge_id: values.challengeId,
    created_at: new Date().toISOString(),
    hepta_binding_status: "unbound",
    human_authority_materialized: false,
    object_count: objects.length,
    objects,
    required_run_ids: requiredRunIds,
    schema: "paper-raid.artifact-bundle.v1"
  };
  const reviewReadyAssembly = {
    schema: "hepta.paper_raid.review_ready_artifact_assembly.v1",
    draft: { manifest_id: values.draft.manifestId, manifest_hash: values.draft.manifestHash },
    frozen_evaluator: {
      manifest_id: values.frozenEvaluator.manifestId,
      manifest_hash: values.frozenEvaluator.manifestHash
    },
    dataset: { manifest_id: values.dataset.manifestId, manifest_hash: values.dataset.manifestHash },
    candidate: { manifest_id: values.candidate.manifestId, manifest_hash: values.candidate.manifestHash }
  };
  const expectedSourceManifestSha256 = await neutralBundleRawSha256(sourceBundle);
  const payload = {
    manifest_id: manifestId,
    expected_paper_version: positiveInteger(values.paperVersion, "paper_version"),
    expected_source_manifest_sha256: expectedSourceManifestSha256,
    source_bundle: sourceBundle,
    storage_locations: objects.map(object => {
      const source = sourceObjects.find(item => item.logicalPath === object.logical_path);
      return {
        logical_path: object.logical_path,
        sha256: object.sha256,
        uri: source.uri,
        acl: "reviewers"
      };
    }),
    review_ready_assembly: reviewReadyAssembly
  };
  const response = await sendCommand("register_artifact", values.paperId, null, payload);
  const result = await responseValue(response);
  if (!response.ok) {
    throw new Error(result && result.error ? result.error : "review_ready_manifest_registration_failed");
  }
  if (!result || canonicalUuid(result.manifest_id, "review_ready_manifest_id") !== manifestId ||
      canonicalUuid(result.paper_project_id, "review_ready_paper_id") !==
        canonicalUuid(values.paperId, "paper_id") ||
      canonicalDigest(result.manifest_hash, "review_ready_manifest_hash") !==
        `sha256:${expectedSourceManifestSha256}` ||
      result.binding_schema !== "hepta.paper_raid.review_ready_artifact_manifest_binding.v1" ||
      canonicalJson(result.review_ready_assembly) !== canonicalJson(reviewReadyAssembly) ||
      result.object_count !== objects.length || result.version !== 1) {
    throw new Error("review_ready_manifest_receipt_is_not_authoritative_or_exact");
  }
  return result;
}

async function createAuthoritativeRunFromArtifacts(form) {
  const runLabel = String(form.elements.run_label.value || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(runLabel)) {
    throw new Error("run_label_must_be_a_safe_logical_identifier");
  }
  const pathLabel = safeLogicalFilename(runLabel, "run");
  const stdoutFile = form.elements.stdout_file.files[0];
  const stderrFile = form.elements.stderr_file.files[0];
  const outputFile = form.elements.output_file.files[0];
  const metricsFile = form.elements.metrics_file.files[0];
  const status = String(form.elements.status.value || "");
  if (!new Set(["succeeded", "failed", "cancelled"]).has(status)) {
    throw new Error("invalid_run_status");
  }
  const common = {
    paperId: form.dataset.paperId,
    paperVersion: form.dataset.paperVersion,
    challengeId: form.dataset.challengeId,
    requiredRunIds: [runLabel]
  };
  const logs = await uploadAndRegisterManifest({
    ...common,
    bundleKind: `${pathLabel}-logs`,
    objects: [
      {
        file: stdoutFile,
        mediaType: "text/plain; charset=utf-8",
        role: "run_stdout",
        allowEmpty: true,
        logicalPath: `runs/${pathLabel}/logs/stdout-${safeLogicalFilename(stdoutFile && stdoutFile.name, "stdout.txt")}`
      },
      {
        file: stderrFile,
        mediaType: "text/plain; charset=utf-8",
        role: "run_stderr",
        allowEmpty: true,
        logicalPath: `runs/${pathLabel}/logs/stderr-${safeLogicalFilename(stderrFile && stderrFile.name, "stderr.txt")}`
      }
    ]
  });
  const outputs = status === "succeeded" ? await uploadAndRegisterManifest({
    ...common,
    bundleKind: `${pathLabel}-outputs`,
    objects: [
      {
        file: outputFile,
        mediaType: form.elements.output_media_type.value,
        role: "run_output",
        logicalPath: `runs/${pathLabel}/outputs/result-${safeLogicalFilename(outputFile && outputFile.name, "output.bin")}`
      },
      {
        file: metricsFile,
        mediaType: form.elements.metrics_media_type.value,
        role: "run_metrics",
        logicalPath: `runs/${pathLabel}/outputs/metrics-${safeLogicalFilename(metricsFile && metricsFile.name, "metrics.json")}`
      }
    ]
  }) : null;
  if (outputs && (logs.manifestId === outputs.manifestId || logs.manifestHash === outputs.manifestHash)) {
    throw new Error("run_logs_and_outputs_must_be_distinct_authoritative_manifests");
  }
  const metrics = outputs && outputs.objects.find(object => object.role === "run_metrics");
  if (outputs && !metrics) throw new Error("registered_outputs_manifest_is_missing_metrics");
  const expectedMetricsHash = metrics
    ? canonicalDigest(metrics.digest, "metrics_hash")
    : null;
  const payload = await runRecordPayload({
    experimentPlanId: form.elements.experiment_plan_id.value,
    status,
    seed: form.elements.seed.value,
    parameters: form.elements.parameters.value,
    logsManifestId: logs.manifestId,
    outputsManifestId: outputs ? outputs.manifestId : null,
    metricsHash: expectedMetricsHash,
    failure: form.elements.failure.value
  });
  const response = await sendCommand("create_run_record", form.dataset.paperId, null, payload);
  const result = await responseValue(response);
  if (!response.ok) throw new Error(result && result.error ? result.error : "run_record_creation_failed");
  if (!result || canonicalUuid(result.run_record_id, "registered_run_record_id") !== payload.run_record_id ||
      result.logs_manifest_id !== logs.manifestId ||
      result.outputs_manifest_id !== (outputs ? outputs.manifestId : null) ||
      result.metrics_hash !== payload.metrics_hash ||
      result.failure_hash !== payload.failure_hash || result.status !== status) {
    throw new Error("run_record_receipt_is_not_authoritative_or_exact");
  }
  return {
    runRecordId: payload.run_record_id,
    logsManifestId: logs.manifestId,
    logsManifestHash: logs.manifestHash,
    outputsManifestId: outputs ? outputs.manifestId : null,
    outputsManifestHash: outputs ? outputs.manifestHash : null
  };
}

async function createAuthoritativeFigureLineage(form) {
  const runRecordIds = selectedValues(form.elements.run_record_ids);
  const figureKey = String(form.elements.figure_key.value || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(figureKey)) {
    throw new Error("figure_key_must_be_a_safe_logical_identifier");
  }
  const pathKey = safeLogicalFilename(figureKey, "figure");
  const figureFile = form.elements.figure_file.files[0];
  const lineageFile = form.elements.lineage_file.files[0];
  const manifest = await uploadAndRegisterManifest({
    paperId: form.dataset.paperId,
    paperVersion: form.dataset.paperVersion,
    challengeId: form.dataset.challengeId,
    bundleKind: `${pathKey}-figure`,
    requiredRunIds: runRecordIds,
    objects: [
      {
        file: figureFile,
        mediaType: "image/svg+xml",
        role: "figure_render",
        logicalPath: `figures/${pathKey}/render-${safeLogicalFilename(figureFile && figureFile.name, "figure.svg")}`
      },
      {
        file: lineageFile,
        mediaType: form.elements.lineage_media_type.value,
        role: "figure_transform",
        logicalPath: `figures/${pathKey}/lineage-${safeLogicalFilename(lineageFile && lineageFile.name, "lineage.json")}`
      }
    ]
  });
  const transform = manifest.objects.find(object => object.role === "figure_transform");
  if (!transform) throw new Error("registered_figure_manifest_is_missing_transform_lineage");
  const payload = figureLineagePayload({
    figureKey,
    figureManifestId: manifest.manifestId,
    runRecordIds,
    transformHash: transform.digest
  });
  const response = await sendCommand("create_figure_lineage", form.dataset.paperId, null, payload);
  const result = await responseValue(response);
  if (!response.ok) throw new Error(result && result.error ? result.error : "figure_lineage_creation_failed");
  if (!result || canonicalUuid(result.figure_lineage_id, "registered_figure_lineage_id") !== payload.figure_lineage_id ||
      result.figure_manifest_id !== manifest.manifestId ||
      result.transform_hash !== transform.digest ||
      !Array.isArray(result.run_record_ids) ||
      result.run_record_ids.length !== runRecordIds.length ||
      result.run_record_ids.some((value, index) => value !== runRecordIds[index])) {
    throw new Error("figure_lineage_receipt_is_not_authoritative_or_exact");
  }
  return {
    figureLineageId: payload.figure_lineage_id,
    figureManifestId: manifest.manifestId,
    figureManifestHash: manifest.manifestHash
  };
}

async function recordProductEvent(eventName, values = {}) {
  return mutation("/api/product-events", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify({
      event_id: uuid(),
      event_name: eventName,
      challenge_id: values.challengeId || null,
      team_id: values.teamId || null,
      paper_id: values.paperId || null,
      phase: values.phase || null
    })
  });
}

async function sha256Label(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return `sha256:${Array.from(digest, byte => byte.toString(16).padStart(2, "0")).join("")}`;
}

function bindLogin() {
  const form = document.querySelector("#login-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const input = form.elements.login_key;
    const loginKey = input.value;
    input.value = "";
    form.querySelector("button").disabled = true;
    try {
      const response = await fetch("/alpha/login", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({ login_key: loginKey })
      });
      const value = await responseValue(response);
      if (!response.ok) throw new Error(value && value.error ? value.error : "login_failed");
      csrfToken = value.csrf;
      window.location.assign("/league/start");
    } catch (error) {
      show(output, error.message, false);
      form.querySelector("button").disabled = false;
    }
  });
}

function bindHumanKeyCreate() {
  const form = document.querySelector("#human-key-create-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const button = form.querySelector("button");
    const passphraseInput = form.elements.passphrase;
    const confirmInput = form.elements.confirm_passphrase;
    const passphrase = passphraseInput.value;
    button.disabled = true;
    try {
      if (humanSigner) {
        throw new Error("forget_current_in_memory_key_before_generating_another");
      }
      if (passphrase.length < 16 || passphrase !== confirmInput.value) {
        throw new Error("passphrases_must_match_and_contain_at_least_16_characters");
      }
      const created = await createEncryptedBundle(passphrase);
      humanSigner = created.signer;
      downloadBundle(created.bundle);
      updateHumanKeyStatus(`In-memory key loaded: ${humanSigner.publicKeyBase64.slice(0, 16)}…`);
      show(output, "Encrypted bundle downloaded. Keep it safe, then register this same in-memory key in Step 2.", true);
    } catch (error) {
      show(output, error.message, false);
    } finally {
      passphraseInput.value = "";
      confirmInput.value = "";
      button.disabled = false;
    }
  });
}

function bindHumanKeyRegistration() {
  const form = document.querySelector("#human-key-register-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const button = form.querySelector("button");
    let registrationSubmitted = false;
    button.disabled = true;
    try {
      if (!humanSigner) {
        throw new Error("generate_a_key_or_import_the_original_encrypted_bundle_first");
      }
      const challengeResponse = await mutation("/api/onboarding/human/challenge", {
        method: "POST",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({ signing_public_key: humanSigner.publicKeyBase64 })
      });
      const challenge = await responseValue(challengeResponse);
      if (!challengeResponse.ok) throw new Error(challenge && challenge.error ? challenge.error : "human_challenge_failed");
      if (!challenge || challenge.signing_public_key !== humanSigner.publicKeyBase64) {
        throw new Error("human_challenge_key_mismatch");
      }
      const signature = await crypto.subtle.sign(
        "Ed25519",
        humanSigner.privateKey,
        base64ToBytes(challenge.signing_bytes)
      );
      registrationSubmitted = true;
      const registerResponse = await mutation("/api/onboarding/human/register", {
        method: "POST",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({
          signing_public_key: humanSigner.publicKeyBase64,
          idempotency_key: challenge.idempotency_key,
          issued_at_unix: challenge.issued_at_unix,
          expires_at_unix: challenge.expires_at_unix,
          key_proof_signature: bytesToBase64(signature)
        })
      });
      const value = await responseValue(registerResponse);
      if (!registerResponse.ok) throw new Error(value && value.error ? value.error : "human_registration_failed");
      const recovered = registerResponse.headers.get("x-paper-raid-registration-recovered") === "self-read";
      show(output, recovered ? "Previously committed registration recovered by self-read. Redirecting…" : "Human key registered. Redirecting…", true);
      window.setTimeout(() => window.location.assign("/league/start"), 700);
    } catch (error) {
      const recovery = registrationSubmitted
        ? " Registration may already be committed: refresh /league/start first. If it is still unregistered, import the original bundle and retry with the same key; do not generate a replacement key."
        : "";
      show(output, `${error.message}.${recovery}`, false);
    } finally {
      button.disabled = false;
    }
  });
}

const AGENT_PAIRING_RETURN_PATHS = new Set(["/league/practice", "/league/quick-raid"]);

function agentPairingReturnTarget(locationValue = window.location) {
  let page;
  try {
    page = new URL(locationValue.href);
  } catch {
    return null;
  }
  const candidates = page.searchParams.getAll("return_to");
  if (candidates.length !== 1 || !AGENT_PAIRING_RETURN_PATHS.has(candidates[0])) {
    return null;
  }
  return candidates[0];
}

function agentPairingHealthReady(grant) {
  return Boolean(grant) && grant.state === "consumed" &&
    grant.signed_health_observed_after_pairing === true;
}

function bindAgentPairing() {
  for (const panel of document.querySelectorAll(".agent-pairing-panel")) {
    const form = panel.querySelector(".agent-pairing-grant-form");
    const formOutput = form && form.querySelector("output");
    const codeBox = panel.querySelector(".agent-pairing-code");
    const codeValue = panel.querySelector(".agent-pairing-code-value");
    const copyButton = panel.querySelector(".agent-pairing-copy");
    const refreshButton = panel.querySelector(".agent-pairing-refresh");
    const revokeButton = panel.querySelector(".agent-pairing-revoke");
    const statusOutput = panel.querySelector(".agent-pairing-status output");
    const returnTarget = agentPairingReturnTarget();
    let visibleGrantId = null;
    let clearTimer = null;
    let pollTimer = null;
    let activePairingGrantId = null;
    let observedState = null;
    let completionHandled = false;
    let initialStatusPending = true;
    let statusRequestGeneration = 0;

    const clearPollTimer = () => {
      if (pollTimer !== null) window.clearTimeout(pollTimer);
      pollTimer = null;
    };

    const invalidateStatusRequests = () => {
      statusRequestGeneration += 1;
    };

    const clearVisibleCode = () => {
      if (clearTimer !== null) window.clearTimeout(clearTimer);
      clearTimer = null;
      visibleGrantId = null;
      if (codeValue) codeValue.textContent = "";
      if (codeBox) codeBox.hidden = true;
    };

    const renderStatus = value => {
      if (!value || value.schema !== "hepta.paper_raid.agent_bridge.pairing_status.v1" ||
          !Object.hasOwn(value, "grant")) {
        throw new Error("agent_pairing_status_is_invalid");
      }
      const grant = value.grant;
      if (grant === null) {
        revokeButton.hidden = true;
        revokeButton.dataset.grantId = "";
        show(statusOutput, "No pairing grant has been issued yet / 尚未签发配对码", true);
        return null;
      }
      const grantId = canonicalUuid(grant.grant_id, "grant_id");
      const createdAt = Date.parse(grant.created_at);
      const expiresAt = Date.parse(grant.expires_at);
      if (!["issued", "pinned", "consumed", "revoked"].includes(grant.state) ||
          Number.isNaN(createdAt) || Number.isNaN(expiresAt) || expiresAt <= createdAt ||
          typeof grant.signed_health_observed_after_pairing !== "boolean") {
        throw new Error("agent_pairing_grant_is_invalid");
      }
      const expired = expiresAt <= Date.now();
      const healthConfirmed = agentPairingHealthReady(grant);
      revokeButton.dataset.grantId = grantId;
      revokeButton.hidden = !(grant.state === "issued" || (grant.state === "pinned" && expired));
      show(statusOutput, {
        state: expired && ["issued", "pinned"].includes(grant.state) ? "expired" : grant.state,
        signed_health: grant.state === "consumed"
          ? healthConfirmed ? "observed_after_pairing" : "pending"
          : null,
        expires_at: grant.expires_at,
        binding_id: grant.binding_id || null,
        agent_id: grant.agent_id || null,
        agent_key_id: grant.agent_key_id || null,
        assurance: grant.binding_id ? "self_declared_unverified" : null
      }, true);
      return {
        grantId,
        lifecycleState: grant.state === "consumed"
          ? healthConfirmed ? "consumed_healthy" : "consumed_pending_health"
          : grant.state,
        healthConfirmed,
        shouldPoll: (!expired && ["issued", "pinned"].includes(grant.state)) ||
          (grant.state === "consumed" && !healthConfirmed && Date.now() < expiresAt + 60000),
        pollDeadline: expiresAt + 60000
      };
    };

    const scheduleStatusRefresh = deadline => {
      clearPollTimer();
      if (!Number.isFinite(deadline) || deadline <= Date.now()) return;
      pollTimer = window.setTimeout(async () => {
        pollTimer = null;
        try {
          await refreshStatus();
        } catch (error) {
          show(statusOutput, error.message, false);
          scheduleStatusRefresh(deadline);
        }
      }, Math.min(1000, Math.max(1, deadline - Date.now())));
    };

    const refreshStatus = async () => {
      const requestGeneration = ++statusRequestGeneration;
      const response = await fetch("/api/agent-bridge/pairing-grants", {
        method: "GET",
        credentials: "same-origin",
        headers: { "accept": "application/json" }
      });
      const value = await responseValue(response);
      if (requestGeneration !== statusRequestGeneration) return null;
      if (!response.ok) throw new Error(value && value.error ? value.error : "agent_pairing_status_failed");
      const previousState = observedState;
      const status = renderStatus(value);
      if (initialStatusPending) {
        initialStatusPending = false;
        if (activePairingGrantId === null && status?.shouldPoll && !status.healthConfirmed) {
          activePairingGrantId = status.grantId;
        }
      }
      const matchesActiveGrant = activePairingGrantId !== null &&
        status?.grantId === activePairingGrantId;
      if (matchesActiveGrant) {
        observedState = status.lifecycleState;
      } else if (activePairingGrantId !== null) {
        activePairingGrantId = null;
        observedState = null;
      }
      if (status?.healthConfirmed && matchesActiveGrant && previousState !== null &&
          previousState !== "consumed_healthy") {
        clearPollTimer();
        clearVisibleCode();
        if (!completionHandled) {
          completionHandled = true;
          show(
            statusOutput,
            returnTarget === null
              ? "Agent paired; its signed self-declared health report was observed. Refreshing… / Agent 已配对；已观察到其签名的自声明健康报告，正在刷新……"
              : "Agent paired; its signed self-declared health report was observed. Returning to practice… / Agent 已配对；已观察到其签名的自声明健康报告，正在返回练习……",
            true,
          );
          window.setTimeout(() => {
            if (returnTarget === null) window.location.reload();
            else window.location.assign(returnTarget);
          }, 300);
        }
      } else if (status?.shouldPoll && matchesActiveGrant) {
        scheduleStatusRefresh(status.pollDeadline);
      } else {
        clearPollTimer();
      }
      return status;
    };

    if (form) form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      button.disabled = true;
      clearPollTimer();
      invalidateStatusRequests();
      clearVisibleCode();
      activePairingGrantId = null;
      observedState = null;
      completionHandled = false;
      initialStatusPending = true;
      try {
        const response = await mutation("/api/agent-bridge/pairing-grants", {
          method: "POST",
          headers: { "content-type": "application/json", "accept": "application/json" },
          body: "{}"
        });
        const value = await responseValue(response);
        if (!response.ok) throw new Error(value && value.error ? value.error : "agent_pairing_grant_failed");
        if (!value || value.schema !== "hepta.paper_raid.agent_bridge.pairing_grant.v1" ||
            value.display_once !== true || typeof value.pairing_code !== "string" ||
            !value.pairing_code.startsWith("prg1.") ||
            !Number.isSafeInteger(value.expires_at_unix)) {
          throw new Error("agent_pairing_grant_response_is_invalid");
        }
        visibleGrantId = canonicalUuid(value.grant_id, "grant_id");
        invalidateStatusRequests();
        activePairingGrantId = visibleGrantId;
        observedState = "issued";
        completionHandled = false;
        initialStatusPending = false;
        codeValue.textContent = value.pairing_code;
        codeBox.hidden = false;
        const remainingMs = Math.max(0, value.expires_at_unix * 1000 - Date.now());
        clearTimer = window.setTimeout(clearVisibleCode, Math.min(remainingMs, 300000));
        show(formOutput, "Pairing code created. It is shown only in the box below / 配对码已生成，仅在下方显示一次", true);
        await refreshStatus();
      } catch (error) {
        show(formOutput, error.message, false);
      } finally {
        button.disabled = false;
      }
    });

    if (copyButton) copyButton.addEventListener("click", async () => {
      try {
        if (!visibleGrantId || !codeValue.textContent) throw new Error("no_visible_pairing_code");
        await navigator.clipboard.writeText(codeValue.textContent);
        show(formOutput, "Pairing code copied. Paste it only into the local Bridge prompt / 配对码已复制，仅粘贴到本地 Bridge 提示符", true);
      } catch (error) {
        show(formOutput, error.message, false);
      }
    });

    if (refreshButton) refreshButton.addEventListener("click", async () => {
      refreshButton.disabled = true;
      try { await refreshStatus(); } catch (error) { show(statusOutput, error.message, false); }
      finally { refreshButton.disabled = false; }
    });

    if (revokeButton) revokeButton.addEventListener("click", async () => {
      const grantId = revokeButton.dataset.grantId;
      revokeButton.disabled = true;
      try {
        canonicalUuid(grantId, "grant_id");
        clearPollTimer();
        invalidateStatusRequests();
        const response = await mutation(`/api/agent-bridge/pairing-grants/${grantId}/revoke`, {
          method: "POST",
          headers: { "content-type": "application/json", "accept": "application/json" },
          body: "{}"
        });
        const value = await responseValue(response);
        if (!response.ok) throw new Error(value && value.error ? value.error : "agent_pairing_revoke_failed");
        clearPollTimer();
        clearVisibleCode();
        activePairingGrantId = null;
        observedState = null;
        completionHandled = false;
        initialStatusPending = true;
        await refreshStatus();
      } catch (error) {
        show(statusOutput, error.message, false);
      } finally {
        revokeButton.disabled = false;
      }
    });

    refreshStatus().catch(error => show(statusOutput, error.message, false));
  }
}

function bindAgentBinding() {
  for (const form of document.querySelectorAll(".agent-binding-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const expected = [
            "binding_id", "player_id", "agent_id", "agent_key_id", "agent_public_key",
          "agent_proof_schema", "capability_disclosure", "capability_disclosure_hash",
          "agent_proof_nonce", "agent_proof_issued_at_unix", "agent_proof_expires_at_unix",
          "agent_proof_signature", "idempotency_key"
        ];
        if (!exactKeys(payload, expected)) throw new Error("agent_binding_payload_must_have_exact_fields");
        if (payload.player_id !== form.dataset.playerId) throw new Error("agent_binding_player_id_mismatch");
        if (payload.agent_proof_schema !== "hepta.paper_raid.agent_binding_proof.v3") {
          throw new Error("agent_binding_requires_v3_proof");
        }
        const disclosure = payload.capability_disclosure;
        if (!exactKeys(disclosure, ["schema", "assurance", "capabilities", "resource_classes", "max_parallel_tasks"]) ||
            disclosure.schema !== "hepta.paper_raid.agent_capability_disclosure.v1" ||
            disclosure.assurance !== "self_declared_unverified" ||
            !Array.isArray(disclosure.capabilities) || disclosure.capabilities.length < 1 || disclosure.capabilities.length > 16 ||
            !Array.isArray(disclosure.resource_classes) || disclosure.resource_classes.length > 16 ||
            !Number.isSafeInteger(disclosure.max_parallel_tasks) || disclosure.max_parallel_tasks < 1 || disclosure.max_parallel_tasks > 32 ||
            typeof payload.capability_disclosure_hash !== "string" || !DIGEST_PATTERN.test(payload.capability_disclosure_hash)) {
          throw new Error("agent_capability_disclosure_is_invalid");
        }
        if (payload.agent_proof_nonce !== payload.idempotency_key) throw new Error("agent_proof_nonce_must_equal_idempotency_key");
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
        if (!uuidPattern.test(payload.binding_id) || !uuidPattern.test(payload.idempotency_key)) {
          throw new Error("binding_and_idempotency_ids_must_be_canonical_uuids");
        }
        if (typeof payload.agent_key_id !== "string" || !/^sha256:[0-9a-f]{64}$/.test(payload.agent_key_id)) {
          throw new Error("agent_key_id_must_be_sha256_of_public_key_bytes");
        }
        const agentPublicKey = base64ToBytes(payload.agent_public_key);
        const agentProofSignature = base64ToBytes(payload.agent_proof_signature);
        if (agentPublicKey.length !== 32 || agentProofSignature.length !== 64 ||
            await sha256Label(agentPublicKey) !== payload.agent_key_id) {
          throw new Error("agent_binding_public_proof_is_not_canonical_ed25519");
        }
        if (!Number.isSafeInteger(payload.agent_proof_issued_at_unix) ||
            !Number.isSafeInteger(payload.agent_proof_expires_at_unix) ||
            payload.agent_proof_issued_at_unix < 0 ||
            payload.agent_proof_expires_at_unix <= payload.agent_proof_issued_at_unix ||
            payload.agent_proof_expires_at_unix - payload.agent_proof_issued_at_unix > 300) {
          throw new Error("agent_binding_proof_interval_must_be_at_most_five_minutes");
        }
        const response = await sendCommand("create_agent_binding", null, null, payload);
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.assign("/league/start"), 800);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindAgentRotation() {
  for (const form of document.querySelectorAll(".agent-rotation-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const expected = [
          "rotation_id", "expected_binding_version", "agent_id", "old_agent_key_id",
          "old_agent_public_key", "new_agent_key_id", "new_agent_public_key",
          "issued_at_unix", "expires_at_unix", "old_key_signature", "new_key_signature",
          "idempotency_key"
        ];
        if (!exactKeys(payload, expected)) throw new Error("agent_rotation_payload_must_have_exact_fields");
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
        if (!uuidPattern.test(payload.rotation_id) || !uuidPattern.test(payload.idempotency_key)) {
          throw new Error("rotation_and_idempotency_ids_must_be_canonical_uuids");
        }
        if (!Number.isSafeInteger(payload.expected_binding_version) ||
            payload.expected_binding_version < 1 ||
            payload.expected_binding_version !== Number(form.dataset.bindingVersion)) {
          throw new Error("agent_rotation_expected_version_mismatch");
        }
        if (payload.agent_id !== form.dataset.agentId ||
            payload.old_agent_key_id !== form.dataset.oldKeyId ||
            payload.old_agent_public_key !== form.dataset.oldPublicKey) {
          throw new Error("agent_rotation_old_binding_mismatch");
        }
        if (payload.new_agent_public_key === payload.old_agent_public_key) {
          throw new Error("agent_rotation_must_change_public_key");
        }
        const oldPublicKey = base64ToBytes(payload.old_agent_public_key);
        const newPublicKey = base64ToBytes(payload.new_agent_public_key);
        const oldSignature = base64ToBytes(payload.old_key_signature);
        const newSignature = base64ToBytes(payload.new_key_signature);
        if (oldPublicKey.length !== 32 || newPublicKey.length !== 32 ||
            oldSignature.length !== 64 || newSignature.length !== 64 ||
            await sha256Label(oldPublicKey) !== payload.old_agent_key_id ||
            await sha256Label(newPublicKey) !== payload.new_agent_key_id) {
          throw new Error("agent_rotation_key_id_hash_mismatch");
        }
        if (!Number.isSafeInteger(payload.issued_at_unix) ||
            !Number.isSafeInteger(payload.expires_at_unix) || payload.issued_at_unix < 0 ||
            payload.expires_at_unix <= payload.issued_at_unix ||
            payload.expires_at_unix - payload.issued_at_unix > 600) {
          throw new Error("agent_rotation_interval_must_be_at_most_ten_minutes");
        }
        const response = await sendCommand(
          "rotate_agent_binding_key",
          form.dataset.bindingId,
          null,
          payload
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindHumanKeyImport() {
  for (const form of document.querySelectorAll(".human-key-import-form")) {
    const button = form.querySelector('button[type="submit"]');
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const fileInput = form.elements.key_bundle;
      const passphraseInput = form.elements.passphrase;
      const passphrase = passphraseInput.value;
      button.disabled = true;
      try {
        const file = fileInput.files[0];
        if (!file || file.size < 1 || file.size > 65536 || passphrase.length < 16) {
          throw new Error("key_bundle_or_passphrase_is_invalid");
        }
        const bundle = JSON.parse(await file.text());
        humanSigner = await decryptBundle(bundle, passphrase);
        const message = `In-memory key loaded: ${humanSigner.publicKeyBase64.slice(0, 16)}…`;
        updateHumanKeyStatus(message);
        show(output, message, true);
      } catch (error) {
        humanSigner = null;
        updateHumanKeyStatus("No in-memory key / 当前无内存密钥");
        show(output, error.message, false);
      } finally {
        passphraseInput.value = "";
        fileInput.value = "";
        button.disabled = false;
      }
    });
    // The server renders this sensitive local-only form disabled.  Enable it
    // only after preventDefault is synchronously installed, so a missing or
    // delayed script can never fall back to a native GET containing its
    // passphrase.
    button.disabled = false;
  }
  for (const button of document.querySelectorAll(".forget-human-key")) {
    button.addEventListener("click", () => {
      humanSigner = null;
      updateHumanKeyStatus("No in-memory key / 当前无内存密钥");
      show(button.parentElement.querySelector(".human-key-status"), "In-memory key forgotten", true);
    });
  }
}

function unsignedHumanPayload(command, payload) {
  const clean = { ...payload };
  for (const field of [
    "idempotency_key", "signature", "signing_key_id", "signing_public_key",
    "signing_public_key_hash", "signed_at_unix", "accepted_at_unix", "player_id",
    "appellant_player_id", "verification_signature", "verification_key_id",
    "verification_public_key", "verification_public_key_hash"
  ]) delete clean[field];
  delete clean.resolver_player_id;
  if (command === "accept_research_team_membership") delete clean.agent_id;
  if (command === "create_citation_record") {
    delete clean.source_hash;
    delete clean.locator;
    delete clean.license;
  }
  return clean;
}

function humanSignatureField(command) {
  if (command === "create_paper_evaluation_draft") return "evaluator_signature";
  return command === "create_evidence_card" || command === "create_citation_record"
    ? "verification_signature"
    : "signature";
}

async function plainTextDigest(value, field) {
  const clean = String(value || "").trim();
  if (!clean) throw new Error(`${field}_is_required`);
  return sha256Label(new TextEncoder().encode(clean));
}

function boundedInteger(value, field, maximum) {
  const parsed = nonNegativeInteger(value, field);
  if (parsed > maximum) throw new Error(`${field}_exceeds_${maximum}`);
  return parsed;
}

function requiredBoolean(value, field) {
  if (value === "true") return true;
  if (value === "false") return false;
  throw new Error(`${field}_must_be_explicitly_confirmed`);
}

async function evaluationAttestationPayload(form, verdict) {
  if (!new Set(["approve", "reject"]).has(verdict)) {
    throw new Error("invalid_evaluation_attestation_verdict");
  }
  return {
    attestation_id: uuid(),
    verdict,
    coi_attestation_hash: await plainTextDigest(
      form.elements.coi_statement.value,
      "reviewer_coi_statement"
    )
  };
}

async function appealPayload(form) {
  return {
    appeal_id: uuid(),
    grounds_hash: await plainTextDigest(form.elements.grounds.value, "appeal_grounds"),
    evidence_manifest_id: canonicalUuid(
      form.elements.evidence_manifest_id.value,
      "evidence_manifest_id"
    )
  };
}

async function paperReworkPayload(form) {
  return {
    rework_id: uuid(),
    reason_hash: await plainTextDigest(form.elements.reason.value, "paper_rework_reason")
  };
}

async function appealResolutionPayload(form, outcome) {
  if (!new Set(["denied", "upheld"]).has(outcome)) {
    throw new Error("invalid_appeal_resolution_outcome");
  }
  return {
    resolution_id: uuid(),
    outcome,
    decision_hash: await plainTextDigest(
      form.elements.decision.value,
      "appeal_resolution_decision"
    )
  };
}

async function signServerFrame(command, frame, signer = humanSigner) {
  if (!signer) throw new Error("import_your_encrypted_human_key_bundle_first");
  if (!frame || frame.signing_public_key !== signer.publicKeyBase64) {
    throw new Error("imported_key_is_not_the_active_registered_human_key");
  }
  const signingBytes = base64ToBytes(frame.signing_bytes);
  const signature = await crypto.subtle.sign("Ed25519", signer.privateKey, signingBytes);
  return {
    ...frame,
    payload: {
      ...frame.payload,
      [humanSignatureField(command)]: bytesToBase64(signature)
    }
  };
}

async function signReviewConfirmationFrame(command, frame, signer = humanSigner) {
  const signedFrame = await signServerFrame(command, frame, signer);
  const contextSigningBytes = base64ToBytes(frame.receipt_context_signing_bytes);
  const contextSignature = await crypto.subtle.sign(
    "Ed25519",
    signer.privateKey,
    contextSigningBytes,
  );
  return {
    signedFrame,
    receiptContextSignature: bytesToBase64(contextSignature),
  };
}

async function signHumanPayload(command, resourceId, childId, payload) {
  if (!humanSigner) throw new Error("import_your_encrypted_human_key_bundle_first");
  const frameResponse = await mutation("/api/onboarding/human/signing-frame", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify({
      command,
      resource_id: resourceId || null,
      child_id: childId || null,
      payload: unsignedHumanPayload(command, payload)
    })
  });
  const frame = await responseValue(frameResponse);
  if (!frameResponse.ok) throw new Error(frame && frame.error ? frame.error : "signing_frame_failed");
  return signServerFrame(command, frame, humanSigner);
}

function validateReviewConfirmationFrame(frame, command, paperId, receiptId, request) {
  const frameKeys = [
    "schema", "receipt_context", "receipt_context_signing_bytes",
    "command", "resource_id", "child_id", "payload",
    "signing_bytes", "signing_key_id", "signing_public_key", "signing_public_key_hash",
  ];
  const contextKeys = [
    "schema", "receipt_id", "receipt_hash", "task_id", "assignment_id",
    "assignment_version", "paper_project_id", "submission_id", "evaluation_id", "kind",
    "bundle_hash", "release_candidate_hash", "paper_bundle_hash", "artifact_manifest_hash",
    "evaluator_version", "agent_id", "agent_key_id", "input_root", "output_root",
    "metrics_hash", "candidate_passed", "seed_set_hash", "environment_hash",
    "run_manifest_hash", "logs_hash", "completed_at_unix",
  ];
  if (
    !exactKeys(frame, frameKeys) ||
    frame.schema !== "hepta.paper_raid.review_receipt_confirmation_frame.v1" ||
    !exactKeys(frame.receipt_context, contextKeys) ||
    frame.receipt_context.schema !== "hepta.paper_raid.review_receipt_confirmation_context.v1" ||
    frame.receipt_context.receipt_id !== receiptId ||
    frame.receipt_context.paper_project_id !== paperId ||
    frame.receipt_context.kind !== (command === "create_paper_evaluation_draft" ? "evaluate" : "reproduce") ||
    frame.command !== command ||
    frame.resource_id !== paperId ||
    (command === "create_paper_evaluation_draft"
      ? frame.child_id !== null || typeof frame.receipt_context.candidate_passed !== "boolean"
      : frame.child_id !== frame.receipt_context.evaluation_id || frame.receipt_context.candidate_passed !== null)
  ) {
    throw new Error("review_receipt_confirmation_frame_mismatch");
  }
  let contextSigningClaim;
  let contextSigningText;
  try {
    contextSigningText = new TextDecoder("utf-8", { fatal: true }).decode(
      base64ToBytes(frame.receipt_context_signing_bytes),
    );
    contextSigningClaim = JSON.parse(contextSigningText);
  } catch (_) {
    throw new Error("review_receipt_confirmation_context_signing_bytes_invalid");
  }
  const expectedContextSigningClaim = {
    schema: "hepta.paper_raid.review_receipt_confirmation_context_signing.v1",
    receipt_context: frame.receipt_context,
    command: frame.command,
    resource_id: frame.resource_id,
    child_id: frame.child_id,
    upstream_signing_bytes: frame.signing_bytes,
    signing_key_id: frame.signing_key_id,
    signing_public_key_hash: frame.signing_public_key_hash,
  };
  if (
    canonicalJson(contextSigningClaim) !== contextSigningText ||
    canonicalJson(contextSigningClaim) !== canonicalJson(expectedContextSigningClaim)
  ) {
    throw new Error("review_receipt_confirmation_context_mismatch");
  }
  for (const field of [
    "receipt_hash", "bundle_hash", "release_candidate_hash", "paper_bundle_hash",
    "artifact_manifest_hash", "input_root", "output_root", "metrics_hash", "seed_set_hash",
    "environment_hash", "run_manifest_hash", "logs_hash",
  ]) {
    if (!DIGEST_PATTERN.test(String(frame.receipt_context[field] || ""))) {
      throw new Error(`review_receipt_${field}_is_invalid`);
    }
  }
  const coiField = command === "create_paper_evaluation_draft"
    ? "evaluator_coi_attestation_hash"
    : "coi_attestation_hash";
  if (frame.payload[coiField] !== request.coi_attestation_hash) {
    throw new Error("review_receipt_coi_attestation_mismatch");
  }
  if (command === "create_paper_evaluation_draft") {
    const observable = request.observable_hard_gates;
    const observedGates = {
      citations_and_data_authentic: frame.payload.hard_gates.citations_and_data_authentic,
      failed_runs_disclosed: frame.payload.hard_gates.failed_runs_disclosed,
      core_claims_have_evidence: frame.payload.hard_gates.core_claims_have_evidence,
      license_ethics_coi_complete: frame.payload.hard_gates.license_ethics_coi_complete,
    };
    if (
      canonicalJson(frame.payload.score_components) !== canonicalJson(request.score_components) ||
      canonicalJson(observedGates) !== canonicalJson(observable) ||
      typeof frame.payload.hard_gates.all_authors_consented !== "boolean" ||
      typeof frame.payload.hard_gates.artifact_lineage_complete !== "boolean"
    ) {
      throw new Error("review_receipt_human_judgment_frame_mismatch");
    }
  }
}

async function confirmReviewReceipt(form) {
  if (!humanSigner) throw new Error("import_your_encrypted_human_key_bundle_first");
  const paperId = canonicalUuid(form.dataset.paperId, "paper_id");
  const receiptId = canonicalUuid(form.dataset.receiptId, "receipt_id");
  const command = form.dataset.kind === "evaluate"
    ? "create_paper_evaluation_draft"
    : form.dataset.kind === "reproduce"
      ? "submit_reproduction"
      : null;
  if (!command) throw new Error("unsupported_review_receipt_kind");
  const coiAttestationHash = await plainTextDigest(
    form.elements.coi_statement.value,
    "review_coi_statement"
  );
  const frameRequest = { coi_attestation_hash: coiAttestationHash };
  if (command === "create_paper_evaluation_draft") {
    frameRequest.score_components = {
      method_rigor_bps: boundedInteger(
        form.elements.method_rigor_bps.value,
        "method_rigor_bps",
        2500,
      ),
      experiment_statistics_bps: boundedInteger(
        form.elements.experiment_statistics_bps.value,
        "experiment_statistics_bps",
        1500,
      ),
      reproducibility_bps: boundedInteger(
        form.elements.reproducibility_bps.value,
        "reproducibility_bps",
        1500,
      ),
      evidence_citations_bps: boundedInteger(
        form.elements.evidence_citations_bps.value,
        "evidence_citations_bps",
        1500,
      ),
      value_originality_bps: boundedInteger(
        form.elements.value_originality_bps.value,
        "value_originality_bps",
        1500,
      ),
      argument_expression_bps: boundedInteger(
        form.elements.argument_expression_bps.value,
        "argument_expression_bps",
        1000,
      ),
      ethics_transparency_bps: boundedInteger(
        form.elements.ethics_transparency_bps.value,
        "ethics_transparency_bps",
        500,
      ),
    };
    frameRequest.observable_hard_gates = {
      citations_and_data_authentic: requiredBoolean(
        form.elements.gate_citations_and_data_authentic.value,
        "citations_and_data_authentic",
      ),
      failed_runs_disclosed: requiredBoolean(
        form.elements.gate_failed_runs_disclosed.value,
        "failed_runs_disclosed",
      ),
      core_claims_have_evidence: requiredBoolean(
        form.elements.gate_core_claims_have_evidence.value,
        "core_claims_have_evidence",
      ),
      license_ethics_coi_complete: requiredBoolean(
        form.elements.gate_license_ethics_coi_complete.value,
        "license_ethics_coi_complete",
      ),
    };
  }
  const frameResponse = await mutation(
    `/api/review/papers/${encodeURIComponent(paperId)}/receipts/${encodeURIComponent(receiptId)}/signing-frame`,
    {
      method: "POST",
      headers: { "content-type": "application/json", "accept": "application/json" },
      body: JSON.stringify(frameRequest)
    }
  );
  const frame = await responseValue(frameResponse);
  if (!frameResponse.ok) {
    throw new Error(frame && frame.error ? frame.error : "review_receipt_signing_frame_failed");
  }
  validateReviewConfirmationFrame(frame, command, paperId, receiptId, frameRequest);
  const signed = await signReviewConfirmationFrame(command, frame, humanSigner);
  const signature = signed.signedFrame.payload[humanSignatureField(command)];
  if (typeof signature !== "string" || !signature) {
    throw new Error("review_receipt_human_signature_missing");
  }
  if (typeof signed.receiptContextSignature !== "string" || !signed.receiptContextSignature) {
    throw new Error("review_receipt_context_signature_missing");
  }
  return mutation(
    `/api/review/papers/${encodeURIComponent(paperId)}/receipts/${encodeURIComponent(receiptId)}/confirm`,
    {
      method: "POST",
      headers: { "content-type": "application/json", "accept": "application/json" },
      body: JSON.stringify({
        signature,
        receipt_context_signature: signed.receiptContextSignature,
      })
    }
  );
}

function bindLocalSigning() {
  for (const button of document.querySelectorAll(".local-sign")) {
    const form = button.closest("form");
    button.addEventListener("click", async () => {
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const child = form.elements.child_id ? form.elements.child_id.value.trim() : null;
        const frame = await signHumanPayload(
          form.dataset.command,
          form.dataset.resourceId || null,
          child || null,
          payload
        );
        form.elements.payload.value = JSON.stringify(frame.payload, null, 2);
        show(output, `Locally signed with ${frame.signing_key_id}`, true);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindFirstPlayableFormation() {
  for (const form of document.querySelectorAll(".materialize-team-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const response = await sendCommand("materialize_team_proposal", form.dataset.proposalId, null, {
          expected_proposal_version: Number(form.dataset.proposalVersion)
        });
        const value = await responseValue(response);
        show(output, response.ok ? "Team built. Loading ready check…" : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".team-ready-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const payload = {
          acceptance_id: uuid(),
          expected_team_version: Number(form.dataset.teamVersion),
          roster_version: Number(form.dataset.rosterVersion),
          participant_slot: Number(form.dataset.participantSlot),
          binding_id: form.dataset.bindingId,
          role: form.dataset.role,
          collaboration_compact_hash: form.dataset.compactHash
        };
        const frame = await signHumanPayload(
          "accept_research_team_membership",
          form.dataset.teamId,
          null,
          payload
        );
        const response = await sendCommand(
          "accept_research_team_membership",
          form.dataset.teamId,
          null,
          frame.payload
        );
        const value = await responseValue(response);
        show(output, response.ok ? "Ready confirmed." : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".lock-team-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const response = await sendCommand("lock_research_team", form.dataset.teamId, null, {
          expected_version: Number(form.dataset.teamVersion)
        });
        const value = await responseValue(response);
        show(output, response.ok ? "Roster locked. Name the Raid next." : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".create-paper-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const paperId = uuid();
        const response = await sendCommand("create_paper_project", null, null, {
          paper_project_id: paperId,
          team_id: form.dataset.teamId,
          title: form.elements.title.value.trim(),
          target_format: form.elements.target_format.value
        });
        const value = await responseValue(response);
        const createdId = value && typeof value.paper_project_id === "string"
          ? value.paper_project_id
          : paperId;
        show(output, response.ok ? "Paper created. Entering the Research Room…" : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.assign(`/league/papers/${encodeURIComponent(createdId)}`), 300);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

async function submitGuidedCommand(form, command, resourceId, childId, payload, sessionId, message) {
  const output = form.querySelector("output");
  const buttons = form.querySelectorAll("button");
  for (const button of buttons) button.disabled = true;
  try {
    const response = await sendCommand(command, resourceId, childId, payload, sessionId);
    const value = await responseValue(response);
    show(output, response.ok ? message : value, response.ok);
    if (response.ok) {
      const destination = form.dataset.successHref;
      window.setTimeout(
        () => destination ? window.location.assign(destination) : window.location.reload(),
        450
      );
    }
  } catch (error) {
    show(output, error.message, false);
  } finally {
    restoreAuthoritativeFormControls(form, buttons);
  }
}

async function submitSignedGuidedCommand(form, command, resourceId, childId, payload, message) {
  const output = form.querySelector("output");
  const buttons = form.querySelectorAll("button");
  for (const button of buttons) button.disabled = true;
  try {
    const frame = await signHumanPayload(command, resourceId, childId, payload);
    const response = await sendCommand(command, resourceId, childId, frame.payload);
    const value = await responseValue(response);
    show(output, response.ok ? message : value, response.ok);
    if (response.ok) window.setTimeout(() => window.location.reload(), 450);
  } catch (error) {
    show(output, error.message, false);
  } finally {
    restoreAuthoritativeFormControls(form, buttons);
  }
}

async function submitDerivedSignedGuidedCommand(form, command, payload, message) {
  const output = form.querySelector("output");
  const buttons = form.querySelectorAll("button");
  for (const button of buttons) button.disabled = true;
  try {
    const expectedPaperId = canonicalUuid(form.dataset.paperId, "paper_id");
    const frame = await signHumanPayload(command, expectedPaperId, null, payload);
    const resourceId = canonicalUuid(frame.resource_id, `${command}_resource_id`);
    const childId = canonicalUuid(frame.child_id, `${command}_child_id`);
    if (resourceId !== expectedPaperId || frame.command !== command) {
      throw new Error(`${command}_authoritative_route_mismatch`);
    }
    const response = await sendCommand(command, resourceId, childId, frame.payload);
    const value = await responseValue(response);
    show(output, response.ok ? message : value, response.ok);
    if (response.ok) window.setTimeout(() => window.location.reload(), 450);
  } catch (error) {
    show(output, error.message, false);
  } finally {
    restoreAuthoritativeFormControls(form, buttons);
  }
}

async function submitPaperDerivedSignedGuidedCommand(form, command, payload, message) {
  const output = form.querySelector("output");
  const buttons = form.querySelectorAll("button");
  for (const button of buttons) button.disabled = true;
  try {
    const expectedPaperId = canonicalUuid(form.dataset.paperId, "paper_id");
    const frame = await signHumanPayload(command, expectedPaperId, null, payload);
    const resourceId = canonicalUuid(frame.resource_id, `${command}_resource_id`);
    if (resourceId !== expectedPaperId || frame.command !== command || frame.child_id !== null) {
      throw new Error(`${command}_authoritative_route_mismatch`);
    }
    const response = await sendCommand(command, resourceId, null, frame.payload);
    const value = await responseValue(response);
    show(output, response.ok ? message : value, response.ok);
    if (response.ok) window.setTimeout(() => window.location.reload(), 450);
  } catch (error) {
    show(output, error.message, false);
  } finally {
    restoreAuthoritativeFormControls(form, buttons);
  }
}

async function freezeContributionLedger(paperId, budget, expectedPaperVersion, releaseCandidateHash) {
  const response = await sendCommand("create_contribution_ledger", paperId, null, {
    contribution_ledger_id: budget.contributionLedgerId,
    expected_paper_version: positiveInteger(expectedPaperVersion, "paper_version"),
    release_candidate_hash: canonicalDigest(releaseCandidateHash, "release_candidate_hash"),
    entries: budget.requestEntries,
    // The deterministic resource UUID is also the durable idempotency key, so
    // a reload after a lost response replays the exact same transaction.
    idempotency_key: budget.contributionLedgerId
  });
  const value = await responseValue(response);
  if (!response.ok) {
    const error = new Error(value && value.error ? value.error : "contribution_ledger_freeze_failed");
    error.status = response.status;
    throw error;
  }
  if (!value || value.schema !== CONTRIBUTION_LEDGER_SCHEMA ||
      value.contribution_ledger_id !== budget.contributionLedgerId ||
      value.paper_project_id !== canonicalUuid(paperId, "paper_id") ||
      value.release_candidate_hash !== canonicalDigest(releaseCandidateHash, "release_candidate_hash") ||
      value.ledger_hash !== budget.ledgerHash) {
    throw new Error("contribution_ledger_response_does_not_match_the_frozen_budget");
  }
  return value;
}

function bindManifestPicker(form) {
  const picker = form.elements.manifest_source;
  if (!picker) return;
  const applyManifest = () => {
    const option = picker.selectedOptions[0];
    if (!option || !option.value) return;
    form.elements.source_manifest_hash.value = option.dataset.sourceManifestHash || "";
    form.elements.artifact_manifest_hash.value = option.dataset.artifactManifestHash || "";
    form.elements.bibliography_hash.value = option.dataset.bibliographyHash || "";
    form.elements.claim_evidence_graph_hash.value = option.dataset.claimEvidenceGraphHash || "";
  };
  picker.addEventListener("change", applyManifest);
  applyManifest();
}

function bindGuidedPaperActions() {
  for (const form of document.querySelectorAll(".input-manifest-wizard-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        const plannedRunId = form.elements.planned_run_id.value.trim();
        if (!LOGICAL_SESSION_PATTERN.test(plannedRunId)) {
          throw new Error("planned_run_id_must_be_a_safe_logical_identifier");
        }
        const inputs = [
          {
            name: "code",
            role: "experiment_code",
            file: form.elements.code_file.files[0],
            mediaType: form.elements.code_media_type.value,
          },
          {
            name: "dataset",
            role: "dataset",
            file: form.elements.dataset_file.files[0],
            mediaType: form.elements.dataset_media_type.value,
          },
          {
            name: "environment",
            role: "environment_manifest",
            file: form.elements.environment_file.files[0],
            mediaType: form.elements.environment_media_type.value,
          },
        ];
        for (let index = 0; index < inputs.length; index += 1) {
          const item = inputs[index];
          show(output, `Registering ${item.name} manifest (${index + 1}/${inputs.length})…`, true);
          await uploadAndRegisterManifest({
            paperId: form.dataset.paperId,
            paperVersion: form.dataset.paperVersion,
            challengeId: form.dataset.challengeId,
            bundleKind: item.name,
            requiredRunIds: [plannedRunId],
            objects: [{
              file: item.file,
              mediaType: item.mediaType,
              role: item.role,
              logicalPath: `inputs/${item.name}/${safeLogicalFilename(item.file && item.file.name, `${item.name}.bin`)}`
            }]
          });
        }
        show(output, "Three input manifests registered. The experiment plan can now bind them.", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }

  for (const form of document.querySelectorAll(".draft-manifest-wizard-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        const requiredRunIds = String(form.dataset.requiredRunIds || "")
          .split(",")
          .map(value => value.trim())
          .filter(Boolean);
        const paperFile = form.elements.paper_file.files[0];
        const bibliographyFile = form.elements.bibliography_file.files[0];
        const claimGraphFile = form.elements.claim_graph_file.files[0];
        show(output, "Hashing and uploading revision artifacts…", true);
        await uploadAndRegisterManifest({
          paperId: form.dataset.paperId,
          paperVersion: form.dataset.paperVersion,
          challengeId: form.dataset.challengeId,
          bundleKind: "draft",
          requiredRunIds,
          objects: [
            {
              file: paperFile,
              mediaType: form.elements.paper_media_type.value,
              role: "paper_source",
              logicalPath: `paper/${safeLogicalFilename(paperFile && paperFile.name, "paper.md")}`
            },
            {
              file: bibliographyFile,
              mediaType: form.elements.bibliography_media_type.value,
              role: "bibliography",
              logicalPath: `bibliography/${safeLogicalFilename(bibliographyFile && bibliographyFile.name, "references.bib")}`
            },
            {
              file: claimGraphFile,
              mediaType: form.elements.claim_graph_media_type.value,
              role: "claim_evidence_graph",
              logicalPath: `evidence/${safeLogicalFilename(claimGraphFile && claimGraphFile.name, "claim-evidence.json")}`
            }
          ]
        });
        show(output, "Revision bundle registered. Select it in Draft + Release.", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }

  for (const form of document.querySelectorAll(".review-ready-manifest-wizard-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        const requiredRunIds = String(form.dataset.requiredRunIds || "")
          .split(",")
          .map(value => value.trim())
          .filter(Boolean);
        const common = {
          paperId: form.dataset.paperId,
          paperVersion: form.dataset.paperVersion,
          challengeId: form.dataset.challengeId,
          requiredRunIds
        };
        const paperFile = form.elements.paper_file.files[0];
        const bibliographyFile = form.elements.bibliography_file.files[0];
        const claimGraphFile = form.elements.claim_graph_file.files[0];
        const evaluatorFile = form.elements.evaluator_file.files[0];
        const datasetFile = form.elements.review_dataset_file.files[0];
        const candidateFile = form.elements.candidate_file.files[0];
        show(output, "Registering the four exact same-Paper Review sources…", true);
        const draft = await uploadAndRegisterManifest({
          ...common,
          bundleKind: "review-draft-source",
          objects: [
            {
              file: paperFile,
              mediaType: form.elements.paper_media_type.value,
              role: "paper_source",
              logicalPath: `paper/${safeLogicalFilename(paperFile && paperFile.name, "paper.md")}`
            },
            {
              file: bibliographyFile,
              mediaType: form.elements.bibliography_media_type.value,
              role: "bibliography",
              logicalPath: `bibliography/${safeLogicalFilename(
                bibliographyFile && bibliographyFile.name,
                "references.bib"
              )}`
            },
            {
              file: claimGraphFile,
              mediaType: "application/json",
              role: "claim_evidence_graph",
              logicalPath: `evidence/${safeLogicalFilename(
                claimGraphFile && claimGraphFile.name,
                "claim-evidence.json"
              )}`
            }
          ]
        });
        const frozenEvaluator = await uploadAndRegisterManifest({
          ...common,
          bundleKind: "review-frozen-evaluator-source",
          objects: [{
            file: evaluatorFile,
            mediaType: "text/x-python; charset=utf-8",
            role: "frozen_evaluator",
            logicalPath: `evaluator/${safeLogicalFilename(
              evaluatorFile && evaluatorFile.name,
              "evaluator.py"
            )}`
          }]
        });
        const dataset = await uploadAndRegisterManifest({
          ...common,
          bundleKind: "review-dataset-source",
          objects: [{
            file: datasetFile,
            mediaType: form.elements.review_dataset_media_type.value,
            role: "dataset",
            logicalPath: `inputs/${safeLogicalFilename(
              datasetFile && datasetFile.name,
              "dataset.json"
            )}`
          }]
        });
        const candidate = await uploadAndRegisterManifest({
          ...common,
          bundleKind: "review-candidate-source",
          objects: [{
            file: candidateFile,
            mediaType: "application/json",
            role: "candidate",
            logicalPath: `inputs/${safeLogicalFilename(
              candidateFile && candidateFile.name,
              "candidate.json"
            )}`
          }]
        });
        show(output, "Server-validating the exact frozen Review assembly…", true);
        await registerReviewReadyManifest({
          ...common,
          draft,
          frozenEvaluator,
          dataset,
          candidate
        });
        show(output, "Review-ready release bundle registered and frozen.", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }

  for (const form of document.querySelectorAll(".run-artifact-wizard-form")) {
    const status = form.elements.status;
    const successFields = form.querySelectorAll(".run-artifact-success-field");
    const failureFields = form.querySelectorAll(".run-artifact-failure-field");
    const updateArtifactOutcomeFields = () => {
      const succeeded = status.value === "succeeded";
      for (const field of successFields) field.hidden = !succeeded;
      for (const field of failureFields) field.hidden = succeeded;
      form.elements.output_file.required = succeeded;
      form.elements.output_media_type.required = succeeded;
      form.elements.metrics_file.required = succeeded;
      form.elements.metrics_media_type.required = succeeded;
      form.elements.failure.required = !succeeded;
    };
    status.addEventListener("change", updateArtifactOutcomeFields);
    updateArtifactOutcomeFields();
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        show(output, "Hashing and preserving the exact run logs, outputs, and metrics…", true);
        const receipt = await createAuthoritativeRunFromArtifacts(form);
        form.dataset.runRecordId = receipt.runRecordId;
        form.dataset.logsManifestId = receipt.logsManifestId;
        form.dataset.logsManifestHash = receipt.logsManifestHash;
        if (receipt.outputsManifestId) {
          form.dataset.outputsManifestId = receipt.outputsManifestId;
          form.dataset.outputsManifestHash = receipt.outputsManifestHash;
        } else {
          delete form.dataset.outputsManifestId;
          delete form.dataset.outputsManifestHash;
        }
        show(output, "Run artifacts and the exact outcome are authoritatively registered.", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }

  for (const form of document.querySelectorAll(".figure-lineage-wizard-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        show(output, "Hashing and preserving the figure plus its transform lineage…", true);
        const receipt = await createAuthoritativeFigureLineage(form);
        form.dataset.figureLineageId = receipt.figureLineageId;
        form.dataset.figureManifestId = receipt.figureManifestId;
        form.dataset.figureManifestHash = receipt.figureManifestHash;
        show(output, "Figure artifact and authoritative run lineage are registered.", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }

  for (const form of document.querySelectorAll(".role-resource-action-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const actionKind = String(form.dataset.actionKind || "");
      let action;
      if (actionKind === "evidence_assessment") {
        action = {
          kind: "evidence_assessment",
          evidence_card_id: canonicalUuid(
            form.elements.evidence_card_id.value,
            "evidence_card_id"
          )
        };
      } else if (actionKind === "captain_checkpoint") {
        action = { kind: "captain_checkpoint" };
      } else {
        show(form.querySelector("output"), "unsupported_role_resource_action", false);
        return;
      }
      await submitGuidedCommand(
        form,
        "create_role_resource_action",
        canonicalUuid(form.dataset.paperId, "paper_id"),
        null,
        {
          action_id: uuid(),
          expected_resource_version: positiveInteger(
            form.dataset.resourceVersion,
            "resource_version"
          ),
          action,
          idempotency_key: uuid()
        },
        null,
        actionKind === "evidence_assessment"
          ? "Evidence assessed; role focus and shared progress were updated authoritatively."
          : "Team checkpoint recorded from fresh Evidence and Experiment progress."
      );
    });
  }

  for (const form of document.querySelectorAll(".paper-phase-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "transition_paper_project",
        form.dataset.paperId,
        null,
        phaseTransitionPayload(form.dataset.paperVersion, form.elements.next_phase.value),
        null,
        "Checkpoint advanced. Refreshing the authoritative room…"
      );
    });
  }

  for (const form of document.querySelectorAll(".create-work-item-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const option = form.elements.assignee.selectedOptions[0];
      const playerId = option && option.value ? option.value : null;
      const bindingId = playerId ? option.dataset.bindingId || null : null;
      await submitGuidedCommand(
        form,
        "create_paper_work_item",
        form.dataset.paperId,
        null,
        workItemPayload(
          form.dataset.paperVersion,
          form.elements.kind.value,
          form.elements.title.value,
          playerId,
          bindingId
        ),
        null,
        "Task added to the authoritative mission board."
      );
    });
  }

  for (const form of document.querySelectorAll(".work-item-transition-form")) {
    const status = form.elements.next_status;
    const artifactField = form.querySelector(".accepted-artifact-field");
    const updateRequirement = () => {
      const accepting = status.value === "accepted";
      artifactField.hidden = !accepting;
      form.elements.artifact_manifest_hash.required = accepting;
    };
    status.addEventListener("change", updateRequirement);
    updateRequirement();
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "transition_paper_work_item",
        form.dataset.workItemId,
        null,
        workItemTransitionPayload(
          form.dataset.workItemVersion,
          status.value,
          form.elements.artifact_manifest_hash.value
        ),
        null,
        "Task status updated."
      );
    });
  }

  for (const form of document.querySelectorAll(".create-experiment-plan-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        const payload = await experimentPlanPayload({
          protocol: form.elements.protocol.value,
          codeManifestId: form.elements.code_manifest_id.value,
          datasetManifestId: form.elements.dataset_manifest_id.value,
          environmentManifestId: form.elements.environment_manifest_id.value,
          seedPolicy: form.elements.seed_policy.value,
          stoppingRule: form.elements.stopping_rule.value
        });
        await submitGuidedCommand(
          form,
          "create_experiment_plan",
          form.dataset.paperId,
          null,
          payload,
          null,
          "Experiment plan frozen with exact manifest lineage."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".create-evidence-card-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const sourceFile = form.elements.source_file.files[0];
        show(output, "Hashing and preserving the exact source snapshot…", true);
        const receipt = await uploadCasArtifact(
          form.dataset.paperId,
          sourceFile,
          form.elements.source_media_type.value
        );
        const payload = evidenceVerificationPayload({
          sourceUri: form.elements.source_uri.value,
          sourceHash: receipt.digest,
          locator: form.elements.locator.value,
          license: form.elements.license.value
        });
        await submitSignedGuidedCommand(
          form,
          "create_evidence_card",
          form.dataset.paperId,
          null,
          payload,
          "Verified evidence recorded with your exact local signature."
        );
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }

  for (const form of document.querySelectorAll(".create-citation-record-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        const payload = citationVerificationPayload({
          evidenceCardId: form.elements.evidence_card_id.value,
          doi: form.elements.doi.value,
          canonicalUrl: form.elements.canonical_url.value
        });
        await submitSignedGuidedCommand(
          form,
          "create_citation_record",
          form.dataset.paperId,
          null,
          payload,
          "Canonical citation bound to authoritative verified evidence."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".create-claim-record-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        const payload = await claimRecordPayload({
          claimKey: form.elements.claim_key.value,
          claimKind: form.elements.claim_kind.value,
          statement: form.elements.statement.value,
          evidenceCardIds: selectedValues(form.elements.evidence_card_ids),
          runRecordIds: selectedValues(form.elements.run_record_ids),
          figureLineageIds: selectedValues(form.elements.figure_lineage_ids)
        });
        await submitGuidedCommand(
          form,
          "create_claim_record",
          form.dataset.paperId,
          null,
          payload,
          null,
          "Claim bound to its authoritative evidence and run lineage."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".create-run-record-form")) {
    const status = form.elements.status;
    const successFields = form.querySelectorAll(".run-success-field");
    const failureFields = form.querySelectorAll(".run-failure-field");
    const updateOutcomeFields = () => {
      const succeeded = status.value === "succeeded";
      for (const field of successFields) field.hidden = !succeeded;
      for (const field of failureFields) field.hidden = succeeded;
      form.elements.outputs_manifest_id.required = succeeded;
      form.elements.metrics.required = succeeded;
      form.elements.failure.required = !succeeded;
    };
    status.addEventListener("change", updateOutcomeFields);
    updateOutcomeFields();
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        const payload = await runRecordPayload({
          experimentPlanId: form.elements.experiment_plan_id.value,
          status: status.value,
          seed: form.elements.seed.value,
          parameters: form.elements.parameters.value,
          logsManifestId: form.elements.logs_manifest_id.value,
          outputsManifestId: form.elements.outputs_manifest_id.value,
          metrics: form.elements.metrics.value,
          failure: form.elements.failure.value
        });
        await submitGuidedCommand(
          form,
          "create_run_record",
          form.dataset.paperId,
          null,
          payload,
          null,
          "Run retained, including its outcome and immutable lineage."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".create-paper-revision-form")) {
    bindManifestPicker(form);
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "create_paper_revision",
        form.dataset.paperId,
        null,
        paperRevisionPayload(form.dataset.paperVersion, form.dataset.parentRevisionId, {
          sourceManifestHash: form.elements.source_manifest_hash.value,
          artifactManifestHash: form.elements.artifact_manifest_hash.value,
          bibliographyHash: form.elements.bibliography_hash.value,
          claimEvidenceGraphHash: form.elements.claim_evidence_graph_hash.value
        }),
        null,
        "Frozen paper revision created."
      );
    });
  }

  for (const form of document.querySelectorAll(".acquire-section-lease-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        const selected = form.elements.section_key.selectedOptions[0];
        if (!selected) throw new Error("section_key_is_required");
        await requireActiveAgentBinding(form.dataset.holderBindingId);
        await submitGuidedCommand(
          form,
          "acquire_section_lease",
          form.dataset.paperId,
          null,
          acquireSectionLeasePayload({
            sectionKey: selected.value,
            holderBindingId: form.dataset.holderBindingId,
            currentRevisionId: form.dataset.currentRevisionId,
            previousFencingToken: selected.dataset.fencingToken,
            ttlSeconds: form.elements.ttl_seconds.value
          }),
          null,
          "Section lease acquired. Send the authoritative task to your local Agent Bridge."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".human-proposal-decision-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button) return;
      try {
        const payload = await humanDecisionPayload({
          proposalId: form.dataset.proposalId,
          proposalVersion: form.dataset.proposalVersion,
          decision: button.value,
          reason: form.elements.reason.value
        });
        await submitSignedGuidedCommand(
          form,
          "record_human_decision",
          form.dataset.paperId,
          null,
          payload,
          `Human ${button.value} decision signed and recorded.`
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".create-section-revision-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        await submitGuidedCommand(
          form,
          "create_section_revision",
          form.dataset.paperId,
          null,
          sectionRevisionPayload({
            sectionKey: form.dataset.sectionKey,
            parentRevisionId: form.dataset.parentRevisionId,
            proposalId: form.dataset.proposalId,
            leaseId: form.dataset.leaseId,
            fencingToken: form.dataset.fencingToken,
            patchManifestId: form.dataset.patchManifestId,
            patchHash: form.dataset.patchHash
          }),
          null,
          "Accepted Agent delivery materialized as an immutable section revision."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".section-review-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button) return;
      try {
        const payload = await sectionReviewPayload({
          revisionVersion: form.dataset.revisionVersion,
          verdict: button.value,
          review: form.elements.review.value
        });
        await submitSignedGuidedCommand(
          form,
          "submit_review",
          form.dataset.paperId,
          form.dataset.sectionRevisionId,
          payload,
          `Independent ${button.value} review signed and recorded.`
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".merge-section-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        await submitSignedGuidedCommand(
          form,
          "merge_section",
          form.dataset.paperId,
          null,
          sectionMergePayload({
            sectionRevisionId: form.dataset.sectionRevisionId,
            revisionVersion: form.dataset.revisionVersion,
            parentRevisionId: form.dataset.parentRevisionId,
            leaseId: form.dataset.leaseId,
            fencingToken: form.dataset.fencingToken
          }),
          "Approved section merged into the authoritative head."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }

  for (const form of document.querySelectorAll(".issue-authorization-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const sessionId = form.elements.session_id.value.trim();
      if (!LOGICAL_SESSION_PATTERN.test(sessionId)) {
        show(form.querySelector("output"), "session_id_must_be_a_safe_logical_identifier", false);
        return;
      }
      await submitGuidedCommand(
        form,
        "issue_research_session_authorization_set",
        null,
        null,
        {
          session_id: sessionId,
          paper_project_id: form.dataset.paperId,
          expected_paper_version: positiveInteger(form.dataset.paperVersion, "paper_version"),
          expected_team_version: positiveInteger(form.dataset.teamVersion, "team_version"),
          ttl_seconds: positiveInteger(form.elements.ttl_seconds.value, "ttl_seconds")
        },
        null,
        "Team authorization issued. Start the live session next."
      );
    });
  }

  for (const form of document.querySelectorAll(".replace-authorization-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "replace_research_session_authorization_set",
        null,
        null,
        {
          paper_project_id: form.dataset.paperId,
          expected_paper_version: positiveInteger(form.dataset.paperVersion, "paper_version"),
          expected_team_version: positiveInteger(form.dataset.teamVersion, "team_version"),
          previous_roster_version: positiveInteger(form.dataset.rosterVersion, "roster_version"),
          disconnected_participant_slot: positiveInteger(form.elements.participant_slot.value, "participant_slot"),
          ttl_seconds: positiveInteger(form.elements.ttl_seconds.value, "ttl_seconds")
        },
        form.dataset.sessionId,
        "Replacement authorization epoch issued."
      );
    });
  }

  for (const form of document.querySelectorAll(".research-session-control-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const command = form.dataset.command;
      const payload = command === "create_nakama_research_session_control" ||
          command === "replace_nakama_research_session_roster_control"
        ? { authorization_set_id: form.dataset.authorizationSetId }
        : {
            session_id: form.dataset.sessionId,
            roster_version: positiveInteger(form.dataset.rosterVersion, "roster_version")
          };
      await submitGuidedCommand(
        form,
        command,
        null,
        null,
        payload,
        null,
        "Live research control applied."
      );
    });
  }

  for (const form of document.querySelectorAll(".promote-release-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const buttons = form.querySelectorAll("button");
      let finishPaperWorkflow = null;
      for (const button of buttons) button.disabled = true;
      let promoteAttempted = false;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        const authors = promoteReleaseAuthors(form);
        const budget = await contributionLedgerBudget(
          form.dataset.paperId,
          form.dataset.revisionId,
          authors
        );
        const payload = {
          expected_paper_version: positiveInteger(form.dataset.paperVersion, "paper_version"),
          expected_revision_version: positiveInteger(form.dataset.revisionVersion, "revision_version"),
          title: form.elements.title.value.trim(),
          abstract_text: form.elements.abstract_text.value.trim(),
          collaboration_compact_hash: canonicalDigest(
            form.dataset.collaborationCompactHash,
            "collaboration_compact_hash"
          ),
          research_protocol_snapshot_hash: canonicalDigest(
            form.elements.research_protocol_snapshot_hash.value,
            "research_protocol_snapshot_hash"
          ),
          ethics_disclosure_hash: await preserveDisclosure(
            form,
            form.dataset.paperId,
            "ethics_disclosure_hash",
            "ethics_disclosure_text"
          ),
          coi_disclosure_hash: await preserveDisclosure(
            form,
            form.dataset.paperId,
            "coi_disclosure_hash",
            "coi_disclosure_text"
          ),
          contribution_ledger_id: budget.contributionLedgerId,
          contribution_ledger_hash: budget.ledgerHash,
          ai_disclosure_hash: await preserveDisclosure(
            form,
            form.dataset.paperId,
            "ai_disclosure_hash",
            "ai_disclosure_text"
          ),
          license: form.elements.license.value.trim(),
          authors: authors.map(author => ({
            author_order: author.author_order,
            participant_slot: author.participant_slot,
            player_id: author.player_id,
            display_name: author.display_name,
            credit_roles: author.credit_roles
          }))
        };
        promoteAttempted = true;
        const response = await sendCommand(
          "promote_paper_release_candidate",
          form.dataset.paperId,
          form.dataset.revisionId,
          payload
        );
        const value = await responseValue(response);
        if (!response.ok) {
          throw new Error(value && value.error ? value.error : "release_candidate_promotion_failed");
        }
        const releaseHash = canonicalDigest(
          value && value.release_candidate_hash,
          "release_candidate_hash"
        );
        const paperVersion = positiveInteger(value && value.paper_version, "paper_version");
        if (!value.revision || value.revision.revision_id !== canonicalUuid(form.dataset.revisionId, "revision_id") ||
            !value.revision.release_candidate ||
            value.revision.release_candidate.contribution_ledger_hash !== budget.ledgerHash) {
          throw new Error("promoted_release_does_not_match_the_budgeted_ledger");
        }
        show(output, "Release candidate promoted. Freezing the exact contribution ledger…", true);
        await freezeContributionLedger(
          form.dataset.paperId,
          budget,
          paperVersion,
          releaseHash
        );
        show(output, "Release and deterministic ledger are authoritative. Loading author consent…", true);
        requestPaperReload(form.dataset.paperId, 450);
      } catch (error) {
        show(
          output,
          promoteAttempted
            ? `Authoritative state may have advanced; reloading into ledger repair: ${error.message}`
            : error.message,
          false
        );
        if (promoteAttempted) requestPaperReload(form.dataset.paperId, 900);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, buttons);
      }
    });
  }

  for (const form of document.querySelectorAll(".freeze-contribution-ledger-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      let freezeAttempted = false;
      try {
        const budget = await contributionLedgerBudget(
          form.dataset.paperId,
          form.dataset.revisionId,
          frozenLedgerAuthors(form)
        );
        const expectedHash = canonicalDigest(form.dataset.expectedLedgerHash, "expected_ledger_hash");
        if (budget.ledgerHash !== expectedHash) {
          throw new Error("reconstructed_ledger_does_not_match_the_frozen_release_candidate");
        }
        freezeAttempted = true;
        await freezeContributionLedger(
          form.dataset.paperId,
          budget,
          form.dataset.paperVersion,
          form.dataset.releaseCandidateHash
        );
        show(output, "Deterministic contribution ledger frozen. Loading author consent…", true);
        window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(output, error.message, false);
        if (freezeAttempted) window.setTimeout(() => window.location.reload(), 900);
      } finally {
        button.disabled = false;
      }
    });
  }

  for (const form of document.querySelectorAll(".author-consent-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const frame = await signHumanPayload(
          "create_authorship_consent",
          form.dataset.paperId,
          null,
          {
            consent_id: uuid(),
            expected_paper_version: positiveInteger(form.dataset.paperVersion, "paper_version"),
            revision_id: form.dataset.revisionId,
            release_candidate_hash: canonicalDigest(
              form.dataset.releaseCandidateHash,
              "release_candidate_hash"
            )
          }
        );
        const response = await sendCommand(
          "create_authorship_consent",
          form.dataset.paperId,
          null,
          frame.payload
        );
        const value = await responseValue(response);
        show(output, response.ok ? "Author signature recorded." : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }

  for (const form of document.querySelectorAll(".finalize-paper-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "finalize_joint_paper_submission",
        form.dataset.paperId,
        null,
        {
          submission_id: uuid(),
          expected_paper_version: positiveInteger(form.dataset.paperVersion, "paper_version"),
          revision_id: form.dataset.revisionId,
          release_candidate_hash: canonicalDigest(
            form.dataset.releaseCandidateHash,
            "release_candidate_hash"
          )
        },
        null,
        "Author Raid finalized. Waiting for independent review and finality."
      );
    });
  }
}

const PARTY_CODE_V1_PATTERN = /^PR1-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

function bindQueueForms() {
  for (const button of document.querySelectorAll(".generate-party-code")) {
    button.addEventListener("click", () => {
      const form = button.closest("form.queue-form");
      const input = form?.elements?.party_code;
      if (!input) return;
      input.value = `PR1-${uuid()}`;
      input.focus();
      input.select();
    });
  }
  for (const form of document.querySelectorAll(".queue-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        if (form.dataset.agentReady !== "true") {
          throw new Error("queue_requires_exactly_one_active_agent_binding");
        }
        const authorizedRoles = String(form.dataset.authorizedRoles || "")
          .split(",")
          .map(value => value.trim())
          .filter(Boolean);
        const roles = Array.from(
          form.querySelectorAll('input[name="roles"]:checked'),
          input => input.value,
        );
        if (roles.length < 1 || roles.length > 3 || new Set(roles).size !== roles.length) {
          throw new Error("roles_must_be_1_to_3_distinct_values");
        }
        if (authorizedRoles.length === 0 || roles.some(role => !authorizedRoles.includes(role))) {
          throw new Error("selected_role_is_not_authorized_for_this_identity");
        }
        const availability = new TextEncoder().encode(form.elements.availability.value);
        const partyCodeInput = form.elements.party_code;
        const partyCode = String(partyCodeInput?.value || "").trim();
        if (partyCode && !PARTY_CODE_V1_PATTERN.test(partyCode)) {
          throw new Error("party_code_must_be_pr1_uuid_v4");
        }
        const partyCodeHash = partyCode
          ? await sha256Label(new TextEncoder().encode(partyCode))
          : null;
        if (partyCodeInput) partyCodeInput.value = "";
        const payload = {
          ticket_id: uuid(),
          challenge_id: form.dataset.challengeId,
          requested_team_size: 3,
          roles,
          availability_hash: await sha256Label(availability)
        };
        if (partyCodeHash) payload.party_code_hash = partyCodeHash;
        const response = await sendCommand("queue_matchmaking", null, null, payload);
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".cancel-ticket-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      await submitGuidedCommand(
        form,
        "cancel_matchmaking_ticket",
        form.dataset.ticketId,
        null,
        { expected_version: positiveInteger(form.dataset.ticketVersion, "ticket_version") },
        null,
        form.dataset.successMessage || "Queue ticket cancelled."
      );
    });
  }
}

// Lobby queue status is a read-only projection.  Keep its freshness visible
// without auto-navigating: a reload can discard an in-memory human signing key
// on adjacent flows, so the player chooses when to refresh the page.
function bindLobbyQueueFreshness() {
  if (window.location.pathname !== "/league") return;
  const panel = document.querySelector("[data-lobby-queue-panel]");
  const countdown = panel?.querySelector("[data-queue-refresh-countdown]");
  const state = panel?.querySelector("[data-queue-freshness-state]");
  const button = panel?.querySelector("[data-queue-refresh]");
  if (!panel || !countdown || !state || !button) return;
  const configured = Number(panel.dataset.queueRefreshSeconds || "");
  if (!Number.isSafeInteger(configured) || configured < 1 || configured > 300) {
    state.textContent = "Refresh timing unavailable / 刷新时序不可用";
    countdown.textContent = "—";
    return;
  }
  let remaining = configured;
  const render = () => {
    if (remaining <= 0) {
      state.textContent = "Refresh check available now / 现在可刷新检查";
      countdown.textContent = "now / 现在";
      button.classList.add("ready");
      return;
    }
    state.textContent = "Next refresh check in / 距下一次刷新检查";
    countdown.textContent = `${remaining}s`;
    button.classList.remove("ready");
  };
  render();
  const timer = window.setInterval(() => {
    remaining = Math.max(0, remaining - 1);
    render();
    if (remaining === 0) window.clearInterval(timer);
  }, 1000);
  button.addEventListener("click", () => {
    if (button.disabled) return;
    savePlayerFocusContext(button);
    button.disabled = true;
    button.textContent = "Refreshing authoritative queue… / 正在刷新权威队列……";
    window.location.reload();
  });
}

function bindProposalCountdowns() {
  for (const element of document.querySelectorAll(".proposal-countdown")) {
    const deadline = Date.parse(element.dataset.proposalExpiresAt || "");
    if (!Number.isFinite(deadline)) {
      element.textContent = "Deadline unavailable / 截止时间不可用";
      element.dataset.state = "unavailable";
      continue;
    }
    let timer = null;
    const update = () => {
      const remainingSeconds = Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
      if (remainingSeconds === 0) {
        element.textContent = "Expired — Hepta will requeue eligible acceptors / 已过期，Hepta 将重排可用玩家";
        element.dataset.state = "expired";
        window.clearInterval(timer);
        return;
      }
      const minutes = Math.floor(remainingSeconds / 60);
      const seconds = remainingSeconds % 60;
      const clock = `${minutes}:${String(seconds).padStart(2, "0")}`;
      element.textContent = `Hepta deadline in ${clock} / 距 Hepta 截止 ${clock}`;
      element.dataset.state = remainingSeconds <= 60 ? "urgent" : "open";
    };
    update();
    if (deadline > Date.now()) timer = window.setInterval(update, 1000);
  }
}

function compactChallengeDuration(totalSeconds) {
  const seconds = Math.max(0, Math.ceil(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  if (days > 0) return `${days}d ${hours}h ${minutes}m`;
  if (hours > 0) return `${hours}h ${minutes}m ${remainder}s`;
  return `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function challengeClockState(deadlineAt, graceExpiresAt, nowMs = Date.now()) {
  const deadline = Date.parse(deadlineAt || "");
  const grace = Date.parse(graceExpiresAt || "");
  if (!Number.isFinite(deadline) || !Number.isFinite(grace) || grace < deadline) {
    return {
      state: "unavailable",
      text: "Authoritative clock unavailable / 权威计时不可用",
    };
  }
  if (nowMs < deadline) {
    const remaining = compactChallengeDuration((deadline - nowMs) / 1000);
    return { state: "active", text: `${remaining} remaining / 剩余 ${remaining}` };
  }
  if (nowMs < grace) {
    const remaining = compactChallengeDuration((grace - nowMs) / 1000);
    return {
      state: "overtime",
      text: `Overtime · ${remaining} grace remaining / 加时 · 宽限剩余 ${remaining}`,
    };
  }
  return {
    state: "expired",
    text: "Grace elapsed · canonical expiry sync pending / 宽限已结束，等待自动同步超时终局",
  };
}

function bindChallengeCountdowns() {
  for (const element of document.querySelectorAll(".challenge-countdown")) {
    let timer = null;
    const update = () => {
      const clock = challengeClockState(
        element.dataset.deadlineAt,
        element.dataset.graceExpiresAt,
      );
      element.dataset.state = clock.state;
      element.textContent = clock.text;
      if (clock.state === "expired" || clock.state === "unavailable") {
        window.clearInterval(timer);
      }
    };
    update();
    if (element.dataset.state === "active" || element.dataset.state === "overtime") {
      timer = window.setInterval(update, 1000);
    }
  }
}

function bindChallengeOutcomeForms() {
  const allowed = Object.freeze({
    failed: new Set(["quality_gate_failed", "preregistered_result_failed", "integrity_failure"]),
    abandoned: new Set(["team_withdrawal", "resource_unavailable", "challenge_infeasible"]),
  });
  for (const form of document.querySelectorAll(".challenge-outcome-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector('button[type="submit"]');
      const outcome = String(form.elements.outcome.value || "");
      const reasonCode = String(form.elements.reason_code.value || "");
      const paperId = String(form.dataset.paperId || "");
      if (!allowed[outcome] || !allowed[outcome].has(reasonCode)) {
        show(output, "invalid_typed_challenge_outcome", false);
        return;
      }
      const confirmation = String(form.dataset.confirmMessage || "");
      if (confirmation && !window.confirm(confirmation)) return;
      button.disabled = true;
      try {
        const response = await mutation(
          `/api/papers/${encodeURIComponent(paperId)}/outcome`,
          {
            method: "POST",
            headers: { "content-type": "application/json", "accept": "application/json" },
            body: JSON.stringify({ outcome, reason_code: reasonCode }),
          },
        );
        const value = await responseValue(response);
        show(
          output,
          response.ok ? "Immutable challenge outcome recorded. Refreshing…" : value,
          response.ok,
        );
        if (response.ok) window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindReviewQueue() {
  for (const form of document.querySelectorAll(".review-claim-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const session = await currentSession();
        const claim = reviewClaimPayload(session, form.dataset);
        const response = await sendCommand(
          "claim_review_assignment",
          claim.paperId,
          null,
          claim.payload
        );
        const value = await responseValue(response);
        show(output, response.ok ? "Review assignment claimed. Opening the frozen bundle…" : value, response.ok);
        if (response.ok) {
          window.setTimeout(
            () => window.location.assign(`/league/review/${encodeURIComponent(claim.paperId)}`),
            350
          );
        }
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindReviewRaid() {
  for (const form of document.querySelectorAll(".review-receipt-confirm-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      let finishPaperWorkflow = null;
      button.disabled = true;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        const response = await confirmReviewReceipt(form);
        const value = await responseValue(response);
        show(
          output,
          response.ok
            ? "Server-verified execution receipt signed and consumed."
            : value,
          response.ok
        );
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, [button]);
      }
    });
  }
  for (const form of document.querySelectorAll(".review-attestation-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button) return;
      let finishPaperWorkflow = null;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        await submitSignedGuidedCommand(
          form,
          "submit_evaluation_draft_attestation",
          canonicalUuid(form.dataset.paperId, "paper_id"),
          canonicalUuid(form.dataset.evaluationId, "evaluation_id"),
          await evaluationAttestationPayload(form, button.value),
          "Independent reviewer attestation recorded."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, form.querySelectorAll("button"));
      }
    });
  }
  for (const form of document.querySelectorAll(".review-finalize-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      let finishPaperWorkflow = null;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        await submitGuidedCommand(
          form,
          "finalize_paper_evaluation_draft",
          canonicalUuid(form.dataset.paperId, "paper_id"),
          canonicalUuid(form.dataset.evaluationId, "evaluation_id"),
          {
            expected_draft_version: positiveInteger(
              form.dataset.draftVersion,
              "draft_version"
            )
          },
          null,
          "Two-reviewer quorum verified; evaluation finalized."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, form.querySelectorAll("button"));
      }
    });
  }
  for (const form of document.querySelectorAll(".review-appeal-resolution-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button || button.disabled) return;
      let finishPaperWorkflow = null;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        await submitDerivedSignedGuidedCommand(
          form,
          "resolve_appeal",
          await appealResolutionPayload(form, button.value),
          "Independent Appeal resolution signed and recorded."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, form.querySelectorAll("button"));
      }
    });
  }
}

function bindAuthorAppeal() {
  for (const form of document.querySelectorAll(".author-appeal-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      let finishPaperWorkflow = null;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        await submitDerivedSignedGuidedCommand(
          form,
          "submit_appeal",
          await appealPayload(form),
          "Appeal signed; scientific finality is now on hold."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, form.querySelectorAll("button"));
      }
    });
  }
}

function bindAuthorRework() {
  for (const form of document.querySelectorAll(".author-rework-start-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      let finishPaperWorkflow = null;
      try {
        finishPaperWorkflow = beginPaperWorkflow(form.dataset.paperId);
        await submitPaperDerivedSignedGuidedCommand(
          form,
          "start_paper_rework",
          await paperReworkPayload(form),
          "Rework lease signed and started. Opening the normal Author workflow…"
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      } finally {
        if (finishPaperWorkflow) finishPaperWorkflow();
        restoreAuthoritativeFormControls(form, form.querySelectorAll("button"));
      }
    });
  }
}

function disableExpiredPaperRework(paperId) {
  for (const form of document.querySelectorAll("form[data-paper-id]")) {
    if (form.dataset.paperId !== paperId) continue;
    form.dataset.reworkLeaseState = "expired";
    for (const control of form.querySelectorAll("button, input, select, textarea")) {
      control.disabled = true;
    }
    const output = form.querySelector("output");
    if (output) show(output, "Rework lease expired; reload authoritative Paper state.", false);
  }
}

function bindPaperReworkCountdowns() {
  for (const panel of document.querySelectorAll(".author-rework-lease")) {
    const paperId = String(panel.dataset.paperId || "");
    const deadline = Date.parse(panel.dataset.reworkExpiresAt || "");
    const output = panel.querySelector(".paper-rework-countdown");
    let timer = null;
    const expire = () => {
      panel.dataset.reworkState = "expired";
      if (output) output.textContent = "Expired / 已过期";
      disableExpiredPaperRework(paperId);
      if (timer !== null) window.clearInterval(timer);
    };
    if (!output || !Number.isFinite(deadline) || !paperId) {
      panel.dataset.reworkState = "unavailable";
      if (output) output.textContent = "Unavailable / 不可用";
      disableExpiredPaperRework(paperId);
      continue;
    }
    const update = () => {
      if (deadline <= Date.now()) {
        expire();
        return;
      }
      const remaining = compactChallengeDuration((deadline - Date.now()) / 1000);
      output.textContent = `${remaining} / 剩余 ${remaining}`;
      panel.dataset.reworkState = "active";
    };
    update();
    if (panel.dataset.reworkState === "active") timer = window.setInterval(update, 1000);
  }
}

function reviewClaimPayload(session, dataset) {
  const scopeForSlot = Object.freeze({
    evaluator: "evaluator",
    reviewer_1: "reviewer",
    reviewer_2: "reviewer",
    reproducer: "reproducer"
  });
  const paperId = String(dataset.paperId || "");
  const playerId = String(dataset.playerId || "");
  const slot = String(dataset.slot || "");
  const reviewRound = positiveInteger(dataset.reviewRound, "review_round");
  const requiredScope = scopeForSlot[slot];
  const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  if (!session || typeof session.player_id !== "string" || !Array.isArray(session.scopes) ||
      !uuidPattern.test(paperId) || !uuidPattern.test(playerId) || session.player_id !== playerId) {
    throw new Error("review_claim_session_identity_mismatch");
  }
  if (!requiredScope || !session.scopes.includes(requiredScope)) {
    throw new Error("review_claim_slot_not_authorized_for_identity");
  }
  return {
    paperId,
    payload: {
      assignment_id: uuid(),
      player_id: session.player_id,
      review_round: reviewRound,
      slot
    }
  };
}

function bindProposalForms() {
  for (const form of document.querySelectorAll(".proposal-decision")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = event.submitter;
      if (!button) return;
      for (const candidate of form.querySelectorAll("button")) candidate.disabled = true;
      try {
        const response = await sendCommand("decide_team_proposal", form.dataset.proposalId, null, {
          decision_id: uuid(),
          expected_proposal_version: Number(form.dataset.proposalVersion),
          decision: button.value
        });
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        for (const candidate of form.querySelectorAll("button")) candidate.disabled = false;
      }
    });
  }
}

function bindCommandForms() {
  for (const form of document.querySelectorAll(".command-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        if (!payload || Array.isArray(payload) || typeof payload !== "object") {
          throw new Error("payload_must_be_a_json_object");
        }
        const child = form.elements.child_id ? form.elements.child_id.value.trim() : null;
        const response = await sendCommand(
          form.dataset.command,
          form.dataset.resourceId || null,
          child || null,
          payload
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 1000);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindArtifactForms() {
  for (const form of document.querySelectorAll(".artifact-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const file = form.elements.artifact.files[0];
        if (!file || file.size === 0 || file.size > 32 * 1024 * 1024) {
          throw new Error("artifact_size_must_be_1_to_33554432_bytes");
        }
        const bytes = await file.arrayBuffer();
        const digest = await sha256Label(bytes);
        const response = await mutation(
          `/api/papers/${encodeURIComponent(form.dataset.paperId)}/artifacts/${encodeURIComponent(digest)}`,
          {
            method: "PUT",
            headers: {
              "content-type": form.elements.media_type.value,
              "accept": "application/json"
            },
            body: file
          }
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function liveCursorKey(paperId) {
  return `hepta.paper-raid.live-cursor.v1:${paperId}`;
}

function validLogicalSessionId(value) {
  return typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value);
}

function readLiveCursor(paperId) {
  try {
    const value = JSON.parse(sessionStorage.getItem(liveCursorKey(paperId)) || "{}");
    return {
      hepta: Number.isSafeInteger(value.hepta) && value.hepta >= 0 ? value.hepta : 0
    };
  } catch (_) {
    return { hepta: 0 };
  }
}

function nextHeptaCursor(events, current) {
  if (!Array.isArray(events)) return current;
  return events.reduce((cursor, event) => {
    const value = event && event.cursor;
    return Number.isSafeInteger(value) && value > cursor ? value : cursor;
  }, current);
}

function reconcileNakamaSessions(heads, current) {
  if (!Array.isArray(heads) || heads.length > 1) throw new Error("invalid_research_session_heads");
  const next = new Map();
  for (const head of heads) {
    const sessionId = head && head.logical_session_id;
    const rosterVersion = head && head.roster_version;
    if (!validLogicalSessionId(sessionId)
      || !Number.isSafeInteger(rosterVersion)
      || rosterVersion <= 0
      || head.nakama_completion_received !== false
      || next.has(sessionId)) {
      throw new Error("invalid_research_session_head");
    }
    const previous = current.get(sessionId);
    next.set(sessionId, previous && previous.rosterVersion === rosterVersion
      ? previous
      : { rosterVersion, sequence: 0, hasMore: true });
  }
  return next;
}

function acceptNakamaArchive(entry, state) {
  const archive = entry && entry.archive;
  const nextSequence = archive && archive.next_after_sequence;
  if (!entry
    || entry.logical_session_id === undefined
    || entry.roster_version !== state.rosterVersion
    || entry.requested_after_sequence !== state.sequence
    || !archive
    || archive.schema !== "trnm.nakama.research-session.archive.v1"
    || archive.logical_session_id !== entry.logical_session_id
    || archive.roster_version !== state.rosterVersion
    || archive.after_sequence !== state.sequence
    || !Number.isSafeInteger(nextSequence)
    || nextSequence < state.sequence
    || typeof archive.has_more !== "boolean"
    || (archive.has_more && nextSequence === state.sequence)) {
    throw new Error("invalid_nakama_archive_cursor");
  }
  return {
    rosterVersion: state.rosterVersion,
    sequence: nextSequence,
    hasMore: archive.has_more
  };
}

const LIVE_AUTHORITY_POLL_MS = 500;
const LIVE_AUTHORITY_REQUEST_TIMEOUT_MS = 750;
const LIVE_AUTHORITY_RETRY_BASE_MS = 100;
const LIVE_AUTHORITY_RETRY_MAX_MS = 1_000;
const REVIEW_AUTHORITY_REVISION = /^[0-9a-f]{64}$/;

function liveAuthorityRetryDelayMs(failures) {
  if (!Number.isSafeInteger(failures) || failures < 1) {
    throw new Error("live_authority_failure_count_is_invalid");
  }
  return Math.min(
    LIVE_AUTHORITY_RETRY_MAX_MS,
    LIVE_AUTHORITY_RETRY_BASE_MS * (2 ** Math.min(failures - 1, 4))
  );
}

function reviewAuthorityRevisionFromHtml(html, paperId) {
  const documentValue = new DOMParser().parseFromString(html, "text/html");
  const cards = documentValue.querySelectorAll(".review-authority-watch");
  if (cards.length !== 1) throw new Error("review_authority_marker_missing");
  const card = cards[0];
  if (card.dataset.paperId !== paperId ||
      !REVIEW_AUTHORITY_REVISION.test(String(card.dataset.authorityRevision || ""))) {
    throw new Error("review_authority_marker_invalid");
  }
  return card.dataset.authorityRevision;
}

async function fetchReviewAuthorityRevision(paperId) {
  const response = await fetch(`/league/review/${encodeURIComponent(paperId)}`, {
    credentials: "same-origin",
    headers: { "accept": "text/html" },
    signal: AbortSignal.timeout(LIVE_AUTHORITY_REQUEST_TIMEOUT_MS)
  });
  if (response.redirected || [401, 403, 404].includes(response.status)) return null;
  if (!response.ok) throw new Error("review_authority_sync_failed");
  return reviewAuthorityRevisionFromHtml(await response.text(), paperId);
}

function invalidateStaleReviewAuthority(card, connection, output) {
  if (new Set(["stale", "stale-pending-workflow"]).has(card.dataset.authorityState)) return;
  const paperId = card.dataset.paperId;
  const applyInvalidation = () => {
    card.dataset.authorityState = "stale";
    connection.dataset.state = "stale-authority";
    connection.textContent = "Review state changed · reloading / 评审状态已变化，正在刷新";
    for (const form of document.querySelectorAll(`form[data-paper-id="${CSS.escape(paperId)}"]`)) {
      form.dataset.authorityState = "stale";
      for (const control of form.elements) control.disabled = true;
    }
    show(
      output,
      "Another participant advanced the Review Raid. Old controls are disabled; loading the current assignment…",
      false
    );
    const reloadCurrentAuthority = () => requestPaperReload(paperId, 0);
    recordProductEvent("stale_ui_reload", { paperId }).finally(reloadCurrentAuthority);
    window.setTimeout(reloadCurrentAuthority, STALE_AUTHORITY_RELOAD_DELAY_MS);
  };
  const pendingWorkflow = requestStaleAuthorityRefresh(paperId, applyInvalidation);
  if (pendingWorkflow) {
    card.dataset.authorityState = "stale-pending-workflow";
    connection.dataset.state = "stale-authority-pending-workflow";
    connection.textContent = "Review changed · finishing this confirmation / 评审已变化，正在完成当前确认";
  }
}

function createReviewAuthoritySync(card) {
  const paperId = canonicalUuid(card.dataset.paperId, "paper_id");
  const baselineRevision = String(card.dataset.authorityRevision || "");
  if (!REVIEW_AUTHORITY_REVISION.test(baselineRevision)) {
    throw new Error("review_authority_revision_is_invalid");
  }
  const button = card.querySelector(".review-authority-refresh");
  const connection = card.querySelector(".review-authority-connection");
  const output = card.querySelector(".review-authority-detail");
  let timer = null;
  let running = false;
  let failures = 0;
  let wasDisconnected = false;
  const schedule = delay => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => sync.run(false), delay);
  };
  const sync = {
    async run(manual) {
      if (running || new Set(["stale", "stale-pending-workflow"]).has(card.dataset.authorityState)) {
        return;
      }
      if (!manual && document.visibilityState === "hidden") {
        schedule(1250);
        return;
      }
      running = true;
      if (manual) button.disabled = true;
      try {
        const revision = await fetchReviewAuthorityRevision(paperId);
        if (revision === null || revision !== baselineRevision) {
          invalidateStaleReviewAuthority(card, connection, output);
          return;
        }
        connection.dataset.state = "live";
        connection.textContent = "Live · current / 实时 · 当前状态";
        show(output, "Review authority is current.", true);
        if (wasDisconnected) {
          try { await recordProductEvent("reconnected", { paperId }); } catch (_) {}
          wasDisconnected = false;
        }
        failures = 0;
        schedule(LIVE_AUTHORITY_POLL_MS);
      } catch (error) {
        failures += 1;
        wasDisconnected = true;
        connection.dataset.state = "reconnecting";
        connection.textContent = "Reconnecting / 正在重连";
        show(output, "Review authority is temporarily unreachable; retrying…", false);
        schedule(liveAuthorityRetryDelayMs(failures));
      } finally {
        running = false;
        button.disabled = false;
      }
    }
  };
  button.addEventListener("click", () => sync.run(true));
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") sync.run(true);
  });
  return sync;
}

function bindReviewAuthority() {
  for (const card of document.querySelectorAll(".review-authority-watch")) {
    const sync = createReviewAuthoritySync(card);
    sync.run(true);
  }
}

async function fetchTimelineValue(paperId, query) {
  const response = await fetch(`/api/papers/${encodeURIComponent(paperId)}/timeline?${query}`, {
    credentials: "same-origin",
    headers: { "accept": "application/json" },
    signal: AbortSignal.timeout(LIVE_AUTHORITY_REQUEST_TIMEOUT_MS)
  });
  const value = await responseValue(response);
  if (!response.ok) throw new Error(value && value.error ? value.error : "live_sync_failed");
  return value;
}

function paperRoomPhase(value) {
  const room = value && value.paper_room && typeof value.paper_room === "object" ? value.paper_room : {};
  const paper = room.paper && typeof room.paper === "object" ? room.paper : {};
  return typeof paper.phase === "string" ? paper.phase : "waiting_for_authority";
}

const PHASE_LABELS = Object.freeze({
  forming: "Form team / 组队成形",
  preregistering: "Preregister / 预注册",
  researching: "Research / 研究",
  experimenting: "Experiment / 实验",
  drafting: "Draft / 起草",
  integrity_review: "Integrity review / 完整性审查",
  reproducing: "Reproduction readiness / 复现准备",
  reproduction_readiness: "Reproduction readiness / 复现准备",
  author_approval: "Author approval / 作者批准",
  integrity_hold: "Integrity hold / 完整性挂起",
  submission_ready: "Author Raid complete / 作者远征完成",
  waiting_for_authority: "Waiting for authority / 等待权威状态",
});

const ROLE_LABELS = Object.freeze({
  captain: "Captain / 队长",
  evidence: "Evidence / 证据",
  experiment: "Experiment / 实验",
  evaluator: "Evaluator / 评估员",
  reviewer_1: "Reviewer 1 / 评审一",
  reviewer_2: "Reviewer 2 / 评审二",
  reproducer: "Reproducer / 复现员",
});

const EVENT_LABELS = Object.freeze({
  "hepta.paper_raid.human_player.created.v2": "Human researcher registered / 人类研究者已注册",
  "hepta.paper_raid.agent_binding.created.v2": "Agent paired / Agent 已配对",
  "hepta.paper_raid.agent_binding.key_rotated.v2": "Agent key rotated / Agent 密钥已轮换",
  "hepta.paper_raid.matchmaking_ticket.created.v1": "Joined matchmaking / 已加入匹配",
  "hepta.paper_raid.matchmaking_ticket.cancelled.v1": "Left matchmaking / 已取消匹配",
  "hepta.paper_raid.team_proposal.created.v1": "Team proposal ready / 组队提案已生成",
  "hepta.paper_raid.team_proposal.decision.v1": "Team decision recorded / 组队决定已记录",
  "hepta.paper_raid.team.materialized.v1": "Research team materialized / 研究队伍已成形",
  "hepta.paper_raid.team_member.accepted.v2": "Teammate accepted / 队友已接受",
  "hepta.paper_raid.team.locked.v2": "Author roster locked / 作者阵容已锁定",
  "hepta.paper_raid.paper_project.created.v2": "Author Raid created / 作者远征已创建",
  "hepta.paper_raid.paper_project.phase_changed.v2": "Checkpoint advanced / 阶段已推进",
  "hepta.paper_raid.work_item.created.v2": "Mission task created / 任务已创建",
  "hepta.paper_raid.work_item.status_changed.v2": "Mission task updated / 任务已更新",
  "hepta.paper_raid.artifact_manifest.created.v1": "Artifact manifest registered / 工件清单已登记",
  "hepta.paper_raid.evidence_card.created.v1": "Evidence verified / 证据已验证",
  "hepta.paper_raid.citation.created.v1": "Citation bound / 引用已绑定",
  "hepta.paper_raid.experiment_plan.created.v1": "Experiment plan frozen / 实验计划已锁定",
  "hepta.paper_raid.run_record.created.v1": "Experiment run retained / 实验运行已保留",
  "hepta.paper_raid.claim.created.v1": "Claim lineage bound / 论断谱系已绑定",
  "hepta.paper_raid.section_lease.acquired.v1": "Section lease acquired / 章节租约已获取",
  "hepta.paper_raid.agent_proposal.submitted.v1": "Agent proposal received / Agent 建议已收到",
  "hepta.paper_raid.human_decision.created.v1": "Human decision recorded / 人类决定已记录",
  "hepta.paper_raid.section_revision.created.v1": "Section revision created / 章节版本已创建",
  "hepta.paper_raid.section_review.created.v1": "Section review recorded / 章节审查已记录",
  "hepta.paper_raid.section_merge.created.v1": "Section merged / 章节已合并",
  "hepta.paper_raid.paper_revision.created.v2": "Whole-paper revision frozen / 全文版本已锁定",
  "hepta.paper_raid.release_candidate.promoted.v2": "Release candidate promoted / 发布候选已生成",
  "hepta.paper_raid.contribution_ledger.frozen.v1": "Provisional contribution ledger frozen / 暂定贡献账本已锁定",
  "hepta.paper_raid.authorship_consent.accepted.v2": "Author consent signed / 作者同意已签署",
  "hepta.paper_raid.joint_submission.finalized.v2": "PaperBundle finalized / 论文包已完成",
  "hepta.paper_raid.joint_submission.integrity_held.v2": "PaperBundle held for integrity review / 论文包进入完整性挂起",
  "hepta.paper_raid.review_assignment.claimed.v1": "Independent review assignment claimed / 独立评审任务已领取",
  "hepta.paper_raid.evaluation_draft.created.v1": "Evaluation draft opened / 评估草案已开启",
  "hepta.paper_raid.evaluation_draft.attested.v1": "Independent review attested / 独立评审已签注",
  "hepta.paper_raid.evaluation_draft.finalized.v1": "Evaluation quorum finalized / 评估仲裁已完成",
  "hepta.paper_raid.evaluation.recorded.v1": "Evaluation recorded / 评估已记录",
  "hepta.paper_raid.reproduction.recorded.v1": "Independent reproduction submitted / 独立复现已提交",
  "hepta.paper_raid.appeal.opened.v1": "Integrity appeal opened / 完整性申诉已开启",
  "hepta.paper_raid.appeal.resolved.v1": "Integrity appeal resolved / 完整性申诉已裁决",
  "hepta.paper_raid.research_session_authorizations.issued.v1": "Research session authorized / 科研会话已授权",
  "hepta.paper_raid.research_session_authorizations.replaced.v1": "Research roster replaced / 科研阵容已替换",
  "hepta.paper_raid.nakama_completion.verified.v1": "Research session completion verified / 科研会话完成已验证",
});

function semanticPhaseLabel(phase) {
  return PHASE_LABELS[phase] || humanizeProtocolToken(phase);
}

function semanticRoleLabel(role) {
  return ROLE_LABELS[role] || humanizeProtocolToken(role || "teammate");
}

function humanizeProtocolToken(value) {
  const text = String(value || "event")
    .replace(/^hepta\.paper_raid\./, "")
    .replace(/\.v\d+$/, "")
    .replace(/[._:-]+/g, " ")
    .trim();
  return text ? `${text.charAt(0).toUpperCase()}${text.slice(1)}` : "Event";
}

function semanticEventLabel(event) {
  const type = event && (event.event_type || event.action_type || event.kind);
  const base = EVENT_LABELS[type] || humanizeProtocolToken(type);
  const payload = event && event.payload && typeof event.payload === "object" ? event.payload : {};
  if (typeof payload.phase === "string") return `${base} → ${semanticPhaseLabel(payload.phase)}`;
  if (typeof payload.status === "string") return `${base} · ${humanizeProtocolToken(payload.status)}`;
  if (typeof payload.section_key === "string") return `${base} · ${payload.section_key}`;
  return base;
}

const TIMELINE_REPLAY_MAX_EVENTS = 64;
const TIMELINE_REPLAY_STEP_MS = 650;

function timelineEventRecords(value) {
  const records = [
    ...(Array.isArray(value && value.hepta_events) ? value.hepta_events : []).map(event => ({
      source: "hepta",
      event,
    })),
    ...(Array.isArray(value && value.nakama_archives) ? value.nakama_archives : []).flatMap(entry => (
      Array.isArray(entry.archive && entry.archive.events)
        ? entry.archive.events.map(event => ({
          source: `nakama:${entry.logical_session_id || "unknown"}`,
          event,
        }))
        : []
    )),
  ];
  const seen = new Set();
  return records.filter(record => {
    const event = record.event || {};
    const identity = event.event_id || event.cursor || `${event.event_type || "event"}:${event.sequence || "0"}`;
    const key = `${record.source}:${identity}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }).slice(-TIMELINE_REPLAY_MAX_EVENTS);
}

function appendTimelineReplayEvent(list, record, index) {
  const item = document.createElement("li");
  const event = record && record.event ? record.event : {};
  const identity = event.event_id || event.cursor || `${event.event_type || "event"}:${event.sequence || "0"}`;
  item.dataset.replayEventKey = `${record && record.source ? record.source : "unknown"}:${identity}`;
  item.dataset.replayIndex = String(index + 1);
  item.textContent = `${index + 1}. ${semanticEventLabel(event)}`;
  list.append(item);
}

function createTimelineReplayController(card) {
  const start = card.querySelector(".timeline-replay-start");
  const pause = card.querySelector(".timeline-replay-pause");
  const panel = card.querySelector(".timeline-replay");
  const list = card.querySelector(".timeline-replay-events");
  const status = card.querySelector(".timeline-replay-status");
  let records = [];
  let timer = null;
  let running = false;
  let index = 0;
  const clearTimer = () => {
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
  };
  const setStatus = text => {
    if (status) status.textContent = text;
  };
  const finish = () => {
    clearTimer();
    running = false;
    if (pause) pause.hidden = true;
    if (start) start.disabled = records.length === 0;
    if (records.length > 0) {
      setStatus(`Replay complete · ${records.length} event(s) / 回放完成 · ${records.length} 个事件`);
    }
  };
  const step = () => {
    if (!running) return;
    if (index >= records.length) {
      finish();
      return;
    }
    if (!list) {
      finish();
      setStatus("Replay surface unavailable / 回放界面不可用");
      return;
    }
    appendTimelineReplayEvent(list, records[index], index);
    index += 1;
    setStatus(`Replaying ${index}/${records.length} / 正在回放 ${index}/${records.length}`);
    timer = window.setTimeout(step, TIMELINE_REPLAY_STEP_MS);
  };
  const begin = () => {
    clearTimer();
    if (records.length === 0) {
      if (panel) panel.hidden = false;
      setStatus("No authenticated events are available yet / 当前尚无可回放的认证事件");
      return;
    }
    running = true;
    index = 0;
    if (panel) panel.hidden = false;
    if (list) list.replaceChildren();
    if (pause) pause.hidden = false;
    if (start) start.disabled = true;
    setStatus(`Replay started · ${records.length} event(s) / 回放开始 · ${records.length} 个事件`);
    step();
  };
  const stop = () => {
    clearTimer();
    running = false;
    if (pause) pause.hidden = true;
    if (start) start.disabled = records.length === 0;
    setStatus(`Replay paused at ${index}/${records.length} / 回放已暂停于 ${index}/${records.length}`);
  };
  if (start) start.addEventListener("click", begin);
  if (pause) pause.addEventListener("click", stop);
  return {
    setRecords(next) {
      if (running) return;
      records = Array.isArray(next) ? next.slice(-TIMELINE_REPLAY_MAX_EVENTS) : [];
      if (start) start.disabled = records.length === 0;
      if (records.length === 0) setStatus("Waiting for authenticated events / 等待认证事件");
      else setStatus(`${records.length} event(s) ready for read-only replay / ${records.length} 个事件可只读回放`);
    },
    stop,
  };
}

function renderLiveRaid(card, value) {
  card.querySelector(".live-phase").textContent = `Phase / 阶段: ${semanticPhaseLabel(paperRoomPhase(value))}`;
  const participantList = card.querySelector(".live-participants");
  const archives = Array.isArray(value && value.nakama_archives) ? value.nakama_archives : [];
  const activeArchives = archives.filter(entry => entry.archive && entry.archive.status !== "completed");
  const presenceArchives = activeArchives.length > 0 ? activeArchives : archives.slice(-1);
  const participants = presenceArchives.flatMap(entry => Array.isArray(entry.archive && entry.archive.participants) ? entry.archive.participants : []);
  participantList.replaceChildren(...participants.map(participant => {
    const item = document.createElement("li");
    const connected = participant.connected === true;
    item.dataset.connected = String(connected);
    item.textContent = `${semanticRoleLabel(participant.role)} · ${connected ? "online / 在线" : "offline / 离线"} · ${participant.ready ? "ready / 就绪" : "not ready / 未就绪"}`;
    return item;
  }));
  if (participants.length === 0) {
    const item = document.createElement("li");
    item.textContent = "Research session has not materialized yet / 科研会话尚未成形";
    participantList.replaceChildren(item);
  }
  const eventList = card.querySelector(".live-events");
  const events = timelineEventRecords({ ...value, nakama_archives: archives });
  for (const record of events.slice(-12)) {
    const event = record.event;
    const identity = event.event_id || event.cursor || `${event.event_type || "event"}:${event.sequence || "0"}`;
    const key = `${record.source}:${identity}`;
    if (eventList.querySelector(`[data-event-key="${CSS.escape(key)}"]`)) continue;
    const item = document.createElement("li");
    item.dataset.eventKey = key;
    item.textContent = semanticEventLabel(event);
    eventList.append(item);
  }
  while (eventList.children.length > 12) eventList.firstElementChild.remove();
}

const STALE_AUTHORITY_RELOAD_DELAY_MS = 100;

function invalidateStalePlayerForms(card, connection, output) {
  if (new Set(["stale", "stale-pending-workflow"]).has(card.dataset.authorityState)) return;
  const paperId = card.dataset.paperId;
  const applyInvalidation = () => {
    card.dataset.authorityState = "stale";
    connection.dataset.state = "stale-authority";
    connection.textContent = "Authoritative state changed · reloading / 权威状态已变化，正在刷新";
    for (const form of document.querySelectorAll(`form[data-paper-id="${CSS.escape(paperId)}"]`)) {
      form.dataset.authorityState = "stale";
      for (const control of form.elements) control.disabled = true;
    }
    show(output, "A teammate changed the Raid. Old controls are disabled; loading the current objective and actions…", false);
    const reloadCurrentAuthority = () => {
      requestPaperReload(paperId, 0);
    };
    recordProductEvent("stale_ui_reload", { paperId }).finally(reloadCurrentAuthority);
    window.setTimeout(reloadCurrentAuthority, STALE_AUTHORITY_RELOAD_DELAY_MS);
  };
  const pendingWorkflow = requestStaleAuthorityRefresh(paperId, applyInvalidation);
  if (pendingWorkflow) {
    card.dataset.authorityState = "stale-pending-workflow";
    connection.dataset.state = "stale-authority-pending-workflow";
    connection.textContent = "Authoritative state changed · finishing the current action before reload / 权威状态已变化，当前操作完成后刷新";
  }
}

function createLiveRaidSync(card) {
  const paperId = card.dataset.paperId;
  const button = card.querySelector(".timeline-refresh");
  const connection = card.querySelector(".live-connection");
  const output = card.querySelector(".live-detail");
  const replay = createTimelineReplayController(card);
  const cursor = readLiveCursor(paperId);
  let nakamaSessions = new Map();
  let timer = null;
  let running = false;
  let failures = 0;
  let wasDisconnected = false;
  let authoritySynchronized = false;
  let synchronizedPhase = null;
  const schedule = delay => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => sync.run(false), delay);
  };
  const sync = {
    async run(manual) {
      if (running || (!manual && document.visibilityState === "hidden")) {
        schedule(1250);
        return;
      }
      running = true;
      if (manual) button.disabled = true;
      try {
        const discoveryQuery = new URLSearchParams({
          after_cursor: String(cursor.hepta),
          after_sequence: "0"
        });
        const value = await fetchTimelineValue(paperId, discoveryQuery);
        const currentPhase = paperRoomPhase(value);
        const newHeptaEvents = Array.isArray(value && value.hepta_events)
          ? value.hepta_events.length
          : 0;
        if (authoritySynchronized && (newHeptaEvents > 0 || currentPhase !== synchronizedPhase)) {
          invalidateStalePlayerForms(card, connection, output);
          return;
        }
        const nextHepta = nextHeptaCursor(value.hepta_events, cursor.hepta);
        const discoveredSessions = reconcileNakamaSessions(value.research_sessions, nakamaSessions);
        const archiveResults = await Promise.all(Array.from(discoveredSessions, async ([sessionId, state]) => {
          const archiveQuery = new URLSearchParams({
            after_cursor: String(nextHepta),
            after_sequence: String(state.sequence),
            logical_session_id: sessionId,
            expected_roster_version: String(state.rosterVersion)
          });
          const page = await fetchTimelineValue(paperId, archiveQuery);
          if (!Array.isArray(page.nakama_archives) || page.nakama_archives.length !== 1) {
            throw new Error("invalid_nakama_archive_page");
          }
          const entry = page.nakama_archives[0];
          if (entry.logical_session_id !== sessionId) throw new Error("invalid_nakama_archive_session");
          return [sessionId, acceptNakamaArchive(entry, state), entry];
        }));
        const nextSessions = new Map();
        const archives = [];
        for (const [sessionId, state, entry] of archiveResults) {
          nextSessions.set(sessionId, state);
          archives.push(entry);
        }
        value.nakama_archives = archives;
        renderLiveRaid(card, value);
        replay.setRecords(timelineEventRecords(value));
        authoritySynchronized = true;
        synchronizedPhase = currentPhase;
        cursor.hepta = nextHepta;
        nakamaSessions = nextSessions;
        sessionStorage.setItem(liveCursorKey(paperId), JSON.stringify({ hepta: cursor.hepta }));
        const catchingUp = Array.from(nakamaSessions.values()).some(state => state.hasMore);
        connection.dataset.state = catchingUp ? "catching-up" : "live";
        connection.textContent = catchingUp
          ? "Catching up / 正在补齐"
          : "Live · synced / 实时已同步";
        if (wasDisconnected) {
          try { await recordProductEvent("reconnected", { paperId }); } catch (_) {}
          wasDisconnected = false;
        }
        failures = 0;
        const sequences = Array.from(nakamaSessions.values(), state => state.sequence);
        const maxSequence = sequences.length > 0 ? Math.max(...sequences) : 0;
        show(output, `cursor ${cursor.hepta} · ${sequences.length} session(s) · max sequence ${maxSequence}`, true);
        schedule(catchingUp ? 25 : LIVE_AUTHORITY_POLL_MS);
      } catch (error) {
        failures += 1;
        wasDisconnected = true;
        connection.dataset.state = "reconnecting";
        connection.textContent = "Reconnecting / 正在重连";
        show(output, error.message, false);
        schedule(liveAuthorityRetryDelayMs(failures));
      } finally {
        running = false;
        button.disabled = false;
      }
    }
  };
  button.addEventListener("click", async () => {
    sync.run(true);
  });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") sync.run(true);
  });
  return sync;
}

function bindTimeline() {
  for (const card of document.querySelectorAll(".live-raid")) {
    const sync = createLiveRaidSync(card);
    sync.run(true);
  }
}

function bindProductTelemetry() {
  for (const link of document.querySelectorAll(".continue-raid-link")) {
    link.addEventListener("click", async event => {
      event.preventDefault();
      const href = link.href;
      try {
        await recordProductEvent("continue_opened", {
          teamId: link.dataset.teamId || null,
          paperId: link.dataset.paperId || null
        });
      } catch (_) {
        // Telemetry must never block the player's authoritative Continue path.
      }
      window.location.assign(href);
    });
  }
  const paperMatch = window.location.pathname.match(
    /^\/league\/(?:papers|review)\/([0-9a-f-]{36})$/i
  );
  if (window.location.pathname === "/league") {
    const challengeId = new URL(window.location.href).searchParams.get("rematch_challenge");
    if (challengeId && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(challengeId)) {
      const form = document.querySelector(`form.queue-form[data-challenge-id="${CSS.escape(challengeId)}"]`);
      if (form) {
        form.classList.add("rematch-target");
        const output = form.querySelector("output");
        renderPlayerMessage(output, "Challenge selected from your last Raid. Review the role and join when ready. / 已从上一局选中挑战；确认角色后再加入。");
        window.requestAnimationFrame(() => {
          form.scrollIntoView({ behavior: "smooth", block: "center" });
          const first = form.querySelector("button.queue-submit:not([disabled])") || form.querySelector("input,select,button");
          if (first instanceof HTMLElement) first.focus({ preventScroll: true });
        });
      }
    }
  }
  const navigation = performance.getEntriesByType("navigation")[0];
  if (paperMatch && navigation && navigation.type === "reload") {
    recordProductEvent("reconnected", { paperId: paperMatch[1] }).catch(() => {});
  }
}

function paperRoomFocusTarget(action) {
  if (!action) return null;
  return action.querySelector(
    "input:not([type=hidden]):not([disabled]), select:not([disabled]), textarea:not([disabled])"
  ) || action.querySelector(
    "button[type=submit]:not([disabled]), a.button[href], button:not([disabled])"
  ) || (action.matches("[tabindex]") ? action : null);
}

function bindPaperRoomProgressiveDisclosure() {
  for (const button of document.querySelectorAll("[data-paper-room-reveal]")) {
    const controlledId = button.getAttribute("aria-controls");
    const advanced = controlledId ? document.getElementById(controlledId) : null;
    if (!(advanced instanceof HTMLDetailsElement)) {
      button.disabled = true;
      continue;
    }
    const updateExpandedState = () => {
      button.setAttribute("aria-expanded", advanced.open ? "true" : "false");
    };
    advanced.addEventListener("toggle", updateExpandedState);
    updateExpandedState();
    button.addEventListener("click", () => {
      advanced.open = true;
      updateExpandedState();
      const targetSelector = button.dataset.paperRoomPrimaryTarget;
      const matchingAction = targetSelector ? advanced.querySelector(targetSelector) : null;
      const primaryControls = Array.from(advanced.querySelectorAll(".primary-action"));
      const target = paperRoomFocusTarget(matchingAction)
        || primaryControls.map(paperRoomFocusTarget).find(Boolean)
        || advanced.querySelector("summary");
      window.requestAnimationFrame(() => {
        if (!(target instanceof HTMLElement)) return;
        target.focus({ preventScroll: true });
        if (typeof target.scrollIntoView === "function") {
          const reduceMotion = window.matchMedia
            && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
          target.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "center" });
        }
      });
    });
  }
}

const CHALLENGE_MATERIALS_SCHEMA = "hepta.paper_raid.bff.challenge_materials.v1";
const CHALLENGE_MATERIAL_OBJECTS = Object.freeze({
  brief: Object.freeze({ role: "playable_brief", label: "Brief / 任务简报" }),
  dataset: Object.freeze({ role: "dataset", label: "Dataset / 数据集" }),
  baseline: Object.freeze({ role: "baseline_code", label: "Baseline / 基线" }),
  evaluator: Object.freeze({ role: "frozen_evaluator", label: "Evaluator / 评估器" }),
});
const CHALLENGE_MATERIAL_OBJECT_KEYS = Object.freeze([
  "object_key", "logical_path", "role", "digest", "size_bytes", "media_type", "download_path",
]);

function challengeMaterialLogicalPath(value) {
  if (typeof value !== "string" || value.length < 1 || value.length > 256 ||
      value.startsWith("/") || value.includes("\\") || value.includes("//") ||
      value.split("/").some(part => part === "" || part === "." || part === "..") ||
      !/^[A-Za-z0-9][A-Za-z0-9._/-]*$/.test(value)) {
    throw new Error("challenge_material_logical_path_is_invalid");
  }
  return value;
}

function challengeMaterialMediaType(value) {
  if (typeof value !== "string" || value.length < 1 || value.length > 128 ||
      value.trim() !== value || /[\u0000-\u001f\u007f]/u.test(value) || !/^[\x20-\x7e]+$/.test(value)) {
    throw new Error("challenge_material_media_type_is_invalid");
  }
  return value;
}

async function validateChallengeMaterialProjection(value, paperId) {
  const expectedPaperId = canonicalUuid(paperId, "paper_id");
  if (!exactKeys(value, [
    "schema", "paper_project_id", "challenge_ruleset_snapshot_hash",
    "material_authority", "objects", "projection_hash",
  ]) || value.schema !== CHALLENGE_MATERIALS_SCHEMA ||
      value.paper_project_id !== expectedPaperId ||
      !value.material_authority || typeof value.material_authority !== "object" ||
      Array.isArray(value.material_authority) || !Array.isArray(value.objects)) {
    throw new Error("challenge_material_projection_is_invalid");
  }
  const snapshotHash = canonicalDigest(
    value.challenge_ruleset_snapshot_hash,
    "challenge_ruleset_snapshot_hash",
  );
  if (value.challenge_ruleset_snapshot_hash !== snapshotHash) {
    throw new Error("challenge_ruleset_snapshot_hash_is_not_canonical");
  }
  const projectionHash = canonicalDigest(value.projection_hash, "projection_hash");
  if (value.projection_hash !== projectionHash || value.objects.length !== 4) {
    throw new Error("challenge_material_projection_is_not_exactly_four_objects");
  }
  const seenKeys = new Set();
  const seenRoles = new Set();
  const objects = value.objects.map(object => {
    if (!exactKeys(object, CHALLENGE_MATERIAL_OBJECT_KEYS) ||
        typeof object.object_key !== "string" || !Object.hasOwn(CHALLENGE_MATERIAL_OBJECTS, object.object_key) ||
        seenKeys.has(object.object_key) || seenRoles.has(object.role)) {
      throw new Error("challenge_material_object_descriptor_is_invalid");
    }
    const expected = CHALLENGE_MATERIAL_OBJECTS[object.object_key];
    if (object.role !== expected.role) throw new Error("challenge_material_object_role_is_invalid");
    seenKeys.add(object.object_key);
    seenRoles.add(object.role);
    const digest = canonicalDigest(object.digest, `${object.object_key}_digest`);
    if (object.digest !== digest || !Number.isSafeInteger(object.size_bytes) || object.size_bytes < 1 ||
        object.size_bytes > 32 * 1024 * 1024) {
      throw new Error("challenge_material_object_size_or_digest_is_invalid");
    }
    challengeMaterialLogicalPath(object.logical_path);
    challengeMaterialMediaType(object.media_type);
    const expectedDownloadPath = `/api/papers/${expectedPaperId}/challenge-materials/${object.object_key}`;
    if (object.download_path !== expectedDownloadPath) {
      throw new Error("challenge_material_download_path_is_not_server_projected");
    }
    return object;
  });
  if (!Object.keys(CHALLENGE_MATERIAL_OBJECTS).every(key => seenKeys.has(key))) {
    throw new Error("challenge_material_projection_is_missing_a_required_role");
  }
  const frame = { ...value };
  delete frame.projection_hash;
  const computedHash = await sha256Label(new TextEncoder().encode(canonicalJson(frame)));
  if (computedHash !== projectionHash) throw new Error("challenge_material_projection_hash_mismatch");
  return { paperId: expectedPaperId, projectionHash, objects };
}

function challengeMaterialUnavailable(panel) {
  const state = panel.querySelector("[data-challenge-materials-state]");
  const list = panel.querySelector("[data-challenge-materials-list]");
  if (list) {
    list.replaceChildren();
    list.hidden = true;
  }
  if (state) {
    state.dataset.state = "unavailable";
    state.textContent = "Frozen challenge materials unavailable. No manual authority file selection is allowed. / 冻结挑战工件不可用；不允许手工选择权威文件。";
  }
}

function renderChallengeMaterials(panel, projection) {
  const state = panel.querySelector("[data-challenge-materials-state]");
  const list = panel.querySelector("[data-challenge-materials-list]");
  if (!state || !list) throw new Error("challenge_materials_panel_is_incomplete");
  list.replaceChildren();
  for (const object of projection.objects) {
    const item = document.createElement("li");
    const details = document.createElement("div");
    const title = document.createElement("strong");
    title.textContent = CHALLENGE_MATERIAL_OBJECTS[object.object_key].label;
    const metadata = document.createElement("small");
    metadata.textContent = `${object.logical_path} · ${object.media_type} · ${object.size_bytes} bytes`;
    details.append(title, metadata);
    const link = document.createElement("a");
    link.className = "button challenge-materials-download";
    link.href = `/api/papers/${encodeURIComponent(projection.paperId)}/challenge-materials/${encodeURIComponent(object.object_key)}?digest=${encodeURIComponent(object.digest)}&projection_hash=${encodeURIComponent(projection.projectionHash)}`;
    link.textContent = "Download / 下载";
    link.setAttribute("download", "");
    link.dataset.challengeMaterialObject = object.object_key;
    item.append(details, link);
    list.append(item);
  }
  state.dataset.state = "available";
  state.textContent = "Frozen snapshot loaded. Four server-selected materials are ready. / 冻结快照已加载；四项服务器选定工件已就绪。";
  list.hidden = false;
}

async function loadChallengeMaterials(panel) {
  const paperId = canonicalUuid(panel.dataset.paperId, "paper_id");
  const response = await fetch(`/api/papers/${encodeURIComponent(paperId)}/challenge-materials`, {
    method: "GET",
    credentials: "same-origin",
    headers: { "accept": "application/json" },
  });
  const value = await responseValue(response);
  if (!response.ok) throw new Error("challenge_material_projection_unavailable");
  const projection = await validateChallengeMaterialProjection(value, paperId);
  renderChallengeMaterials(panel, projection);
}

function bindChallengeMaterials() {
  for (const panel of document.querySelectorAll("[data-challenge-materials]")) {
    challengeMaterialUnavailable(panel);
    const state = panel.querySelector("[data-challenge-materials-state]");
    if (state) {
      state.dataset.state = "loading";
      state.textContent = "Loading frozen materials… / 正在加载冻结工件……";
    }
    loadChallengeMaterials(panel).catch(() => challengeMaterialUnavailable(panel));
  }
}

const PRACTICE_CHOICES = Object.freeze({
  captain_plan: new Set(["audit_highest_risk_claim", "audit_evidence_chain_first"]),
  evidence_assessment: new Set(["unsupported_claim", "citation_mismatch", "evidence_sufficient"]),
  experiment_interpretation: new Set(["revise_claim", "request_more_evidence", "retain_claim_with_caveat"]),
  captain_aar: new Set(["improve_evidence_triage", "improve_experiment_design", "improve_team_coordination"]),
});

function practiceVersion(form) {
  return positiveInteger(form.dataset.practiceVersion, "practice_version");
}

function practiceAdvancePayload(form) {
  const action = String(form.dataset.practiceAction || "");
  const choices = PRACTICE_CHOICES[action];
  const selected = form.elements.choice && String(form.elements.choice.value || "");
  if (!choices || !choices.has(selected)) throw new Error("invalid_request");
  return {
    expected_version: practiceVersion(form),
    action: { action, choice: selected },
  };
}

async function submitPracticeMutation(form, url, body, successMessage) {
  const controls = form.querySelectorAll("button, input, select, textarea");
  const output = form.querySelector("output");
  for (const control of controls) control.disabled = true;
  try {
    const response = await mutation(url, {
      method: "POST",
      headers: { "content-type": "application/json", "accept": "application/json" },
      body: JSON.stringify(body),
    });
    const value = await responseValue(response);
    show(output, response.ok ? successMessage : value, response.ok);
    if (response.ok) window.setTimeout(() => window.location.reload(), 250);
  } catch (error) {
    show(output, error.message, false);
  } finally {
    for (const control of controls) control.disabled = false;
  }
}

function bindPractice() {
  for (const form of document.querySelectorAll(".practice-start-form")) {
    form.addEventListener("submit", event => {
      event.preventDefault();
      submitPracticeMutation(
        form,
        "/api/practice/start",
        {},
        "Practice ready. Loading your first role… / 练习已就绪，正在加载第一个角色……",
      );
    });
  }
  for (const form of document.querySelectorAll(".practice-advance-form")) {
    form.addEventListener("submit", event => {
      event.preventDefault();
      try {
        submitPracticeMutation(
          form,
          "/api/practice/advance",
          practiceAdvancePayload(form),
          "Choice saved. Loading the next role… / 选择已保存，正在加载下一角色……",
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }
  for (const form of document.querySelectorAll(".practice-abandon-form")) {
    form.addEventListener("submit", event => {
      event.preventDefault();
      submitPracticeMutation(
        form,
        "/api/practice/abandon",
        { expected_version: practiceVersion(form) },
        "Practice left. Returning to the local summary… / 已退出练习，正在返回本地摘要……",
      );
    });
  }
}

const QUICK_RAID_CHOICES = Object.freeze({
  review_evidence: new Set(["flag_citation_gap", "accept_as_sufficient"]),
  run_experiment: new Set(["recheck_baseline", "run_candidate"]),
  publish_paper: new Set(["revise_claim", "retain_with_caveat"]),
});

function quickRaidVersion(form) {
  return positiveInteger(form.dataset.quickRaidVersion, "quick_raid_version");
}

function quickRaidActionPayload(form) {
  const action = String(form.dataset.quickRaidAction || "");
  const choices = QUICK_RAID_CHOICES[action];
  const selected = form.elements.choice && String(form.elements.choice.value || "");
  if (!choices || !choices.has(selected)) throw new Error("invalid_request");
  const actionName = action === "review_evidence"
    ? "review_evidence"
    : action === "run_experiment"
      ? "run_experiment"
      : "publish_paper";
  return {
    expected_version: quickRaidVersion(form),
    action: actionName === "review_evidence"
      ? { action: "review_evidence", choice: selected }
      : actionName === "run_experiment"
        ? { action: "run_experiment", choice: selected }
        : { action: "publish_paper", conclusion: selected },
  };
}

function bindQuickRaid() {
  if (window.location.pathname !== "/league/quick-raid") return;
  for (const form of document.querySelectorAll(".quick-raid-start-form")) {
    form.addEventListener("submit", event => {
      event.preventDefault();
      submitPracticeMutation(
        form,
        "/api/quick-raid/start",
        {},
        "Quick Raid ready. Loading the first card… / 快速远征已就绪，正在加载第一张卡……",
      );
    });
  }
  for (const form of document.querySelectorAll(".quick-raid-action-form")) {
    form.addEventListener("submit", event => {
      event.preventDefault();
      try {
        submitPracticeMutation(
          form,
          "/api/quick-raid/action",
          quickRaidActionPayload(form),
          "Saved. Loading the next Quick Raid step… / 已保存，正在加载下一步……",
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  bindPlayerFocusContext();
  bindPaperRoomProgressiveDisclosure();
  bindChallengeMaterials();
  bindPractice();
  bindQuickRaid();
  bindLogin();
  bindHumanKeyCreate();
  bindHumanKeyRegistration();
  bindHumanKeyImport();
  bindAgentPairing();
  bindAgentBinding();
  bindAgentRotation();
  bindLocalSigning();
  bindQueueForms();
  bindLobbyQueueFreshness();
  bindProposalCountdowns();
  bindChallengeCountdowns();
  bindChallengeOutcomeForms();
  bindReviewQueue();
  bindReviewRaid();
  bindAuthorRework();
  bindAuthorAppeal();
  bindProposalForms();
  bindFirstPlayableFormation();
  bindGuidedPaperActions();
  bindCommandForms();
  bindArtifactForms();
  bindReviewAuthority();
  bindPaperReworkCountdowns();
  bindTimeline();
  bindProductTelemetry();
  const focusRestored = restorePlayerFocusContext();
  if (!focusRestored) focusProgressedPracticeAction();
  // Binding player controls must not depend on a network round trip.  A slow
  // CSRF refresh previously left native forms briefly active without their
  // fail-closed JavaScript handlers, so a real player click could submit and
  // navigate away before the listener existed.  Mutations already refresh a
  // missing token lazily; publish one explicit readiness contract after every
  // control is synchronously bound, then warm the token in the background.
  document.documentElement.dataset.paperRaidBindingsReady = "true";
  if (document.body.dataset.authenticated === "true") {
    try { await refreshCsrf(); } catch (_) { return; }
  }
});
