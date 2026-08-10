import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import {
  AGENT_BINDING_PROOF_SCHEMA,
  AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
  agentBindingProofFrame,
  agentBridgeRequestProofFrame,
  agentCapabilityDisclosureFrame,
  agentCapabilityDisclosureHash,
  canonicalJsonBytes,
  canonicalQuery,
  researchSessionActionFrame,
  sha256Digest,
} from "../src/canonical.mjs";
import { DISCLOSURE, fixture } from "./helpers.mjs";

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
  for (const tampered of [
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
