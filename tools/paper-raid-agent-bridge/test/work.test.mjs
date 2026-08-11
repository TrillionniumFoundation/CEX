import assert from "node:assert/strict";
import test from "node:test";

import {
  actionableDeliveryCandidates,
  deliveryCandidateKey,
  deliveryCandidates,
  proposalInput,
  workResult,
} from "../src/work.mjs";

const PAPER_ID = "11111111-1111-4111-8111-111111111111";
const WORK_ID = "22222222-2222-4222-8222-222222222222";
const REVISION_ID = "33333333-3333-4333-8333-333333333333";
const MANIFEST_ID = "44444444-4444-4444-8444-444444444444";
const BINDING_ID = "66666666-6666-4666-8666-666666666666";
const DRAFT_ID = "77777777-7777-4777-8777-777777777777";
const LEASE_ID = "88888888-8888-4888-8888-888888888888";
const MANIFEST_HASH = `sha256:${"a".repeat(64)}`;
const PAYLOAD_HASH = `sha256:${"b".repeat(64)}`;

function candidate(overrides = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidate.v1",
    delivery_draft_id: DRAFT_ID,
    binding_id: BINDING_ID,
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    lease_id: LEASE_ID,
    lease_fencing_token: 7,
    expected_work_version: 3,
    parent_revision_id: REVISION_ID,
    proposal_kind: "delivery",
    payload_hash: PAYLOAD_HASH,
    artifact_manifest_id: MANIFEST_ID,
    artifact_manifest_hash: MANIFEST_HASH,
    delivery_state: "pending",
    declared_at_unix: 1_999_999_100,
    expires_at_unix: 2_000_000_000,
    ...overrides,
  };
}

function inbox({ status = "available", items = [candidate()], projection = {} } = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "drafting",
      tasks: [],
      proposals: [],
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status,
        reason_code: status === "unavailable"
          ? "authoritative_task_section_manifest_binding_not_modeled"
          : null,
        items,
        ...projection,
      },
    }],
  };
}

test("work consumes one explicit server-bound delivery without local joins", () => {
  const [selected] = deliveryCandidates(inbox());
  assert.equal(deliveryCandidateKey(selected),
    `${DRAFT_ID}:${PAPER_ID}:${WORK_ID}:methods:${REVISION_ID}:${MANIFEST_ID}`);
  assert.deepEqual(proposalInput(selected), {
    delivery_draft_id: DRAFT_ID,
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    parent_revision_id: REVISION_ID,
    lease_id: LEASE_ID,
    lease_fencing_token: 7,
    expected_work_version: 3,
    proposal_kind: "delivery",
    payload_hash: PAYLOAD_HASH,
    artifact_manifest_id: MANIFEST_ID,
    artifact_manifest_hash: MANIFEST_HASH,
    declared_at_unix: 1_999_999_100,
  });
});

test("work stays idle when the authority cannot prove a delivery binding", () => {
  assert.deepEqual(deliveryCandidates(inbox({
    status: "unavailable",
    items: [],
  })), []);
});

test("persisted submitting and consumed candidates remain restart-recoverable", () => {
  for (const deliveryState of ["submitting", "consumed"]) {
    const recovery = inbox({
      items: [candidate({ delivery_state: deliveryState })],
    });
    recovery.papers[0].phase = "submission_ready";
    assert.equal(deliveryCandidates(recovery)[0].delivery_state, deliveryState);
  }
  const stalePending = inbox();
  stalePending.papers[0].phase = "submission_ready";
  assert.throws(
    () => deliveryCandidates(stalePending),
    /outside an author phase/,
  );
});

test("watch suppresses only a locally acknowledged consumed replay", () => {
  const consumed = inbox({
    items: [candidate({ delivery_state: "consumed" })],
  });
  consumed.papers[0].phase = "submission_ready";
  const key = deliveryCandidateKey(candidate());
  assert.equal(actionableDeliveryCandidates(consumed, new Set()).length, 1);
  assert.equal(actionableDeliveryCandidates(consumed, new Set([key])).length, 0);

  const restartedProcess = new Set();
  assert.equal(actionableDeliveryCandidates(consumed, restartedProcess).length, 1);
  assert.throws(
    () => actionableDeliveryCandidates(consumed, []),
    /must be a Set/,
  );
});

test("legacy task x lease x manifest collections are never guessed", () => {
  const legacy = inbox();
  legacy.papers[0].delivery_candidates = {
    leases: [{ section_key: "methods", status: "active" }],
    section_heads: [{
      section_key: "methods",
      current_head_revision_id: REVISION_ID,
    }],
    artifact_manifests: [{
      manifest_id: MANIFEST_ID,
      manifest_hash: MANIFEST_HASH,
    }],
  };
  assert.throws(() => deliveryCandidates(legacy), /projection is unsupported/);
});

test("unavailable or empty available projections fail closed", () => {
  assert.throws(
    () => deliveryCandidates(inbox({ status: "unavailable", items: [candidate()] })),
    /must contain no candidates/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ status: "available", items: [] })),
    /available delivery projection is invalid/,
  );
});

test("cross-paper, cross-binding, malformed, and duplicate candidates fail closed", () => {
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ paper_id: WORK_ID })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ binding_id: WORK_ID })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ payload_hash: "sha256:no" })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ expected_work_version: 0 })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ lease_fencing_token: 0 })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ delivery_state: "invalidated" })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => proposalInput(candidate({ binding_id: undefined })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate(), candidate()] })),
    /duplicate binding/,
  );
});

test("work rejects non-authoritative inbox schemas", () => {
  assert.throws(
    () => deliveryCandidates({ schema: "hepta.paper_raid.agent_bridge.inbox.v1" }),
    /schema is unsupported/,
  );
});

test("one-shot and watch can share the exact work result wrapper", () => {
  const selected = candidate();
  assert.deepEqual(workResult("idle"), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "idle",
    candidate_count: 0,
  });
  assert.deepEqual(workResult("awaiting_local_selection", { candidateCount: 2 }), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "awaiting_local_selection",
    candidate_count: 2,
  });
  assert.deepEqual(workResult("submitted", {
    candidateCount: 1,
    candidate: selected,
    result: { schema: "hepta.paper_raid.agent_bridge.proposal_result.v2" },
  }), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "submitted",
    candidate_count: 1,
    candidate_key: `${DRAFT_ID}:${PAPER_ID}:${WORK_ID}:methods:${REVISION_ID}:${MANIFEST_ID}`,
    result: { schema: "hepta.paper_raid.agent_bridge.proposal_result.v2" },
  });
});
