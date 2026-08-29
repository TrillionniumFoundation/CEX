import {
  preparePrivateJsonDestination,
  readSafeJson,
  writePrivateJsonExclusive,
  writePrivateJsonReplacing,
} from "./files.mjs";
import {
  assertCanonicalUuid,
  assertDigest,
  assertLogicalId,
} from "./canonical.mjs";

export const STATE_SCHEMA = "hepta.paper_raid.agent_bridge.state.v2";
const STATE_KEYS = new Set([
  "schema",
  "binding_id",
  "player_id",
  "agent_id",
  "agent_key_id",
  "agent_public_key_hash",
  "capability_disclosure_hash",
  "bound_at_unix",
]);

function validatedState(value, identity, { allowRotatedKey = false } = {}) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    throw new Error("Agent Bridge state must be a JSON object");
  }
  if (value.schema === "hepta.paper_raid.agent_bridge.state.v1") {
    throw new Error("Agent Bridge state v1 is retired; remove it and pair with Bridge v2");
  }
  if (value.schema !== STATE_SCHEMA) {
    throw new Error("unsupported Agent Bridge state schema");
  }
  const keys = Object.keys(value);
  if (keys.length !== STATE_KEYS.size || keys.some(key => !STATE_KEYS.has(key))) {
    throw new Error("Agent Bridge state contains unsupported or missing fields");
  }
  assertCanonicalUuid(value.binding_id, "binding_id");
  assertCanonicalUuid(value.player_id, "player_id");
  assertLogicalId(value.agent_id, "agent_id");
  assertDigest(value.agent_key_id, "agent_key_id");
  assertDigest(value.agent_public_key_hash, "agent_public_key_hash");
  assertDigest(value.capability_disclosure_hash, "capability_disclosure_hash");
  if (
    value.agent_id !== identity.agent_id ||
    (!allowRotatedKey &&
      (value.agent_key_id !== identity.agent_key_id ||
        value.agent_public_key_hash !== identity.agent_public_key_hash))
  ) {
    throw new Error("Agent Bridge state belongs to a different Agent identity");
  }
  if (!Number.isSafeInteger(value.bound_at_unix) || value.bound_at_unix < 0) {
    throw new Error("Agent Bridge state bound_at_unix is invalid");
  }
  return Object.freeze({ ...value });
}

export async function loadBridgeState(path, identity, { optional = false } = {}) {
  let value;
  try {
    value = await readSafeJson(path, {
      privateFile: true,
      maxBytes: 64 * 1024,
    });
  } catch (error) {
    if (optional && error?.code === "ENOENT") return null;
    throw error;
  }
  return validatedState(value, identity);
}

export async function prepareBridgeStateForPairing(path, identity) {
  let existing = null;
  try {
    existing = validatedState(
      await readSafeJson(path, {
        privateFile: true,
        maxBytes: 64 * 1024,
      }),
      identity,
      { allowRotatedKey: true },
    );
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  await preparePrivateJsonDestination(path, { replace: existing !== null });
  return existing;
}

export async function saveBridgeState(
  path,
  identity,
  binding,
  nowUnix,
  { replace = false } = {},
) {
  const value = {
    schema: STATE_SCHEMA,
    binding_id: binding.binding_id,
    player_id: binding.player_id,
    agent_id: binding.agent_id,
    agent_key_id: binding.agent_key_id,
    agent_public_key_hash: binding.agent_public_key_hash,
    capability_disclosure_hash: binding.capability_disclosure_hash,
    bound_at_unix: nowUnix,
  };
  const state = validatedState(value, identity);
  if (replace) await writePrivateJsonReplacing(path, state);
  else await writePrivateJsonExclusive(path, state);
  return state;
}
