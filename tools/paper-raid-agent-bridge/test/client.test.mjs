import assert from "node:assert/strict";
import test from "node:test";
import {
  AGENT_BRIDGE_ENDPOINTS,
  AgentBridgeClient,
} from "../src/client.mjs";
import { saveBridgeState } from "../src/state.mjs";
import { fixture, headersObject, jsonResponse } from "./helpers.mjs";

test("signed transport retries lost response with byte-identical headers and body", async t => {
  const { config, identity, binding } = await fixture(t, "signed-replay");
  const state = await saveBridgeState(
    config.state_file,
    identity,
    binding,
    1_700_000_000,
  );
  const seen = [];
  const fetchImplementation = async (url, init) => {
    seen.push({
      url: String(url),
      method: init.method,
      headers: headersObject(init.headers),
      body: init.body ?? null,
    });
    if (seen.length === 1) throw new TypeError("simulated lost response");
    return jsonResponse({ ok: true });
  };
  const client = new AgentBridgeClient(
    config.bff_url,
    config.request_timeout_ms,
    fetchImplementation,
  );
  const value = await client.signed(identity, state, {
    method: "POST",
    path: AGENT_BRIDGE_ENDPOINTS.inbox,
    body: {
      schema: "hepta.paper_raid.agent_bridge.inbox_request.v1",
      paper_ids: [],
    },
    nowUnix: 1_700_000_100,
    nonce: "77777777-7777-4777-8777-777777777777",
  });
  assert.deepEqual(value, { ok: true });
  assert.equal(seen.length, 2);
  assert.deepEqual(seen[0], seen[1]);
  assert.equal(seen[0].url.endsWith("/api/agent-bridge/inbox"), true);
  assert.equal(
    seen[0].body,
    '{"paper_ids":[],"schema":"hepta.paper_raid.agent_bridge.inbox_request.v1"}',
  );
  for (const forbidden of [
    "authorization",
    "cookie",
    "x-paper-raid-csrf",
    "x-csrf-token",
  ]) {
    assert.equal(forbidden in seen[0].headers, false);
  }
  for (const required of [
    "x-paper-raid-agent-schema",
    "x-paper-raid-agent-binding-id",
    "x-paper-raid-agent-id",
    "x-paper-raid-agent-key-id",
    "x-paper-raid-agent-nonce",
    "x-paper-raid-agent-issued-at",
    "x-paper-raid-agent-expires-at",
    "x-paper-raid-agent-body-sha256",
    "x-paper-raid-agent-signature",
  ]) {
    assert.equal(typeof seen[0].headers[required], "string");
  }
});

test("client has no login/session route and restricts unauthenticated POSTs to pairing", async () => {
  const paths = [];
  const client = new AgentBridgeClient("http://127.0.0.1:8088", 2_000, async url => {
    paths.push(new URL(url).pathname);
    return jsonResponse({ ok: true });
  });
  await client.publicPost(AGENT_BRIDGE_ENDPOINTS.pairing_context, {
    pairing_code: "never-output-this-code",
  });
  assert.deepEqual(paths, ["/api/agent-bridge/pairing-context"]);
  assert.equal(paths.includes("/alpha/login"), false);
  await assert.rejects(
    client.publicPost(AGENT_BRIDGE_ENDPOINTS.health, { status: "healthy" }),
    /restricted to pairing endpoints/,
  );
});

test("pairing HTTP errors never reflect response bodies", async () => {
  const secret = "prg1.must-not-be-reflected";
  const client = new AgentBridgeClient(
    "http://127.0.0.1:8088",
    2_000,
    async () => jsonResponse({ error: secret, detail: secret }, 403),
  );
  await assert.rejects(
    client.publicPost(AGENT_BRIDGE_ENDPOINTS.pairing_context, {
      pairing_code: secret,
    }),
    error =>
      error.code === "agent_bridge_request_failed" &&
      !error.message.includes(secret),
  );
});
