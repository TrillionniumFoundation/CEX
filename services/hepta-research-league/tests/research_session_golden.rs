use std::{collections::HashMap, fs, path::PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use hepta_research_league::{
    paper_raid_contracts::{
        research_session_action_fingerprint, research_session_action_signing_bytes,
        research_session_archive_hash, research_session_authorization_claim_frame,
        research_session_authorization_signing_bytes, research_session_completion_signing_bytes,
        research_session_event_facts_frame, research_session_event_hash,
        research_session_event_root_from_commitments, research_session_roster_frame,
        research_session_roster_root, research_session_terminal_facts_frame, sha256_digest,
        verify_research_session_action, verify_research_session_authorization,
        verify_research_session_completion_against_archive, ResearchSessionActionV1,
        ResearchSessionCompletionV1, ResearchSessionEventCommitmentV1, ResearchSessionEventV1,
        ResearchSessionRosterMemberV1, ResearchSessionTerminalFactsV1,
        SignedResearchSessionAuthorizationV1,
    },
    SecurityConfig,
};
use serde::Deserialize;

const FIXTURE_BYTES: &[u8] = include_bytes!(
    "../../../docs/sdk-fixtures/trnm-nakama-research-session-golden-vectors-v1.json"
);
const FIXTURE_SHA256: &str =
    "sha256:f62094ca55f4772c0d9ef8db48ed11f9954e11ed989d5e3542a657baab2efa89";

#[derive(Debug, Deserialize)]
struct KeyVector {
    public_key_base64: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationVector {
    value: SignedResearchSessionAuthorizationV1,
    claim_frame_hex: String,
    signing_frame_hex: String,
}

#[derive(Debug, Deserialize)]
struct RosterVector {
    version: u64,
    entries: Vec<ResearchSessionRosterMemberV1>,
    frame_hex: String,
    root: String,
}

#[derive(Debug, Deserialize)]
struct ActionVector {
    value: ResearchSessionActionV1,
    signing_frame_hex: String,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct NegativeActionPair {
    action_type: String,
    payload_type: String,
}

#[derive(Debug, Deserialize)]
struct EventVector {
    value: ResearchSessionEventV1,
    facts_frame_hex: String,
}

#[derive(Debug, Deserialize)]
struct EventMerkleVector {
    commitments: Vec<ResearchSessionEventCommitmentV1>,
    root: String,
}

#[derive(Debug, Deserialize)]
struct ArchiveVector {
    hash: String,
}

#[derive(Debug, Deserialize)]
struct TerminalFactsVector {
    value: ResearchSessionTerminalFactsV1,
    frame_hex: String,
}

#[derive(Debug, Deserialize)]
struct CompletionVector {
    value: ResearchSessionCompletionV1,
    signing_frame_hex: String,
}

#[derive(Debug, Deserialize)]
struct GoldenFixture {
    schema: String,
    keys: HashMap<String, KeyVector>,
    authorizations: Vec<AuthorizationVector>,
    roster: RosterVector,
    actions: Vec<ActionVector>,
    negative_action_pairs: Vec<NegativeActionPair>,
    sealed_events: Vec<EventVector>,
    event_merkle: EventMerkleVector,
    archive: ArchiveVector,
    terminal_facts: TerminalFactsVector,
    completion: CompletionVector,
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("fixture hex"))
        .collect()
}

fn verifying_key(vector: &KeyVector) -> VerifyingKey {
    let bytes: [u8; 32] = BASE64
        .decode(&vector.public_key_base64)
        .expect("fixture key base64")
        .try_into()
        .expect("fixture key length");
    VerifyingKey::from_bytes(&bytes).expect("fixture Ed25519 key")
}

#[test]
fn rust_independently_verifies_the_canonical_nakama_fixture() {
    assert_eq!(sha256_digest(FIXTURE_BYTES), FIXTURE_SHA256);
    let fixture: GoldenFixture = serde_json::from_slice(FIXTURE_BYTES).expect("golden fixture");
    assert_eq!(
        fixture.schema,
        "trnm.nakama.research_session.golden_vectors.v1"
    );
    assert_eq!(fixture.authorizations.len(), 3);
    assert_eq!(fixture.sealed_events.len(), 11);

    let issuer = verifying_key(fixture.keys.get("issuer").expect("issuer key"));
    for authorization in &fixture.authorizations {
        assert_eq!(
            research_session_authorization_claim_frame(&authorization.value.claim).unwrap(),
            decode_hex(&authorization.claim_frame_hex)
        );
        assert_eq!(
            research_session_authorization_signing_bytes(
                &authorization.value.claim,
                &authorization.value.issuer_key_id,
            )
            .unwrap(),
            decode_hex(&authorization.signing_frame_hex)
        );
        verify_research_session_authorization(&authorization.value, &issuer).unwrap();
    }

    let first_claim = &fixture.authorizations[0].value.claim;
    assert_eq!(
        research_session_roster_frame(
            &first_claim.session_id,
            &first_claim.team_id,
            &first_claim.paper_project_id,
            fixture.roster.version,
            &fixture.roster.entries,
        )
        .unwrap(),
        decode_hex(&fixture.roster.frame_hex)
    );
    assert_eq!(
        research_session_roster_root(
            &first_claim.session_id,
            &first_claim.team_id,
            &first_claim.paper_project_id,
            fixture.roster.version,
            &fixture.roster.entries,
        )
        .unwrap(),
        fixture.roster.root
    );

    for action in &fixture.actions {
        assert_eq!(
            research_session_action_signing_bytes(&action.value).unwrap(),
            decode_hex(&action.signing_frame_hex)
        );
        assert_eq!(
            research_session_action_fingerprint(&action.value).unwrap(),
            action.fingerprint
        );
        let key_name = format!("agent_{}", action.value.participant_slot);
        verify_research_session_action(
            &action.value,
            &verifying_key(fixture.keys.get(&key_name).expect("agent key")),
        )
        .unwrap();
    }
    for negative in &fixture.negative_action_pairs {
        let mut invalid = fixture.actions[0].value.clone();
        invalid.action_type.clone_from(&negative.action_type);
        invalid.payload_type.clone_from(&negative.payload_type);
        assert!(research_session_action_signing_bytes(&invalid).is_err());
    }

    let events: Vec<_> = fixture
        .sealed_events
        .iter()
        .map(|vector| vector.value.clone())
        .collect();
    for event in &fixture.sealed_events {
        assert_eq!(
            research_session_event_facts_frame(&event.value).unwrap(),
            decode_hex(&event.facts_frame_hex)
        );
        assert_eq!(
            research_session_event_hash(&event.value).unwrap(),
            event.value.event_hash
        );
    }
    assert_eq!(
        research_session_event_root_from_commitments(&fixture.event_merkle.commitments).unwrap(),
        fixture.event_merkle.root
    );
    assert_eq!(
        research_session_archive_hash(&events).unwrap(),
        fixture.archive.hash
    );
    assert_eq!(
        research_session_terminal_facts_frame(&fixture.terminal_facts.value).unwrap(),
        decode_hex(&fixture.terminal_facts.frame_hex)
    );
    assert_eq!(
        research_session_completion_signing_bytes(&fixture.completion.value).unwrap(),
        decode_hex(&fixture.completion.signing_frame_hex)
    );

    let authority = verifying_key(fixture.keys.get("authority").expect("authority key"));
    verify_research_session_completion_against_archive(
        &fixture.completion.value,
        &events,
        &authority,
    )
    .unwrap();
    SecurityConfig::new("operator-test", "nakama-test")
        .with_trusted_nakama_research_authority(
            "nakama-retiring-overlap-v0",
            SigningKey::from_bytes(&[0x44; 32])
                .verifying_key()
                .to_bytes(),
        )
        .unwrap()
        .with_trusted_nakama_research_authority(
            fixture.completion.value.authority_key_id.clone(),
            authority.to_bytes(),
        )
        .unwrap()
        .verify_nakama_research_completion(&fixture.completion.value, &events)
        .unwrap();

    let mut tampered_archive = events.clone();
    tampered_archive[0].occurred_at_unix += 1;
    assert!(verify_research_session_completion_against_archive(
        &fixture.completion.value,
        &tampered_archive,
        &authority,
    )
    .is_err());
    let untrusted = SecurityConfig::new("operator-test", "nakama-test");
    assert!(untrusted
        .verify_nakama_research_completion(&fixture.completion.value, &events)
        .is_err());
}

#[test]
fn vendored_fixture_is_byte_identical_to_nakama_when_sibling_is_present() {
    let canonical = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../trillionnium-nakama/contracts/research-session-golden-vectors.json");
    if canonical.exists() {
        assert_eq!(
            fs::read(canonical).expect("read Nakama canonical fixture"),
            FIXTURE_BYTES
        );
    }
}
