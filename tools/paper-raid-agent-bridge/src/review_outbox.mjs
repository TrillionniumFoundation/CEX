import {
  readSafeJson,
  removePrivateFile,
  writePrivateJsonExclusive,
} from "./files.mjs";
import { canonicalJsonBytes } from "./canonical.mjs";
import { validateReviewReceiptRequest } from "./review.mjs";

export const REVIEW_OUTBOX_SCHEMA =
  "hepta.paper_raid.agent_bridge.review_outbox.v1";

export function reviewOutboxPath(stateFile) {
  return `${stateFile}.review-outbox.json`;
}

function validateOutbox(value, state, identity) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("review outbox must be an object");
  }
  const expected = [
    "schema",
    "binding_id",
    "agent_id",
    "agent_key_id",
    "task_key",
    "request",
  ].sort();
  const keys = Object.keys(value).sort();
  if (
    keys.length !== expected.length ||
    keys.some((key, index) => key !== expected[index]) ||
    value.schema !== REVIEW_OUTBOX_SCHEMA ||
    value.binding_id !== state.binding_id ||
    value.agent_id !== identity.agent_id ||
    value.agent_key_id !== identity.agent_key_id ||
    value.task_key !== value.request?.receipt?.task_id
  ) {
    throw new Error("review outbox identity or authority tuple is invalid");
  }
  validateReviewReceiptRequest(state, identity, value.request);
  return Object.freeze({ ...value });
}

export async function loadReviewOutbox(config, state, identity, { optional = true } = {}) {
  let value;
  try {
    value = await readSafeJson(reviewOutboxPath(config.state_file), {
      privateFile: true,
      maxBytes: 1024 * 1024,
    });
  } catch (error) {
    if (optional && error?.code === "ENOENT") return null;
    throw error;
  }
  return validateOutbox(value, state, identity);
}

export async function saveReviewOutbox(config, state, identity, request) {
  validateReviewReceiptRequest(state, identity, request);
  const value = validateOutbox({
    schema: REVIEW_OUTBOX_SCHEMA,
    binding_id: state.binding_id,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    task_key: request.receipt.task_id,
    request,
  }, state, identity);
  const existing = await loadReviewOutbox(config, state, identity, { optional: true });
  if (existing !== null) {
    if (!canonicalJsonBytes(existing).equals(canonicalJsonBytes(value))) {
      throw new Error("a different review receipt is already pending in the outbox");
    }
    return existing;
  }
  try {
    await writePrivateJsonExclusive(reviewOutboxPath(config.state_file), value);
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
    const raced = await loadReviewOutbox(config, state, identity, { optional: false });
    if (!canonicalJsonBytes(raced).equals(canonicalJsonBytes(value))) {
      throw new Error("another process claimed the review outbox");
    }
    return raced;
  }
  return value;
}

export async function clearReviewOutbox(config) {
  return removePrivateFile(reviewOutboxPath(config.state_file), { optional: false });
}
