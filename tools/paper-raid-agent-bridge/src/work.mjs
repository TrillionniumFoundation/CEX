import { reviewTaskKey } from "./review.mjs";

const AUTHOR_DELIVERY_PHASES = new Set(["drafting", "reproducing"]);
const DELIVERY_CANDIDATES_SCHEMA =
  "hepta.paper_raid.agent_bridge.delivery_candidates.v1";
const DELIVERY_CANDIDATE_SCHEMA =
  "hepta.paper_raid.agent_bridge.delivery_candidate.v1";
const WORK_RESULT_SCHEMA = "hepta.paper_raid.agent_bridge.work_result.v1";
const DELIVERY_STATES = new Set(["pending", "submitting", "consumed"]);
const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/;

function records(value) {
  return Array.isArray(value) ? value : [];
}

function contractText(value) {
  return typeof value === "string" && value.length > 0 && value.length <= 128 &&
    !/[\u0000-\u001f\u007f]/u.test(value);
}

function validateCandidate(candidate, paperId, bindingId) {
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate) ||
      candidate.schema !== DELIVERY_CANDIDATE_SCHEMA ||
      !UUID_PATTERN.test(String(candidate.binding_id || "")) ||
      !UUID_PATTERN.test(String(candidate.paper_id || "")) ||
      !UUID_PATTERN.test(String(candidate.delivery_draft_id || "")) ||
      candidate.binding_id !== bindingId || candidate.paper_id !== paperId ||
      candidate.proposal_kind !== "delivery" ||
      !UUID_PATTERN.test(String(candidate.work_item_id || "")) ||
      !contractText(candidate.section_key) ||
      !UUID_PATTERN.test(String(candidate.lease_id || "")) ||
      !Number.isSafeInteger(candidate.lease_fencing_token) ||
      candidate.lease_fencing_token <= 0 ||
      !Number.isSafeInteger(candidate.expected_work_version) ||
      candidate.expected_work_version <= 0 ||
      !UUID_PATTERN.test(String(candidate.parent_revision_id || "")) ||
      !UUID_PATTERN.test(String(candidate.artifact_manifest_id || "")) ||
      !DIGEST_PATTERN.test(String(candidate.artifact_manifest_hash || "")) ||
      !DIGEST_PATTERN.test(String(candidate.payload_hash || "")) ||
      !DELIVERY_STATES.has(candidate.delivery_state) ||
      !Number.isSafeInteger(candidate.declared_at_unix) ||
      candidate.declared_at_unix < 0 ||
      !Number.isSafeInteger(candidate.expires_at_unix) ||
      candidate.expires_at_unix <= candidate.declared_at_unix) {
    throw new Error("server-projected delivery candidate is invalid");
  }
  return Object.freeze({
    schema: DELIVERY_CANDIDATE_SCHEMA,
    delivery_draft_id: candidate.delivery_draft_id,
    binding_id: candidate.binding_id,
    paper_id: candidate.paper_id,
    work_item_id: candidate.work_item_id,
    section_key: candidate.section_key,
    lease_id: candidate.lease_id,
    lease_fencing_token: candidate.lease_fencing_token,
    expected_work_version: candidate.expected_work_version,
    parent_revision_id: candidate.parent_revision_id,
    proposal_kind: "delivery",
    payload_hash: candidate.payload_hash,
    artifact_manifest_id: candidate.artifact_manifest_id,
    artifact_manifest_hash: candidate.artifact_manifest_hash,
    delivery_state: candidate.delivery_state,
    declared_at_unix: candidate.declared_at_unix,
    expires_at_unix: candidate.expires_at_unix,
  });
}

export function deliveryCandidateKey(candidate) {
  return [
    candidate.delivery_draft_id,
    candidate.paper_id,
    candidate.work_item_id,
    candidate.section_key,
    candidate.parent_revision_id,
    candidate.artifact_manifest_id,
  ].join(":");
}

export function deliveryCandidates(inbox) {
  if (!inbox || inbox.schema !== "hepta.paper_raid.agent_bridge.inbox.v2" ||
      !UUID_PATTERN.test(String(inbox.binding_id || "")) ||
      !Array.isArray(inbox.papers)) {
    throw new Error("Agent Bridge inbox schema is unsupported");
  }
  const candidates = [];
  const seen = new Set();
  for (const paper of records(inbox.papers)) {
    if (!paper || !UUID_PATTERN.test(String(paper.paper_id || ""))) {
      throw new Error("Agent Bridge inbox paper projection is invalid");
    }
    const projection = paper.delivery_candidates;
    if (!projection || typeof projection !== "object" ||
        Array.isArray(projection) || projection.schema !== DELIVERY_CANDIDATES_SCHEMA ||
        !Array.isArray(projection.items)) {
      throw new Error("delivery candidate projection is unsupported");
    }
    if (projection.status === "unavailable") {
      if (projection.items.length !== 0 || !contractText(projection.reason_code)) {
        throw new Error("unavailable delivery projection must contain no candidates");
      }
      continue;
    }
    if (projection.status !== "available" || projection.items.length === 0) {
      throw new Error("available delivery projection is invalid");
    }
    for (const item of projection.items) {
      const candidate = validateCandidate(item, paper.paper_id, inbox.binding_id);
      if (candidate.delivery_state === "pending" &&
          !AUTHOR_DELIVERY_PHASES.has(paper.phase)) {
        throw new Error("pending delivery candidate is outside an author phase");
      }
      const key = deliveryCandidateKey(candidate);
      if (seen.has(key)) {
        throw new Error("delivery candidate projection contains a duplicate binding");
      }
      seen.add(key);
      candidates.push(candidate);
    }
  }
  return candidates.sort((left, right) =>
    deliveryCandidateKey(left).localeCompare(deliveryCandidateKey(right))
  );
}

export function actionableDeliveryCandidates(inbox, acknowledgedKeys = new Set()) {
  if (!(acknowledgedKeys instanceof Set)) {
    throw new Error("acknowledged delivery keys must be a Set");
  }
  return deliveryCandidates(inbox).filter(candidate =>
    candidate.delivery_state !== "consumed" ||
    !acknowledgedKeys.has(deliveryCandidateKey(candidate))
  );
}

export function proposalInput(candidate) {
  const validated = validateCandidate(
    candidate,
    candidate && candidate.paper_id,
    candidate && candidate.binding_id,
  );
  return Object.freeze({
    delivery_draft_id: validated.delivery_draft_id,
    paper_id: validated.paper_id,
    work_item_id: validated.work_item_id,
    section_key: validated.section_key,
    parent_revision_id: validated.parent_revision_id,
    lease_id: validated.lease_id,
    lease_fencing_token: validated.lease_fencing_token,
    expected_work_version: validated.expected_work_version,
    proposal_kind: "delivery",
    payload_hash: validated.payload_hash,
    artifact_manifest_id: validated.artifact_manifest_id,
    artifact_manifest_hash: validated.artifact_manifest_hash,
    declared_at_unix: validated.declared_at_unix,
  });
}

export function workResult(status, {
  candidateCount = 0,
  candidate = null,
  result = undefined,
  recoveredTaskId = undefined,
} = {}) {
  if (!Number.isSafeInteger(candidateCount) || candidateCount < 0) {
    throw new Error("work result candidate count is invalid");
  }
  const value = {
    schema: WORK_RESULT_SCHEMA,
    status,
    candidate_count: candidateCount,
  };
  if (status === "submitted") {
    if (!candidate || result === undefined) {
      throw new Error("submitted work result requires a candidate and result");
    }
    value.candidate_key = candidate.schema ===
      "hepta.paper_raid.agent_bridge.review_task.v1"
      ? reviewTaskKey(candidate)
      : deliveryCandidateKey(candidate);
    value.result = result;
  } else if (status === "recovered") {
    if (result === undefined) {
      throw new Error("recovered work result requires a result");
    }
    value.recovered_task_id = recoveredTaskId;
    value.result = result;
  } else if (!["idle", "awaiting_local_selection", "cancelled_locally"].includes(status)) {
    throw new Error("work result status is invalid");
  }
  return Object.freeze(value);
}

export function formatDeliveryCandidate(candidate, index) {
  return `${index + 1}. paper ${candidate.paper_id} · task ${candidate.work_item_id} · section ${candidate.section_key} · manifest ${candidate.artifact_manifest_id} · draft ${candidate.delivery_draft_id}`;
}
