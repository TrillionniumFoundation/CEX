//! Consensus identity, signed command, quorum, proof, and receipt wire types.

pub mod crypto;
pub mod protocol;

pub use crypto::{decode_hash32, hash_domain, Hash32};
pub use protocol::*;
