use sha2::{Digest, Sha256};
use thiserror::Error;

/// Stable wire name used by schemas and cross-implementation fixtures.
pub const CANONICAL_ENCODING: &str = "rfc8949-deterministic-cbor-array-v1";

/// Minimal deterministic-CBOR encoder.
///
/// Protocol values use only definite-length arrays, UTF-8 text, byte strings,
/// unsigned integers, booleans, and null. Maps and floating point values are
/// deliberately absent, eliminating map-order and float-normalization drift.
#[doc(hidden)]
#[derive(Default)]
pub struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn head(&mut self, major: u8, value: u64) {
        let prefix = major << 5;
        match value {
            0..=23 => self.bytes.push(prefix | value as u8),
            24..=0xff => {
                self.bytes.push(prefix | 24);
                self.bytes.push(value as u8);
            }
            0x100..=0xffff => {
                self.bytes.push(prefix | 25);
                self.bytes.extend_from_slice(&(value as u16).to_be_bytes());
            }
            0x1_0000..=0xffff_ffff => {
                self.bytes.push(prefix | 26);
                self.bytes.extend_from_slice(&(value as u32).to_be_bytes());
            }
            _ => {
                self.bytes.push(prefix | 27);
                self.bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
    }

    pub fn uint(&mut self, value: u64) {
        self.head(0, value);
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.head(2, value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    pub fn text(&mut self, value: &str) {
        self.head(3, value.len() as u64);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub fn array(&mut self, len: usize) {
        self.head(4, len as u64);
    }

    pub fn bool(&mut self, value: bool) {
        self.bytes.push(if value { 0xf5 } else { 0xf4 });
    }

    pub fn null(&mut self) {
        self.bytes.push(0xf6);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanonicalDecodeError {
    #[error("unexpected end of canonical CBOR")]
    UnexpectedEof,
    #[error("unexpected CBOR major type")]
    UnexpectedType,
    #[error("indefinite-length and reserved CBOR forms are forbidden")]
    ForbiddenAdditionalInfo,
    #[error("CBOR integer or length did not use its shortest form")]
    NonMinimalEncoding,
    #[error("invalid UTF-8 text")]
    InvalidUtf8,
    #[error("array length mismatch: expected {expected}, got {got}")]
    ArrayLengthMismatch { expected: usize, got: usize },
    #[error("byte string length mismatch: expected {expected}, got {got}")]
    ByteLengthMismatch { expected: usize, got: usize },
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u64),
    #[error("unknown enum discriminant {value} for {name}")]
    UnknownDiscriminant { name: &'static str, value: u64 },
    #[error("trailing bytes after canonical command")]
    TrailingBytes,
    #[error("decoded command is not byte-for-byte canonical")]
    NonCanonicalRoundTrip,
}

pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, cursor: 0 }
    }

    pub(crate) fn finish(self) -> Result<(), CanonicalDecodeError> {
        if self.cursor == self.input.len() {
            Ok(())
        } else {
            Err(CanonicalDecodeError::TrailingBytes)
        }
    }

    fn byte(&mut self) -> Result<u8, CanonicalDecodeError> {
        let byte = *self
            .input
            .get(self.cursor)
            .ok_or(CanonicalDecodeError::UnexpectedEof)?;
        self.cursor += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], CanonicalDecodeError> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or(CanonicalDecodeError::UnexpectedEof)?;
        let bytes = self
            .input
            .get(self.cursor..end)
            .ok_or(CanonicalDecodeError::UnexpectedEof)?;
        self.cursor = end;
        Ok(bytes)
    }

    fn head(&mut self) -> Result<(u8, u64), CanonicalDecodeError> {
        let first = self.byte()?;
        let major = first >> 5;
        let additional = first & 0x1f;
        let value = match additional {
            0..=23 => additional as u64,
            24 => {
                let value = self.byte()? as u64;
                if value < 24 {
                    return Err(CanonicalDecodeError::NonMinimalEncoding);
                }
                value
            }
            25 => {
                let value = u16::from_be_bytes(
                    self.take(2)?
                        .try_into()
                        .map_err(|_| CanonicalDecodeError::UnexpectedEof)?,
                ) as u64;
                if value <= 0xff {
                    return Err(CanonicalDecodeError::NonMinimalEncoding);
                }
                value
            }
            26 => {
                let value = u32::from_be_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| CanonicalDecodeError::UnexpectedEof)?,
                ) as u64;
                if value <= 0xffff {
                    return Err(CanonicalDecodeError::NonMinimalEncoding);
                }
                value
            }
            27 => {
                let value = u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| CanonicalDecodeError::UnexpectedEof)?,
                );
                if value <= 0xffff_ffff {
                    return Err(CanonicalDecodeError::NonMinimalEncoding);
                }
                value
            }
            _ => return Err(CanonicalDecodeError::ForbiddenAdditionalInfo),
        };
        Ok((major, value))
    }

    pub(crate) fn uint(&mut self) -> Result<u64, CanonicalDecodeError> {
        let (major, value) = self.head()?;
        if major != 0 {
            return Err(CanonicalDecodeError::UnexpectedType);
        }
        Ok(value)
    }

    pub(crate) fn array(&mut self, expected: usize) -> Result<(), CanonicalDecodeError> {
        let got = self.array_len()?;
        if got != expected {
            return Err(CanonicalDecodeError::ArrayLengthMismatch { expected, got });
        }
        Ok(())
    }

    pub(crate) fn array_len(&mut self) -> Result<usize, CanonicalDecodeError> {
        let (major, value) = self.head()?;
        if major != 4 {
            return Err(CanonicalDecodeError::UnexpectedType);
        }
        let got = usize::try_from(value).map_err(|_| CanonicalDecodeError::UnexpectedType)?;
        Ok(got)
    }

    pub(crate) fn bytes(&mut self) -> Result<&'a [u8], CanonicalDecodeError> {
        let (major, value) = self.head()?;
        if major != 2 {
            return Err(CanonicalDecodeError::UnexpectedType);
        }
        let len = usize::try_from(value).map_err(|_| CanonicalDecodeError::UnexpectedType)?;
        self.take(len)
    }

    pub(crate) fn bytes_exact<const N: usize>(&mut self) -> Result<[u8; N], CanonicalDecodeError> {
        let bytes = self.bytes()?;
        bytes
            .try_into()
            .map_err(|_| CanonicalDecodeError::ByteLengthMismatch {
                expected: N,
                got: bytes.len(),
            })
    }

    pub(crate) fn text(&mut self) -> Result<String, CanonicalDecodeError> {
        let (major, value) = self.head()?;
        if major != 3 {
            return Err(CanonicalDecodeError::UnexpectedType);
        }
        let len = usize::try_from(value).map_err(|_| CanonicalDecodeError::UnexpectedType)?;
        let text =
            std::str::from_utf8(self.take(len)?).map_err(|_| CanonicalDecodeError::InvalidUtf8)?;
        Ok(text.to_string())
    }

    pub(crate) fn bool(&mut self) -> Result<bool, CanonicalDecodeError> {
        match self.byte()? {
            0xf4 => Ok(false),
            0xf5 => Ok(true),
            _ => Err(CanonicalDecodeError::UnexpectedType),
        }
    }

    pub(crate) fn option_digest(&mut self) -> Result<Option<[u8; 32]>, CanonicalDecodeError> {
        if self.input.get(self.cursor) == Some(&0xf6) {
            self.cursor += 1;
            Ok(None)
        } else {
            self.bytes_exact().map(Some)
        }
    }

    pub(crate) fn consume_null(&mut self) -> bool {
        if self.input.get(self.cursor) == Some(&0xf6) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
}

/// Implemented explicitly by every consensus-facing protocol value.
pub trait CanonicalCbor {
    #[doc(hidden)]
    fn encode_canonical(&self, encoder: &mut Encoder);

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut encoder = Encoder::default();
        self.encode_canonical(&mut encoder);
        encoder.finish()
    }

    fn canonical_hash(&self, domain: &str) -> [u8; 32] {
        canonical_hash(domain, &self.canonical_bytes())
    }
}

/// SHA-256 with unambiguous domain and message length framing.
pub fn canonical_hash(domain: &str, canonical_bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"TRNM_RESEARCH_PROTOCOL_HASH_V1\0");
    hasher.update((domain.len() as u32).to_be_bytes());
    hasher.update(domain.as_bytes());
    hasher.update((canonical_bytes.len() as u64).to_be_bytes());
    hasher.update(canonical_bytes);
    hasher.finalize().into()
}

pub(crate) fn encode_option_digest(encoder: &mut Encoder, value: &Option<[u8; 32]>) {
    match value {
        Some(digest) => encoder.bytes(digest),
        None => encoder.null(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_use_rfc8949_shortest_forms() {
        let mut encoder = Encoder::default();
        encoder.array(6);
        for value in [23, 24, 255, 256, 65_535, 65_536] {
            encoder.uint(value);
        }
        assert_eq!(
            hex::encode(encoder.finish()),
            "8617181818ff19010019ffff1a00010000"
        );
    }

    #[test]
    fn hashing_length_frames_domain_and_message() {
        assert_ne!(canonical_hash("ab", b"c"), canonical_hash("a", b"bc"));
    }
}
