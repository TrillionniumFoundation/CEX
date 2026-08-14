import { resolve } from "node:path";
import { loadConfig } from "./config.mjs";
import { loadIdentity } from "./identity.mjs";
import { readSafeJson, writePrivateJsonReplacing } from "./files.mjs";
import {
  bridgeHealth,
  executeAndSubmitReviewTask,
  getInbox,
  recoverPendingReviewReceipt,
  submitAgentProposal,
} from "./operations.mjs";
import {
  actionableDeliveryCandidates,
  deliveryCandidateKey,
  proposalInput,
} from "./work.mjs";
import { actionableReviewTasks, reviewTaskKey } from "./review.mjs";

export const SERVICE_CONFIG_SCHEMA =
  "hepta.paper_raid.agent_bridge.service_config.v1";
export const SERVICE_STATUS_SCHEMA =
  "hepta.paper_raid.agent_bridge.service_status.v1";

const SERVICE_KEYS = new Set(["schema", "mode"]);
const RUNTIME_CYCLES = new Map();

export function serviceConfigPath(root) {
  return resolve(root, "data", "service.json");
}

export function serviceStatusPath(root) {
  return resolve(root, "data", "service-status.json");
}

export function installedBridgeConfigPath(root) {
  return resolve(root, "data", "bridge.config.json");
}

export async function loadServiceConfig(root) {
  const value = await readSafeJson(serviceConfigPath(root), {
    privateFile: true,
    maxBytes: 4096,
  });
  if (
    !value ||
    Array.isArray(value) ||
    typeof value !== "object" ||
    value.schema !== SERVICE_CONFIG_SCHEMA ||
    !["confirm", "auto"].includes(value.mode) ||
    Object.keys(value).length !== SERVICE_KEYS.size ||
    Object.keys(value).some(key => !SERVICE_KEYS.has(key))
  ) {
    throw new Error("installed service configuration is invalid");
  }
  return Object.freeze({ schema: value.schema, mode: value.mode });
}

function actionableItems(inbox, acknowledgedDeliveries, acknowledgedReviews) {
  const deliveries = actionableDeliveryCandidates(inbox, acknowledgedDeliveries)
    .filter(value => !acknowledgedDeliveries.has(deliveryCandidateKey(value)));
  const reviews = actionableReviewTasks(inbox, acknowledgedReviews);
  return [
    ...deliveries.map(value => Object.freeze({ kind: "delivery", value })),
    ...reviews.map(value => Object.freeze({ kind: "review", value })),
  ];
}

async function submitOne(config, identity, item) {
  if (item.kind === "review") {
    return executeAndSubmitReviewTask(config, identity, item.value);
  }
  return submitAgentProposal(config, identity, proposalInput(item.value));
}

function itemKey(item) {
  return item.kind === "review"
    ? reviewTaskKey(item.value)
    : deliveryCandidateKey(item.value);
}

function safeStatus({ mode, state, candidateCount = 0, action = "none" }) {
  if (!["confirm", "auto"].includes(mode)) throw new Error("service mode is invalid");
  if (![
    "ready",
    "awaiting_confirmation",
    "awaiting_pairing",
    "multiple_items",
    "submitted",
    "recovered",
    "degraded",
  ].includes(state)) {
    throw new Error("service state is invalid");
  }
  if (!Number.isSafeInteger(candidateCount) || candidateCount < 0 || candidateCount > 256) {
    throw new Error("service candidate count is invalid");
  }
  if (!["none", "delivery", "review", "recovery"].includes(action)) {
    throw new Error("service action is invalid");
  }
  return Object.freeze({
    schema: SERVICE_STATUS_SCHEMA,
    mode,
    state,
    candidate_count: candidateCount,
    action,
  });
}

async function saveStatus(root, status) {
  await writePrivateJsonReplacing(serviceStatusPath(root), status);
  return status;
}

async function daemonCycleUnlocked(
  root,
  { acknowledgedDeliveries, acknowledgedReviews },
) {
  const service = await loadServiceConfig(root);
  const config = await loadConfig(installedBridgeConfigPath(root));
  const identity = await loadIdentity(config.identity_file);
  let recovered;
  try {
    recovered = await recoverPendingReviewReceipt(config, identity);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return saveStatus(root, safeStatus({ mode: service.mode, state: "awaiting_pairing" }));
    }
    throw error;
  }
  if (recovered !== null) {
    return saveStatus(
      root,
      safeStatus({ mode: service.mode, state: "recovered", action: "recovery" }),
    );
  }
  await bridgeHealth(config, identity, { status: "healthy" });
  const inbox = await getInbox(config, identity);
  const items = actionableItems(inbox, acknowledgedDeliveries, acknowledgedReviews);
  if (service.mode === "confirm") {
    return saveStatus(
      root,
      safeStatus({
        mode: service.mode,
        state: items.length === 0 ? "ready" : "awaiting_confirmation",
        candidateCount: items.length,
      }),
    );
  }
  if (items.length !== 1) {
    return saveStatus(
      root,
      safeStatus({
        mode: service.mode,
        state: items.length === 0 ? "ready" : "multiple_items",
        candidateCount: items.length,
      }),
    );
  }
  const item = items[0];
  await submitOne(config, identity, item);
  const key = itemKey(item);
  if (item.kind === "review") acknowledgedReviews.add(key);
  else acknowledgedDeliveries.add(key);
  return saveStatus(
    root,
    safeStatus({
      mode: service.mode,
      state: "submitted",
      candidateCount: 1,
      action: item.kind,
    }),
  );
}

function runtimeCycle(root) {
  const key = resolve(root);
  let runtime = RUNTIME_CYCLES.get(key);
  if (!runtime) {
    runtime = {
      tail: Promise.resolve(),
      acknowledgedDeliveries: new Set(),
      acknowledgedReviews: new Set(),
    };
    RUNTIME_CYCLES.set(key, runtime);
  }
  return runtime;
}

export function daemonCycle(root, options = {}) {
  const runtime = runtimeCycle(root);
  const acknowledgedDeliveries = options.acknowledgedDeliveries
    ?? runtime.acknowledgedDeliveries;
  const acknowledgedReviews = options.acknowledgedReviews
    ?? runtime.acknowledgedReviews;
  if (!(acknowledgedDeliveries instanceof Set) || !(acknowledgedReviews instanceof Set)) {
    return Promise.reject(new Error("daemon acknowledgement state must use Sets"));
  }
  const cycle = runtime.tail.then(() => daemonCycleUnlocked(root, {
    acknowledgedDeliveries,
    acknowledgedReviews,
  }));
  runtime.tail = cycle.catch(() => {});
  return cycle;
}

export async function runDaemon(root, { signal = undefined } = {}) {
  const acknowledgedDeliveries = new Set();
  const acknowledgedReviews = new Set();
  process.stdout.write("paper-raid-agent-bridge service started\n");
  for (;;) {
    if (signal?.aborted) return;
    let delay = 5_000;
    try {
      const status = await daemonCycle(root, {
        acknowledgedDeliveries,
        acknowledgedReviews,
      });
      const config = await loadConfig(installedBridgeConfigPath(root));
      delay = config.poll_interval_ms;
      process.stdout.write(`service status=${status.state} mode=${status.mode}\n`);
    } catch {
      const service = await loadServiceConfig(root).catch(() => ({ mode: "confirm" }));
      await saveStatus(root, safeStatus({ mode: service.mode, state: "degraded" })).catch(
        () => {},
      );
      process.stderr.write("paper-raid-agent-bridge service degraded; run diagnose\n");
    }
    await new Promise(resolveTimer => {
      const timer = setTimeout(resolveTimer, delay);
      signal?.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          resolveTimer();
        },
        { once: true },
      );
    });
  }
}
