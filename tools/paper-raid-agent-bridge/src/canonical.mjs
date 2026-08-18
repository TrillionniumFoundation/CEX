import { createHash } from "node:crypto";

export const AGENT_BINDING_PROOF_SCHEMA =
  "hepta.paper_raid.agent_binding_proof.v3";
export const AGENT_CAPABILITY_DISCLOSURE_SCHEMA =
  "hepta.paper_raid.agent_capability_disclosure.v1";
export const AGENT_BRIDGE_REQUEST_PROOF_SCHEMA =
  "hepta.paper_raid.agent_bridge_request_proof.v1";
export const AGENT_PROPOSAL_V1_SCHEMA = "hepta.paper_raid.agent_proposal.v1";
export const AGENT_PROPOSAL_SCHEMA = "hepta.paper_raid.agent_proposal.v2";
export const REVIEW_EXECUTION_RECEIPT_SCHEMA =
  "hepta.paper_raid.review_execution_receipt.v1";
export const RESEARCH_SESSION_ACTION_SCHEMA =
  "trnm.research-session.action.v1";

export const AGENT_CAPABILITIES = Object.freeze([
  "artifact_analysis",
  "citation_verification",
  "evidence_search",
  "experiment_execution",
  "experiment_planning",
  "reproduction",
  "research_session_signing",
  "section_drafting",
]);
export const AGENT_RESOURCE_CLASSES = Object.freeze([
  "artifact_io",
  "browser",
  "code_execution",
  "cpu",
  "gpu",
  "network",
  "sandbox",
]);

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/;
const LOGICAL_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;

function fail(message) {
  throw new Error(message);
}

export function assertCanonicalUuid(value, field = "uuid") {
  if (typeof value !== "string" || !UUID_PATTERN.test(value)) {
    fail(`${field} must be a canonical lowercase UUID`);
  }
  return value;
}

export function assertDigest(value, field = "digest") {
  if (typeof value !== "string" || !DIGEST_PATTERN.test(value)) {
    fail(`${field} must be sha256 followed by 64 lowercase hexadecimal bytes`);
  }
  return value;
}

export function assertLogicalId(value, field = "logical_id") {
  if (typeof value !== "string" || !LOGICAL_ID_PATTERN.test(value)) {
    fail(`${field} must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}`);
  }
  return value;
}

export function assertText(value, field = "text") {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    [...value].length > 512 ||
    value.includes("\0")
  ) {
    fail(`${field} must contain 1-512 Unicode code points without NUL`);
  }
  return value;
}

export function decodeCanonicalBase64(value, expectedLength, field = "base64") {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${field} must be canonical padded base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.toString("base64") !== value) {
    fail(`${field} must be canonical padded base64`);
  }
  if (expectedLength !== undefined && decoded.length !== expectedLength) {
    fail(`${field} must decode to exactly ${expectedLength} bytes`);
  }
  return decoded;
}

export function sha256Digest(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function u32(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    fail("canonical uint32 is out of range");
  }
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32BE(value);
  return bytes;
}

function u64(value) {
  if (!Number.isSafeInteger(value) || value < 0) {
    fail("canonical uint64 must be a non-negative safe integer");
  }
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64BE(BigInt(value));
  return bytes;
}

function i64(value) {
  if (!Number.isSafeInteger(value)) {
    fail("canonical int64 must be a safe integer");
  }
  const bytes = Buffer.alloc(8);
  bytes.writeBigInt64BE(BigInt(value));
  return bytes;
}

function lengthPrefixed(value) {
  const bytes = Buffer.from(value);
  return Buffer.concat([u32(bytes.length), bytes]);
}

export function canonicalFrame(domain, fields) {
  if (typeof domain !== "string" || !/^[a-z0-9_]+$/.test(domain)) {
    fail("canonical frame domain is invalid");
  }
  const output = [Buffer.from(`${domain}\0`, "utf8")];
  for (const [kind, value] of fields) {
    switch (kind) {
      case "string":
        assertText(value, "canonical string");
        output.push(lengthPrefixed(Buffer.from(value, "utf8")));
        break;
      case "bytes":
        output.push(lengthPrefixed(value));
        break;
      case "digest":
        assertDigest(value);
        output.push(Buffer.from(value.slice(7), "hex"));
        break;
      case "u32":
        output.push(u32(value));
        break;
      case "u64":
        output.push(u64(value));
        break;
      case "i64":
        output.push(i64(value));
        break;
      default:
        fail(`unsupported canonical frame field ${kind}`);
    }
  }
  return Buffer.concat(output);
}

function assertSortedAllowlist(values, allowlist, { field, min, max }) {
  if (!Array.isArray(values) || values.length < min || values.length > max) {
    fail(`${field} must contain ${min} to ${max} entries`);
  }
  for (let index = 0; index < values.length; index += 1) {
    if (!allowlist.includes(values[index])) {
      fail(`${field}[${index}] is unsupported`);
    }
    if (index > 0 && values[index - 1] >= values[index]) {
      fail(`${field} must be lexicographically sorted and unique`);
    }
  }
  return values;
}

export function validateCapabilityDisclosure(disclosure) {
  if (
    !disclosure ||
    Array.isArray(disclosure) ||
    typeof disclosure !== "object" ||
    Object.keys(disclosure).length !== 5 ||
    ![
      "schema",
      "assurance",
      "capabilities",
      "resource_classes",
      "max_parallel_tasks",
    ].every(field => Object.hasOwn(disclosure, field))
  ) {
    fail("capability disclosure must contain exactly the frozen v1 fields");
  }
  if (disclosure.schema !== AGENT_CAPABILITY_DISCLOSURE_SCHEMA) {
    fail("unsupported Agent capability disclosure schema");
  }
  if (disclosure.assurance !== "self_declared_unverified") {
    fail("capability disclosure assurance must be self_declared_unverified");
  }
  assertSortedAllowlist(disclosure.capabilities, AGENT_CAPABILITIES, {
    field: "capabilities",
    min: 1,
    max: 16,
  });
  assertSortedAllowlist(disclosure.resource_classes, AGENT_RESOURCE_CLASSES, {
    field: "resource_classes",
    min: 0,
    max: 16,
  });
  if (
    !Number.isSafeInteger(disclosure.max_parallel_tasks) ||
    disclosure.max_parallel_tasks < 1 ||
    disclosure.max_parallel_tasks > 32
  ) {
    fail("max_parallel_tasks must be between 1 and 32");
  }
  return disclosure;
}

export function agentCapabilityDisclosureFrame(disclosure) {
  validateCapabilityDisclosure(disclosure);
  const fields = [
    ["string", disclosure.schema],
    ["string", disclosure.assurance],
    ["u32", disclosure.capabilities.length],
    ...disclosure.capabilities.map(value => ["string", value]),
    ["u32", disclosure.resource_classes.length],
    ...disclosure.resource_classes.map(value => ["string", value]),
    ["u32", disclosure.max_parallel_tasks],
  ];
  return canonicalFrame(
    "hepta_paper_raid_agent_capability_disclosure_v1",
    fields,
  );
}

export function agentCapabilityDisclosureHash(disclosure) {
  return sha256Digest(agentCapabilityDisclosureFrame(disclosure));
}

export function agentBindingProofFrame(claim) {
  if (claim.schema !== AGENT_BINDING_PROOF_SCHEMA) {
    fail("unsupported Agent binding proof schema");
  }
  assertCanonicalUuid(claim.binding_id, "binding_id");
  assertText(claim.agent_id, "agent_id");
  assertText(claim.agent_key_id, "agent_key_id");
  assertDigest(claim.capability_disclosure_hash, "capability_disclosure_hash");
  assertText(claim.subject_id, "subject_id");
  assertCanonicalUuid(claim.player_id, "player_id");
  assertText(claim.nonce, "nonce");
  if (
    !Number.isSafeInteger(claim.issued_at_unix) ||
    claim.issued_at_unix < 0 ||
    !Number.isSafeInteger(claim.expires_at_unix) ||
    claim.expires_at_unix <= claim.issued_at_unix
  ) {
    fail("Agent binding proof validity interval is invalid");
  }
  const publicKey = decodeCanonicalBase64(
    claim.agent_public_key,
    32,
    "agent_public_key",
  );
  assertDigest(claim.agent_public_key_hash, "agent_public_key_hash");
  if (sha256Digest(publicKey) !== claim.agent_public_key_hash) {
    fail("agent_public_key_hash does not match key");
  }
  if (claim.agent_key_id !== claim.agent_public_key_hash) {
    fail("agent_key_id must equal agent_public_key_hash");
  }
  return canonicalFrame("hepta_paper_raid_agent_binding_proof_v3", [
    ["string", claim.schema],
    ["string", claim.binding_id],
    ["string", claim.agent_id],
    ["string", claim.agent_key_id],
    ["bytes", publicKey],
    ["digest", claim.agent_public_key_hash],
    ["digest", claim.capability_disclosure_hash],
    ["string", claim.subject_id],
    ["string", claim.player_id],
    ["string", claim.nonce],
    ["i64", claim.issued_at_unix],
    ["i64", claim.expires_at_unix],
  ]);
}

function canonicalJsonValue(value, path = "body") {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) fail(`${path} number must be a safe integer`);
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item, index) => canonicalJsonValue(item, `${path}[${index}]`));
  }
  if (!value || typeof value !== "object") {
    fail(`${path} contains an unsupported JSON value`);
  }
  const output = Object.create(null);
  for (const key of Object.keys(value).sort()) {
    if (key.includes("\0")) fail(`${path} contains a NUL key`);
    output[key] = canonicalJsonValue(value[key], `${path}.${key}`);
  }
  return output;
}

export function canonicalJsonBytes(value) {
  return Buffer.from(JSON.stringify(canonicalJsonValue(value)), "utf8");
}

function decodeCanonicalQueryComponent(value, field) {
  const bytes = [];
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (value[index] === "%") {
      if (index + 2 >= value.length || !/^[0-9A-F]{2}$/.test(value.slice(index + 1, index + 3))) {
        fail(`${field} percent escapes must use two uppercase hex digits`);
      }
      const decoded = Number.parseInt(value.slice(index + 1, index + 3), 16);
      if (
        (decoded >= 0x30 && decoded <= 0x39) ||
        (decoded >= 0x41 && decoded <= 0x5a) ||
        (decoded >= 0x61 && decoded <= 0x7a) ||
        [0x2d, 0x2e, 0x5f, 0x7e].includes(decoded)
      ) {
        fail(`${field} percent-encodes an unreserved byte`);
      }
      bytes.push(decoded);
      index += 2;
      continue;
    }
    if (
      !(
        (code >= 0x30 && code <= 0x39) ||
        (code >= 0x41 && code <= 0x5a) ||
        (code >= 0x61 && code <= 0x7a) ||
        [0x2d, 0x2e, 0x5f, 0x7e].includes(code)
      )
    ) {
      fail(`${field} contains a non-canonical query byte`);
    }
    bytes.push(code);
  }
  const decoded = Buffer.from(bytes).toString("utf8");
  if (!Buffer.from(decoded, "utf8").equals(Buffer.from(bytes))) {
    fail(`${field} is not valid UTF-8`);
  }
  return decoded;
}

export function canonicalQuery(value = "") {
  if (typeof value !== "string") {
    fail("canonical Agent Bridge query must be a string");
  }
  if (Buffer.byteLength(value) > 2048) {
    fail("canonical Agent Bridge query exceeds 2048 bytes");
  }
  if (value === "") return value;
  const pairs = value.split("&");
  if (pairs.length > 32) fail("canonical Agent Bridge query exceeds 32 pairs");
  const keys = new Set();
  let previous = null;
  for (const pair of pairs) {
    const separator = pair.indexOf("=");
    if (separator <= 0 || pair.indexOf("=", separator + 1) !== -1) {
      fail("canonical Agent Bridge query pair is malformed");
    }
    const encodedKey = pair.slice(0, separator);
    const encodedValue = pair.slice(separator + 1);
    const key = decodeCanonicalQueryComponent(encodedKey, "query key");
    const decodedValue = decodeCanonicalQueryComponent(encodedValue, "query value");
    if ([...key].length > 64 || [...decodedValue].length > 512) {
      fail("canonical Agent Bridge query key or value is too long");
    }
    if (keys.has(key)) fail("canonical Agent Bridge query keys must be unique");
    keys.add(key);
    if (previous !== null && Buffer.compare(Buffer.from(previous), Buffer.from(pair)) >= 0) {
      fail("canonical Agent Bridge query pairs must be strictly sorted");
    }
    previous = pair;
  }
  return value;
}

export function agentBridgeRequestProofFrame(claim) {
  if (claim.schema !== AGENT_BRIDGE_REQUEST_PROOF_SCHEMA) {
    fail("unsupported Agent Bridge request-proof schema");
  }
  assertCanonicalUuid(claim.binding_id, "binding_id");
  assertLogicalId(claim.agent_id, "agent_id");
  assertDigest(claim.agent_key_id, "agent_key_id");
  assertCanonicalUuid(claim.nonce, "nonce");
  if (
    !Number.isSafeInteger(claim.issued_at_unix) ||
    claim.issued_at_unix < 0 ||
    !Number.isSafeInteger(claim.expires_at_unix) ||
    claim.expires_at_unix <= claim.issued_at_unix ||
    claim.expires_at_unix - claim.issued_at_unix > 60
  ) {
    fail("request proof validity interval must be between 1 and 60 seconds");
  }
  const allowedRoute = new Set([
    "GET /api/agent-bridge/binding",
    "POST /api/agent-bridge/health",
    "POST /api/agent-bridge/inbox",
    "POST /api/agent-bridge/practice-tasks",
    "POST /api/agent-bridge/practice-claims",
    "POST /api/agent-bridge/practice-results",
    "POST /api/agent-bridge/delivery-drafts",
    "POST /api/agent-bridge/proposals",
    "GET /api/agent-bridge/challenge-objects",
    "GET /api/agent-bridge/review-objects",
    "POST /api/agent-bridge/review-receipts",
  ]);
  if (
    typeof claim.canonical_path !== "string" ||
    !claim.canonical_path.startsWith("/") ||
    claim.canonical_path.includes("?") ||
    claim.canonical_path.includes("#") ||
    claim.canonical_path.includes("\0")
  ) {
    fail("Agent Bridge request path must be a canonical absolute path");
  }
  if (!allowedRoute.has(`${claim.http_method} ${claim.canonical_path}`)) {
    fail("Agent Bridge request method/path is not allowed");
  }
  if (claim.canonical_query !== canonicalQuery(claim.canonical_query)) {
    fail("Agent Bridge request query is not canonical");
  }
  assertDigest(claim.body_hash, "body_hash");
  if (
    claim.http_method === "GET" &&
    claim.body_hash !== sha256Digest(Buffer.alloc(0))
  ) {
    fail("Agent Bridge GET body hash must be SHA-256(empty)");
  }
  return canonicalFrame("hepta_paper_raid_agent_bridge_request_proof_v1", [
    ["string", claim.schema],
    ["string", claim.binding_id],
    ["string", claim.agent_id],
    ["string", claim.agent_key_id],
    ["string", claim.http_method],
    ["string", claim.canonical_path],
    ["bytes", Buffer.from(claim.canonical_query, "utf8")],
    ["digest", claim.body_hash],
    ["string", claim.nonce],
    ["i64", claim.issued_at_unix],
    ["i64", claim.expires_at_unix],
  ]);
}

export function agentProposalV1Frame(proposal) {
  if (
    proposal.schema !== AGENT_PROPOSAL_V1_SCHEMA ||
    !["proposal", "delivery"].includes(proposal.proposal_kind)
  ) {
    fail("Agent proposal schema or kind is invalid");
  }
  for (const field of [
    "proposal_id",
    "paper_project_id",
    "work_item_id",
    "parent_revision_id",
    "binding_id",
  ]) {
    assertCanonicalUuid(proposal[field], field);
  }
  assertLogicalId(proposal.section_key, "section_key");
  assertDigest(proposal.payload_hash, "payload_hash");
  assertDigest(proposal.artifact_manifest_hash, "artifact_manifest_hash");
  assertText(proposal.agent_id, "agent_id");
  assertText(proposal.agent_key_id, "agent_key_id");
  if (!Number.isSafeInteger(proposal.signed_at_unix) || proposal.signed_at_unix < 0) {
    fail("signed_at_unix must be a non-negative safe integer");
  }
  return canonicalFrame("hepta_paper_raid_agent_proposal_v1", [
    ["string", proposal.schema],
    ["string", proposal.proposal_id],
    ["string", proposal.paper_project_id],
    ["string", proposal.work_item_id],
    ["string", proposal.section_key],
    ["string", proposal.parent_revision_id],
    ["string", proposal.proposal_kind],
    ["digest", proposal.payload_hash],
    ["digest", proposal.artifact_manifest_hash],
    ["string", proposal.agent_id],
    ["string", proposal.binding_id],
    ["string", proposal.agent_key_id],
    ["i64", proposal.signed_at_unix],
  ]);
}

export function agentProposalFrame(proposal) {
  if (
    proposal.schema !== AGENT_PROPOSAL_SCHEMA ||
    !["proposal", "delivery"].includes(proposal.proposal_kind)
  ) {
    fail("Agent proposal V2 schema or kind is invalid");
  }
  for (const field of [
    "proposal_id",
    "paper_project_id",
    "work_item_id",
    "parent_revision_id",
    "lease_id",
    "artifact_manifest_id",
    "binding_id",
  ]) {
    assertCanonicalUuid(proposal[field], field);
  }
  assertLogicalId(proposal.section_key, "section_key");
  assertDigest(proposal.payload_hash, "payload_hash");
  assertDigest(proposal.artifact_manifest_hash, "artifact_manifest_hash");
  assertText(proposal.agent_id, "agent_id");
  assertText(proposal.agent_key_id, "agent_key_id");
  for (const field of ["lease_fencing_token", "expected_work_version"]) {
    if (!Number.isSafeInteger(proposal[field]) || proposal[field] <= 0) {
      fail(`${field} must be a valid safe unsigned integer`);
    }
  }
  if (!Number.isSafeInteger(proposal.signed_at_unix) || proposal.signed_at_unix < 0) {
    fail("signed_at_unix must be a non-negative safe integer");
  }
  return canonicalFrame("hepta_paper_raid_agent_proposal_v2", [
    ["string", proposal.schema],
    ["string", proposal.proposal_id],
    ["string", proposal.paper_project_id],
    ["string", proposal.work_item_id],
    ["string", proposal.section_key],
    ["string", proposal.parent_revision_id],
    ["string", proposal.lease_id],
    ["u64", proposal.lease_fencing_token],
    ["u64", proposal.expected_work_version],
    ["string", proposal.proposal_kind],
    ["digest", proposal.payload_hash],
    ["string", proposal.artifact_manifest_id],
    ["digest", proposal.artifact_manifest_hash],
    ["string", proposal.agent_id],
    ["string", proposal.binding_id],
    ["string", proposal.agent_key_id],
    ["i64", proposal.signed_at_unix],
  ]);
}

export function reviewExecutionReceiptFrame(receipt) {
  if (receipt.schema !== REVIEW_EXECUTION_RECEIPT_SCHEMA) {
    fail("unsupported review execution receipt schema");
  }
  for (const field of [
    "receipt_id",
    "task_id",
    "assignment_id",
    "binding_id",
    "paper_project_id",
    "submission_id",
  ]) {
    assertCanonicalUuid(receipt[field], field);
  }
  assertCanonicalUuid(receipt.evaluation_id, "evaluation_id");
  if (!["evaluate", "reproduce"].includes(receipt.kind)) {
    fail("review execution receipt kind is invalid");
  }
  for (const field of ["attempt", "fencing_token"]) {
    if (!Number.isSafeInteger(receipt[field]) || receipt[field] <= 0) {
      fail(`${field} must be a positive safe integer`);
    }
  }
  for (const field of [
    "bundle_hash",
    "input_root",
    "output_root",
    "metrics_hash",
    "seed_set_hash",
    "environment_hash",
    "run_manifest_hash",
    "logs_hash",
    "agent_key_id",
    "signing_public_key_hash",
  ]) {
    assertDigest(receipt[field], field);
  }
  assertText(receipt.evaluator_version, "evaluator_version");
  assertLogicalId(receipt.agent_id, "agent_id");
  if (
    (receipt.kind === "evaluate" && typeof receipt.candidate_passed !== "boolean") ||
    (receipt.kind === "reproduce" && receipt.candidate_passed !== null)
  ) {
    fail("review execution receipt kind does not bind candidate_passed correctly");
  }
  if (receipt.signing_public_key_hash !== receipt.agent_key_id) {
    fail("signing_public_key_hash must equal agent_key_id");
  }
  if (
    !Number.isSafeInteger(receipt.started_at_unix) ||
    receipt.started_at_unix < 0 ||
    !Number.isSafeInteger(receipt.completed_at_unix) ||
    receipt.completed_at_unix < receipt.started_at_unix
  ) {
    fail("review execution receipt time interval is invalid");
  }
  return canonicalFrame("hepta_paper_raid_review_execution_receipt_v1", [
    ["string", receipt.schema],
    ["string", receipt.receipt_id],
    ["string", receipt.task_id],
    ["string", receipt.assignment_id],
    ["string", receipt.binding_id],
    ["string", receipt.paper_project_id],
    ["string", receipt.submission_id],
    ["string", receipt.evaluation_id],
    ["string", receipt.kind],
    ["u64", receipt.attempt],
    ["u64", receipt.fencing_token],
    ["digest", receipt.bundle_hash],
    ["string", receipt.evaluator_version],
    ["digest", receipt.input_root],
    ["digest", receipt.output_root],
    ["digest", receipt.metrics_hash],
    ["u32", receipt.candidate_passed === null ? 0 : receipt.candidate_passed ? 2 : 1],
    ["digest", receipt.seed_set_hash],
    ["digest", receipt.environment_hash],
    ["digest", receipt.run_manifest_hash],
    ["digest", receipt.logs_hash],
    ["i64", receipt.started_at_unix],
    ["i64", receipt.completed_at_unix],
    ["string", receipt.agent_id],
    ["digest", receipt.agent_key_id],
    ["digest", receipt.signing_public_key_hash],
  ]);
}

const ACTION_TYPES = new Map([
  ["participant.ready", "trnm.research-session.ready.v1"],
  ["research.task.claimed", "trnm.paper-raid.task-claim.v1"],
  ["agent.proposal.submitted", "trnm.paper-raid.agent-proposal.v1"],
  ["artifact.manifest.published", "trnm.paper-raid.artifact-manifest.v1"],
  ["review.submitted", "trnm.paper-raid.review.v1"],
  ["checkpoint.recorded", "trnm.paper-raid.checkpoint.v1"],
  ["paper.release.acknowledged", "trnm.paper-raid.release-acknowledgement.v1"],
]);

export function researchSessionActionFrame(action) {
  if (action.schema !== RESEARCH_SESSION_ACTION_SCHEMA) {
    fail("unsupported research-session action schema");
  }
  for (const field of [
    "action_id",
    "authorization_id",
    "team_id",
    "paper_project_id",
    "challenge_id",
    "action_type",
    "payload_type",
    "agent_key_id",
  ]) {
    assertText(action[field], field);
  }
  assertLogicalId(action.session_id, "session_id");
  if (action.roster_version < 1 || action.participant_sequence < 1) {
    fail("roster_version and participant_sequence must be positive");
  }
  if (
    !Number.isSafeInteger(action.participant_slot) ||
    action.participant_slot < 1 ||
    action.participant_slot > 5 ||
    !Number.isSafeInteger(action.expected_session_version) ||
    action.expected_session_version < 1 ||
    !Number.isSafeInteger(action.issued_at_unix) ||
    action.issued_at_unix < 0
  ) {
    fail("research-session action counters or time are invalid");
  }
  if (ACTION_TYPES.get(action.action_type) !== action.payload_type) {
    fail("action_type and payload_type are not an allowed pair");
  }
  const payload = decodeCanonicalBase64(action.payload, undefined, "payload");
  if (payload.length < 1 || payload.length > 65_536) {
    fail("payload must contain 1 to 65536 bytes");
  }
  assertDigest(action.payload_hash, "payload_hash");
  if (sha256Digest(payload) !== action.payload_hash) {
    fail("payload_hash does not match payload bytes");
  }
  assertDigest(action.reference_hash, "reference_hash");
  return canonicalFrame("trnm_research_session_action_signature_v1", [
    ["string", action.schema],
    ["string", action.action_id],
    ["string", action.authorization_id],
    ["string", action.session_id],
    ["string", action.team_id],
    ["string", action.paper_project_id],
    ["string", action.challenge_id],
    ["u64", action.roster_version],
    ["u32", action.participant_slot],
    ["u64", action.participant_sequence],
    ["u64", action.expected_session_version],
    ["i64", action.issued_at_unix],
    ["string", action.action_type],
    ["string", action.payload_type],
    ["bytes", payload],
    ["digest", action.payload_hash],
    ["digest", action.reference_hash],
    ["string", action.agent_key_id],
  ]);
}
