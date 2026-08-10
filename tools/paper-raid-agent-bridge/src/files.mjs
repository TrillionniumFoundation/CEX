import {
  constants as fsConstants,
  link,
  lstat,
  open,
  rename,
  unlink,
} from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { basename, dirname, join } from "node:path";

const MAX_LOCAL_FILE_BYTES = 1024 * 1024;

async function assertSafeFileStat(path, stats, { privateFile, ownerOnly }) {
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error(`${path} must be a regular non-symlink file`);
  }
  if (privateFile && (stats.mode & 0o077) !== 0) {
    throw new Error(`${path} must not grant group or other permissions`);
  }
  if (!privateFile && (stats.mode & 0o022) !== 0) {
    throw new Error(`${path} must not be group or world writable`);
  }
  if (
    ownerOnly &&
    typeof process.getuid === "function" &&
    stats.uid !== process.getuid()
  ) {
    throw new Error(`${path} must be owned by the current user`);
  }
  if (stats.size < 1 || stats.size > MAX_LOCAL_FILE_BYTES) {
    throw new Error(`${path} has an invalid size`);
  }
}

export async function readSafeFile(
  path,
  { privateFile = false, ownerOnly = privateFile, maxBytes = MAX_LOCAL_FILE_BYTES } = {},
) {
  const before = await lstat(path);
  await assertSafeFileStat(path, before, { privateFile, ownerOnly });
  const handle = await open(
    path,
    fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
  );
  try {
    const after = await handle.stat();
    await assertSafeFileStat(path, after, { privateFile, ownerOnly });
    if (after.dev !== before.dev || after.ino !== before.ino || after.size > maxBytes) {
      throw new Error(`${path} changed while it was being opened`);
    }
    return await handle.readFile();
  } finally {
    await handle.close();
  }
}

export async function readSafeJson(path, options) {
  const bytes = await readSafeFile(path, options);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error(`${path} must contain valid JSON`);
  }
}

export async function writePrivateJsonExclusive(path, value) {
  const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
  const directory = dirname(path);
  const temporaryPath = join(
    directory,
    `.${basename(path)}.${process.pid}.${randomBytes(12).toString("hex")}.tmp`,
  );
  const handle = await open(
    temporaryPath,
    fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL,
    0o600,
  );
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
  try {
    // Linking a fully synced temporary file makes first creation atomic while
    // retaining O_EXCL semantics for identities and durable transactions.
    await link(temporaryPath, path);
    await syncDirectory(directory);
  } finally {
    await unlink(temporaryPath).catch(error => {
      if (error?.code !== "ENOENT") throw error;
    });
  }
}

export async function preparePrivateJsonDestination(path, { replace = false } = {}) {
  const directory = dirname(path);
  const directoryStats = await lstat(directory);
  if (!directoryStats.isDirectory() || directoryStats.isSymbolicLink()) {
    throw new Error(`${directory} must be a non-symlink directory`);
  }
  if ((directoryStats.mode & 0o022) !== 0) {
    throw new Error(`${directory} must not be group or world writable`);
  }
  if (
    typeof process.getuid === "function" &&
    directoryStats.uid !== process.getuid()
  ) {
    throw new Error(`${directory} must be owned by the current user`);
  }
  try {
    const existing = await lstat(path);
    if (!replace) throw new Error(`${path} already exists`);
    await assertSafeFileStat(path, existing, {
      privateFile: true,
      ownerOnly: true,
    });
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const probe = join(
    directory,
    `.${basename(path)}.${process.pid}.${randomBytes(12).toString("hex")}.probe`,
  );
  const handle = await open(
    probe,
    fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL,
    0o600,
  );
  try {
    await handle.writeFile("ready\n");
    await handle.sync();
  } finally {
    await handle.close();
  }
  await unlink(probe);
  await syncDirectory(directory);
}

export async function writePrivateJsonReplacing(path, value) {
  await preparePrivateJsonDestination(path, { replace: true });
  const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
  const directory = dirname(path);
  const temporaryPath = join(
    directory,
    `.${basename(path)}.${process.pid}.${randomBytes(12).toString("hex")}.tmp`,
  );
  const handle = await open(
    temporaryPath,
    fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL,
    0o600,
  );
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
  try {
    await rename(temporaryPath, path);
    await syncDirectory(directory);
  } finally {
    await unlink(temporaryPath).catch(error => {
      if (error?.code !== "ENOENT") throw error;
    });
  }
}

async function syncDirectory(path) {
  const handle = await open(path, fsConstants.O_RDONLY);
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

export async function removePrivateFile(path, { optional = false } = {}) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (optional && error?.code === "ENOENT") return false;
    throw error;
  }
  await assertSafeFileStat(path, stats, {
    privateFile: true,
    ownerOnly: true,
  });
  try {
    await unlink(path);
  } catch (error) {
    if (optional && error?.code === "ENOENT") return false;
    throw error;
  }
  await syncDirectory(dirname(path));
  return true;
}
