import assert from "node:assert/strict";
import {
  chmod,
  link,
  lstat,
  mkdir,
  mkdtemp,
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
import { fileURLToPath } from "node:url";
import test from "node:test";

import { agentCapabilityDisclosureHash } from "../src/canonical.mjs";
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

test("every update pointer phase restores current, previous, service, and new release", async t => {
  const directory = await temporary(t, "update-phases");
  const trust = signer();
  const first = await releaseFixture(directory, trust, "3.4.0", 20);
  const second = await releaseFixture(directory, trust, "3.5.0", 21);
  const phases = [
    "update_after_publish",
    "update_after_previous",
    "update_after_current",
    "update_after_verify",
    "update_after_restart",
  ];
  for (const phase of phases) {
    const root = join(directory, phase);
    const mock = await writeMockSystemctl(directory);
    await installProduct(installOptions(root, mock, first));
    await assert.rejects(updateProduct(updateOptions(root, mock, second, {
      testHooks: {
        phase(current) {
          if (current === phase) throw new Error(`injected ${phase}`);
        },
      },
    })), new RegExp(`injected ${phase}`));
    assert.equal(await pointerIdentity(root, "current"), first.releaseId);
    await absent(join(root, "previous"));
    assert.deepEqual(await readdir(join(root, "releases")), [first.releaseId]);
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

function inbox(items) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "drafting",
      tasks: [],
      proposals: [],
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status: "available",
        reason_code: null,
        items,
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

test("daemon keeps Confirm inert, Auto exact-one, and concurrent Auto single-submit", async t => {
  const directory = await temporary(t, "daemon");
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });
  let activeInbox = inbox([deliveryCandidate(0)]);
  let proposals = 0;
  globalThis.fetch = async (url, init = {}) => {
    const path = new URL(url).pathname;
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
    if (path === "/api/agent-bridge/proposals") {
      proposals += 1;
      assert.equal(JSON.parse(init.body).schema, "hepta.paper_raid.agent_bridge.proposal_request.v1");
      return new Response(JSON.stringify({ accepted: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    throw new Error(`unexpected daemon route ${path}`);
  };

  const confirmRoot = await daemonRoot(directory, "confirm", "confirm");
  const confirm = await daemonCycle(confirmRoot);
  assert.equal(confirm.state, "awaiting_confirmation");
  assert.equal(proposals, 0);

  const autoRoot = await daemonRoot(directory, "auto", "auto");
  const concurrent = await Promise.all([daemonCycle(autoRoot), daemonCycle(autoRoot)]);
  assert.deepEqual(concurrent.map(value => value.state), ["submitted", "ready"]);
  assert.equal(proposals, 1);

  const multipleRoot = await daemonRoot(directory, "multiple", "auto");
  activeInbox = inbox([deliveryCandidate(0), deliveryCandidate(1)]);
  const multiple = await daemonCycle(multipleRoot);
  assert.equal(multiple.state, "multiple_items");
  assert.equal(multiple.candidate_count, 2);
  assert.equal(proposals, 1);
});
