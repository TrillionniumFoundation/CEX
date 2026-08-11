import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { access, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  canonicalJsonBytes,
  reviewExecutionReceiptFrame,
  sha256Digest,
} from "../src/canonical.mjs";
import {
  actionableReviewTasks,
  createReviewReceiptRequest,
  downloadFrozenReviewObjects,
  executeReviewTask,
  frozenReviewAuthorityHash,
  frozenReviewBundleHash,
  reviewObjectQuery,
  reviewExecutionReceiptId,
  reviewTaskKey,
  reviewTasks,
  runFrozenPythonAdapter,
  validateReviewReceiptRequest,
  validateReviewReceiptResult,
} from "../src/review.mjs";
import { reviewOutboxPath } from "../src/review_outbox.mjs";
import {
  executeAndSubmitReviewTask,
  recoverPendingReviewReceipt,
} from "../src/operations.mjs";
import { saveBridgeState } from "../src/state.mjs";
import { fixture, jsonResponse } from "./helpers.mjs";

const PAPER_ID = "77777777-7777-4777-8777-777777777777";
const TASK_ID = "11111111-1111-4111-8111-111111111111";
const ASSIGNMENT_ID = "22222222-2222-4222-8222-222222222222";
const SUBMISSION_ID = "33333333-3333-4333-8333-333333333333";
const EVALUATION_ID = "44444444-4444-4444-8444-444444444444";
const EVALUATOR_DIGEST =
  "sha256:b50f61282b4e89797c25c343fef3077d2bf0dc8713ffedcb5ecbe26d15258392";
const LOADER_DIGEST =
  "sha256:8cba01ffa388fe0d71caa9237b4d6a63700766be9e05a4ee7bc3b9cfd6099df3";

test("review receipt ID matches the Rust golden vector", () => {
  assert.equal(
    reviewExecutionReceiptId(
      "11111111-1111-4111-8111-111111111111",
      "22222222-2222-4222-8222-222222222222",
      "33333333-3333-4333-8333-333333333333",
      `sha256:${"a".repeat(64)}`,
      2,
      7,
    ),
    "445f1306-eda6-5a10-bcd9-bbc15e349bce",
  );
});

test("review receipt result is immutable and exact", async t => {
  const item = await fixture(t, "review-result");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    1_800_000_000,
  );
  const bytes = await objectBytes();
  const [task] = reviewTasks(inbox(item.binding.binding_id, [await taskValue()]));
  const request = await executeReviewTask(state, item.identity, task, {
    downloadObject: async (_task, object) => bytes.get(object.object_key),
    adapter: async () => fakeExecution(),
  });
  const unsigned = { ...request.receipt };
  delete unsigned.signature;
  const expected = {
    schema: "hepta.paper_raid.agent_bridge.review_receipt_result.v1",
    receipt_id: request.receipt.receipt_id,
    task_id: request.receipt.task_id,
    attempt: request.receipt.attempt,
    receipt_hash: sha256Digest(
      reviewExecutionReceiptFrame(unsigned),
    ),
    status: "stored",
  };
  assert.deepEqual(validateReviewReceiptResult(request, expected), expected);
  assert.throws(
    () => validateReviewReceiptResult(request, { ...expected, replay: true }),
    /unsupported or missing fields/,
  );
  assert.throws(
    () => validateReviewReceiptResult(request, { ...expected, status: "consumed" }),
    /immutable stored receipt/,
  );
});

async function objectBytes() {
  const evaluatorFixture = await readFile(new URL(
    "./fixtures/evidence-audit-evaluator.py",
    import.meta.url,
  ));
  const evaluator = evaluatorFixture;
  assert.equal(sha256Digest(evaluator), EVALUATOR_DIGEST);
  const excerpt = "audited exact excerpt";
  const dataset = canonicalJsonBytes({
    claims: [{
      citation_source_id: "source-1",
      claim_id: "claim-1",
      declared_license: "MIT",
      evidence_excerpt: excerpt,
      evidence_sha256: sha256Digest(Buffer.from(excerpt)),
    }],
    sources: [{ license: "MIT", source_id: "source-1" }],
  });
  const candidate = canonicalJsonBytes({
    decisions: [{ claim_id: "claim-1", outcome: "pass", reasons: [] }],
    schema: "hepta.evidence_audit.report.v1",
    summary: { failed: 0, passed: 1 },
  });
  return new Map([
    ["candidate-report", candidate],
    ["dataset-claims", dataset],
    ["frozen-evaluator", evaluator],
  ]);
}

function descriptor(objectKey, logicalPath, role, bytes, overrides = {}) {
  return {
    object_key: objectKey,
    logical_path: logicalPath,
    role,
    digest: sha256Digest(bytes),
    size_bytes: bytes.length,
    media_type: ["frozen_evaluator", "evaluator_support"].includes(role)
      ? "text/x-python; charset=utf-8"
      : "application/json",
    download_path: "/api/agent-bridge/review-objects",
    ...overrides,
  };
}

function sourceAuthorityObjects(objects) {
  return structuredClone(objects).map(object => ({
    ...object,
    logical_path: {
      candidate: "release/candidate.json",
      dataset: object.media_type === "application/json"
        ? "dataset/claims.json"
        : "dataset/observations.csv",
      frozen_evaluator: "evaluator.py",
      evaluator_support: "baseline.py",
    }[object.role],
  }));
}

async function taskValue({
  kind = "evaluate",
  role = kind === "evaluate" ? "evaluator" : "reproducer",
  taskId = TASK_ID,
  assignmentId = ASSIGNMENT_ID,
  overrides = {},
  bundleOverrides = {},
  executionOverrides = {},
} = {}) {
  const bytes = await objectBytes();
  const objects = [
    descriptor(
      "candidate-report",
      "inputs/candidate.json",
      "candidate",
      bytes.get("candidate-report"),
    ),
    descriptor(
      "dataset-claims",
      "inputs/dataset.json",
      "dataset",
      bytes.get("dataset-claims"),
    ),
    descriptor(
      "frozen-evaluator",
      "evaluator/main.py",
      "frozen_evaluator",
      bytes.get("frozen-evaluator"),
    ),
  ];
  const executionPolicy = {
    schema: "hepta.paper_raid.review_execution_policy.v1",
    kind,
    adapter: "python3-stdlib-v1",
    timeout_ms: 2_000,
    seed: 1701,
  };
  const authority = {
    schema: "hepta.paper_raid.frozen_review_authority.v1",
    authority_hash: "",
    assignment_id: assignmentId,
    paper_project_id: PAPER_ID,
    submission_id: SUBMISSION_ID,
    review_round: 1,
    slot: role,
    assignment_version: 7,
    expires_at: "2099-01-01T00:00:00Z",
    release_candidate_hash: `sha256:${"a".repeat(64)}`,
    paper_bundle_hash: `sha256:${"b".repeat(64)}`,
    artifact_manifest_hash: `sha256:${"c".repeat(64)}`,
    evaluator_manifest_hash: `sha256:${"d".repeat(64)}`,
    dataset_manifest_hash: `sha256:${"e".repeat(64)}`,
    artifact_objects: sourceAuthorityObjects(objects),
    execution_policy: executionPolicy,
  };
  authority.authority_hash = frozenReviewAuthorityHash(authority);
  const bundle = {
    schema: "hepta.paper_raid.resolved_frozen_review_bundle.v1",
    bundle_hash: "",
    authority,
    authority_hash: authority.authority_hash,
    assignment_id: assignmentId,
    paper_project_id: PAPER_ID,
    submission_id: SUBMISSION_ID,
    review_round: 1,
    slot: role,
    assignment_version: 7,
    expires_at: "2099-01-01T00:00:00Z",
    release_candidate_hash: `sha256:${"a".repeat(64)}`,
    paper_bundle_hash: `sha256:${"b".repeat(64)}`,
    artifact_manifest_hash: `sha256:${"c".repeat(64)}`,
    evaluator_manifest_hash: `sha256:${"d".repeat(64)}`,
    dataset_manifest_hash: `sha256:${"e".repeat(64)}`,
    objects,
    execution: {
      schema: "hepta.paper_raid.review_execution_plan.v1",
      kind,
      adapter: "python3-stdlib-v1",
      evaluator_version: EVALUATOR_DIGEST,
      entrypoint: "evaluator/main.py",
      timeout_ms: 2_000,
      seed: 1701,
      ...executionOverrides,
    },
    ...bundleOverrides,
  };
  if (!Object.hasOwn(bundleOverrides, "bundle_hash")) {
    bundle.bundle_hash = frozenReviewBundleHash(bundle);
  }
  return {
    schema: "hepta.paper_raid.agent_bridge.review_task.v1",
    task_id: taskId,
    assignment_id: assignmentId,
    paper_id: PAPER_ID,
    evaluation_id: EVALUATION_ID,
    role,
    kind,
    attempt: 1,
    fencing_token: 7,
    state: "pending",
    bundle,
    ...overrides,
  };
}

async function exactPythonFixture(name, digest) {
  const bytes = await readFile(new URL(`./fixtures/${name}`, import.meta.url));
  assert.equal(sha256Digest(bytes), digest);
  return bytes;
}

function benchmarkCandidate() {
  const rows = [
    ["s01", 1, 0, 2], ["s02", 2, 1, 5], ["s03", 3, 0, 6],
    ["s04", 4, -1, 7], ["s05", 5, 1, 11], ["s06", 6, 0, 12],
    ["s07", 7, -1, 13], ["s08", 8, 1, 17], ["s09", 9, 0, 18],
    ["s10", 10, -1, 19],
  ];
  const runs = ["full", "zeroed-signal", "without-shortcut"].map(mode => {
    const predictions = rows.map(([sampleId, signal, shortcut]) => ({
      sample_id: sampleId,
      prediction: mode === "full"
        ? 2 * signal + shortcut
        : mode === "zeroed-signal" ? shortcut : 2 * signal,
    }));
    const targets = new Map(rows.map(([sampleId, , , target]) => [sampleId, target]));
    return {
      mode,
      predictions,
      retained: mode === "zeroed-signal",
      sample_count: rows.length,
      status: mode === "zeroed-signal" ? "failed" : "successful",
      sum_squared_error: predictions.reduce(
        (sum, item) => sum + (item.prediction - targets.get(item.sample_id)) ** 2,
        0,
      ),
    };
  });
  return canonicalJsonBytes({
    schema: "hepta.benchmark_ablation.report.v1",
    seed: 1701,
    runs,
  });
}

async function supportPackValue(pack) {
  const definitions = {
    benchmark: {
      evaluator: "benchmark-ablation-evaluator.py",
      evaluatorDigest: "sha256:cf34d96f7f0075d4a7e60cdc49fee5ecd15b9d29bcb19eb935c9ec32e2e96eea",
      support: "benchmark-ablation-baseline.py",
      supportDigest: "sha256:f0ca6e845f87f5344b8763490ad18e1c436c9da6acf36e9c5b7c2464f5071393",
      dataset: Buffer.from([
        "sample_id,signal,shortcut,target", "s01,1,0,2", "s02,2,1,5",
        "s03,3,0,6", "s04,4,-1,7", "s05,5,1,11", "s06,6,0,12",
        "s07,7,-1,13", "s08,8,1,17", "s09,9,0,18", "s10,10,-1,19", "",
      ].join("\n")),
      candidate: benchmarkCandidate(),
    },
    replication: {
      evaluator: "replication-evaluator.py",
      evaluatorDigest: "sha256:3c256c75533b8154305b821cb30de3c5dd4c3f575ed1f72e6513320d87eddbf4",
      support: "replication-baseline.py",
      supportDigest: "sha256:199d18b168d6e09d46b75c3108757aaffbb2426cf758a71951c6b51451b58422",
      dataset: Buffer.from([
        "observation_id,group,value", "c01,control,4", "c02,control,6",
        "c03,control,5", "c04,control,7", "c05,control,3", "c06,control,5",
        "t01,treated,6", "t02,treated,8", "t03,treated,7", "t04,treated,7",
        "t05,treated,5", "t06,treated,6", "",
      ].join("\n")),
      candidate: canonicalJsonBytes({
        analysis_id: "frozen-mean-difference-v1",
        difference_denominator: 2,
        difference_numerator: 3,
        groups: {
          control: { count: 6, mean_denominator: 1, mean_numerator: 5, sum: 30 },
          treated: { count: 6, mean_denominator: 2, mean_numerator: 13, sum: 39 },
        },
        predeclared_tolerance: { denominator: 1, numerator: 0 },
        schema: "hepta.replication.report.v1",
      }),
    },
  };
  const definition = definitions[pack];
  const evaluator = await exactPythonFixture(
    definition.evaluator,
    definition.evaluatorDigest,
  );
  const support = await exactPythonFixture(
    definition.support,
    definition.supportDigest,
  );
  const bytes = new Map([
    ["candidate-report", definition.candidate],
    ["dataset-input", definition.dataset],
    ["frozen-evaluator", evaluator],
    ["support-baseline", support],
  ]);
  const value = await taskValue();
  value.bundle.objects = [
    descriptor("candidate-report", "inputs/candidate.json", "candidate", definition.candidate),
    descriptor("dataset-input", "inputs/dataset.csv", "dataset", definition.dataset, {
      media_type: "text/csv; charset=utf-8",
    }),
    descriptor("frozen-evaluator", "evaluator/main.py", "frozen_evaluator", evaluator),
    descriptor("support-baseline", "evaluator/baseline.py", "evaluator_support", support),
  ];
  value.bundle.execution.evaluator_version = definition.evaluatorDigest;
  value.bundle.authority.artifact_objects = sourceAuthorityObjects(value.bundle.objects);
  value.bundle.authority.authority_hash = frozenReviewAuthorityHash(value.bundle.authority);
  value.bundle.authority_hash = value.bundle.authority.authority_hash;
  value.bundle.bundle_hash = frozenReviewBundleHash(value.bundle);
  return { bytes, value };
}

function inbox(bindingId, items, { status = "available" } = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: bindingId,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "review_raid",
      tasks: [],
      proposals: [],
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status: "unavailable",
        reason_code: "authoritative_task_section_manifest_binding_not_modeled",
        items: [],
      },
      review_tasks: {
        schema: "hepta.paper_raid.agent_bridge.review_tasks.v1",
        status,
        reason_code: status === "available"
          ? null
          : "frozen_review_objects_unavailable",
        items,
      },
    }],
  };
}

function evaluationOutput() {
  return {
    candidate_passed: true,
    reference_metrics_micros: { primary_effect: 1_240_000 },
    tolerance_policy_version: "1",
    tolerance_rules: [{
      kind: "absolute",
      metric: "primary_effect",
      max_delta_micros: 1_000,
    }],
  };
}

function reproductionOutput() {
  return {
    observed_metrics_micros: { primary_effect: 1_240_000 },
    statistical_evidence: {
      primary_effect: {
        interval_overlap_bps: 9_000,
        effect_delta_micros: 0,
        p_value_micros: 50_000,
      },
    },
  };
}

function fakeExecution(
  output = evaluationOutput(),
  {
    kind = "evaluate",
    evaluatorDigest = EVALUATOR_DIGEST,
    supportDigests = [],
    exitCode = output.candidate_passed === false ? 1 : 0,
  } = {},
) {
  const stdout = canonicalJsonBytes(output);
  return Object.freeze({
    output,
    stdout,
    stderr: Buffer.alloc(0),
    exit_code: exitCode,
    elapsed_ms: 12,
    started_at_unix: 1_800_000_010,
    completed_at_unix: 1_800_000_011,
    environment: Object.freeze({
      schema: "hepta.paper_raid.review_execution_environment.v1",
      adapter: "python3-stdlib-v1",
      bridge_version: "0.2.0",
      runtime: "python3-stdlib",
      runtime_path: "/usr/bin/python3",
      runtime_flags: ["-I", "-S", "-B"],
      loader_digest: LOADER_DIGEST,
      evaluator_digest: evaluatorDigest,
      support_digests: supportDigests,
      review_kind: kind,
      platform: "linux",
      architecture: "x64",
    }),
  });
}

test("review inbox freezes evaluator and reproducer assignment authority", async t => {
  const item = await fixture(t, "review-projection");
  const evaluator = await taskValue();
  const reproducer = await taskValue({
    kind: "reproduce",
    taskId: "55555555-5555-4555-8555-555555555555",
    assignmentId: "66666666-6666-4666-8666-666666666666",
  });
  const tasks = reviewTasks(inbox(item.binding.binding_id, [evaluator, reproducer]), {
    nowUnix: 1_800_000_000,
  });
  assert.equal(tasks.length, 2);
  assert.equal(tasks[0].binding_id, item.binding.binding_id);
  assert.equal(tasks.some(task => task.kind === "evaluate"), true);
  assert.equal(tasks.some(task => task.kind === "reproduce"), true);
  assert.match(reviewTaskKey(tasks[0]), /^11111111-/);
  const consumed = structuredClone(evaluator);
  consumed.state = "consumed";
  const consumedInbox = inbox(item.binding.binding_id, [consumed]);
  assert.equal(actionableReviewTasks(consumedInbox).length, 0);
  assert.equal(
    actionableReviewTasks(consumedInbox, new Set([reviewTaskKey(tasks[0])])).length,
    0,
  );
  const retry = structuredClone(evaluator);
  retry.attempt = 2;
  const [retryTask] = reviewTasks(inbox(item.binding.binding_id, [retry]), {
    nowUnix: 1_800_000_000,
  });
  assert.notEqual(reviewTaskKey(retryTask), reviewTaskKey(tasks[0]));
  assert.equal(
    actionableReviewTasks(
      inbox(item.binding.binding_id, [retry]),
      new Set(),
      { nowUnix: 1_800_000_000 },
    ).length,
    1,
  );
  assert.notEqual(
    reviewExecutionReceiptId(
      item.binding.binding_id,
      retry.task_id,
      retry.assignment_id,
      retry.bundle.bundle_hash,
      retry.attempt,
      retry.fencing_token,
    ),
    reviewExecutionReceiptId(
      item.binding.binding_id,
      evaluator.task_id,
      evaluator.assignment_id,
      evaluator.bundle.bundle_hash,
      evaluator.attempt,
      evaluator.fencing_token,
    ),
  );
});

test("review object route is fixed and every authority pin is canonical-query signed", async t => {
  const item = await fixture(t, "review-query");
  const [task] = reviewTasks(inbox(item.binding.binding_id, [await taskValue()]));
  assert.equal(reviewObjectQuery(task, task.bundle.objects[0]), [
    `assignment_id=${ASSIGNMENT_ID}`,
    `bundle_hash=${task.bundle.bundle_hash.replace(":", "%3A")}`,
    `digest=${task.bundle.objects[0].digest.replace(":", "%3A")}`,
    "object_key=candidate-report",
    `task_id=${TASK_ID}`,
  ].join("&"));
});

test("real allowlisted evaluator runs from verified frozen bytes and signs all seals", async t => {
  const item = await fixture(t, "review-execution");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    1_800_000_000,
  );
  const bytes = await objectBytes();
  const [task] = reviewTasks(inbox(item.binding.binding_id, [await taskValue()]));
  const downloaded = await downloadFrozenReviewObjects(
    task,
    async (_task, object) => bytes.get(object.object_key),
  );
  const directTimes = [1_800_000_010, 1_800_000_011];
  const direct = await runFrozenPythonAdapter(task, downloaded, {
    nowUnix: () => directTimes.shift(),
  });
  assert.equal(direct.output.candidate_passed, true);
  assert.deepEqual(direct.environment.runtime_flags, ["-I", "-S", "-B"]);
  assert.match(direct.environment.loader_digest, /^sha256:[0-9a-f]{64}$/);
  const request = await executeReviewTask(state, item.identity, task, {
    downloadObject: async (_task, object) => bytes.get(object.object_key),
    adapter: async () => fakeExecution(),
  });
  assert.deepEqual(request.output, evaluationOutput());
  assert.equal(request.receipt.task_id, TASK_ID);
  assert.equal(request.receipt.assignment_id, ASSIGNMENT_ID);
  assert.equal(request.receipt.binding_id, item.binding.binding_id);
  assert.equal(request.receipt.bundle_hash, task.bundle.bundle_hash);
  assert.equal(request.receipt.attempt, 1);
  assert.equal(request.receipt.fencing_token, 7);
  assert.equal(request.receipt.evaluator_version, EVALUATOR_DIGEST);
  assert.equal(request.receipt.input_root, request.run_manifest.input_root);
  assert.equal(request.receipt.output_root, request.run_manifest.output_root);
  assert.equal(request.receipt.environment_hash, request.run_manifest.environment_hash);
  assert.equal(request.receipt.logs_hash, request.run_manifest.logs_hash);
  assert.equal(request.logs.truncated, false);
  assert.equal(request.run_manifest.evaluation_id, EVALUATION_ID);
  assert.equal(request.run_manifest.timeout_ms, 2_000);
  assert.equal(request.run_manifest.elapsed_ms, 12);
  assert.equal(validateReviewReceiptRequest(state, item.identity, request), request);

  const tampered = structuredClone(request);
  tampered.output.candidate_passed = false;
  assert.throws(
    () => validateReviewReceiptRequest(state, item.identity, tampered),
    /output_root does not match/,
  );
  const fenceTamper = structuredClone(request);
  fenceTamper.receipt.fencing_token = 8;
  assert.throws(
    () => validateReviewReceiptRequest(state, item.identity, fenceTamper),
    /signature is invalid/,
  );
});

test("all three audited seeded-pack evaluators run through the sealed loader", async t => {
  const item = await fixture(t, "review-three-packs");
  for (const pack of ["benchmark", "replication"]) {
    const { bytes, value } = await supportPackValue(pack);
    const [task] = reviewTasks(inbox(item.binding.binding_id, [value]));
    const downloaded = await downloadFrozenReviewObjects(
      task,
      async (_task, object) => bytes.get(object.object_key),
    );
    let invocation;
    const execution = await runFrozenPythonAdapter(task, downloaded, {
      spawnImplementation: (executable, args, options) => {
        invocation = { executable, args, options };
        return spawn(executable, args, options);
      },
    });
    assert.equal(execution.output.candidate_passed, true, pack);
    assert.equal(invocation.executable, "/usr/bin/python3");
    assert.deepEqual(invocation.args.slice(0, 3), ["-I", "-S", "-B"]);
    assert.equal(invocation.args.at(-1), "evaluate");
    assert.deepEqual(Object.keys(invocation.options.env).sort(), [
      "LANG", "LC_ALL", "PATH", "TMPDIR",
    ]);
    assert.deepEqual(execution.environment.support_digests, [
      task.bundle.objects.find(object => object.role === "evaluator_support").digest,
    ]);
  }
});

test("isolated loader ignores site, pth, user-site, and baseline module injection", async t => {
  const item = await fixture(t, "review-python-injection");
  const ambient = await mkdtemp(join(tmpdir(), "paper-raid-python-injection-"));
  const userSite = join(ambient, "user", "lib", "python3.12", "site-packages");
  const siteMarker = join(ambient, "site-marker");
  const pthMarker = join(ambient, "pth-marker");
  const baselineMarker = join(ambient, "baseline-marker");
  const previous = {
    PYTHONPATH: process.env.PYTHONPATH,
    PYTHONUSERBASE: process.env.PYTHONUSERBASE,
    HOME: process.env.HOME,
  };
  try {
    await mkdir(userSite, { recursive: true });
    await writeFile(
      join(ambient, "sitecustomize.py"),
      `from pathlib import Path\nPath(${JSON.stringify(siteMarker)}).write_text('loaded')\n`,
    );
    await writeFile(
      join(ambient, "baseline.py"),
      `from pathlib import Path\nPath(${JSON.stringify(baselineMarker)}).write_text('loaded')\nraise RuntimeError('ambient baseline loaded')\n`,
    );
    await writeFile(
      join(userSite, "hostile.pth"),
      `import pathlib; pathlib.Path(${JSON.stringify(pthMarker)}).write_text('loaded')\n`,
    );
    process.env.PYTHONPATH = ambient;
    process.env.PYTHONUSERBASE = join(ambient, "user");
    process.env.HOME = ambient;

    const { bytes, value } = await supportPackValue("benchmark");
    const [task] = reviewTasks(inbox(item.binding.binding_id, [value]));
    const downloaded = await downloadFrozenReviewObjects(
      task,
      async (_task, object) => bytes.get(object.object_key),
    );
    const execution = await runFrozenPythonAdapter(task, downloaded);
    assert.equal(execution.output.candidate_passed, true);
    for (const marker of [siteMarker, pthMarker, baselineMarker]) {
      await assert.rejects(access(marker), error => error.code === "ENOENT");
    }
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    await rm(ambient, { recursive: true, force: true });
  }
});

test("bundle, role, download, evaluator, expiry, and byte tampering fail closed", async t => {
  const item = await fixture(t, "review-hostile");
  const valid = await taskValue();
  for (const [mutate, pattern] of [
    [value => { value.paper_id = "88888888-8888-4888-8888-888888888888"; }, /crosses the inbox Paper/],
    [value => { value.role = "author"; }, /role\/kind pair/],
    [value => { value.bundle.assignment_version = 8; }, /does not match the review task authority/],
    [value => { value.bundle.execution.adapter = "shell-v1"; }, /execution plan is unsupported/],
    [value => { value.bundle.execution.entrypoint = "../../bin/sh"; }, /execution plan is unsupported/],
    [value => { value.bundle.objects[0].logical_path = "evaluator/json.py"; }, /not deterministic/],
    [value => { value.bundle.objects[0].download_path = "https://evil.example/object"; }, /fixed review route/],
    [value => { value.bundle.objects[2].digest = `sha256:${"d".repeat(64)}`; }, /differs from Hepta authority/],
    [value => { value.bundle.objects.reverse(); }, /sorted by object_key/],
  ]) {
    const hostile = structuredClone(valid);
    mutate(hostile);
    assert.throws(
      () => reviewTasks(inbox(item.binding.binding_id, [hostile])),
      pattern,
    );
  }
  const expired = structuredClone(valid);
  expired.bundle.expires_at = "2020-01-01T00:00:00Z";
  expired.bundle.authority.expires_at = "2020-01-01T00:00:00Z";
  expired.bundle.authority.authority_hash = frozenReviewAuthorityHash(
    expired.bundle.authority,
  );
  expired.bundle.authority_hash = expired.bundle.authority.authority_hash;
  expired.bundle.bundle_hash = frozenReviewBundleHash(expired.bundle);
  assert.throws(
    () => reviewTasks(inbox(item.binding.binding_id, [expired]), {
      nowUnix: 1_800_000_000,
    }),
    /expired frozen bundle/,
  );

  const authorityByteMutant = structuredClone(valid);
  authorityByteMutant.bundle.authority.release_candidate_hash =
    `sha256:f${"a".repeat(63)}`;
  assert.throws(
    () => reviewTasks(inbox(item.binding.binding_id, [authorityByteMutant])),
    /authority hash does not match canonical authority bytes/,
  );

  const evaluatorAllowlistMutant = structuredClone(valid);
  evaluatorAllowlistMutant.bundle.objects[2].digest = `sha256:${"d".repeat(64)}`;
  evaluatorAllowlistMutant.bundle.authority.artifact_objects[2].digest =
    evaluatorAllowlistMutant.bundle.objects[2].digest;
  evaluatorAllowlistMutant.bundle.authority.authority_hash = frozenReviewAuthorityHash(
    evaluatorAllowlistMutant.bundle.authority,
  );
  evaluatorAllowlistMutant.bundle.authority_hash =
    evaluatorAllowlistMutant.bundle.authority.authority_hash;
  evaluatorAllowlistMutant.bundle.bundle_hash = frozenReviewBundleHash(
    evaluatorAllowlistMutant.bundle,
  );
  assert.throws(
    () => reviewTasks(inbox(item.binding.binding_id, [evaluatorAllowlistMutant])),
    /not in this Bridge release allowlist/,
  );

  const bundleByteMutant = structuredClone(valid);
  bundleByteMutant.bundle.objects[0].size_bytes += 1;
  bundleByteMutant.bundle.authority.artifact_objects[0].size_bytes += 1;
  bundleByteMutant.bundle.authority.authority_hash = frozenReviewAuthorityHash(
    bundleByteMutant.bundle.authority,
  );
  bundleByteMutant.bundle.authority_hash =
    bundleByteMutant.bundle.authority.authority_hash;
  assert.throws(
    () => reviewTasks(inbox(item.binding.binding_id, [bundleByteMutant])),
    /bundle hash does not match canonical bundle bytes/,
  );

  const [task] = reviewTasks(inbox(item.binding.binding_id, [valid]));
  const bytes = await objectBytes();
  await assert.rejects(
    downloadFrozenReviewObjects(task, async (_task, object) => {
      const exact = bytes.get(object.object_key);
      return object.object_key === "dataset-claims"
        ? Buffer.concat([exact.subarray(0, -1), Buffer.from("X")])
        : exact;
    }),
    /failed size\/digest verification/,
  );
});

test("owner-only outbox replays the exact receipt after a lost response and restart", async t => {
  const item = await fixture(t, "review-recovery");
  await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    1_800_000_000,
  );
  const bytes = await objectBytes();
  const [task] = reviewTasks(inbox(item.binding.binding_id, [await taskValue()]));
  const receiptBodies = [];
  let receiptAttempts = 0;
  const fetchImplementation = async (url, init) => {
    const parsed = new URL(url);
    if (parsed.pathname === "/api/agent-bridge/review-objects") {
      const objectKey = parsed.searchParams.get("object_key");
      assert.equal(parsed.searchParams.get("assignment_id"), ASSIGNMENT_ID);
      assert.equal(parsed.searchParams.get("bundle_hash"), task.bundle.bundle_hash);
      assert.equal(parsed.searchParams.get("task_id"), TASK_ID);
      const value = bytes.get(objectKey);
      assert.equal(sha256Digest(value), parsed.searchParams.get("digest"));
      return new Response(value, { status: 200 });
    }
    assert.equal(parsed.pathname, "/api/agent-bridge/review-receipts");
    receiptAttempts += 1;
    receiptBodies.push(init.body);
    if (receiptAttempts <= 2) throw new TypeError("response vanished after commit");
    const request = JSON.parse(init.body);
    const unsigned = { ...request.receipt };
    delete unsigned.signature;
    return jsonResponse({
      schema: "hepta.paper_raid.agent_bridge.review_receipt_result.v1",
      receipt_id: request.receipt.receipt_id,
      task_id: request.receipt.task_id,
      attempt: request.receipt.attempt,
      receipt_hash: sha256Digest(reviewExecutionReceiptFrame(unsigned)),
      status: "stored",
    });
  };
  await assert.rejects(
    executeAndSubmitReviewTask(item.config, item.identity, task, {
      nowUnix: 1_800_000_020,
      fetchImplementation,
      adapter: async () => fakeExecution(),
    }),
    error => error.code === "agent_bridge_transport_failed",
  );
  assert.equal(receiptBodies.length, 2);
  assert.equal(receiptBodies[0], receiptBodies[1]);
  assert.equal((await stat(reviewOutboxPath(item.statePath))).mode & 0o777, 0o600);

  const recovered = await recoverPendingReviewReceipt(item.config, item.identity, {
    nowUnix: 1_800_000_021,
    fetchImplementation,
  });
  assert.equal(recovered.recovered, true);
  assert.equal(recovered.task_id, TASK_ID);
  assert.equal(receiptBodies.length, 3);
  assert.equal(receiptBodies[2], receiptBodies[0]);
  await assert.rejects(
    stat(reviewOutboxPath(item.statePath)),
    error => error.code === "ENOENT",
  );
});

test("receipt construction rejects noncanonical metrics and cross-binding state", async t => {
  const item = await fixture(t, "review-receipt-hostile");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    1_800_000_000,
  );
  const bytes = await objectBytes();
  const [task] = reviewTasks(inbox(item.binding.binding_id, [await taskValue()]));
  const downloaded = await downloadFrozenReviewObjects(
    task,
    async (_task, object) => bytes.get(object.object_key),
  );
  const invalidMetrics = {
    ...fakeExecution(),
    output: {
      ...evaluationOutput(),
      reference_metrics_micros: { score: 1.5 },
    },
  };
  assert.throws(
    () => createReviewReceiptRequest(
      state,
      item.identity,
      task,
      downloaded,
      invalidMetrics,
    ),
    /safe integer/,
  );
  assert.throws(
    () => createReviewReceiptRequest(
      { ...state, binding_id: "88888888-8888-4888-8888-888888888888" },
      item.identity,
      task,
      downloaded,
      fakeExecution(),
    ),
    /does not match the paired Agent state/,
  );

  const emptyMetrics = fakeExecution({
    ...evaluationOutput(),
    reference_metrics_micros: {},
  });
  assert.throws(
    () => createReviewReceiptRequest(
      state,
      item.identity,
      task,
      downloaded,
      emptyMetrics,
    ),
    /must contain 1 to 256 metrics/,
  );

  const overtime = { ...fakeExecution(), elapsed_ms: 2_001 };
  assert.throws(
    () => createReviewReceiptRequest(
      state,
      item.identity,
      task,
      downloaded,
      overtime,
    ),
    /exit, elapsed time, or timestamps are invalid/,
  );
});

test("reproduction output maps exact metrics and statistical evidence into the receipt", async t => {
  const item = await fixture(t, "review-reproduction-output");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    1_800_000_000,
  );
  const bytes = await objectBytes();
  const raw = await taskValue({ kind: "reproduce" });
  const [task] = reviewTasks(inbox(item.binding.binding_id, [raw]));
  const downloaded = await downloadFrozenReviewObjects(
    task,
    async (_task, object) => bytes.get(object.object_key),
  );
  const request = createReviewReceiptRequest(
    state,
    item.identity,
    task,
    downloaded,
    fakeExecution(reproductionOutput(), { kind: "reproduce" }),
  );
  assert.deepEqual(
    request.receipt.observed_metrics_micros,
    reproductionOutput().observed_metrics_micros,
  );
  assert.deepEqual(
    request.receipt.statistical_evidence,
    reproductionOutput().statistical_evidence,
  );
  assert.equal(request.run_manifest.kind, "reproduce");
});
