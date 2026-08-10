import {
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign as ed25519Sign,
  verify as ed25519Verify,
} from "node:crypto";
import { readSafeFile, readSafeJson, writePrivateJsonExclusive } from "./files.mjs";
import {
  assertLogicalId,
  decodeCanonicalBase64,
  sha256Digest,
} from "./canonical.mjs";

export const IDENTITY_SCHEMA = "hepta.paper_raid.agent_bridge.identity.v1";

function publicRaw(privateKey) {
  const jwk = createPublicKey(privateKey).export({ format: "jwk" });
  if (jwk.kty !== "OKP" || jwk.crv !== "Ed25519" || typeof jwk.x !== "string") {
    throw new Error("identity is not an Ed25519 key");
  }
  const raw = Buffer.from(jwk.x, "base64url");
  if (raw.length !== 32) throw new Error("Ed25519 public key must contain 32 bytes");
  return raw;
}

function identityDocument(agentId, privateKey) {
  assertLogicalId(agentId, "agent_id");
  if (privateKey.asymmetricKeyType !== "ed25519") {
    throw new Error("private key must be Ed25519");
  }
  const raw = publicRaw(privateKey);
  const keyId = sha256Digest(raw);
  return {
    schema: IDENTITY_SCHEMA,
    agent_id: agentId,
    agent_key_id: keyId,
    agent_public_key: raw.toString("base64"),
    agent_public_key_hash: keyId,
    private_key_pkcs8: privateKey
      .export({ format: "der", type: "pkcs8" })
      .toString("base64"),
    created_at_unix: Math.floor(Date.now() / 1000),
  };
}

function privateKeyFromDocument(document) {
  if (
    !document ||
    Array.isArray(document) ||
    document.schema !== IDENTITY_SCHEMA
  ) {
    throw new Error("unsupported Agent identity file schema");
  }
  assertLogicalId(document.agent_id, "agent_id");
  const pkcs8 = decodeCanonicalBase64(
    document.private_key_pkcs8,
    undefined,
    "private_key_pkcs8",
  );
  const privateKey = createPrivateKey({ key: pkcs8, format: "der", type: "pkcs8" });
  if (privateKey.asymmetricKeyType !== "ed25519") {
    throw new Error("identity private key is not Ed25519");
  }
  const raw = publicRaw(privateKey);
  const publicKey = decodeCanonicalBase64(
    document.agent_public_key,
    32,
    "agent_public_key",
  );
  const keyId = sha256Digest(raw);
  if (
    !raw.equals(publicKey) ||
    document.agent_key_id !== keyId ||
    document.agent_public_key_hash !== keyId
  ) {
    throw new Error("Agent identity public and private key material does not match");
  }
  return { privateKey, raw, keyId };
}

export async function generateIdentity(agentId, outputPath) {
  const { privateKey } = generateKeyPairSync("ed25519");
  const document = identityDocument(agentId, privateKey);
  await writePrivateJsonExclusive(outputPath, document);
  return publicDescription(document);
}

export async function importIdentity(agentId, sourcePath, outputPath) {
  const source = await readSafeFile(sourcePath, {
    privateFile: true,
    maxBytes: 64 * 1024,
  });
  let privateKey;
  try {
    const document = JSON.parse(source.toString("utf8"));
    if (document.schema === IDENTITY_SCHEMA) {
      const parsed = privateKeyFromDocument(document);
      privateKey = parsed.privateKey;
      agentId ??= document.agent_id;
    }
  } catch (error) {
    if (error instanceof SyntaxError) {
      // PEM and DER are tried below.
    } else {
      throw error;
    }
  }
  if (!privateKey) {
    try {
      privateKey = createPrivateKey(source.toString("utf8"));
    } catch {
      privateKey = createPrivateKey({ key: source, format: "der", type: "pkcs8" });
    }
  }
  const document = identityDocument(agentId, privateKey);
  await writePrivateJsonExclusive(outputPath, document);
  return publicDescription(document);
}

export async function loadIdentity(path) {
  const document = await readSafeJson(path, {
    privateFile: true,
    maxBytes: 64 * 1024,
  });
  const { privateKey } = privateKeyFromDocument(document);
  return Object.freeze({
    ...publicDescription(document),
    sign(bytes) {
      const signature = ed25519Sign(null, Buffer.from(bytes), privateKey);
      if (signature.length !== 64) throw new Error("Ed25519 signature length is invalid");
      return signature.toString("base64");
    },
    verify(bytes, signature) {
      return ed25519Verify(
        null,
        Buffer.from(bytes),
        createPublicKey(privateKey),
        decodeCanonicalBase64(signature, 64, "signature"),
      );
    },
  });
}

export function publicDescription(document) {
  return Object.freeze({
    schema: document.schema,
    agent_id: document.agent_id,
    agent_key_id: document.agent_key_id,
    agent_public_key: document.agent_public_key,
    agent_public_key_hash: document.agent_public_key_hash,
    created_at_unix: document.created_at_unix,
  });
}
