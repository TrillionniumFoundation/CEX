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

function show(output, value, ok = true) {
  if (output) {
    output.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
    output.classList.toggle("result-ok", ok);
    output.classList.toggle("result-error", !ok);
  }
  const toast = document.querySelector("#toast");
  if (toast) {
    toast.textContent = typeof value === "string" ? value : JSON.stringify(value);
    toast.hidden = false;
    window.setTimeout(() => { toast.hidden = true; }, 5000);
  }
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

function shellQuote(value) {
  return `'${String(value).replaceAll("'", `'"'"'`)}'`;
}

function bridgeProposalCommand(dataset, values) {
  const sectionKey = String(dataset.sectionKey || "").trim();
  if (!LOGICAL_SESSION_PATTERN.test(sectionKey)) throw new Error("section_key_must_be_a_safe_logical_identifier");
  const proposalKind = String(values.proposalKind || "");
  if (!new Set(["proposal", "delivery"]).has(proposalKind)) throw new Error("invalid_agent_proposal_kind");
  const fields = {
    paperId: canonicalUuid(dataset.paperId, "paper_id"),
    workItemId: canonicalUuid(values.workItemId, "work_item_id"),
    parentRevisionId: canonicalUuid(dataset.parentRevisionId, "parent_revision_id"),
    artifactManifestId: canonicalUuid(values.artifactManifestId, "artifact_manifest_id"),
    artifactManifestHash: canonicalDigest(values.artifactManifestHash, "artifact_manifest_hash"),
    payloadHash: canonicalDigest(values.payloadHash, "payload_hash")
  };
  canonicalUuid(dataset.bindingId, "binding_id");
  if (!String(dataset.agentId || "").trim()) throw new Error("agent_id_is_required");
  return [
    "node tools/paper-raid-agent-bridge/src/cli.mjs submit-proposal",
    "  --config paper-raid-agent-bridge.local.json",
    `  --paper-id ${shellQuote(fields.paperId)}`,
    `  --work-item-id ${shellQuote(fields.workItemId)}`,
    `  --section-key ${shellQuote(sectionKey)}`,
    `  --parent-revision-id ${shellQuote(fields.parentRevisionId)}`,
    `  --proposal-kind ${shellQuote(proposalKind)}`,
    `  --payload-hash ${shellQuote(fields.payloadHash)}`,
    `  --artifact-manifest-id ${shellQuote(fields.artifactManifestId)}`,
    `  --artifact-manifest-hash ${shellQuote(fields.artifactManifestHash)}`
  ].join(" \\\n");
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
  if (DIGEST_PATTERN.test(clean.toLowerCase())) return clean.toLowerCase();
  return sha256Label(new TextEncoder().encode(clean));
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
  return {
    run_record_id: uuid(),
    experiment_plan_id: values.experimentPlanId,
    status,
    seed: safeInteger(values.seed, "seed"),
    parameters_hash: await semanticDigest(values.parameters, "parameters"),
    logs_manifest_id: values.logsManifestId,
    outputs_manifest_id: succeeded ? values.outputsManifestId : null,
    metrics_hash: succeeded ? await semanticDigest(values.metrics, "metrics") : null,
    failure_hash: succeeded ? null : await semanticDigest(values.failure, "failure")
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
    return {
      player_id: playerId,
      credit_roles: normalizedCreditRoles(author.credit_roles),
      accepted_artifact_manifest_ids: [],
      accepted_section_review_ids: []
    };
  }).sort((left, right) => left.player_id < right.player_id ? -1 : left.player_id > right.player_id ? 1 : 0);
  const contributionLedgerId = await uuidV5(
    CONTRIBUTION_LEDGER_NAMESPACE,
    `${CONTRIBUTION_LEDGER_SCHEMA}\n${canonicalPaperId}\n${canonicalRevisionId}`
  );
  const frozenEntries = requestEntries.map(entry => ({ ...entry, contribution_points: 0 }));
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
    )
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
      )
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

async function uploadCasArtifact(paperId, file, mediaType) {
  if (!file || file.size < 1 || file.size > 32 * 1024 * 1024) {
    throw new Error("artifact_size_must_be_1_to_33554432_bytes");
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
    const stored = await uploadCasArtifact(values.paperId, item.file, item.mediaType);
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
  const payload = {
    manifest_id: uuid(),
    expected_paper_version: positiveInteger(values.paperVersion, "paper_version"),
    expected_source_manifest_sha256: await neutralBundleRawSha256(sourceBundle),
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
  return result;
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
    let visibleGrantId = null;
    let clearTimer = null;

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
        return;
      }
      const grantId = canonicalUuid(grant.grant_id, "grant_id");
      const expiresAt = Date.parse(grant.expires_at);
      if (!["issued", "pinned", "consumed", "revoked"].includes(grant.state) ||
          Number.isNaN(expiresAt)) {
        throw new Error("agent_pairing_grant_is_invalid");
      }
      const expired = expiresAt <= Date.now();
      revokeButton.dataset.grantId = grantId;
      revokeButton.hidden = !(grant.state === "issued" || (grant.state === "pinned" && expired));
      show(statusOutput, {
        state: expired && ["issued", "pinned"].includes(grant.state) ? "expired" : grant.state,
        expires_at: grant.expires_at,
        binding_id: grant.binding_id || null,
        agent_id: grant.agent_id || null,
        agent_key_id: grant.agent_key_id || null,
        assurance: grant.binding_id ? "self_declared_unverified" : null
      }, true);
    };

    const refreshStatus = async () => {
      const response = await fetch("/api/agent-bridge/pairing-grants", {
        method: "GET",
        credentials: "same-origin",
        headers: { "accept": "application/json" }
      });
      const value = await responseValue(response);
      if (!response.ok) throw new Error(value && value.error ? value.error : "agent_pairing_status_failed");
      renderStatus(value);
    };

    if (form) form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      button.disabled = true;
      clearVisibleCode();
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
        const response = await mutation(`/api/agent-bridge/pairing-grants/${grantId}/revoke`, {
          method: "POST",
          headers: { "content-type": "application/json", "accept": "application/json" },
          body: "{}"
        });
        const value = await responseValue(response);
        if (!response.ok) throw new Error(value && value.error ? value.error : "agent_pairing_revoke_failed");
        clearVisibleCode();
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
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
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

async function evaluationDraftPayload(form) {
  const values = form.elements;
  const metric = String(values.metric_key.value || "").trim();
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(metric)) {
    throw new Error("metric_key_must_be_a_safe_logical_identifier");
  }
  const preset = String(values.tolerance_preset.value || "");
  let rule;
  if (preset === "absolute") {
    rule = {
      kind: "absolute",
      metric,
      max_delta_micros: nonNegativeInteger(values.absolute_delta.value, "absolute_delta")
    };
  } else if (preset === "relative") {
    rule = {
      kind: "relative",
      metric,
      max_delta_bps: boundedInteger(values.relative_delta.value, "relative_delta", 10000)
    };
  } else if (preset === "statistical") {
    rule = {
      kind: "statistical",
      metric,
      minimum_interval_overlap_bps: boundedInteger(
        values.interval_overlap.value,
        "interval_overlap",
        10000
      ),
      maximum_effect_delta_micros: nonNegativeInteger(values.effect_delta.value, "effect_delta"),
      minimum_p_value_micros: boundedInteger(values.p_value.value, "p_value", 1000000)
    };
  } else {
    throw new Error("unsupported_tolerance_preset");
  }
  return {
    evaluation_id: uuid(),
    supersedes_evaluation_id: null,
    tolerance_policy: {
      schema: "hepta.paper_raid.tolerance_policy.v1",
      version: "1",
      rules: [rule]
    },
    reference_metrics_micros: {
      [metric]: safeInteger(values.reference_metric.value, "reference_metric")
    },
    score_components: {
      method_rigor_bps: boundedInteger(values.method_rigor_bps.value, "method_rigor_bps", 2500),
      experiment_statistics_bps: boundedInteger(
        values.experiment_statistics_bps.value,
        "experiment_statistics_bps",
        1500
      ),
      reproducibility_bps: boundedInteger(values.reproducibility_bps.value, "reproducibility_bps", 1500),
      evidence_citations_bps: boundedInteger(values.evidence_citations_bps.value, "evidence_citations_bps", 1500),
      value_originality_bps: boundedInteger(values.value_originality_bps.value, "value_originality_bps", 1500),
      argument_expression_bps: boundedInteger(values.argument_expression_bps.value, "argument_expression_bps", 1000),
      ethics_transparency_bps: boundedInteger(values.ethics_transparency_bps.value, "ethics_transparency_bps", 500)
    },
    hard_gates: {
      citations_and_data_authentic: values.citations_and_data_authentic.checked,
      failed_runs_disclosed: values.failed_runs_disclosed.checked,
      all_authors_consented: values.all_authors_consented.checked,
      core_claims_have_evidence: values.core_claims_have_evidence.checked,
      artifact_lineage_complete: values.artifact_lineage_complete.checked,
      license_ethics_coi_complete: values.license_ethics_coi_complete.checked
    },
    evaluator_coi_attestation_hash: await plainTextDigest(
      values.coi_statement.value,
      "evaluator_coi_statement"
    )
  };
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

async function reproductionPayload(form) {
  const observed = {};
  for (const input of form.querySelectorAll(".observed-metric")) {
    const metric = String(input.dataset.metric || "");
    if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(metric) || metric in observed) {
      throw new Error("invalid_or_duplicate_observed_metric");
    }
    observed[metric] = safeInteger(input.value, `observed_${metric}`);
  }
  if (Object.keys(observed).length < 1) throw new Error("observed_metrics_are_required");
  const statisticalEvidence = {};
  for (const fieldset of form.querySelectorAll(".statistical-metric")) {
    const metric = String(fieldset.dataset.metric || "");
    if (!(metric in observed) || metric in statisticalEvidence) {
      throw new Error("invalid_or_duplicate_statistical_metric");
    }
    statisticalEvidence[metric] = {
      interval_overlap_bps: boundedInteger(
        fieldset.elements.interval_overlap_bps.value,
        `interval_overlap_${metric}`,
        10000
      ),
      effect_delta_micros: safeInteger(
        fieldset.elements.effect_delta_micros.value,
        `effect_delta_${metric}`
      ),
      p_value_micros: boundedInteger(
        fieldset.elements.p_value_micros.value,
        `p_value_${metric}`,
        1000000
      )
    };
  }
  return {
    reproduction_id: uuid(),
    supersedes_reproduction_id: null,
    observed_metrics_micros: observed,
    statistical_evidence: statisticalEvidence,
    seed_set_hash: await plainTextDigest(form.elements.seed_statement.value, "seed_statement"),
    environment_hash: await plainTextDigest(
      form.elements.environment_statement.value,
      "environment_statement"
    ),
    run_manifest_hash: await plainTextDigest(
      form.elements.run_manifest_statement.value,
      "run_manifest_statement"
    ),
    coi_attestation_hash: await plainTextDigest(
      form.elements.coi_statement.value,
      "reproducer_coi_statement"
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
    for (const button of buttons) button.disabled = false;
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
    for (const button of buttons) button.disabled = false;
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
    for (const button of buttons) button.disabled = false;
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
      button.disabled = true;
      try {
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
        window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }

  for (const form of document.querySelectorAll(".draft-manifest-wizard-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
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
        window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
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
      try {
        const payload = evidenceVerificationPayload({
          sourceUri: form.elements.source_uri.value,
          sourceHash: form.elements.source_hash.value,
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
        show(form.querySelector("output"), error.message, false);
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

  for (const form of document.querySelectorAll(".bridge-proposal-task-form")) {
    const button = form.querySelector(".copy-bridge-proposal");
    button.addEventListener("click", async () => {
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        await requireActiveAgentBinding(form.dataset.bindingId);
        const artifactManifest = form.elements.artifact_manifest_id.selectedOptions[0];
        if (!artifactManifest) throw new Error("artifact_manifest_is_required");
        const command = bridgeProposalCommand(form.dataset, {
          workItemId: form.elements.work_item_id.value,
          artifactManifestId: artifactManifest.value,
          artifactManifestHash: artifactManifest.dataset.artifactManifestHash,
          proposalKind: form.elements.proposal_kind.value,
          payloadHash: form.elements.payload_hash.value
        });
        if (!navigator.clipboard || typeof navigator.clipboard.writeText !== "function") {
          throw new Error("clipboard_unavailable_copy_the_rendered_command_manually");
        }
        await navigator.clipboard.writeText(command);
        show(output, `${command}\n\nCopied. Run on the Agent host; no signature returns to this textarea.`, true);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
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
      for (const button of buttons) button.disabled = true;
      let promoteAttempted = false;
      try {
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
          ethics_disclosure_hash: canonicalDigest(
            form.elements.ethics_disclosure_hash.value,
            "ethics_disclosure_hash"
          ),
          coi_disclosure_hash: canonicalDigest(
            form.elements.coi_disclosure_hash.value,
            "coi_disclosure_hash"
          ),
          contribution_ledger_hash: budget.ledgerHash,
          ai_disclosure_hash: canonicalDigest(
            form.elements.ai_disclosure_hash.value,
            "ai_disclosure_hash"
          ),
          license: form.elements.license.value.trim(),
          authors
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
        window.setTimeout(() => window.location.reload(), 450);
      } catch (error) {
        show(
          output,
          promoteAttempted
            ? `Authoritative state may have advanced; reloading into ledger repair: ${error.message}`
            : error.message,
          false
        );
        if (promoteAttempted) window.setTimeout(() => window.location.reload(), 900);
      } finally {
        for (const button of buttons) button.disabled = false;
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

function bindQueueForms() {
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
        if (roles.length < 1 || roles.length > 5 || new Set(roles).size !== roles.length) {
          throw new Error("roles_must_be_1_to_5_distinct_values");
        }
        if (authorizedRoles.length === 0 || roles.some(role => !authorizedRoles.includes(role))) {
          throw new Error("selected_role_is_not_authorized_for_this_identity");
        }
        const availability = new TextEncoder().encode(form.elements.availability.value);
        const payload = {
          ticket_id: uuid(),
          challenge_id: form.dataset.challengeId,
          requested_team_size: 3,
          roles,
          availability_hash: await sha256Label(availability)
        };
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
    text: "Grace elapsed · Captain may record expired / 宽限已结束，队长可登记超时",
  };
}

function bindChallengeCountdowns() {
  for (const element of document.querySelectorAll(".challenge-countdown")) {
    const panel = element.closest(".challenge-ruleset-panel");
    const expiredControl = panel && panel.querySelector(".expired-outcome-control");
    let timer = null;
    const update = () => {
      const clock = challengeClockState(
        element.dataset.deadlineAt,
        element.dataset.graceExpiresAt,
      );
      element.dataset.state = clock.state;
      element.textContent = clock.text;
      if (expiredControl) expiredControl.hidden = clock.state !== "expired";
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
    expired: new Set(["grace_window_elapsed"]),
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
  for (const form of document.querySelectorAll(".review-evaluation-draft-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        await submitSignedGuidedCommand(
          form,
          "create_paper_evaluation_draft",
          canonicalUuid(form.dataset.paperId, "paper_id"),
          null,
          await evaluationDraftPayload(form),
          "Immutable evaluation draft signed and frozen."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }
  for (const form of document.querySelectorAll(".review-attestation-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button) return;
      try {
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
      }
    });
  }
  for (const form of document.querySelectorAll(".review-finalize-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
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
      }
    });
  }
  for (const form of document.querySelectorAll(".review-reproduction-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        await submitSignedGuidedCommand(
          form,
          "submit_reproduction",
          canonicalUuid(form.dataset.paperId, "paper_id"),
          canonicalUuid(form.dataset.evaluationId, "evaluation_id"),
          await reproductionPayload(form),
          "Independent reproduction signed and recorded."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }
  for (const form of document.querySelectorAll(".review-appeal-resolution-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = event.submitter;
      if (!button || button.disabled) return;
      try {
        await submitDerivedSignedGuidedCommand(
          form,
          "resolve_appeal",
          await appealResolutionPayload(form, button.value),
          "Independent Appeal resolution signed and recorded."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
  }
}

function bindAuthorAppeal() {
  for (const form of document.querySelectorAll(".author-appeal-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      try {
        await submitDerivedSignedGuidedCommand(
          form,
          "submit_appeal",
          await appealPayload(form),
          "Appeal signed; scientific finality is now on hold."
        );
      } catch (error) {
        show(form.querySelector("output"), error.message, false);
      }
    });
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

async function fetchTimelineValue(paperId, query) {
  const response = await fetch(`/api/papers/${encodeURIComponent(paperId)}/timeline?${query}`, {
    credentials: "same-origin",
    headers: { "accept": "application/json" }
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
  reproducing: "Reproduce / 复现",
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
  const events = [
    ...(Array.isArray(value && value.hepta_events) ? value.hepta_events : []).map(event => ({
      source: "hepta",
      event,
    })),
    ...archives.flatMap(entry => (Array.isArray(entry.archive && entry.archive.events) ? entry.archive.events : []).map(event => ({
      source: `nakama:${entry.logical_session_id || "unknown"}`,
      event,
    })))
  ];
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

function createLiveRaidSync(card) {
  const paperId = card.dataset.paperId;
  const button = card.querySelector(".timeline-refresh");
  const connection = card.querySelector(".live-connection");
  const output = card.querySelector(".live-detail");
  const cursor = readLiveCursor(paperId);
  let nakamaSessions = new Map();
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
        schedule(catchingUp ? 25 : 1250);
      } catch (error) {
        failures += 1;
        wasDisconnected = true;
        connection.dataset.state = "reconnecting";
        connection.textContent = "Reconnecting / 正在重连";
        show(output, error.message, false);
        schedule(Math.min(5000, 1250 * (2 ** Math.min(failures, 2))));
      } finally {
        running = false;
        button.disabled = false;
      }
    }
  };
  button.addEventListener("click", async () => {
    try { await recordProductEvent("replay_started", { paperId }); } catch (_) {}
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
  const paperMatch = window.location.pathname.match(/^\/league\/papers\/([0-9a-f-]{36})$/i);
  const navigation = performance.getEntriesByType("navigation")[0];
  if (paperMatch && navigation && navigation.type === "reload") {
    recordProductEvent("reconnected", { paperId: paperMatch[1] }).catch(() => {});
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  bindLogin();
  if (document.body.dataset.authenticated === "true") {
    try { await refreshCsrf(); } catch (_) { return; }
  }
  bindHumanKeyCreate();
  bindHumanKeyRegistration();
  bindHumanKeyImport();
  bindAgentPairing();
  bindAgentBinding();
  bindAgentRotation();
  bindLocalSigning();
  bindQueueForms();
  bindProposalCountdowns();
  bindChallengeCountdowns();
  bindChallengeOutcomeForms();
  bindReviewQueue();
  bindReviewRaid();
  bindAuthorAppeal();
  bindProposalForms();
  bindFirstPlayableFormation();
  bindGuidedPaperActions();
  bindCommandForms();
  bindArtifactForms();
  bindTimeline();
  bindProductTelemetry();
});
