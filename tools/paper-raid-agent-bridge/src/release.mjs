import {
  constants as fsConstants,
  chmod,
  lstat,
  mkdir,
  open,
  readdir,
  rename,
  rmdir,
  unlink,
} from "node:fs/promises";
import {
  createHash,
  createPublicKey,
  verify as ed25519Verify,
} from "node:crypto";
import { basename, dirname, join, resolve, sep } from "node:path";
import { canonicalJsonBytes } from "./canonical.mjs";

export const RELEASE_MANIFEST_SCHEMA =
  "hepta.paper_raid.agent_bridge.release_manifest.v1";
export const RELEASE_BUNDLE_SCHEMA =
  "hepta.paper_raid.agent_bridge.release_bundle.v1";
export const RELEASE_PRODUCT = "paper-raid-agent-bridge";
export const RELEASE_NODE_MINIMUM = 22;

const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/;
const VERSION_PATTERN = /^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?$/;
const FINGERPRINT_PATTERN = DIGEST_PATTERN;
const FILE_PATH_PATTERN = /^(?:package\.json|src\/[a-z0-9_]+(?:\.[a-z0-9_]+)*\.(?:mjs|py))$/;
const MAX_RELEASE_FILE_BYTES = 2 * 1024 * 1024;
const MAX_RELEASE_BUNDLE_BYTES = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 256 * 1024;

// This is the complete runtime closure. A release cannot add a dependency,
// executable, helper, or data file by mentioning it in a signed manifest.
export const RELEASE_FILE_SET = Object.freeze([
  Object.freeze({ path: "package.json", mode: 0o400 }),
  Object.freeze({ path: "src/canonical.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/cli.mjs", mode: 0o500 }),
  Object.freeze({ path: "src/client.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/config.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/daemon.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/files.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/frozen_python_loader.py", mode: 0o400 }),
  Object.freeze({ path: "src/identity.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/inbox.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/lifecycle.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/operations.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/practice.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/release.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/review.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/review_outbox.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/state.mjs", mode: 0o400 }),
  Object.freeze({ path: "src/work.mjs", mode: 0o400 }),
]);

const EXPECTED_PATHS = new Map(RELEASE_FILE_SET.map(entry => [entry.path, entry]));
const MANIFEST_KEYS = Object.freeze([
  "schema",
  "product",
  "version",
  "sequence",
  "node_minimum",
  "package_sha256",
  "files",
]);
const BUNDLE_KEYS = Object.freeze([
  "schema",
  "product",
  "version",
  "sequence",
  "files",
]);
const MANIFEST_FILE_KEYS = Object.freeze(["path", "type", "mode", "size", "sha256"]);
const BUNDLE_FILE_KEYS = Object.freeze([
  "path",
  "type",
  "mode",
  "size",
  "sha256",
  "data_base64",
]);

function exactKeys(value, expected, field) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    throw new Error(`${field} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new Error(`${field} contains unsupported or missing fields`);
  }
}

function canonicalDocumentBytes(value) {
  return Buffer.concat([canonicalJsonBytes(value), Buffer.from("\n")]);
}

function sha256(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function canonicalBase64(value, field) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${field} must be canonical padded base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.toString("base64") !== value) {
    throw new Error(`${field} must be canonical padded base64`);
  }
  return decoded;
}

function validateVersion(value) {
  if (typeof value !== "string" || !VERSION_PATTERN.test(value) || value.length > 96) {
    throw new Error("release version must be a bounded semantic version");
  }
  return value;
}

function validateSequence(value) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error("release sequence must be a positive safe integer");
  }
  return value;
}

function validateReleasePath(value, field = "release path") {
  if (
    typeof value !== "string" ||
    !FILE_PATH_PATTERN.test(value) ||
    value.startsWith("/") ||
    value.includes("\\") ||
    value.split("/").some(part => part === "" || part === "." || part === "..")
  ) {
    throw new Error(`${field} is not a safe relative runtime path`);
  }
  return value;
}

function validateReleaseFiles(files, { bundled }) {
  if (!Array.isArray(files) || files.length !== RELEASE_FILE_SET.length) {
    throw new Error("release file set is not exact");
  }
  let previous = "";
  return files.map((entry, index) => {
    exactKeys(entry, bundled ? BUNDLE_FILE_KEYS : MANIFEST_FILE_KEYS, `files[${index}]`);
    const path = validateReleasePath(entry.path, `files[${index}].path`);
    if (path <= previous) throw new Error("release file paths must be strictly sorted and unique");
    previous = path;
    const expected = EXPECTED_PATHS.get(path);
    if (!expected || expected.path !== RELEASE_FILE_SET[index].path) {
      throw new Error("release contains an extra, missing, or reordered file");
    }
    if (entry.type !== "file") {
      throw new Error("release entries must be regular files; links and devices are forbidden");
    }
    if (entry.mode !== expected.mode) {
      throw new Error(`release mode for ${path} is wider or different than the fixed mode`);
    }
    if (
      !Number.isSafeInteger(entry.size) ||
      entry.size < 1 ||
      entry.size > MAX_RELEASE_FILE_BYTES
    ) {
      throw new Error(`release size for ${path} is invalid`);
    }
    if (typeof entry.sha256 !== "string" || !DIGEST_PATTERN.test(entry.sha256)) {
      throw new Error(`release digest for ${path} is invalid`);
    }
    if (!bundled) return Object.freeze({ ...entry });
    const bytes = canonicalBase64(entry.data_base64, `${path}.data_base64`);
    if (bytes.length !== entry.size || sha256(bytes) !== entry.sha256) {
      throw new Error(`release bytes for ${path} do not match the signed descriptor`);
    }
    return Object.freeze({ ...entry, bytes });
  });
}

function validatePackageJson(bytes) {
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("release package.json is invalid JSON");
  }
  if (
    !value ||
    Array.isArray(value) ||
    value.name !== "@trillionnium/paper-raid-agent-bridge" ||
    value.type !== "module" ||
    value.engines?.node !== ">=22.0.0" ||
    value.bin?.["paper-raid-agent-bridge"] !== "./src/cli.mjs" ||
    Object.hasOwn(value, "dependencies") ||
    Object.hasOwn(value, "devDependencies") ||
    Object.hasOwn(value, "optionalDependencies") ||
    Object.hasOwn(value, "peerDependencies") ||
    Object.hasOwn(value, "bundledDependencies")
  ) {
    throw new Error("release package.json expands the fixed dependency-free runtime");
  }
}

export function validateReleaseManifest(value) {
  exactKeys(value, MANIFEST_KEYS, "release manifest");
  if (value.schema !== RELEASE_MANIFEST_SCHEMA || value.product !== RELEASE_PRODUCT) {
    throw new Error("unsupported Agent Bridge release manifest");
  }
  validateVersion(value.version);
  validateSequence(value.sequence);
  if (value.node_minimum !== RELEASE_NODE_MINIMUM) {
    throw new Error("release Node preflight is not pinned to Node 22");
  }
  if (typeof value.package_sha256 !== "string" || !DIGEST_PATTERN.test(value.package_sha256)) {
    throw new Error("release package digest is invalid");
  }
  validateReleaseFiles(value.files, { bundled: false });
  return Object.freeze(value);
}

export function validateReleaseBundle(value) {
  exactKeys(value, BUNDLE_KEYS, "release bundle");
  if (value.schema !== RELEASE_BUNDLE_SCHEMA || value.product !== RELEASE_PRODUCT) {
    throw new Error("unsupported Agent Bridge release bundle");
  }
  validateVersion(value.version);
  validateSequence(value.sequence);
  const files = validateReleaseFiles(value.files, { bundled: true });
  validatePackageJson(files.find(entry => entry.path === "package.json").bytes);
  return Object.freeze({ ...value, files: Object.freeze(files) });
}

export async function readHeldRegularFile(
  path,
  {
    maxBytes,
    privateFile = false,
    allowGroupWritable = false,
    testHooks = undefined,
  } = {},
) {
  if (
    testHooks !== undefined &&
    (
      !testHooks ||
      Array.isArray(testHooks) ||
      typeof testHooks !== "object" ||
      Object.keys(testHooks).some(key => !["afterOpen", "afterRead"].includes(key)) ||
      (testHooks.afterOpen !== undefined && typeof testHooks.afterOpen !== "function") ||
      (testHooks.afterRead !== undefined && typeof testHooks.afterRead !== "function")
    )
  ) {
    throw new Error("held-file test hooks are invalid");
  }
  const before = await lstat(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n) {
    throw new Error(`${path} must be one regular non-linked file`);
  }
  if (
    typeof process.getuid === "function" &&
    before.uid !== BigInt(process.getuid())
  ) {
    throw new Error(`${path} must be owned by the installing user`);
  }
  if (
    (before.mode & (allowGroupWritable ? 0o002n : 0o022n)) !== 0n ||
    (privateFile && (before.mode & 0o077n) !== 0n)
  ) {
    throw new Error(`${path} has unsafe permissions`);
  }
  if (before.size < 1n || before.size > BigInt(maxBytes)) {
    throw new Error(`${path} has an invalid size`);
  }
  const handle = await open(path, fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0));
  try {
    await testHooks?.afterOpen?.();
    const after = await handle.stat({ bigint: true });
    if (
      !after.isFile() ||
      after.nlink !== 1n ||
      after.dev !== before.dev ||
      after.ino !== before.ino ||
      after.uid !== before.uid ||
      after.size !== before.size ||
      after.mtimeNs !== before.mtimeNs ||
      after.ctimeNs !== before.ctimeNs
    ) {
      throw new Error(`${path} changed while being opened`);
    }
    const bytes = await handle.readFile();
    await testHooks?.afterRead?.();
    const final = await handle.stat({ bigint: true });
    if (
      final.dev !== after.dev ||
      final.ino !== after.ino ||
      final.uid !== after.uid ||
      final.size !== after.size ||
      final.mtimeNs !== after.mtimeNs ||
      final.ctimeNs !== after.ctimeNs ||
      BigInt(bytes.length) !== final.size
    ) {
      throw new Error(`${path} changed while being read`);
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

function publicKeyAndFingerprint(bytes) {
  let key;
  try {
    key = createPublicKey(bytes.toString("utf8"));
  } catch {
    try {
      key = createPublicKey({ key: bytes, format: "der", type: "spki" });
    } catch {
      throw new Error("trusted release public key must be PEM or DER SPKI");
    }
  }
  if (key.asymmetricKeyType !== "ed25519") {
    throw new Error("trusted release public key must be Ed25519");
  }
  const spki = key.export({ format: "der", type: "spki" });
  return Object.freeze({ key, spki, fingerprint: sha256(spki) });
}

export async function readTrustedPublicKey(
  path,
  fingerprintPin,
  { testHooks = undefined } = {},
) {
  if (typeof fingerprintPin !== "string" || !FINGERPRINT_PATTERN.test(fingerprintPin)) {
    throw new Error("trusted release key fingerprint pin must be sha256:<64 lowercase hex>");
  }
  const bytes = await readHeldRegularFile(resolve(path), {
    maxBytes: 64 * 1024,
    testHooks,
  });
  const trusted = publicKeyAndFingerprint(bytes);
  if (trusted.fingerprint !== fingerprintPin) {
    throw new Error("trusted release public key does not match the operator fingerprint pin");
  }
  return Object.freeze({ ...trusted, sourceBytes: bytes });
}

export async function readReleaseInputs({
  packagePath,
  manifestPath,
  signaturePath,
  trustedKeyPath,
  fingerprint,
  nodeMajor = Number(process.versions.node.split(".")[0]),
}) {
  if (!Number.isSafeInteger(nodeMajor) || nodeMajor < RELEASE_NODE_MINIMUM) {
    throw new Error(`Node >=${RELEASE_NODE_MINIMUM} is required before release verification`);
  }
  const [packageBytes, manifestBytes, signature, trusted] = await Promise.all([
    readHeldRegularFile(resolve(packagePath), { maxBytes: MAX_RELEASE_BUNDLE_BYTES }),
    readHeldRegularFile(resolve(manifestPath), { maxBytes: MAX_MANIFEST_BYTES }),
    readHeldRegularFile(resolve(signaturePath), { maxBytes: 1024 }),
    readTrustedPublicKey(trustedKeyPath, fingerprint),
  ]);
  if (signature.length !== 64) {
    throw new Error("detached release signature must contain exactly 64 raw Ed25519 bytes");
  }
  let manifestValue;
  let bundleValue;
  try {
    manifestValue = JSON.parse(manifestBytes.toString("utf8"));
  } catch {
    throw new Error("release manifest is invalid JSON");
  }
  const manifest = validateReleaseManifest(manifestValue);
  if (!canonicalDocumentBytes(manifestValue).equals(manifestBytes)) {
    throw new Error("release manifest bytes are not the exact canonical document");
  }
  if (!ed25519Verify(null, manifestBytes, trusted.key, signature)) {
    throw new Error("detached Ed25519 release signature is invalid");
  }
  if (sha256(packageBytes) !== manifest.package_sha256) {
    throw new Error("release package bytes do not match the signed manifest");
  }
  try {
    bundleValue = JSON.parse(packageBytes.toString("utf8"));
  } catch {
    throw new Error("release package is invalid JSON");
  }
  const bundle = validateReleaseBundle(bundleValue);
  if (!canonicalDocumentBytes(bundleValue).equals(packageBytes)) {
    throw new Error("release package bytes are not the exact canonical document");
  }
  if (bundle.version !== manifest.version || bundle.sequence !== manifest.sequence) {
    throw new Error("release package identity does not match the signed manifest");
  }
  for (let index = 0; index < bundle.files.length; index += 1) {
    const bundled = bundle.files[index];
    const declared = manifest.files[index];
    for (const field of MANIFEST_FILE_KEYS) {
      if (bundled[field] !== declared[field]) {
        throw new Error(`release descriptor mismatch for ${bundled.path}`);
      }
    }
  }
  return Object.freeze({
    manifest,
    manifestBytes,
    bundle,
    packageBytes,
    signature,
    trusted,
    releaseId: releaseId(manifest),
  });
}

export function releaseId(manifest) {
  validateVersion(manifest.version);
  validateSequence(manifest.sequence);
  return `${String(manifest.sequence).padStart(12, "0")}-${manifest.version}`;
}

async function safeSourceFile(path) {
  return readHeldRegularFile(path, {
    maxBytes: MAX_RELEASE_FILE_BYTES,
    allowGroupWritable: true,
  });
}

export async function buildDeterministicRelease({ sourceRoot, version, sequence }) {
  validateVersion(version);
  validateSequence(sequence);
  const root = resolve(sourceRoot);
  const files = [];
  for (const expected of RELEASE_FILE_SET) {
    const bytes = await safeSourceFile(join(root, ...expected.path.split("/")));
    files.push({
      path: expected.path,
      type: "file",
      mode: expected.mode,
      size: bytes.length,
      sha256: sha256(bytes),
      data_base64: bytes.toString("base64"),
    });
  }
  validatePackageJson(Buffer.from(files[0].data_base64, "base64"));
  const bundle = {
    schema: RELEASE_BUNDLE_SCHEMA,
    product: RELEASE_PRODUCT,
    version,
    sequence,
    files,
  };
  const packageBytes = canonicalDocumentBytes(bundle);
  const manifest = {
    schema: RELEASE_MANIFEST_SCHEMA,
    product: RELEASE_PRODUCT,
    version,
    sequence,
    node_minimum: RELEASE_NODE_MINIMUM,
    package_sha256: sha256(packageBytes),
    files: files.map(({ data_base64: _data, ...entry }) => entry),
  };
  const manifestBytes = canonicalDocumentBytes(manifest);
  validateReleaseBundle(bundle);
  validateReleaseManifest(manifest);
  return Object.freeze({ bundle, packageBytes, manifest, manifestBytes });
}

async function syncDirectory(path) {
  const handle = await open(path, fsConstants.O_RDONLY);
  try {
    await handle.sync();
  } finally {
    await handle.close();
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
  const installed = await open(
    path,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    await installed.sync();
  } finally {
    await installed.close();
  }
}

async function ensureStagingDirectory(path) {
  await mkdir(path, { mode: 0o700 });
  const stats = await lstat(path);
  if (
    !stats.isDirectory() ||
    stats.isSymbolicLink() ||
    stats.nlink < 2 ||
    (stats.mode & 0o077) !== 0 ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error(`${path} is not an owner-only staging directory`);
  }
}

function insideRoot(root, path) {
  const resolvedRoot = resolve(root);
  const resolvedPath = resolve(path);
  return resolvedPath.startsWith(`${resolvedRoot}${sep}`);
}

export async function extractVerifiedRelease(verified, stagingPath) {
  const staging = resolve(stagingPath);
  const releasesRoot = dirname(staging);
  if (!insideRoot(releasesRoot, staging)) {
    throw new Error("release staging path escaped its parent");
  }
  await ensureStagingDirectory(staging);
  const createdDirectories = new Set([staging]);
  try {
    for (const entry of verified.bundle.files) {
      const destination = join(staging, ...entry.path.split("/"));
      if (!insideRoot(staging, destination)) {
        throw new Error("release destination escaped staging root");
      }
      const parent = dirname(destination);
      if (!createdDirectories.has(parent)) {
        await ensureStagingDirectory(parent);
        createdDirectories.add(parent);
      }
      await writeExclusive(destination, entry.bytes, entry.mode);
      const stats = await lstat(destination);
      if (
        !stats.isFile() ||
        stats.isSymbolicLink() ||
        stats.nlink !== 1 ||
        (stats.mode & 0o777) !== entry.mode ||
        stats.size !== entry.size ||
        (typeof process.getuid === "function" && stats.uid !== process.getuid())
      ) {
        throw new Error(`extracted release file ${entry.path} failed verification`);
      }
    }
    await writeExclusive(
      join(staging, "release.manifest.json"),
      verified.manifestBytes,
      0o400,
    );
    await writeExclusive(join(staging, "release.manifest.sig"), verified.signature, 0o400);
    for (const directory of [...createdDirectories].sort((a, b) => b.length - a.length)) {
      await chmod(directory, 0o500);
      await syncDirectory(directory);
    }
    return staging;
  } catch (error) {
    await removeStagingRelease(staging).catch(() => {});
    throw error;
  }
}

async function listTree(root, relative = "") {
  const directory = relative ? join(root, ...relative.split("/")) : root;
  const entries = await readdir(directory, { withFileTypes: true });
  const output = [];
  for (const entry of entries) {
    const childRelative = relative ? `${relative}/${entry.name}` : entry.name;
    const child = join(directory, entry.name);
    const stats = await lstat(child);
    if (stats.isSymbolicLink()) throw new Error("installed release contains a symbolic link");
    if (stats.isDirectory()) {
      output.push({ path: childRelative, type: "directory", stats });
      output.push(...(await listTree(root, childRelative)));
    } else if (stats.isFile()) {
      if (stats.nlink !== 1) throw new Error("installed release contains a hard-linked file");
      output.push({ path: childRelative, type: "file", stats });
    } else {
      throw new Error("installed release contains a device, socket, or other special file");
    }
  }
  return output.sort((a, b) => a.path.localeCompare(b.path));
}

async function readInstalledManifest(releasePath, signatureOverridePath) {
  const manifestPath = join(releasePath, "release.manifest.json");
  const signaturePath = signatureOverridePath
    ? resolve(signatureOverridePath)
    : join(releasePath, "release.manifest.sig");
  const [manifestBytes, signature] = await Promise.all([
    readHeldRegularFile(manifestPath, { maxBytes: MAX_MANIFEST_BYTES, privateFile: true }),
    readHeldRegularFile(signaturePath, { maxBytes: 1024 }),
  ]);
  if (signature.length !== 64) throw new Error("stored release signature is invalid");
  let value;
  try {
    value = JSON.parse(manifestBytes.toString("utf8"));
  } catch {
    throw new Error("stored release manifest is invalid JSON");
  }
  const manifest = validateReleaseManifest(value);
  if (!canonicalDocumentBytes(value).equals(manifestBytes)) {
    throw new Error("stored release manifest is not canonical");
  }
  return { manifest, manifestBytes, signature };
}

export async function verifyInstalledRelease({
  releasePath,
  trusted,
  expectedReleaseId,
  signatureOverridePath,
}) {
  const root = resolve(releasePath);
  const rootStats = await lstat(root);
  if (
    !rootStats.isDirectory() ||
    rootStats.isSymbolicLink() ||
    (rootStats.mode & 0o777) !== 0o500 ||
    (typeof process.getuid === "function" && rootStats.uid !== process.getuid())
  ) {
    throw new Error("installed release directory is unsafe");
  }
  const { manifest, manifestBytes, signature } = await readInstalledManifest(
    root,
    signatureOverridePath,
  );
  if (releaseId(manifest) !== expectedReleaseId || basename(root) !== expectedReleaseId) {
    throw new Error("installed release directory does not match its signed identity");
  }
  if (!ed25519Verify(null, manifestBytes, trusted.key, signature)) {
    throw new Error("installed release detached signature is invalid");
  }
  if (signatureOverridePath) {
    const stored = await readHeldRegularFile(join(root, "release.manifest.sig"), {
      maxBytes: 1024,
      privateFile: true,
    });
    if (!stored.equals(signature)) {
      throw new Error("operator rollback signature does not match the verified previous release");
    }
  }
  const expectedFiles = new Set([
    ...manifest.files.map(entry => entry.path),
    "release.manifest.json",
    "release.manifest.sig",
  ]);
  const expectedDirectories = new Set(
    manifest.files
      .map(entry => dirname(entry.path))
      .filter(path => path !== "."),
  );
  const tree = await listTree(root);
  for (const entry of tree) {
    if (entry.type === "directory") {
      if (
        !expectedDirectories.has(entry.path) ||
        (entry.stats.mode & 0o777) !== 0o500 ||
        (typeof process.getuid === "function" && entry.stats.uid !== process.getuid())
      ) {
        throw new Error("installed release contains an extra or wide-permission directory");
      }
      continue;
    }
    if (!expectedFiles.delete(entry.path)) {
      throw new Error("installed release contains an extra file");
    }
  }
  if (expectedFiles.size !== 0) throw new Error("installed release is missing a file");
  for (const descriptor of manifest.files) {
    const path = join(root, ...descriptor.path.split("/"));
    const bytes = await readHeldRegularFile(path, {
      maxBytes: descriptor.size,
      privateFile: true,
    });
    const stats = await lstat(path);
    if (
      bytes.length !== descriptor.size ||
      sha256(bytes) !== descriptor.sha256 ||
      (stats.mode & 0o777) !== descriptor.mode ||
      (typeof process.getuid === "function" && stats.uid !== process.getuid())
    ) {
      throw new Error(`installed release ${descriptor.path} is tampered`);
    }
  }
  for (const metadata of ["release.manifest.json", "release.manifest.sig"]) {
    const stats = await lstat(join(root, metadata));
    if (
      !stats.isFile() ||
      stats.isSymbolicLink() ||
      stats.nlink !== 1 ||
      (stats.mode & 0o777) !== 0o400 ||
      (typeof process.getuid === "function" && stats.uid !== process.getuid())
    ) {
      throw new Error("installed release metadata is unsafe");
    }
  }
  return Object.freeze({
    manifest,
    manifestBytes,
    signature,
    releaseId: expectedReleaseId,
    rootIdentity: Object.freeze({ dev: rootStats.dev, ino: rootStats.ino }),
  });
}

function sameIdentity(stats, identity) {
  return stats.dev === identity.dev && stats.ino === identity.ino;
}

export async function removeStagingRelease(stagingPath, { expectedIdentity } = {}) {
  const root = resolve(stagingPath);
  let rootStats;
  try {
    rootStats = await lstat(root);
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }
  if (
    !rootStats.isDirectory() ||
    rootStats.isSymbolicLink() ||
    (typeof process.getuid === "function" && rootStats.uid !== process.getuid()) ||
    (expectedIdentity && !sameIdentity(rootStats, expectedIdentity))
  ) {
    throw new Error("refusing to remove a release directory that changed identity");
  }
  let entries;
  try {
    entries = await listTree(root);
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }
  await chmod(root, 0o700);
  for (const entry of entries
    .filter(value => value.type === "directory")
    .sort((left, right) => left.path.length - right.path.length)) {
    await chmod(join(root, ...entry.path.split("/")), 0o700);
  }
  for (const entry of entries.filter(value => value.type === "file").sort((a, b) => b.path.length - a.path.length)) {
    await unlink(join(root, ...entry.path.split("/")));
  }
  for (const entry of entries.filter(value => value.type === "directory").sort((a, b) => b.path.length - a.path.length)) {
    const path = join(root, ...entry.path.split("/"));
    await rmdir(path);
  }
  await rmdir(root);
  await syncDirectory(dirname(root));
}

export async function publishStagedRelease(stagingPath, finalPath) {
  const staging = resolve(stagingPath);
  const final = resolve(finalPath);
  if (dirname(staging) !== dirname(final)) {
    throw new Error("release publish must remain inside one managed release directory");
  }

  const stagingStats = await lstat(staging);
  if (
    !stagingStats.isDirectory() ||
    stagingStats.isSymbolicLink() ||
    (stagingStats.mode & 0o777) !== 0o500 ||
    (typeof process.getuid === "function" && stagingStats.uid !== process.getuid())
  ) {
    throw new Error("release staging directory is unsafe");
  }

  // Node does not expose Linux renameat2(RENAME_NOREPLACE).  Publishing by a
  // plain rename after an lstat check would therefore overwrite a directory
  // created in the check/rename window.  Reserve the final name with mkdir's
  // kernel-enforced EEXIST semantics, populate that private directory, fsync
  // it, and only then make it read-only.  The active `current` pointer is
  // switched later, so a partially populated but unreferenced reservation is
  // never executable.
  await mkdir(final, { mode: 0o700 });
  const reserved = await lstat(final);
  const finalIdentity = Object.freeze({ dev: reserved.dev, ino: reserved.ino });
  if (
    !reserved.isDirectory() ||
    reserved.isSymbolicLink() ||
    (reserved.mode & 0o077) !== 0 ||
    (typeof process.getuid === "function" && reserved.uid !== process.getuid())
  ) {
    await removeStagingRelease(final, { expectedIdentity: finalIdentity }).catch(() => {});
    throw new Error("reserved release directory is unsafe");
  }

  try {
    const tree = await listTree(staging);
    const directories = tree
      .filter(entry => entry.type === "directory")
      .sort((left, right) => left.path.length - right.path.length);
    await chmod(staging, 0o700);
    for (const entry of directories) {
      await chmod(join(staging, ...entry.path.split("/")), 0o700);
    }
    for (const entry of directories) {
      await mkdir(join(final, ...entry.path.split("/")), { mode: 0o700 });
    }
    for (const entry of tree.filter(value => value.type === "file")) {
      await rename(
        join(staging, ...entry.path.split("/")),
        join(final, ...entry.path.split("/")),
      );
    }
    for (const entry of [...directories].sort((left, right) => right.path.length - left.path.length)) {
      await rmdir(join(staging, ...entry.path.split("/")));
    }
    await rmdir(staging);
    for (const entry of [...directories].sort((left, right) => right.path.length - left.path.length)) {
      const path = join(final, ...entry.path.split("/"));
      await chmod(path, 0o500);
      await syncDirectory(path);
    }
    await chmod(final, 0o500);
    await syncDirectory(final);
    await syncDirectory(dirname(final));
    return finalIdentity;
  } catch (error) {
    try {
      await removeStagingRelease(final, { expectedIdentity: finalIdentity });
    } catch (cleanupError) {
      throw new AggregateError(
        [error, cleanupError],
        "release publication failed and its private reservation could not be cleaned",
      );
    }
    throw error;
  }
}

export async function writeCanonicalDocumentExclusive(path, value, mode = 0o600) {
  await writeExclusive(resolve(path), canonicalDocumentBytes(value), mode);
  await syncDirectory(dirname(resolve(path)));
}

export async function writeReleaseArtifacts({ outputDirectory, version, sequence, sourceRoot }) {
  const output = resolve(outputDirectory);
  await mkdir(output, { recursive: true, mode: 0o700 });
  const stats = await lstat(output);
  if (
    !stats.isDirectory() ||
    stats.isSymbolicLink() ||
    (stats.mode & 0o022) !== 0 ||
    (typeof process.getuid === "function" && stats.uid !== process.getuid())
  ) {
    throw new Error("release output directory is unsafe");
  }
  const built = await buildDeterministicRelease({ sourceRoot, version, sequence });
  const prefix = `${RELEASE_PRODUCT}-${String(sequence).padStart(12, "0")}-${version}`;
  const packagePath = join(output, `${prefix}.bundle.json`);
  const manifestPath = join(output, `${prefix}.manifest.json`);
  await writeExclusive(packagePath, built.packageBytes, 0o600);
  try {
    await writeExclusive(manifestPath, built.manifestBytes, 0o600);
  } catch (error) {
    await unlink(packagePath).catch(() => {});
    throw error;
  }
  await syncDirectory(output);
  return Object.freeze({ packagePath, manifestPath, ...built });
}
