use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, SigningKey, Verifier};
use hepta_research_league::{
    nakama_authorization_signing_bytes, sign_nakama_match_authorization, NakamaAuthorizationClaimV1,
};

// This is Nakama's language-neutral authorization golden vector. Keeping the
// expected Go-generated Ed25519 signature here catches framing, field-order,
// digest-decoding, endian, and JSON base64 drift without a sibling dependency.
#[test]
fn emits_the_canonical_nakama_signed_authorization_golden_vector() {
    let seed = <[u8; 32]>::try_from((0_u8..=31).collect::<Vec<_>>())
        .expect("32-byte deterministic test seed");
    let signing_key = SigningKey::from_bytes(&seed);
    let claim = NakamaAuthorizationClaimV1 {
        schema: "trnm.match.authorization.v1".to_string(),
        authorization_id: "auth-golden-001".to_string(),
        match_id: "match-golden-001".to_string(),
        challenge_id: "challenge-golden-001".to_string(),
        agent_id: "agent-golden-001".to_string(),
        agent_did: "did:trnm:agent-golden-001".to_string(),
        agent_key_id: "agent-key-golden-001".to_string(),
        agent_public_key: "Kay64UG8yvCyLhqU000LxzYeUm0L/hLIl5S8kyKWbdc=".to_string(),
        subject_user_id: "user-golden-001".to_string(),
        participant_slot: 1,
        role: "challenger".to_string(),
        ruleset_hash: "sha256:8784917cf11d7b7b832d1fc4756fe4bafc98a937f73af975748be5f0a9b386d3"
            .to_string(),
        dataset_hash: "sha256:8e68d28e325776de5181125323ebcb57f6fcce140cbcd2624055516b5be45ab2"
            .to_string(),
        challenge_snapshot_hash:
            "sha256:35c7960a73bb62199ce463ca0dc8dd26e81e8656217221f27608364b0b1cf987".to_string(),
        issued_at_unix: 1_800_000_000,
        expires_at_unix: 1_800_003_600,
    };
    let signed =
        sign_nakama_match_authorization(claim.clone(), "issuer-key-golden-001", &signing_key)
            .expect("canonical authorization signs");

    assert_eq!(
        signed.signature,
        "ODDi1QuKNlOnykERx29Kkk7ORAlMAGn4MrS2mjLvIkstfEpdpOWp55/PJ6LZjZfUCD+BuzbJAhUWUnkerU+iCA=="
    );
    assert_eq!(
        BASE64.encode(signing_key.verifying_key().to_bytes()),
        "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg="
    );
    let signing_bytes = nakama_authorization_signing_bytes(&claim, "issuer-key-golden-001")
        .expect("canonical signing bytes");
    let signature = Signature::from_slice(
        &BASE64
            .decode(&signed.signature)
            .expect("golden signature base64"),
    )
    .expect("64-byte signature");
    signing_key
        .verifying_key()
        .verify(&signing_bytes, &signature)
        .expect("golden signature verifies");
    assert_eq!(
        serde_json::to_value(signed).expect("authorization JSON"),
        serde_json::json!({
            "claim": {
                "schema": "trnm.match.authorization.v1",
                "authorization_id": "auth-golden-001",
                "match_id": "match-golden-001",
                "challenge_id": "challenge-golden-001",
                "agent_id": "agent-golden-001",
                "agent_did": "did:trnm:agent-golden-001",
                "agent_key_id": "agent-key-golden-001",
                "agent_public_key": "Kay64UG8yvCyLhqU000LxzYeUm0L/hLIl5S8kyKWbdc=",
                "subject_user_id": "user-golden-001",
                "participant_slot": 1,
                "role": "challenger",
                "ruleset_hash": "sha256:8784917cf11d7b7b832d1fc4756fe4bafc98a937f73af975748be5f0a9b386d3",
                "dataset_hash": "sha256:8e68d28e325776de5181125323ebcb57f6fcce140cbcd2624055516b5be45ab2",
                "challenge_snapshot_hash": "sha256:35c7960a73bb62199ce463ca0dc8dd26e81e8656217221f27608364b0b1cf987",
                "issued_at_unix": 1800000000_i64,
                "expires_at_unix": 1800003600_i64
            },
            "issuer_key_id": "issuer-key-golden-001",
            "signature": "ODDi1QuKNlOnykERx29Kkk7ORAlMAGn4MrS2mjLvIkstfEpdpOWp55/PJ6LZjZfUCD+BuzbJAhUWUnkerU+iCA=="
        })
    );
}
