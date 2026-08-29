import { REVIEW_TASKS_SCHEMA, reviewTaskKey } from "./review.mjs";
import { canonicalJsonBytes, sha256Digest } from "./canonical.mjs";
import { randomBytes } from "node:crypto";
import {
  constants as fsConstants,
  lstat,
  mkdir,
  open,
  realpath,
  readdir,
  rm,
} from "node:fs/promises";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { basename, dirname, join, resolve } from "node:path";
import { AUTHOR_EXECUTOR_SCHEMA } from "./config.mjs";

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
const LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_SCHEMA =
  "hepta.paper_raid.legacy_golden_qualification_material_authority.v1";
const LEGACY_GOLDEN_QUALIFICATION_ID =
  "paper-raid-golden-v2-strict-review-v1";
const LEGACY_GOLDEN_TITLE = "Paper Raid: Reproducible Synthetic Ablation";
const LEGACY_GOLDEN_DESCRIPTION =
  "Reproduce a public deterministic baseline, retain the failed run, and deliver one evidence-bound ablation as a short paper. paper-raid-alpha-evaluator-sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8";
const LEGACY_GOLDEN_RULESET_VERSION = "paper-raid-golden-v2";
const LEGACY_GOLDEN_RULESET_HASH =
  "sha256:39cfc6a5c883e49b78bf315b53079336539c932ca48ff5a040415d7e5dd9b2c0";
const LEGACY_GOLDEN_DATASET_MANIFEST_HASH =
  "sha256:6c0494ec10383018b4a938528d179d16fcb6dd8961b8294ff6f4093dda42aeb4";
const LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH =
  "sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8";
const CHALLENGE_OBJECT_PATH = "/api/agent-bridge/challenge-objects";
const MATERIALIZATION_SCHEMA =
  "hepta.paper_raid.agent_bridge.challenge_materialization.v1";
export const AUTHOR_WORK_START_SCHEMA =
  "hepta.paper_raid.agent_bridge.author_work_start.v1";
export const AUTHOR_EXECUTOR_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.author_executor_request.v1";
export const AUTHOR_EXECUTOR_RESULT_SCHEMA =
  "hepta.paper_raid.agent_bridge.author_executor_result.v1";
const ACTIVE_AUTHOR_TASK_STATES = new Set(["planned", "in_progress"]);
const AUTHOR_START_TASK_STATES = new Set(["planned", "in_progress"]);
const MATERIAL_MOVE_EXECUTABLE = "/usr/bin/mv";
const execFileAsync = promisify(execFile);
const AUTHOR_EXECUTOR_MAX_OUTPUT_BYTES = 64 * 1024;

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
    if (paper.delivery_candidates === undefined) {
      const hasAuthorSurface = [
        "phase",
        "tasks",
        "proposals",
        "challenge_materials",
      ].some(field => Object.hasOwn(paper, field));
      const reviewProjection = paper.review_tasks;
      if (!hasAuthorSurface && reviewProjection &&
          typeof reviewProjection === "object" && !Array.isArray(reviewProjection) &&
          reviewProjection.schema === REVIEW_TASKS_SCHEMA) {
        continue;
      }
      throw new Error("delivery candidate projection is unsupported");
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
  if (authority?.schema ===
      LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_SCHEMA) {
    const fields = [
      "schema",
      "authority_hash",
      "qualification_id",
      "challenge_id",
      "challenge_snapshot_hash",
      "challenge_title",
      "challenge_description",
      "challenge_status",
      "ruleset_version",
      "ruleset_hash",
      "ruleset_absent",
      "dataset_manifest_hash",
      "evaluator_manifest_hash",
      "objects",
    ];
    if (!exactKeys(authority, fields) ||
        authority.qualification_id !== LEGACY_GOLDEN_QUALIFICATION_ID ||
        !canonicalUuid(authority.challenge_id) ||
        authority.challenge_title !== LEGACY_GOLDEN_TITLE ||
        authority.challenge_description !== LEGACY_GOLDEN_DESCRIPTION ||
        authority.challenge_status !== "open" ||
        authority.ruleset_version !== LEGACY_GOLDEN_RULESET_VERSION ||
        authority.ruleset_hash !== LEGACY_GOLDEN_RULESET_HASH ||
        authority.ruleset_absent !== true ||
        authority.dataset_manifest_hash !== LEGACY_GOLDEN_DATASET_MANIFEST_HASH ||
        authority.evaluator_manifest_hash !== LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH ||
        !DIGEST_PATTERN.test(String(authority.authority_hash || "")) ||
        !DIGEST_PATTERN.test(String(authority.challenge_snapshot_hash || "")) ||
        !Array.isArray(authority.objects) || authority.objects.length !== 4) {
      throw new Error("legacy golden qualification material authority is invalid");
    }
    const exact = [
      {
        object_key: "brief",
        source_path: "qualification/legacy-golden/brief.md",
        logical_path: "challenge/brief.md",
        role: "playable_brief",
        digest: "sha256:b926b4c868af652b2fed4671efc07f9bbc021c65ac188c971c2a2b67c365a9a3",
        size_bytes: 913,
        media_type: "text/markdown; charset=utf-8",
        download_path: CHALLENGE_OBJECT_PATH,
      },
      {
        object_key: "dataset",
        source_path: "data/synthetic-observations.csv",
        logical_path: "challenge/dataset.csv",
        role: "dataset",
        digest: "sha256:b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314",
        size_bytes: 230,
        media_type: "text/csv; charset=utf-8",
        download_path: CHALLENGE_OBJECT_PATH,
      },
      {
        object_key: "baseline",
        source_path: "code/baseline.py",
        logical_path: "challenge/baseline.py",
        role: "baseline_code",
        digest: "sha256:059717cc82d10cee0504ed6af3fa81121d7a7645c53d8e8a788a35f63ae644ba",
        size_bytes: 1071,
        media_type: "text/x-python; charset=utf-8",
        download_path: CHALLENGE_OBJECT_PATH,
      },
      {
        object_key: "evaluator",
        source_path: "evaluator/legacy-golden-evaluator.py",
        logical_path: "challenge/evaluator.py",
        role: "frozen_evaluator",
        digest: "sha256:63971194ab97e1d14752795ff1ff8c39a44d1a7782ffbb9459ec2210fe8a8e3d",
        size_bytes: 3483,
        media_type: "text/x-python; charset=utf-8",
        download_path: CHALLENGE_OBJECT_PATH,
      },
    ];
    const objects = authority.objects.map((object, index) =>
      validateChallengeMaterialObject(object, {
        objectKey: exact[index].object_key,
        role: exact[index].role,
        logicalPath: exact[index].logical_path,
        mediaType: exact[index].media_type,
      })
    );
    if (canonicalJsonBytes(objects).compare(canonicalJsonBytes(exact)) !== 0) {
      throw new Error("legacy golden qualification material objects are not exact");
    }
    const frame = Object.fromEntries(
      fields.filter(field => field !== "authority_hash")
        .map(field => [field, field === "objects" ? objects : authority[field]]),
    );
    if (sha256Digest(canonicalJsonBytes(frame)) !== authority.authority_hash) {
      throw new Error("legacy golden qualification material authority hash mismatch");
    }
    return Object.freeze({ ...authority, objects: Object.freeze(objects) });
  }
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
        bundle.paper_project_id !== task.paper_project_id ||
        bundle.work_item_id !== task.work_item_id ||
        bundle.work_item_version !== task.version ||
        bundle.binding_id !== task.assigned_binding_id ||
        bundle.player_id !== task.assigned_player_id ||
        !["planned", "in_progress"].includes(task.status)
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
  if (authority.schema ===
      LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_SCHEMA &&
      canonicalJsonBytes(objects).compare(canonicalJsonBytes(authority.objects)) !== 0) {
    throw new Error("legacy golden qualification bundle differs from frozen authority");
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
      // Validate the server-signed bundle before using its work item as a
      // lookup key. A terminal task may remain in an append-only inbox next
      // to a new active task, but it must never become a material source for
      // current Author work.
      const projected = validateChallengeMaterialBundle(item, {
        paperId: paper.paper_id,
        bindingId: inbox.binding_id,
      });
      const tasks = paper.tasks.filter(candidate =>
        candidate && candidate.work_item_id === projected.work_item_id
      );
      if (tasks.length !== 1) {
        throw new Error("challenge material bundle has no matching inbox work item");
      }
      const [task] = tasks;
      if (!ACTIVE_AUTHOR_TASK_STATES.has(task.status)) continue;
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

function validateAuthorWorkStart(start) {
  const fields = [
    "schema",
    "binding_id",
    "player_id",
    "paper_id",
    "work_item_id",
    "work_item_version",
    "task_kind",
    "bundle",
  ];
  if (!exactKeys(start, fields) || start.schema !== AUTHOR_WORK_START_SCHEMA ||
      !canonicalUuid(start.binding_id) || !canonicalUuid(start.player_id) ||
      !canonicalUuid(start.paper_id) || !canonicalUuid(start.work_item_id) ||
      !Number.isSafeInteger(start.work_item_version) || start.work_item_version <= 0 ||
      !contractText(start.task_kind)) {
    throw new Error("Author work start is invalid");
  }
  const bundle = validateChallengeMaterialBundle(start.bundle, {
    paperId: start.paper_id,
    bindingId: start.binding_id,
    task: {
      work_item_id: start.work_item_id,
      paper_project_id: start.paper_id,
      assigned_binding_id: start.binding_id,
      assigned_player_id: start.player_id,
      version: start.work_item_version,
      status: "in_progress",
    },
  });
  if (bundle.player_id !== start.player_id ||
      bundle.work_item_id !== start.work_item_id ||
      bundle.work_item_version !== start.work_item_version) {
    throw new Error("Author work start crosses its frozen material assignment");
  }
  return Object.freeze({ ...start, bundle });
}

export function authorWorkStartKey(start) {
  const exact = validateAuthorWorkStart(start);
  return [
    exact.paper_id,
    exact.work_item_id,
    exact.work_item_version,
    exact.bundle.bundle_hash,
  ].join(":");
}

export function formatAuthorWorkStart(start, index) {
  const exact = validateAuthorWorkStart(start);
  return `${index + 1}. Author ${exact.task_kind} ` +
    `${exact.paper_id}/${exact.work_item_id} v${exact.work_item_version}`;
}

export function authorWorkStarts(inbox, acknowledgedKeys = new Set()) {
  if (!(acknowledgedKeys instanceof Set)) {
    throw new Error("acknowledged Author work-start keys must be a Set");
  }
  const candidates = deliveryCandidates(inbox);
  // Recovery is globally ordered ahead of new scientific execution. A pending,
  // submitting, or consumed delivery must be resolved before any other task can
  // launch an Author executor.
  if (candidates.length !== 0) return Object.freeze([]);
  const starts = [];
  for (const bundle of challengeMaterialBundles(inbox)) {
    const paper = inbox.papers.find(item => item?.paper_id === bundle.paper_project_id);
    const tasks = records(paper?.tasks).filter(task =>
      task?.work_item_id === bundle.work_item_id &&
      task?.paper_project_id === bundle.paper_project_id &&
      task?.assigned_binding_id === bundle.binding_id &&
      task?.assigned_player_id === bundle.player_id &&
      task?.version === bundle.work_item_version
    );
    if (tasks.length !== 1) {
      throw new Error("Author work start requires one exact assigned task");
    }
    const [task] = tasks;
    if (!AUTHOR_START_TASK_STATES.has(task.status)) continue;
    if (!contractText(task.kind)) {
      throw new Error("Author work start requires one exact task kind");
    }
    const sameVersionProposal = records(paper?.proposals).some(proposal =>
      proposal?.work_item_id === bundle.work_item_id &&
      proposal?.expected_work_version === bundle.work_item_version
    );
    if (sameVersionProposal) continue;
    const start = validateAuthorWorkStart({
      schema: AUTHOR_WORK_START_SCHEMA,
      binding_id: bundle.binding_id,
      player_id: bundle.player_id,
      paper_id: bundle.paper_project_id,
      work_item_id: bundle.work_item_id,
      work_item_version: bundle.work_item_version,
      task_kind: task.kind,
      bundle,
    });
    if (!acknowledgedKeys.has(authorWorkStartKey(start))) starts.push(start);
  }
  return Object.freeze(starts.sort((left, right) =>
    authorWorkStartKey(left).localeCompare(authorWorkStartKey(right))
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

async function syncDirectory(path) {
  const handle = await open(
    path,
    fsConstants.O_RDONLY | (fsConstants.O_DIRECTORY ?? 0),
  );
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

function assertOwnedDirectory(path, stats, { privateDirectory }) {
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`${path} must be a non-symlink directory`);
  }
  if ((stats.mode & (privateDirectory ? 0o077 : 0o022)) !== 0) {
    throw new Error(`${path} has unsafe directory permissions`);
  }
  if (typeof process.getuid === "function" && stats.uid !== process.getuid()) {
    throw new Error(`${path} must be owned by the current user`);
  }
}

async function ensureMaterialRoot(root) {
  const exactRoot = resolve(root);
  const parent = dirname(exactRoot);
  assertOwnedDirectory(parent, await lstat(parent), { privateDirectory: false });
  if (await realpath(parent) !== parent) {
    throw new Error(`${parent} must not traverse a symbolic link`);
  }
  let created = false;
  try {
    await mkdir(exactRoot, { mode: 0o700 });
    created = true;
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
  }
  assertOwnedDirectory(exactRoot, await lstat(exactRoot), { privateDirectory: true });
  if (await realpath(exactRoot) !== exactRoot) {
    throw new Error(`${exactRoot} must not traverse a symbolic link`);
  }
  if (created) await syncDirectory(parent);
  return exactRoot;
}

export async function validateMaterialPublisherExecutable(
  path = MATERIAL_MOVE_EXECUTABLE,
) {
  const exactPath = resolve(path);
  let before;
  try {
    before = await lstat(exactPath);
  } catch {
    throw new Error("trusted challenge material publisher is unavailable");
  }
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1 ||
      before.uid !== 0 || (before.mode & 0o022) !== 0 ||
      (before.mode & 0o111) === 0 || await realpath(exactPath) !== exactPath) {
    throw new Error("trusted challenge material publisher is unsafe");
  }
  const handle = await open(
    exactPath,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    const opened = await handle.stat();
    if (!opened.isFile() || opened.dev !== before.dev || opened.ino !== before.ino ||
        opened.mode !== before.mode || opened.uid !== before.uid ||
        opened.nlink !== before.nlink) {
      throw new Error("trusted challenge material publisher changed while opening");
    }
  } finally {
    await handle.close();
  }
  return exactPath;
}

function materialDirectoryName(bundle) {
  return `${bundle.work_item_id}.v${bundle.work_item_version}.${bundle.bundle_hash.slice(7)}`;
}

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function readFrozenMaterial(path, descriptor) {
  const before = await lstat(path);
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1 ||
      (before.mode & 0o777) !== 0o400 ||
      (typeof process.getuid === "function" && before.uid !== process.getuid()) ||
      before.size !== descriptor.size_bytes) {
    throw new Error(`${path} is not an exact frozen challenge material`);
  }
  const handle = await open(
    path,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    const opened = await handle.stat();
    if (opened.dev !== before.dev || opened.ino !== before.ino ||
        opened.mode !== before.mode || opened.uid !== before.uid ||
        opened.size !== before.size || opened.nlink !== 1) {
      throw new Error(`${path} changed while it was opened`);
    }
    const bytes = await handle.readFile();
    const after = await handle.stat();
    if (after.dev !== opened.dev || after.ino !== opened.ino ||
        after.mode !== opened.mode || after.uid !== opened.uid ||
        after.size !== opened.size || after.nlink !== 1 ||
        bytes.length !== descriptor.size_bytes ||
        sha256Digest(bytes) !== descriptor.digest) {
      throw new Error(`${path} differs from frozen challenge authority`);
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

async function verifyMaterialDirectory(path, bundle) {
  const before = await lstat(path);
  assertOwnedDirectory(path, before, { privateDirectory: true });
  if ((before.mode & 0o777) !== 0o700) {
    throw new Error(`${path} must have mode 0700`);
  }
  const expectedTop = ["challenge"];
  const actualTop = (await readdir(path)).sort();
  if (actualTop.length !== expectedTop.length ||
      actualTop.some((value, index) => value !== expectedTop[index])) {
    throw new Error(`${path} contains an unexpected material entry`);
  }
  const challengePath = join(path, "challenge");
  const challengeBefore = await lstat(challengePath);
  assertOwnedDirectory(challengePath, challengeBefore, { privateDirectory: true });
  if ((challengeBefore.mode & 0o777) !== 0o700) {
    throw new Error(`${challengePath} must have mode 0700`);
  }
  const expectedFiles = bundle.objects.map(object => basename(object.logical_path)).sort();
  const actualFiles = (await readdir(challengePath)).sort();
  if (actualFiles.length !== expectedFiles.length ||
      actualFiles.some((value, index) => value !== expectedFiles[index])) {
    throw new Error(`${challengePath} does not contain the exact four materials`);
  }
  for (const descriptor of bundle.objects) {
    await readFrozenMaterial(join(path, descriptor.logical_path), descriptor);
  }
  const challengeAfter = await lstat(challengePath);
  const after = await lstat(path);
  if (after.dev !== before.dev || after.ino !== before.ino ||
      after.mode !== before.mode || after.uid !== before.uid ||
      challengeAfter.dev !== challengeBefore.dev ||
      challengeAfter.ino !== challengeBefore.ino ||
      challengeAfter.mode !== challengeBefore.mode ||
      challengeAfter.uid !== challengeBefore.uid) {
    throw new Error(`${path} changed while its material set was verified`);
  }
}

function materializationResult(bundle, directory, status) {
  return Object.freeze({
    schema: MATERIALIZATION_SCHEMA,
    status,
    directory,
    paper_id: bundle.paper_project_id,
    work_item_id: bundle.work_item_id,
    work_item_version: bundle.work_item_version,
    bundle_hash: bundle.bundle_hash,
    authority_hash: bundle.authority_hash,
    objects: Object.freeze(bundle.objects.map(object => Object.freeze({
      object_key: object.object_key,
      logical_path: object.logical_path,
      digest: object.digest,
      size_bytes: object.size_bytes,
      media_type: object.media_type,
    }))),
  });
}

async function validateAuthorExecutor(executor) {
  if (!executor || executor.schema !== AUTHOR_EXECUTOR_SCHEMA ||
      typeof executor.executable !== "string" || executor.executable.length === 0 ||
      !Number.isSafeInteger(executor.timeout_ms) ||
      executor.timeout_ms < 1_000 || executor.timeout_ms > 3_600_000) {
    throw new Error("Author work requires one configured v1 executor");
  }
  const executable = resolve(executor.executable);
  let before;
  try {
    before = await lstat(executable);
  } catch {
    throw new Error("configured Author executor is unavailable");
  }
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1 ||
      (before.mode & 0o111) === 0 || (before.mode & 0o022) !== 0 ||
      (typeof process.getuid === "function" && before.uid !== process.getuid()) ||
      await realpath(executable) !== executable) {
    throw new Error("configured Author executor is unsafe");
  }
  const handle = await open(
    executable,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    const opened = await handle.stat();
    if (!opened.isFile() || opened.dev !== before.dev || opened.ino !== before.ino ||
        opened.mode !== before.mode || opened.uid !== before.uid ||
        opened.nlink !== before.nlink) {
      throw new Error("configured Author executor changed while opening");
    }
  } finally {
    await handle.close();
  }
  return Object.freeze({
    executable,
    timeout_ms: executor.timeout_ms,
  });
}

function authorExecutorRequest(materialization, start) {
  const exactStart = validateAuthorWorkStart(start);
  if (!materialization || materialization.schema !== MATERIALIZATION_SCHEMA ||
      !["created", "reused"].includes(materialization.status) ||
      typeof materialization.directory !== "string" ||
      resolve(materialization.directory) !== materialization.directory ||
      materialization.paper_id !== exactStart.paper_id ||
      materialization.work_item_id !== exactStart.work_item_id ||
      materialization.work_item_version !== exactStart.work_item_version ||
      materialization.bundle_hash !== exactStart.bundle.bundle_hash ||
      materialization.authority_hash !== exactStart.bundle.authority_hash ||
      !Array.isArray(materialization.objects) || materialization.objects.length !== 4) {
    throw new Error("Author executor materialization does not match its work start");
  }
  return Object.freeze({
    schema: AUTHOR_EXECUTOR_REQUEST_SCHEMA,
    start_key: authorWorkStartKey(exactStart),
    material_directory: materialization.directory,
    binding_id: exactStart.binding_id,
    player_id: exactStart.player_id,
    paper_id: exactStart.paper_id,
    work_item_id: exactStart.work_item_id,
    work_item_version: exactStart.work_item_version,
    task_kind: exactStart.task_kind,
    bundle_hash: materialization.bundle_hash,
    authority_hash: materialization.authority_hash,
    objects: materialization.objects,
  });
}

function validateAuthorExecutorResult(value, request) {
  const keys = [
    "schema",
    "status",
    "start_key",
    "material_directory",
    "binding_id",
    "player_id",
    "paper_id",
    "work_item_id",
    "work_item_version",
    "task_kind",
    "bundle_hash",
    "authority_hash",
    "section_key",
    "artifact_manifest_id",
    "payload_hash",
  ];
  if (!exactKeys(value, keys) || value.schema !== AUTHOR_EXECUTOR_RESULT_SCHEMA ||
      value.status !== "completed" ||
      value.start_key !== request.start_key ||
      value.material_directory !== request.material_directory ||
      value.binding_id !== request.binding_id ||
      value.player_id !== request.player_id ||
      value.paper_id !== request.paper_id ||
      value.work_item_id !== request.work_item_id ||
      value.work_item_version !== request.work_item_version ||
      value.task_kind !== request.task_kind ||
      value.bundle_hash !== request.bundle_hash ||
      value.authority_hash !== request.authority_hash ||
      !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(String(value.section_key || "")) ||
      !canonicalUuid(value.artifact_manifest_id) ||
      !DIGEST_PATTERN.test(String(value.payload_hash || ""))) {
    throw new Error("Author executor result differs from its exact material work start");
  }
  return Object.freeze({
    paper_id: value.paper_id,
    work_item_id: value.work_item_id,
    section_key: value.section_key,
    artifact_manifest_id: value.artifact_manifest_id,
    payload_hash: value.payload_hash,
  });
}

function runAuthorProcess(executor, request, spawnImplementation) {
  return new Promise((resolveRun, rejectRun) => {
    let child;
    try {
      child = spawnImplementation(executor.executable, ["author-work-v1"], {
        cwd: request.material_directory,
        env: Object.freeze({
          LANG: "C.UTF-8",
          LC_ALL: "C.UTF-8",
          HEPTA_PAPER_RAID_MATERIAL_DIRECTORY: request.material_directory,
        }),
        stdio: ["pipe", "pipe", "pipe"],
      });
    } catch (error) {
      rejectRun(error);
      return;
    }
    const stdout = [];
    const stderr = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let settled = false;
    let timer;
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) rejectRun(error);
      else resolveRun(value);
    };
    const bounded = (chunks, field) => chunk => {
      const bytes = Buffer.from(chunk);
      if (field === "stdout") stdoutBytes += bytes.length;
      else stderrBytes += bytes.length;
      if (stdoutBytes > AUTHOR_EXECUTOR_MAX_OUTPUT_BYTES ||
          stderrBytes > AUTHOR_EXECUTOR_MAX_OUTPUT_BYTES) {
        child.kill("SIGKILL");
        finish(new Error(`Author executor ${field} exceeded its bounded output`));
        return;
      }
      chunks.push(bytes);
    };
    child.stdout.on("data", bounded(stdout, "stdout"));
    child.stderr.on("data", bounded(stderr, "stderr"));
    child.once("error", error => finish(error));
    child.once("close", (code, signal) => {
      if (code !== 0 || signal !== null) {
        finish(new Error("Author executor failed closed"));
        return;
      }
      finish(null, Buffer.concat(stdout).toString("utf8"));
    });
    timer = setTimeout(() => {
      child.kill("SIGKILL");
      finish(new Error("Author executor timed out"));
    }, executor.timeout_ms);
    child.stdin.on("error", error => finish(error));
    child.stdin.end(`${JSON.stringify(request)}\n`);
  });
}

/**
 * Start Author scientific work only after the exact four frozen materials are
 * atomically published. The executor receives that one directory through both
 * the canonical request and a dedicated environment variable. Its exact result
 * produces the five-field output descriptor from that material context. No
 * delivery draft exists until the executor has completed successfully.
 */
export async function executeAuthorWorkStart(
  config,
  materialization,
  start,
  { spawnImplementation = spawn } = {},
) {
  const executor = await validateAuthorExecutor(config?.author_executor);
  const request = authorExecutorRequest(materialization, start);
  const output = await runAuthorProcess(executor, request, spawnImplementation);
  let value;
  try {
    value = JSON.parse(output);
  } catch {
    throw new Error("Author executor did not return one JSON result");
  }
  return validateAuthorExecutorResult(value, request);
}

function sameDeliveryCandidate(left, right) {
  return [
    "schema",
    "delivery_draft_id",
    "binding_id",
    "paper_id",
    "work_item_id",
    "section_key",
    "lease_id",
    "lease_fencing_token",
    "expected_work_version",
    "parent_revision_id",
    "proposal_kind",
    "payload_hash",
    "artifact_manifest_id",
    "artifact_manifest_hash",
    "delivery_state",
    "declared_at_unix",
    "expires_at_unix",
  ].every(field => left[field] === right[field]);
}

export function deliveryCandidateForAuthorOutput(
  inbox,
  start,
  output,
  prepared,
) {
  const exactStart = validateAuthorWorkStart(start);
  if (!exactKeys(output, [
    "paper_id",
    "work_item_id",
    "section_key",
    "artifact_manifest_id",
    "payload_hash",
  ]) || output.paper_id !== exactStart.paper_id ||
      output.work_item_id !== exactStart.work_item_id ||
      !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(String(output.section_key || "")) ||
      !canonicalUuid(output.artifact_manifest_id) ||
      !DIGEST_PATTERN.test(String(output.payload_hash || ""))) {
    throw new Error("Author output descriptor does not match its work start");
  }
  if (!exactKeys(prepared, ["schema", "candidate"]) ||
      prepared.schema !== "hepta.paper_raid.agent_bridge.delivery_draft_result.v1") {
    throw new Error("delivery draft result is invalid");
  }
  const preparedCandidate = validateCandidate(
    prepared.candidate,
    exactStart.paper_id,
    exactStart.binding_id,
  );
  const paper = records(inbox?.papers).find(item => item?.paper_id === exactStart.paper_id);
  const assignedTasks = records(paper?.tasks).filter(task =>
    task?.work_item_id === exactStart.work_item_id &&
    task?.version === exactStart.work_item_version &&
    task?.assigned_binding_id === exactStart.binding_id &&
    task?.assigned_player_id === exactStart.player_id &&
    AUTHOR_START_TASK_STATES.has(task?.status)
  );
  if (assignedTasks.length !== 1) {
    throw new Error("fresh inbox no longer contains the exact Author work assignment");
  }
  const matches = deliveryCandidates(inbox).filter(candidate =>
    candidate.binding_id === exactStart.binding_id &&
    candidate.paper_id === exactStart.paper_id &&
    candidate.work_item_id === exactStart.work_item_id &&
    candidate.expected_work_version === exactStart.work_item_version &&
    candidate.section_key === output.section_key &&
    candidate.artifact_manifest_id === output.artifact_manifest_id &&
    candidate.payload_hash === output.payload_hash &&
    candidate.delivery_state === "pending"
  );
  if (matches.length !== 1 || !sameDeliveryCandidate(matches[0], preparedCandidate)) {
    throw new Error("fresh inbox delivery candidate differs from the prepared Author output");
  }
  return matches[0];
}

/**
 * Publish the exact four authority bytes as one task-specific local input.
 * Hidden pending directories are never consumer-visible. GNU mv's
 * no-copy/no-clobber path is required; if it cannot rename,
 * this function either verifies an already identical publication or fails.
 */
export async function materializeAssignedChallengeMaterials(
  bundle,
  downloaded,
  root,
) {
  const exactBundle = validateChallengeMaterialBundle(bundle);
  if (!Array.isArray(downloaded) || downloaded.length !== exactBundle.objects.length) {
    throw new Error("challenge material download set is incomplete");
  }
  const exactDownloads = exactBundle.objects.map((descriptor, index) => {
    const item = downloaded[index];
    if (!item || item.descriptor?.object_key !== descriptor.object_key ||
        item.descriptor?.digest !== descriptor.digest) {
      throw new Error("challenge material download order differs from authority");
    }
    const bytes = Buffer.from(item.bytes);
    if (bytes.length !== descriptor.size_bytes || sha256Digest(bytes) !== descriptor.digest) {
      throw new Error("challenge material download differs from frozen authority");
    }
    return Object.freeze({ descriptor, bytes });
  });
  const moveExecutable = await validateMaterialPublisherExecutable();
  const exactRoot = await ensureMaterialRoot(root);
  const finalPath = join(exactRoot, materialDirectoryName(exactBundle));
  if (await pathExists(finalPath)) {
    await verifyMaterialDirectory(finalPath, exactBundle);
    return materializationResult(exactBundle, finalPath, "reused");
  }
  const pendingPath = join(
    exactRoot,
    `.${materialDirectoryName(exactBundle)}.pending.${process.pid}.${randomBytes(12).toString("hex")}`,
  );
  await mkdir(pendingPath, { mode: 0o700 });
  let published = false;
  try {
    const challengePath = join(pendingPath, "challenge");
    await mkdir(challengePath, { mode: 0o700 });
    for (const { descriptor, bytes } of exactDownloads) {
      const path = join(pendingPath, descriptor.logical_path);
      const handle = await open(
        path,
        fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL |
          (fsConstants.O_NOFOLLOW ?? 0),
        0o400,
      );
      try {
        await handle.writeFile(bytes);
        await handle.sync();
      } finally {
        await handle.close();
      }
      await readFrozenMaterial(path, descriptor);
    }
    await syncDirectory(challengePath);
    await syncDirectory(pendingPath);
    try {
      await execFileAsync(
        moveExecutable,
        ["--no-copy", "--no-clobber", "--no-target-directory", pendingPath, finalPath],
        { timeout: 5_000, maxBuffer: 64 * 1024 },
      );
    } catch (error) {
      // Coreutils versions differ on whether --no-clobber reports an
      // existing destination as success or failure. Only an exact verified
      // winner makes that race recoverable; every other mv failure remains
      // fatal and the hidden pending directory is removed below.
      if (!(await pathExists(pendingPath)) || !(await pathExists(finalPath))) {
        throw error;
      }
      await verifyMaterialDirectory(finalPath, exactBundle);
    }
    published = !(await pathExists(pendingPath));
    await verifyMaterialDirectory(finalPath, exactBundle);
    await syncDirectory(exactRoot);
    return materializationResult(exactBundle, finalPath, published ? "created" : "reused");
  } finally {
    if (!published && await pathExists(pendingPath)) {
      await rm(pendingPath, { recursive: true, force: false });
      await syncDirectory(exactRoot);
    }
  }
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
      : candidate.schema === AUTHOR_WORK_START_SCHEMA
        ? authorWorkStartKey(candidate)
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
