//! Minimal, node-independent verification for TRNM finality receipts.

use anyhow::{anyhow, ensure, Result};
use trnm_finality_types::{
    decode_hash32, hash_domain, FinalityReceiptV1, Hash32, MerkleProofV1, ValidatorSetV1,
    FINALITY_RECEIPT_SCHEMA_V1,
};

fn merkle_parent(tree_domain: &str, left: &Hash32, right: &Hash32) -> Hash32 {
    hash_domain(
        "trnm.merkle.parent.v1",
        &[tree_domain.as_bytes(), left, right],
    )
}

fn verify_proof(expected_root: &Hash32, proof: &MerkleProofV1) -> Result<()> {
    ensure!(
        proof.leaf_count > 0,
        "Merkle proof leaf_count must be positive"
    );
    ensure!(
        proof.leaf_index < proof.leaf_count,
        "Merkle proof leaf_index is out of range"
    );
    let mut current = decode_hash32("Merkle proof leaf_hash_hex", &proof.leaf_hash_hex)?;
    for step in &proof.steps {
        let sibling = decode_hash32("Merkle proof sibling_hash_hex", &step.sibling_hash_hex)?;
        current = if step.sibling_on_left {
            merkle_parent(&proof.tree_domain, &sibling, &current)
        } else {
            merkle_parent(&proof.tree_domain, &current, &sibling)
        };
    }
    ensure!(current == *expected_root, "Merkle proof root mismatch");
    Ok(())
}

pub fn verify_finality_receipt(
    receipt: &FinalityReceiptV1,
    validator_set: &ValidatorSetV1,
) -> Result<()> {
    ensure!(
        receipt.schema == FINALITY_RECEIPT_SCHEMA_V1,
        "unsupported finality receipt schema"
    );
    ensure!(
        receipt.block_header.chain_id == receipt.chain_id,
        "receipt chain_id does not match block header"
    );
    if let Some(fingerprint) = &receipt.domain_command_fingerprint_hex {
        let _ = decode_hash32("domain_command_fingerprint_hex", fingerprint)?;
    }
    receipt.block_header.validate()?;
    ensure!(
        receipt.block_header.height == receipt.block_height,
        "receipt block height does not match header"
    );
    ensure!(
        receipt.block_header.state_root_hex == receipt.state_root_hex,
        "receipt state root does not match header"
    );
    ensure!(
        receipt.block_header.transaction_root_hex == receipt.transaction_root_hex,
        "receipt transaction root does not match header"
    );
    ensure!(
        receipt.block_header.validator_set_id == receipt.validator_set_id,
        "receipt validator_set_id does not match header"
    );
    ensure!(
        hex::encode(receipt.block_header.block_hash()?) == receipt.block_hash_hex,
        "receipt block hash does not match header"
    );
    ensure!(
        receipt.quorum_certificate.height == receipt.block_height
            && receipt.quorum_certificate.block_hash_hex == receipt.block_hash_hex,
        "receipt quorum certificate does not bind the block"
    );
    receipt
        .quorum_certificate
        .verify(&receipt.chain_id, validator_set)?;

    let transaction_leaf = hash_domain(
        "trnm.transaction.leaf.v1",
        &[receipt.transaction_hash_hex.as_bytes()],
    );
    ensure!(
        receipt.transaction_inclusion_proof.leaf_hash_hex == hex::encode(transaction_leaf),
        "transaction inclusion proof leaf does not bind transaction hash"
    );
    ensure!(
        receipt.transaction_inclusion_proof.leaf_index == receipt.transaction_index,
        "transaction inclusion proof index mismatch"
    );
    verify_proof(
        &decode_hash32("transaction_root_hex", &receipt.transaction_root_hex)?,
        &receipt.transaction_inclusion_proof,
    )?;

    match (&receipt.object_ref, &receipt.object_inclusion_proof) {
        (Some(object_ref), Some(proof)) => {
            let leaf = hash_domain(
                "trnm.state.object.leaf.v1",
                &[
                    object_ref.object_key_hex.as_bytes(),
                    object_ref.object_type.as_bytes(),
                    &object_ref.version.to_be_bytes(),
                    object_ref.value_hash_hex.as_bytes(),
                ],
            );
            ensure!(
                proof.leaf_hash_hex == hex::encode(leaf),
                "object inclusion proof leaf does not bind object_ref"
            );
            verify_proof(
                &decode_hash32("state_root_hex", &receipt.state_root_hex)?,
                proof,
            )?;
        }
        (None, None) => {}
        _ => return Err(anyhow!("object_ref and object proof presence mismatch")),
    }

    ensure!(
        receipt.receipt_hash_hex == hex::encode(receipt.compute_receipt_hash()?),
        "receipt_hash_hex mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use trnm_finality_types::{
        BlockHeaderV1, FinalityReceiptV1, MerkleProofV1, QuorumCertificateV1,
        ValidatorDescriptorV1, ValidatorSetV1, ValidatorVoteV1, BLOCK_HEADER_SCHEMA_V1,
        FINALITY_RECEIPT_SCHEMA_V1, VALIDATOR_VOTE_SCHEMA_V1,
    };

    use super::*;

    fn public_key_hex(signing_key: &SigningKey) -> String {
        hex::encode(signing_key.verifying_key().to_bytes())
    }

    fn sign_hex(signing_key: &SigningKey, message: &[u8]) -> String {
        hex::encode(signing_key.sign(message).to_bytes())
    }

    fn fixture() -> (FinalityReceiptV1, ValidatorSetV1) {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let validator_set = ValidatorSetV1 {
            validator_set_id: "validators-v1".to_string(),
            validators: vec![ValidatorDescriptorV1 {
                validator_id: "validator-1".to_string(),
                public_key_hex: public_key_hex(&signing_key),
                vote_endpoint: "http://127.0.0.1:39001/v1/vote".to_string(),
                voting_power: 1,
            }],
            quorum_power: 1,
        };
        let transaction_hash_hex = hex::encode(hash_domain("test.transaction.v1", &[b"tx"]));
        let transaction_leaf = hash_domain(
            "trnm.transaction.leaf.v1",
            &[transaction_hash_hex.as_bytes()],
        );
        let transaction_root_hex = hex::encode(transaction_leaf);
        let state_root_hex = hex::encode(hash_domain("test.state.v1", &[b"state"]));
        let header = BlockHeaderV1 {
            schema: BLOCK_HEADER_SCHEMA_V1.to_string(),
            chain_id: "trnm-test-chain".to_string(),
            height: 1,
            previous_block_hash_hex: hex::encode([0u8; 32]),
            transaction_root_hex: transaction_root_hex.clone(),
            state_root_hex: state_root_hex.clone(),
            validator_set_id: validator_set.validator_set_id.clone(),
            timestamp_unix_ms: 1,
        };
        let block_hash_hex = hex::encode(header.block_hash().unwrap());
        let vote = ValidatorVoteV1 {
            schema: VALIDATOR_VOTE_SCHEMA_V1.to_string(),
            validator_id: "validator-1".to_string(),
            validator_set_id: validator_set.validator_set_id.clone(),
            height: 1,
            block_hash_hex: block_hash_hex.clone(),
            public_key_hex: public_key_hex(&signing_key),
            signature_hex: sign_hex(
                &signing_key,
                &ValidatorVoteV1::signing_bytes(
                    &header.chain_id,
                    &validator_set.validator_set_id,
                    1,
                    &block_hash_hex,
                ),
            ),
        };
        let mut receipt = FinalityReceiptV1 {
            schema: FINALITY_RECEIPT_SCHEMA_V1.to_string(),
            chain_id: header.chain_id.clone(),
            command_id: "command-1".to_string(),
            transaction_hash_hex,
            domain_command_fingerprint_hex: None,
            block_height: 1,
            transaction_index: 0,
            block_hash_hex: block_hash_hex.clone(),
            transaction_root_hex,
            state_root_hex,
            validator_set_id: validator_set.validator_set_id.clone(),
            block_header: header,
            quorum_certificate: QuorumCertificateV1 {
                validator_set_id: validator_set.validator_set_id.clone(),
                height: 1,
                block_hash_hex,
                signatures: vec![vote],
            },
            transaction_inclusion_proof: MerkleProofV1 {
                tree_domain: "trnm.transactions.v1".to_string(),
                leaf_index: 0,
                leaf_count: 1,
                leaf_hash_hex: hex::encode(transaction_leaf),
                steps: Vec::new(),
            },
            object_ref: None,
            object_inclusion_proof: None,
            receipt_hash_hex: String::new(),
        };
        receipt.receipt_hash_hex = hex::encode(receipt.compute_receipt_hash().unwrap());
        (receipt, validator_set)
    }

    #[test]
    fn verifies_minimal_single_transaction_receipt() {
        let (receipt, validator_set) = fixture();
        verify_finality_receipt(&receipt, &validator_set).unwrap();
    }

    #[test]
    fn rejects_tampered_receipt_root() {
        let (mut receipt, validator_set) = fixture();
        receipt.transaction_root_hex = hex::encode([9u8; 32]);
        assert!(verify_finality_receipt(&receipt, &validator_set).is_err());
    }
}
