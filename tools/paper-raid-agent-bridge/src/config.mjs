import { dirname, resolve } from "node:path";
import { readSafeJson } from "./files.mjs";
import {
  AGENT_CAPABILITIES,
  AGENT_CAPABILITY_DISCLOSURE_SCHEMA,
  AGENT_RESOURCE_CLASSES,
  assertCanonicalUuid,
  validateCapabilityDisclosure,
} from "./canonical.mjs";

export const CONFIG_SCHEMA = "hepta.paper_raid.agent_bridge.config.v2";
export const AUTHOR_EXECUTOR_SCHEMA =
  "hepta.paper_raid.agent_bridge.author_executor.v1";
const LEGACY_CONFIG_SCHEMA = "hepta.paper_raid.agent_bridge.config.v1";

const EXPECTED_KEYS = new Set([
  "schema",
  "bff_url",
  "identity_file",
  "state_file",
  "author_executor",
  "capabilities",
  "resource_classes",
  "max_parallel_tasks",
  "paper_ids",
  "poll_interval_ms",
  "request_timeout_ms",
]);

const AUTHOR_EXECUTOR_KEYS = new Set([
  "schema",
  "executable",
  "timeout_ms",
]);

const SECRET_CONFIG_KEYS = new Set([
  "authorization",
  "api_key",
  "bearer",
  "bearer_token",
  "cookie",
  "credential",
  "credentials",
  "csrf",
  "csrf_token",
  "login_key",
  "login_key_file",
  "pair_code",
  "pairing_code",
  "password",
  "private_key",
  "private_key_file",
  "secret",
  "secret_key",
  "client_secret",
  "session_cookie",
  "token",
  "access_token",
  "refresh_token",
]);

function normalizedKey(value) {
  return value.replaceAll("-", "_").toLowerCase();
}

function rejectSecretKeys(value, path = "config") {
  if (!value || typeof value !== "object") return;
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectSecretKeys(item, `${path}[${index}]`));
    return;
  }
  for (const [key, child] of Object.entries(value)) {
    if (SECRET_CONFIG_KEYS.has(normalizedKey(key))) {
      throw new Error(`secret field ${path}.${key} is forbidden in Bridge config v2`);
    }
    rejectSecretKeys(child, `${path}.${key}`);
  }
}

function rejectUnknownKeys(value) {
  for (const key of Object.keys(value)) {
    if (!EXPECTED_KEYS.has(key)) {
      throw new Error(`unsupported config field ${key}`);
    }
  }
}

function validatedOrigin(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("bff_url must be an absolute HTTP(S) origin");
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    (url.pathname !== "/" && url.pathname !== "") ||
    url.search ||
    url.hash
  ) {
    throw new Error("bff_url must contain only an HTTP(S) origin");
  }
  if (
    url.protocol === "http:" &&
    !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)
  ) {
    throw new Error("plain HTTP is allowed only for a loopback BFF");
  }
  return url.origin;
}

export async function loadConfig(path) {
  const document = await readSafeJson(path, {
    privateFile: false,
    ownerOnly: false,
    maxBytes: 64 * 1024,
  });
  if (!document || Array.isArray(document) || typeof document !== "object") {
    throw new Error("Agent Bridge configuration must be a JSON object");
  }
  if (document.schema === LEGACY_CONFIG_SCHEMA) {
    throw new Error(
      "Agent Bridge config v1 is retired; migrate to config v2 and remove login_key_file",
    );
  }
  rejectSecretKeys(document);
  rejectUnknownKeys(document);
  if (document.schema !== CONFIG_SCHEMA) {
    throw new Error("unsupported Agent Bridge configuration schema; use config v2");
  }

  const base = dirname(resolve(path));
  const localPath = (value, field) => {
    if (typeof value !== "string" || value.length === 0 || value.includes("\0")) {
      throw new Error(`${field} must be a local file path`);
    }
    return resolve(base, value);
  };
  let authorExecutor = null;
  if (document.author_executor !== undefined && document.author_executor !== null) {
    const executor = document.author_executor;
    if (!executor || Array.isArray(executor) || typeof executor !== "object" ||
        Object.keys(executor).length !== AUTHOR_EXECUTOR_KEYS.size ||
        Object.keys(executor).some(key => !AUTHOR_EXECUTOR_KEYS.has(key)) ||
        executor.schema !== AUTHOR_EXECUTOR_SCHEMA ||
        !Number.isSafeInteger(executor.timeout_ms) ||
        executor.timeout_ms < 1_000 || executor.timeout_ms > 3_600_000) {
      throw new Error("author_executor must be one exact bounded v1 executor");
    }
    authorExecutor = Object.freeze({
      schema: AUTHOR_EXECUTOR_SCHEMA,
      executable: localPath(executor.executable, "author_executor.executable"),
      timeout_ms: executor.timeout_ms,
    });
  }
  if (
    !Array.isArray(document.paper_ids) ||
    document.paper_ids.length > 64
  ) {
    throw new Error("paper_ids must be an array with 0 to 64 entries");
  }
  const paperIds = document.paper_ids.map((value, index) =>
    assertCanonicalUuid(value, `paper_ids[${index}]`),
  );
  if (new Set(paperIds).size !== paperIds.length) {
    throw new Error("paper_ids must be unique");
  }
  if (
    !Number.isSafeInteger(document.poll_interval_ms) ||
    document.poll_interval_ms < 500 ||
    document.poll_interval_ms > 60_000
  ) {
    throw new Error("poll_interval_ms must be between 500 and 60000");
  }
  if (
    !Number.isSafeInteger(document.request_timeout_ms) ||
    document.request_timeout_ms < 500 ||
    document.request_timeout_ms > 30_000
  ) {
    throw new Error("request_timeout_ms must be between 500 and 30000");
  }
  if (!Array.isArray(document.capabilities)) {
    throw new Error("capabilities must be an array");
  }
  if (!Array.isArray(document.resource_classes)) {
    throw new Error("resource_classes must be an array");
  }
  const capabilityDisclosure = Object.freeze({
    schema: AGENT_CAPABILITY_DISCLOSURE_SCHEMA,
    assurance: "self_declared_unverified",
    capabilities: Object.freeze([...document.capabilities]),
    resource_classes: Object.freeze([...document.resource_classes]),
    max_parallel_tasks: document.max_parallel_tasks,
  });
  validateCapabilityDisclosure(capabilityDisclosure);
  // These redundant allowlist checks make the config boundary self-documenting
  // and stop a future contract expansion from silently changing Bridge v2.
  if (capabilityDisclosure.capabilities.some(value => !AGENT_CAPABILITIES.includes(value))) {
    throw new Error("capabilities contains an unsupported Bridge v2 value");
  }
  if (
    capabilityDisclosure.resource_classes.some(
      value => !AGENT_RESOURCE_CLASSES.includes(value),
    )
  ) {
    throw new Error("resource_classes contains an unsupported Bridge v2 value");
  }
  return Object.freeze({
    schema: document.schema,
    config_path: resolve(path),
    bff_url: validatedOrigin(document.bff_url),
    identity_file: localPath(document.identity_file, "identity_file"),
    state_file: localPath(document.state_file, "state_file"),
    author_executor: authorExecutor,
    capability_disclosure: capabilityDisclosure,
    paper_ids: Object.freeze([...paperIds]),
    poll_interval_ms: document.poll_interval_ms,
    request_timeout_ms: document.request_timeout_ms,
  });
}
