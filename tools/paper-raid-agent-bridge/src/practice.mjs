export const PRACTICE_TASK_QUERY_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_task_query.v1";
export const PRACTICE_TASKS_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_tasks.v1";
export const PRACTICE_TASK_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_task.v1";
export const PRACTICE_MATERIALS_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_materials.v1";
export const PRACTICE_CLAIM_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_claim_request.v1";
export const PRACTICE_RESULT_REQUEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_result_request.v1";
export const PRACTICE_TRANSITION_RESULT_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_transition_result.v1";
export const PRACTICE_AUTO_RESULT_SCHEMA =
  "hepta.paper_raid.agent_bridge.practice_auto_result.v1";

export const PRACTICE_RESULT_CODES = Object.freeze([
  "concern_confirmed",
  "concern_not_detected",
  "inconclusive",
]);

const PRACTICE_MATERIALS = Object.freeze({
  schema: PRACTICE_MATERIALS_SCHEMA,
  claim: "The candidate result remains supported after the evidence audit.",
  baseline: "Compare the stated claim with the supplied observation summary.",
  observations: Object.freeze([
    "The cited observation and the claimed scope do not fully align.",
    "Run the bounded practice check and report only one allowed result code.",
  ]),
});

function exactObject(value, fields, label) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    throw new Error(`${label} is invalid`);
  }
  const keys = Object.keys(value);
  if (
    keys.length !== fields.length ||
    !fields.every(field => Object.hasOwn(value, field))
  ) {
    throw new Error(`${label} has unsupported fields`);
  }
  return value;
}

function positiveVersion(value, label) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function opaqueTaskToken(value) {
  if (typeof value !== "string" || !/^sha256:[0-9a-f]{64}$/.test(value)) {
    throw new Error("practice task token is invalid");
  }
  return value;
}

function exactStringArray(value, expected, label) {
  if (
    !Array.isArray(value) ||
    value.length !== expected.length ||
    !expected.every((entry, index) => value[index] === entry)
  ) {
    throw new Error(`${label} is invalid`);
  }
}

function validateMaterials(value) {
  exactObject(
    value,
    ["schema", "claim", "baseline", "observations"],
    "practice materials",
  );
  if (
    value.schema !== PRACTICE_MATERIALS.schema ||
    value.claim !== PRACTICE_MATERIALS.claim ||
    value.baseline !== PRACTICE_MATERIALS.baseline
  ) {
    throw new Error("practice materials do not match the installed exercise");
  }
  exactStringArray(
    value.observations,
    PRACTICE_MATERIALS.observations,
    "practice observations",
  );
  return PRACTICE_MATERIALS;
}

function validateTask(value) {
  exactObject(
    value,
    [
      "schema",
      "kind",
      "state",
      "version",
      "task_token",
      "expires_at",
      "materials",
      "allowed_result_codes",
      "result_code",
    ],
    "practice task",
  );
  if (
    value.schema !== PRACTICE_TASK_SCHEMA ||
    value.kind !== "evidence_audit_intro" ||
    !["pending", "claimed", "completed"].includes(value.state)
  ) {
    throw new Error("practice task identity or state is invalid");
  }
  positiveVersion(value.version, "practice task version");
  if (
    typeof value.expires_at !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(value.expires_at) ||
    Number.isNaN(Date.parse(value.expires_at))
  ) {
    throw new Error("practice task expiry is invalid");
  }
  const materials = validateMaterials(value.materials);
  exactStringArray(
    value.allowed_result_codes,
    PRACTICE_RESULT_CODES,
    "practice result-code authority",
  );
  if (
    (value.state === "completed" && !PRACTICE_RESULT_CODES.includes(value.result_code)) ||
    (value.state !== "completed" && value.result_code !== null)
  ) {
    throw new Error("practice task result binding is invalid");
  }
  return Object.freeze({
    schema: PRACTICE_TASK_SCHEMA,
    kind: "evidence_audit_intro",
    state: value.state,
    version: value.version,
    task_token: opaqueTaskToken(value.task_token),
    expires_at: value.expires_at,
    materials,
    allowed_result_codes: PRACTICE_RESULT_CODES,
    result_code: value.result_code,
  });
}

export function validatePracticeTasks(value) {
  exactObject(value, ["schema", "mode", "status", "task"], "practice tasks response");
  if (
    value.schema !== PRACTICE_TASKS_SCHEMA ||
    value.mode !== "practice_unranked" ||
    !["absent", "not_ready", "expired", "ready"].includes(value.status)
  ) {
    throw new Error("practice tasks response identity or status is invalid");
  }
  if (value.status !== "ready") {
    if (value.task !== null) {
      throw new Error("unavailable practice response supplied a task");
    }
    return Object.freeze({
      schema: PRACTICE_TASKS_SCHEMA,
      mode: "practice_unranked",
      status: value.status,
      task: null,
    });
  }
  return Object.freeze({
    schema: PRACTICE_TASKS_SCHEMA,
    mode: "practice_unranked",
    status: "ready",
    task: validateTask(value.task),
  });
}

export function validatePracticeTransitionResult(
  value,
  operation,
  expectedFromVersion,
  expectedResultCode = undefined,
) {
  const isResult = operation === "result";
  if (!isResult && operation !== "claim") {
    throw new Error("practice transition operation is invalid");
  }
  exactObject(
    value,
    isResult
      ? ["schema", "operation", "status", "from_version", "version", "result_code"]
      : ["schema", "operation", "status", "from_version", "version"],
    "practice transition response",
  );
  positiveVersion(expectedFromVersion, "expected practice version");
  if (
    value.schema !== PRACTICE_TRANSITION_RESULT_SCHEMA ||
    value.operation !== operation ||
    value.status !== (isResult ? "completed" : "claimed") ||
    value.from_version !== expectedFromVersion ||
    value.version !== expectedFromVersion + 1 ||
    (isResult &&
      (!PRACTICE_RESULT_CODES.includes(expectedResultCode) ||
        value.result_code !== expectedResultCode))
  ) {
    throw new Error("practice transition response binding is invalid");
  }
  return Object.freeze({
    schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
    operation,
    status: value.status,
    from_version: value.from_version,
    version: value.version,
    ...(isResult ? { result_code: value.result_code } : {}),
  });
}

export function fixedPracticeResult(tasks) {
  const exact = validatePracticeTasks(tasks);
  if (exact.status !== "ready" || exact.task.state !== "claimed") {
    throw new Error("practice task must be claimed before local execution");
  }
  // The installed introductory exercise is immutable and intentionally bounded.
  // Its first observation contradicts the scope asserted by the fixed claim.
  return "concern_confirmed";
}
