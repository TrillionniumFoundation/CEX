use anyhow::{anyhow, ensure, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub type Hash32 = [u8; 32];

pub fn hash_domain(domain: &str, parts: &[&[u8]]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update(b"trnm.domain.hash.v1");
    put_len_prefixed(&mut hasher, domain.as_bytes());
    for part in parts {
        put_len_prefixed(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn put_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

pub fn put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

pub fn put_str(out: &mut Vec<u8>, value: &str) {
    put_bytes(out, value.as_bytes());
}

pub fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub fn decode_hash32(label: &str, value: &str) -> Result<Hash32> {
    let bytes = hex::decode(value).map_err(|_| anyhow!("{label} must be lowercase hex"))?;
    ensure!(bytes.len() == 32, "{label} must encode exactly 32 bytes");
    ensure!(
        hex::encode(&bytes) == value,
        "{label} must use canonical lowercase hex"
    );
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn decode_signature(label: &str, value: &str) -> Result<Signature> {
    let bytes = hex::decode(value).map_err(|_| anyhow!("{label} must be lowercase hex"))?;
    ensure!(bytes.len() == 64, "{label} must encode exactly 64 bytes");
    ensure!(
        hex::encode(&bytes) == value,
        "{label} must use canonical lowercase hex"
    );
    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(Signature::from_bytes(&out))
}

pub fn verifying_key_from_hex(value: &str) -> Result<VerifyingKey> {
    let bytes = decode_hash32("public_key_hex", value)?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|_| anyhow!("public_key_hex is not a valid Ed25519 key"))
}

pub fn signing_key_from_hex(value: &str) -> Result<SigningKey> {
    let bytes = decode_hash32("private_key_hex", value)?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

pub fn sign_hex(signing_key: &SigningKey, message: &[u8]) -> String {
    hex::encode(signing_key.sign(message).to_bytes())
}

pub fn verify_hex(public_key_hex: &str, message: &[u8], signature_hex: &str) -> Result<()> {
    let key = verifying_key_from_hex(public_key_hex)?;
    let signature = decode_signature("signature_hex", signature_hex)?;
    key.verify(message, &signature)
        .map_err(|_| anyhow!("Ed25519 signature verification failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_hash_is_framed_and_domain_separated() {
        assert_ne!(hash_domain("a", &[b"bc"]), hash_domain("a", &[b"b", b"c"]));
        assert_ne!(hash_domain("a", &[b"x"]), hash_domain("b", &[b"x"]));
    }

    #[test]
    fn ed25519_roundtrip_rejects_tamper() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let signature = sign_hex(&key, b"payload");
        verify_hex(&public_key_hex(&key), b"payload", &signature).unwrap();
        assert!(verify_hex(&public_key_hex(&key), b"tampered", &signature).is_err());
    }
}
