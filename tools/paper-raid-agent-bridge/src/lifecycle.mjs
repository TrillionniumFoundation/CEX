import {
  constants as fsConstants,
  chmod,
  link,
  lstat,
  mkdir,
  open,
  readdir,
  readlink,
  rename,
  rmdir,
  symlink,
  unlink,
} from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { homedir } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";
import { spawn } from "node:child_process";
import { loadConfig, CONFIG_SCHEMA } from "./config.mjs";
import { loadIdentity, generateIdentity } from "./identity.mjs";
import { loadBridgeState } from "./state.mjs";
import {
  readSafeJson,
  writePrivateJsonReplacing,
} from "./files.mjs";
import {
  SERVICE_CONFIG_SCHEMA,
  SERVICE_STATUS_SCHEMA,
  installedBridgeConfigPath,
  loadServiceConfig,
  runDaemon,
  serviceConfigPath,
  serviceStatusPath,
} from "./daemon.mjs";
import {
  extractVerifiedRelease,
  publishStagedRelease,
  readReleaseInputs,
  readTrustedPublicKey,
  releaseId,
  removeStagingRelease,
  verifyInstalledRelease,
} from "./release.mjs";

export const INSTALL_SCHEMA = "hepta.paper_raid.agent_bridge.installation.v1";
export const UNIT_NAME = "paper-raid-agent-bridge.service";
export const SYSTEMCTL_TIMEOUT_MS = 15_000;

const INSTALL_KEYS = new Set([
  "schema",
  "root",
  "unit_path",
  "launcher_path",
  "node_path",
  "trusted_key_path",
  "trusted_key_fingerprint",
  "test_harness",
]);
const STATUS_KEYS = new Set(["schema", "mode", "state", "candidate_count", "action"]);
const CAPABILITY_VALUES = new Set([
  "artifact_analysis",
  "citation_verification",
  "evidence_search",
  "experiment_execution",
  "experiment_planning",
  "reproduction",
  "research_session_signing",
  "section_drafting",
]);
const RESOURCE_VALUES = new Set([
  "artifact_io",
  "browser",
  "code_execution",
  "cpu",
  "gpu",
  "network",
  "sandbox",
]);
const MANAGED_DATA_FILES = new Set([
  "bridge.config.json",
  "identity.json",
  "service.json",
  "service-status.json",
  "state.json",
  "state.json.review-outbox.json",
]);

function randomSuffix() {
  return `${process.pid}.${randomBytes(8).toString("hex")}`;
}

async function syncDirectory(path) {
  const handle = await open(path, fsConstants.O_RDONLY);
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

function assertSafeAbsolutePath(path, field) {
  const value = resolve(path);
  if (
    value === "/" ||
    value.includes("\0") ||
    /[\r\n]/.test(value) ||
    value.split(sep).some(part => part === "..")
  ) {
    throw new Error(`${field} is unsafe`);
  }
  return value;
}

export function defaultInstallRoot() {
  const base = process.env.XDG_DATA_HOME
    ? resolve(process.env.XDG_DATA_HOME)
    : resolve(homedir(), ".local", "share");
  return join(base, "paper-raid-agent-bridge");
}

export function resolveInstallRoot(value) {
  return assertSafeAbsolutePath(value ? resolve(value) : defaultInstallRoot(), "install root");
}

function layout(root, { testHarness }) {
  const resolvedRoot = resolveInstallRoot(root);
  const configBase = process.env.XDG_CONFIG_HOME
    ? resolve(process.env.XDG_CONFIG_HOME)
    : resolve(homedir(), ".config");
  const binBase = resolve(homedir(), ".local", "bin");
  return Object.freeze({
    root: resolvedRoot,
    releases: join(resolvedRoot, "releases"),
    data: join(resolvedRoot, "data"),
    trust: join(resolvedRoot, "trust"),
    current: join(resolvedRoot, "current"),
    previous: join(resolvedRoot, "previous"),
    install: join(resolvedRoot, "installation.json"),
    trustedKey: join(resolvedRoot, "trust", "release-key.spki"),
    launcher: testHarness
      ? join(resolvedRoot, "bin", "paper-raid-agent-bridge")
      : join(binBase, "paper-raid-agent-bridge"),
    unit: testHarness
      ? join(resolvedRoot, "systemd", "user", UNIT_NAME)
      : join(configBase, "systemd", "user", UNIT_NAME),
    testHarness,
  });
}

async function ensureDirectory(path, { ownerOnly = true, recursive = false } = {}) {
  try {
    await mkdir(path, { recursive, mode: ownerOnly ? 0o700 : 0o755 });
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
  }
  const stats = await lstat(path);
  if (
    !stats.isDirectory() ||
    stats.isSymbolicLink() ||
    (stats.mode & 0o022) !== 0 ||
    (ownerOnly && (stats.mode & 0o077) !== 0) ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error(`${path} is not a safe user-owned directory`);
  }
}

async function writeExclusive(path, bytes, mode) {
  const handle = await open(
    path,
    fsConstants.O_WRONLY |
      fsConstants.O_CREAT |
      fsConstants.O_EXCL |
      (fsConstants.O_NOFOLLOW ?? 0),
    mode,
  );
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
  await chmod(path, mode);
  const installedHandle = await open(
    path,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    await installedHandle.sync();
  } finally {
    await installedHandle.close();
  }
  await syncDirectory(dirname(path));
  const stats = await lstat(path);
  return Object.freeze({ dev: stats.dev, ino: stats.ino });
}

function canonicalPrivateJson(value) {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function parseList(value, allowed, field, { allowEmpty = false } = {}) {
  if (typeof value !== "string") throw new Error(`${field} is required`);
  if (allowEmpty && (value === "none" || value === "")) return [];
  const output = value.split(",").map(item => item.trim()).filter(Boolean);
  if (
    output.length === 0 ||
    output.length > 16 ||
    new Set(output).size !== output.length ||
    output.some(item => !allowed.has(item))
  ) {
    throw new Error(`${field} contains an unsupported, duplicate, or empty value`);
  }
  return output.sort();
}

function shellSingleQuote(value) {
  if (/[\0\r\n]/.test(value)) throw new Error("launcher path contains control bytes");
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function launcherBytes(root, nodePath = process.execPath) {
  const cli = join(root, "current", "src", "cli.mjs");
  return Buffer.from(
    `#!/bin/sh\nexec ${shellSingleQuote(nodePath)} ${shellSingleQuote(cli)} "$@"\n`,
    "utf8",
  );
}

function systemdQuote(value) {
  if (/[\0\r\n]/.test(value)) throw new Error("systemd path contains control bytes");
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

function unitBytes(root, nodePath = process.execPath) {
  const cli = join(root, "current", "src", "cli.mjs");
  return Buffer.from(`[Unit]
Description=Paper Raid Agent Bridge
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${systemdQuote(nodePath)} ${systemdQuote(cli)} service --root ${systemdQuote(root)}
Restart=on-failure
RestartSec=5s
UMask=0077
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${systemdQuote(join(root, "data"))}
LockPersonality=yes
RestrictSUIDSGID=yes
RestrictAddressFamilies=AF_INET AF_INET6
SystemCallArchitectures=native

[Install]
WantedBy=default.target
`, "utf8");
}

async function executableSystemctl(path, { testHarness }) {
  if (testHarness && !path) {
    throw new Error("--root test harness requires an explicit --systemctl mock");
  }
  if (!testHarness && path !== undefined) {
    throw new Error("a custom systemctl executable is allowed only with an explicit test root");
  }
  let candidate;
  if (testHarness) {
    candidate = resolve(path);
  } else {
    for (const fixed of ["/usr/bin/systemctl", "/bin/systemctl"]) {
      try {
        await lstat(fixed);
        candidate = fixed;
        break;
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
    if (!candidate) throw new Error("the fixed host systemctl executable is unavailable");
  }
  const stats = await lstat(candidate);
  if (
    !stats.isFile() ||
    stats.isSymbolicLink() ||
    stats.nlink !== 1 ||
    (stats.mode & 0o022) !== 0 ||
    (stats.mode & 0o111) === 0 ||
    (testHarness && typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error("systemctl executable is unsafe");
  }
  return candidate;
}

function systemctlTimeout(options, testHarness) {
  const value = options?.systemctlTimeoutMs ?? SYSTEMCTL_TIMEOUT_MS;
  if (options?.systemctlTimeoutMs !== undefined && !testHarness) {
    throw new Error("a custom systemctl timeout is allowed only in the explicit test harness");
  }
  if (!Number.isSafeInteger(value) || value < 25 || value > SYSTEMCTL_TIMEOUT_MS) {
    throw new Error("systemctl timeout is outside the bounded lifecycle range");
  }
  return value;
}

async function systemctl(
  path,
  testHarness,
  args,
  { allowInactive = false, timeoutMs = SYSTEMCTL_TIMEOUT_MS } = {},
) {
  const executable = await executableSystemctl(path, { testHarness });
  const result = await new Promise((resolveChild, rejectChild) => {
    const child = spawn(executable, ["--user", ...args], {
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
      env: process.env,
    });
    const stdout = [];
    const stderr = [];
    let size = 0;
    let settled = false;
    let timedOut = false;
    const settle = callback => value => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      callback(value);
    };
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, timeoutMs);
    child.stdout.on("data", chunk => {
      size += chunk.length;
      if (size <= 64 * 1024) stdout.push(chunk);
    });
    child.stderr.on("data", chunk => {
      size += chunk.length;
      if (size <= 64 * 1024) stderr.push(chunk);
    });
    child.once("error", settle(rejectChild));
    child.once("close", code => {
      if (settled) return;
      settle(resolveChild)({
        code,
        timedOut,
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
      });
    });
  });
  if (result.timedOut) {
    throw new Error(`systemctl --user ${args[0]} timed out and was killed`);
  }
  if (result.code !== 0 && !(allowInactive && result.code === 3)) {
    throw new Error(`systemctl --user ${args[0]} failed closed`);
  }
  return result;
}

function pointerTarget(releaseIdentity) {
  return `releases/${releaseIdentity}`;
}

async function atomicPointer(root, name, releaseIdentity) {
  const linkPath = join(root, name);
  const temporary = join(root, `.${name}.${randomSuffix()}.new`);
  try {
    const current = await lstat(linkPath);
    if (
      !current.isSymbolicLink() ||
      (typeof process.getuid === "function" && current.uid !== process.getuid())
    ) {
      throw new Error(`${name} is not a managed release pointer`);
    }
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  await symlink(pointerTarget(releaseIdentity), temporary, "dir");
  try {
    await rename(temporary, linkPath);
    await syncDirectory(root);
  } finally {
    await unlink(temporary).catch(error => {
      if (error?.code !== "ENOENT") throw error;
    });
  }
}

async function removePointer(path, { optional = false } = {}) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (optional && error?.code === "ENOENT") return;
    throw error;
  }
  if (
    !stats.isSymbolicLink() ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error(`${path} is not a managed symlink`);
  }
  await unlink(path);
  await syncDirectory(dirname(path));
}

async function readPointer(layoutValue, name, { optional = false } = {}) {
  const path = layoutValue[name];
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (optional && error?.code === "ENOENT") return null;
    throw error;
  }
  if (
    !stats.isSymbolicLink() ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error(`${name} release pointer is unsafe`);
  }
  const target = await readlink(path);
  if (!/^releases\/[0-9]{12}-[0-9A-Za-z.-]+$/.test(target)) {
    throw new Error(`${name} release pointer escaped the managed release directory`);
  }
  const identity = target.slice("releases/".length);
  const releasePath = resolve(layoutValue.root, target);
  if (!releasePath.startsWith(`${layoutValue.releases}${sep}`)) {
    throw new Error(`${name} release pointer escaped the install root`);
  }
  return Object.freeze({ identity, path: releasePath });
}

function bridgeConfig({ bffUrl, capabilities, resourceClasses, maxParallelTasks = 1 }) {
  return {
    schema: CONFIG_SCHEMA,
    bff_url: bffUrl,
    identity_file: "identity.json",
    state_file: "state.json",
    capabilities,
    resource_classes: resourceClasses,
    max_parallel_tasks: maxParallelTasks,
    paper_ids: [],
    poll_interval_ms: 5_000,
    request_timeout_ms: 5_000,
  };
}

function serviceConfig(mode) {
  if (!new Set(["confirm", "auto"]).has(mode)) {
    throw new Error("service mode must be confirm or auto");
  }
  return { schema: SERVICE_CONFIG_SCHEMA, mode };
}

function installationDocument(layoutValue, fingerprint, nodePath) {
  return {
    schema: INSTALL_SCHEMA,
    root: layoutValue.root,
    unit_path: layoutValue.unit,
    launcher_path: layoutValue.launcher,
    node_path: nodePath,
    trusted_key_path: layoutValue.trustedKey,
    trusted_key_fingerprint: fingerprint,
    test_harness: layoutValue.testHarness,
  };
}

async function loadInstallation(root) {
  const requestedRoot = resolveInstallRoot(root);
  const value = await readSafeJson(join(requestedRoot, "installation.json"), {
    privateFile: true,
    maxBytes: 64 * 1024,
  });
  if (
    !value ||
    Array.isArray(value) ||
    typeof value !== "object" ||
    value.schema !== INSTALL_SCHEMA ||
    Object.keys(value).length !== INSTALL_KEYS.size ||
    Object.keys(value).some(key => !INSTALL_KEYS.has(key)) ||
    typeof value.test_harness !== "boolean"
  ) {
    throw new Error("installed lifecycle metadata is invalid");
  }
  const expectedLayout = layout(value.root, { testHarness: value.test_harness });
  if (
    value.root !== requestedRoot ||
    value.root !== expectedLayout.root ||
    value.unit_path !== expectedLayout.unit ||
    value.launcher_path !== expectedLayout.launcher ||
    value.trusted_key_path !== expectedLayout.trustedKey ||
    value.node_path !== process.execPath
  ) {
    throw new Error("installed lifecycle paths or Node runtime changed");
  }
  return Object.freeze({ document: value, layout: expectedLayout });
}

async function storedTrust(installation) {
  return readTrustedPublicKey(
    installation.document.trusted_key_path,
    installation.document.trusted_key_fingerprint,
  );
}

async function verifyCurrent(installation) {
  const pointer = await readPointer(installation.layout, "current");
  const trusted = await storedTrust(installation);
  const verified = await verifyInstalledRelease({
    releasePath: pointer.path,
    trusted,
    expectedReleaseId: pointer.identity,
  });
  return Object.freeze({ pointer, trusted, verified });
}

async function exactManagedDataDirectory(dataPath) {
  let entries;
  try {
    entries = await readdir(dataPath, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }
  for (const entry of entries) {
    if (!entry.isFile() || entry.isSymbolicLink() || !MANAGED_DATA_FILES.has(entry.name)) {
      throw new Error("data directory contains an unmanaged or non-regular entry");
    }
    const stats = await lstat(join(dataPath, entry.name));
    if (
      stats.nlink !== 1 ||
      (stats.mode & 0o077) !== 0 ||
      (typeof process.getuid === "function" && stats.uid !== process.getuid())
    ) {
      throw new Error("data directory contains an unsafe file");
    }
  }
}

async function prepareInstallLayout(layoutValue) {
  let rootExisted = true;
  let dataExisted = true;
  let initialDataNames = new Set();
  try {
    try {
      const existing = await lstat(layoutValue.root);
      if (!existing.isDirectory() || existing.isSymbolicLink()) {
        throw new Error("install root is not a directory");
      }
      const names = await readdir(layoutValue.root);
      if (names.some(name => name !== "data")) {
        throw new Error("install root already contains an installation or unmanaged data");
      }
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      rootExisted = false;
      dataExisted = false;
      await ensureDirectory(layoutValue.root, { ownerOnly: true, recursive: true });
    }
    await ensureDirectory(layoutValue.root);
    if (rootExisted) {
      try {
        await lstat(layoutValue.data);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
        dataExisted = false;
      }
    }
    await ensureDirectory(layoutValue.data);
    await exactManagedDataDirectory(layoutValue.data);
    initialDataNames = new Set(await readdir(layoutValue.data));
    await ensureDirectory(layoutValue.releases);
    await ensureDirectory(layoutValue.trust);
    await ensureDirectory(dirname(layoutValue.launcher), {
      ownerOnly: layoutValue.testHarness,
      recursive: true,
    });
    await ensureDirectory(dirname(layoutValue.unit), {
      ownerOnly: layoutValue.testHarness,
      recursive: true,
    });
    for (const path of [layoutValue.launcher, layoutValue.unit]) {
      try {
        await lstat(path);
        throw new Error(`${path} already exists and will not be overwritten`);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
  } catch (error) {
    const cleanupFailures = [];
    for (const directory of [layoutValue.trust, layoutValue.releases]) {
      try {
        await rmdir(directory);
      } catch (cleanupError) {
        if (!new Set(["ENOENT", "ENOTEMPTY"]).has(cleanupError?.code)) {
          cleanupFailures.push(cleanupError);
        }
      }
    }
    if (!dataExisted) {
      try {
        await rmdir(layoutValue.data);
      } catch (cleanupError) {
        if (!new Set(["ENOENT", "ENOTEMPTY"]).has(cleanupError?.code)) {
          cleanupFailures.push(cleanupError);
        }
      }
    }
    if (layoutValue.testHarness) {
      await removeEmptyParent(layoutValue.unit, layoutValue.root).catch(cleanupError => {
        cleanupFailures.push(cleanupError);
      });
      await removeEmptyParent(layoutValue.launcher, layoutValue.root).catch(cleanupError => {
        cleanupFailures.push(cleanupError);
      });
    }
    if (!rootExisted) {
      try {
        await rmdir(layoutValue.root);
      } catch (cleanupError) {
        if (!new Set(["ENOENT", "ENOTEMPTY"]).has(cleanupError?.code)) {
          cleanupFailures.push(cleanupError);
        }
      }
    }
    if (cleanupFailures.length !== 0) {
      throw new AggregateError(
        [error, ...cleanupFailures],
        "install layout preparation failed and cleanup also failed closed",
      );
    }
    throw error;
  }
  return Object.freeze({ rootExisted, dataExisted, initialDataNames });
}

async function installIdentity(dataPath, agentId) {
  const path = join(dataPath, "identity.json");
  try {
    const identity = await loadIdentity(path);
    if (identity.agent_id !== agentId) {
      throw new Error("preserved identity belongs to a different Agent ID");
    }
    return Object.freeze({ identity, created: null });
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  await generateIdentity(agentId, path);
  const stats = await lstat(path);
  return Object.freeze({
    identity: await loadIdentity(path),
    created: Object.freeze({ dev: stats.dev, ino: stats.ino }),
  });
}

async function stageAndPublish(layoutValue, verified) {
  const finalPath = join(layoutValue.releases, verified.releaseId);
  try {
    await lstat(finalPath);
    throw new Error("release identity is already installed");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const staging = join(layoutValue.releases, `.staging.${verified.releaseId}.${randomSuffix()}`);
  await extractVerifiedRelease(verified, staging);
  let finalIdentity;
  try {
    finalIdentity = await publishStagedRelease(staging, finalPath);
  } catch (error) {
    await removeStagingRelease(staging).catch(() => {});
    throw error;
  }
  try {
    await verifyInstalledRelease({
      releasePath: finalPath,
      trusted: verified.trusted,
      expectedReleaseId: verified.releaseId,
    });
  } catch (error) {
    await removeStagingRelease(finalPath, { expectedIdentity: finalIdentity }).catch(() => {});
    throw error;
  }
  return Object.freeze({ path: finalPath, identity: finalIdentity });
}

async function removeExactFile(path, expectedBytes, { optional = false } = {}) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (optional && error?.code === "ENOENT") return;
    throw error;
  }
  if (
    !stats.isFile() ||
    stats.isSymbolicLink() ||
    stats.nlink !== 1 ||
    (stats.mode & 0o077) !== 0 ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error(`${path} is not a managed regular file`);
  }
  const handle = await open(path, fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0));
  try {
    const opened = await handle.stat();
    if (
      !opened.isFile() ||
      opened.nlink !== 1 ||
      opened.dev !== stats.dev ||
      opened.ino !== stats.ino ||
      opened.size !== stats.size
    ) {
      throw new Error(`${path} changed while being opened`);
    }
    const bytes = await handle.readFile();
    const final = await handle.stat();
    if (
      final.dev !== opened.dev ||
      final.ino !== opened.ino ||
      final.size !== opened.size ||
      bytes.length !== final.size
    ) {
      throw new Error(`${path} changed while being read`);
    }
    if (expectedBytes && !bytes.equals(expectedBytes)) {
      throw new Error(`${path} differs from the managed installation`);
    }
  } finally {
    await handle.close();
  }
  await unlink(path);
  await syncDirectory(dirname(path));
}

async function removeCreatedFile(path, identity) {
  if (!identity) return;
  const stats = await lstat(path);
  if (
    !stats.isFile() ||
    stats.isSymbolicLink() ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid()) ||
    stats.dev !== identity.dev ||
    stats.ino !== identity.ino
  ) {
    throw new Error("refusing to remove a file that replaced this install's inode");
  }
  await unlink(path);
  await syncDirectory(dirname(path));
}

function sameFileIdentity(stats, identity) {
  return stats.dev === identity.dev && stats.ino === identity.ino;
}

async function transactionalPrivateJsonReplace(path, value, records) {
  const record = {
    path,
    backup: null,
    original: null,
    replacement: null,
  };
  records.push(record);
  try {
    const existing = await lstat(path);
    if (
      !existing.isFile() ||
      existing.isSymbolicLink() ||
      existing.nlink !== 1 ||
      (existing.mode & 0o077) !== 0 ||
      (typeof process.getuid === "function" && existing.uid !== process.getuid())
    ) {
      throw new Error(`${path} is not one owner-only lifecycle data file`);
    }
    record.original = Object.freeze({ dev: existing.dev, ino: existing.ino });
    record.backup = join(
      dirname(path),
      `.${basename(path)}.${randomSuffix()}.install-backup`,
    );
    await link(path, record.backup);
    await syncDirectory(dirname(path));
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  record.replacement = await writePrivateJsonReplacing(path, value, {
    expectedExistingIdentity: record.original,
  });
  return record;
}

async function rollbackPrivateJsonReplacements(records) {
  const failures = [];
  for (const record of [...records].reverse()) {
    try {
      let current = null;
      try {
        current = await lstat(record.path);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
      if (record.backup) {
        if (current && record.replacement && sameFileIdentity(current, record.replacement)) {
          await rename(record.backup, record.path);
          await syncDirectory(dirname(record.path));
        } else if (current && record.original && sameFileIdentity(current, record.original)) {
          await unlink(record.backup);
          await syncDirectory(dirname(record.path));
        } else if (!current) {
          await rename(record.backup, record.path);
          await syncDirectory(dirname(record.path));
        } else {
          throw new Error("refusing to restore lifecycle data over a replacement inode");
        }
      } else if (record.replacement) {
        await removeCreatedFile(record.path, record.replacement);
      }
    } catch (error) {
      failures.push(error);
    }
  }
  if (failures.length !== 0) {
    throw new AggregateError(failures, "lifecycle data transaction rollback failed closed");
  }
}

async function commitPrivateJsonReplacements(records) {
  for (const record of records) {
    if (record.backup) {
      await unlink(record.backup);
      await syncDirectory(dirname(record.backup));
      record.backup = null;
    }
  }
}

async function removeNewManagedData(layoutValue, preparation) {
  const names = await readdir(layoutValue.data);
  for (const name of names) {
    if (!preparation.initialDataNames.has(name)) {
      if (!MANAGED_DATA_FILES.has(name)) {
        throw new Error("failed install left an unmanaged lifecycle data entry");
      }
      await removeExactFile(join(layoutValue.data, name), null);
    }
  }
  if (!preparation.dataExisted && (await readdir(layoutValue.data)).length === 0) {
    await rmdir(layoutValue.data);
    await syncDirectory(layoutValue.root);
  }
}

async function cleanupPartialInstall(
  layoutValue,
  verified,
  created,
  preparation,
  dataTransactions,
) {
  const failures = [];
  const attempt = async action => {
    try {
      await action();
    } catch (error) {
      failures.push(error);
    }
  };
  if (created.current) {
    await attempt(() => removePointer(layoutValue.current, { optional: true }));
  }
  await attempt(() => removeCreatedFile(layoutValue.launcher, created.launcher));
  await attempt(() => removeCreatedFile(layoutValue.unit, created.unit));
  await attempt(() => removeCreatedFile(layoutValue.install, created.install));
  await attempt(() => removeCreatedFile(layoutValue.trustedKey, created.trustedKey));
  if (verified && created.release) {
    await attempt(() => removeStagingRelease(
      join(layoutValue.releases, verified.releaseId),
      { expectedIdentity: created.release },
    ));
  }
  await attempt(() => rollbackPrivateJsonReplacements(dataTransactions));
  await attempt(() => removeCreatedFile(join(layoutValue.data, "identity.json"), created.identity));
  await attempt(() => removeNewManagedData(layoutValue, preparation));
  for (const directory of [layoutValue.trust, layoutValue.releases]) {
    await attempt(async () => {
      try {
        await rmdir(directory);
      } catch (error) {
        if (!new Set(["ENOENT", "ENOTEMPTY"]).has(error?.code)) throw error;
      }
    });
  }
  if (layoutValue.testHarness) {
    await attempt(() => removeEmptyParent(layoutValue.unit, layoutValue.root));
    await attempt(() => removeEmptyParent(layoutValue.launcher, layoutValue.root));
  }
  if (!preparation.rootExisted) {
    await attempt(async () => {
      const remaining = await readdir(layoutValue.root);
      if (remaining.length !== 0) {
        throw new Error("failed fresh install did not leave an empty managed root");
      }
      await rmdir(layoutValue.root);
    });
  }
  if (failures.length !== 0) {
    throw new AggregateError(failures, "partial Agent Bridge installation cleanup failed closed");
  }
}

export async function installProduct(options) {
  const testHarness = options.root !== undefined;
  const layoutValue = layout(resolveInstallRoot(options.root), { testHarness });
  const timeoutMs = systemctlTimeout(options, testHarness);
  const mode = options.mode ?? "confirm";
  if (mode === "auto" && options.autoAcknowledged !== true) {
    throw new Error("Auto mode requires the explicit --acknowledge-auto flag");
  }
  const capabilities = parseList(options.capabilities, CAPABILITY_VALUES, "capabilities");
  const resourceClasses = parseList(
    options.resourceClasses,
    RESOURCE_VALUES,
    "resource classes",
    { allowEmpty: true },
  );
  const verified = await readReleaseInputs({
    packagePath: options.packagePath,
    manifestPath: options.manifestPath,
    signaturePath: options.signaturePath,
    trustedKeyPath: options.trustedKeyPath,
    fingerprint: options.fingerprint,
  });
  await executableSystemctl(options.systemctlPath, { testHarness });
  const preparation = await prepareInstallLayout(layoutValue);
  const dataTransactions = [];
  const created = {
    trustedKey: null,
    release: false,
    current: false,
    launcher: null,
    unit: null,
    install: null,
    identity: null,
  };
  try {
    created.trustedKey = await writeExclusive(
      layoutValue.trustedKey,
      verified.trusted.spki,
      0o400,
    );
    const installedIdentity = await installIdentity(layoutValue.data, options.agentId);
    created.identity = installedIdentity.created;
    await transactionalPrivateJsonReplace(
      installedBridgeConfigPath(layoutValue.root),
      bridgeConfig({
        bffUrl: options.bffUrl,
        capabilities,
        resourceClasses,
      }),
      dataTransactions,
    );
    await loadConfig(installedBridgeConfigPath(layoutValue.root));
    await transactionalPrivateJsonReplace(
      serviceConfigPath(layoutValue.root),
      serviceConfig(mode),
      dataTransactions,
    );
    await loadServiceConfig(layoutValue.root);
    const published = await stageAndPublish(layoutValue, verified);
    created.release = published.identity;
    await verifyInstalledRelease({
      releasePath: published.path,
      trusted: verified.trusted,
      expectedReleaseId: verified.releaseId,
    });
    created.current = true;
    await atomicPointer(layoutValue.root, "current", verified.releaseId);
    created.launcher = await writeExclusive(
      layoutValue.launcher,
      launcherBytes(layoutValue.root),
      0o700,
    );
    created.unit = await writeExclusive(layoutValue.unit, unitBytes(layoutValue.root), 0o600);
    created.install = await writeExclusive(
      layoutValue.install,
      canonicalPrivateJson(
        installationDocument(layoutValue, verified.trusted.fingerprint, process.execPath),
      ),
      0o600,
    );
    await verifyCurrent(await loadInstallation(layoutValue.root));
    await systemctl(options.systemctlPath, testHarness, ["daemon-reload"], { timeoutMs });
    await systemctl(
      options.systemctlPath,
      testHarness,
      ["enable", "--now", UNIT_NAME],
      { timeoutMs },
    );
    await systemctl(
      options.systemctlPath,
      testHarness,
      ["is-active", "--quiet", UNIT_NAME],
      { timeoutMs },
    );
    await commitPrivateJsonReplacements(dataTransactions);
    return Object.freeze({ status: "installed", mode, version: verified.manifest.version });
  } catch (error) {
    const cleanupFailures = [];
    try {
      await systemctl(options.systemctlPath, testHarness, ["disable", "--now", UNIT_NAME], {
        allowInactive: true,
        timeoutMs,
      });
    } catch (cleanupError) {
      cleanupFailures.push(cleanupError);
    }
    try {
      await cleanupPartialInstall(
        layoutValue,
        verified,
        created,
        preparation,
        dataTransactions,
      );
    } catch (cleanupError) {
      cleanupFailures.push(cleanupError);
    }
    if (cleanupFailures.length !== 0) {
      throw new AggregateError(
        [error, ...cleanupFailures],
        "Agent Bridge install failed and cleanup also failed closed",
      );
    }
    throw error;
  }
}

async function assertOperatorTrustMatches(installation, trusted) {
  if (trusted.fingerprint !== installation.document.trusted_key_fingerprint) {
    throw new Error("operator key pin does not match the installed trust root");
  }
  const stored = await storedTrust(installation);
  if (!stored.spki.equals(trusted.spki)) {
    throw new Error("operator key does not match the installed trust root");
  }
  return stored;
}

async function lifecyclePhase(options, installation, phase) {
  if (options.testHooks === undefined) return;
  if (
    !installation.layout.testHarness ||
    !options.testHooks ||
    typeof options.testHooks.phase !== "function"
  ) {
    throw new Error("lifecycle phase hooks are restricted to the explicit test harness");
  }
  await options.testHooks.phase(phase);
}

export async function updateProduct(options) {
  const installation = await loadInstallation(resolveInstallRoot(options.root));
  const timeoutMs = systemctlTimeout(options, installation.layout.testHarness);
  const current = await verifyCurrent(installation);
  const oldPrevious = await readPointer(installation.layout, "previous", { optional: true });
  const verifiedOldPrevious = oldPrevious
    ? await verifyInstalledRelease({
        releasePath: oldPrevious.path,
        trusted: current.trusted,
        expectedReleaseId: oldPrevious.identity,
      })
    : null;
  const verified = await readReleaseInputs({
    packagePath: options.packagePath,
    manifestPath: options.manifestPath,
    signaturePath: options.signaturePath,
    trustedKeyPath: options.trustedKeyPath,
    fingerprint: options.fingerprint,
  });
  await assertOperatorTrustMatches(installation, verified.trusted);
  const highestKnownSequence = Math.max(
    current.verified.manifest.sequence,
    verifiedOldPrevious?.manifest.sequence ?? 0,
  );
  if (verified.manifest.sequence <= highestKnownSequence) {
    throw new Error("update rejects same-sequence and every known-history downgrade release");
  }
  const published = await stageAndPublish(installation.layout, verified);
  let previousAttempted = false;
  let currentAttempted = false;
  try {
    await lifecyclePhase(options, installation, "update_after_publish");
    previousAttempted = true;
    await atomicPointer(installation.layout.root, "previous", current.pointer.identity);
    await lifecyclePhase(options, installation, "update_after_previous");
    currentAttempted = true;
    await atomicPointer(installation.layout.root, "current", verified.releaseId);
    await lifecyclePhase(options, installation, "update_after_current");
    await verifyCurrent(installation);
    await lifecyclePhase(options, installation, "update_after_verify");
    await systemctl(
      options.systemctlPath,
      installation.layout.testHarness,
      ["restart", UNIT_NAME],
      { timeoutMs },
    );
    await lifecyclePhase(options, installation, "update_after_restart");
    await systemctl(options.systemctlPath, installation.layout.testHarness, [
      "is-active",
      "--quiet",
      UNIT_NAME,
    ], { timeoutMs });
  } catch (error) {
    const recoveryFailures = [];
    if (currentAttempted) {
      try {
        await atomicPointer(installation.layout.root, "current", current.pointer.identity);
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    if (previousAttempted) {
      try {
        if (oldPrevious) {
          await atomicPointer(installation.layout.root, "previous", oldPrevious.identity);
        } else {
          await removePointer(installation.layout.previous, { optional: true });
        }
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    if (currentAttempted || previousAttempted) {
      try {
        await systemctl(options.systemctlPath, installation.layout.testHarness, [
          "restart",
          UNIT_NAME,
        ], { timeoutMs });
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    try {
      await removeStagingRelease(published.path, {
        expectedIdentity: published.identity,
      });
    } catch (recoveryError) {
      recoveryFailures.push(recoveryError);
    }
    if (recoveryFailures.length !== 0) {
      throw new AggregateError(
        [error, ...recoveryFailures],
        "Agent Bridge update failed and recovery also failed closed",
      );
    }
    throw error;
  }
  if (oldPrevious && oldPrevious.identity !== current.pointer.identity) {
    await verifyInstalledRelease({
      releasePath: oldPrevious.path,
      trusted: current.trusted,
      expectedReleaseId: oldPrevious.identity,
    });
    await removeStagingRelease(oldPrevious.path, {
      expectedIdentity: verifiedOldPrevious.rootIdentity,
    });
  }
  return Object.freeze({ status: "updated", mode: (await loadServiceConfig(installation.layout.root)).mode, version: verified.manifest.version });
}

export async function rollbackProduct(options) {
  const installation = await loadInstallation(resolveInstallRoot(options.root));
  const timeoutMs = systemctlTimeout(options, installation.layout.testHarness);
  const current = await verifyCurrent(installation);
  const previous = await readPointer(installation.layout, "previous");
  const operatorTrust = await readTrustedPublicKey(options.trustedKeyPath, options.fingerprint);
  await assertOperatorTrustMatches(installation, operatorTrust);
  const verifiedPrevious = await verifyInstalledRelease({
    releasePath: previous.path,
    trusted: operatorTrust,
    expectedReleaseId: previous.identity,
    signatureOverridePath: options.signaturePath,
  });
  if (verifiedPrevious.manifest.sequence >= current.verified.manifest.sequence) {
    throw new Error("rollback target is not the verified lower-sequence previous release");
  }
  let previousAttempted = false;
  let currentAttempted = false;
  try {
    previousAttempted = true;
    await atomicPointer(installation.layout.root, "previous", current.pointer.identity);
    await lifecyclePhase(options, installation, "rollback_after_previous");
    currentAttempted = true;
    await atomicPointer(installation.layout.root, "current", previous.identity);
    await lifecyclePhase(options, installation, "rollback_after_current");
    await verifyCurrent(installation);
    await lifecyclePhase(options, installation, "rollback_after_verify");
    await systemctl(
      options.systemctlPath,
      installation.layout.testHarness,
      ["restart", UNIT_NAME],
      { timeoutMs },
    );
    await lifecyclePhase(options, installation, "rollback_after_restart");
    await systemctl(options.systemctlPath, installation.layout.testHarness, [
      "is-active",
      "--quiet",
      UNIT_NAME,
    ], { timeoutMs });
  } catch (error) {
    const recoveryFailures = [];
    if (currentAttempted) {
      try {
        await atomicPointer(installation.layout.root, "current", current.pointer.identity);
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    if (previousAttempted) {
      try {
        await atomicPointer(installation.layout.root, "previous", previous.identity);
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    if (currentAttempted || previousAttempted) {
      try {
        await systemctl(options.systemctlPath, installation.layout.testHarness, [
          "restart",
          UNIT_NAME,
        ], { timeoutMs });
      } catch (recoveryError) {
        recoveryFailures.push(recoveryError);
      }
    }
    if (recoveryFailures.length !== 0) {
      throw new AggregateError(
        [error, ...recoveryFailures],
        "Agent Bridge rollback failed and recovery also failed closed",
      );
    }
    throw error;
  }
  return Object.freeze({
    status: "rolled_back",
    mode: (await loadServiceConfig(installation.layout.root)).mode,
    version: verifiedPrevious.manifest.version,
  });
}

async function safeServiceStatus(root) {
  try {
    const value = await readSafeJson(serviceStatusPath(root), {
      privateFile: true,
      maxBytes: 4096,
    });
    if (
      !value ||
      Array.isArray(value) ||
      value.schema !== SERVICE_STATUS_SCHEMA ||
      Object.keys(value).length !== STATUS_KEYS.size ||
      Object.keys(value).some(key => !STATUS_KEYS.has(key))
    ) {
      return null;
    }
    return value;
  } catch {
    return null;
  }
}

export async function diagnoseProduct(options) {
  const timeoutMs = systemctlTimeout(options, options.root !== undefined);
  let installation;
  let verified;
  let localIntegrity = false;
  let identityReady = false;
  let paired = false;
  let serviceMode = "unknown";
  let daemonState = "unknown";
  let candidateCount = 0;
  let serviceActive = false;
  try {
    installation = await loadInstallation(resolveInstallRoot(options.root));
    verified = await verifyCurrent(installation);
    localIntegrity = true;
    const config = await loadConfig(installedBridgeConfigPath(installation.layout.root));
    const identity = await loadIdentity(config.identity_file);
    identityReady = Boolean(identity);
    try {
      const state = await loadBridgeState(config.state_file, identity);
      paired = state.agent_id === identity.agent_id;
    } catch {
      paired = false;
    }
    serviceMode = (await loadServiceConfig(installation.layout.root)).mode;
    const status = await safeServiceStatus(installation.layout.root);
    daemonState = status?.state ?? "starting";
    candidateCount = status?.candidate_count ?? 0;
    serviceActive = (
      await systemctl(
        options.systemctlPath,
        installation.layout.testHarness,
        ["is-active", "--quiet", UNIT_NAME],
        { allowInactive: true, timeoutMs },
      )
    ).code === 0;
  } catch {
    // A diagnostic is useful even when verification fails. It never exposes
    // the failing path, identity, UUID, digest, or raw exception.
  }
  const ok = localIntegrity && identityReady && paired && serviceActive;
  return Object.freeze({
    schema: "hepta.paper_raid.agent_bridge.diagnosis.v1",
    status: ok ? "ready" : "needs_attention",
    release: localIntegrity ? "verified" : "unverified",
    identity: identityReady ? "ready" : "unavailable",
    pairing: paired ? "paired" : "not_paired",
    service_mode: serviceMode,
    service: serviceActive ? "active" : "inactive",
    work: candidateCount === 0 ? "idle" : "waiting",
    daemon: daemonState,
    version: verified?.verified.manifest.version ?? null,
  });
}

export function formatDiagnosis(value) {
  const headline = value.status === "ready"
    ? "Agent Bridge is installed and its release is verified."
    : "Agent Bridge needs attention; no unverified release will run.";
  const pairing = value.pairing === "paired"
    ? "Pairing is ready."
    : "Pairing is not complete; generate one browser code and run Pair.";
  const work = value.work === "waiting"
    ? value.service_mode === "confirm"
      ? "Work is waiting for local confirmation."
      : "Work is waiting, but Auto will act only when exactly one item is actionable."
    : "No actionable work is waiting.";
  return `${headline}\nService: ${value.service} (${value.service_mode}). ${pairing}\n${work}\n`;
}

export async function setServiceMode(options) {
  const installation = await loadInstallation(resolveInstallRoot(options.root));
  const timeoutMs = systemctlTimeout(options, installation.layout.testHarness);
  await verifyCurrent(installation);
  if (options.mode === "auto" && options.autoAcknowledged !== true) {
    throw new Error("Auto mode requires the explicit --acknowledge-auto flag");
  }
  const oldService = await loadServiceConfig(installation.layout.root);
  const dataTransactions = [];
  try {
    await transactionalPrivateJsonReplace(
      serviceConfigPath(installation.layout.root),
      serviceConfig(options.mode),
      dataTransactions,
    );
    await loadServiceConfig(installation.layout.root);
    await lifecyclePhase(options, installation, "mode_after_write");
    await systemctl(options.systemctlPath, installation.layout.testHarness, [
      "restart",
      UNIT_NAME,
    ], { timeoutMs });
    await lifecyclePhase(options, installation, "mode_after_restart");
    await systemctl(options.systemctlPath, installation.layout.testHarness, [
      "is-active",
      "--quiet",
      UNIT_NAME,
    ], { timeoutMs });
    await commitPrivateJsonReplacements(dataTransactions);
  } catch (error) {
    const recoveryFailures = [];
    try {
      await rollbackPrivateJsonReplacements(dataTransactions);
    } catch (recoveryError) {
      recoveryFailures.push(recoveryError);
    }
    try {
      const restored = await loadServiceConfig(installation.layout.root);
      if (restored.mode !== oldService.mode) {
        throw new Error("service mode transaction did not restore the previous mode");
      }
    } catch (recoveryError) {
      recoveryFailures.push(recoveryError);
    }
    try {
      await systemctl(options.systemctlPath, installation.layout.testHarness, [
        "restart",
        UNIT_NAME,
      ], { timeoutMs });
    } catch (recoveryError) {
      recoveryFailures.push(recoveryError);
    }
    if (recoveryFailures.length !== 0) {
      throw new AggregateError(
        [error, ...recoveryFailures],
        "Agent Bridge mode change failed and recovery also failed closed",
      );
    }
    throw error;
  }
  return Object.freeze({ status: "mode_changed", mode: options.mode });
}

async function removeData(layoutValue) {
  await exactManagedDataDirectory(layoutValue.data);
  const entries = await readdir(layoutValue.data);
  for (const name of entries) {
    await removeExactFile(join(layoutValue.data, name), null);
  }
  await rmdir(layoutValue.data);
  await syncDirectory(layoutValue.root);
}

async function removeEmptyParent(path, stop) {
  let current = dirname(path);
  const boundary = resolve(stop);
  while (current.startsWith(`${boundary}${sep}`) && current !== boundary) {
    try {
      await rmdir(current);
    } catch {
      break;
    }
    current = dirname(current);
  }
}

export async function uninstallProduct(options) {
  const installation = await loadInstallation(resolveInstallRoot(options.root));
  const timeoutMs = systemctlTimeout(options, installation.layout.testHarness);
  if (options.purgeData === true && !installation.layout.testHarness) {
    throw new Error("data purge is restricted to an explicit isolated test root");
  }
  const current = await verifyCurrent(installation);
  const previous = await readPointer(installation.layout, "previous", { optional: true });
  const verifiedPrevious = previous
    ? await verifyInstalledRelease({
      releasePath: previous.path,
      trusted: current.trusted,
      expectedReleaseId: previous.identity,
    })
    : null;
  const releaseEntries = await readdir(installation.layout.releases);
  const allowedReleases = new Set([
    current.pointer.identity,
    ...(previous ? [previous.identity] : []),
  ]);
  if (
    releaseEntries.length !== allowedReleases.size ||
    releaseEntries.some(name => !allowedReleases.has(name))
  ) {
    throw new Error("release directory contains an unmanaged or unverified entry");
  }
  await systemctl(options.systemctlPath, installation.layout.testHarness, [
    "disable",
    "--now",
    UNIT_NAME,
  ], { allowInactive: true, timeoutMs });
  await removeExactFile(
    installation.layout.unit,
    unitBytes(installation.layout.root, installation.document.node_path),
  );
  await removeExactFile(
    installation.layout.launcher,
    launcherBytes(installation.layout.root, installation.document.node_path),
  );
  await removePointer(installation.layout.current);
  await removePointer(installation.layout.previous, { optional: true });
  if (previous) {
    await removeStagingRelease(previous.path, {
      expectedIdentity: verifiedPrevious.rootIdentity,
    });
  }
  await removeStagingRelease(current.pointer.path, {
    expectedIdentity: current.verified.rootIdentity,
  });
  await rmdir(installation.layout.releases);
  await removeExactFile(installation.layout.trustedKey, current.trusted.spki);
  await rmdir(installation.layout.trust);
  await removeExactFile(installation.layout.install, null);
  if (options.purgeData === true) await removeData(installation.layout);
  await systemctl(
    options.systemctlPath,
    installation.layout.testHarness,
    ["daemon-reload"],
    { timeoutMs },
  );
  if (installation.layout.testHarness) {
    await removeEmptyParent(installation.layout.unit, installation.layout.root).catch(() => {});
    await removeEmptyParent(installation.layout.launcher, installation.layout.root).catch(() => {});
  }
  const remaining = await readdir(installation.layout.root);
  if (remaining.length === 0) {
    await rmdir(installation.layout.root);
  } else if (!(remaining.length === 1 && remaining[0] === "data")) {
    throw new Error("uninstall left an unexpected managed-root entry");
  }
  return Object.freeze({
    status: "uninstalled",
    data: options.purgeData === true ? "purged" : "preserved",
  });
}

export async function runInstalledService(options) {
  const installation = await loadInstallation(resolveInstallRoot(options.root));
  await verifyCurrent(installation);
  return runDaemon(installation.layout.root);
}

export async function installedConfigPath(root) {
  const installation = await loadInstallation(resolveInstallRoot(root));
  await verifyCurrent(installation);
  return installedBridgeConfigPath(installation.layout.root);
}

export async function installedConfirmConfigPath(root) {
  const installation = await loadInstallation(resolveInstallRoot(root));
  await verifyCurrent(installation);
  const service = await loadServiceConfig(installation.layout.root);
  if (service.mode !== "confirm") {
    throw new Error("manual Confirm is disabled while the background service is in Auto mode");
  }
  return installedBridgeConfigPath(installation.layout.root);
}
