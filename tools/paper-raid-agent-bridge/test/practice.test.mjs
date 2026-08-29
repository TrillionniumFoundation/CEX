import assert from "node:assert/strict";
import test from "node:test";
import {
  PRACTICE_MATERIALS_SCHEMA,
  PRACTICE_TASKS_SCHEMA,
  PRACTICE_TASK_SCHEMA,
  PRACTICE_TRANSITION_RESULT_SCHEMA,
  fixedPracticeResult,
  validatePracticeTasks,
  validatePracticeTransitionResult,
} from "../src/practice.mjs";

function taskResponse(
  state = "pending",
  version = 3,
  resultCode = null,
  taskToken = `sha256:${(state === "pending" ? "a" : state === "claimed" ? "b" : "c").repeat(64)}`,
) {
  return {
    schema: PRACTICE_TASKS_SCHEMA,
    mode: "practice_unranked",
    status: "ready",
    task: {
      schema: PRACTICE_TASK_SCHEMA,
      kind: "evidence_audit_intro",
      state,
      version,
      task_token: taskToken,
      expires_at: "2027-01-15T08:00:00Z",
      materials: {
        schema: PRACTICE_MATERIALS_SCHEMA,
        claim: "The candidate result remains supported after the evidence audit.",
        baseline: "Compare the stated claim with the supplied observation summary.",
        observations: [
          "The cited observation and the claimed scope do not fully align.",
          "Run the bounded practice check and report only one allowed result code.",
        ],
      },
      allowed_result_codes: [
        "concern_confirmed",
        "concern_not_detected",
        "inconclusive",
      ],
      result_code: resultCode,
    },
  };
}

test("practice projection is exact, owner-opaque, and locally deterministic", () => {
  const claimed = validatePracticeTasks(taskResponse("claimed", 4));
  assert.equal(claimed.task.state, "claimed");
  assert.equal(claimed.task.task_token, `sha256:${"b".repeat(64)}`);
  assert.equal(fixedPracticeResult(claimed), "concern_confirmed");
  const text = JSON.stringify(claimed);
  for (const forbidden of [
    "practice_session_id",
    "subject_id",
    "player_id",
    "binding_id",
    "bridge_task_id",
    "request_hash",
    "result_hash",
    "activation",
    "qualification",
    "finality",
    "ranking",
    "reward",
    "economy",
  ]) assert.equal(text.includes(forbidden), false, `projection leaked ${forbidden}`);

  const completed = validatePracticeTasks(
    taskResponse("completed", 5, "concern_confirmed"),
  );
  assert.equal(completed.task.result_code, "concern_confirmed");
  assert.throws(() => fixedPracticeResult(completed), /must be claimed/);
  assert.deepEqual(
    validatePracticeTasks({
      schema: PRACTICE_TASKS_SCHEMA,
      mode: "practice_unranked",
      status: "not_ready",
      task: null,
    }),
    {
      schema: PRACTICE_TASKS_SCHEMA,
      mode: "practice_unranked",
      status: "not_ready",
      task: null,
    },
  );
});

test("practice projection rejects hidden IDs, material drift, and result substitution", () => {
  for (const mutate of [
    value => { value.binding_id = "11111111-1111-4111-8111-111111111111"; },
    value => { value.task.bridge_task_id = "22222222-2222-4222-8222-222222222222"; },
    value => { value.task.materials.claim = "substituted claim"; },
    value => { value.task.materials.observations.reverse(); },
    value => { value.task.allowed_result_codes.reverse(); },
    value => { value.task.result_code = "concern_confirmed"; },
    value => { value.task.version = 0; },
    value => { value.task.task_token = `sha256:${"A".repeat(64)}`; },
    value => { value.task.task_token = `sha256:${"a".repeat(63)}`; },
    value => { value.task.expires_at = "not-a-time"; },
  ]) {
    const hostile = structuredClone(taskResponse());
    mutate(hostile);
    assert.throws(() => validatePracticeTasks(hostile));
  }
  const unavailableWithTask = taskResponse();
  unavailableWithTask.status = "expired";
  assert.throws(() => validatePracticeTasks(unavailableWithTask));
  const completedWithoutResult = taskResponse("completed", 5, null);
  assert.throws(() => validatePracticeTasks(completedWithoutResult));
});

test("practice transition receipts are exact and version-bound", () => {
  const claim = {
    schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
    operation: "claim",
    status: "claimed",
    from_version: 3,
    version: 4,
  };
  assert.deepEqual(validatePracticeTransitionResult(claim, "claim", 3), claim);
  const result = {
    schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
    operation: "result",
    status: "completed",
    from_version: 4,
    version: 5,
    result_code: "concern_confirmed",
  };
  assert.deepEqual(
    validatePracticeTransitionResult(result, "result", 4, "concern_confirmed"),
    result,
  );
  for (const hostile of [
    { ...claim, practice_session_id: "33333333-3333-4333-8333-333333333333" },
    { ...claim, version: 5 },
    { ...claim, status: "completed" },
    { ...result, result_code: "concern_not_detected" },
    { ...result, result_hash: `sha256:${"a".repeat(64)}` },
  ]) {
    assert.throws(() =>
      hostile.operation === "claim"
        ? validatePracticeTransitionResult(hostile, "claim", 3)
        : validatePracticeTransitionResult(
          hostile,
          "result",
          4,
          "concern_confirmed",
        )
    );
  }
});
