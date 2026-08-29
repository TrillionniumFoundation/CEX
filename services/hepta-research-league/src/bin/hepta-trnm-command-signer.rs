use std::{fs, io::Read, process::ExitCode};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;
use hepta_research_league::trnm_v1::{
    AuthorityRole, ExternalKey, ResearchCommandV1, SignedResearchCommandV1,
};
use serde::{Deserialize, Serialize};

const INPUT_PROTOCOL_V1: &str = "hepta_trnm_command_signing_input_v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SigningInputV1 {
    protocol: String,
    chain_id: String,
    command_id: String,
    nonce: u64,
    command: ResearchCommandV1,
}

#[derive(Serialize)]
struct SigningOutputV1 {
    protocol: &'static str,
    signed_command: SignedResearchCommandV1,
}

fn run() -> Result<(), String> {
    let input_path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: hepta-trnm-command-signer <input.json|->".to_string())?;
    let input_json = if input_path == "-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| format!("read signing input from stdin: {error}"))?;
        input
    } else {
        fs::read_to_string(&input_path)
            .map_err(|error| format!("read signing input {input_path}: {error}"))?
    };
    let input: SigningInputV1 = serde_json::from_str(&input_json)
        .map_err(|error| format!("decode Hepta signing input: {error}"))?;
    if input.protocol != INPUT_PROTOCOL_V1 {
        return Err(format!(
            "unsupported signing input protocol: {}",
            input.protocol
        ));
    }
    let signer_did = std::env::var("HEPTA_TRNM_SIGNER_DID")
        .map_err(|_| "HEPTA_TRNM_SIGNER_DID is required".to_string())?;
    let seed = std::env::var("HEPTA_TRNM_ED25519_PRIVATE_KEY_BASE64")
        .map_err(|_| "HEPTA_TRNM_ED25519_PRIVATE_KEY_BASE64 is required".to_string())
        .and_then(|value| {
            BASE64
                .decode(value)
                .map_err(|error| format!("decode Hepta signing seed: {error}"))
        })?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| "Hepta signing seed must contain exactly 32 bytes".to_string())?;
    let command_id = ExternalKey::from_external_id("hepta.command", &input.command_id)
        .map_err(|error| format!("invalid command_id: {error}"))?;
    let signed_command = SignedResearchCommandV1::sign(
        input.chain_id,
        command_id,
        signer_did,
        AuthorityRole::HeptaAuthority,
        input.nonce,
        input.command,
        &SigningKey::from_bytes(&seed),
    )
    .map_err(|error| format!("sign Hepta research command: {error}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&SigningOutputV1 {
            protocol: "hepta_signed_trnm_command_v1",
            signed_command,
        })
        .map_err(|error| format!("encode signed command: {error}"))?
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Hepta TRNM command signer failed closed: {error}");
            ExitCode::FAILURE
        }
    }
}
