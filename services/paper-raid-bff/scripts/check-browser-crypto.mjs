import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const browserUrl = new URL("../src/browser.js", import.meta.url);
const source = await readFile(browserUrl, "utf8");

assert.equal(source.includes("localStorage"), false);
assert.equal(source.includes("indexedDB"), false);
assert.equal(source.includes(".style"), false);
assert.ok(source.includes("hepta.paper-raid.live-cursor.v1:"));
assert.equal((source.match(/sessionStorage/g) || []).length, 2);

const createStart = source.indexOf("function bindHumanKeyCreate()");
const registrationStart = source.indexOf("function bindHumanKeyRegistration()");
const agentStart = source.indexOf("function bindAgentBinding()");
assert.ok(createStart >= 0 && registrationStart > createStart && agentStart > registrationStart);
const createFlow = source.slice(createStart, registrationStart);
const registrationFlow = source.slice(registrationStart, agentStart);
assert.equal(createFlow.includes("/api/onboarding/"), false);
assert.equal(registrationFlow.includes("createEncryptedBundle"), false);
assert.ok(registrationFlow.includes("/api/onboarding/human/challenge"));
assert.ok(registrationFlow.includes("/api/onboarding/human/register"));
assert.ok(registrationFlow.includes("import the original bundle"));

const sessionValues = new Map();
const context = vm.createContext({
  Uint8Array,
  ArrayBuffer,
  Blob,
  TextEncoder,
  TextDecoder,
  URL,
  atob,
  btoa,
  console,
  crypto: webcrypto,
  document: {
    addEventListener() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
  },
  setTimeout,
  clearTimeout,
  sessionStorage: {
    getItem(key) { return sessionValues.get(key) ?? null; },
    setItem(key, value) { sessionValues.set(key, value); },
  },
  window: {
    location: { assign() {} },
    setTimeout,
  },
});
vm.runInContext(source, context, { filename: browserUrl.pathname });

assert.equal(context.nextHeptaCursor([{ cursor: 2 }, { cursor: 7 }, { cursor: 5 }], 3), 7);
assert.equal(context.nextHeptaCursor([{ cursor: -1 }, { cursor: "8" }], 4), 4);
assert.equal(context.paperRoomPhase({ paper_room: { paper: { phase: "submission_ready" } } }), "submission_ready");
assert.equal(context.paperRoomPhase({ paper_room: { phase: "untrusted_shape" } }), "waiting_for_authority");
let liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 0);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);
sessionValues.set("hepta.paper-raid.live-cursor.v1:paper-alpha", JSON.stringify({
  hepta: 11,
  nakama: { "paper.raid:one": 19, "paper.raid:two": 7 },
}));
liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 11);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);
sessionValues.set("hepta.paper-raid.live-cursor.v1:paper-alpha", "not-json");
liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 0);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);

const heads = [
  { logical_session_id: "paper.raid:one", roster_version: 1, nakama_completion_received: false },
];
let nakamaSessions = context.reconcileNakamaSessions(heads, new Map());
assert.equal(nakamaSessions.size, 1);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 0);
const accepted = context.acceptNakamaArchive({
  logical_session_id: "paper.raid:one",
  roster_version: 1,
  requested_after_sequence: 0,
  archive: {
    schema: "trnm.nakama.research-session.archive.v1",
    logical_session_id: "paper.raid:one",
    roster_version: 1,
    after_sequence: 0,
    next_after_sequence: 3,
    has_more: true,
  },
}, nakamaSessions.get("paper.raid:one"));
nakamaSessions.set("paper.raid:one", accepted);
nakamaSessions = context.reconcileNakamaSessions(heads, nakamaSessions);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 3);
nakamaSessions = context.reconcileNakamaSessions([
  { logical_session_id: "paper.raid:one", roster_version: 2, nakama_completion_received: false },
], nakamaSessions);
assert.equal(nakamaSessions.size, 1);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 0);
assert.equal(nakamaSessions.get("paper.raid:one").rosterVersion, 2);
assert.throws(() => context.acceptNakamaArchive({
  logical_session_id: "paper.raid:one",
  roster_version: 2,
  requested_after_sequence: 0,
  archive: {
    schema: "trnm.nakama.research-session.archive.v1",
    logical_session_id: "paper.raid:one",
    roster_version: 2,
    after_sequence: 0,
    next_after_sequence: 0,
    has_more: true,
  },
}, nakamaSessions.get("paper.raid:one")), /invalid_nakama_archive_cursor/);

const passphrase = "paper-raid-browser-gate-passphrase";
const created = await context.createEncryptedBundle(passphrase);
assert.equal(created.bundle.schema, "hepta.paper_raid.human_key_bundle.v1");
assert.equal(created.bundle.kdf.iterations, 310000);
assert.equal(created.bundle.cipher.name, "AES-GCM");
assert.equal(created.signer.privateKey.extractable, false);

const restored = await context.decryptBundle(
  JSON.parse(JSON.stringify(created.bundle)),
  passphrase,
);
assert.equal(restored.privateKey.extractable, false);
assert.equal(restored.publicKeyBase64, created.signer.publicKeyBase64);

const challenge = webcrypto.getRandomValues(new Uint8Array(96));
const publicKey = await webcrypto.subtle.importKey(
  "raw",
  Uint8Array.from(atob(restored.publicKeyBase64), character => character.charCodeAt(0)),
  { name: "Ed25519" },
  false,
  ["verify"],
);
for (const signer of [created.signer, restored]) {
  const signature = await webcrypto.subtle.sign("Ed25519", signer.privateKey, challenge);
  assert.equal(
    await webcrypto.subtle.verify("Ed25519", publicKey, signature, challenge),
    true,
  );
}

await assert.rejects(
  context.decryptBundle(created.bundle, "wrong-paper-raid-passphrase"),
);

console.log("paper-raid-bff browser crypto and same-key recovery gate: ok");
