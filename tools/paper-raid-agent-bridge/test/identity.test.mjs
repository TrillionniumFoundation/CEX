import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { generateIdentity, importIdentity, loadIdentity } from "../src/identity.mjs";

test("generated identity stays owner-only and signs with its local Ed25519 key", async t => {
  const directory = await mkdtemp(join(tmpdir(), "paper-raid-agent-bridge-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "agent.identity.json");
  const description = await generateIdentity("agent.local.fixture", path);
  const identity = await loadIdentity(path);
  const message = Buffer.from("canonical fixture", "utf8");
  const signature = identity.sign(message);
  assert.equal(description.agent_key_id, identity.agent_key_id);
  assert.equal(identity.verify(message, signature), true);
  await chmod(path, 0o644);
  await assert.rejects(() => loadIdentity(path), /must not grant group or other permissions/);
});

test("PEM Ed25519 import produces the same public key and an owner-only identity", async t => {
  const directory = await mkdtemp(join(tmpdir(), "paper-raid-agent-import-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const source = join(directory, "source.pem");
  const output = join(directory, "imported.identity.json");
  const { privateKey } = generateKeyPairSync("ed25519");
  await writeFile(
    source,
    privateKey.export({ format: "pem", type: "pkcs8" }),
    { mode: 0o600 },
  );
  const description = await importIdentity("agent.imported", source, output);
  const identity = await loadIdentity(output);
  assert.equal(identity.agent_id, "agent.imported");
  assert.equal(identity.agent_key_id, description.agent_key_id);
});
