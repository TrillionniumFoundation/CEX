import assert from "node:assert/strict";
import { createPublicKey, verify } from "node:crypto";
import http from "node:http";

const playerId = process.env.PAPER_RAID_BFF_A11Y_PLAYER_ID;
const bindingId = process.env.PAPER_RAID_BFF_A11Y_BINDING_ID;
const agentDigest = process.env.PAPER_RAID_BFF_A11Y_AGENT_DIGEST;
const consumerPublicKey = process.env.PAPER_RAID_BFF_A11Y_CONSUMER_PUBLIC_KEY_B64;
const port = Number(process.env.PAPER_RAID_BFF_A11Y_HEPTA_PORT || "7011");
const uuidV4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

assert.match(playerId || "", uuidV4);
assert.match(bindingId || "", uuidV4);
assert.match(agentDigest || "", /^sha256:[0-9a-f]{64}$/);
assert.ok(Number.isSafeInteger(port) && port >= 1024 && port <= 65535);

const expectedSubjectId = "alpha-author-captain";
const expectedNakamaUserId = "00000000-0000-4000-8000-000000000001";
const expectedIssuer = "browser-mobile-a11y";
const expectedAudience = "hepta-research-league";
const expectedKeyId = "browser-mobile-a11y-key-1";
const emptyBodyHash = `sha256:${"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}`;
const digest = agentDigest;
const rawConsumerPublicKey = Buffer.from(consumerPublicKey || "", "base64");
assert.equal(rawConsumerPublicKey.length, 32);
assert.equal(rawConsumerPublicKey.toString("base64"), consumerPublicKey);
const consumerVerifyingKey = createPublicKey({
  key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), rawConsumerPublicKey]),
  format: "der",
  type: "spki",
});
const seenAssertionIds = new Set();
const capabilityDisclosure = Object.freeze({
  schema: "hepta.paper_raid.agent_capability_disclosure.v1",
  assurance: "self_declared_unverified",
  capabilities: ["experiment_execution"],
  resource_classes: ["cpu", "sandbox"],
  max_parallel_tasks: 1,
});
const binding = Object.freeze({
  binding_id: bindingId,
  player_id: playerId,
  agent_id: "did:trnm:agent:browser-a11y-prequalified-v1",
  agent_key_id: digest,
  agent_public_key_hash: digest,
  capability_disclosure_hash: digest,
  capability_disclosure: capabilityDisclosure,
  status: "active",
  version: 1,
  created_at: "2026-08-18T00:00:00Z",
  updated_at: "2026-08-18T00:00:00Z",
});

function sendJson(response, status, value) {
  const bytes = Buffer.from(`${JSON.stringify(value)}\n`, "utf8");
  response.writeHead(status, {
    "cache-control": "no-store",
    "content-length": String(bytes.length),
    "content-type": "application/json",
    "x-content-type-options": "nosniff",
  });
  response.end(bytes);
}

function exactKeys(value, expected) {
  return value && typeof value === "object" && !Array.isArray(value)
    && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...expected].sort());
}

function decodeCanonicalBase64(value, expectedBytes = null) {
  assert.equal(typeof value, "string");
  assert.match(value, /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/);
  const bytes = Buffer.from(value, "base64");
  assert.equal(bytes.toString("base64"), value);
  if (expectedBytes !== null) assert.equal(bytes.length, expectedBytes);
  return bytes;
}

function frameString(parts, value) {
  assert.equal(typeof value, "string");
  assert.ok(value.length > 0 && [...value].length <= 512 && !value.includes("\0"));
  const bytes = Buffer.from(value, "utf8");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(bytes.length);
  parts.push(length, bytes);
}

function assertionSigningBytes(claim, issuerKeyId) {
  const parts = [Buffer.from("hepta_consumer_edge_user_assertion_v2\0", "utf8")];
  for (const value of [
    claim.schema,
    claim.assertion_id,
    claim.issuer,
    claim.audience,
    claim.subject_id,
    claim.nakama_user_id,
    claim.player_id,
    claim.operation,
    claim.http_method,
    claim.canonical_path,
    claim.idempotency_key,
  ]) frameString(parts, value);
  parts.push(Buffer.from(claim.body_hash.slice("sha256:".length), "hex"));
  for (const value of [claim.issued_at_unix, claim.expires_at_unix]) {
    assert.ok(Number.isSafeInteger(value) && value >= 0);
    const encoded = Buffer.alloc(8);
    encoded.writeBigInt64BE(BigInt(value));
    parts.push(encoded);
  }
  frameString(parts, claim.nonce);
  frameString(parts, issuerKeyId);
  return Buffer.concat(parts);
}

const routeOperations = new Map([
  ["/v2/hepta/players/me", "get_self_human_player_v2"],
  ["/v2/hepta/agent-bindings", "list_self_agent_bindings_v2"],
  ["/v2/hepta/challenges", "list_public_challenges_v2"],
  ["/v2/hepta/matchmaking/tickets", "list_matchmaking_tickets_v3"],
  ["/v2/hepta/team-proposals", "list_team_proposals_v3"],
  ["/v2/hepta/raid-state", "get_player_raid_state_v1"],
]);

function assertionValid(request, canonicalPath) {
  try {
    const encoded = request.headers["x-hepta-user-assertion"];
    if (typeof encoded !== "string" || encoded.length < 64 || encoded.length > 16384) return false;
    const signed = JSON.parse(decodeCanonicalBase64(encoded).toString("utf8"));
    if (!exactKeys(signed, ["claim", "issuer_key_id", "signature"])) return false;
    if (!exactKeys(signed.claim, [
      "schema", "assertion_id", "issuer", "audience", "subject_id", "nakama_user_id",
      "player_id", "operation", "http_method", "canonical_path", "idempotency_key",
      "body_hash", "issued_at_unix", "expires_at_unix", "nonce",
    ])) return false;
    const claim = signed.claim;
    const now = Math.floor(Date.now() / 1000);
    if (
      claim.schema !== "hepta.consumer-edge.user-assertion.v2"
      || !uuidV4.test(claim.assertion_id)
      || claim.issuer !== expectedIssuer
      || claim.audience !== expectedAudience
      || claim.subject_id !== expectedSubjectId
      || claim.nakama_user_id !== expectedNakamaUserId
      || claim.player_id !== playerId
      || claim.operation !== routeOperations.get(canonicalPath)
      || claim.http_method !== "GET"
      || claim.canonical_path !== canonicalPath
      || !uuidV4.test(claim.idempotency_key)
      || claim.nonce !== claim.idempotency_key
      || claim.body_hash !== emptyBodyHash
      || !Number.isSafeInteger(claim.issued_at_unix)
      || !Number.isSafeInteger(claim.expires_at_unix)
      || claim.expires_at_unix - claim.issued_at_unix !== 30
      || claim.issued_at_unix > now + 2
      || claim.expires_at_unix < now
      || signed.issuer_key_id !== expectedKeyId
      || seenAssertionIds.has(claim.assertion_id)
    ) return false;
    const signature = decodeCanonicalBase64(signed.signature, 64);
    if (!verify(null, assertionSigningBytes(claim, signed.issuer_key_id), consumerVerifyingKey, signature)) return false;
    seenAssertionIds.add(claim.assertion_id);
    return true;
  } catch {
    return false;
  }
}

const routes = new Map([
  ["/ready", { ok: true }],
  ["/health", { ok: true }],
  ["/healthcheck", { ok: true }],
  ["/v2/hepta/players/me", {
    player_id: playerId,
    subject_id: "alpha-author-captain",
    signing_key_id: digest,
    signing_public_key: "browser-a11y-prequalified-public-key",
    signing_public_key_hash: digest,
  }],
  ["/v2/hepta/agent-bindings", [binding]],
  ["/v2/hepta/challenges", []],
  ["/v2/hepta/matchmaking/tickets", []],
  ["/v2/hepta/team-proposals", []],
  ["/v2/hepta/raid-state", { current_raid: null }],
]);

const server = http.createServer((request, response) => {
  let url;
  try {
    url = new URL(request.url || "", `http://127.0.0.1:${port}`);
  } catch {
    sendJson(response, 400, { error: "invalid_request_target" });
    return;
  }
  if (request.method !== "GET" || url.search || url.hash) {
    sendJson(response, 405, { error: "method_not_allowed" });
    return;
  }
  const value = routes.get(url.pathname);
  if (value === undefined) {
    sendJson(response, 404, { error: "not_found" });
    return;
  }
  if (!url.pathname.startsWith("/health") && url.pathname !== "/ready" && !assertionValid(request, url.pathname)) {
    sendJson(response, 401, { error: "consumer_assertion_invalid" });
    return;
  }
  sendJson(response, 200, value);
});

server.on("clientError", (_error, socket) => socket.destroy());
server.listen(port, "0.0.0.0", () => {
  process.stdout.write(`${JSON.stringify({
    schema: "hepta.paper_raid.browser_mobile_a11y.mock_ready.v1",
    port,
  })}\n`);
});
