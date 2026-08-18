import assert from "node:assert/strict";
import {
  chown,
  chmod,
  link,
  lstat,
  mkdir,
  mkdtemp,
  realpath,
  readFile,
  readdir,
  readlink,
  rename,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import {
  createHash,
  generateKeyPairSync,
  sign as ed25519Sign,
} from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

import {
  AGENT_BRIDGE_REQUEST_PROOF_SCHEMA,
  agentCapabilityDisclosureHash,
  agentBridgeRequestProofFrame,
  canonicalJsonBytes,
  sha256Digest,
} from "../src/canonical.mjs";
import { main as cliMain } from "../src/cli.mjs";
import { loadConfig } from "../src/config.mjs";
import {
  daemonCycle,
  SERVICE_CONFIG_SCHEMA,
  serviceConfigPath,
} from "../src/daemon.mjs";
import { writePrivateJsonReplacing } from "../src/files.mjs";
import { loadIdentity } from "../src/identity.mjs";
import {
  diagnoseProduct,
  installProduct,
  installedConfirmConfigPath,
  rollbackProduct,
  setServiceMode,
  uninstallProduct,
  updateProduct,
} from "../src/lifecycle.mjs";
import {
  buildDeterministicRelease,
  extractVerifiedRelease,
  publishStagedRelease,
  readReleaseInputs,
  readTrustedPublicKey,
  releaseId,
  removeStagingRelease,
  validateReleaseBundle,
  validateReleaseManifest,
  verifyInstalledRelease,
} from "../src/release.mjs";
import { saveBridgeState } from "../src/state.mjs";

const TOOL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PAPER_ID = "77777777-7777-4777-8777-777777777777";
const BINDING_ID = "22222222-2222-4222-8222-222222222222";
const PLAYER_ID = "11111111-1111-4111-8111-111111111111";
const DIGEST_A = `sha256:${"a".repeat(64)}`;
const DIGEST_B = `sha256:${"b".repeat(64)}`;
const PRACTICE_PENDING_TOKEN = DIGEST_A;
const PRACTICE_CLAIMED_TOKEN = DIGEST_B;
const PRACTICE_COMPLETED_TOKEN = `sha256:${"c".repeat(64)}`;

function installedPracticeTask(state) {
  const byState = {
    pending: { version: 3, taskToken: PRACTICE_PENDING_TOKEN, resultCode: null },
    claimed: { version: 4, taskToken: PRACTICE_CLAIMED_TOKEN, resultCode: null },
    completed: {
      version: 5,
      taskToken: PRACTICE_COMPLETED_TOKEN,
      resultCode: "concern_confirmed",
    },
  };
  const exact = byState[state];
  assert.ok(exact, `unsupported installed practice fixture state ${state}`);
  return {
    schema: "hepta.paper_raid.agent_bridge.practice_tasks.v1",
    mode: "practice_unranked",
    status: "ready",
    task: {
      schema: "hepta.paper_raid.agent_bridge.practice_task.v1",
      kind: "evidence_audit_intro",
      state,
      version: exact.version,
      task_token: exact.taskToken,
      expires_at: "2027-01-15T08:00:00Z",
      materials: {
        schema: "hepta.paper_raid.agent_bridge.practice_materials.v1",
        claim: "The candidate result remains supported after the evidence audit.",
        baseline: "Compare the stated claim with the supplied observation summary.",
        observations: [
          "The cited observation and the claimed scope do not fully align.",
          "Run the bounded practice check and report only one allowed result code.",
        ],
      },
      allowed_result_codes: [
        "concern_confirmed",
        "concern_not_detected",
        "inconclusive",
      ],
      result_code: exact.resultCode,
    },
  };
}

async function makeTreeRemovable(path) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }
  if (!stats.isDirectory() || stats.isSymbolicLink()) return;
  await chmod(path, 0o700);
  for (const entry of await readdir(path, { withFileTypes: true })) {
    if (entry.isDirectory() && !entry.isSymbolicLink()) {
      await makeTreeRemovable(join(path, entry.name));
    }
  }
}

async function temporary(t, label) {
  const root = await mkdtemp(join(tmpdir(), `paper-raid-lifecycle-${label}-`));
  t.after(async () => {
    await makeTreeRemovable(root);
    await rm(root, { recursive: true, force: true });
  });
  return root;
}

async function writeSecure(path, bytes, mode = 0o600) {
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  await writeFile(path, bytes, { mode });
  await chmod(path, mode);
  return path;
}

function signer() {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const spki = publicKey.export({ format: "der", type: "spki" });
  return Object.freeze({
    privateKey,
    spki,
    fingerprint: `sha256:${createHash("sha256").update(spki).digest("hex")}`,
  });
}

async function releaseFixture(directory, trust, version, sequence) {
  const built = await buildDeterministicRelease({
    sourceRoot: TOOL_ROOT,
    version,
    sequence,
  });
  const prefix = `${String(sequence).padStart(4, "0")}-${version}`;
  const packagePath = await writeSecure(
    join(directory, `${prefix}.bundle.json`),
    built.packageBytes,
  );
  const manifestPath = await writeSecure(
    join(directory, `${prefix}.manifest.json`),
    built.manifestBytes,
  );
  const signaturePath = await writeSecure(
    join(directory, `${prefix}.manifest.sig`),
    ed25519Sign(null, built.manifestBytes, trust.privateKey),
  );
  const trustedKeyPath = join(directory, "release-key.spki");
  try {
    await lstat(trustedKeyPath);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
    await writeSecure(trustedKeyPath, trust.spki);
  }
  return Object.freeze({
    ...built,
    packagePath,
    manifestPath,
    signaturePath,
    trustedKeyPath,
    fingerprint: trust.fingerprint,
    releaseId: releaseId(built.manifest),
  });
}

async function writeMockSystemctl(directory, behavior = "ok") {
  const path = join(directory, `mock-systemctl-${Math.random().toString(16).slice(2)}`);
  const script = `#!/bin/sh
mode_file="$0.mode"
log_file="$0.log"
printf '%s\\n' "$*" >> "$log_file"
mode="$(cat "$mode_file" 2>/dev/null || printf ok)"
command="$2"
case "$mode" in
  "fail:$command") exit 1 ;;
  "fail-once:$command") printf '%s\\n' ok > "$mode_file"; exit 1 ;;
  "hang:$command") exec sleep 60 ;;
  "inactive:$command") exit 3 ;;
esac
exit 0
`;
  await writeSecure(path, script, 0o700);
  await writeSecure(`${path}.mode`, `${behavior}\n`, 0o600);
  return path;
}

async function setMockBehavior(path, behavior) {
  await writeFile(`${path}.mode`, `${behavior}\n`);
  await chmod(`${path}.mode`, 0o600);
}

function installOptions(root, systemctlPath, artifact, overrides = {}) {
  return {
    root,
    systemctlPath,
    packagePath: artifact.packagePath,
    manifestPath: artifact.manifestPath,
    signaturePath: artifact.signaturePath,
    trustedKeyPath: artifact.trustedKeyPath,
    fingerprint: artifact.fingerprint,
    bffUrl: "http://127.0.0.1:7020",
    agentId: "agent.lifecycle.hostile",
    authorExecutor: systemctlPath,
    capabilities: "artifact_analysis,evidence_search,section_drafting",
    resourceClasses: "artifact_io,cpu,sandbox",
    mode: "confirm",
    ...overrides,
  };
}

function updateOptions(root, systemctlPath, artifact, overrides = {}) {
  return {
    root,
    systemctlPath,
    packagePath: artifact.packagePath,
    manifestPath: artifact.manifestPath,
    signaturePath: artifact.signaturePath,
    trustedKeyPath: artifact.trustedKeyPath,
    fingerprint: artifact.fingerprint,
    authorExecutor: systemctlPath,
    ...overrides,
  };
}

async function pointerIdentity(root, name) {
  const target = await readlink(join(root, name));
  assert.match(target, /^releases\//);
  return target.slice("releases/".length);
}

async function absent(path) {
  await assert.rejects(lstat(path), error => error?.code === "ENOENT");
}

function assertSignedBridgePost(url, init, identity, expectedPath) {
  const parsed = new URL(url);
  assert.equal(parsed.pathname, expectedPath);
  assert.equal(parsed.search, "");
  assert.equal(init.method, "POST");
  assert.equal(init.redirect, "error");
  const headers = new Headers(init.headers);
  assert.equal(headers.get("accept"), "application/json");
  assert.equal(headers.get("content-type"), "application/json");
  assert.equal(headers.has("authorization"), false);
  const body = JSON.parse(init.body);
  assert.equal(
    Buffer.from(init.body).equals(canonicalJsonBytes(body)),
    true,
    "installed Bridge must sign and send one canonical JSON body",
  );
  const claim = Object.freeze({
    schema: headers.get("x-paper-raid-agent-schema"),
    binding_id: headers.get("x-paper-raid-agent-binding-id"),
    agent_id: headers.get("x-paper-raid-agent-id"),
    agent_key_id: headers.get("x-paper-raid-agent-key-id"),
    http_method: "POST",
    canonical_path: expectedPath,
    canonical_query: "",
    body_hash: headers.get("x-paper-raid-agent-body-sha256"),
    nonce: headers.get("x-paper-raid-agent-nonce"),
    issued_at_unix: Number(headers.get("x-paper-raid-agent-issued-at")),
    expires_at_unix: Number(headers.get("x-paper-raid-agent-expires-at")),
  });
  assert.equal(claim.schema, AGENT_BRIDGE_REQUEST_PROOF_SCHEMA);
  assert.equal(claim.binding_id, BINDING_ID);
  assert.equal(claim.agent_id, identity.agent_id);
  assert.equal(claim.agent_key_id, identity.agent_key_id);
  assert.equal(claim.body_hash, sha256Digest(Buffer.from(init.body)));
  assert.equal(claim.expires_at_unix, claim.issued_at_unix + 60);
  assert.equal(
    identity.verify(
      agentBridgeRequestProofFrame(claim),
      headers.get("x-paper-raid-agent-signature"),
    ),
    true,
    "installed Bridge request proof must verify against the installed identity",
  );
  return body;
}

test("release verification pins detached Ed25519 bytes and rejects canonical-form attacks", async t => {
  const directory = await temporary(t, "release-inputs");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.0.0", 7);
  const rebuilt = await buildDeterministicRelease({
    sourceRoot: TOOL_ROOT,
    version: "3.0.0",
    sequence: 7,
  });
  assert.equal(rebuilt.packageBytes.equals(artifact.packageBytes), true);
  assert.equal(rebuilt.manifestBytes.equals(artifact.manifestBytes), true);
  assert.equal(rebuilt.manifest.package_sha256, artifact.manifest.package_sha256);
  assert.deepEqual(
    artifact.manifest.files
      .filter(entry => entry.path === "src/practice.mjs")
      .map(entry => entry.path),
    ["src/practice.mjs"],
  );
  assert.deepEqual(
    artifact.bundle.files
      .filter(entry => entry.path === "src/practice.mjs")
      .map(entry => entry.path),
    ["src/practice.mjs"],
  );
  assert.equal(
    JSON.parse(artifact.packageBytes).files.some(
      entry => entry.path === "src/practice.mjs" && entry.data_base64.length > 0,
    ),
    true,
  );
  const verified = await readReleaseInputs(artifact);
  assert.equal(verified.releaseId, artifact.releaseId);
  assert.equal(verified.trusted.fingerprint, trust.fingerprint);

  const realGetuid = process.getuid;
  try {
    process.getuid = () => realGetuid() + 1;
    await assert.rejects(
      readTrustedPublicKey(artifact.trustedKeyPath, trust.fingerprint),
      /owned by the installing user/,
    );
  } finally {
    process.getuid = realGetuid;
  }

  const heldKeyPath = join(directory, "release-key.held.spki");
  const replacementKeyPath = join(directory, "release-key.replacement.spki");
  const replacementTrust = signer();
  await writeSecure(replacementKeyPath, replacementTrust.spki);
  let heldLinked = false;
  let replacementPublished = false;
  let pathRaceError = null;
  try {
    await readTrustedPublicKey(
      artifact.trustedKeyPath,
      trust.fingerprint,
      {
        testHooks: {
          async afterOpen() {
            await link(artifact.trustedKeyPath, heldKeyPath);
            heldLinked = true;
            await rename(replacementKeyPath, artifact.trustedKeyPath);
            replacementPublished = true;
          },
        },
      },
    );
  } catch (error) {
    pathRaceError = error;
  }
  const pathRaceCleanupErrors = [];
  if (replacementPublished) {
    try {
      await unlink(artifact.trustedKeyPath);
    } catch (error) {
      if (error?.code !== "ENOENT") pathRaceCleanupErrors.push(error);
    }
    try {
      await rename(heldKeyPath, artifact.trustedKeyPath);
    } catch (error) {
      pathRaceCleanupErrors.push(error);
    }
  } else if (heldLinked) {
    try {
      await unlink(heldKeyPath);
    } catch (error) {
      pathRaceCleanupErrors.push(error);
    }
  }
  if (pathRaceCleanupErrors.length !== 0) {
    throw new AggregateError(
      [...(pathRaceError ? [pathRaceError] : []), ...pathRaceCleanupErrors],
      "path-replacement hostile probe and restoration failed",
    );
  }
  assert(pathRaceError instanceof Error);
  assert.match(pathRaceError.message, /changed while being opened/);
  assert.equal((await readFile(artifact.trustedKeyPath)).equals(trust.spki), true);

  await assert.rejects(
    readTrustedPublicKey(artifact.trustedKeyPath, trust.fingerprint, {
      testHooks: {
        async afterRead() {
          await writeFile(artifact.trustedKeyPath, trust.spki);
          await chmod(artifact.trustedKeyPath, 0o600);
        },
      },
    }),
    /changed while being read/,
  );

  await assert.rejects(
    readReleaseInputs({ ...artifact, fingerprint: `sha256:${"0".repeat(64)}` }),
    /fingerprint pin/,
  );
  const wrongSignaturePath = await writeSecure(
    join(directory, "wrong.sig"),
    Buffer.alloc(64, 0x5a),
  );
  await assert.rejects(
    readReleaseInputs({ ...artifact, signaturePath: wrongSignaturePath }),
    /signature is invalid/,
  );

  const prettyManifest = Buffer.from(
    `${JSON.stringify(artifact.manifest, null, 2)}\n`,
    "utf8",
  );
  const prettyPath = await writeSecure(join(directory, "pretty.manifest.json"), prettyManifest);
  const prettySignaturePath = await writeSecure(
    join(directory, "pretty.manifest.sig"),
    ed25519Sign(null, prettyManifest, trust.privateKey),
  );
  await assert.rejects(
    readReleaseInputs({
      ...artifact,
      manifestPath: prettyPath,
      signaturePath: prettySignaturePath,
    }),
    /exact canonical document/,
  );

  const duplicateManifest = Buffer.from(
    artifact.manifestBytes
      .toString("utf8")
      .replace("{", `{"schema":"${artifact.manifest.schema}",`),
    "utf8",
  );
  const duplicatePath = await writeSecure(
    join(directory, "duplicate.manifest.json"),
    duplicateManifest,
  );
  const duplicateSignaturePath = await writeSecure(
    join(directory, "duplicate.manifest.sig"),
    ed25519Sign(null, duplicateManifest, trust.privateKey),
  );
  await assert.rejects(
    readReleaseInputs({
      ...artifact,
      manifestPath: duplicatePath,
      signaturePath: duplicateSignaturePath,
    }),
    /exact canonical document/,
  );

  const unknown = structuredClone(artifact.manifest);
  unknown.unknown = true;
  assert.throws(() => validateReleaseManifest(unknown), /unsupported or missing fields/);
  const missing = structuredClone(artifact.manifest);
  delete missing.node_minimum;
  assert.throws(() => validateReleaseManifest(missing), /unsupported or missing fields/);
  const traversal = structuredClone(artifact.manifest);
  traversal.files[0].path = "../package.json";
  assert.throws(() => validateReleaseManifest(traversal), /safe relative runtime path/);
  const duplicateFile = structuredClone(artifact.manifest);
  duplicateFile.files[1] = structuredClone(duplicateFile.files[0]);
  assert.throws(() => validateReleaseManifest(duplicateFile), /strictly sorted and unique|extra, missing, or reordered/);
  const unknownBundle = structuredClone(artifact.bundle);
  unknownBundle.command = "/bin/sh";
  assert.throws(() => validateReleaseBundle(unknownBundle), /unsupported or missing fields/);

  const packageLink = join(directory, "bundle.symlink");
  await symlink(artifact.packagePath, packageLink);
  await assert.rejects(
    readReleaseInputs({ ...artifact, packagePath: packageLink }),
    /regular non-linked file/,
  );
  const packageHardlink = join(directory, "bundle.hardlink");
  await link(artifact.packagePath, packageHardlink);
  await assert.rejects(readReleaseInputs(artifact), /regular non-linked file/);
  await unlink(packageHardlink);

  await chmod(artifact.manifestPath, 0o666);
  await assert.rejects(readReleaseInputs(artifact), /unsafe permissions/);
  await chmod(artifact.manifestPath, 0o600);

  const source = await readFile(resolve(TOOL_ROOT, "src", "release.mjs"), "utf8");
  assert.match(source, /before\.uid !== BigInt\(process\.getuid\(\)\)/);
  assert.match(source, /final\.mtimeNs !== after\.mtimeNs/);
  assert.match(source, /O_NOFOLLOW/);
});

test("publication is no-replace under concurrency and never follows hostile final paths", async t => {
  const directory = await temporary(t, "publish");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.1.0", 8);
  const verified = await readReleaseInputs(artifact);
  const releases = join(directory, "releases");
  await mkdir(releases, { mode: 0o700 });
  const stagingA = join(releases, ".staging-a");
  const stagingB = join(releases, ".staging-b");
  await extractVerifiedRelease(verified, stagingA);
  await extractVerifiedRelease(verified, stagingB);
  const final = join(releases, artifact.releaseId);
  const raced = await Promise.allSettled([
    publishStagedRelease(stagingA, final),
    publishStagedRelease(stagingB, final),
  ]);
  assert.equal(
    raced.filter(result => result.status === "fulfilled").length,
    1,
    raced.map(result => result.status === "rejected" ? result.reason?.stack : "fulfilled").join("\n"),
  );
  assert.equal(raced.filter(result => result.status === "rejected").length, 1);
  await verifyInstalledRelease({
    releasePath: final,
    trusted: verified.trusted,
    expectedReleaseId: artifact.releaseId,
  });
  const installedPackage = join(final, "package.json");
  await chmod(installedPackage, 0o600);
  await assert.rejects(verifyInstalledRelease({
    releasePath: final,
    trusted: verified.trusted,
    expectedReleaseId: artifact.releaseId,
  }), /tampered/);
  await chmod(installedPackage, 0o400);
  const installedHardlink = join(directory, "installed-package-hardlink");
  await link(installedPackage, installedHardlink);
  await assert.rejects(verifyInstalledRelease({
    releasePath: final,
    trusted: verified.trusted,
    expectedReleaseId: artifact.releaseId,
  }), /hard-linked file/);
  await unlink(installedHardlink);
  for (const staging of [stagingA, stagingB]) {
    await removeStagingRelease(staging).catch(() => {});
  }

  const stagingExisting = join(releases, ".staging-existing");
  await extractVerifiedRelease(verified, stagingExisting);
  const existing = join(releases, "000000000099-9.9.9");
  await mkdir(existing, { mode: 0o700 });
  await writeSecure(join(existing, "sentinel"), "do-not-overwrite\n");
  await assert.rejects(publishStagedRelease(stagingExisting, existing), /EEXIST/);
  assert.equal(await readFile(join(existing, "sentinel"), "utf8"), "do-not-overwrite\n");
  await removeStagingRelease(stagingExisting);

  const stagingSymlink = join(releases, ".staging-symlink");
  await extractVerifiedRelease(verified, stagingSymlink);
  const target = join(directory, "outside-target");
  await mkdir(target, { mode: 0o700 });
  const symlinkFinal = join(releases, "000000000100-9.9.10");
  await symlink(target, symlinkFinal, "dir");
  await assert.rejects(publishStagedRelease(stagingSymlink, symlinkFinal), /EEXIST/);
  assert.deepEqual(await readdir(target), []);
  await removeStagingRelease(stagingSymlink);

  const stagingHardlink = join(releases, ".staging-hardlink");
  await extractVerifiedRelease(verified, stagingHardlink);
  const outsideLink = join(directory, "outside-hardlink");
  await link(join(stagingHardlink, "package.json"), outsideLink);
  await assert.rejects(
    publishStagedRelease(stagingHardlink, join(releases, "000000000101-9.9.11")),
    /hard-linked file/,
  );
  await unlink(outsideLink);
  await removeStagingRelease(stagingHardlink);
});

test("failed install restores old config/service inodes and cleans a fresh root", async t => {
  const directory = await temporary(t, "install-transaction");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.2.0", 10);
  const mock = await writeMockSystemctl(directory, "fail:enable");
  const root = join(directory, "preserved-root");
  const data = join(root, "data");
  await mkdir(data, { recursive: true, mode: 0o700 });
  const oldConfig = await writeSecure(join(data, "bridge.config.json"), "old-config\n");
  const oldService = await writeSecure(join(data, "service.json"), "old-service\n");
  const oldConfigStat = await lstat(oldConfig);
  const oldServiceStat = await lstat(oldService);
  await assert.rejects(installProduct(installOptions(root, mock, artifact)), /failed closed/);
  assert.equal(await readFile(oldConfig, "utf8"), "old-config\n");
  assert.equal(await readFile(oldService, "utf8"), "old-service\n");
  assert.equal((await lstat(oldConfig)).ino, oldConfigStat.ino);
  assert.equal((await lstat(oldService)).ino, oldServiceStat.ino);
  assert.deepEqual((await readdir(root)).sort(), ["data"]);
  assert.deepEqual((await readdir(data)).sort(), ["bridge.config.json", "service.json"]);

  const wideRoot = join(directory, "wide-root");
  await mkdir(wideRoot, { mode: 0o700 });
  await chmod(wideRoot, 0o755);
  await assert.rejects(
    installProduct(installOptions(wideRoot, mock, artifact)),
    /safe user-owned directory/,
  );

  const timeoutMock = await writeMockSystemctl(directory, "hang:daemon-reload");
  const freshRoot = join(directory, "fresh-root");
  await assert.rejects(
    installProduct(installOptions(freshRoot, timeoutMock, artifact, {
      systemctlTimeoutMs: 50,
    })),
    /timed out/,
  );
  await absent(freshRoot);
});

test("install and update validate and canonicalize the Author executor before mutation", async t => {
  const directory = await temporary(t, "author-executor-lifecycle");
  const trust = signer();
  const first = await releaseFixture(directory, trust, "3.2.1", 12);
  const second = await releaseFixture(directory, trust, "3.2.2", 13);
  const mock = await writeMockSystemctl(directory);
  const validDirectory = join(directory, "canonical-executor");
  await mkdir(validDirectory, { mode: 0o700 });
  const validExecutor = await writeSecure(
    join(validDirectory, "author-executor"),
    "#!/bin/sh\nexit 0\n",
    0o700,
  );
  const aliasDirectory = join(directory, "executor-alias");
  await symlink(validDirectory, aliasDirectory, "dir");
  const aliasedExecutor = join(aliasDirectory, "author-executor");

  const symlinkExecutor = join(directory, "author-executor.symlink");
  await symlink(validExecutor, symlinkExecutor);
  const hardlinkSource = await writeSecure(
    join(directory, "author-executor.hard-source"),
    "#!/bin/sh\nexit 0\n",
    0o700,
  );
  const hardlinkExecutor = join(directory, "author-executor.hardlink");
  await link(hardlinkSource, hardlinkExecutor);
  const writableExecutor = await writeSecure(
    join(directory, "author-executor.group-writable"),
    "#!/bin/sh\nexit 0\n",
    0o720,
  );
  const nonExecutable = await writeSecure(
    join(directory, "author-executor.not-executable"),
    "exit 0\n",
    0o600,
  );
  const invalidExecutors = [
    ["missing", join(directory, "author-executor.missing")],
    ["symlink", symlinkExecutor],
    ["hardlink", hardlinkExecutor],
    ["group-writable", writableExecutor],
    ["not-executable", nonExecutable],
  ];
  let wrongOwnerExecutor = "/usr/bin/true";
  if (typeof process.getuid === "function" && process.getuid() === 0) {
    wrongOwnerExecutor = await writeSecure(
      join(directory, "author-executor.wrong-owner"),
      "#!/bin/sh\nexit 0\n",
      0o700,
    );
    await chown(wrongOwnerExecutor, 65534, 65534);
  }
  invalidExecutors.push(["wrong-owner", wrongOwnerExecutor]);

  for (const [name, executor] of invalidExecutors) {
    const root = join(directory, `install-${name}`);
    await assert.rejects(
      installProduct(installOptions(root, mock, first, { authorExecutor: executor })),
      /Author executor/,
    );
    await absent(root);
  }

  const root = join(directory, "installed");
  await installProduct(installOptions(root, mock, first, {
    authorExecutor: aliasedExecutor,
  }));
  const configPath = join(root, "data", "bridge.config.json");
  const installedConfig = await loadConfig(configPath);
  assert.equal(installedConfig.author_executor.executable, await realpath(validExecutor));
  const beforeConfig = await readFile(configPath);
  const beforeConfigStat = await lstat(configPath);

  for (const [, executor] of invalidExecutors) {
    await assert.rejects(
      updateProduct(updateOptions(root, mock, second, { authorExecutor: executor })),
      /Author executor/,
    );
    assert.equal(await pointerIdentity(root, "current"), first.releaseId);
    assert.deepEqual(await readdir(join(root, "releases")), [first.releaseId]);
    assert.deepEqual(await readFile(configPath), beforeConfig);
    assert.equal((await lstat(configPath)).ino, beforeConfigStat.ino);
  }

  await updateProduct(updateOptions(root, mock, second, {
    authorExecutor: aliasedExecutor,
  }));
  assert.equal(await pointerIdentity(root, "current"), second.releaseId);
  assert.equal(
    (await loadConfig(configPath)).author_executor.executable,
    await realpath(validExecutor),
  );
});

test("install, paired diagnose, preserve uninstall, and isolated purge form one player runbook", async t => {
  const directory = await temporary(t, "runbook");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.3.0", 11);
  const mock = await writeMockSystemctl(directory);
  const root = join(directory, "bridge-root");
  assert.deepEqual(await installProduct(installOptions(root, mock, artifact)), {
    status: "installed",
    mode: "confirm",
    version: "3.3.0",
  });
  assert.equal(await pointerIdentity(root, "current"), artifact.releaseId);
  assert.equal((await diagnoseProduct({ root, systemctlPath: mock })).status, "needs_attention");

  const config = await loadConfig(join(root, "data", "bridge.config.json"));
  const identity = await loadIdentity(config.identity_file);
  await writePrivateJsonReplacing(config.state_file, { agent_id: identity.agent_id });
  assert.equal((await diagnoseProduct({ root, systemctlPath: mock })).pairing, "not_paired");
  await unlink(config.state_file);
  const disclosure = config.capability_disclosure;
  await saveBridgeState(config.state_file, identity, {
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    agent_public_key: identity.agent_public_key,
    agent_public_key_hash: identity.agent_public_key_hash,
    capability_disclosure: disclosure,
    capability_disclosure_hash: agentCapabilityDisclosureHash(disclosure),
    status: "active",
    version: 1,
    created_at: "2026-08-14T00:00:00Z",
    updated_at: "2026-08-14T00:00:00Z",
  }, 1_800_000_000);
  const ready = await diagnoseProduct({ root, systemctlPath: mock });
  assert.equal(ready.status, "ready");
  assert.equal(ready.pairing, "paired");

  const installationPath = join(root, "installation.json");
  const installation = JSON.parse(await readFile(installationPath, "utf8"));
  await writePrivateJsonReplacing(installationPath, {
    ...installation,
    root: join(directory, "redirected-root"),
  });
  const redirected = await diagnoseProduct({ root, systemctlPath: mock });
  assert.equal(redirected.status, "needs_attention");
  assert.equal(redirected.release, "unverified");
  await writePrivateJsonReplacing(installationPath, installation);

  assert.deepEqual(await uninstallProduct({ root, systemctlPath: mock }), {
    status: "uninstalled",
    data: "preserved",
  });
  assert.deepEqual(await readdir(root), ["data"]);
  assert.ok((await readdir(join(root, "data"))).includes("identity.json"));

  await installProduct(installOptions(root, mock, artifact));
  assert.deepEqual(await uninstallProduct({
    root,
    systemctlPath: mock,
    purgeData: true,
  }), {
    status: "uninstalled",
    data: "purged",
  });
  await absent(root);

  await assert.rejects(
    cliMain(["diagnose", "--systemctl", mock]),
    /--root and --systemctl are a test-harness pair/,
  );
  await assert.rejects(
    cliMain(["uninstall", "--purge-data", "--systemctl", mock]),
    /--root and --systemctl are a test-harness pair/,
  );
  const lifecycleSource = await readFile(resolve(TOOL_ROOT, "src", "lifecycle.mjs"), "utf8");
  assert.match(
    lifecycleSource,
    /custom systemctl executable is allowed only with an explicit test root/,
  );
  assert.match(
    lifecycleSource,
    /options\.purgeData === true && !installation\.layout\.testHarness/,
  );
});

test("installed release resolves practice CLI and daemon completes signed practice without replacing inbox behavior", async t => {
  const directory = await temporary(t, "installed-practice");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.3.1", 14);
  const mock = await writeMockSystemctl(directory);
  const root = join(directory, "bridge-root");
  await installProduct(installOptions(root, mock, artifact));

  const configPath = join(root, "data", "bridge.config.json");
  const config = await loadConfig(configPath);
  const identity = await loadIdentity(config.identity_file);
  const disclosure = config.capability_disclosure;
  await saveBridgeState(config.state_file, identity, {
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    agent_public_key: identity.agent_public_key,
    agent_public_key_hash: identity.agent_public_key_hash,
    capability_disclosure: disclosure,
    capability_disclosure_hash: agentCapabilityDisclosureHash(disclosure),
    status: "active",
    version: 1,
    created_at: "2026-08-14T00:00:00Z",
    updated_at: "2026-08-14T00:00:00Z",
  }, 1_800_000_000);

  const installedRelease = join(
    root,
    "releases",
    await pointerIdentity(root, "current"),
  );
  assert.equal(
    (await lstat(join(installedRelease, "src", "practice.mjs"))).mode & 0o777,
    0o400,
  );
  const installedCli = await import(
    pathToFileURL(join(installedRelease, "src", "cli.mjs")).href
  );
  const installedDaemon = await import(
    pathToFileURL(join(installedRelease, "src", "daemon.mjs")).href
  );

  const originalFetch = globalThis.fetch;
  const originalWrite = process.stdout.write;
  let phase = "cli";
  let practiceState = "pending";
  const routes = [];
  let output = "";
  const jsonResponse = value => new Response(JSON.stringify(value), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
  globalThis.fetch = async (url, init = {}) => {
    const parsed = new URL(url);
    assert.equal(parsed.origin, "http://127.0.0.1:7020");
    const path = parsed.pathname;
    const body = assertSignedBridgePost(url, init, identity, path);
    routes.push(path);
    if (path === "/api/agent-bridge/practice-tasks") {
      assert.deepEqual(body, {
        schema: "hepta.paper_raid.agent_bridge.practice_task_query.v1",
      });
      if (phase === "cli") {
        return jsonResponse({
          schema: "hepta.paper_raid.agent_bridge.practice_tasks.v1",
          mode: "practice_unranked",
          status: "absent",
          task: null,
        });
      }
      return jsonResponse(installedPracticeTask(practiceState));
    }
    if (path === "/api/agent-bridge/practice-claims") {
      assert.equal(practiceState, "pending");
      assert.deepEqual(body, {
        schema: "hepta.paper_raid.agent_bridge.practice_claim_request.v1",
        task_token: PRACTICE_PENDING_TOKEN,
        expected_version: 3,
      });
      practiceState = "claimed";
      return jsonResponse({
        schema: "hepta.paper_raid.agent_bridge.practice_transition_result.v1",
        operation: "claim",
        status: "claimed",
        from_version: 3,
        version: 4,
      });
    }
    if (path === "/api/agent-bridge/practice-results") {
      assert.equal(practiceState, "claimed");
      assert.deepEqual(body, {
        schema: "hepta.paper_raid.agent_bridge.practice_result_request.v1",
        task_token: PRACTICE_CLAIMED_TOKEN,
        expected_version: 4,
        result_code: "concern_confirmed",
      });
      practiceState = "completed";
      return jsonResponse({
        schema: "hepta.paper_raid.agent_bridge.practice_transition_result.v1",
        operation: "result",
        status: "completed",
        from_version: 4,
        version: 5,
        result_code: "concern_confirmed",
      });
    }
    if (path === "/api/agent-bridge/health") {
      assert.equal(body.schema, "hepta.paper_raid.agent_bridge.health_report.v1");
      assert.equal(body.assurance, "self_declared_unverified");
      assert.equal(body.status, "healthy");
      assert.equal(Number.isSafeInteger(body.observed_at_unix), true);
      return jsonResponse({ accepted: true });
    }
    if (path === "/api/agent-bridge/inbox") {
      assert.deepEqual(body, {
        schema: "hepta.paper_raid.agent_bridge.inbox_request.v1",
        paper_ids: [],
      });
      return jsonResponse(reviewOnlyInbox());
    }
    throw new Error(`unexpected installed practice route ${path}`);
  };
  process.stdout.write = chunk => {
    output += String(chunk);
    return true;
  };
  try {
    await installedCli.main(["practice", "--config", configPath]);
    assert.deepEqual(JSON.parse(output), {
      schema: "hepta.paper_raid.agent_bridge.practice_tasks.v1",
      mode: "practice_unranked",
      status: "absent",
      task: null,
    });
    phase = "daemon";
    const status = await installedDaemon.daemonCycle(root);
    assert.deepEqual(status, {
      schema: "hepta.paper_raid.agent_bridge.service_status.v1",
      mode: "confirm",
      state: "ready",
      candidate_count: 0,
      action: "none",
    });
  } finally {
    process.stdout.write = originalWrite;
    globalThis.fetch = originalFetch;
  }
  assert.equal(practiceState, "completed");
  assert.deepEqual(routes, [
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-claims",
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/practice-results",
    "/api/agent-bridge/practice-tasks",
    "/api/agent-bridge/health",
    "/api/agent-bridge/inbox",
  ]);
});

test("every update pointer phase restores current, previous, service, and new release", async t => {
  const directory = await temporary(t, "update-phases");
  const trust = signer();
  const first = await releaseFixture(directory, trust, "3.4.0", 20);
  const second = await releaseFixture(directory, trust, "3.5.0", 21);
  const phases = [
    "update_after_publish",
    "update_after_config",
    "update_after_previous",
    "update_after_current",
    "update_after_verify",
    "update_after_restart",
  ];
  for (const phase of phases) {
    const root = join(directory, phase);
    const mock = await writeMockSystemctl(directory);
    await installProduct(installOptions(root, mock, first));
    const oldConfig = await readFile(join(root, "data", "bridge.config.json"));
    const oldConfigIdentity = await lstat(join(root, "data", "bridge.config.json"));
    const alternateExecutor = await writeMockSystemctl(directory);
    await assert.rejects(updateProduct(updateOptions(root, mock, second, {
      authorExecutor: alternateExecutor,
      testHooks: {
        phase(current) {
          if (current === phase) throw new Error(`injected ${phase}`);
        },
      },
    })), new RegExp(`injected ${phase}`));
    assert.equal(await pointerIdentity(root, "current"), first.releaseId);
    await absent(join(root, "previous"));
    assert.deepEqual(await readdir(join(root, "releases")), [first.releaseId]);
    assert.deepEqual(await readFile(join(root, "data", "bridge.config.json")), oldConfig);
    assert.equal(
      (await lstat(join(root, "data", "bridge.config.json"))).ino,
      oldConfigIdentity.ino,
    );
  }
});

test("rollback phases restore the pre-rollback pointers and update enforces history anti-rollback", async t => {
  const directory = await temporary(t, "rollback-phases");
  const trust = signer();
  const first = await releaseFixture(directory, trust, "3.6.0", 30);
  const sameSequence = await releaseFixture(directory, trust, "3.6.1", 31);
  const second = await releaseFixture(directory, trust, "3.7.0", 31);
  const third = await releaseFixture(directory, trust, "3.8.0", 32);
  const phases = [
    "rollback_after_previous",
    "rollback_after_current",
    "rollback_after_verify",
    "rollback_after_restart",
  ];
  for (const phase of phases) {
    const root = join(directory, phase);
    const mock = await writeMockSystemctl(directory);
    await installProduct(installOptions(root, mock, first));
    await updateProduct(updateOptions(root, mock, second));
    await assert.rejects(rollbackProduct({
      root,
      systemctlPath: mock,
      signaturePath: first.signaturePath,
      trustedKeyPath: first.trustedKeyPath,
      fingerprint: first.fingerprint,
      testHooks: {
        phase(current) {
          if (current === phase) throw new Error(`injected ${phase}`);
        },
      },
    }), new RegExp(`injected ${phase}`));
    assert.equal(await pointerIdentity(root, "current"), second.releaseId);
    assert.equal(await pointerIdentity(root, "previous"), first.releaseId);
  }

  const root = join(directory, "anti-rollback");
  const mock = await writeMockSystemctl(directory);
  await installProduct(installOptions(root, mock, first));
  await updateProduct(updateOptions(root, mock, second));
  await assert.rejects(updateProduct(updateOptions(root, mock, first)), /known-history downgrade/);
  await assert.rejects(
    updateProduct(updateOptions(root, mock, sameSequence)),
    /same-sequence|known-history downgrade/,
  );
  assert.equal((await rollbackProduct({
    root,
    systemctlPath: mock,
    signaturePath: first.signaturePath,
    trustedKeyPath: first.trustedKeyPath,
    fingerprint: first.fingerprint,
  })).status, "rolled_back");
  assert.equal(await pointerIdentity(root, "current"), first.releaseId);
  assert.equal(await pointerIdentity(root, "previous"), second.releaseId);
  await assert.rejects(
    updateProduct(updateOptions(root, mock, sameSequence)),
    /known-history downgrade/,
  );
  await updateProduct(updateOptions(root, mock, third));
  assert.equal(await pointerIdentity(root, "current"), third.releaseId);
  assert.equal(await pointerIdentity(root, "previous"), first.releaseId);
  assert.deepEqual(
    (await readdir(join(root, "releases"))).sort(),
    [first.releaseId, third.releaseId].sort(),
  );
});

test("mode restart failure restores the exact prior file and Confirm/Auto boundary", async t => {
  const directory = await temporary(t, "mode");
  const trust = signer();
  const artifact = await releaseFixture(directory, trust, "3.9.0", 40);
  const mock = await writeMockSystemctl(directory);
  const root = join(directory, "bridge-root");
  await installProduct(installOptions(root, mock, artifact));
  const servicePath = serviceConfigPath(root);
  const before = await lstat(servicePath);
  await setMockBehavior(mock, "fail-once:restart");
  await assert.rejects(setServiceMode({
    root,
    systemctlPath: mock,
    mode: "auto",
    autoAcknowledged: true,
  }), /failed closed/);
  assert.equal((await lstat(servicePath)).ino, before.ino);
  assert.deepEqual(JSON.parse(await readFile(servicePath, "utf8")), {
    schema: SERVICE_CONFIG_SCHEMA,
    mode: "confirm",
  });
  assert.equal(await installedConfirmConfigPath(root), join(root, "data", "bridge.config.json"));
  await setServiceMode({
    root,
    systemctlPath: mock,
    mode: "auto",
    autoAcknowledged: true,
  });
  await assert.rejects(installedConfirmConfigPath(root), /manual Confirm is disabled/);
  await assert.rejects(setServiceMode({
    root,
    systemctlPath: mock,
    mode: "auto",
    autoAcknowledged: false,
  }), /explicit --acknowledge-auto/);
});

function deliveryCandidate(index = 0) {
  const digit = String(index + 1);
  const uuid = prefix => `${prefix}${digit.repeat(7)}-${digit.repeat(4)}-4${digit.repeat(3)}-8${digit.repeat(3)}-${digit.repeat(12)}`;
  return {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidate.v1",
    delivery_draft_id: uuid("a"),
    binding_id: BINDING_ID,
    paper_id: PAPER_ID,
    work_item_id: uuid("b"),
    section_key: `methods-${digit}`,
    lease_id: uuid("c"),
    lease_fencing_token: index + 1,
    expected_work_version: index + 1,
    parent_revision_id: uuid("d"),
    proposal_kind: "delivery",
    payload_hash: DIGEST_A,
    artifact_manifest_id: uuid("e"),
    artifact_manifest_hash: DIGEST_B,
    delivery_state: "pending",
    declared_at_unix: 1_800_000_000,
    expires_at_unix: 1_900_000_000,
  };
}

const daemonMaterialBytes = Object.freeze({
  brief: Buffer.from("# daemon brief\n"),
  dataset: Buffer.from('{"rows":[1]}'),
  baseline: Buffer.from("print('daemon baseline')\n"),
  evaluator: Buffer.from("print('daemon evaluator')\n"),
});

function canonicalHash(value, field) {
  const frame = { ...value };
  delete frame[field];
  return { ...value, [field]: sha256Digest(canonicalJsonBytes(frame)) };
}

function daemonMaterialBundle(candidate) {
  const authority = canonicalHash({
    schema: "hepta.paper_raid.frozen_challenge_material_authority.v1",
    authority_hash: DIGEST_A,
    activation_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    activation_request_sha256: `sha256:${"1".repeat(64)}`,
    challenge_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
    challenge_snapshot_hash: `sha256:${"2".repeat(64)}`,
    template: "evidence-audit",
    pack_id: "paper-raid-evidence-audit-seeded-v1",
    pack_manifest_hash: `sha256:${"3".repeat(64)}`,
    ruleset_version: "paper-raid-evidence-audit-v1",
    ruleset_hash: `sha256:${"4".repeat(64)}`,
    dataset_manifest_hash: `sha256:${"5".repeat(64)}`,
    evaluator_manifest_hash: `sha256:${"6".repeat(64)}`,
  }, "authority_hash");
  const objects = [
    ["brief", "playable_brief", "challenge/brief.md", "text/markdown; charset=utf-8"],
    ["dataset", "dataset", "challenge/dataset.json", "application/json"],
    ["baseline", "baseline_code", "challenge/baseline.py", "text/x-python; charset=utf-8"],
    ["evaluator", "frozen_evaluator", "challenge/evaluator.py", "text/x-python; charset=utf-8"],
  ].map(([key, role, logicalPath, mediaType]) => ({
    object_key: key,
    source_path: `source/${key}`,
    logical_path: logicalPath,
    role,
    digest: sha256Digest(daemonMaterialBytes[key]),
    size_bytes: daemonMaterialBytes[key].length,
    media_type: mediaType,
    download_path: "/api/agent-bridge/challenge-objects",
  }));
  return canonicalHash({
    schema: "hepta.paper_raid.assigned_challenge_material_bundle.v1",
    bundle_hash: DIGEST_B,
    authority,
    authority_hash: authority.authority_hash,
    paper_project_id: PAPER_ID,
    challenge_ruleset_snapshot_hash: `sha256:${"7".repeat(64)}`,
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    work_item_id: candidate.work_item_id,
    work_item_version: candidate.expected_work_version,
    objects,
  }, "bundle_hash");
}

function inbox(items, { drafts = true } = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "drafting",
      tasks: items.map(candidate => ({
        work_item_id: candidate.work_item_id,
        paper_project_id: PAPER_ID,
        kind: "assigned_research",
        assigned_binding_id: BINDING_ID,
        assigned_player_id: PLAYER_ID,
        status: "in_progress",
        version: candidate.expected_work_version,
      })),
      proposals: [],
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status: drafts ? "available" : "unavailable",
        reason_code: drafts ? null : "no_agent_declared_delivery_draft",
        items: drafts ? items : [],
      },
      challenge_materials: {
        schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
        status: "available",
        reason_code: null,
        items: items.map(daemonMaterialBundle),
      },
    }],
  };
}

function reviewOnlyInbox() {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      review_tasks: {
        schema: "hepta.paper_raid.agent_bridge.review_tasks.v1",
        status: "unavailable",
        reason_code: "no_executable_review_assignment",
        items: [],
      },
    }],
  };
}

async function daemonRoot(directory, name, mode) {
  const root = join(directory, name);
  const data = join(root, "data");
  await mkdir(data, { recursive: true, mode: 0o700 });
  const identityPath = join(data, "identity.json");
  const { generateIdentity } = await import("../src/identity.mjs");
  await generateIdentity(`agent.daemon.${name}`, identityPath);
  const identity = await loadIdentity(identityPath);
  const executorLog = join(data, "author-executor.log");
  const executorPath = await writeSecure(
    join(data, "author-executor.mjs"),
    `#!${process.execPath}
import { appendFile, readdir } from "node:fs/promises";
let raw = "";
for await (const chunk of process.stdin) raw += chunk;
const request = JSON.parse(raw);
if (process.argv[2] !== "author-work-v1" ||
    process.env.HEPTA_PAPER_RAID_MATERIAL_DIRECTORY !== request.material_directory ||
    process.cwd() !== request.material_directory) process.exit(41);
const files = (await readdir(new URL("challenge/", \`file://\${request.material_directory}/\`))).sort();
if (files.join(",") !== "baseline.py,brief.md,dataset.json,evaluator.py") process.exit(42);
await appendFile(${JSON.stringify(executorLog)}, \`\${request.work_item_id}\n\`, { mode: 0o600 });
const digit = String(request.work_item_version);
const uuid = prefix => \`\${prefix}\${digit.repeat(7)}-\${digit.repeat(4)}-4\${digit.repeat(3)}-8\${digit.repeat(3)}-\${digit.repeat(12)}\`;
process.stdout.write(JSON.stringify({
  schema: "hepta.paper_raid.agent_bridge.author_executor_result.v1",
  status: "completed",
  material_directory: request.material_directory,
  paper_id: request.paper_id,
  binding_id: request.binding_id,
  player_id: request.player_id,
  work_item_id: request.work_item_id,
  work_item_version: request.work_item_version,
  task_kind: request.task_kind,
  start_key: request.start_key,
  bundle_hash: request.bundle_hash,
  authority_hash: request.authority_hash,
  section_key: \`methods-\${digit}\`,
  artifact_manifest_id: uuid("e"),
  payload_hash: ${JSON.stringify(DIGEST_A)},
}));
`,
    0o700,
  );
  const disclosure = {
    schema: "hepta.paper_raid.agent_capability_disclosure.v1",
    assurance: "self_declared_unverified",
    capabilities: ["artifact_analysis", "evidence_search", "section_drafting"],
    resource_classes: ["artifact_io", "cpu", "sandbox"],
    max_parallel_tasks: 1,
  };
  await writeSecure(join(data, "bridge.config.json"), `${JSON.stringify({
    schema: "hepta.paper_raid.agent_bridge.config.v2",
    bff_url: "http://127.0.0.1:7020",
    identity_file: "identity.json",
    state_file: "state.json",
    author_executor: {
      schema: "hepta.paper_raid.agent_bridge.author_executor.v1",
      executable: executorPath,
      timeout_ms: 2_000,
    },
    capabilities: disclosure.capabilities,
    resource_classes: disclosure.resource_classes,
    max_parallel_tasks: disclosure.max_parallel_tasks,
    paper_ids: [PAPER_ID],
    poll_interval_ms: 500,
    request_timeout_ms: 2_000,
  }, null, 2)}\n`);
  await writeSecure(join(data, "service.json"), `${JSON.stringify({
    schema: SERVICE_CONFIG_SCHEMA,
    mode,
  }, null, 2)}\n`);
  await saveBridgeState(join(data, "state.json"), identity, {
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    agent_id: identity.agent_id,
    agent_key_id: identity.agent_key_id,
    agent_public_key: identity.agent_public_key,
    agent_public_key_hash: identity.agent_public_key_hash,
    capability_disclosure: disclosure,
    capability_disclosure_hash: agentCapabilityDisclosureHash(disclosure),
    status: "active",
    version: 1,
    created_at: "2026-08-14T00:00:00Z",
    updated_at: "2026-08-14T00:00:00Z",
  }, 1_800_000_000);
  return root;
}

async function authorExecutorCalls(root) {
  try {
    return (await readFile(join(root, "data", "author-executor.log"), "utf8"))
      .trim()
      .split("\n")
      .filter(Boolean);
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
}

test("daemon keeps Confirm inert, Auto exact-one, and concurrent Auto single-submit", async t => {
  const directory = await temporary(t, "daemon");
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });
  let activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  let proposals = 0;
  let deliveryDrafts = 0;
  let materialReads = 0;
  let failedMaterialKey = null;
  globalThis.fetch = async (url, init = {}) => {
    const parsed = new URL(url);
    const path = parsed.pathname;
    if (path === "/api/agent-bridge/practice-tasks") {
      return new Response(JSON.stringify({
        schema: "hepta.paper_raid.agent_bridge.practice_tasks.v1",
        mode: "practice_unranked",
        status: "absent",
        task: null,
      }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (path === "/api/agent-bridge/health") {
      return new Response(JSON.stringify({ accepted: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (path === "/api/agent-bridge/inbox") {
      return new Response(JSON.stringify(activeInbox), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (path === "/api/agent-bridge/delivery-drafts") {
      const body = JSON.parse(init.body);
      const task = activeInbox.papers[0].tasks.find(item =>
        item.work_item_id === body.work_item_id
      );
      assert.ok(task);
      const candidate = {
        ...deliveryCandidate(task.version - 1),
        delivery_draft_id: body.delivery_draft_id,
        paper_id: body.paper_id,
        work_item_id: body.work_item_id,
        section_key: body.section_key,
        artifact_manifest_id: body.artifact_manifest_id,
        payload_hash: body.payload_hash,
      };
      activeInbox = inbox([candidate]);
      deliveryDrafts += 1;
      return new Response(JSON.stringify({
        schema: "hepta.paper_raid.agent_bridge.delivery_draft_result.v1",
        candidate,
      }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (path === "/api/agent-bridge/proposals") {
      proposals += 1;
      assert.equal(JSON.parse(init.body).schema, "hepta.paper_raid.agent_bridge.proposal_request.v1");
      const candidate = activeInbox.papers[0].delivery_candidates.items[0];
      if (candidate) candidate.delivery_state = "consumed";
      return new Response(JSON.stringify({ accepted: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (path === "/api/agent-bridge/challenge-objects") {
      const key = parsed.searchParams.get("object_key");
      if (key === failedMaterialKey) {
        throw new Error("simulated Challenge material transport failure");
      }
      const bundle = activeInbox.papers[0].challenge_materials.items.find(item =>
        item.work_item_id === parsed.searchParams.get("work_item_id")
      );
      const object = bundle?.objects.find(item => item.object_key === key);
      assert.ok(object);
      materialReads += 1;
      return new Response(daemonMaterialBytes[key], {
        status: 200,
        headers: {
          "content-type": object.media_type,
          "content-length": String(daemonMaterialBytes[key].length),
        },
      });
    }
    throw new Error(`unexpected daemon route ${path}`);
  };

  const confirmRoot = await daemonRoot(directory, "confirm", "confirm");
  const confirm = await daemonCycle(confirmRoot);
  assert.equal(confirm.state, "awaiting_confirmation");
  assert.equal(proposals, 0);
  assert.equal(deliveryDrafts, 0);
  assert.equal(materialReads, 0);
  assert.deepEqual(await authorExecutorCalls(confirmRoot), []);

  const autoRoot = await daemonRoot(directory, "auto", "auto");
  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  const concurrent = await Promise.all([daemonCycle(autoRoot), daemonCycle(autoRoot)]);
  assert.deepEqual(concurrent.map(value => value.state), ["submitted", "ready"]);
  assert.equal(proposals, 1);
  assert.equal(deliveryDrafts, 1);
  assert.equal(materialReads, 4);
  assert.deepEqual(await authorExecutorCalls(autoRoot), [deliveryCandidate(0).work_item_id]);

  const multipleRoot = await daemonRoot(directory, "multiple", "auto");
  activeInbox = inbox(
    [deliveryCandidate(0), deliveryCandidate(1)],
    { drafts: false },
  );
  const multiple = await daemonCycle(multipleRoot);
  assert.equal(multiple.state, "multiple_items");
  assert.equal(multiple.candidate_count, 2);
  assert.equal(proposals, 1);
  assert.equal(materialReads, 4);
  assert.deepEqual(await authorExecutorCalls(multipleRoot), []);

  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  const cliWorkRoot = await daemonRoot(directory, "cli-work", "confirm");
  const originalCliWrite = process.stdout.write;
  let cliWorkOutput = "";
  process.stdout.write = chunk => {
    cliWorkOutput += String(chunk);
    return true;
  };
  try {
    await cliMain([
      "work",
      "--config",
      join(cliWorkRoot, "data", "bridge.config.json"),
      "--auto",
    ]);
  } finally {
    process.stdout.write = originalCliWrite;
  }
  assert.equal(JSON.parse(cliWorkOutput).status, "submitted");
  assert.equal(proposals, 2);
  assert.equal(deliveryDrafts, 2);
  assert.equal(materialReads, 8);
  assert.deepEqual(
    await authorExecutorCalls(cliWorkRoot),
    [deliveryCandidate(0).work_item_id],
  );

  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  const cliRoot = await daemonRoot(directory, "cli-materials", "confirm");
  const originalWrite = process.stdout.write;
  let cliOutput = "";
  process.stdout.write = chunk => {
    cliOutput += String(chunk);
    return true;
  };
  try {
    await cliMain([
      "prepare-materials",
      "--config",
      join(cliRoot, "data", "bridge.config.json"),
      "--work-item",
      deliveryCandidate(0).work_item_id,
    ]);
  } finally {
    process.stdout.write = originalWrite;
  }
  const prepared = JSON.parse(cliOutput);
  assert.equal(prepared.schema,
    "hepta.paper_raid.agent_bridge.challenge_materialization.v1");
  assert.equal(prepared.work_item_id, deliveryCandidate(0).work_item_id);
  assert.equal(materialReads, 12);
  assert.deepEqual(await authorExecutorCalls(cliRoot), []);

  const partialRoot = await daemonRoot(directory, "partial-materials", "auto");
  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  failedMaterialKey = "baseline";
  await assert.rejects(
    daemonCycle(partialRoot),
    /simulated Challenge material transport failure|agent_bridge_transport_failed/,
  );
  failedMaterialKey = null;
  assert.equal(proposals, 2);
  assert.deepEqual(await authorExecutorCalls(partialRoot), []);

  const mismatchedRoot = await daemonRoot(directory, "mismatched-executor", "auto");
  await writeSecure(
    join(mismatchedRoot, "data", "author-executor.mjs"),
    `#!${process.execPath}\nprocess.stdin.resume();\nprocess.stdin.on("end", () => process.stdout.write("{}"));\n`,
    0o700,
  );
  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  await assert.rejects(
    daemonCycle(mismatchedRoot),
    /Author executor result differs/,
  );
  assert.equal(proposals, 2);

  const zeroRoot = await daemonRoot(directory, "zero-materials", "auto");
  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  activeInbox.papers[0].challenge_materials = {
    schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
    status: "unavailable",
    reason_code: "frozen_challenge_material_authority_unavailable",
    items: [],
  };
  const zero = await daemonCycle(zeroRoot);
  assert.equal(zero.state, "ready");
  assert.equal(proposals, 2);
  assert.deepEqual(await authorExecutorCalls(zeroRoot), []);

  const duplicateRoot = await daemonRoot(directory, "duplicate-materials", "auto");
  activeInbox = inbox([deliveryCandidate(0)], { drafts: false });
  activeInbox.papers[0].challenge_materials.items.push(
    daemonMaterialBundle(deliveryCandidate(0)),
  );
  await assert.rejects(
    daemonCycle(duplicateRoot),
    /duplicates a work-item assignment/,
  );
  assert.equal(proposals, 2);
  assert.deepEqual(await authorExecutorCalls(duplicateRoot), []);

  activeInbox = reviewOnlyInbox();
  const reviewCliRoot = await daemonRoot(directory, "review-only-cli", "confirm");
  let reviewCliOutput = "";
  process.stdout.write = chunk => {
    reviewCliOutput += String(chunk);
    return true;
  };
  try {
    await cliMain([
      "work",
      "--config",
      join(reviewCliRoot, "data", "bridge.config.json"),
      "--auto",
    ]);
  } finally {
    process.stdout.write = originalWrite;
  }
  assert.equal(JSON.parse(reviewCliOutput).status, "idle");
  assert.deepEqual(await authorExecutorCalls(reviewCliRoot), []);

  const reviewDaemonRoot = await daemonRoot(directory, "review-only-daemon", "auto");
  const reviewDaemon = await daemonCycle(reviewDaemonRoot);
  assert.equal(reviewDaemon.state, "ready");
  assert.equal(reviewDaemon.candidate_count, 0);
  assert.deepEqual(await authorExecutorCalls(reviewDaemonRoot), []);

  activeInbox = reviewOnlyInbox();
  activeInbox.papers[0].delivery_candidates = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
    status: "available",
    reason_code: null,
    items: [],
  };
  await assert.rejects(
    cliMain([
      "work",
      "--config",
      join(reviewCliRoot, "data", "bridge.config.json"),
      "--auto",
    ]),
    /available delivery projection is invalid/,
  );
  await assert.rejects(
    daemonCycle(reviewDaemonRoot),
    /available delivery projection is invalid/,
  );

  const recoveryRoot = await daemonRoot(directory, "consumed-recovery", "auto");
  const consumed = deliveryCandidate(0);
  consumed.delivery_state = "consumed";
  activeInbox = inbox([consumed]);
  activeInbox.papers[0].tasks[0].status = "accepted";
  activeInbox.papers[0].challenge_materials = {
    schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
    status: "unavailable",
    reason_code: "terminal_work_has_no_active_material_input",
    items: [],
  };
  const recovered = await daemonCycle(recoveryRoot);
  assert.equal(recovered.state, "submitted");
  assert.equal(proposals, 3);
  assert.equal(materialReads, 18);
  assert.deepEqual(await authorExecutorCalls(recoveryRoot), []);
});
