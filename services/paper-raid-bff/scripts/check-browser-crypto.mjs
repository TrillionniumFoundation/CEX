import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const browserUrl = new URL("../src/browser.js", import.meta.url);
const source = await readFile(browserUrl, "utf8");

assert.equal(source.includes("localStorage"), false);
assert.equal(source.includes("sessionStorage"), false);
assert.equal(source.includes("indexedDB"), false);
assert.equal(source.includes(".style"), false);

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
  window: {
    location: { assign() {} },
    setTimeout,
  },
});
vm.runInContext(source, context, { filename: browserUrl.pathname });

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
