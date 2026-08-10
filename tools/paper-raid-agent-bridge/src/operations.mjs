import { createHash, randomUUID } from "node:crypto";
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

export const PAIRING_CONTEXT_SCHEMA =
  "hepta.paper_raid.agent_bridge.pairing_context.v1";
export const HEALTH_REPORT_SCHEMA =
  "hepta.paper_raid.agent_bridge.health_report.v1";
export const INBOX_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.inbox_request.v1";
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

function deterministicUuid(domain, fields) {
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

export function createProposalRequest(
  state,
  identity,
  input,
  {
    nowUnix = Math.floor(Date.now() / 1000),
    proposalId = randomUUID(),
    idempotencyKey = randomUUID(),
  } = {},
) {
  for (const [field, value] of [
    ["paper_id", input.paper_id],
    ["work_item_id", input.work_item_id],
    ["parent_revision_id", input.parent_revision_id],
    ["artifact_manifest_id", input.artifact_manifest_id],
  ]) {
    assertCanonicalUuid(value, field);
  }
  assertLogicalId(input.section_key, "section_key");
  assertDigest(input.payload_hash, "payload_hash");
  assertDigest(input.artifact_manifest_hash, "artifact_manifest_hash");
  if (!["proposal", "delivery"].includes(input.proposal_kind)) {
    throw new Error("proposal_kind must be proposal or delivery");
  }
  const claim = {
    schema: AGENT_PROPOSAL_SCHEMA,
    proposal_id: assertCanonicalUuid(proposalId, "proposal_id"),
    paper_project_id: input.paper_id,
    work_item_id: input.work_item_id,
    section_key: input.section_key,
    parent_revision_id: input.parent_revision_id,
    proposal_kind: input.proposal_kind,
    payload_hash: input.payload_hash,
    artifact_manifest_hash: input.artifact_manifest_hash,
    agent_id: identity.agent_id,
    binding_id: state.binding_id,
    agent_key_id: identity.agent_key_id,
    signed_at_unix: nowUnix,
  };
  const payload = Object.freeze({
    proposal_id: claim.proposal_id,
    work_item_id: claim.work_item_id,
    section_key: claim.section_key,
    parent_revision_id: claim.parent_revision_id,
    proposal_kind: claim.proposal_kind,
    payload_hash: claim.payload_hash,
    artifact_manifest_id: input.artifact_manifest_id,
    agent_id: claim.agent_id,
    binding_id: claim.binding_id,
    agent_key_id: claim.agent_key_id,
    signed_at_unix: claim.signed_at_unix,
    signature: identity.sign(agentProposalFrame(claim)),
    idempotency_key: assertCanonicalUuid(idempotencyKey, "idempotency_key"),
  });
  return Object.freeze({
    schema: PROPOSAL_REQUEST_SCHEMA,
    paper_id: input.paper_id,
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

export function signResearchSessionAction(identity, input) {
  const action = { ...input };
  delete action.signature;
  if (action.agent_key_id !== identity.agent_key_id) {
    throw new Error("research-session action agent_key_id does not match local identity");
  }
  action.signature = identity.sign(researchSessionActionFrame(action));
  return action;
}
