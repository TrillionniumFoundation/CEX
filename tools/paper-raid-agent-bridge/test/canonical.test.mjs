import assert from "node:assert/strict";
import { createHash, createPrivateKey, sign as signMessage } from "node:crypto";
import test from "node:test";
import {
  AGENT_BINDING_PROOF_SCHEMA,
  AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
  AGENT_PROPOSAL_SCHEMA,
  AGENT_PROPOSAL_V1_SCHEMA,
  REVIEW_EXECUTION_RECEIPT_SCHEMA,
  agentBindingProofFrame,
  agentBridgeRequestProofFrame,
  agentCapabilityDisclosureFrame,
  agentCapabilityDisclosureHash,
  agentProposalFrame,
  agentProposalV1Frame,
  canonicalJsonBytes,
  canonicalQuery,
  researchSessionActionFrame,
  reviewExecutionReceiptFrame,
  sha256Digest,
} from "../src/canonical.mjs";
import { DISCLOSURE, fixture } from "./helpers.mjs";

test("Agent Proposal V1 historical frame remains byte frozen", () => {
  const frame = agentProposalV1Frame({
    schema: AGENT_PROPOSAL_V1_SCHEMA,
    proposal_id: "11111111-1111-4111-8111-111111111111",
    paper_project_id: "22222222-2222-4222-8222-222222222222",
    work_item_id: "33333333-3333-4333-8333-333333333333",
    section_key: "methods",
    parent_revision_id: "44444444-4444-4444-8444-444444444444",
    proposal_kind: "delivery",
    payload_hash: `sha256:${"aa".repeat(32)}`,
    artifact_manifest_hash: `sha256:${"bb".repeat(32)}`,
    agent_id: "agent.fixture",
    binding_id: "55555555-5555-4555-8555-555555555555",
    agent_key_id: `sha256:${"cc".repeat(32)}`,
    signed_at_unix: 1_700_000_000,
  });
  assert.equal(
    createHash("sha256").update(frame).digest("hex"),
    "d2b9c101267889a53ba7766a2f252a94ee5f7b08ba92237e6e801391b3b7c1ad",
  );
});

test("Agent Proposal V2 frame matches the frozen Rust epoch vector", () => {
  const frame = agentProposalFrame({
    schema: AGENT_PROPOSAL_SCHEMA,
    proposal_id: "11111111-1111-4111-8111-111111111111",
    paper_project_id: "22222222-2222-4222-8222-222222222222",
    work_item_id: "33333333-3333-4333-8333-333333333333",
    section_key: "results.main",
    parent_revision_id: "44444444-4444-4444-8444-444444444444",
    lease_id: "55555555-5555-4555-8555-555555555555",
    lease_fencing_token: 7,
    expected_work_version: 11,
    proposal_kind: "delivery",
    payload_hash: "sha256:7917212537bd6e80eb59be660839509f2c0319c23e7236ab589e1bf6868e598b",
    artifact_manifest_id: "66666666-6666-4666-8666-666666666666",
    artifact_manifest_hash: "sha256:fad5d89eff2f29912c8c10f4cb411fbc87b0dbb8d916258c741edb39575c388c",
    agent_id: "did:trnm:agent-proposal-v2",
    binding_id: "77777777-7777-4777-8777-777777777777",
    agent_key_id: "sha256:3097e2dee2cb4a34b53840cdb705aed71067c36f68db0e0f559c3f3fa043315f",
    signed_at_unix: 1_770_000_000,
  });
  assert.equal(
    createHash("sha256").update(frame).digest("hex"),
    "b285cf8c2b1609a73afea9dfb7c4af2ef5720c82eded01e793e6516df4d694be",
  );
  const privateKey = createPrivateKey({
    key: Buffer.concat([
      Buffer.from("302e020100300506032b657004220420", "hex"),
      Buffer.alloc(32, 0x42),
    ]),
    format: "der",
    type: "pkcs8",
  });
  assert.equal(
    signMessage(null, frame, privateKey).toString("base64"),
    "tPgBHp6Aypntpam1qkp8h14l1x4wRWn+mEfsQsqb3N79YIiKExuQEG2mEgUgxkR1HiWsV7I/daUXzXxfuZ6uCw==",
  );
  const proposalFrame = agentProposalFrame({
    schema: AGENT_PROPOSAL_SCHEMA,
    proposal_id: "11111111-1111-4111-8111-111111111111",
    paper_project_id: "22222222-2222-4222-8222-222222222222",
    work_item_id: "33333333-3333-4333-8333-333333333333",
    section_key: "results.main",
    parent_revision_id: "44444444-4444-4444-8444-444444444444",
    lease_id: "55555555-5555-4555-8555-555555555555",
    lease_fencing_token: 7,
    expected_work_version: 11,
    proposal_kind: "proposal",
    payload_hash: "sha256:7917212537bd6e80eb59be660839509f2c0319c23e7236ab589e1bf6868e598b",
    artifact_manifest_id: "66666666-6666-4666-8666-666666666666",
    artifact_manifest_hash: "sha256:fad5d89eff2f29912c8c10f4cb411fbc87b0dbb8d916258c741edb39575c388c",
    agent_id: "did:trnm:agent-proposal-v2",
    binding_id: "77777777-7777-4777-8777-777777777777",
    agent_key_id: "sha256:3097e2dee2cb4a34b53840cdb705aed71067c36f68db0e0f559c3f3fa043315f",
    signed_at_unix: 1_770_000_000,
  });
  assert.notDeepEqual(proposalFrame, frame);
  for (const tampered of [
    { lease_fencing_token: 0 },
    { expected_work_version: 0 },
    { artifact_manifest_id: "88888888-8888-4888-8888-888888888888" },
  ]) {
    const proposal = {
      schema: AGENT_PROPOSAL_SCHEMA,
      proposal_id: "11111111-1111-4111-8111-111111111111",
      paper_project_id: "22222222-2222-4222-8222-222222222222",
      work_item_id: "33333333-3333-4333-8333-333333333333",
      section_key: "results.main",
      parent_revision_id: "44444444-4444-4444-8444-444444444444",
      lease_id: "55555555-5555-4555-8555-555555555555",
      lease_fencing_token: 7,
      expected_work_version: 11,
      proposal_kind: "delivery",
      payload_hash: "sha256:7917212537bd6e80eb59be660839509f2c0319c23e7236ab589e1bf6868e598b",
      artifact_manifest_id: "66666666-6666-4666-8666-666666666666",
      artifact_manifest_hash: "sha256:fad5d89eff2f29912c8c10f4cb411fbc87b0dbb8d916258c741edb39575c388c",
      agent_id: "did:trnm:agent-proposal-v2",
      binding_id: "77777777-7777-4777-8777-777777777777",
      agent_key_id: "sha256:3097e2dee2cb4a34b53840cdb705aed71067c36f68db0e0f559c3f3fa043315f",
      signed_at_unix: 1_770_000_000,
      ...tampered,
    };
    if (tampered.lease_fencing_token === 0 || tampered.expected_work_version === 0) {
      assert.throws(() => agentProposalFrame(proposal), /unsigned integer/);
    } else {
      assert.notEqual(
        createHash("sha256").update(agentProposalFrame(proposal)).digest("hex"),
        "b285cf8c2b1609a73afea9dfb7c4af2ef5720c82eded01e793e6516df4d694be",
      );
    }
  }
});

test("ReviewExecutionReceiptV1 frame freezes assignment, evaluation, seals, and fencing", () => {
  const receipt = {
    schema: REVIEW_EXECUTION_RECEIPT_SCHEMA,
    receipt_id: "11111111-1111-4111-8111-111111111111",
    task_id: "22222222-2222-4222-8222-222222222222",
    assignment_id: "33333333-3333-4333-8333-333333333333",
    binding_id: "44444444-4444-4444-8444-444444444444",
    paper_project_id: "55555555-5555-4555-8555-555555555555",
    submission_id: "66666666-6666-4666-8666-666666666666",
    evaluation_id: "77777777-7777-4777-8777-777777777777",
    kind: "evaluate",
    attempt: 2,
    fencing_token: 9,
    bundle_hash: `sha256:${"01".repeat(32)}`,
    evaluator_version: "evidence-audit-evaluator-v1",
    input_root: `sha256:${"02".repeat(32)}`,
    output_root: `sha256:${"03".repeat(32)}`,
    metrics_hash: `sha256:${"04".repeat(32)}`,
    candidate_passed: true,
    seed_set_hash: `sha256:${"05".repeat(32)}`,
    environment_hash: `sha256:${"06".repeat(32)}`,
    run_manifest_hash: `sha256:${"07".repeat(32)}`,
    logs_hash: `sha256:${"08".repeat(32)}`,
    started_at_unix: 1_800_000_010,
    completed_at_unix: 1_800_000_011,
    agent_id: "did:trnm:review-agent",
    agent_key_id: `sha256:${"09".repeat(32)}`,
    signing_public_key_hash: `sha256:${"09".repeat(32)}`,
  };
  const frame = reviewExecutionReceiptFrame(receipt);
  assert.equal(
    createHash("sha256").update(frame).digest("hex"),
    "52b2f4f7bcf9991c6eddc7e296db05d98e851cc819e9d5baf6a89203bd0730f9",
  );
  for (const tamper of [
    { evaluation_id: "88888888-8888-4888-8888-888888888888" },
    { attempt: 3 },
    { fencing_token: 10 },
    { output_root: `sha256:${"ff".repeat(32)}` },
    { candidate_passed: false },
  ]) {
    assert.notDeepEqual(
      reviewExecutionReceiptFrame({ ...receipt, ...tamper }),
      frame,
    );
  }
  assert.throws(
    () => reviewExecutionReceiptFrame({ ...receipt, evaluation_id: null }),
    /evaluation_id must be a canonical lowercase UUID/,
  );
});

test("AgentBinding V3 capability disclosure matches the frozen Rust vector", () => {
  assert.equal(
    agentCapabilityDisclosureFrame(DISCLOSURE).toString("hex"),
    "68657074615f70617065725f726169645f6167656e745f6361706162696c6974795f646973636c6f737572655f7631000000002f68657074612e70617065725f726169642e6167656e745f6361706162696c6974795f646973636c6f737572652e76310000001873656c665f6465636c617265645f756e7665726966696564000000030000001161727469666163745f616e616c797369730000000f65766964656e63655f7365617263680000001073656374696f6e5f6472616674696e67000000030000000b61727469666163745f696f000000036370750000000773616e64626f7800000002",
  );
  assert.equal(
    agentCapabilityDisclosureHash(DISCLOSURE),
    "sha256:22aeaec9954e5774212ab9103e28d7e3e52c827c8d976839715d9b3b1cc41fec",
  );
});

test("optional research-session signer keeps the existing frozen action frame", () => {
  const payload = Buffer.from('{"participant_slot":1}', "utf8");
  const action = {
    schema: "trnm.research-session.action.v1",
    action_id: "action.fixture.1",
    authorization_id: "authorization.fixture.1",
    session_id: "session.fixture.1",
    team_id: "team.fixture.1",
    paper_project_id: "paper.fixture.1",
    challenge_id: "challenge.fixture.1",
    roster_version: 1,
    participant_slot: 1,
    participant_sequence: 1,
    expected_session_version: 1,
    issued_at_unix: 1_700_000_000,
    action_type: "participant.ready",
    payload_type: "trnm.research-session.ready.v1",
    payload: payload.toString("base64"),
    payload_hash: sha256Digest(payload),
    reference_hash: `sha256:${"cc".repeat(32)}`,
    agent_key_id: `sha256:${"dd".repeat(32)}`,
  };
  assert.ok(researchSessionActionFrame(action).length > payload.length);
  assert.throws(
    () =>
      researchSessionActionFrame({
        ...action,
        payload_hash: `sha256:${"00".repeat(32)}`,
      }),
    /payload_hash does not match/,
  );
});

test("AgentBinding V3 frame binds capability disclosure material", () => {
  const publicKey = Buffer.from(Array.from({ length: 32 }, (_, index) => index));
  const keyHash = sha256Digest(publicKey);
  const claim = {
    schema: AGENT_BINDING_PROOF_SCHEMA,
    binding_id: "11111111-1111-4111-8111-111111111111",
    agent_id: "agent.fixture",
    agent_key_id: keyHash,
    agent_public_key: publicKey.toString("base64"),
    agent_public_key_hash: keyHash,
    capability_disclosure_hash: agentCapabilityDisclosureHash(DISCLOSURE),
    subject_id: "subject.fixture",
    player_id: "22222222-2222-4222-8222-222222222222",
    nonce: "fixture-binding-nonce",
    issued_at_unix: 1_700_000_000,
    expires_at_unix: 1_700_000_300,
  };
  const original = agentBindingProofFrame(claim);
  const tampered = agentBindingProofFrame({
    ...claim,
    capability_disclosure_hash: `sha256:${"ff".repeat(32)}`,
  });
  assert.notDeepEqual(original, tampered);
});

test("request proof freezes method, path, canonical query, and exact body hash", async t => {
  const { identity, binding } = await fixture(t, "request-proof");
  const body = canonicalJsonBytes({ z: 1, nested: { b: 2, a: 1 } });
  assert.equal(body.toString("utf8"), '{"nested":{"a":1,"b":2},"z":1}');
  assert.equal(canonicalQuery("a=3&z=2"), "a=3&z=2");
  assert.throws(() => canonicalQuery("z=2&a=3"), /strictly sorted/);
  assert.throws(() => canonicalQuery("a=1&a=2"), /must be unique/);
  const claim = {
    schema: AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
    binding_id: binding.binding_id,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    http_method: "POST",
    canonical_path: "/api/agent-bridge/inbox",
    canonical_query: "",
    body_hash: sha256Digest(body),
    nonce: "77777777-7777-4777-8777-777777777777",
    issued_at_unix: 1_700_000_000,
    expires_at_unix: 1_700_000_060,
  };
  const signature = identity.sign(agentBridgeRequestProofFrame(claim));
  assert.equal(identity.verify(agentBridgeRequestProofFrame(claim), signature), true);
  assert.doesNotThrow(() =>
    agentBridgeRequestProofFrame({
      ...claim,
      http_method: "GET",
      canonical_path: "/api/agent-bridge/review-objects",
      canonical_query:
        "assignment_id=33333333-3333-4333-8333-333333333333&object_key=object-0000&task_id=44444444-4444-4444-8444-444444444444",
      body_hash: sha256Digest(Buffer.alloc(0)),
    }),
  );
  assert.doesNotThrow(() =>
    agentBridgeRequestProofFrame({
      ...claim,
      canonical_path: "/api/agent-bridge/review-receipts",
    }),
  );
  for (const canonical_path of [
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-claims",
    "/api/agent-bridge/practice-results",
  ]) {
    assert.doesNotThrow(() =>
      agentBridgeRequestProofFrame({ ...claim, canonical_path }),
    );
  }
  assert.throws(
    () =>
      agentBridgeRequestProofFrame({
        ...claim,
        http_method: "GET",
        canonical_path: "/api/agent-bridge/review-receipts",
        body_hash: sha256Digest(Buffer.alloc(0)),
      }),
    /method\/path is not allowed/,
  );
  for (const tampered of [
    { ...claim, agent_key_id: sha256Digest(Buffer.from("tampered-key")) },
    { ...claim, canonical_path: "/api/agent-bridge/health" },
    { ...claim, canonical_query: "after=1" },
    { ...claim, body_hash: sha256Digest(Buffer.from("tampered")) },
    {
      ...claim,
      http_method: "GET",
      canonical_path: "/api/agent-bridge/binding",
      body_hash: sha256Digest(Buffer.alloc(0)),
    },
  ]) {
    assert.equal(
      identity.verify(agentBridgeRequestProofFrame(tampered), signature),
      false,
    );
  }
  assert.equal(
    createHash("sha256").update(agentBridgeRequestProofFrame(claim)).digest("hex").length,
    64,
  );
});

test("AgentBridgeRequestProofV1 has frozen language-neutral bytes", () => {
  const frame = agentBridgeRequestProofFrame({
    schema: AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
    binding_id: "11111111-1111-4111-8111-111111111111",
    agent_id: "agent.fixture",
    agent_key_id: `sha256:${"aa".repeat(32)}`,
    http_method: "POST",
    canonical_path: "/api/agent-bridge/inbox",
    canonical_query: "",
    body_hash: `sha256:${"bb".repeat(32)}`,
    nonce: "22222222-2222-4222-8222-222222222222",
    issued_at_unix: 1_700_000_000,
    expires_at_unix: 1_700_000_060,
  });
  assert.equal(
    frame.toString("hex"),
    "68657074615f70617065725f726169645f6167656e745f6272696467655f726571756573745f70726f6f665f7631000000002e68657074612e70617065725f726169642e6167656e745f6272696467655f726571756573745f70726f6f662e76310000002431313131313131312d313131312d343131312d383131312d3131313131313131313131310000000d6167656e742e66697874757265000000477368613235363a6161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616100000004504f5354000000172f6170692f6167656e742d6272696467652f696e626f7800000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000002432323232323232322d323232322d343232322d383232322d323232323232323232323232000000006553f100000000006553f13c",
  );
  assert.equal(
    createHash("sha256").update(frame).digest("hex"),
    "a4016d74baab0315d849fbe120c238188860cee326324eba34c7a8e21745f062",
  );
});
