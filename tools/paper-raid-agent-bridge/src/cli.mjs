#!/usr/bin/env node
import { createInterface } from "node:readline";
import { resolve } from "node:path";
import { readSafeJson } from "./files.mjs";
import { generateIdentity, importIdentity, loadIdentity } from "./identity.mjs";
import { loadConfig } from "./config.mjs";
import { inboxFingerprint } from "./inbox.mjs";
import {
  bridgeHealth,
  getBinding,
  getInbox,
  pairAgent,
  signResearchSessionAction,
  submitAgentProposal,
} from "./operations.mjs";

const HELP = `Paper Raid Agent Bridge v2

Usage:
  paper-raid-agent-bridge identity-generate --agent-id ID --out FILE
  paper-raid-agent-bridge identity-import --agent-id ID --from FILE --out FILE
  paper-raid-agent-bridge describe --config FILE
  paper-raid-agent-bridge pair --config FILE
  paper-raid-agent-bridge binding --config FILE
  paper-raid-agent-bridge health --config FILE [--status healthy|degraded|offline]
  paper-raid-agent-bridge inbox --config FILE [--watch]
  paper-raid-agent-bridge submit-proposal --config FILE \\
    --paper-id UUID --work-item-id UUID --section-key KEY \\
    --parent-revision-id UUID --proposal-kind proposal|delivery \\
    --payload-hash sha256:HEX --artifact-manifest-id UUID \\
    --artifact-manifest-hash sha256:HEX
  paper-raid-agent-bridge sign-action --config FILE --input FILE

The pair command reads its one-time code only from a silent TTY prompt or
stdin. It never accepts pairing codes through argv, environment, or config.
`;

function parseArguments(argv) {
  const [command, ...rest] = argv;
  const flags = new Map();
  for (let index = 0; index < rest.length; index += 1) {
    const name = rest[index];
    if (!name.startsWith("--")) throw new Error(`unexpected argument ${name}`);
    if (["--watch", "--help"].includes(name)) {
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

export async function main(argv) {
  const { command, flags } = parseArguments(argv);
  if (!command || command === "help" || flags.has("--help")) {
    process.stdout.write(HELP);
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
    allowed(flags, ["--config"]);
    const { config, identity } = await configured(flags);
    output(
      await pairAgent(config, identity, {
        readPairingCode: () => readPairingCodeFromInput(),
      }),
    );
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
  if (command === "submit-proposal") {
    allowed(flags, [
      "--config",
      "--paper-id",
      "--work-item-id",
      "--section-key",
      "--parent-revision-id",
      "--proposal-kind",
      "--payload-hash",
      "--artifact-manifest-id",
      "--artifact-manifest-hash",
    ]);
    const { config, identity } = await configured(flags);
    output(
      await submitAgentProposal(config, identity, {
        paper_id: required(flags, "--paper-id"),
        work_item_id: required(flags, "--work-item-id"),
        section_key: required(flags, "--section-key"),
        parent_revision_id: required(flags, "--parent-revision-id"),
        proposal_kind: required(flags, "--proposal-kind"),
        payload_hash: required(flags, "--payload-hash"),
        artifact_manifest_id: required(flags, "--artifact-manifest-id"),
        artifact_manifest_hash: required(flags, "--artifact-manifest-hash"),
      }),
    );
    return;
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
