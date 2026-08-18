import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { loadConfig } from "../src/config.mjs";

const VALID = {
  schema: "hepta.paper_raid.agent_bridge.config.v2",
  bff_url: "http://127.0.0.1:8088",
  identity_file: "agent.identity.json",
  state_file: "agent.state.json",
  author_executor: {
    schema: "hepta.paper_raid.agent_bridge.author_executor.v1",
    executable: "author-executor",
    timeout_ms: 900_000,
  },
  capabilities: ["artifact_analysis", "section_drafting"],
  resource_classes: ["cpu", "sandbox"],
  max_parallel_tasks: 2,
  paper_ids: [],
  poll_interval_ms: 2500,
  request_timeout_ms: 8000,
};

async function writeConfig(path, value) {
  await writeFile(path, `${JSON.stringify(value)}\n`, { mode: 0o600 });
}

test("config v2 rejects v1 with a migration hint and rejects every secret field", async t => {
  const directory = await mkdtemp(join(tmpdir(), "paper-raid-agent-config-v2-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "bridge.json");
  await writeConfig(path, {
    ...VALID,
    schema: "hepta.paper_raid.agent_bridge.config.v1",
    login_key_file: "login-key",
  });
  await assert.rejects(
    () => loadConfig(path),
    /config v1 is retired; migrate to config v2 and remove login_key_file/,
  );

  for (const secret of [
    "login_key_file",
    "login_key",
    "token",
    "access_token",
    "bearer",
    "cookie",
    "csrf",
    "authorization",
    "private_key",
    "pairing_code",
    "password",
    "secret",
  ]) {
    await writeConfig(path, { ...VALID, [secret]: "forbidden" });
    await assert.rejects(
      () => loadConfig(path),
      error => error.message.includes("secret field") && error.message.includes(secret),
    );
  }
  await writeConfig(path, { ...VALID, nested: { token: "forbidden" } });
  await assert.rejects(() => loadConfig(path), /secret field config.nested.token/);
});

test("valid v2 config creates one bounded self-declared disclosure", async t => {
  const directory = await mkdtemp(join(tmpdir(), "paper-raid-agent-config-valid-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "bridge.json");
  await writeConfig(path, VALID);
  const config = await loadConfig(path);
  assert.equal(config.bff_url, VALID.bff_url);
  assert.deepEqual(config.paper_ids, []);
  assert.equal(config.author_executor.executable, join(directory, "author-executor"));
  assert.deepEqual(config.capability_disclosure, {
    schema: "hepta.paper_raid.agent_capability_disclosure.v1",
    assurance: "self_declared_unverified",
    capabilities: VALID.capabilities,
    resource_classes: VALID.resource_classes,
    max_parallel_tasks: 2,
  });

  await writeConfig(path, {
    ...VALID,
    capabilities: ["section_drafting", "artifact_analysis"],
  });
  await assert.rejects(() => loadConfig(path), /sorted and unique/);
  await writeConfig(path, { ...VALID, max_parallel_tasks: 33 });
  await assert.rejects(() => loadConfig(path), /between 1 and 32/);
  await writeConfig(path, {
    ...VALID,
    author_executor: { ...VALID.author_executor, timeout_ms: 999 },
  });
  await assert.rejects(() => loadConfig(path), /one exact bounded v1 executor/);
});
