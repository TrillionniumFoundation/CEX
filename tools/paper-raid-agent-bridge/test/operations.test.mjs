import assert from "node:assert/strict";
import { chmod, lstat, readdir, readFile, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import {
  AGENT_PROPOSAL_SCHEMA,
  AGENT_BINDING_PROOF_SCHEMA,
  agentBindingProofFrame,
  agentCapabilityDisclosureHash,
  agentProposalFrame,
  canonicalJsonBytes,
  sha256Digest,
} from "../src/canonical.mjs";
import {
  HEALTH_REPORT_SCHEMA,
  INBOX_REQUEST_SCHEMA,
  PAIRING_COMPLETION_SCHEMA,
  bridgeHealth,
  createDeliveryDraftRequest,
  createProposalRequest,
  downloadChallengeMaterialBundle,
  executeAndSubmitAuthorWorkStart,
  executePracticeAuto,
  getBinding,
  getInbox,
  getPractice,
  pairAgent,
  pairAgentAndCheckHealth,
  prepareChallengeMaterialsForStart,
  prepareDeliveryDraft,
  stableDeliveryDraftId,
  submitAgentProposal,
} from "../src/operations.mjs";
import {
  PRACTICE_CLAIM_REQUEST_SCHEMA,
  PRACTICE_MATERIALS_SCHEMA,
  PRACTICE_RESULT_REQUEST_SCHEMA,
  PRACTICE_TASKS_SCHEMA,
  PRACTICE_TASK_SCHEMA,
  PRACTICE_TRANSITION_RESULT_SCHEMA,
} from "../src/practice.mjs";
import { authorWorkStarts } from "../src/work.mjs";
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
const WORK_ID = "88888888-8888-4888-8888-888888888888";

function practiceTaskResponse(
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

function hashField(value, field) {
  const frame = { ...value };
  delete frame[field];
  return { ...value, [field]: sha256Digest(canonicalJsonBytes(frame)) };
}

function challengeMaterialFixture() {
  const bytes = Object.freeze({
    brief: Buffer.from("# brief\n"),
    dataset: Buffer.from('{"rows":[1]}'),
    baseline: Buffer.from("print('baseline')\n"),
    evaluator: Buffer.from("print('evaluate')\n"),
  });
  const authority = hashField({
    schema: "hepta.paper_raid.frozen_challenge_material_authority.v1",
    authority_hash: `sha256:${"0".repeat(64)}`,
    activation_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    activation_request_sha256: `sha256:${"1".repeat(64)}`,
    challenge_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    challenge_snapshot_hash: `sha256:${"2".repeat(64)}`,
    template: "evidence-audit",
    pack_id: "paper-raid-evidence-audit-seeded-v1",
    pack_manifest_hash: `sha256:${"3".repeat(64)}`,
    ruleset_version: "paper-raid-evidence-audit-v1",
    ruleset_hash: `sha256:${"4".repeat(64)}`,
    dataset_manifest_hash: `sha256:${"5".repeat(64)}`,
    evaluator_manifest_hash: `sha256:${"6".repeat(64)}`,
  }, "authority_hash");
  const objects = [
    ["brief", "playable_brief", "challenge/brief.md", "text/markdown; charset=utf-8"],
    ["dataset", "dataset", "challenge/dataset.json", "application/json"],
    ["baseline", "baseline_code", "challenge/baseline.py", "text/x-python; charset=utf-8"],
    ["evaluator", "frozen_evaluator", "challenge/evaluator.py", "text/x-python; charset=utf-8"],
  ].map(([key, role, logicalPath, mediaType]) => ({
    object_key: key,
    source_path: `source/${key}`,
    logical_path: logicalPath,
    role,
    digest: sha256Digest(bytes[key]),
    size_bytes: bytes[key].length,
    media_type: mediaType,
    download_path: "/api/agent-bridge/challenge-objects",
  }));
  const bundle = hashField({
    schema: "hepta.paper_raid.assigned_challenge_material_bundle.v1",
    bundle_hash: `sha256:${"7".repeat(64)}`,
    authority,
    authority_hash: authority.authority_hash,
    paper_project_id: PAPER_ID,
    challenge_ruleset_snapshot_hash: `sha256:${"8".repeat(64)}`,
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    work_item_id: WORK_ID,
    work_item_version: 3,
    objects,
  }, "bundle_hash");
  return { bundle, bytes };
}

function challengeDeliveryInbox(bundle) {
  const candidate = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidate.v1",
    delivery_draft_id: "99999999-9999-4999-8999-999999999999",
    binding_id: BINDING_ID,
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    lease_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    lease_fencing_token: 1,
    expected_work_version: 3,
    parent_revision_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    proposal_kind: "delivery",
    payload_hash: `sha256:${"c".repeat(64)}`,
    artifact_manifest_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
    artifact_manifest_hash: `sha256:${"d".repeat(64)}`,
    delivery_state: "pending",
    declared_at_unix: NOW,
    expires_at_unix: NOW + 300,
  };
  return {
    candidate,
    inbox: {
      schema: "hepta.paper_raid.agent_bridge.inbox.v2",
      binding_id: BINDING_ID,
      assurance: "self_declared_unverified",
      papers: [{
        paper_id: PAPER_ID,
        phase: "drafting",
        tasks: [{
          work_item_id: WORK_ID,
          paper_project_id: PAPER_ID,
          kind: "assigned_research",
          assigned_binding_id: BINDING_ID,
          assigned_player_id: PLAYER_ID,
          status: "in_progress",
          version: 3,
        }],
        challenge_materials: {
          schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
          status: "available",
          reason_code: null,
          items: [bundle],
        },
        delivery_candidates: {
          schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
          status: "available",
          reason_code: null,
          items: [candidate],
        },
      }],
    },
  };
}

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

test("pair completion observes one signed self-declared health report without reflecting peer bytes", async t => {
  const item = await fixture(t, "pair-health-confirmed");
  const paths = [];
  let healthHeaders;
  let healthBody;
  const fetchImplementation = async (url, init) => {
    const path = new URL(url).pathname;
    paths.push(path);
    if (path === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    if (path === "/api/agent-bridge/pair") {
      const request = JSON.parse(init.body).binding_request;
      return jsonResponse({
        binding: {
          ...item.binding,
          binding_id: request.binding_id,
          player_id: request.player_id,
        },
      });
    }
    assert.equal(path, "/api/agent-bridge/health");
    assert.equal((await stat(item.statePath)).mode & 0o777, 0o600);
    healthHeaders = headersObject(init.headers);
    healthBody = JSON.parse(init.body);
    return jsonResponse({ reflected_pairing_code: SECRET_PAIR_CODE });
  };
  const result = await pairAgentAndCheckHealth(item.config, item.identity, {
    readPairingCode: async () => SECRET_PAIR_CODE,
    fetchImplementation,
    nowUnix: NOW,
    bindingId: BINDING_ID,
    nonce: NONCE,
  });
  assert.deepEqual(paths, [
    "/api/agent-bridge/pairing-context",
    "/api/agent-bridge/pair",
    "/api/agent-bridge/health",
  ]);
  assert.deepEqual(healthBody, {
    schema: HEALTH_REPORT_SCHEMA,
    assurance: "self_declared_unverified",
    status: "healthy",
    observed_at_unix: NOW,
  });
  assert.equal(typeof healthHeaders["x-paper-raid-agent-signature"], "string");
  assert.equal("authorization" in healthHeaders, false);
  assert.equal("cookie" in healthHeaders, false);
  assert.equal(result.schema, PAIRING_COMPLETION_SCHEMA);
  assert.equal(result.pairing, "paired");
  assert.equal(result.signed_health, "observed");
  assert.equal(result.binding.binding_id, BINDING_ID);
  assert.equal(JSON.stringify(result).includes(SECRET_PAIR_CODE), false);
  assert.equal(JSON.stringify(result).includes(context().subject_id), false);
});

test("pair completion remains health-pending after a hostile failed health response", async t => {
  const item = await fixture(t, "pair-health-pending");
  const fetchImplementation = async (url, init) => {
    const path = new URL(url).pathname;
    if (path === "/api/agent-bridge/pairing-context") {
      return jsonResponse(context());
    }
    if (path === "/api/agent-bridge/pair") {
      const request = JSON.parse(init.body).binding_request;
      return jsonResponse({
        binding: {
          ...item.binding,
          binding_id: request.binding_id,
          player_id: request.player_id,
        },
      });
    }
    assert.equal(path, "/api/agent-bridge/health");
    return jsonResponse({ error: `health rejected ${SECRET_PAIR_CODE}` }, 503);
  };
  const result = await pairAgentAndCheckHealth(item.config, item.identity, {
    readPairingCode: async () => SECRET_PAIR_CODE,
    fetchImplementation,
    nowUnix: NOW,
    bindingId: BINDING_ID,
    nonce: NONCE,
  });
  assert.equal(result.pairing, "paired");
  assert.equal(result.signed_health, "pending");
  assert.equal(JSON.stringify(result).includes(SECRET_PAIR_CODE), false);
  assert.equal((await loadBridgeState(item.statePath, item.identity)).binding_id, BINDING_ID);
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

test("practice-auto signs discover-claim-result and retries lost responses byte-exactly", async t => {
  const item = await fixture(t, "practice-auto-exact-replay");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  let taskState = "pending";
  let version = 3;
  let resultCode = null;
  let claimAttempts = 0;
  let resultAttempts = 0;
  const calls = [];
  const fetchImplementation = async (url, init) => {
    const path = new URL(url).pathname;
    const call = {
      path,
      headers: headersObject(init.headers),
      body: init.body,
    };
    calls.push(call);
    for (const forbidden of ["authorization", "cookie", "x-paper-raid-csrf"]) {
      assert.equal(forbidden in call.headers, false);
    }
    assert.equal(typeof call.headers["x-paper-raid-agent-signature"], "string");
    if (path === "/api/agent-bridge/practice-tasks") {
      assert.deepEqual(JSON.parse(init.body), {
        schema: "hepta.paper_raid.agent_bridge.practice_task_query.v1",
      });
      return jsonResponse(practiceTaskResponse(taskState, version, resultCode));
    }
    if (path === "/api/agent-bridge/practice-claims") {
      assert.deepEqual(JSON.parse(init.body), {
        schema: PRACTICE_CLAIM_REQUEST_SCHEMA,
        expected_version: 3,
        task_token: `sha256:${"a".repeat(64)}`,
      });
      claimAttempts += 1;
      taskState = "claimed";
      version = 4;
      if (claimAttempts === 1) throw new TypeError("claim response lost after commit");
      return jsonResponse({
        schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
        operation: "claim",
        status: "claimed",
        from_version: 3,
        version: 4,
      });
    }
    assert.equal(path, "/api/agent-bridge/practice-results");
    assert.deepEqual(JSON.parse(init.body), {
      schema: PRACTICE_RESULT_REQUEST_SCHEMA,
      expected_version: 4,
      task_token: `sha256:${"b".repeat(64)}`,
      result_code: "concern_confirmed",
    });
    resultAttempts += 1;
    taskState = "completed";
    version = 5;
    resultCode = "concern_confirmed";
    if (resultAttempts === 1) throw new TypeError("result response lost after commit");
    return jsonResponse({
      schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
      operation: "result",
      status: "completed",
      from_version: 4,
      version: 5,
      result_code: resultCode,
    });
  };

  const result = await executePracticeAuto(item.config, item.identity, {
    nowUnix: NOW,
    fetchImplementation,
  });
  assert.deepEqual(result, {
    schema: "hepta.paper_raid.agent_bridge.practice_auto_result.v1",
    mode: "practice_unranked",
    status: "completed",
    task_state: "completed",
    version: 5,
    result_code: "concern_confirmed",
  });
  assert.equal(claimAttempts, 2);
  assert.equal(resultAttempts, 2);
  const claimCalls = calls.filter(call => call.path.endsWith("practice-claims"));
  const resultCalls = calls.filter(call => call.path.endsWith("practice-results"));
  assert.deepEqual(claimCalls[0], claimCalls[1]);
  assert.deepEqual(resultCalls[0], resultCalls[1]);
  assert.deepEqual(calls.map(call => call.path), [
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-claims",
    "/api/agent-bridge/practice-claims",
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-results",
    "/api/agent-bridge/practice-results",
    "/api/agent-bridge/practice-tasks",
  ]);
  for (const call of calls.filter(call => !call.path.endsWith("practice-tasks"))) {
    for (const forbidden of [
      "practice_session_id",
      "subject_id",
      "player_id",
      "binding_id",
      "bridge_task_id",
      "event_id",
      "request_hash",
      "result_hash",
    ]) assert.equal(call.body.includes(forbidden), false, `request leaked ${forbidden}`);
  }
});

test("practice-auto recovers completed state before any new execution", async t => {
  const item = await fixture(t, "practice-auto-process-recovery");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  let taskState = "pending";
  let version = 3;
  let resultCode = null;
  let claimCalls = 0;
  let resultCalls = 0;
  const firstFetch = async (url, init) => {
    const path = new URL(url).pathname;
    if (path.endsWith("practice-tasks")) {
      return jsonResponse(practiceTaskResponse(taskState, version, resultCode));
    }
    if (path.endsWith("practice-claims")) {
      claimCalls += 1;
      taskState = "claimed";
      version = 4;
      return jsonResponse({
        schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
        operation: "claim",
        status: "claimed",
        from_version: 3,
        version: 4,
      });
    }
    assert.equal(path.endsWith("practice-results"), true);
    resultCalls += 1;
    taskState = "completed";
    version = 5;
    resultCode = "concern_confirmed";
    throw new TypeError("result committed but both responses vanished");
  };
  await assert.rejects(
    executePracticeAuto(item.config, item.identity, {
      nowUnix: NOW + 1,
      fetchImplementation: firstFetch,
    }),
    error => error?.code === "agent_bridge_transport_failed",
  );
  assert.equal(claimCalls, 1);
  assert.equal(resultCalls, 2, "one exact signed result was retried once");

  const secondPaths = [];
  const recovered = await executePracticeAuto(item.config, item.identity, {
    nowUnix: NOW + 2,
    fetchImplementation: async url => {
      const path = new URL(url).pathname;
      secondPaths.push(path);
      assert.equal(path, "/api/agent-bridge/practice-tasks");
      return jsonResponse(practiceTaskResponse(taskState, version, resultCode));
    },
  });
  assert.deepEqual(recovered, {
    schema: "hepta.paper_raid.agent_bridge.practice_auto_result.v1",
    mode: "practice_unranked",
    status: "already_completed",
    task_state: "completed",
    version: 5,
    result_code: "concern_confirmed",
  });
  assert.deepEqual(secondPaths, ["/api/agent-bridge/practice-tasks"]);
  assert.equal(claimCalls, 1, "recovery issued a second claim");
  assert.equal(resultCalls, 2, "recovery issued a second result operation");

  const readOnly = await getPractice(item.config, item.identity, {
    nowUnix: NOW + 3,
    fetchImplementation: async url => {
      assert.equal(new URL(url).pathname, "/api/agent-bridge/practice-tasks");
      return jsonResponse(practiceTaskResponse(taskState, version, resultCode));
    },
  });
  assert.equal(readOnly.task.state, "completed");
});

test("practice-auto echoes the discovered token and rejects same-version session replacement", async t => {
  const item = await fixture(t, "practice-auto-aba-rejected");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const tokenA = `sha256:${"a".repeat(64)}`;
  const tokenB = `sha256:${"d".repeat(64)}`;
  const calls = [];
  await assert.rejects(
    executePracticeAuto(item.config, item.identity, {
      nowUnix: NOW + 4,
      fetchImplementation: async (url, init) => {
        const path = new URL(url).pathname;
        calls.push({ path, body: JSON.parse(init.body) });
        if (path.endsWith("practice-tasks")) {
          return jsonResponse(practiceTaskResponse("pending", 3, null, tokenA));
        }
        assert.equal(path, "/api/agent-bridge/practice-claims");
        assert.deepEqual(JSON.parse(init.body), {
          schema: PRACTICE_CLAIM_REQUEST_SCHEMA,
          expected_version: 3,
          task_token: tokenA,
        });
        return jsonResponse(
          { error: "practice_agent_task_changed", current_task_token: tokenB },
          409,
        );
      },
    }),
    error => error?.code === "agent_bridge_request_failed",
  );
  assert.deepEqual(calls.map(call => call.path), [
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-claims",
  ]);
  assert.equal(
    calls.some(call => call.path.endsWith("practice-results")),
    false,
    "same-version replacement reached result execution",
  );
});

test("practice-auto obtains a fresh signing time for every new operation", async t => {
  const item = await fixture(t, "practice-auto-fresh-clock");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const clockValues = [NOW, NOW + 70, NOW + 140, NOW + 210, NOW + 280];
  const observedIssuedAt = [];
  let taskState = "pending";
  let version = 3;
  let resultCode = null;
  const result = await executePracticeAuto(item.config, item.identity, {
    clock: () => clockValues.shift(),
    fetchImplementation: async (url, init) => {
      const path = new URL(url).pathname;
      observedIssuedAt.push(
        Number(headersObject(init.headers)["x-paper-raid-agent-issued-at"]),
      );
      if (path.endsWith("practice-tasks")) {
        return jsonResponse(practiceTaskResponse(taskState, version, resultCode));
      }
      if (path.endsWith("practice-claims")) {
        taskState = "claimed";
        version = 4;
        return jsonResponse({
          schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
          operation: "claim",
          status: "claimed",
          from_version: 3,
          version: 4,
        });
      }
      assert.equal(path, "/api/agent-bridge/practice-results");
      taskState = "completed";
      version = 5;
      resultCode = "concern_confirmed";
      return jsonResponse({
        schema: PRACTICE_TRANSITION_RESULT_SCHEMA,
        operation: "result",
        status: "completed",
        from_version: 4,
        version: 5,
        result_code: resultCode,
      });
    },
  });
  assert.equal(result.status, "completed");
  assert.deepEqual(observedIssuedAt, [NOW, NOW + 70, NOW + 140, NOW + 210, NOW + 280]);
  assert.equal(clockValues.length, 0);
});

test("Bridge downloads all four challenge objects through exact signed assignment queries", async t => {
  const item = await fixture(t, "challenge-material-download");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const { bundle, bytes } = challengeMaterialFixture();
  const calls = [];
  const downloaded = await downloadChallengeMaterialBundle(
    item.config,
    item.identity,
    bundle,
    {
      nowUnix: NOW + 3,
      fetchImplementation: async (url, init) => {
        const parsed = new URL(url);
        const key = parsed.searchParams.get("object_key");
        calls.push({
          path: parsed.pathname,
          query: Object.fromEntries(parsed.searchParams.entries()),
          headers: headersObject(init.headers),
        });
        return new Response(bytes[key], {
          status: 200,
          headers: {
            "content-type": bundle.objects.find(object => object.object_key === key).media_type,
            "content-length": String(bytes[key].length),
          },
        });
      },
    },
  );
  assert.deepEqual(downloaded.map(value => value.descriptor.object_key), [
    "brief",
    "dataset",
    "baseline",
    "evaluator",
  ]);
  assert.equal(calls.length, 4);
  for (const [index, call] of calls.entries()) {
    const object = bundle.objects[index];
    assert.equal(call.path, "/api/agent-bridge/challenge-objects");
    assert.deepEqual(call.query, {
      bundle_hash: bundle.bundle_hash,
      digest: object.digest,
      object_key: object.object_key,
      paper_id: PAPER_ID,
      work_item_id: WORK_ID,
    });
    assert.equal(typeof call.headers["x-paper-raid-agent-signature"], "string");
    assert.equal(call.headers["x-paper-raid-agent-binding-id"], BINDING_ID);
    assert.equal("cookie" in call.headers, false);
    assert.equal("authorization" in call.headers, false);
  }

  await assert.rejects(
    () => downloadChallengeMaterialBundle(
      item.config,
      item.identity,
      bundle,
      {
        nowUnix: NOW + 4,
        fetchImplementation: async (url) => {
          const key = new URL(url).searchParams.get("object_key");
          return new Response(bytes[key], {
            status: 200,
            headers: {
              "content-type": key === "baseline"
                ? "application/octet-stream"
                : bundle.objects.find(object => object.object_key === key).media_type,
              "content-length": String(bytes[key].length),
            },
          });
        },
      },
    ),
    error => error?.code === "agent_bridge_object_media_type_mismatch",
  );
});

test("diagnostic Author-start material preparation publishes only after all four signed reads", async t => {
  const item = await fixture(t, "challenge-material-prepare");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const { bundle, bytes } = challengeMaterialFixture();
  const initial = challengeDeliveryInbox(bundle).inbox;
  initial.papers[0].proposals = [];
  initial.papers[0].delivery_candidates = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
    status: "unavailable",
    reason_code: "no_recoverable_author_delivery",
    items: [],
  };
  const [start] = authorWorkStarts(initial);
  let reads = 0;
  const result = await prepareChallengeMaterialsForStart(
    item.config,
    item.identity,
    start,
    {
      nowUnix: NOW + 5,
      fetchImplementation: async url => {
        const key = new URL(url).searchParams.get("object_key");
        reads += 1;
        return new Response(bytes[key], {
          status: 200,
          headers: {
            "content-type": bundle.objects.find(object => object.object_key === key).media_type,
            "content-length": String(bytes[key].length),
          },
        });
      },
    },
  );
  assert.equal(reads, 4);
  assert.equal(result.status, "created");
  assert.equal((await lstat(result.directory)).mode & 0o777, 0o700);

  const failed = await fixture(t, "challenge-material-partial-network");
  await saveBridgeState(failed.statePath, failed.identity, failed.binding, NOW);
  let attempts = 0;
  await assert.rejects(
    () => prepareChallengeMaterialsForStart(
      failed.config,
      failed.identity,
      start,
      {
        nowUnix: NOW + 6,
        fetchImplementation: async url => {
          const key = new URL(url).searchParams.get("object_key");
          attempts += 1;
          if (key === "baseline") {
            throw new Error("simulated interrupted material download");
          }
          return new Response(bytes[key], {
            status: 200,
            headers: {
              "content-type": bundle.objects.find(object => object.object_key === key).media_type,
              "content-length": String(bytes[key].length),
            },
          });
        },
      },
    ),
    error => error?.code === "agent_bridge_transport_failed",
  );
  await assert.rejects(
    lstat(join(failed.directory, "challenge-materials")),
    error => error?.code === "ENOENT",
  );
});

test("Author start materializes four objects before executor, draft, fresh inbox, and proposal", async t => {
  const item = await fixture(t, "author-start-operation");
  await saveBridgeState(item.statePath, item.identity, item.binding, NOW);
  const { bundle, bytes } = challengeMaterialFixture();
  const initial = challengeDeliveryInbox(bundle).inbox;
  initial.papers[0].proposals = [];
  initial.papers[0].delivery_candidates = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
    status: "unavailable",
    reason_code: "no_recoverable_author_delivery",
    items: [],
  };
  const [start] = authorWorkStarts(initial);
  const executorLog = join(item.directory, "author-executor.log");
  const executorPath = join(item.directory, "author-executor.mjs");
  await writeFile(executorPath, `#!${process.execPath}
import { appendFile, readFile } from "node:fs/promises";
let input = "";
for await (const chunk of process.stdin) input += chunk;
const request = JSON.parse(input);
for (const object of request.objects) await readFile(object.logical_path);
await appendFile(${JSON.stringify(executorLog)}, request.start_key + "\\n", { mode: 0o600 });
process.stdout.write(JSON.stringify({
  schema: "hepta.paper_raid.agent_bridge.author_executor_result.v1",
  status: "completed",
  start_key: request.start_key,
  material_directory: request.material_directory,
  binding_id: request.binding_id,
  player_id: request.player_id,
  paper_id: request.paper_id,
  work_item_id: request.work_item_id,
  work_item_version: request.work_item_version,
  task_kind: request.task_kind,
  bundle_hash: request.bundle_hash,
  authority_hash: request.authority_hash,
  section_key: "methods",
  artifact_manifest_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
  payload_hash: "sha256:${"c".repeat(64)}"
}) + "\\n");
`, { mode: 0o700 });
  await chmod(executorPath, 0o700);
  const config = Object.freeze({
    ...item.config,
    author_executor: Object.freeze({
      schema: "hepta.paper_raid.agent_bridge.author_executor.v1",
      executable: executorPath,
      timeout_ms: 5_000,
    }),
  });
  const calls = [];
  let exactCandidate;
  const fetchImplementation = async (url, init) => {
    const parsed = new URL(url);
    calls.push(`${init.method}:${parsed.pathname}`);
    if (parsed.pathname === "/api/agent-bridge/challenge-objects") {
      const key = parsed.searchParams.get("object_key");
      const object = bundle.objects.find(value => value.object_key === key);
      return new Response(bytes[key], {
        status: 200,
        headers: {
          "content-type": object.media_type,
          "content-length": String(bytes[key].length),
        },
      });
    }
    if (parsed.pathname === "/api/agent-bridge/delivery-drafts") {
      const request = JSON.parse(init.body);
      exactCandidate = {
        ...challengeDeliveryInbox(bundle).candidate,
        delivery_draft_id: request.delivery_draft_id,
        declared_at_unix: NOW + 5,
        expires_at_unix: NOW + 305,
      };
      return jsonResponse({
        schema: "hepta.paper_raid.agent_bridge.delivery_draft_result.v1",
        candidate: exactCandidate,
      });
    }
    if (parsed.pathname === "/api/agent-bridge/inbox") {
      const fresh = challengeDeliveryInbox(bundle).inbox;
      fresh.papers[0].proposals = [];
      fresh.papers[0].delivery_candidates.items = [exactCandidate];
      return jsonResponse(fresh);
    }
    assert.equal(parsed.pathname, "/api/agent-bridge/proposals");
    const proposal = JSON.parse(init.body);
    assert.equal(proposal.delivery_draft_id, exactCandidate.delivery_draft_id);
    assert.equal(proposal.payload.expected_work_version, 3);
    return jsonResponse({
      schema: "hepta.paper_raid.agent_bridge.proposal_result.v2",
      status: "accepted",
    });
  };

  const result = await executeAndSubmitAuthorWorkStart(
    config,
    item.identity,
    start,
    { nowUnix: NOW + 5, fetchImplementation },
  );
  assert.equal(result.schema, "hepta.paper_raid.agent_bridge.author_work_submission.v1");
  assert.equal(result.candidate.delivery_draft_id, exactCandidate.delivery_draft_id);
  assert.deepEqual(calls, [
    "GET:/api/agent-bridge/challenge-objects",
    "GET:/api/agent-bridge/challenge-objects",
    "GET:/api/agent-bridge/challenge-objects",
    "GET:/api/agent-bridge/challenge-objects",
    "POST:/api/agent-bridge/delivery-drafts",
    "POST:/api/agent-bridge/inbox",
    "POST:/api/agent-bridge/proposals",
  ]);
  assert.equal((await readFile(executorLog, "utf8")).trim().length > 0, true);
});

test("legacy non-delivery Agent Proposal V1 is not a Bridge mutation bypass", async t => {
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
    proposal_kind: "proposal",
    payload_hash: `sha256:${"aa".repeat(32)}`,
    artifact_manifest_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    artifact_manifest_hash: `sha256:${"bb".repeat(32)}`,
  };
  assert.throws(
    () => createProposalRequest(state, item.identity, input),
    /Proposal V2 requires a delivery_draft_id-bound delivery/,
  );
  await assert.rejects(
    submitAgentProposal(item.config, item.identity, input),
    /Proposal V2 requires a delivery_draft_id-bound delivery/,
  );
});

test("delivery draft is a stable Agent declaration and drives deterministic proposal recovery", async t => {
  const item = await fixture(t, "delivery-draft");
  const state = await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    NOW,
  );
  const draftInput = {
    paper_id: PAPER_ID,
    work_item_id: "88888888-8888-4888-8888-888888888888",
    section_key: "methods",
    artifact_manifest_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    payload_hash: `sha256:${"aa".repeat(32)}`,
  };
  const deliveryDraftId = stableDeliveryDraftId(state, draftInput);
  const exactDraft = createDeliveryDraftRequest(state, draftInput);
  assert.equal(exactDraft.delivery_draft_id, deliveryDraftId);
  let seenDraft;
  const draftResult = await prepareDeliveryDraft(item.config, item.identity, draftInput, {
    nowUnix: NOW + 5,
    fetchImplementation: async (url, init) => {
      seenDraft = {
        path: new URL(url).pathname,
        body: JSON.parse(init.body),
      };
      return jsonResponse({
        schema: "hepta.paper_raid.agent_bridge.delivery_draft_result.v1",
        candidate: { delivery_draft_id: deliveryDraftId },
      });
    },
  });
  assert.equal(seenDraft.path, "/api/agent-bridge/delivery-drafts");
  assert.deepEqual(seenDraft.body, exactDraft);
  assert.equal(draftResult.candidate.delivery_draft_id, deliveryDraftId);

  const proposalInput = {
    ...draftInput,
    delivery_draft_id: deliveryDraftId,
    declared_at_unix: NOW + 5,
    parent_revision_id: "99999999-9999-4999-8999-999999999999",
    lease_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    lease_fencing_token: 9,
    expected_work_version: 3,
    proposal_kind: "delivery",
    artifact_manifest_hash: `sha256:${"bb".repeat(32)}`,
  };
  const first = createProposalRequest(state, item.identity, proposalInput, {
    nowUnix: NOW + 10,
  });
  const recovered = createProposalRequest(state, item.identity, proposalInput, {
    nowUnix: NOW + 11,
  });
  assert.equal(first.delivery_draft_id, deliveryDraftId);
  assert.equal(first.payload.proposal_id, recovered.payload.proposal_id);
  assert.equal(first.payload.idempotency_key, recovered.payload.idempotency_key);
  assert.equal(first.payload.signed_at_unix, NOW + 5);
  assert.deepEqual(first, recovered);
  assert.equal(item.identity.verify(agentProposalFrame({
    schema: AGENT_PROPOSAL_SCHEMA,
    proposal_id: first.payload.proposal_id,
    paper_project_id: first.paper_id,
    work_item_id: first.payload.work_item_id,
    section_key: first.payload.section_key,
    parent_revision_id: first.payload.parent_revision_id,
    lease_id: first.payload.lease_id,
    lease_fencing_token: first.payload.lease_fencing_token,
    expected_work_version: first.payload.expected_work_version,
    proposal_kind: first.payload.proposal_kind,
    payload_hash: first.payload.payload_hash,
    artifact_manifest_id: first.payload.artifact_manifest_id,
    artifact_manifest_hash: proposalInput.artifact_manifest_hash,
    agent_id: first.payload.agent_id,
    binding_id: first.payload.binding_id,
    agent_key_id: first.payload.agent_key_id,
    signed_at_unix: first.payload.signed_at_unix,
  }), first.payload.signature), true);
  assert.throws(
    () => createProposalRequest(state, item.identity, {
      ...proposalInput,
      lease_fencing_token: 0,
    }),
    /lease_fencing_token is invalid/,
  );
  assert.throws(
    () => createProposalRequest(state, item.identity, proposalInput, {
      proposalId: PROPOSAL_ID,
    }),
    /proposal_id is fixed by delivery_draft_id/,
  );
  assert.throws(
    () => createProposalRequest(state, item.identity, proposalInput, {
      idempotencyKey: IDEMPOTENCY_ID,
    }),
    /idempotency_key is fixed by delivery_draft_id/,
  );
  assert.throws(
    () => createProposalRequest(state, item.identity, {
      ...proposalInput,
      proposal_kind: "proposal",
    }),
    /Proposal V2 requires a delivery_draft_id-bound delivery/,
  );
  assert.throws(
    () => createProposalRequest(state, item.identity, {
      ...proposalInput,
      delivery_draft_id: undefined,
    }),
    /Proposal V2 requires a delivery_draft_id-bound delivery/,
  );
});

test("delivery draft can fail closed on an ambiguous response without retrying", async t => {
  const item = await fixture(t, "delivery-draft-no-retry");
  await saveBridgeState(
    item.statePath,
    item.identity,
    item.binding,
    NOW,
  );
  const draftInput = {
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    artifact_manifest_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    payload_hash: `sha256:${"aa".repeat(32)}`,
  };
  let calls = 0;
  await assert.rejects(
    prepareDeliveryDraft(item.config, item.identity, draftInput, {
      nowUnix: NOW + 5,
      retryLostResponse: false,
      fetchImplementation: async () => {
        calls += 1;
        throw new Error("simulated lost response");
      },
    }),
    /agent_bridge_transport_failed/,
  );
  assert.equal(calls, 1);
});
