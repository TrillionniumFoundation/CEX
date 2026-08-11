import { createHash } from "node:crypto";
import { constants as fsConstants } from "node:fs";
import { mkdtemp, mkdir, open, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

import {
  REVIEW_EXECUTION_RECEIPT_SCHEMA,
  assertCanonicalUuid,
  assertDigest,
  assertLogicalId,
  canonicalJsonBytes,
  reviewExecutionReceiptFrame,
  sha256Digest,
} from "./canonical.mjs";
import { AGENT_BRIDGE_ENDPOINTS } from "./client.mjs";

export const REVIEW_TASKS_SCHEMA =
  "hepta.paper_raid.agent_bridge.review_tasks.v1";
export const REVIEW_TASK_SCHEMA =
  "hepta.paper_raid.agent_bridge.review_task.v1";
export const FROZEN_REVIEW_BUNDLE_SCHEMA =
  "hepta.paper_raid.resolved_frozen_review_bundle.v1";
export const FROZEN_REVIEW_AUTHORITY_SCHEMA =
  "hepta.paper_raid.frozen_review_authority.v1";
export const REVIEW_EXECUTION_POLICY_SCHEMA =
  "hepta.paper_raid.review_execution_policy.v1";
export const REVIEW_EXECUTION_PLAN_SCHEMA =
  "hepta.paper_raid.review_execution_plan.v1";
export const REVIEW_RECEIPT_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.review_receipt_request.v1";

const REVIEW_TASK_STATES = new Set(["pending", "submitting", "consumed"]);
const REVIEW_ROLE_KIND = new Map([
  ["evaluator", "evaluate"],
  ["reproducer", "reproduce"],
]);
const OBJECT_ROLES = new Set([
  "candidate",
  "dataset",
  "evaluator_support",
  "frozen_evaluator",
  "input",
]);
const MAX_OBJECTS = 64;
const MAX_OBJECT_BYTES = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES = 64 * 1024 * 1024;
const MAX_OUTPUT_BYTES = 256 * 1024;
const MAX_LOG_BYTES = 256 * 1024;
const PYTHON_RUNTIME = "/usr/bin/python3";
const PYTHON_ADAPTER = "python3-stdlib-v1";
const PYTHON_ENTRYPOINT = "evaluator/main.py";
const PYTHON_LOADER_SOURCE = fileURLToPath(new URL(
  "./frozen_python_loader.py",
  import.meta.url,
));
const PYTHON_LOADER_DIGEST =
  "sha256:8cba01ffa388fe0d71caa9237b4d6a63700766be9e05a4ee7bc3b9cfd6099df3";
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/;
const LOGICAL_PATH = /^[a-z0-9][a-z0-9._-]*(?:\/[a-z0-9][a-z0-9._-]*){0,7}$/;

// Only these audited evaluator bytes may execute. A bundle cannot select a
// binary, module, argv vector, or arbitrary source digest. Adding an evaluator
// therefore requires a reviewed Bridge release, not a server-side descriptor.
const FROZEN_EVALUATORS = new Map([
  [
    "sha256:b50f61282b4e89797c25c343fef3077d2bf0dc8713ffedcb5ecbe26d15258392",
    Object.freeze({ support: Object.freeze([]) }),
  ],
  [
    "sha256:cf34d96f7f0075d4a7e60cdc49fee5ecd15b9d29bcb19eb935c9ec32e2e96eea",
    Object.freeze({
      support: Object.freeze([
        "sha256:f0ca6e845f87f5344b8763490ad18e1c436c9da6acf36e9c5b7c2464f5071393",
      ]),
    }),
  ],
  [
    "sha256:3c256c75533b8154305b821cb30de3c5dd4c3f575ed1f72e6513320d87eddbf4",
    Object.freeze({
      support: Object.freeze([
        "sha256:199d18b168d6e09d46b75c3108757aaffbb2426cf758a71951c6b51451b58422",
      ]),
    }),
  ],
]);

function exactKeys(value, expected, field) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${field} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new Error(`${field} contains unsupported or missing fields`);
  }
  return value;
}

function contractText(value, field, max = 256) {
  if (
    typeof value !== "string" ||
    value.length < 1 ||
    [...value].length > max ||
    /[\u0000-\u001f\u007f]/u.test(value)
  ) {
    throw new Error(`${field} is invalid`);
  }
  return value;
}

function positiveInteger(value, field) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${field} must be a positive safe integer`);
  }
  return value;
}

function nonNegativeInteger(value, field) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${field} must be a non-negative safe integer`);
  }
  return value;
}

function validateLogicalPath(value, field) {
  if (
    typeof value !== "string" ||
    value.length > 192 ||
    !LOGICAL_PATH.test(value) ||
    value.split("/").some(segment => segment === "." || segment === "..")
  ) {
    throw new Error(`${field} is not a safe relative logical path`);
  }
  return value;
}

function validateObjectShape(value, index, field) {
  exactKeys(value, [
    "object_key",
    "logical_path",
    "role",
    "digest",
    "size_bytes",
    "media_type",
    "download_path",
  ], `${field}[${index}]`);
  assertLogicalId(value.object_key, `${field}[${index}].object_key`);
  validateLogicalPath(value.logical_path, `${field}[${index}].logical_path`);
  if (!OBJECT_ROLES.has(value.role)) {
    throw new Error(`${field}[${index}].role is unsupported`);
  }
  assertDigest(value.digest, `${field}[${index}].digest`);
  positiveInteger(value.size_bytes, `${field}[${index}].size_bytes`);
  if (value.size_bytes > MAX_OBJECT_BYTES) {
    throw new Error(`${field}[${index}].size_bytes exceeds the Bridge limit`);
  }
  contractText(value.media_type, `${field}[${index}].media_type`, 128);
  if (
    (["frozen_evaluator", "evaluator_support"].includes(value.role) &&
      value.media_type !== "text/x-python; charset=utf-8") ||
    (value.role === "candidate" && value.media_type !== "application/json") ||
    (value.role === "dataset" &&
      !["application/json", "text/csv; charset=utf-8"].includes(value.media_type))
  ) {
    throw new Error(`${field}[${index}].media_type is invalid for its role`);
  }
  if (value.download_path !== AGENT_BRIDGE_ENDPOINTS.review_objects) {
    throw new Error(`${field}[${index}].download_path is not the fixed review route`);
  }
  return Object.freeze({ ...value });
}

function transportPath(role, mediaType) {
  if (role === "frozen_evaluator" && mediaType === "text/x-python; charset=utf-8") {
    return PYTHON_ENTRYPOINT;
  }
  if (role === "evaluator_support" && mediaType === "text/x-python; charset=utf-8") {
    return "evaluator/baseline.py";
  }
  if (role === "candidate" && mediaType === "application/json") {
    return "inputs/candidate.json";
  }
  if (role === "dataset" && mediaType === "application/json") {
    return "inputs/dataset.json";
  }
  if (role === "dataset" && mediaType === "text/csv; charset=utf-8") {
    return "inputs/dataset.csv";
  }
  return null;
}

function validateAuthorityObject(value, index) {
  return validateObjectShape(value, index, "authority.artifact_objects");
}

function validateObject(value, index) {
  const object = validateObjectShape(value, index, "bundle.objects");
  if (object.logical_path !== transportPath(object.role, object.media_type)) {
    throw new Error(`bundle.objects[${index}].logical_path is not deterministic`);
  }
  return object;
}

function validateExecution(value, kind) {
  exactKeys(value, [
    "schema",
    "kind",
    "adapter",
    "evaluator_version",
    "entrypoint",
    "timeout_ms",
    "seed",
  ], "frozen bundle execution");
  if (
    value.schema !== REVIEW_EXECUTION_PLAN_SCHEMA ||
    value.kind !== kind ||
    value.adapter !== PYTHON_ADAPTER ||
    value.entrypoint !== PYTHON_ENTRYPOINT
  ) {
    throw new Error("frozen bundle execution plan is unsupported");
  }
  contractText(value.evaluator_version, "execution.evaluator_version", 128);
  if (
    !Number.isSafeInteger(value.timeout_ms) ||
    value.timeout_ms < 500 ||
    value.timeout_ms > 30_000
  ) {
    throw new Error("execution.timeout_ms must be between 500 and 30000");
  }
  nonNegativeInteger(value.seed, "execution.seed");
  return Object.freeze({ ...value });
}

function authorityHashFrame(authority) {
  return {
    schema: authority.schema,
    assignment_id: authority.assignment_id,
    paper_project_id: authority.paper_project_id,
    submission_id: authority.submission_id,
    review_round: authority.review_round,
    slot: authority.slot,
    assignment_version: authority.assignment_version,
    expires_at: authority.expires_at,
    release_candidate_hash: authority.release_candidate_hash,
    paper_bundle_hash: authority.paper_bundle_hash,
    artifact_manifest_hash: authority.artifact_manifest_hash,
    evaluator_manifest_hash: authority.evaluator_manifest_hash,
    dataset_manifest_hash: authority.dataset_manifest_hash,
    artifact_objects: authority.artifact_objects,
    execution_policy: authority.execution_policy,
  };
}

export function frozenReviewAuthorityHash(authority) {
  return sha256Digest(canonicalJsonBytes(authorityHashFrame(authority)));
}

function bundleHashFrame(bundle) {
  return {
    schema: bundle.schema,
    authority: bundle.authority,
    authority_hash: bundle.authority_hash,
    assignment_id: bundle.assignment_id,
    paper_project_id: bundle.paper_project_id,
    submission_id: bundle.submission_id,
    review_round: bundle.review_round,
    slot: bundle.slot,
    assignment_version: bundle.assignment_version,
    expires_at: bundle.expires_at,
    release_candidate_hash: bundle.release_candidate_hash,
    paper_bundle_hash: bundle.paper_bundle_hash,
    artifact_manifest_hash: bundle.artifact_manifest_hash,
    evaluator_manifest_hash: bundle.evaluator_manifest_hash,
    dataset_manifest_hash: bundle.dataset_manifest_hash,
    objects: bundle.objects,
    execution: bundle.execution,
  };
}

export function frozenReviewBundleHash(bundle) {
  return sha256Digest(canonicalJsonBytes(bundleHashFrame(bundle)));
}

function validateExecutionPolicy(value, kind) {
  exactKeys(value, [
    "schema",
    "kind",
    "adapter",
    "timeout_ms",
    "seed",
  ], "frozen review authority execution_policy");
  if (
    value.schema !== REVIEW_EXECUTION_POLICY_SCHEMA ||
    value.kind !== kind ||
    value.adapter !== PYTHON_ADAPTER ||
    !Number.isSafeInteger(value.timeout_ms) ||
    value.timeout_ms < 500 ||
    value.timeout_ms > 30_000
  ) {
    throw new Error("frozen review authority execution policy is unsupported");
  }
  nonNegativeInteger(value.seed, "authority.execution_policy.seed");
  return Object.freeze({ ...value });
}

function validateAuthority(value, task) {
  exactKeys(value, [
    "schema",
    "authority_hash",
    "assignment_id",
    "paper_project_id",
    "submission_id",
    "review_round",
    "slot",
    "assignment_version",
    "expires_at",
    "release_candidate_hash",
    "paper_bundle_hash",
    "artifact_manifest_hash",
    "evaluator_manifest_hash",
    "dataset_manifest_hash",
    "artifact_objects",
    "execution_policy",
  ], "frozen review authority");
  if (value.schema !== FROZEN_REVIEW_AUTHORITY_SCHEMA) {
    throw new Error("frozen review authority schema is unsupported");
  }
  for (const field of ["assignment_id", "paper_project_id", "submission_id"]) {
    assertCanonicalUuid(value[field], `authority.${field}`);
  }
  for (const field of [
    "authority_hash",
    "release_candidate_hash",
    "paper_bundle_hash",
    "artifact_manifest_hash",
    "evaluator_manifest_hash",
    "dataset_manifest_hash",
  ]) {
    assertDigest(value[field], `authority.${field}`);
  }
  positiveInteger(value.review_round, "authority.review_round");
  positiveInteger(value.assignment_version, "authority.assignment_version");
  if (
    typeof value.expires_at !== "string" ||
    !RFC3339.test(value.expires_at) ||
    Number.isNaN(Date.parse(value.expires_at))
  ) {
    throw new Error("authority.expires_at must be canonical UTC RFC3339");
  }
  const expectedSlot = task.role === "evaluator" ? "evaluator" : "reproducer";
  if (value.slot !== expectedSlot) {
    throw new Error("frozen review authority slot does not match the review role");
  }
  if (
    value.assignment_id !== task.assignment_id ||
    value.paper_project_id !== task.paper_id ||
    value.assignment_version !== task.fencing_token
  ) {
    throw new Error("frozen review authority does not match the review task");
  }
  if (
    !Array.isArray(value.artifact_objects) ||
    value.artifact_objects.length < 3 ||
    value.artifact_objects.length > MAX_OBJECTS
  ) {
    throw new Error("frozen review authority must contain 3 to 64 artifact objects");
  }
  const artifactObjects = value.artifact_objects.map(validateAuthorityObject);
  validateSortedObjects(artifactObjects, "frozen review authority");
  const executionPolicy = validateExecutionPolicy(value.execution_policy, task.kind);
  const authority = Object.freeze({
    ...value,
    artifact_objects: Object.freeze(artifactObjects),
    execution_policy: executionPolicy,
  });
  if (frozenReviewAuthorityHash(authority) !== value.authority_hash) {
    throw new Error("frozen review authority hash does not match canonical authority bytes");
  }
  return authority;
}

function validateSortedObjects(objects, field) {
  let totalBytes = 0;
  const objectKeys = new Set();
  const logicalPaths = new Set();
  let previous = null;
  for (const object of objects) {
    totalBytes += object.size_bytes;
    if (totalBytes > MAX_TOTAL_BYTES) {
      throw new Error(`${field} exceeds the total byte limit`);
    }
    if (objectKeys.has(object.object_key) || logicalPaths.has(object.logical_path)) {
      throw new Error(`${field} object keys and paths must be unique`);
    }
    const ordering = `${object.object_key}\0${object.logical_path}`;
    if (previous !== null && previous >= ordering) {
      throw new Error(`${field} objects must be sorted by object_key/path`);
    }
    objectKeys.add(object.object_key);
    logicalPaths.add(object.logical_path);
    previous = ordering;
  }
}

function validateBundle(value, task) {
  exactKeys(value, [
    "schema",
    "bundle_hash",
    "authority",
    "authority_hash",
    "assignment_id",
    "paper_project_id",
    "submission_id",
    "review_round",
    "slot",
    "assignment_version",
    "expires_at",
    "release_candidate_hash",
    "paper_bundle_hash",
    "artifact_manifest_hash",
    "evaluator_manifest_hash",
    "dataset_manifest_hash",
    "objects",
    "execution",
  ], "frozen review bundle");
  if (value.schema !== FROZEN_REVIEW_BUNDLE_SCHEMA) {
    throw new Error("frozen review bundle schema is unsupported");
  }
  for (const field of ["assignment_id", "paper_project_id", "submission_id"]) {
    assertCanonicalUuid(value[field], `bundle.${field}`);
  }
  for (const field of [
    "bundle_hash",
    "authority_hash",
    "release_candidate_hash",
    "paper_bundle_hash",
    "artifact_manifest_hash",
    "evaluator_manifest_hash",
    "dataset_manifest_hash",
  ]) {
    assertDigest(value[field], `bundle.${field}`);
  }
  const authority = validateAuthority(value.authority, task);
  positiveInteger(value.review_round, "bundle.review_round");
  assertLogicalId(value.slot, "bundle.slot");
  positiveInteger(value.assignment_version, "bundle.assignment_version");
  if (
    typeof value.expires_at !== "string" ||
    !RFC3339.test(value.expires_at) ||
    Number.isNaN(Date.parse(value.expires_at))
  ) {
    throw new Error("bundle.expires_at must be canonical UTC RFC3339");
  }
  if (!Array.isArray(value.objects) || value.objects.length < 3 || value.objects.length > MAX_OBJECTS) {
    throw new Error("frozen review bundle must contain 3 to 64 objects");
  }
  const objects = value.objects.map(validateObject);
  validateSortedObjects(objects, "frozen review bundle");
  const execution = validateExecution(value.execution, task.kind);
  if (
    value.assignment_id !== task.assignment_id ||
    value.paper_project_id !== task.paper_id ||
    value.assignment_version !== task.fencing_token
  ) {
    throw new Error("frozen review bundle does not match the review task authority");
  }
  const expectedSlot = task.role === "evaluator" ? "evaluator" : "reproducer";
  if (value.slot !== expectedSlot) {
    throw new Error("frozen review bundle slot does not match the review role");
  }
  for (const field of [
    "authority_hash",
    "assignment_id",
    "paper_project_id",
    "submission_id",
    "review_round",
    "slot",
    "assignment_version",
    "expires_at",
    "release_candidate_hash",
    "paper_bundle_hash",
    "artifact_manifest_hash",
    "evaluator_manifest_hash",
    "dataset_manifest_hash",
  ]) {
    if (value[field] !== authority[field]) {
      throw new Error(`resolved review bundle ${field} disagrees with its authority`);
    }
  }
  if (objects.length !== authority.artifact_objects.length) {
    throw new Error("resolved review object count differs from Hepta authority objects");
  }
  for (let index = 0; index < objects.length; index += 1) {
    const resolved = objects[index];
    const source = authority.artifact_objects[index];
    if (
      resolved.object_key !== source.object_key ||
      resolved.role !== source.role ||
      resolved.digest !== source.digest ||
      resolved.size_bytes !== source.size_bytes ||
      resolved.media_type !== source.media_type ||
      resolved.download_path !== source.download_path
    ) {
      throw new Error("resolved review object differs from Hepta authority object");
    }
  }
  if (
    execution.kind !== authority.execution_policy.kind ||
    execution.adapter !== authority.execution_policy.adapter ||
    execution.timeout_ms !== authority.execution_policy.timeout_ms ||
    execution.seed !== authority.execution_policy.seed
  ) {
    throw new Error("resolved execution plan disagrees with its authority policy");
  }
  const entrypoints = objects.filter(object => object.role === "frozen_evaluator");
  const datasets = objects.filter(object => object.role === "dataset");
  const candidates = objects.filter(object => object.role === "candidate");
  if (
    entrypoints.length !== 1 ||
    entrypoints[0].logical_path !== PYTHON_ENTRYPOINT ||
    datasets.length !== 1 ||
    candidates.length !== 1
  ) {
    throw new Error("frozen Python adapter requires one evaluator, dataset, and candidate");
  }
  const allowlist = FROZEN_EVALUATORS.get(entrypoints[0].digest);
  if (!allowlist) {
    throw new Error("frozen evaluator digest is not in this Bridge release allowlist");
  }
  if (execution.evaluator_version !== entrypoints[0].digest) {
    throw new Error("execution evaluator_version does not bind the frozen entrypoint bytes");
  }
  const supportDigests = objects
    .filter(object => object.role === "evaluator_support")
    .map(object => object.digest)
    .sort();
  if (
    supportDigests.length !== allowlist.support.length ||
    supportDigests.some((digest, index) => digest !== allowlist.support[index])
  ) {
    throw new Error("frozen evaluator support objects do not match the allowlist");
  }
  const bundle = Object.freeze({
    ...value,
    authority,
    objects: Object.freeze(objects),
    execution,
  });
  if (frozenReviewBundleHash(bundle) !== value.bundle_hash) {
    throw new Error("frozen review bundle hash does not match canonical bundle bytes");
  }
  return bundle;
}

export function validateReviewTask(value, paperId, bindingId, { nowUnix } = {}) {
  exactKeys(value, [
    "schema",
    "task_id",
    "assignment_id",
    "paper_id",
    "evaluation_id",
    "role",
    "kind",
    "attempt",
    "fencing_token",
    "state",
    "bundle",
  ], "review task");
  if (value.schema !== REVIEW_TASK_SCHEMA) {
    throw new Error("review task schema is unsupported");
  }
  for (const field of ["task_id", "assignment_id", "paper_id"]) {
    assertCanonicalUuid(value[field], field);
  }
  if (value.paper_id !== paperId) {
    throw new Error("review task crosses the inbox Paper boundary");
  }
  if (!REVIEW_ROLE_KIND.has(value.role) || REVIEW_ROLE_KIND.get(value.role) !== value.kind) {
    throw new Error("review task role/kind pair is unsupported");
  }
  assertCanonicalUuid(value.evaluation_id, "evaluation_id");
  positiveInteger(value.attempt, "attempt");
  positiveInteger(value.fencing_token, "fencing_token");
  if (!REVIEW_TASK_STATES.has(value.state)) {
    throw new Error("review task state is unsupported");
  }
  const task = {
    schema: value.schema,
    task_id: value.task_id,
    assignment_id: value.assignment_id,
    paper_id: value.paper_id,
    evaluation_id: value.evaluation_id,
    role: value.role,
    kind: value.kind,
    attempt: value.attempt,
    fencing_token: value.fencing_token,
    state: value.state,
  };
  const bundle = validateBundle(value.bundle, task);
  if (
    value.state === "pending" &&
    Number.isSafeInteger(nowUnix) &&
    Date.parse(bundle.expires_at) <= nowUnix * 1000
  ) {
    throw new Error("pending review task contains an expired frozen bundle");
  }
  return Object.freeze({ ...task, binding_id: bindingId, bundle });
}

export function reviewTaskKey(task) {
  return [
    task.task_id,
    task.assignment_id,
    task.paper_id,
    task.kind,
    task.attempt,
    task.fencing_token,
    task.bundle.bundle_hash,
  ].join(":");
}

export function reviewTasks(
  inbox,
  { nowUnix = Math.floor(Date.now() / 1000) } = {},
) {
  if (
    !inbox ||
    inbox.schema !== "hepta.paper_raid.agent_bridge.inbox.v2" ||
    !Array.isArray(inbox.papers)
  ) {
    throw new Error("Agent Bridge inbox schema is unsupported");
  }
  assertCanonicalUuid(inbox.binding_id, "inbox.binding_id");
  const result = [];
  const seen = new Set();
  for (const paper of inbox.papers) {
    if (!paper || typeof paper !== "object" || Array.isArray(paper)) {
      throw new Error("Agent Bridge inbox paper projection is invalid");
    }
    assertCanonicalUuid(paper.paper_id, "inbox.paper_id");
    if (paper.review_tasks === undefined) continue;
    const projection = paper.review_tasks;
    exactKeys(projection, ["schema", "status", "reason_code", "items"], "review task projection");
    if (projection.schema !== REVIEW_TASKS_SCHEMA || !Array.isArray(projection.items)) {
      throw new Error("review task projection is unsupported");
    }
    if (projection.status === "unavailable") {
      if (projection.items.length !== 0) {
        throw new Error("unavailable review task projection must contain no items");
      }
      contractText(projection.reason_code, "review task unavailable reason", 128);
      continue;
    }
    if (
      projection.status !== "available" ||
      projection.reason_code !== null ||
      projection.items.length === 0
    ) {
      throw new Error("available review task projection is invalid");
    }
    for (const item of projection.items) {
      const task = validateReviewTask(
        item,
        paper.paper_id,
        inbox.binding_id,
        { nowUnix },
      );
      const key = reviewTaskKey(task);
      if (seen.has(key)) {
        throw new Error("review task projection contains a duplicate authority tuple");
      }
      seen.add(key);
      result.push(task);
    }
  }
  return result.sort((left, right) => reviewTaskKey(left).localeCompare(reviewTaskKey(right)));
}

export function actionableReviewTasks(inbox, acknowledged = new Set(), options) {
  if (!(acknowledged instanceof Set)) {
    throw new Error("acknowledged review task keys must be a Set");
  }
  return reviewTasks(inbox, options).filter(task =>
    task.state === "pending" && !acknowledged.has(reviewTaskKey(task))
  );
}

export function formatReviewTask(task, index) {
  return `${index + 1}. ${task.kind} paper ${task.paper_id} · assignment ${task.assignment_id} · bundle ${task.bundle.bundle_hash} · attempt ${task.attempt}/${task.fencing_token}`;
}

function encodedQueryValue(value) {
  return encodeURIComponent(value).replace(/[!'()*]/g, character =>
    `%${character.charCodeAt(0).toString(16).toUpperCase()}`
  );
}

export function reviewObjectQuery(task, object) {
  const pairs = [
    ["assignment_id", task.assignment_id],
    ["bundle_hash", task.bundle.bundle_hash],
    ["digest", object.digest],
    ["object_key", object.object_key],
    ["task_id", task.task_id],
  ];
  return pairs
    .map(([key, value]) => `${encodedQueryValue(key)}=${encodedQueryValue(value)}`)
    .sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)))
    .join("&");
}

export async function downloadFrozenReviewObjects(task, downloadObject) {
  if (typeof downloadObject !== "function") {
    throw new Error("review object downloader is unavailable");
  }
  const downloaded = [];
  for (const object of task.bundle.objects) {
    const bytes = Buffer.from(await downloadObject(task, object));
    if (bytes.length !== object.size_bytes || sha256Digest(bytes) !== object.digest) {
      throw new Error(`frozen review object ${object.object_key} failed size/digest verification`);
    }
    downloaded.push(Object.freeze({ descriptor: object, bytes }));
  }
  return Object.freeze(downloaded);
}

function stableUuid(domain, fields) {
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

export function reviewExecutionReceiptId(
  bindingId,
  taskId,
  assignmentId,
  bundleHash,
  attempt,
  fencingToken,
) {
  assertCanonicalUuid(bindingId, "receipt binding_id");
  assertCanonicalUuid(taskId, "receipt task_id");
  assertCanonicalUuid(assignmentId, "receipt assignment_id");
  assertDigest(bundleHash, "receipt bundle_hash");
  positiveInteger(attempt, "receipt attempt");
  positiveInteger(fencingToken, "receipt fencing_token");
  return stableUuid(
    "hepta.paper_raid.review_execution_receipt_id.v1",
    [
      bindingId,
      taskId,
      assignmentId,
      bundleHash,
      String(attempt),
      String(fencingToken),
    ],
  );
}

function capture(stream, maxBytes, child, label) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let length = 0;
    stream.on("data", chunk => {
      const bytes = Buffer.from(chunk);
      length += bytes.length;
      if (length > maxBytes) {
        child.kill("SIGKILL");
        reject(new Error(`${label} exceeded its byte limit`));
        return;
      }
      chunks.push(bytes);
    });
    stream.once("end", () => resolve(Buffer.concat(chunks)));
    stream.once("error", () => reject(new Error(`${label} stream failed`)));
  });
}

async function pinnedPythonLoaderBytes() {
  let handle;
  try {
    handle = await open(
      PYTHON_LOADER_SOURCE,
      fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW,
    );
    const metadata = await handle.stat();
    if (!metadata.isFile() || metadata.nlink !== 1 || (metadata.mode & 0o022) !== 0) {
      throw new Error("pinned Python loader file ownership boundary is invalid");
    }
    const bytes = await handle.readFile();
    if (sha256Digest(bytes) !== PYTHON_LOADER_DIGEST) {
      throw new Error("pinned Python loader bytes do not match this Bridge release");
    }
    return bytes;
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("pinned Python loader")) {
      throw error;
    }
    throw new Error("pinned Python loader is unavailable");
  } finally {
    await handle?.close();
  }
}

function validateAdapterDownloads(task, downloaded) {
  if (!Array.isArray(downloaded) || downloaded.length !== task.bundle.objects.length) {
    throw new Error("frozen adapter input set does not match the bundle");
  }
  for (let index = 0; index < downloaded.length; index += 1) {
    const item = downloaded[index];
    const expected = task.bundle.objects[index];
    if (
      !item ||
      canonicalJsonBytes(item.descriptor).compare(canonicalJsonBytes(expected)) !== 0 ||
      !Buffer.isBuffer(item.bytes) ||
      item.bytes.length !== expected.size_bytes ||
      sha256Digest(item.bytes) !== expected.digest
    ) {
      throw new Error("frozen adapter input bytes or descriptor differ from the bundle");
    }
  }
}

async function runProcess(executable, args, options, timeoutMs, spawnImplementation) {
  const monotonicStart = performance.now();
  const child = spawnImplementation(executable, args, options);
  const stdoutPromise = capture(child.stdout, MAX_OUTPUT_BYTES, child, "frozen evaluator stdout");
  const stderrPromise = capture(child.stderr, MAX_LOG_BYTES, child, "frozen evaluator stderr");
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    child.kill("SIGKILL");
  }, timeoutMs);
  const exit = await new Promise((resolve, reject) => {
    child.once("error", () => reject(new Error("frozen evaluator could not start")));
    child.once("close", (code, signal) => resolve({ code, signal }));
  }).finally(() => clearTimeout(timer));
  const [stdout, stderr] = await Promise.all([stdoutPromise, stderrPromise]);
  if (timedOut) throw new Error("frozen evaluator timed out");
  if (exit.signal !== null || ![0, 1].includes(exit.code)) {
    throw new Error("frozen evaluator exited outside its bounded result contract");
  }
  return {
    ...exit,
    stdout,
    stderr,
    elapsed_ms: Math.ceil(performance.now() - monotonicStart),
  };
}

export async function runFrozenPythonAdapter(
  task,
  downloaded,
  {
    spawnImplementation = spawn,
    nowUnix = () => Math.floor(Date.now() / 1000),
  } = {},
) {
  if (task.bundle.execution.adapter !== PYTHON_ADAPTER) {
    throw new Error("review execution adapter is unsupported");
  }
  validateAdapterDownloads(task, downloaded);
  const loaderBytes = await pinnedPythonLoaderBytes();
  const directory = await mkdtemp(join(tmpdir(), "paper-raid-frozen-review-"));
  try {
    for (const item of downloaded) {
      const destination = join(directory, ...item.descriptor.logical_path.split("/"));
      await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
      await writeFile(destination, item.bytes, { flag: "wx", mode: 0o400 });
    }
    const entrypoint = downloaded.find(item => item.descriptor.role === "frozen_evaluator");
    const dataset = downloaded.find(item => item.descriptor.role === "dataset");
    const candidate = downloaded.find(item => item.descriptor.role === "candidate");
    const support = downloaded.filter(item => item.descriptor.role === "evaluator_support");
    const loader = join(directory, "runtime", "frozen_python_loader.py");
    await mkdir(dirname(loader), { recursive: true, mode: 0o700 });
    await writeFile(loader, loaderBytes, { flag: "wx", mode: 0o400 });
    const startedAtUnix = nowUnix();
    const execution = await runProcess(
      PYTHON_RUNTIME,
      [
        "-I",
        "-S",
        "-B",
        loader,
        entrypoint.descriptor.digest,
        dataset.descriptor.logical_path,
        dataset.descriptor.digest,
        candidate.descriptor.digest,
        support.length === 0 ? "-" : support[0].descriptor.digest,
        task.kind,
      ],
      {
        cwd: directory,
        env: Object.freeze({
          LANG: "C.UTF-8",
          LC_ALL: "C.UTF-8",
          PATH: "/usr/bin",
          TMPDIR: directory,
        }),
        shell: false,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      },
      task.bundle.execution.timeout_ms,
      spawnImplementation,
    );
    const completedAtUnix = nowUnix();
    let output;
    let canonicalOutput;
    try {
      output = JSON.parse(execution.stdout.toString("utf8"));
      canonicalOutput = canonicalJsonBytes(output);
    } catch {
      throw new Error("frozen evaluator stdout is not canonicalizable JSON");
    }
    const expectedStdout = Buffer.concat([canonicalOutput, Buffer.from("\n")]);
    if (!execution.stdout.equals(expectedStdout)) {
      throw new Error("frozen evaluator stdout is not exact canonical JSON plus newline");
    }
    return Object.freeze({
      output,
      stdout: execution.stdout,
      stderr: execution.stderr,
      exit_code: execution.code,
      elapsed_ms: execution.elapsed_ms,
      started_at_unix: startedAtUnix,
      completed_at_unix: completedAtUnix,
      environment: Object.freeze({
        schema: "hepta.paper_raid.review_execution_environment.v1",
        adapter: PYTHON_ADAPTER,
        bridge_version: "0.2.0",
        runtime: "python3-stdlib",
        runtime_path: PYTHON_RUNTIME,
        runtime_flags: Object.freeze(["-I", "-S", "-B"]),
        loader_digest: PYTHON_LOADER_DIGEST,
        evaluator_digest: entrypoint.descriptor.digest,
        support_digests: Object.freeze(support.map(item => item.descriptor.digest)),
        review_kind: task.kind,
        platform: process.platform,
        architecture: process.arch,
      }),
    });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

function validateMetricMap(value, field) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${field} must be an object`);
  }
  const keys = Object.keys(value).sort();
  if (keys.length < 1 || keys.length > 256) {
    throw new Error(`${field} must contain 1 to 256 metrics`);
  }
  const result = {};
  for (const key of keys) {
    assertLogicalId(key, `${field} key`);
    if (!Number.isSafeInteger(value[key])) {
      throw new Error(`${field} values must be safe integer micros`);
    }
    result[key] = value[key];
  }
  return Object.freeze(result);
}

const NO_STATISTICAL_EVIDENCE = Object.freeze({
  schema: "hepta.paper_raid.statistical_evidence.none.v1",
  reason: "frozen_evaluator_did_not_emit_statistical_evidence",
});

function validateToleranceRules(value, metrics) {
  if (!Array.isArray(value) || value.length < 1 || value.length > 128) {
    throw new Error("evaluation tolerance_rules must contain 1 to 128 rules");
  }
  const seen = new Set();
  for (const rule of value) {
    if (!rule || typeof rule !== "object" || Array.isArray(rule)) {
      throw new Error("evaluation tolerance rule must be an object");
    }
    let key;
    switch (rule.kind) {
      case "absolute":
        exactKeys(rule, ["kind", "metric", "max_delta_micros"], "absolute tolerance rule");
        assertLogicalId(rule.metric, "absolute tolerance metric");
        if (
          !Object.hasOwn(metrics, rule.metric) ||
          !Number.isSafeInteger(rule.max_delta_micros) ||
          rule.max_delta_micros < 0
        ) {
          throw new Error("absolute tolerance rule is invalid");
        }
        key = `absolute:${rule.metric}`;
        break;
      case "relative":
        exactKeys(rule, ["kind", "metric", "max_delta_bps"], "relative tolerance rule");
        assertLogicalId(rule.metric, "relative tolerance metric");
        if (
          !Object.hasOwn(metrics, rule.metric) ||
          !Number.isSafeInteger(rule.max_delta_bps) ||
          rule.max_delta_bps < 0 ||
          rule.max_delta_bps > 10_000
        ) {
          throw new Error("relative tolerance rule is invalid");
        }
        key = `relative:${rule.metric}`;
        break;
      case "statistical":
        exactKeys(rule, [
          "kind",
          "metric",
          "minimum_interval_overlap_bps",
          "maximum_effect_delta_micros",
          "minimum_p_value_micros",
        ], "statistical tolerance rule");
        assertLogicalId(rule.metric, "statistical tolerance metric");
        if (
          !Object.hasOwn(metrics, rule.metric) ||
          !Number.isSafeInteger(rule.minimum_interval_overlap_bps) ||
          rule.minimum_interval_overlap_bps < 0 ||
          rule.minimum_interval_overlap_bps > 10_000 ||
          !Number.isSafeInteger(rule.maximum_effect_delta_micros) ||
          rule.maximum_effect_delta_micros < 0 ||
          !Number.isSafeInteger(rule.minimum_p_value_micros) ||
          rule.minimum_p_value_micros < 0 ||
          rule.minimum_p_value_micros > 1_000_000
        ) {
          throw new Error("statistical tolerance rule is invalid");
        }
        key = `statistical:${rule.metric}`;
        break;
      case "seed":
        exactKeys(rule, ["kind", "expected_seed_set_hash"], "seed tolerance rule");
        assertDigest(rule.expected_seed_set_hash, "seed tolerance expected_seed_set_hash");
        key = `seed:${rule.expected_seed_set_hash}`;
        break;
      default:
        throw new Error("evaluation tolerance rule kind is unsupported");
    }
    if (seen.has(key)) throw new Error("evaluation tolerance rules must be unique");
    seen.add(key);
  }
}

function validateStatisticalEvidence(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("reproduction statistical_evidence must be an object");
  }
  const output = {};
  const keys = Object.keys(value).sort();
  if (keys.length > 256) {
    throw new Error("reproduction statistical_evidence is oversized");
  }
  for (const key of keys) {
    assertLogicalId(key, "statistical evidence metric");
    const item = value[key];
    exactKeys(item, [
      "interval_overlap_bps",
      "effect_delta_micros",
      "p_value_micros",
    ], "statistical evidence item");
    if (
      !Number.isSafeInteger(item.interval_overlap_bps) ||
      item.interval_overlap_bps < 0 ||
      item.interval_overlap_bps > 10_000 ||
      !Number.isSafeInteger(item.effect_delta_micros) ||
      !Number.isSafeInteger(item.p_value_micros) ||
      item.p_value_micros < 0 ||
      item.p_value_micros > 1_000_000
    ) {
      throw new Error("statistical evidence item exceeds fixed-point bounds");
    }
    output[key] = Object.freeze({ ...item });
  }
  return Object.freeze(output);
}

function validateExecutionOutput(kind, output) {
  if (kind === "evaluate") {
    exactKeys(output, [
      "reference_metrics_micros",
      "tolerance_policy_version",
      "tolerance_rules",
      "candidate_passed",
    ], "evaluation execution output");
    const metrics = validateMetricMap(
      output.reference_metrics_micros,
      "reference_metrics_micros",
    );
    contractText(output.tolerance_policy_version, "tolerance_policy_version", 128);
    validateToleranceRules(output.tolerance_rules, metrics);
    if (typeof output.candidate_passed !== "boolean") {
      throw new Error("evaluation candidate_passed must be a boolean");
    }
    return Object.freeze({
      observed_metrics_micros: metrics,
      statistical_evidence: NO_STATISTICAL_EVIDENCE,
      candidate_passed: output.candidate_passed,
    });
  }
  if (kind === "reproduce") {
    exactKeys(output, [
      "observed_metrics_micros",
      "statistical_evidence",
    ], "reproduction execution output");
    return Object.freeze({
      observed_metrics_micros: validateMetricMap(
        output.observed_metrics_micros,
        "observed_metrics_micros",
      ),
      statistical_evidence: validateStatisticalEvidence(output.statistical_evidence),
      candidate_passed: null,
    });
  }
  throw new Error("review execution output kind is unsupported");
}

export function createReviewReceiptRequest(state, identity, task, downloaded, execution) {
  if (
    state.binding_id !== task.binding_id ||
    state.agent_id !== identity.agent_id ||
    state.agent_key_id !== identity.agent_key_id
  ) {
    throw new Error("review task does not match the paired Agent state");
  }
  const inputObjects = downloaded.map(item => Object.freeze({
    object_key: item.descriptor.object_key,
    logical_path: item.descriptor.logical_path,
    role: item.descriptor.role,
    digest: item.descriptor.digest,
    size_bytes: item.descriptor.size_bytes,
  }));
  const inputRoot = sha256Digest(canonicalJsonBytes(inputObjects));
  const output = execution.output;
  const outputRoot = sha256Digest(canonicalJsonBytes(output));
  if (
    !Number.isSafeInteger(execution.exit_code) ||
    ![0, 1].includes(execution.exit_code) ||
    !Number.isSafeInteger(execution.elapsed_ms) ||
    execution.elapsed_ms < 0 ||
    execution.elapsed_ms > task.bundle.execution.timeout_ms ||
    !Number.isSafeInteger(execution.started_at_unix) ||
    execution.started_at_unix < 0 ||
    !Number.isSafeInteger(execution.completed_at_unix) ||
    execution.completed_at_unix < execution.started_at_unix
  ) {
    throw new Error("review execution exit, elapsed time, or timestamps are invalid");
  }
  const facts = validateExecutionOutput(task.kind, output);
  const observedMetricsMicros = facts.observed_metrics_micros;
  const evidence = facts.statistical_evidence;
  const candidatePassed = facts.candidate_passed;
  if (
    task.kind === "evaluate" &&
    candidatePassed !== (execution.exit_code === 0)
  ) {
    throw new Error("evaluation candidate_passed disagrees with the evaluator exit code");
  }
  const metricsHash = sha256Digest(canonicalJsonBytes({
    observed_metrics_micros: observedMetricsMicros,
    statistical_evidence: evidence,
    candidate_passed: candidatePassed,
  }));
  const seedSetHash = sha256Digest(canonicalJsonBytes([
    task.bundle.execution.seed,
  ]));
  const environmentHash = sha256Digest(canonicalJsonBytes(execution.environment));
  const logs = Object.freeze({
    schema: "hepta.paper_raid.review_execution_logs.v1",
    stdout_hash: sha256Digest(execution.stdout),
    stderr_hash: sha256Digest(execution.stderr),
    stdout_bytes: execution.stdout.length,
    stderr_bytes: execution.stderr.length,
    truncated: false,
  });
  const logsHash = sha256Digest(canonicalJsonBytes(logs));
  const runManifest = Object.freeze({
    schema: "hepta.paper_raid.review_run_manifest.v1",
    task_id: task.task_id,
    assignment_id: task.assignment_id,
    paper_project_id: task.paper_id,
    submission_id: task.bundle.submission_id,
    evaluation_id: task.evaluation_id,
    kind: task.kind,
    attempt: task.attempt,
    fencing_token: task.fencing_token,
    bundle_hash: task.bundle.bundle_hash,
    evaluator_version: task.bundle.execution.evaluator_version,
    adapter: task.bundle.execution.adapter,
    entrypoint: task.bundle.execution.entrypoint,
    timeout_ms: task.bundle.execution.timeout_ms,
    candidate_passed: candidatePassed,
    input_objects: Object.freeze(inputObjects),
    input_root: inputRoot,
    output_root: outputRoot,
    metrics_hash: metricsHash,
    seed: task.bundle.execution.seed,
    seed_set_hash: seedSetHash,
    environment_hash: environmentHash,
    logs_hash: logsHash,
    exit_code: execution.exit_code,
    elapsed_ms: execution.elapsed_ms,
    started_at_unix: execution.started_at_unix,
    completed_at_unix: execution.completed_at_unix,
  });
  const runManifestHash = sha256Digest(canonicalJsonBytes(runManifest));
  const receiptId = reviewExecutionReceiptId(
    state.binding_id,
    task.task_id,
    task.assignment_id,
    task.bundle.bundle_hash,
    task.attempt,
    task.fencing_token,
  );
  const unsigned = {
    schema: REVIEW_EXECUTION_RECEIPT_SCHEMA,
    receipt_id: receiptId,
    task_id: task.task_id,
    assignment_id: task.assignment_id,
    binding_id: state.binding_id,
    paper_project_id: task.paper_id,
    submission_id: task.bundle.submission_id,
    evaluation_id: task.evaluation_id,
    kind: task.kind,
    attempt: task.attempt,
    fencing_token: task.fencing_token,
    bundle_hash: task.bundle.bundle_hash,
    evaluator_version: task.bundle.execution.evaluator_version,
    input_root: inputRoot,
    output_root: outputRoot,
    metrics_hash: metricsHash,
    observed_metrics_micros: observedMetricsMicros,
    statistical_evidence: evidence,
    candidate_passed: candidatePassed,
    seed_set_hash: seedSetHash,
    environment_hash: environmentHash,
    run_manifest_hash: runManifestHash,
    logs_hash: logsHash,
    started_at_unix: execution.started_at_unix,
    completed_at_unix: execution.completed_at_unix,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    signing_public_key_hash: identity.agent_public_key_hash,
  };
  const receipt = Object.freeze({
    ...unsigned,
    signature: identity.sign(reviewExecutionReceiptFrame(unsigned)),
  });
  const request = Object.freeze({
    schema: REVIEW_RECEIPT_REQUEST_SCHEMA,
    idempotency_key: receiptId,
    receipt,
    output,
    environment: execution.environment,
    run_manifest: runManifest,
    logs,
  });
  validateReviewReceiptRequest(state, identity, request);
  return request;
}

export function validateReviewReceiptRequest(state, identity, request) {
  exactKeys(request, [
    "schema",
    "idempotency_key",
    "receipt",
    "output",
    "environment",
    "run_manifest",
    "logs",
  ], "review receipt request");
  if (request.schema !== REVIEW_RECEIPT_REQUEST_SCHEMA) {
    throw new Error("review receipt request schema is unsupported");
  }
  const receipt = request.receipt;
  exactKeys(receipt, [
    "schema",
    "receipt_id",
    "task_id",
    "assignment_id",
    "binding_id",
    "paper_project_id",
    "submission_id",
    "evaluation_id",
    "kind",
    "attempt",
    "fencing_token",
    "bundle_hash",
    "evaluator_version",
    "input_root",
    "output_root",
    "metrics_hash",
    "observed_metrics_micros",
    "statistical_evidence",
    "candidate_passed",
    "seed_set_hash",
    "environment_hash",
    "run_manifest_hash",
    "logs_hash",
    "started_at_unix",
    "completed_at_unix",
    "agent_id",
    "agent_key_id",
    "signing_public_key_hash",
    "signature",
  ], "review execution receipt");
  if (
    request.idempotency_key !== receipt.receipt_id ||
    receipt.binding_id !== state.binding_id ||
    receipt.agent_id !== identity.agent_id ||
    receipt.agent_key_id !== identity.agent_key_id ||
    receipt.signing_public_key_hash !== identity.agent_public_key_hash
  ) {
    throw new Error("review receipt does not match the paired Agent identity");
  }
  const unsigned = { ...receipt };
  delete unsigned.signature;
  if (!identity.verify(reviewExecutionReceiptFrame(unsigned), receipt.signature)) {
    throw new Error("review receipt signature is invalid");
  }
  if (sha256Digest(canonicalJsonBytes(request.output)) !== receipt.output_root) {
    throw new Error("review receipt output_root does not match output");
  }
  const outputFacts = validateExecutionOutput(receipt.kind, request.output);
  if (
    outputFacts.candidate_passed !== receipt.candidate_passed ||
    canonicalJsonBytes(outputFacts.observed_metrics_micros).compare(
      canonicalJsonBytes(receipt.observed_metrics_micros),
    ) !== 0 ||
    canonicalJsonBytes(outputFacts.statistical_evidence).compare(
      canonicalJsonBytes(receipt.statistical_evidence),
    ) !== 0
  ) {
    throw new Error("review receipt metrics differ from the typed evaluator output");
  }
  exactKeys(request.environment, [
    "schema",
    "adapter",
    "bridge_version",
    "runtime",
    "runtime_path",
    "runtime_flags",
    "loader_digest",
    "evaluator_digest",
    "support_digests",
    "review_kind",
    "platform",
    "architecture",
  ], "review execution environment");
  if (
    request.environment.schema !== "hepta.paper_raid.review_execution_environment.v1" ||
    request.environment.adapter !== PYTHON_ADAPTER ||
    request.environment.bridge_version !== "0.2.0" ||
    request.environment.runtime !== "python3-stdlib" ||
    request.environment.runtime_path !== PYTHON_RUNTIME ||
    canonicalJsonBytes(request.environment.runtime_flags).compare(
      canonicalJsonBytes(["-I", "-S", "-B"]),
    ) !== 0 ||
    request.environment.loader_digest !== PYTHON_LOADER_DIGEST ||
    request.environment.evaluator_digest !== receipt.evaluator_version ||
    request.environment.review_kind !== receipt.kind ||
    !Array.isArray(request.environment.support_digests) ||
    request.environment.support_digests.some(digest => {
      try {
        assertDigest(digest, "environment.support_digest");
        return false;
      } catch {
        return true;
      }
    }) ||
    typeof request.environment.platform !== "string" ||
    request.environment.platform.length === 0 ||
    typeof request.environment.architecture !== "string" ||
    request.environment.architecture.length === 0
  ) {
    throw new Error("review execution environment is not the sealed Bridge runtime");
  }
  exactKeys(request.run_manifest, [
    "schema",
    "task_id",
    "assignment_id",
    "paper_project_id",
    "submission_id",
    "evaluation_id",
    "kind",
    "attempt",
    "fencing_token",
    "bundle_hash",
    "evaluator_version",
    "adapter",
    "entrypoint",
    "timeout_ms",
    "candidate_passed",
    "input_objects",
    "input_root",
    "output_root",
    "metrics_hash",
    "seed",
    "seed_set_hash",
    "environment_hash",
    "logs_hash",
    "exit_code",
    "elapsed_ms",
    "started_at_unix",
    "completed_at_unix",
  ], "review run manifest");
  if (
    request.run_manifest.schema !== "hepta.paper_raid.review_run_manifest.v1" ||
    request.run_manifest.adapter !== PYTHON_ADAPTER ||
    request.run_manifest.entrypoint !== PYTHON_ENTRYPOINT ||
    !Number.isSafeInteger(request.run_manifest.seed) ||
    request.run_manifest.seed < 0 ||
    !Array.isArray(request.run_manifest.input_objects) ||
    request.run_manifest.input_objects.length < 3 ||
    request.run_manifest.input_objects.length > MAX_OBJECTS
  ) {
    throw new Error("review run manifest execution or input contract is invalid");
  }
  let previousObjectKey = null;
  for (const [index, object] of request.run_manifest.input_objects.entries()) {
    exactKeys(object, [
      "object_key",
      "logical_path",
      "role",
      "digest",
      "size_bytes",
    ], `review run input object[${index}]`);
    assertLogicalId(object.object_key, `run_manifest.input_objects[${index}].object_key`);
    validateLogicalPath(
      object.logical_path,
      `run_manifest.input_objects[${index}].logical_path`,
    );
    if (!OBJECT_ROLES.has(object.role)) {
      throw new Error("review run input object role is unsupported");
    }
    assertDigest(object.digest, `run_manifest.input_objects[${index}].digest`);
    positiveInteger(object.size_bytes, `run_manifest.input_objects[${index}].size_bytes`);
    if (previousObjectKey !== null && previousObjectKey >= object.object_key) {
      throw new Error("review run input objects are not strictly sorted");
    }
    previousObjectKey = object.object_key;
  }
  const manifestSupportDigests = request.run_manifest.input_objects
    .filter(object => object.role === "evaluator_support")
    .map(object => object.digest)
    .sort();
  if (
    canonicalJsonBytes(request.environment.support_digests).compare(
      canonicalJsonBytes(manifestSupportDigests),
    ) !== 0
  ) {
    throw new Error("review environment support digests differ from sealed inputs");
  }
  exactKeys(request.logs, [
    "schema",
    "stdout_hash",
    "stderr_hash",
    "stdout_bytes",
    "stderr_bytes",
    "truncated",
  ], "review execution logs");
  if (
    request.logs.schema !== "hepta.paper_raid.review_execution_logs.v1" ||
    request.logs.truncated !== false ||
    !Number.isSafeInteger(request.logs.stdout_bytes) ||
    request.logs.stdout_bytes < 0 ||
    !Number.isSafeInteger(request.logs.stderr_bytes) ||
    request.logs.stderr_bytes < 0
  ) {
    throw new Error("review execution log seal is invalid");
  }
  assertDigest(request.logs.stdout_hash, "logs.stdout_hash");
  assertDigest(request.logs.stderr_hash, "logs.stderr_hash");
  if (sha256Digest(canonicalJsonBytes(request.environment)) !== receipt.environment_hash) {
    throw new Error("review receipt environment_hash does not match environment");
  }
  if (sha256Digest(canonicalJsonBytes(request.run_manifest)) !== receipt.run_manifest_hash) {
    throw new Error("review receipt run_manifest_hash does not match manifest");
  }
  if (sha256Digest(canonicalJsonBytes(request.logs)) !== receipt.logs_hash) {
    throw new Error("review receipt logs_hash does not match seals");
  }
  if (
    sha256Digest(canonicalJsonBytes(request.run_manifest.input_objects)) !==
      receipt.input_root ||
    sha256Digest(canonicalJsonBytes([request.run_manifest.seed])) !==
      receipt.seed_set_hash
  ) {
    throw new Error("review receipt input_root or seed_set_hash is invalid");
  }
  if (
    sha256Digest(canonicalJsonBytes({
      observed_metrics_micros: receipt.observed_metrics_micros,
      statistical_evidence: receipt.statistical_evidence,
      candidate_passed: receipt.candidate_passed,
    })) !== receipt.metrics_hash
  ) {
    throw new Error("review receipt metrics_hash does not match metrics");
  }
  if (
    request.run_manifest.task_id !== receipt.task_id ||
    request.run_manifest.assignment_id !== receipt.assignment_id ||
    request.run_manifest.paper_project_id !== receipt.paper_project_id ||
    request.run_manifest.submission_id !== receipt.submission_id ||
    request.run_manifest.evaluation_id !== receipt.evaluation_id ||
    request.run_manifest.kind !== receipt.kind ||
    request.run_manifest.attempt !== receipt.attempt ||
    request.run_manifest.fencing_token !== receipt.fencing_token ||
    request.run_manifest.bundle_hash !== receipt.bundle_hash ||
    request.run_manifest.evaluator_version !== receipt.evaluator_version ||
    request.run_manifest.input_root !== receipt.input_root ||
    request.run_manifest.output_root !== receipt.output_root ||
    request.run_manifest.metrics_hash !== receipt.metrics_hash ||
    request.run_manifest.candidate_passed !== receipt.candidate_passed ||
    request.run_manifest.seed_set_hash !== receipt.seed_set_hash ||
    request.run_manifest.environment_hash !== receipt.environment_hash ||
    request.run_manifest.logs_hash !== receipt.logs_hash ||
    !Number.isSafeInteger(request.run_manifest.timeout_ms) ||
    request.run_manifest.timeout_ms < 500 ||
    request.run_manifest.timeout_ms > 30_000 ||
    ![0, 1].includes(request.run_manifest.exit_code) ||
    !Number.isSafeInteger(request.run_manifest.elapsed_ms) ||
    request.run_manifest.elapsed_ms < 0 ||
    request.run_manifest.elapsed_ms > request.run_manifest.timeout_ms ||
    request.run_manifest.started_at_unix !== receipt.started_at_unix ||
    request.run_manifest.completed_at_unix !== receipt.completed_at_unix
  ) {
    throw new Error("review receipt and run manifest authority tuples differ");
  }
  if (
    (receipt.kind === "evaluate" &&
      (typeof receipt.candidate_passed !== "boolean" ||
        receipt.candidate_passed !== (request.run_manifest.exit_code === 0))) ||
    (receipt.kind === "reproduce" && receipt.candidate_passed !== null)
  ) {
    throw new Error("review receipt candidate_passed or exit-code binding is invalid");
  }
  return request;
}

export function validateReviewReceiptResult(request, result) {
  exactKeys(result, [
    "schema",
    "receipt_id",
    "task_id",
    "attempt",
    "receipt_hash",
    "status",
  ], "review receipt result");
  const unsigned = { ...request.receipt };
  delete unsigned.signature;
  const expectedHash = sha256Digest(reviewExecutionReceiptFrame(unsigned));
  if (
    result.schema !== "hepta.paper_raid.agent_bridge.review_receipt_result.v1" ||
    result.receipt_id !== request.receipt.receipt_id ||
    result.task_id !== request.receipt.task_id ||
    result.attempt !== request.receipt.attempt ||
    result.receipt_hash !== expectedHash ||
    result.status !== "stored"
  ) {
    throw new Error("review receipt result does not match the immutable stored receipt");
  }
  return Object.freeze({ ...result });
}

export async function executeReviewTask(
  state,
  identity,
  task,
  {
    downloadObject,
    adapter = runFrozenPythonAdapter,
    adapterOptions,
  } = {},
) {
  const downloaded = await downloadFrozenReviewObjects(task, downloadObject);
  const execution = await adapter(task, downloaded, adapterOptions);
  return createReviewReceiptRequest(state, identity, task, downloaded, execution);
}
