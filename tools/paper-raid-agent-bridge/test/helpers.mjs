import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { generateIdentity, loadIdentity } from "../src/identity.mjs";
import { agentCapabilityDisclosureHash } from "../src/canonical.mjs";

export const PLAYER_ID = "11111111-1111-4111-8111-111111111111";
export const BINDING_ID = "22222222-2222-4222-8222-222222222222";
export const GRANT_ID = "33333333-3333-4333-8333-333333333333";
export const NONCE = "44444444-4444-4444-8444-444444444444";
export const PROPOSAL_ID = "55555555-5555-4555-8555-555555555555";
export const IDEMPOTENCY_ID = "66666666-6666-4666-8666-666666666666";
export const PAPER_ID = "77777777-7777-4777-8777-777777777777";

export const DISCLOSURE = Object.freeze({
  schema: "hepta.paper_raid.agent_capability_disclosure.v1",
  assurance: "self_declared_unverified",
  capabilities: Object.freeze([
    "artifact_analysis",
    "evidence_search",
    "section_drafting",
  ]),
  resource_classes: Object.freeze(["artifact_io", "cpu", "sandbox"]),
  max_parallel_tasks: 2,
});

export async function fixture(t, name = "fixture") {
  const directory = await mkdtemp(join(tmpdir(), `paper-raid-bridge-v2-${name}-`));
  t?.after(() => rm(directory, { recursive: true, force: true }));
  const identityPath = join(directory, "agent.identity.json");
  const statePath = join(directory, "agent.state.json");
  await generateIdentity(`agent.${name}`, identityPath);
  const identity = await loadIdentity(identityPath);
  const config = Object.freeze({
    schema: "hepta.paper_raid.agent_bridge.config.v2",
    bff_url: "http://127.0.0.1:8088",
    identity_file: identityPath,
    state_file: statePath,
    capability_disclosure: DISCLOSURE,
    paper_ids: Object.freeze([PAPER_ID]),
    poll_interval_ms: 500,
    request_timeout_ms: 2_000,
  });
  const binding = Object.freeze({
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    agent_public_key: identity.agent_public_key,
    agent_public_key_hash: identity.agent_public_key_hash,
    capability_disclosure: DISCLOSURE,
    capability_disclosure_hash: agentCapabilityDisclosureHash(DISCLOSURE),
    status: "active",
    version: 1,
    created_at: "2026-08-10T00:00:00Z",
    updated_at: "2026-08-10T00:00:00Z",
  });
  return { directory, identityPath, statePath, identity, config, binding };
}

export function jsonResponse(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

export function headersObject(headers) {
  return Object.fromEntries(new Headers(headers).entries());
}
