import { createHash, randomUUID } from "node:crypto";
import { dirname, join } from "node:path";
import {
  AGENT_BINDING_PROOF_SCHEMA,
  AGENT_PROPOSAL_SCHEMA,
  agentBindingProofFrame,
  agentCapabilityDisclosureHash,
  agentProposalFrame,
  assertCanonicalUuid,
  assertDigest,
  assertLogicalId,
  decodeCanonicalBase64,
  researchSessionActionFrame,
} from "./canonical.mjs";
import { AGENT_BRIDGE_ENDPOINTS, AgentBridgeClient } from "./client.mjs";
import {
  loadBridgeState,
  prepareBridgeStateForPairing,
  saveBridgeState,
} from "./state.mjs";
import {
  executeReviewTask,
  reviewObjectQuery,
  validateReviewReceiptRequest,
  validateReviewReceiptResult,
} from "./review.mjs";
import {
  clearReviewOutbox,
  loadReviewOutbox,
  saveReviewOutbox,
} from "./review_outbox.mjs";
import {
  PRACTICE_AUTO_RESULT_SCHEMA,
  PRACTICE_CLAIM_REQUEST_SCHEMA,
  PRACTICE_RESULT_REQUEST_SCHEMA,
  PRACTICE_TASK_QUERY_SCHEMA,
  fixedPracticeResult,
  validatePracticeTasks,
  validatePracticeTransitionResult,
} from "./practice.mjs";
import {
  authorWorkStartKey,
  challengeMaterialObjectQuery,
  deliveryCandidateForAuthorOutput,
  downloadAssignedChallengeMaterials,
  executeAuthorWorkStart,
  materializeAssignedChallengeMaterials,
  proposalInput,
} from "./work.mjs";

export const PAIRING_CONTEXT_SCHEMA =
  "hepta.paper_raid.agent_bridge.pairing_context.v1";
export const HEALTH_REPORT_SCHEMA =
  "hepta.paper_raid.agent_bridge.health_report.v1";
export const INBOX_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.inbox_request.v1";
export const DELIVERY_DRAFT_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.delivery_draft_request.v1";
export const PROPOSAL_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.proposal_request.v1";

function clientFor(config, fetchImplementation) {
  return new AgentBridgeClient(
    config.bff_url,
    config.request_timeout_ms,
    fetchImplementation,
  );
}

function validatePairingCode(value) {
  if (
    typeof value !== "string" ||
    value.length < 6 ||
    value.length > 128 ||
    value.trim() !== value ||
    /[\0\r\n]/.test(value)
  ) {
    throw new Error("pairing code input is invalid");
  }
  return value;
}

function validatePairingContext(value, nowUnix) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    throw new Error("Agent Bridge pairing context is invalid");
  }
  const fields = [
    "schema",
    "grant_id",
    "subject_id",
    "player_id",
    "issued_at_unix",
    "expires_at_unix",
  ];
  if (
    Object.keys(value).length !== fields.length ||
    !fields.every(field => Object.hasOwn(value, field)) ||
    value.schema !== PAIRING_CONTEXT_SCHEMA
  ) {
    throw new Error("Agent Bridge pairing context has unsupported fields or schema");
  }
  assertCanonicalUuid(value.grant_id, "pairing context grant_id");
  if (
    typeof value.subject_id !== "string" ||
    value.subject_id.length < 1 ||
    value.subject_id.length > 512 ||
    value.subject_id.includes("\0")
  ) {
    throw new Error("Agent Bridge pairing context subject_id is invalid");
  }
  assertCanonicalUuid(value.player_id, "pairing context player_id");
  if (
    !Number.isSafeInteger(value.issued_at_unix) ||
    !Number.isSafeInteger(value.expires_at_unix) ||
    value.issued_at_unix < 0 ||
    value.expires_at_unix <= value.issued_at_unix ||
    value.expires_at_unix - value.issued_at_unix > 300 ||
    value.expires_at_unix <= nowUnix
  ) {
    throw new Error("Agent Bridge pairing context is expired or invalid");
  }
  return value;
}

export function deterministicUuid(domain, fields) {
  const digest = createHash("sha256")
    .update(domain, "utf8")
    .update("\0", "utf8")
    .update(fields.join("\0"), "utf8")
    .digest()
    .subarray(0, 16);
  digest[6] = (digest[6] & 0x0f) | 0x50;
  digest[8] = (digest[8] & 0x3f) | 0x80;
  const hex = digest.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function stableDeliveryDraftId(state, input) {
  return deterministicUuid("hepta.paper_raid.agent_bridge.delivery_draft_id.v1", [
    state.binding_id,
    input.paper_id,
    input.work_item_id,
    input.section_key,
    input.artifact_manifest_id,
    input.payload_hash,
  ]);
}

export function stableBindingId(identity, playerId) {
  return deterministicUuid("hepta.paper_raid.agent_bridge.binding_id.v1", [
    playerId,
    identity.agent_id,
  ]);
}

function stablePairNonce(context, bindingId, identity, disclosureHash) {
  return deterministicUuid("hepta.paper_raid.agent_bridge.pair_nonce.v1", [
    context.grant_id,
    bindingId,
    identity.agent_key_id,
    disclosureHash,
  ]);
}

export function createBindingRequest(
  identity,
  capabilityDisclosure,
  context,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    issuedAtUnix = nowUnix,
    bindingId = randomUUID(),
    nonce = randomUUID(),
  } = {},
) {
  const capabilityDisclosureHash = agentCapabilityDisclosureHash(
    capabilityDisclosure,
  );
  const expiresAtUnix = Math.min(issuedAtUnix + 300, context.expires_at_unix);
  if (expiresAtUnix <= nowUnix || issuedAtUnix > nowUnix + 5) {
    throw new Error("pairing grant expires before Agent binding proof can be issued");
  }
  const claim = {
    schema: AGENT_BINDING_PROOF_SCHEMA,
    binding_id: assertCanonicalUuid(bindingId, "binding_id"),
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    agent_public_key: identity.agent_public_key,
    agent_public_key_hash: identity.agent_public_key_hash,
    capability_disclosure_hash: capabilityDisclosureHash,
    subject_id: context.subject_id,
    player_id: context.player_id,
    nonce: assertCanonicalUuid(nonce, "agent_proof_nonce"),
    issued_at_unix: issuedAtUnix,
    expires_at_unix: expiresAtUnix,
  };
  return Object.freeze({
    agent_proof_schema: AGENT_BINDING_PROOF_SCHEMA,
    binding_id: claim.binding_id,
    player_id: claim.player_id,
    agent_id: claim.agent_id,
    agent_key_id: claim.agent_key_id,
    agent_public_key: claim.agent_public_key,
    capability_disclosure: capabilityDisclosure,
    capability_disclosure_hash: capabilityDisclosureHash,
    agent_proof_nonce: claim.nonce,
    agent_proof_issued_at_unix: claim.issued_at_unix,
    agent_proof_expires_at_unix: claim.expires_at_unix,
    agent_proof_signature: identity.sign(agentBindingProofFrame(claim)),
    idempotency_key: claim.nonce,
  });
}

function responseBinding(value) {
  if (
    value &&
    !Array.isArray(value) &&
    typeof value === "object" &&
    value.binding &&
    !Array.isArray(value.binding) &&
    typeof value.binding === "object"
  ) {
    return value.binding;
  }
  return value;
}

function validatePublicBinding(value, identity, disclosureHash, playerId) {
  const binding = responseBinding(value);
  if (!binding || Array.isArray(binding) || typeof binding !== "object") {
    throw new Error("Agent Bridge returned an invalid binding");
  }
  assertCanonicalUuid(binding.binding_id, "binding_id");
  assertCanonicalUuid(binding.player_id, "player_id");
  assertLogicalId(binding.agent_id, "agent_id");
  assertDigest(binding.agent_key_id, "agent_key_id");
  assertDigest(binding.agent_public_key_hash, "agent_public_key_hash");
  assertDigest(binding.capability_disclosure_hash, "capability_disclosure_hash");
  decodeCanonicalBase64(binding.agent_public_key, 32, "agent_public_key");
  if (
    agentCapabilityDisclosureHash(binding.capability_disclosure) !== disclosureHash
  ) {
    throw new Error("Agent Bridge binding capability disclosure is invalid");
  }
  if (!Number.isSafeInteger(binding.version) || binding.version < 1) {
    throw new Error("Agent Bridge binding version is invalid");
  }
  for (const field of ["created_at", "updated_at"]) {
    if (
      typeof binding[field] !== "string" ||
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(binding[field]) ||
      Number.isNaN(Date.parse(binding[field]))
    ) {
      throw new Error(`Agent Bridge binding ${field} is invalid`);
    }
  }
  if (
    binding.player_id !== playerId ||
    binding.agent_id !== identity.agent_id ||
    binding.agent_key_id !== identity.agent_key_id ||
    binding.agent_public_key !== identity.agent_public_key ||
    binding.agent_public_key_hash !== identity.agent_public_key_hash ||
    binding.capability_disclosure_hash !== disclosureHash ||
    binding.status !== "active"
  ) {
    throw new Error("Agent Bridge binding does not match the local identity and disclosure");
  }
  // Return an allowlisted public projection. Even a malformed/malicious BFF
  // response cannot reflect a pairing code, subject context, or credential to
  // CLI output through an unexpected field.
  const disclosure = Object.freeze({
    schema: binding.capability_disclosure.schema,
    assurance: binding.capability_disclosure.assurance,
    capabilities: Object.freeze([...binding.capability_disclosure.capabilities]),
    resource_classes: Object.freeze([
      ...binding.capability_disclosure.resource_classes,
    ]),
    max_parallel_tasks: binding.capability_disclosure.max_parallel_tasks,
  });
  return Object.freeze({
    binding_id: binding.binding_id,
    player_id: binding.player_id,
    agent_id: binding.agent_id,
    agent_key_id: binding.agent_key_id,
    agent_public_key: binding.agent_public_key,
    agent_public_key_hash: binding.agent_public_key_hash,
    capability_disclosure: disclosure,
    capability_disclosure_hash: binding.capability_disclosure_hash,
    status: binding.status,
    version: binding.version,
    created_at: binding.created_at,
    updated_at: binding.updated_at,
  });
}

function assertStateMatchesConfig(config, state) {
  const expected = agentCapabilityDisclosureHash(config.capability_disclosure);
  if (state.capability_disclosure_hash !== expected) {
    throw new Error("Bridge config capability disclosure differs from the paired binding");
  }
  return expected;
}

export async function pairAgent(
  config,
  identity,
  {
    readPairingCode,
    fetchImplementation,
    nowUnix = Math.floor(Date.now() / 1000),
    bindingId,
    nonce,
  } = {},
) {
  // Prove the owner-only destination is writable before consuming a one-time
  // grant. An existing same-Agent state supplies binding continuity across an
  // Agent key rotation; its stale key is never accepted for signed requests.
  const existingState = await prepareBridgeStateForPairing(
    config.state_file,
    identity,
  );
  if (typeof readPairingCode !== "function") {
    throw new Error("pairing code must be supplied by an injected input reader");
  }
  let pairingCode;
  try {
    pairingCode = validatePairingCode(await readPairingCode());
  } catch {
    throw new Error("pairing code input failed");
  }
  const client = clientFor(config, fetchImplementation);
  const context = validatePairingContext(
    await client.publicPost(AGENT_BRIDGE_ENDPOINTS.pairing_context, {
      pairing_code: pairingCode,
    }),
    nowUnix,
  );
  if (existingState && existingState.player_id !== context.player_id) {
    throw new Error("pairing context player differs from the persisted binding");
  }
  const disclosureHash = agentCapabilityDisclosureHash(
    config.capability_disclosure,
  );
  const exactBindingId = bindingId ?? existingState?.binding_id ?? stableBindingId(
    identity,
    context.player_id,
  );
  const exactNonce = nonce ?? stablePairNonce(
    context,
    exactBindingId,
    identity,
    disclosureHash,
  );
  const bindingRequest = createBindingRequest(
    identity,
    config.capability_disclosure,
    context,
    {
      nowUnix,
      issuedAtUnix: context.issued_at_unix,
      bindingId: exactBindingId,
      nonce: exactNonce,
    },
  );
  // publicPost freezes one canonical body before its transport retry loop, so
  // an ambiguous lost response reuses this same in-memory code and exact V3
  // request. Neither value is ever written to a pending file.
  const value = await client.publicPost(AGENT_BRIDGE_ENDPOINTS.pair, {
    pairing_code: pairingCode,
    binding_request: bindingRequest,
  });
  const binding = validatePublicBinding(
    value,
    identity,
    disclosureHash,
    context.player_id,
  );
  await saveBridgeState(config.state_file, identity, binding, nowUnix, {
    replace: existingState !== null,
  });
  // Do not return the code, context (including subject_id), or signed request.
  return binding;
}

async function signedClientState(config, identity, fetchImplementation) {
  const state = await loadBridgeState(config.state_file, identity);
  assertStateMatchesConfig(config, state);
  return { state, client: clientFor(config, fetchImplementation) };
}

export async function getBinding(config, identity, fetchImplementation) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  const value = await client.signed(identity, state, {
    method: "GET",
    path: AGENT_BRIDGE_ENDPOINTS.binding,
  });
  return validatePublicBinding(
    value,
    identity,
    state.capability_disclosure_hash,
    state.player_id,
  );
}

export async function bridgeHealth(
  config,
  identity,
  {
    status = "healthy",
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
  } = {},
) {
  if (!["healthy", "degraded", "offline"].includes(status)) {
    throw new Error("health status must be healthy, degraded, or offline");
  }
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  return client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.health,
    nowUnix,
    body: {
      schema: HEALTH_REPORT_SCHEMA,
      assurance: "self_declared_unverified",
      status,
      observed_at_unix: nowUnix,
    },
  });
}

export async function getInbox(
  config,
  identity,
  { nowUnix = Math.floor(Date.now() / 1000), fetchImplementation } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  return client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.inbox,
    nowUnix,
    body: {
      schema: INBOX_REQUEST_SCHEMA,
      paper_ids: config.paper_ids,
    },
  });
}

async function getPracticeWithClient(client, state, identity, nowUnix) {
  const value = await client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.practice_tasks,
    nowUnix,
    body: { schema: PRACTICE_TASK_QUERY_SCHEMA },
  });
  return validatePracticeTasks(value);
}

export async function getPractice(
  config,
  identity,
  { nowUnix = Math.floor(Date.now() / 1000), fetchImplementation } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  return getPracticeWithClient(client, state, identity, nowUnix);
}

export async function executePracticeAuto(
  config,
  identity,
  {
    nowUnix = undefined,
    clock = () => Math.floor(Date.now() / 1000),
    fetchImplementation,
  } = {},
) {
  if (nowUnix !== undefined && (!Number.isSafeInteger(nowUnix) || nowUnix < 1)) {
    throw new Error("practice operation timestamp is invalid");
  }
  if (typeof clock !== "function") {
    throw new Error("practice operation clock is invalid");
  }
  const freshNowUnix = () => {
    const value = nowUnix ?? clock();
    if (!Number.isSafeInteger(value) || value < 1) {
      throw new Error("practice operation clock returned an invalid timestamp");
    }
    return value;
  };
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );

  // Discovery is the recovery authority. A prior process can have lost the
  // claim/result response after the BFF committed it; never start a new
  // transition until the exact owner-bound task state has been reread.
  let tasks = await getPracticeWithClient(
    client,
    state,
    identity,
    freshNowUnix(),
  );
  if (tasks.status !== "ready") {
    return Object.freeze({
      schema: PRACTICE_AUTO_RESULT_SCHEMA,
      mode: "practice_unranked",
      status: tasks.status,
      task_state: null,
      version: null,
      result_code: null,
    });
  }
  if (tasks.task.state === "completed") {
    return Object.freeze({
      schema: PRACTICE_AUTO_RESULT_SCHEMA,
      mode: "practice_unranked",
      status: "already_completed",
      task_state: "completed",
      version: tasks.task.version,
      result_code: tasks.task.result_code,
    });
  }

  if (tasks.task.state === "pending") {
    const fromVersion = tasks.task.version;
    const value = await client.signed(identity, state, {
      method: "POST",
      path: AGENT_BRIDGE_ENDPOINTS.practice_claims,
      nowUnix: freshNowUnix(),
      body: {
        schema: PRACTICE_CLAIM_REQUEST_SCHEMA,
        expected_version: fromVersion,
        task_token: tasks.task.task_token,
      },
    });
    validatePracticeTransitionResult(value, "claim", fromVersion);
    tasks = await getPracticeWithClient(
      client,
      state,
      identity,
      freshNowUnix(),
    );
  }
  if (tasks.status !== "ready" || tasks.task.state !== "claimed") {
    throw new Error("practice claim did not resolve to one exact claimed task");
  }

  const fromVersion = tasks.task.version;
  const resultCode = fixedPracticeResult(tasks);
  const value = await client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.practice_results,
    nowUnix: freshNowUnix(),
    body: {
      schema: PRACTICE_RESULT_REQUEST_SCHEMA,
      expected_version: fromVersion,
      task_token: tasks.task.task_token,
      result_code: resultCode,
    },
  });
  const result = validatePracticeTransitionResult(
    value,
    "result",
    fromVersion,
    resultCode,
  );
  const completed = await getPracticeWithClient(
    client,
    state,
    identity,
    freshNowUnix(),
  );
  if (
    completed.status !== "ready" ||
    completed.task.state !== "completed" ||
    completed.task.version !== result.version ||
    completed.task.result_code !== resultCode
  ) {
    throw new Error("practice result did not resolve to one exact completed task");
  }
  return Object.freeze({
    schema: PRACTICE_AUTO_RESULT_SCHEMA,
    mode: "practice_unranked",
    status: "completed",
    task_state: "completed",
    version: completed.task.version,
    result_code: completed.task.result_code,
  });
}

export async function downloadChallengeMaterialBundle(
  config,
  identity,
  bundle,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
  } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  return downloadAssignedChallengeMaterials(
    bundle,
    (exactBundle, object) => client.signedBytes(
      identity,
      state,
      {
        method: "GET",
        path: object.download_path,
        query: challengeMaterialObjectQuery(exactBundle, object),
        nowUnix,
      },
      {
        expectedBytes: object.size_bytes,
        maxBytes: object.size_bytes,
        expectedMediaType: object.media_type,
      },
    ),
  );
}

export async function prepareChallengeMaterialsForStart(
  config,
  identity,
  start,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
    materialRoot = join(dirname(config.state_file), "challenge-materials"),
  } = {},
) {
  authorWorkStartKey(start);
  const downloaded = await downloadChallengeMaterialBundle(
    config,
    identity,
    start.bundle,
    { nowUnix, fetchImplementation },
  );
  return materializeAssignedChallengeMaterials(start.bundle, downloaded, materialRoot);
}

export async function executeAndSubmitAuthorWorkStart(
  config,
  identity,
  start,
  options = {},
) {
  const materialization = await prepareChallengeMaterialsForStart(
    config,
    identity,
    start,
    options,
  );
  const output = await executeAuthorWorkStart(
    config,
    materialization,
    start,
    options,
  );
  const prepared = await prepareDeliveryDraft(
    config,
    identity,
    output,
    options,
  );
  const freshInbox = await getInbox(config, identity, options);
  const candidate = deliveryCandidateForAuthorOutput(
    freshInbox,
    start,
    output,
    prepared,
  );
  const result = await submitAgentProposal(
    config,
    identity,
    proposalInput(candidate),
    options,
  );
  return Object.freeze({
    schema: "hepta.paper_raid.agent_bridge.author_work_submission.v1",
    candidate,
    result,
  });
}

export async function executeAndSubmitAuthorDelivery(
  config,
  identity,
  candidate,
  options = {},
) {
  return submitAgentProposal(config, identity, proposalInput(candidate), options);
}

export function createDeliveryDraftRequest(state, input, { draftId } = {}) {
  for (const [field, value] of [
    ["paper_id", input.paper_id],
    ["work_item_id", input.work_item_id],
    ["artifact_manifest_id", input.artifact_manifest_id],
  ]) {
    assertCanonicalUuid(value, field);
  }
  assertLogicalId(input.section_key, "section_key");
  assertDigest(input.payload_hash, "payload_hash");
  return Object.freeze({
    schema: DELIVERY_DRAFT_REQUEST_SCHEMA,
    delivery_draft_id: assertCanonicalUuid(
      draftId ?? stableDeliveryDraftId(state, input),
      "delivery_draft_id",
    ),
    paper_id: input.paper_id,
    work_item_id: input.work_item_id,
    section_key: input.section_key,
    artifact_manifest_id: input.artifact_manifest_id,
    payload_hash: input.payload_hash,
  });
}

export async function prepareDeliveryDraft(
  config,
  identity,
  input,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
    draftId,
  } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  return client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.delivery_drafts,
    nowUnix,
    body: createDeliveryDraftRequest(state, input, { draftId }),
  });
}

export function createProposalRequest(
  state,
  identity,
  input,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    proposalId,
    idempotencyKey,
  } = {},
) {
  if (input.proposal_kind !== "delivery" || input.delivery_draft_id === undefined) {
    throw new Error("Agent Proposal V2 requires a delivery_draft_id-bound delivery");
  }
  for (const [field, value] of [
    ["paper_id", input.paper_id],
    ["work_item_id", input.work_item_id],
    ["parent_revision_id", input.parent_revision_id],
    ["lease_id", input.lease_id],
    ["artifact_manifest_id", input.artifact_manifest_id],
  ]) {
    assertCanonicalUuid(value, field);
  }
  assertLogicalId(input.section_key, "section_key");
  assertDigest(input.payload_hash, "payload_hash");
  assertDigest(input.artifact_manifest_hash, "artifact_manifest_hash");
  assertCanonicalUuid(input.delivery_draft_id, "delivery_draft_id");
  for (const [field, value] of [
    ["lease_fencing_token", input.lease_fencing_token],
    ["expected_work_version", input.expected_work_version],
  ]) {
    if (!Number.isSafeInteger(value) || value <= 0) {
      throw new Error(`${field} is invalid`);
    }
  }
  const signedAtUnix = input.declared_at_unix;
  if (!Number.isSafeInteger(signedAtUnix) || signedAtUnix < 0) {
    throw new Error("delivery declared_at_unix is invalid");
  }
  const stableProposalId = deterministicUuid(
    "hepta.paper_raid.agent_bridge.delivery_proposal_id.v1",
    [state.binding_id, input.delivery_draft_id],
  );
  const stableIdempotencyKey = deterministicUuid(
    "hepta.paper_raid.agent_bridge.delivery_idempotency_key.v1",
    [state.binding_id, input.delivery_draft_id],
  );
  if (proposalId !== undefined && proposalId !== stableProposalId) {
    throw new Error("delivery proposal_id is fixed by delivery_draft_id");
  }
  if (idempotencyKey !== undefined && idempotencyKey !== stableIdempotencyKey) {
    throw new Error("delivery idempotency_key is fixed by delivery_draft_id");
  }
  const exactProposalId = proposalId ?? stableProposalId;
  const exactIdempotencyKey = idempotencyKey ?? stableIdempotencyKey;
  const claim = {
    schema: AGENT_PROPOSAL_SCHEMA,
    proposal_id: assertCanonicalUuid(exactProposalId, "proposal_id"),
    paper_project_id: input.paper_id,
    work_item_id: input.work_item_id,
    section_key: input.section_key,
    parent_revision_id: input.parent_revision_id,
    lease_id: input.lease_id,
    lease_fencing_token: input.lease_fencing_token,
    expected_work_version: input.expected_work_version,
    proposal_kind: input.proposal_kind,
    payload_hash: input.payload_hash,
    artifact_manifest_id: input.artifact_manifest_id,
    artifact_manifest_hash: input.artifact_manifest_hash,
    agent_id: identity.agent_id,
    binding_id: state.binding_id,
    agent_key_id: identity.agent_key_id,
    signed_at_unix: signedAtUnix,
  };
  const payload = Object.freeze({
    proposal_id: claim.proposal_id,
    work_item_id: claim.work_item_id,
    section_key: claim.section_key,
    parent_revision_id: claim.parent_revision_id,
    lease_id: claim.lease_id,
    lease_fencing_token: claim.lease_fencing_token,
    expected_work_version: claim.expected_work_version,
    proposal_kind: claim.proposal_kind,
    payload_hash: claim.payload_hash,
    artifact_manifest_id: claim.artifact_manifest_id,
    agent_id: claim.agent_id,
    binding_id: claim.binding_id,
    agent_key_id: claim.agent_key_id,
    signed_at_unix: claim.signed_at_unix,
    signature: identity.sign(agentProposalFrame(claim)),
    idempotency_key: assertCanonicalUuid(exactIdempotencyKey, "idempotency_key"),
  });
  return Object.freeze({
    schema: PROPOSAL_REQUEST_SCHEMA,
    paper_id: input.paper_id,
    delivery_draft_id: input.delivery_draft_id,
    payload,
  });
}

export async function submitAgentProposal(
  config,
  identity,
  input,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
    proposalId,
    idempotencyKey,
  } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  const request = createProposalRequest(state, identity, input, {
    nowUnix,
    proposalId,
    idempotencyKey,
  });
  return client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.proposals,
    nowUnix,
    body: request,
  });
}

async function submitReviewReceiptWithClient(
  client,
  state,
  identity,
  request,
  nowUnix,
) {
  validateReviewReceiptRequest(state, identity, request);
  const result = await client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.review_receipts,
    nowUnix,
    body: request,
  });
  return validateReviewReceiptResult(request, result);
}

export async function recoverPendingReviewReceipt(
  config,
  identity,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
  } = {},
) {
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  const pending = await loadReviewOutbox(config, state, identity, {
    optional: true,
  });
  if (pending === null) return null;
  const result = await submitReviewReceiptWithClient(
    client,
    state,
    identity,
    pending.request,
    nowUnix,
  );
  await clearReviewOutbox(config);
  return Object.freeze({
    receipt_id: pending.request.receipt.receipt_id,
    task_id: pending.request.receipt.task_id,
    recovered: true,
    result,
  });
}

export async function executeAndSubmitReviewTask(
  config,
  identity,
  task,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    fetchImplementation,
    adapter,
    adapterOptions,
  } = {},
) {
  const requiredCapability = task.kind === "reproduce"
    ? "reproduction"
    : "artifact_analysis";
  if (!config.capability_disclosure.capabilities.includes(requiredCapability)) {
    throw new Error(`review task requires declared ${requiredCapability} capability`);
  }
  const { state, client } = await signedClientState(
    config,
    identity,
    fetchImplementation,
  );
  const pending = await loadReviewOutbox(config, state, identity, {
    optional: true,
  });
  if (pending !== null) {
    if (pending.request.receipt.task_id !== task.task_id) {
      throw new Error("a different review receipt must be recovered before new work");
    }
    const result = await submitReviewReceiptWithClient(
      client,
      state,
      identity,
      pending.request,
      nowUnix,
    );
    await clearReviewOutbox(config);
    return Object.freeze({
      receipt_id: pending.request.receipt.receipt_id,
      task_id: task.task_id,
      recovered: true,
      result,
    });
  }
  if (task.state !== "pending") {
    throw new Error("review execution can start only from a pending task");
  }
  const request = await executeReviewTask(state, identity, task, {
    downloadObject: (exactTask, object) => client.signedBytes(
      identity,
      state,
      {
        method: "GET",
        path: object.download_path,
        query: reviewObjectQuery(exactTask, object),
        nowUnix,
      },
      {
        expectedBytes: object.size_bytes,
        maxBytes: object.size_bytes,
      },
    ),
    adapter,
    adapterOptions,
  });
  const outbox = await saveReviewOutbox(
    config,
    state,
    identity,
    request,
  );
  const result = await submitReviewReceiptWithClient(
    client,
    state,
    identity,
    outbox.request,
    nowUnix,
  );
  await clearReviewOutbox(config);
  return Object.freeze({
    receipt_id: request.receipt.receipt_id,
    task_id: task.task_id,
    recovered: false,
    result,
  });
}

export function signResearchSessionAction(identity, input) {
  const action = { ...input };
  delete action.signature;
  if (action.agent_key_id !== identity.agent_key_id) {
    throw new Error("research-session action agent_key_id does not match local identity");
  }
  action.signature = identity.sign(researchSessionActionFrame(action));
  return action;
}
