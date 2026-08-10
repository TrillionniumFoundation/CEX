import { randomUUID } from "node:crypto";
import {
  AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
  agentBridgeRequestProofFrame,
  canonicalJsonBytes,
  canonicalQuery,
  sha256Digest,
} from "./canonical.mjs";

const MAX_JSON_BYTES = 2 * 1024 * 1024;
const USER_AGENT = "paper-raid-agent-bridge/0.2";

export const AGENT_BRIDGE_ENDPOINTS = Object.freeze({
  pairing_context: "/api/agent-bridge/pairing-context",
  pair: "/api/agent-bridge/pair",
  binding: "/api/agent-bridge/binding",
  health: "/api/agent-bridge/health",
  inbox: "/api/agent-bridge/inbox",
  proposals: "/api/agent-bridge/proposals",
});

export class BridgeHttpError extends Error {
  constructor(code, status) {
    super(code);
    this.name = "BridgeHttpError";
    this.code = code;
    this.status = status;
  }
}

async function jsonResponse(response) {
  const bytes = Buffer.from(await response.arrayBuffer());
  if (bytes.length > MAX_JSON_BYTES) {
    throw new BridgeHttpError("agent_bridge_response_too_large", response.status);
  }
  if (bytes.length === 0) return null;
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new BridgeHttpError("agent_bridge_returned_non_json", response.status);
  }
}

function assertEndpointPath(path) {
  if (!Object.values(AGENT_BRIDGE_ENDPOINTS).includes(path)) {
    throw new Error("Agent Bridge client refused an unregistered endpoint");
  }
  return path;
}

export function createSignedRequest(
  identity,
  state,
  {
    method,
    path,
    query = "",
    body = null,
    nowUnix = Math.floor(Date.now() / 1000),
    nonce = randomUUID(),
  },
) {
  assertEndpointPath(path);
  const httpMethod = method.toUpperCase();
  const canonicalQueryValue = canonicalQuery(query);
  const bodyBytes = body === null ? Buffer.alloc(0) : canonicalJsonBytes(body);
  const bodyHash = sha256Digest(bodyBytes);
  const claim = Object.freeze({
    schema: AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
    binding_id: state.binding_id,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    http_method: httpMethod,
    canonical_path: path,
    canonical_query: canonicalQueryValue,
    body_hash: bodyHash,
    nonce,
    issued_at_unix: nowUnix,
    expires_at_unix: nowUnix + 60,
  });
  const headers = Object.freeze({
    accept: "application/json",
    "user-agent": USER_AGENT,
    "x-paper-raid-agent-schema": claim.schema,
    "x-paper-raid-agent-binding-id": claim.binding_id,
    "x-paper-raid-agent-id": claim.agent_id,
    "x-paper-raid-agent-key-id": claim.agent_key_id,
    "x-paper-raid-agent-nonce": claim.nonce,
    "x-paper-raid-agent-issued-at": String(claim.issued_at_unix),
    "x-paper-raid-agent-expires-at": String(claim.expires_at_unix),
    "x-paper-raid-agent-body-sha256": claim.body_hash,
    "x-paper-raid-agent-signature": identity.sign(
      agentBridgeRequestProofFrame(claim),
    ),
    ...(body === null ? {} : { "content-type": "application/json" }),
  });
  return Object.freeze({
    method: httpMethod,
    path,
    canonical_query: canonicalQueryValue,
    target: canonicalQueryValue ? `${path}?${canonicalQueryValue}` : path,
    body_bytes: bodyBytes,
    body_text: body === null ? null : bodyBytes.toString("utf8"),
    headers,
    claim,
  });
}

export class AgentBridgeClient {
  constructor(origin, timeoutMs, fetchImplementation = globalThis.fetch) {
    if (typeof fetchImplementation !== "function") {
      throw new Error("fetch is unavailable");
    }
    this.origin = origin;
    this.timeoutMs = timeoutMs;
    this.fetch = fetchImplementation;
  }

  async #sendExact(request, attempts) {
    let response;
    let value;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      try {
        response = await this.fetch(new URL(request.target, this.origin), {
          method: request.method,
          headers: request.headers,
          ...(request.body_text === null ? {} : { body: request.body_text }),
          redirect: "error",
          signal: AbortSignal.timeout(this.timeoutMs),
        });
        value = await jsonResponse(response);
        break;
      } catch (error) {
        if (
          error instanceof BridgeHttpError &&
          error.code !== "agent_bridge_returned_non_json"
        ) {
          throw error;
        }
        if (attempt + 1 === attempts) {
          throw error instanceof BridgeHttpError
            ? error
            : new BridgeHttpError("agent_bridge_transport_failed", 0);
        }
      }
    }
    if (!response.ok) {
      // Never reflect an upstream body into CLI errors. Pairing responses are
      // the only remote surface that has seen the one-time code, so even a
      // malicious/misconfigured peer cannot make the Bridge print it back.
      throw new BridgeHttpError("agent_bridge_request_failed", response.status);
    }
    return value;
  }

  async publicPost(path, body, { retryLostResponse = true } = {}) {
    assertEndpointPath(path);
    if (![
      AGENT_BRIDGE_ENDPOINTS.pairing_context,
      AGENT_BRIDGE_ENDPOINTS.pair,
    ].includes(path)) {
      throw new Error("public Agent Bridge POST is restricted to pairing endpoints");
    }
    const bodyBytes = canonicalJsonBytes(body);
    const request = Object.freeze({
      method: "POST",
      path,
      target: path,
      body_text: bodyBytes.toString("utf8"),
      headers: Object.freeze({
        accept: "application/json",
        "content-type": "application/json",
        "user-agent": USER_AGENT,
      }),
    });
    return this.#sendExact(request, retryLostResponse ? 2 : 1);
  }

  async signed(
    identity,
    state,
    request,
    { retryLostResponse = true } = {},
  ) {
    const exact = createSignedRequest(identity, state, request);
    return this.#sendExact(exact, retryLostResponse ? 2 : 1);
  }
}
