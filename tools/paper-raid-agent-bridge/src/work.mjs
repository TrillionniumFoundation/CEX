import { reviewTaskKey } from "./review.mjs";
import { canonicalJsonBytes, sha256Digest } from "./canonical.mjs";

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
const ASSIGNED_CHALLENGE_MATERIALS_SCHEMA =
  "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1";
const ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_SCHEMA =
  "hepta.paper_raid.assigned_challenge_material_bundle.v1";
const FROZEN_CHALLENGE_MATERIAL_AUTHORITY_SCHEMA =
  "hepta.paper_raid.frozen_challenge_material_authority.v1";
const CHALLENGE_OBJECT_PATH = "/api/agent-bridge/challenge-objects";

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

function exactKeys(value, keys) {
  return value && typeof value === "object" && !Array.isArray(value) &&
    Object.keys(value).sort().join("\0") === [...keys].sort().join("\0");
}

function canonicalUuid(value) {
  return typeof value === "string" && value === value.toLowerCase() &&
    UUID_PATTERN.test(value);
}

function validateFrozenChallengeMaterialAuthority(authority) {
  const fields = [
    "schema",
    "authority_hash",
    "activation_id",
    "activation_request_sha256",
    "challenge_id",
    "challenge_snapshot_hash",
    "template",
    "pack_id",
    "pack_manifest_hash",
    "ruleset_version",
    "ruleset_hash",
    "dataset_manifest_hash",
    "evaluator_manifest_hash",
  ];
  if (!exactKeys(authority, fields) ||
      authority.schema !== FROZEN_CHALLENGE_MATERIAL_AUTHORITY_SCHEMA ||
      !canonicalUuid(authority.activation_id) ||
      !canonicalUuid(authority.challenge_id) ||
      !contractText(authority.template) ||
      !contractText(authority.pack_id) ||
      !contractText(authority.ruleset_version)) {
    throw new Error("frozen challenge material authority is invalid");
  }
  for (const field of [
    "authority_hash",
    "activation_request_sha256",
    "challenge_snapshot_hash",
    "pack_manifest_hash",
    "ruleset_hash",
    "dataset_manifest_hash",
    "evaluator_manifest_hash",
  ]) {
    if (!DIGEST_PATTERN.test(String(authority[field] || ""))) {
      throw new Error("frozen challenge material authority digest is invalid");
    }
  }
  const frame = Object.fromEntries(
    fields.filter(field => field !== "authority_hash")
      .map(field => [field, authority[field]]),
  );
  if (sha256Digest(canonicalJsonBytes(frame)) !== authority.authority_hash) {
    throw new Error("frozen challenge material authority hash mismatch");
  }
  return Object.freeze({ ...authority });
}

function validateChallengeMaterialObject(object, expected) {
  const fields = [
    "object_key",
    "source_path",
    "logical_path",
    "role",
    "digest",
    "size_bytes",
    "media_type",
    "download_path",
  ];
  const dataset = expected.objectKey === "dataset";
  const datasetMapping =
    (object.logical_path === "challenge/dataset.json" &&
      object.media_type === "application/json") ||
    (object.logical_path === "challenge/dataset.csv" &&
      object.media_type === "text/csv; charset=utf-8");
  if (!exactKeys(object, fields) || object.object_key !== expected.objectKey ||
      object.role !== expected.role ||
      (!dataset && object.logical_path !== expected.logicalPath) ||
      (!dataset && object.media_type !== expected.mediaType) ||
      (dataset && !datasetMapping) ||
      !contractText(object.source_path) || object.source_path.startsWith("/") ||
      object.source_path.includes("\\") ||
      object.source_path.split("/").some(part => !part || part === "." || part === "..") ||
      !DIGEST_PATTERN.test(String(object.digest || "")) ||
      !Number.isSafeInteger(object.size_bytes) || object.size_bytes <= 0 ||
      object.size_bytes > 16 * 1024 * 1024 ||
      object.download_path !== CHALLENGE_OBJECT_PATH) {
    throw new Error("assigned challenge material object is invalid");
  }
  return Object.freeze({ ...object });
}

function validateChallengeMaterialBundle(bundle, {
  paperId,
  bindingId,
  task,
} = {}) {
  const fields = [
    "schema",
    "bundle_hash",
    "authority",
    "authority_hash",
    "paper_project_id",
    "challenge_ruleset_snapshot_hash",
    "binding_id",
    "player_id",
    "work_item_id",
    "work_item_version",
    "objects",
  ];
  if (!exactKeys(bundle, fields) ||
      bundle.schema !== ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_SCHEMA ||
      !DIGEST_PATTERN.test(String(bundle.bundle_hash || "")) ||
      !DIGEST_PATTERN.test(String(bundle.authority_hash || "")) ||
      !DIGEST_PATTERN.test(String(bundle.challenge_ruleset_snapshot_hash || "")) ||
      !canonicalUuid(bundle.paper_project_id) ||
      !canonicalUuid(bundle.binding_id) ||
      !canonicalUuid(bundle.player_id) ||
      !canonicalUuid(bundle.work_item_id) ||
      !Number.isSafeInteger(bundle.work_item_version) ||
      bundle.work_item_version <= 0 ||
      !Array.isArray(bundle.objects) || bundle.objects.length !== 4) {
    throw new Error("assigned challenge material bundle is invalid");
  }
  const authority = validateFrozenChallengeMaterialAuthority(bundle.authority);
  if (bundle.authority_hash !== authority.authority_hash ||
      (paperId !== undefined && bundle.paper_project_id !== paperId) ||
      (bindingId !== undefined && bundle.binding_id !== bindingId) ||
      (task !== undefined && (
        bundle.work_item_id !== task.work_item_id ||
        bundle.work_item_version !== task.version ||
        bundle.binding_id !== task.assigned_binding_id ||
        bundle.player_id !== task.assigned_player_id ||
        !["planned", "in_progress", "review"].includes(task.status)
      ))) {
    throw new Error("assigned challenge material bundle crosses its Author assignment");
  }
  const expected = [
    {
      objectKey: "brief",
      role: "playable_brief",
      logicalPath: "challenge/brief.md",
      mediaType: "text/markdown; charset=utf-8",
    },
    { objectKey: "dataset", role: "dataset" },
    {
      objectKey: "baseline",
      role: "baseline_code",
      logicalPath: "challenge/baseline.py",
      mediaType: "text/x-python; charset=utf-8",
    },
    {
      objectKey: "evaluator",
      role: "frozen_evaluator",
      logicalPath: "challenge/evaluator.py",
      mediaType: "text/x-python; charset=utf-8",
    },
  ];
  const objects = bundle.objects.map((object, index) =>
    validateChallengeMaterialObject(object, expected[index])
  );
  if (new Set(objects.map(object => object.source_path)).size !== objects.length ||
      new Set(objects.map(object => object.digest)).size !== objects.length) {
    throw new Error("assigned challenge material bundle contains duplicate objects");
  }
  const frame = Object.fromEntries(
    fields.filter(field => field !== "bundle_hash")
      .map(field => [field, field === "authority" ? authority :
        field === "objects" ? objects : bundle[field]]),
  );
  if (sha256Digest(canonicalJsonBytes(frame)) !== bundle.bundle_hash) {
    throw new Error("assigned challenge material bundle hash mismatch");
  }
  return Object.freeze({ ...bundle, authority, objects: Object.freeze(objects) });
}

export function challengeMaterialBundles(inbox) {
  if (!inbox || inbox.schema !== "hepta.paper_raid.agent_bridge.inbox.v2" ||
      !canonicalUuid(inbox.binding_id) || !Array.isArray(inbox.papers)) {
    throw new Error("Agent Bridge inbox schema is unsupported");
  }
  const bundles = [];
  const seen = new Set();
  for (const paper of records(inbox.papers)) {
    if (!paper || !canonicalUuid(paper.paper_id)) {
      throw new Error("Agent Bridge inbox paper projection is invalid");
    }
    if (paper.challenge_materials === undefined) continue;
    const projection = paper.challenge_materials;
    if (!exactKeys(projection, ["schema", "status", "reason_code", "items"]) ||
        projection.schema !== ASSIGNED_CHALLENGE_MATERIALS_SCHEMA ||
        !Array.isArray(projection.items)) {
      throw new Error("assigned challenge material projection is unsupported");
    }
    if (projection.status === "unavailable") {
      if (projection.items.length !== 0 || !contractText(projection.reason_code)) {
        throw new Error("unavailable challenge material projection must contain no bundle");
      }
      continue;
    }
    if (projection.status !== "available" || projection.reason_code !== null ||
        projection.items.length === 0 || !Array.isArray(paper.tasks)) {
      throw new Error("available challenge material projection is invalid");
    }
    for (const item of projection.items) {
      const task = paper.tasks.find(candidate =>
        candidate && candidate.work_item_id === item.work_item_id
      );
      if (!task) {
        throw new Error("challenge material bundle has no matching inbox work item");
      }
      const bundle = validateChallengeMaterialBundle(item, {
        paperId: paper.paper_id,
        bindingId: inbox.binding_id,
        task,
      });
      if (seen.has(bundle.work_item_id)) {
        throw new Error("challenge material projection duplicates a work-item assignment");
      }
      seen.add(bundle.work_item_id);
      bundles.push(bundle);
    }
  }
  return Object.freeze(bundles.sort((left, right) =>
    left.work_item_id.localeCompare(right.work_item_id)
  ));
}

export function challengeMaterialObjectQuery(bundle, object) {
  const exactBundle = validateChallengeMaterialBundle(bundle);
  const exactObject = exactBundle.objects.find(candidate =>
    candidate.object_key === object.object_key && candidate.digest === object.digest
  );
  if (!exactObject) {
    throw new Error("challenge material object is not in the assigned bundle");
  }
  const pairs = [
    ["bundle_hash", exactBundle.bundle_hash],
    ["digest", exactObject.digest],
    ["object_key", exactObject.object_key],
    ["paper_id", exactBundle.paper_project_id],
    ["work_item_id", exactBundle.work_item_id],
  ];
  return pairs
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)))
    .join("&");
}

export async function downloadAssignedChallengeMaterials(bundle, downloadObject) {
  if (typeof downloadObject !== "function") {
    throw new Error("challenge material downloader is unavailable");
  }
  const exactBundle = validateChallengeMaterialBundle(bundle);
  const downloaded = [];
  for (const object of exactBundle.objects) {
    const bytes = Buffer.from(await downloadObject(exactBundle, object));
    if (bytes.length !== object.size_bytes || sha256Digest(bytes) !== object.digest) {
      throw new Error("downloaded challenge material differs from frozen authority");
    }
    downloaded.push(Object.freeze({ descriptor: object, bytes }));
  }
  return Object.freeze(downloaded);
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
