import assert from "node:assert/strict";
import { readdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import {
  AGENT_BINDING_PROOF_SCHEMA,
  AGENT_PROPOSAL_SCHEMA,
  agentBindingProofFrame,
  agentCapabilityDisclosureHash,
  agentProposalFrame,
} from "../src/canonical.mjs";
import {
  HEALTH_REPORT_SCHEMA,
  INBOX_REQUEST_SCHEMA,
  bridgeHealth,
  createProposalRequest,
  getBinding,
  getInbox,
  pairAgent,
  submitAgentProposal,
} from "../src/operations.mjs";
import { loadBridgeState, saveBridgeState } from "../src/state.mjs";
import { generateIdentity, loadIdentity } from "../src/identity.mjs";
import {
  BINDING_ID,
  DISCLOSURE,
  GRANT_ID,
  IDEMPOTENCY_ID,
  NONCE,
  PAPER_ID,
  PLAYER_ID,
  PROPOSAL_ID,
  fixture,
  headersObject,
  jsonResponse,
} from "./helpers.mjs";

const SECRET_PAIR_CODE = "PAIR-ULTRA-SECRET-846219";
const NOW = 1_800_000_000;

function context() {
  return {
    schema: "hepta.paper_raid.agent_bridge.pairing_context.v1",
    grant_id: GRANT_ID,
    subject_id: "subject.bridge.fixture",
    player_id: PLAYER_ID,
    issued_at_unix: NOW,
    expires_at_unix: NOW + 300,
  };
}

test("pair keeps code/request in memory, retries exact lost response, and persists only public state", async t => {
  const item = await fixture(t, "pair-lost-response");
  let codeReads = 0;
  const calls = [];
  let pairAttempts = 0;
  const fetchImplementation = async (url, init) => {
    const call = {
      path: new URL(url).pathname,
      headers: headersObject(init.headers),
      body: init.body,
    };
    calls.push(call);
    for (const forbidden of ["authorization", "cookie", "x-paper-raid-csrf"]) {
      assert.equal(forbidden in call.headers, false);
    }
    if (call.path === "/api/agent-bridge/pairing-context") {
      assert.deepEqual(JSON.parse(call.body), { pairing_code: SECRET_PAIR_CODE });
      return jsonResponse(context());
    }
    assert.equal(call.path, "/api/agent-bridge/pair");
    pairAttempts += 1;
    if (pairAttempts === 1) throw new TypeError("response vanished after commit");
    const request = JSON.parse(call.body);
    return jsonResponse({
      binding: {
        ...item.binding,
        binding_id: request.binding_request.binding_id,
        player_id: request.binding_request.player_id,
        pairing_code: SECRET_PAIR_CODE,
        subject_id: context().subject_id,
      },
    }, 201);
  };
  const result = await pairAgent(item.config, item.identity, {
    readPairingCode: async () => {
      codeReads += 1;
      return SECRET_PAIR_CODE;
    },
    fetchImplementation,
    nowUnix: NOW,
    bindingId: BINDING_ID,
    nonce: NONCE,
  });
  assert.equal(codeReads, 1);
  assert.equal(pairAttempts, 2);
  assert.deepEqual(calls[1], calls[2]);
  assert.equal(calls.some(call => call.path === "/alpha/login"), false);

  const pairBody = JSON.parse(calls[1].body);
  assert.equal(pairBody.pairing_code, SECRET_PAIR_CODE);
  const request = pairBody.binding_request;
  assert.equal(request.agent_proof_schema, AGENT_BINDING_PROOF_SCHEMA);
  assert.deepEqual(request.capability_disclosure, DISCLOSURE);
  assert.equal(
    request.capability_disclosure_hash,
    agentCapabilityDisclosureHash(DISCLOSURE),
  );
  assert.equal(
    item.identity.verify(
      agentBindingProofFrame({
        schema: request.agent_proof_schema,
        binding_id: request.binding_id,
        agent_id: request.agent_id,
        agent_key_id: request.agent_key_id,
        agent_public_key: request.agent_public_key,
        agent_public_key_hash: request.agent_key_id,
        capability_disclosure_hash: request.capability_disclosure_hash,
        subject_id: context().subject_id,
        player_id: request.player_id,
        nonce: request.agent_proof_nonce,
        issued_at_unix: request.agent_proof_issued_at_unix,
        expires_at_unix: request.agent_proof_expires_at_unix,
      }),
      request.agent_proof_signature,
    ),
    true,
  );

  assert.equal(result.binding_id, BINDING_ID);
  assert.equal(JSON.stringify(result).includes(SECRET_PAIR_CODE), false);
  assert.equal(JSON.stringify(result).includes(context().subject_id), false);
  const stateText = await readFile(item.statePath, "utf8");
  assert.equal(stateText.includes(SECRET_PAIR_CODE), false);
  assert.equal(stateText.includes("agent_proof_signature"), false);
  assert.equal(stateText.includes("subject.bridge.fixture"), false);
  assert.equal((await stat(item.statePath)).mode & 0o777, 0o600);
  const files = await readdir(item.directory);
  assert.equal(files.some(name => name.includes("pending")), false);
  for (const name of files) {
    assert.equal((await readFile(`${item.directory}/${name}`, "utf8")).includes(SECRET_PAIR_CODE), false);
  }
  const state = await loadBridgeState(item.statePath, item.identity);
  assert.deepEqual(Object.keys(state).sort(), [
    "agent_id",
    "agent_key_id",
    "agent_public_key_hash",
    "binding_id",
    "bound_at_unix",
    "capability_disclosure_hash",
    "player_id",
    "schema",
  ]);
});

test("ambiguous pair failure creates no pending code or binding state", async t => {
  const item = await fixture(t, "pair-failure");
  const pairBodies = [];
  let codeReads = 0;
  const fetchImplementation = async (url, init) => {
    if (new URL(url).pathname === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    pairBodies.push(init.body);
    throw new TypeError("lost");
  };
  await assert.rejects(
    pairAgent(item.config, item.identity, {
      readPairingCode: async () => {
        codeReads += 1;
        return SECRET_PAIR_CODE;
      },
      fetchImplementation,
      nowUnix: NOW,
      bindingId: BINDING_ID,
      nonce: NONCE,
    }),
    error => error.code === "agent_bridge_transport_failed",
  );
  assert.equal(codeReads, 1);
  assert.equal(pairBodies.length, 2);
  assert.equal(pairBodies[0], pairBodies[1]);
  await assert.rejects(stat(item.statePath), error => error.code === "ENOENT");
  assert.equal((await readdir(item.directory)).some(name => name.includes("pending")), false);
});

test("a new process reconstructs the exact same grant request after a lost pair response", async t => {
  const item = await fixture(t, "pair-process-recovery");
  const pairBodies = [];
  const firstFetch = async (url, init) => {
    if (new URL(url).pathname === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    pairBodies.push(init.body);
    throw new TypeError("response lost after commit");
  };
  await assert.rejects(
    pairAgent(item.config, item.identity, {
      readPairingCode: async () => SECRET_PAIR_CODE,
      fetchImplementation: firstFetch,
      nowUnix: NOW + 1,
    }),
    error => error.code === "agent_bridge_transport_failed",
  );
  const secondFetch = async (url, init) => {
    if (new URL(url).pathname === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    pairBodies.push(init.body);
    const request = JSON.parse(init.body).binding_request;
    return jsonResponse({
      binding: {
        ...item.binding,
        binding_id: request.binding_id,
        player_id: request.player_id,
      },
    });
  };
  const recovered = await pairAgent(item.config, item.identity, {
    readPairingCode: async () => SECRET_PAIR_CODE,
    fetchImplementation: secondFetch,
    nowUnix: NOW + 2,
  });
  assert.equal(pairBodies.length, 3);
  assert.equal(pairBodies[0], pairBodies[1]);
  assert.equal(pairBodies[1], pairBodies[2]);
  assert.equal(
    recovered.binding_id,
    JSON.parse(pairBodies[0]).binding_request.binding_id,
  );
});

test("same binding state can be atomically refreshed after an Agent key rotation", async t => {
  const item = await fixture(t, "pair-key-rotation");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const rotatedPath = join(item.directory, "rotated.identity.json");
  await generateIdentity(item.identity.agent_id, rotatedPath);
  const rotatedIdentity = await loadIdentity(rotatedPath);
  assert.notEqual(rotatedIdentity.agent_key_id, item.identity.agent_key_id);
  const fetchImplementation = async (url, init) => {
    if (new URL(url).pathname === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    const request = JSON.parse(init.body).binding_request;
    assert.equal(request.binding_id, BINDING_ID);
    return jsonResponse({
      binding: {
        ...item.binding,
        binding_id: BINDING_ID,
        agent_id: rotatedIdentity.agent_id,
        agent_key_id: rotatedIdentity.agent_key_id,
        agent_public_key: rotatedIdentity.agent_public_key,
        agent_public_key_hash: rotatedIdentity.agent_public_key_hash,
      },
    });
  };
  const refreshed = await pairAgent(item.config, rotatedIdentity, {
    readPairingCode: async () => SECRET_PAIR_CODE,
    fetchImplementation,
    nowUnix: NOW + 1,
  });
  assert.equal(refreshed.binding_id, BINDING_ID);
  assert.equal(refreshed.agent_key_id, rotatedIdentity.agent_key_id);
  await assert.rejects(loadBridgeState(item.statePath, item.identity));
  assert.equal(
    (await loadBridgeState(item.statePath, rotatedIdentity)).agent_key_id,
    rotatedIdentity.agent_key_id,
  );
});

test("binding, health, and inbox use only dedicated proof-authenticated endpoints", async t => {
  const item = await fixture(t, "dedicated-endpoints");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const calls = [];
  const fetchImplementation = async (url, init) => {
    const path = new URL(url).pathname;
    calls.push({ path, headers: headersObject(init.headers), body: init.body ?? null });
    if (path === "/api/agent-bridge/binding") return jsonResponse(item.binding);
    if (path === "/api/agent-bridge/health") return jsonResponse({ accepted: true });
    if (path === "/api/agent-bridge/inbox") return jsonResponse({ tasks: [] });
    return jsonResponse({ error: "not_found" }, 404);
  };
  assert.equal(
    (await getBinding(item.config, item.identity, fetchImplementation)).binding_id,
    BINDING_ID,
  );
  assert.deepEqual(
    await bridgeHealth(item.config, item.identity, {
      status: "healthy",
      nowUnix: NOW + 1,
      fetchImplementation,
    }),
    { accepted: true },
  );
  assert.deepEqual(
    await getInbox(item.config, item.identity, {
      nowUnix: NOW + 2,
      fetchImplementation,
    }),
    { tasks: [] },
  );
  assert.deepEqual(calls.map(call => call.path), [
    "/api/agent-bridge/binding",
    "/api/agent-bridge/health",
    "/api/agent-bridge/inbox",
  ]);
  assert.equal(calls[0].body, null);
  assert.deepEqual(JSON.parse(calls[1].body), {
    schema: HEALTH_REPORT_SCHEMA,
    assurance: "self_declared_unverified",
    status: "healthy",
    observed_at_unix: NOW + 1,
  });
  assert.deepEqual(JSON.parse(calls[2].body), {
    schema: INBOX_REQUEST_SCHEMA,
    paper_ids: [PAPER_ID],
  });
  for (const call of calls) {
    assert.equal(typeof call.headers["x-paper-raid-agent-signature"], "string");
    assert.equal("cookie" in call.headers, false);
    assert.equal("authorization" in call.headers, false);
    assert.equal("x-paper-raid-csrf" in call.headers, false);
  }
});

test("proposal endpoint carries exact Hepta payload plus independent request proof", async t => {
  const item = await fixture(t, "proposal");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    NOW,
  );
  const input = {
    paper_id: PAPER_ID,
    work_item_id: "88888888-8888-4888-8888-888888888888",
    section_key: "methods",
    parent_revision_id: "99999999-9999-4999-8999-999999999999",
    proposal_kind: "delivery",
    payload_hash: `sha256:${"aa".repeat(32)}`,
    artifact_manifest_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    artifact_manifest_hash: `sha256:${"bb".repeat(32)}`,
  };
  const exact = createProposalRequest(state, item.identity, input, {
    nowUnix: NOW + 10,
    proposalId: PROPOSAL_ID,
    idempotencyKey: IDEMPOTENCY_ID,
  });
  assert.equal(
    item.identity.verify(
      agentProposalFrame({
        schema: AGENT_PROPOSAL_SCHEMA,
        proposal_id: exact.payload.proposal_id,
        paper_project_id: exact.paper_id,
        work_item_id: exact.payload.work_item_id,
        section_key: exact.payload.section_key,
        parent_revision_id: exact.payload.parent_revision_id,
        proposal_kind: exact.payload.proposal_kind,
        payload_hash: exact.payload.payload_hash,
        artifact_manifest_hash: input.artifact_manifest_hash,
        agent_id: exact.payload.agent_id,
        binding_id: exact.payload.binding_id,
        agent_key_id: exact.payload.agent_key_id,
        signed_at_unix: exact.payload.signed_at_unix,
      }),
      exact.payload.signature,
    ),
    true,
  );
  let seen;
  const fetchImplementation = async (url, init) => {
    seen = {
      path: new URL(url).pathname,
      body: JSON.parse(init.body),
      headers: headersObject(init.headers),
    };
    return jsonResponse({ proposal_id: PROPOSAL_ID }, 201);
  };
  const result = await submitAgentProposal(item.config, item.identity, input, {
    nowUnix: NOW + 10,
    proposalId: PROPOSAL_ID,
    idempotencyKey: IDEMPOTENCY_ID,
    fetchImplementation,
  });
  assert.deepEqual(result, { proposal_id: PROPOSAL_ID });
  assert.equal(seen.path, "/api/agent-bridge/proposals");
  assert.deepEqual(seen.body, exact);
  assert.equal(typeof seen.headers["x-paper-raid-agent-signature"], "string");
  assert.equal("cookie" in seen.headers, false);
  assert.equal("authorization" in seen.headers, false);
});
