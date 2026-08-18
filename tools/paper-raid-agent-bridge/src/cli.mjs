#!/usr/bin/env node
import { createInterface } from "node:readline";
import { resolve } from "node:path";
import { readSafeJson } from "./files.mjs";
import { generateIdentity, importIdentity, loadIdentity } from "./identity.mjs";
import { loadConfig } from "./config.mjs";
import { inboxFingerprint } from "./inbox.mjs";
import {
  actionableDeliveryCandidates,
  authorWorkStartKey,
  authorWorkStarts,
  deliveryCandidateKey,
  deliveryCandidates,
  formatAuthorWorkStart,
  formatDeliveryCandidate,
  workResult,
} from "./work.mjs";
import {
  actionableReviewTasks,
  formatReviewTask,
  reviewTaskKey,
} from "./review.mjs";
import {
  bridgeHealth,
  executePracticeAuto,
  getBinding,
  getInbox,
  getPractice,
  pairAgent,
  prepareChallengeMaterialsForStart,
  executeAndSubmitAuthorDelivery,
  executeAndSubmitAuthorWorkStart,
  executeAndSubmitReviewTask,
  recoverPendingReviewReceipt,
  signResearchSessionAction,
} from "./operations.mjs";
import * as lifecycleApi from "./lifecycle.mjs";

const HELP = `Paper Raid Agent Bridge v2

Usage:
  paper-raid-agent-bridge install --package FILE --manifest FILE --signature FILE \\
    --trusted-key FILE --fingerprint sha256:HEX --bff-url ORIGIN --agent-id ID \\
    --author-executor FILE --capabilities LIST --resource-classes LIST \\
    [--mode confirm|auto] [--acknowledge-auto]
  paper-raid-agent-bridge pair [--root TEST_ROOT]
  paper-raid-agent-bridge confirm [--root TEST_ROOT]
  paper-raid-agent-bridge mode --mode confirm|auto [--acknowledge-auto] [--root TEST_ROOT]
  paper-raid-agent-bridge diagnose [--json] [--root TEST_ROOT]
  paper-raid-agent-bridge update --package FILE --manifest FILE --signature FILE \\
    --trusted-key FILE --fingerprint sha256:HEX --author-executor FILE \\
    [--root TEST_ROOT]
  paper-raid-agent-bridge rollback --signature FILE --trusted-key FILE \\
    --fingerprint sha256:HEX [--root TEST_ROOT]
  paper-raid-agent-bridge uninstall [--purge-data] [--root TEST_ROOT]
  paper-raid-agent-bridge identity-generate --agent-id ID --out FILE
  paper-raid-agent-bridge identity-import --agent-id ID --from FILE --out FILE
  paper-raid-agent-bridge describe --config FILE
  paper-raid-agent-bridge pair --config FILE
  paper-raid-agent-bridge binding --config FILE
  paper-raid-agent-bridge health --config FILE [--status healthy|degraded|offline]
  paper-raid-agent-bridge inbox --config FILE [--watch]
  paper-raid-agent-bridge practice --config FILE
  paper-raid-agent-bridge practice-auto --config FILE
  paper-raid-agent-bridge prepare-materials --config FILE --work-item UUID
  paper-raid-agent-bridge work --config FILE [--watch] [--auto]
  paper-raid-agent-bridge sign-action --config FILE --input FILE

Install, update, and rollback require a detached raw Ed25519 signature plus an
operator-pinned trusted public key fingerprint. --root and --systemctl are an
isolated test harness and never contact the host user manager.

The pair command reads its one-time code only from a silent TTY prompt or stdin.
It never accepts pairing codes through argv, environment, or config. The
background service defaults to Confirm; Auto requires an explicit acknowledgement.
`;

function parseArguments(argv) {
  const [command, ...rest] = argv;
  const flags = new Map();
  for (let index = 0; index < rest.length; index += 1) {
    const name = rest[index];
    if (!name.startsWith("--")) throw new Error(`unexpected argument ${name}`);
    if (
      [
        "--watch",
        "--auto",
        "--help",
        "--json",
        "--purge-data",
        "--acknowledge-auto",
      ].includes(name)
    ) {
      if (flags.has(name)) throw new Error(`duplicate flag ${name}`);
      flags.set(name, true);
      continue;
    }
    const value = rest[++index];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${name} requires a value`);
    }
    if (flags.has(name)) throw new Error(`duplicate flag ${name}`);
    flags.set(name, value);
  }
  return { command, flags };
}

function required(flags, name) {
  const value = flags.get(name);
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function allowed(flags, names) {
  for (const name of flags.keys()) {
    if (!names.includes(name)) throw new Error(`unsupported flag ${name}`);
  }
}

function output(value) {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

function safeError(error) {
  const message = error instanceof Error ? error.message : "Agent Bridge failed";
  return message
    .replace(
      /(login[_-]?key|pair(?:ing)?[_-]?code|token|cookie|csrf|authorization|private[_-]?key)\s*[:=]\s*\S+/gi,
      "$1=[REDACTED]",
    )
    .slice(0, 512);
}

async function configured(flags) {
  const config = await loadConfig(required(flags, "--config"));
  const identity = await loadIdentity(config.identity_file);
  return { config, identity };
}

async function lifecycleModule() {
  return lifecycleApi;
}

function optionalString(flags, name) {
  const value = flags.get(name);
  if (value === undefined) return undefined;
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${name} must be a non-empty value`);
  }
  return value;
}

function lifecycleCommon(flags) {
  const root = optionalString(flags, "--root");
  const systemctlPath = optionalString(flags, "--systemctl");
  if ((root === undefined) !== (systemctlPath === undefined)) {
    throw new Error("--root and --systemctl are a test-harness pair and must appear together");
  }
  return { root, systemctlPath };
}

function readPipedLine(input) {
  return new Promise((resolveLine, rejectLine) => {
    const reader = createInterface({ input, crlfDelay: Infinity, terminal: false });
    let settled = false;
    reader.once("line", line => {
      settled = true;
      reader.close();
      resolveLine(line);
    });
    reader.once("close", () => {
      if (!settled) rejectLine(new Error("pairing code input ended before one line"));
    });
    reader.once("error", () => rejectLine(new Error("pairing code input failed")));
  });
}

function readSilentTty(input, errorOutput) {
  return new Promise((resolveCode, rejectCode) => {
    let code = "";
    const previousRaw = input.isRaw;
    const finish = (error, value) => {
      input.off("data", onData);
      try {
        input.setRawMode(previousRaw ?? false);
      } catch {
        // The input may have closed while restoring terminal state.
      }
      input.pause();
      errorOutput.write("\n");
      if (error) rejectCode(error);
      else resolveCode(value);
    };
    const onData = chunk => {
      for (const byte of Buffer.from(chunk)) {
        if (byte === 3) {
          finish(new Error("pairing code input cancelled"));
          return;
        }
        if (byte === 13 || byte === 10) {
          finish(null, code);
          return;
        }
        if (byte === 127 || byte === 8) {
          code = code.slice(0, -1);
          continue;
        }
        if (byte < 0x21 || byte > 0x7e || code.length >= 2048) {
          finish(new Error("pairing code input is invalid"));
          return;
        }
        code += String.fromCharCode(byte);
      }
    };
    errorOutput.write("Pairing code (input hidden): ");
    input.setRawMode(true);
    input.resume();
    input.on("data", onData);
  });
}

export async function readPairingCodeFromInput(
  input = process.stdin,
  errorOutput = process.stderr,
) {
  if (input.isTTY && typeof input.setRawMode === "function") {
    return readSilentTty(input, errorOutput);
  }
  return readPipedLine(input);
}

export async function chooseDeliveryCandidate(
  candidates,
  input = process.stdin,
  errorOutput = process.stderr,
) {
  if (!Array.isArray(candidates) || candidates.length === 0) return null;
  for (const [index, candidate] of candidates.entries()) {
    errorOutput.write(`${formatDeliveryCandidate(candidate, index)}\n`);
  }
  const reader = createInterface({
    input,
    output: errorOutput,
    crlfDelay: Infinity,
    terminal: Boolean(input.isTTY),
  });
  const answer = await new Promise((resolveAnswer, rejectAnswer) => {
    reader.question(
      "Select one delivery to sign and submit, or 0 to cancel: ",
      resolveAnswer,
    );
    reader.once("error", rejectAnswer);
  }).finally(() => reader.close());
  const selected = Number(String(answer).trim());
  if (!Number.isSafeInteger(selected) || selected < 0 || selected > candidates.length) {
    throw new Error("delivery selection is outside the displayed range");
  }
  return selected === 0 ? null : candidates[selected - 1];
}

export async function chooseWorkCandidate(
  candidates,
  input = process.stdin,
  errorOutput = process.stderr,
) {
  if (!Array.isArray(candidates) || candidates.length === 0) return null;
  for (const [index, candidate] of candidates.entries()) {
    const line = candidate.work_kind === "review"
      ? formatReviewTask(candidate.value, index)
      : candidate.work_kind === "author_start"
        ? formatAuthorWorkStart(candidate.value, index)
        : formatDeliveryCandidate(candidate.value, index);
    errorOutput.write(`${line}\n`);
  }
  const reader = createInterface({
    input,
    output: errorOutput,
    crlfDelay: Infinity,
    terminal: Boolean(input.isTTY),
  });
  const answer = await new Promise((resolveAnswer, rejectAnswer) => {
    reader.question(
      "Select one local task to confirm and submit, or 0 to cancel: ",
      resolveAnswer,
    );
    reader.once("error", rejectAnswer);
  }).finally(() => reader.close());
  const selected = Number(String(answer).trim());
  if (!Number.isSafeInteger(selected) || selected < 0 || selected > candidates.length) {
    throw new Error("work selection is outside the displayed range");
  }
  return selected === 0 ? null : candidates[selected - 1];
}

function combinedWorkItems(
  inbox,
  acknowledgedDeliveries,
  acknowledgedReviews,
  acknowledgedAuthorStarts,
) {
  const starts = authorWorkStarts(inbox, acknowledgedAuthorStarts ?? new Set());
  const deliveries = acknowledgedDeliveries === undefined
    ? deliveryCandidates(inbox)
    : actionableDeliveryCandidates(inbox, acknowledgedDeliveries);
  const reviews = acknowledgedReviews === undefined
    ? actionableReviewTasks(inbox)
    : actionableReviewTasks(inbox, acknowledgedReviews);
  return [
    ...starts.map(value => Object.freeze({ work_kind: "author_start", value })),
    ...deliveries.map(value => Object.freeze({ work_kind: "delivery", value })),
    ...reviews.map(value => Object.freeze({ work_kind: "review", value })),
  ];
}

async function submitWorkItem(config, identity, candidate) {
  if (candidate.work_kind === "review") {
    return executeAndSubmitReviewTask(config, identity, candidate.value);
  }
  if (candidate.work_kind === "author_start") {
    return executeAndSubmitAuthorWorkStart(config, identity, candidate.value);
  }
  return executeAndSubmitAuthorDelivery(
    config,
    identity,
    candidate.value,
  );
}

async function workOnce(config, identity, flags) {
  const recovered = await recoverPendingReviewReceipt(config, identity);
  if (recovered !== null) {
    output(workResult("recovered", {
      recoveredTaskId: recovered.task_id,
      result: recovered,
    }));
    return null;
  }
  const inbox = await getInbox(config, identity);
  const candidates = combinedWorkItems(inbox);
  if (candidates.length === 0) {
    output(workResult("idle"));
    return inbox;
  }
  let candidate;
  if (flags.has("--auto")) {
    if (candidates.length !== 1) {
      output(workResult("awaiting_local_selection", {
        candidateCount: candidates.length,
      }));
      return inbox;
    }
    candidate = candidates[0];
  } else {
    candidate = await chooseWorkCandidate(candidates);
    if (!candidate) {
      output(workResult("cancelled_locally", {
        candidateCount: candidates.length,
      }));
      return inbox;
    }
  }
  const result = await submitWorkItem(config, identity, candidate);
  output(workResult("submitted", {
    candidateCount: candidates.length,
    candidate: candidate.value,
    result,
  }));
  return inbox;
}

export async function main(argv) {
  const { command, flags } = parseArguments(argv);
  if (!command || command === "help" || flags.has("--help")) {
    process.stdout.write(HELP);
    return;
  }
  if (command === "install") {
    allowed(flags, [
      "--package",
      "--manifest",
      "--signature",
      "--trusted-key",
      "--fingerprint",
      "--bff-url",
      "--agent-id",
      "--author-executor",
      "--capabilities",
      "--resource-classes",
      "--mode",
      "--acknowledge-auto",
      "--root",
      "--systemctl",
    ]);
    const lifecycle = await lifecycleModule();
    output(await lifecycle.installProduct({
      ...lifecycleCommon(flags),
      packagePath: required(flags, "--package"),
      manifestPath: required(flags, "--manifest"),
      signaturePath: required(flags, "--signature"),
      trustedKeyPath: required(flags, "--trusted-key"),
      fingerprint: required(flags, "--fingerprint"),
      bffUrl: required(flags, "--bff-url"),
      agentId: required(flags, "--agent-id"),
      authorExecutor: required(flags, "--author-executor"),
      capabilities: required(flags, "--capabilities"),
      resourceClasses: required(flags, "--resource-classes"),
      mode: optionalString(flags, "--mode") ?? "confirm",
      autoAcknowledged: flags.has("--acknowledge-auto"),
    }));
    return;
  }
  if (command === "update") {
    allowed(flags, [
      "--package",
      "--manifest",
      "--signature",
      "--trusted-key",
      "--fingerprint",
      "--author-executor",
      "--root",
      "--systemctl",
    ]);
    const lifecycle = await lifecycleModule();
    output(await lifecycle.updateProduct({
      ...lifecycleCommon(flags),
      packagePath: required(flags, "--package"),
      manifestPath: required(flags, "--manifest"),
      signaturePath: required(flags, "--signature"),
      trustedKeyPath: required(flags, "--trusted-key"),
      fingerprint: required(flags, "--fingerprint"),
      authorExecutor: required(flags, "--author-executor"),
    }));
    return;
  }
  if (command === "rollback") {
    allowed(flags, [
      "--signature",
      "--trusted-key",
      "--fingerprint",
      "--root",
      "--systemctl",
    ]);
    const lifecycle = await lifecycleModule();
    output(await lifecycle.rollbackProduct({
      ...lifecycleCommon(flags),
      signaturePath: required(flags, "--signature"),
      trustedKeyPath: required(flags, "--trusted-key"),
      fingerprint: required(flags, "--fingerprint"),
    }));
    return;
  }
  if (command === "uninstall") {
    allowed(flags, ["--purge-data", "--root", "--systemctl"]);
    const lifecycle = await lifecycleModule();
    output(await lifecycle.uninstallProduct({
      ...lifecycleCommon(flags),
      purgeData: flags.has("--purge-data"),
    }));
    return;
  }
  if (command === "diagnose") {
    allowed(flags, ["--json", "--root", "--systemctl"]);
    const lifecycle = await lifecycleModule();
    const result = await lifecycle.diagnoseProduct(lifecycleCommon(flags));
    if (flags.has("--json")) output(result);
    else process.stdout.write(lifecycle.formatDiagnosis(result));
    return;
  }
  if (command === "mode") {
    allowed(flags, ["--mode", "--acknowledge-auto", "--root", "--systemctl"]);
    const lifecycle = await lifecycleModule();
    output(await lifecycle.setServiceMode({
      ...lifecycleCommon(flags),
      mode: required(flags, "--mode"),
      autoAcknowledged: flags.has("--acknowledge-auto"),
    }));
    return;
  }
  if (command === "service") {
    allowed(flags, ["--root"]);
    const lifecycle = await lifecycleModule();
    await lifecycle.runInstalledService({ root: optionalString(flags, "--root") });
    return;
  }
  if (command === "identity-generate") {
    allowed(flags, ["--agent-id", "--out"]);
    output(
      await generateIdentity(
        required(flags, "--agent-id"),
        resolve(required(flags, "--out")),
      ),
    );
    return;
  }
  if (command === "identity-import") {
    allowed(flags, ["--agent-id", "--from", "--out"]);
    output(
      await importIdentity(
        required(flags, "--agent-id"),
        resolve(required(flags, "--from")),
        resolve(required(flags, "--out")),
      ),
    );
    return;
  }
  if (command === "describe") {
    allowed(flags, ["--config"]);
    const { identity } = await configured(flags);
    output(identity);
    return;
  }
  if (command === "pair") {
    allowed(flags, ["--config", "--root"]);
    if (flags.has("--config") && flags.has("--root")) {
      throw new Error("pair accepts either --config or --root, not both");
    }
    let config;
    let identity;
    if (flags.has("--config")) {
      ({ config, identity } = await configured(flags));
    } else {
      const lifecycle = await lifecycleModule();
      config = await loadConfig(
        await lifecycle.installedConfigPath(optionalString(flags, "--root")),
      );
      identity = await loadIdentity(config.identity_file);
    }
    output(
      await pairAgent(config, identity, {
        readPairingCode: () => readPairingCodeFromInput(),
      }),
    );
    return;
  }
  if (command === "confirm") {
    allowed(flags, ["--root"]);
    const lifecycle = await lifecycleModule();
    const config = await loadConfig(
      await lifecycle.installedConfirmConfigPath(optionalString(flags, "--root")),
    );
    const identity = await loadIdentity(config.identity_file);
    await workOnce(config, identity, new Map());
    return;
  }
  if (command === "binding") {
    allowed(flags, ["--config"]);
    const { config, identity } = await configured(flags);
    output(await getBinding(config, identity));
    return;
  }
  if (command === "health") {
    allowed(flags, ["--config", "--status"]);
    const { config, identity } = await configured(flags);
    output(
      await bridgeHealth(config, identity, {
        status: flags.get("--status") ?? "healthy",
      }),
    );
    return;
  }
  if (command === "inbox") {
    allowed(flags, ["--config", "--watch"]);
    const { config, identity } = await configured(flags);
    if (!flags.has("--watch")) {
      output(await getInbox(config, identity));
      return;
    }
    let previous = null;
    for (;;) {
      const inbox = await getInbox(config, identity);
      const fingerprint = inboxFingerprint(inbox);
      if (fingerprint !== previous) {
        process.stdout.write(`${JSON.stringify(inbox)}\n`);
        previous = fingerprint;
      }
      await new Promise(resolveTimer =>
        setTimeout(resolveTimer, config.poll_interval_ms),
      );
    }
  }
  if (command === "practice") {
    allowed(flags, ["--config"]);
    const { config, identity } = await configured(flags);
    output(await getPractice(config, identity));
    return;
  }
  if (command === "practice-auto") {
    allowed(flags, ["--config"]);
    const { config, identity } = await configured(flags);
    output(await executePracticeAuto(config, identity));
    return;
  }
  if (command === "prepare-materials") {
    allowed(flags, ["--config", "--work-item"]);
    const { config, identity } = await configured(flags);
    const workItemId = required(flags, "--work-item");
    const inbox = await getInbox(config, identity);
    const starts = authorWorkStarts(inbox).filter(start =>
      start.work_item_id === workItemId
    );
    if (starts.length !== 1) {
      throw new Error("prepare-materials requires one exact Author work start");
    }
    output(await prepareChallengeMaterialsForStart(
      config,
      identity,
      starts[0],
    ));
    return;
  }
  if (command === "work") {
    allowed(flags, ["--config", "--watch", "--auto"]);
    const { config, identity } = await configured(flags);
    if (!flags.has("--watch")) {
      await workOnce(config, identity, flags);
      return;
    }
    const recovered = await recoverPendingReviewReceipt(config, identity);
    if (recovered !== null) {
      output(workResult("recovered", {
        recoveredTaskId: recovered.task_id,
        result: recovered,
      }));
    }
    let previous = null;
    const acknowledgedDeliveries = new Set();
    const acknowledgedReviews = new Set();
    const acknowledgedAuthorStarts = new Set();
    for (;;) {
      const inbox = await getInbox(config, identity);
      const fingerprint = inboxFingerprint(inbox);
      if (fingerprint !== previous) {
        const candidates = combinedWorkItems(
          inbox,
          acknowledgedDeliveries,
          acknowledgedReviews,
          acknowledgedAuthorStarts,
        );
        if (candidates.length === 0) {
          output(workResult("idle"));
        } else {
          let candidate;
          if (flags.has("--auto")) {
            candidate = candidates.length === 1 ? candidates[0] : null;
            if (!candidate) {
              output(workResult("awaiting_local_selection", {
                candidateCount: candidates.length,
              }));
            }
          } else {
            candidate = await chooseWorkCandidate(candidates);
            if (!candidate) {
              output(workResult("cancelled_locally", {
                candidateCount: candidates.length,
              }));
            }
          }
          if (candidate) {
            const candidateKey = candidate.work_kind === "review"
              ? reviewTaskKey(candidate.value)
              : candidate.work_kind === "author_start"
                ? authorWorkStartKey(candidate.value)
                : deliveryCandidateKey(candidate.value);
            const result = await submitWorkItem(config, identity, inbox, candidate);
            output(workResult("submitted", {
              candidateCount: candidates.length,
              candidate: candidate.value,
              result,
            }));
            if (candidate.work_kind === "review") {
              acknowledgedReviews.add(candidateKey);
            } else if (candidate.work_kind === "author_start") {
              acknowledgedAuthorStarts.add(candidateKey);
              acknowledgedDeliveries.add(deliveryCandidateKey(result.candidate));
            } else {
              acknowledgedDeliveries.add(candidateKey);
            }
          }
        }
        previous = fingerprint;
      }
      await new Promise(resolveTimer =>
        setTimeout(resolveTimer, config.poll_interval_ms),
      );
    }
  }
  if (command === "sign-action") {
    allowed(flags, ["--config", "--input"]);
    const { identity } = await configured(flags);
    const action = await readSafeJson(resolve(required(flags, "--input")), {
      privateFile: false,
      ownerOnly: false,
      maxBytes: 1024 * 1024,
    });
    output(signResearchSessionAction(identity, action));
    return;
  }
  throw new Error(`unsupported command ${command}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main(process.argv.slice(2)).catch(error => {
    process.stderr.write(`paper-raid-agent-bridge: ${safeError(error)}\n`);
    process.exitCode = 1;
  });
}
