"use strict";

let csrfToken = null;
let humanSigner = null;

const KEY_BUNDLE_SCHEMA = "hepta.paper_raid.human_key_bundle.v1";
const KEY_BUNDLE_AAD = new TextEncoder().encode(KEY_BUNDLE_SCHEMA);
const KEY_BUNDLE_ITERATIONS = 310000;

function bytesToBase64(value) {
  const bytes = value instanceof Uint8Array ? value : new Uint8Array(value);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(value) {
  if (typeof value !== "string" || value.length > 100000) throw new Error("invalid_base64");
  const binary = atob(value);
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
  if (bytesToBase64(bytes) !== value) throw new Error("non_canonical_base64");
  return bytes;
}

function updateHumanKeyStatus(message) {
  for (const output of document.querySelectorAll(".human-key-status")) output.textContent = message;
}

async function deriveBundleKey(passphrase, salt) {
  const material = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(passphrase),
    "PBKDF2",
    false,
    ["deriveKey"]
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations: KEY_BUNDLE_ITERATIONS },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

async function nonExtractableSigner(privatePkcs8, publicRaw) {
  const privateKey = await crypto.subtle.importKey(
    "pkcs8",
    privatePkcs8,
    { name: "Ed25519" },
    false,
    ["sign"]
  );
  const publicKey = await crypto.subtle.importKey(
    "raw",
    publicRaw,
    { name: "Ed25519" },
    false,
    ["verify"]
  );
  const probe = crypto.getRandomValues(new Uint8Array(32));
  const signature = await crypto.subtle.sign("Ed25519", privateKey, probe);
  if (!await crypto.subtle.verify("Ed25519", publicKey, signature, probe)) {
    throw new Error("key_bundle_public_private_mismatch");
  }
  return Object.freeze({ privateKey, publicKeyBase64: bytesToBase64(publicRaw) });
}

async function createEncryptedBundle(passphrase) {
  if (!crypto.subtle || typeof crypto.subtle.generateKey !== "function") {
    throw new Error("webcrypto_unavailable");
  }
  const generated = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const publicRaw = new Uint8Array(await crypto.subtle.exportKey("raw", generated.publicKey));
  const privatePkcs8 = new Uint8Array(await crypto.subtle.exportKey("pkcs8", generated.privateKey));
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  try {
    const key = await deriveBundleKey(passphrase, salt);
    const encrypted = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv, additionalData: KEY_BUNDLE_AAD, tagLength: 128 },
      key,
      privatePkcs8
    );
    const signer = await nonExtractableSigner(privatePkcs8, publicRaw);
    const bundle = {
      schema: KEY_BUNDLE_SCHEMA,
      public_key: signer.publicKeyBase64,
      kdf: {
        name: "PBKDF2",
        hash: "SHA-256",
        iterations: KEY_BUNDLE_ITERATIONS,
        salt: bytesToBase64(salt)
      },
      cipher: {
        name: "AES-GCM",
        tag_length: 128,
        iv: bytesToBase64(iv)
      },
      encrypted_pkcs8: bytesToBase64(encrypted),
      created_at_unix: Math.floor(Date.now() / 1000)
    };
    return { bundle, signer };
  } finally {
    privatePkcs8.fill(0);
  }
}

function exactKeys(value, expected) {
  if (!value || Array.isArray(value) || typeof value !== "object") return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

async function decryptBundle(bundle, passphrase) {
  if (!exactKeys(bundle, ["schema", "public_key", "kdf", "cipher", "encrypted_pkcs8", "created_at_unix"]) ||
      bundle.schema !== KEY_BUNDLE_SCHEMA ||
      !exactKeys(bundle.kdf, ["name", "hash", "iterations", "salt"]) ||
      bundle.kdf.name !== "PBKDF2" || bundle.kdf.hash !== "SHA-256" ||
      bundle.kdf.iterations !== KEY_BUNDLE_ITERATIONS ||
      !exactKeys(bundle.cipher, ["name", "tag_length", "iv"]) ||
      bundle.cipher.name !== "AES-GCM" || bundle.cipher.tag_length !== 128) {
    throw new Error("unsupported_or_malformed_key_bundle");
  }
  const publicRaw = base64ToBytes(bundle.public_key);
  const salt = base64ToBytes(bundle.kdf.salt);
  const iv = base64ToBytes(bundle.cipher.iv);
  const encrypted = base64ToBytes(bundle.encrypted_pkcs8);
  if (publicRaw.length !== 32 || salt.length !== 16 || iv.length !== 12 || encrypted.length > 4096) {
    throw new Error("invalid_key_bundle_lengths");
  }
  const key = await deriveBundleKey(passphrase, salt);
  const decrypted = new Uint8Array(await crypto.subtle.decrypt(
    { name: "AES-GCM", iv, additionalData: KEY_BUNDLE_AAD, tagLength: 128 },
    key,
    encrypted
  ));
  try {
    return await nonExtractableSigner(decrypted, publicRaw);
  } finally {
    decrypted.fill(0);
  }
}

function downloadBundle(bundle) {
  const blob = new Blob([`${JSON.stringify(bundle, null, 2)}\n`], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `hepta-paper-raid-human-key-${Date.now()}.json`;
  link.rel = "noopener";
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function uuid() {
  return crypto.randomUUID();
}

function show(output, value, ok = true) {
  if (output) {
    output.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
    output.classList.toggle("result-ok", ok);
    output.classList.toggle("result-error", !ok);
  }
  const toast = document.querySelector("#toast");
  if (toast) {
    toast.textContent = typeof value === "string" ? value : JSON.stringify(value);
    toast.hidden = false;
    window.setTimeout(() => { toast.hidden = true; }, 5000);
  }
}

async function responseValue(response) {
  const text = await response.text();
  if (!text) return null;
  try { return JSON.parse(text); } catch (_) { return { error: "non_json_response" }; }
}

async function refreshCsrf() {
  const response = await fetch("/session/refresh", {
    method: "POST",
    credentials: "same-origin",
    headers: { "accept": "application/json" }
  });
  if (response.status === 401) {
    window.location.assign("/login");
    throw new Error("authentication_required");
  }
  const value = await responseValue(response);
  if (!response.ok || !value || typeof value.csrf !== "string") {
    throw new Error(value && value.error ? value.error : "csrf_refresh_failed");
  }
  csrfToken = value.csrf;
  return csrfToken;
}

async function mutation(url, init) {
  let networkFailure = false;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    if (!csrfToken) await refreshCsrf();
    const headers = new Headers(init.headers || {});
    headers.set("x-paper-raid-csrf", csrfToken);
    try {
      const response = await fetch(url, {
        ...init,
        headers,
        credentials: "same-origin"
      });
      const rotated = response.headers.get("x-paper-raid-csrf");
      if (rotated) csrfToken = rotated;
      if (response.status === 401) {
        window.location.assign("/login");
        return response;
      }
      if (response.status === 409 && attempt === 0) {
        csrfToken = null;
        await refreshCsrf();
        continue;
      }
      return response;
    } catch (error) {
      networkFailure = true;
      if (attempt === 0) {
        csrfToken = null;
        await refreshCsrf();
        continue;
      }
      throw error;
    }
  }
  throw new Error(networkFailure ? "network_retry_exhausted" : "mutation_retry_exhausted");
}

async function sendCommand(command, resourceId, childId, payload) {
  const supplied = typeof payload.idempotency_key === "string" ? payload.idempotency_key : "";
  const idempotencyKey = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(supplied)
    ? supplied
    : uuid();
  payload.idempotency_key = idempotencyKey;
  const envelope = {
    command,
    resource_id: resourceId || null,
    child_id: childId || null,
    session_id: null,
    idempotency_key: idempotencyKey,
    payload
  };
  return mutation("/api/hepta/commands", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify(envelope)
  });
}

async function recordProductEvent(eventName, values = {}) {
  return mutation("/api/product-events", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify({
      event_id: uuid(),
      event_name: eventName,
      challenge_id: values.challengeId || null,
      team_id: values.teamId || null,
      paper_id: values.paperId || null,
      phase: values.phase || null
    })
  });
}

async function sha256Label(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return `sha256:${Array.from(digest, byte => byte.toString(16).padStart(2, "0")).join("")}`;
}

function bindLogin() {
  const form = document.querySelector("#login-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const input = form.elements.login_key;
    const loginKey = input.value;
    input.value = "";
    form.querySelector("button").disabled = true;
    try {
      const response = await fetch("/alpha/login", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({ login_key: loginKey })
      });
      const value = await responseValue(response);
      if (!response.ok) throw new Error(value && value.error ? value.error : "login_failed");
      csrfToken = value.csrf;
      window.location.assign("/league/start");
    } catch (error) {
      show(output, error.message, false);
      form.querySelector("button").disabled = false;
    }
  });
}

function bindHumanKeyCreate() {
  const form = document.querySelector("#human-key-create-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const button = form.querySelector("button");
    const passphraseInput = form.elements.passphrase;
    const confirmInput = form.elements.confirm_passphrase;
    const passphrase = passphraseInput.value;
    button.disabled = true;
    try {
      if (humanSigner) {
        throw new Error("forget_current_in_memory_key_before_generating_another");
      }
      if (passphrase.length < 16 || passphrase !== confirmInput.value) {
        throw new Error("passphrases_must_match_and_contain_at_least_16_characters");
      }
      const created = await createEncryptedBundle(passphrase);
      humanSigner = created.signer;
      downloadBundle(created.bundle);
      updateHumanKeyStatus(`In-memory key loaded: ${humanSigner.publicKeyBase64.slice(0, 16)}…`);
      show(output, "Encrypted bundle downloaded. Keep it safe, then register this same in-memory key in Step 2.", true);
    } catch (error) {
      show(output, error.message, false);
    } finally {
      passphraseInput.value = "";
      confirmInput.value = "";
      button.disabled = false;
    }
  });
}

function bindHumanKeyRegistration() {
  const form = document.querySelector("#human-key-register-form");
  if (!form) return;
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const output = form.querySelector("output");
    const button = form.querySelector("button");
    let registrationSubmitted = false;
    button.disabled = true;
    try {
      if (!humanSigner) {
        throw new Error("generate_a_key_or_import_the_original_encrypted_bundle_first");
      }
      const challengeResponse = await mutation("/api/onboarding/human/challenge", {
        method: "POST",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({ signing_public_key: humanSigner.publicKeyBase64 })
      });
      const challenge = await responseValue(challengeResponse);
      if (!challengeResponse.ok) throw new Error(challenge && challenge.error ? challenge.error : "human_challenge_failed");
      if (!challenge || challenge.signing_public_key !== humanSigner.publicKeyBase64) {
        throw new Error("human_challenge_key_mismatch");
      }
      const signature = await crypto.subtle.sign(
        "Ed25519",
        humanSigner.privateKey,
        base64ToBytes(challenge.signing_bytes)
      );
      registrationSubmitted = true;
      const registerResponse = await mutation("/api/onboarding/human/register", {
        method: "POST",
        headers: { "content-type": "application/json", "accept": "application/json" },
        body: JSON.stringify({
          signing_public_key: humanSigner.publicKeyBase64,
          idempotency_key: challenge.idempotency_key,
          issued_at_unix: challenge.issued_at_unix,
          expires_at_unix: challenge.expires_at_unix,
          key_proof_signature: bytesToBase64(signature)
        })
      });
      const value = await responseValue(registerResponse);
      if (!registerResponse.ok) throw new Error(value && value.error ? value.error : "human_registration_failed");
      const recovered = registerResponse.headers.get("x-paper-raid-registration-recovered") === "self-read";
      show(output, recovered ? "Previously committed registration recovered by self-read. Redirecting…" : "Human key registered. Redirecting…", true);
      window.setTimeout(() => window.location.assign("/league/start"), 700);
    } catch (error) {
      const recovery = registrationSubmitted
        ? " Registration may already be committed: refresh /league/start first. If it is still unregistered, import the original bundle and retry with the same key; do not generate a replacement key."
        : "";
      show(output, `${error.message}.${recovery}`, false);
    } finally {
      button.disabled = false;
    }
  });
}

function bindAgentBinding() {
  for (const form of document.querySelectorAll(".agent-binding-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const expected = [
          "binding_id", "player_id", "agent_id", "agent_key_id", "agent_public_key",
          "agent_proof_nonce", "agent_proof_issued_at_unix", "agent_proof_expires_at_unix",
          "agent_proof_signature", "idempotency_key"
        ];
        if (!exactKeys(payload, expected)) throw new Error("agent_binding_payload_must_have_exact_fields");
        if (payload.player_id !== form.dataset.playerId) throw new Error("agent_binding_player_id_mismatch");
        if (payload.agent_proof_nonce !== payload.idempotency_key) throw new Error("agent_proof_nonce_must_equal_idempotency_key");
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
        if (!uuidPattern.test(payload.binding_id) || !uuidPattern.test(payload.idempotency_key)) {
          throw new Error("binding_and_idempotency_ids_must_be_canonical_uuids");
        }
        if (typeof payload.agent_key_id !== "string" || !/^sha256:[0-9a-f]{64}$/.test(payload.agent_key_id)) {
          throw new Error("agent_key_id_must_be_sha256_of_public_key_bytes");
        }
        const agentPublicKey = base64ToBytes(payload.agent_public_key);
        const agentProofSignature = base64ToBytes(payload.agent_proof_signature);
        if (agentPublicKey.length !== 32 || agentProofSignature.length !== 64 ||
            await sha256Label(agentPublicKey) !== payload.agent_key_id) {
          throw new Error("agent_binding_public_proof_is_not_canonical_ed25519");
        }
        if (!Number.isSafeInteger(payload.agent_proof_issued_at_unix) ||
            !Number.isSafeInteger(payload.agent_proof_expires_at_unix) ||
            payload.agent_proof_issued_at_unix < 0 ||
            payload.agent_proof_expires_at_unix <= payload.agent_proof_issued_at_unix ||
            payload.agent_proof_expires_at_unix - payload.agent_proof_issued_at_unix > 600) {
          throw new Error("agent_binding_proof_interval_must_be_at_most_ten_minutes");
        }
        const response = await sendCommand("create_agent_binding", null, null, payload);
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.assign("/league/start"), 800);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindAgentRotation() {
  for (const form of document.querySelectorAll(".agent-rotation-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const expected = [
          "rotation_id", "expected_binding_version", "agent_id", "old_agent_key_id",
          "old_agent_public_key", "new_agent_key_id", "new_agent_public_key",
          "issued_at_unix", "expires_at_unix", "old_key_signature", "new_key_signature",
          "idempotency_key"
        ];
        if (!exactKeys(payload, expected)) throw new Error("agent_rotation_payload_must_have_exact_fields");
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
        if (!uuidPattern.test(payload.rotation_id) || !uuidPattern.test(payload.idempotency_key)) {
          throw new Error("rotation_and_idempotency_ids_must_be_canonical_uuids");
        }
        if (!Number.isSafeInteger(payload.expected_binding_version) ||
            payload.expected_binding_version < 1 ||
            payload.expected_binding_version !== Number(form.dataset.bindingVersion)) {
          throw new Error("agent_rotation_expected_version_mismatch");
        }
        if (payload.agent_id !== form.dataset.agentId ||
            payload.old_agent_key_id !== form.dataset.oldKeyId ||
            payload.old_agent_public_key !== form.dataset.oldPublicKey) {
          throw new Error("agent_rotation_old_binding_mismatch");
        }
        if (payload.new_agent_public_key === payload.old_agent_public_key) {
          throw new Error("agent_rotation_must_change_public_key");
        }
        const oldPublicKey = base64ToBytes(payload.old_agent_public_key);
        const newPublicKey = base64ToBytes(payload.new_agent_public_key);
        const oldSignature = base64ToBytes(payload.old_key_signature);
        const newSignature = base64ToBytes(payload.new_key_signature);
        if (oldPublicKey.length !== 32 || newPublicKey.length !== 32 ||
            oldSignature.length !== 64 || newSignature.length !== 64 ||
            await sha256Label(oldPublicKey) !== payload.old_agent_key_id ||
            await sha256Label(newPublicKey) !== payload.new_agent_key_id) {
          throw new Error("agent_rotation_key_id_hash_mismatch");
        }
        if (!Number.isSafeInteger(payload.issued_at_unix) ||
            !Number.isSafeInteger(payload.expires_at_unix) || payload.issued_at_unix < 0 ||
            payload.expires_at_unix <= payload.issued_at_unix ||
            payload.expires_at_unix - payload.issued_at_unix > 600) {
          throw new Error("agent_rotation_interval_must_be_at_most_ten_minutes");
        }
        const response = await sendCommand(
          "rotate_agent_binding_key",
          form.dataset.bindingId,
          null,
          payload
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindHumanKeyImport() {
  for (const form of document.querySelectorAll(".human-key-import-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      const fileInput = form.elements.key_bundle;
      const passphraseInput = form.elements.passphrase;
      const passphrase = passphraseInput.value;
      button.disabled = true;
      try {
        const file = fileInput.files[0];
        if (!file || file.size < 1 || file.size > 65536 || passphrase.length < 16) {
          throw new Error("key_bundle_or_passphrase_is_invalid");
        }
        const bundle = JSON.parse(await file.text());
        humanSigner = await decryptBundle(bundle, passphrase);
        const message = `In-memory key loaded: ${humanSigner.publicKeyBase64.slice(0, 16)}…`;
        updateHumanKeyStatus(message);
        show(output, message, true);
      } catch (error) {
        humanSigner = null;
        updateHumanKeyStatus("No in-memory key / 当前无内存密钥");
        show(output, error.message, false);
      } finally {
        passphraseInput.value = "";
        fileInput.value = "";
        button.disabled = false;
      }
    });
  }
  for (const button of document.querySelectorAll(".forget-human-key")) {
    button.addEventListener("click", () => {
      humanSigner = null;
      updateHumanKeyStatus("No in-memory key / 当前无内存密钥");
      show(button.parentElement.querySelector(".human-key-status"), "In-memory key forgotten", true);
    });
  }
}

function unsignedHumanPayload(command, payload) {
  const clean = { ...payload };
  for (const field of [
    "idempotency_key", "signature", "signing_key_id", "signing_public_key",
    "signing_public_key_hash", "signed_at_unix", "accepted_at_unix", "player_id",
    "appellant_player_id"
  ]) delete clean[field];
  if (command === "accept_research_team_membership") delete clean.agent_id;
  return clean;
}

async function signHumanPayload(command, resourceId, childId, payload) {
  if (!humanSigner) throw new Error("import_your_encrypted_human_key_bundle_first");
  const frameResponse = await mutation("/api/onboarding/human/signing-frame", {
    method: "POST",
    headers: { "content-type": "application/json", "accept": "application/json" },
    body: JSON.stringify({
      command,
      resource_id: resourceId || null,
      child_id: childId || null,
      payload: unsignedHumanPayload(command, payload)
    })
  });
  const frame = await responseValue(frameResponse);
  if (!frameResponse.ok) throw new Error(frame && frame.error ? frame.error : "signing_frame_failed");
  if (frame.signing_public_key !== humanSigner.publicKeyBase64) {
    throw new Error("imported_key_is_not_the_active_registered_human_key");
  }
  const signature = await crypto.subtle.sign(
    "Ed25519",
    humanSigner.privateKey,
    base64ToBytes(frame.signing_bytes)
  );
  frame.payload.signature = bytesToBase64(signature);
  return frame;
}

function bindLocalSigning() {
  for (const button of document.querySelectorAll(".local-sign")) {
    const form = button.closest("form");
    button.addEventListener("click", async () => {
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        const child = form.elements.child_id ? form.elements.child_id.value.trim() : null;
        const frame = await signHumanPayload(
          form.dataset.command,
          form.dataset.resourceId || null,
          child || null,
          payload
        );
        form.elements.payload.value = JSON.stringify(frame.payload, null, 2);
        show(output, `Locally signed with ${frame.signing_key_id}`, true);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindFirstPlayableFormation() {
  for (const form of document.querySelectorAll(".materialize-team-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const response = await sendCommand("materialize_team_proposal", form.dataset.proposalId, null, {
          expected_proposal_version: Number(form.dataset.proposalVersion)
        });
        const value = await responseValue(response);
        show(output, response.ok ? "Team built. Loading ready check…" : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".team-ready-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const payload = {
          acceptance_id: uuid(),
          expected_team_version: Number(form.dataset.teamVersion),
          roster_version: Number(form.dataset.rosterVersion),
          participant_slot: Number(form.dataset.participantSlot),
          binding_id: form.dataset.bindingId,
          role: form.dataset.role,
          collaboration_compact_hash: form.dataset.compactHash
        };
        const frame = await signHumanPayload(
          "accept_research_team_membership",
          form.dataset.teamId,
          null,
          payload
        );
        const response = await sendCommand(
          "accept_research_team_membership",
          form.dataset.teamId,
          null,
          frame.payload
        );
        const value = await responseValue(response);
        show(output, response.ok ? "Ready confirmed." : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".lock-team-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const response = await sendCommand("lock_research_team", form.dataset.teamId, null, {
          expected_version: Number(form.dataset.teamVersion)
        });
        const value = await responseValue(response);
        show(output, response.ok ? "Roster locked. Name the Raid next." : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 400);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
  for (const form of document.querySelectorAll(".create-paper-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const button = form.querySelector("button");
      const output = form.querySelector("output");
      button.disabled = true;
      try {
        const paperId = uuid();
        const response = await sendCommand("create_paper_project", null, null, {
          paper_project_id: paperId,
          team_id: form.dataset.teamId,
          title: form.elements.title.value.trim(),
          target_format: form.elements.target_format.value
        });
        const value = await responseValue(response);
        const createdId = value && typeof value.paper_project_id === "string"
          ? value.paper_project_id
          : paperId;
        show(output, response.ok ? "Paper created. Entering the Research Room…" : value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.assign(`/league/papers/${encodeURIComponent(createdId)}`), 300);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindQueueForms() {
  for (const form of document.querySelectorAll(".queue-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const roles = form.elements.roles.value.split(",").map(value => value.trim()).filter(Boolean);
        if (roles.length < 1 || roles.length > 5 || new Set(roles).size !== roles.length) {
          throw new Error("roles_must_be_1_to_5_distinct_values");
        }
        const availability = new TextEncoder().encode(form.elements.availability.value);
        const payload = {
          ticket_id: uuid(),
          challenge_id: form.dataset.challengeId,
          requested_team_size: 3,
          roles,
          availability_hash: await sha256Label(availability)
        };
        const response = await sendCommand("queue_matchmaking", null, null, payload);
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindProposalForms() {
  for (const form of document.querySelectorAll(".proposal-decision")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = event.submitter;
      if (!button) return;
      for (const candidate of form.querySelectorAll("button")) candidate.disabled = true;
      try {
        const response = await sendCommand("decide_team_proposal", form.dataset.proposalId, null, {
          decision_id: uuid(),
          expected_proposal_version: Number(form.dataset.proposalVersion),
          decision: button.value
        });
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 900);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        for (const candidate of form.querySelectorAll("button")) candidate.disabled = false;
      }
    });
  }
}

function bindCommandForms() {
  for (const form of document.querySelectorAll(".command-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const payload = JSON.parse(form.elements.payload.value);
        if (!payload || Array.isArray(payload) || typeof payload !== "object") {
          throw new Error("payload_must_be_a_json_object");
        }
        const child = form.elements.child_id ? form.elements.child_id.value.trim() : null;
        const response = await sendCommand(
          form.dataset.command,
          form.dataset.resourceId || null,
          child || null,
          payload
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
        if (response.ok) window.setTimeout(() => window.location.reload(), 1000);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindArtifactForms() {
  for (const form of document.querySelectorAll(".artifact-form")) {
    form.addEventListener("submit", async event => {
      event.preventDefault();
      const output = form.querySelector("output");
      const button = form.querySelector("button");
      button.disabled = true;
      try {
        const file = form.elements.artifact.files[0];
        if (!file || file.size === 0 || file.size > 32 * 1024 * 1024) {
          throw new Error("artifact_size_must_be_1_to_33554432_bytes");
        }
        const bytes = await file.arrayBuffer();
        const digest = await sha256Label(bytes);
        const response = await mutation(
          `/api/papers/${encodeURIComponent(form.dataset.paperId)}/artifacts/${encodeURIComponent(digest)}`,
          {
            method: "PUT",
            headers: {
              "content-type": form.elements.media_type.value,
              "accept": "application/json"
            },
            body: file
          }
        );
        const value = await responseValue(response);
        show(output, value, response.ok);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindTimeline() {
  for (const button of document.querySelectorAll(".timeline-refresh")) {
    button.addEventListener("click", async () => {
      const output = button.parentElement.querySelector("output");
      button.disabled = true;
      try {
        await recordProductEvent("replay_started", { paperId: button.dataset.paperId });
        const response = await fetch(`/api/papers/${encodeURIComponent(button.dataset.paperId)}/timeline?after_cursor=0&after_sequence=0`, {
          credentials: "same-origin",
          headers: { "accept": "application/json" }
        });
        const value = await responseValue(response);
        show(output, value, response.ok);
      } catch (error) {
        show(output, error.message, false);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function bindProductTelemetry() {
  for (const link of document.querySelectorAll(".continue-raid-link")) {
    link.addEventListener("click", async event => {
      event.preventDefault();
      const href = link.href;
      try {
        await recordProductEvent("continue_opened", {
          teamId: link.dataset.teamId || null,
          paperId: link.dataset.paperId || null
        });
      } catch (_) {
        // Telemetry must never block the player's authoritative Continue path.
      }
      window.location.assign(href);
    });
  }
  const paperMatch = window.location.pathname.match(/^\/league\/papers\/([0-9a-f-]{36})$/i);
  const navigation = performance.getEntriesByType("navigation")[0];
  if (paperMatch && navigation && navigation.type === "reload") {
    recordProductEvent("reconnected", { paperId: paperMatch[1] }).catch(() => {});
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  bindLogin();
  if (document.body.dataset.authenticated === "true") {
    try { await refreshCsrf(); } catch (_) { return; }
  }
  bindHumanKeyCreate();
  bindHumanKeyRegistration();
  bindHumanKeyImport();
  bindAgentBinding();
  bindAgentRotation();
  bindLocalSigning();
  bindQueueForms();
  bindProposalForms();
  bindFirstPlayableFormation();
  bindCommandForms();
  bindArtifactForms();
  bindTimeline();
  bindProductTelemetry();
});
