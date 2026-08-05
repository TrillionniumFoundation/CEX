use super::*;

const INTEGRATION_MANIFEST_SHA256: &str =
    "9c2234c2c677307b262faa6be52a9855958003212fa0733e3c991af86df1555d";

fn integration_bundle() -> NeutralArtifactBundleV1 {
    serde_json::from_str(include_str!(
        "../../../docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json"
    ))
    .expect("vendored Integration bundle must parse as the exact neutral contract")
}

fn request_for(bundle: NeutralArtifactBundleV1) -> CreateArtifactManifestRequest {
    let expected_source_manifest_sha256 =
        neutral_bundle_sha256(&bundle).expect("canonical neutral digest");
    let storage_locations = bundle
        .objects
        .iter()
        .map(|object| ArtifactReference {
            logical_path: object.logical_path.clone(),
            sha256: object.sha256.clone(),
            uri: format!("cas://sha256/{}", object.sha256),
            acl: ArtifactAcl::Team,
        })
        .collect();
    CreateArtifactManifestRequest {
        manifest_id: Uuid::from_u128(0x9033_0000_0000_4000_8000_0000_0000_0001),
        expected_paper_version: 1,
        expected_source_manifest_sha256,
        source_bundle: bundle,
        storage_locations,
        idempotency_key: "integration-artifact-fixture".to_string(),
    }
}

#[test]
fn exact_integration_fixture_parses_and_hashes_without_trusting_its_digest() {
    let bundle = integration_bundle();
    assert_eq!(
        neutral_bundle_sha256(&bundle).expect("canonical neutral digest"),
        INTEGRATION_MANIFEST_SHA256
    );
    let request = request_for(bundle);
    let (_, independently_computed) =
        validate_artifact_manifest_request(&request).expect("exact Integration bundle validates");
    assert_eq!(independently_computed, INTEGRATION_MANIFEST_SHA256);
}

#[test]
fn logical_integration_challenge_is_not_an_implicit_uuid_alias() {
    let bundle = integration_bundle();
    let paper_challenge_id = Uuid::from_u128(0x9033_0000_0000_4000_8000_0000_0000_0002);
    let error = validate_artifact_challenge_id(&bundle.challenge_id, paper_challenge_id)
        .expect_err("logical challenge must not bind to a UUID paper challenge");
    assert_eq!(error.code, "artifact_challenge_mismatch");
}

#[test]
fn uuid_challenge_variant_uses_the_same_canonical_algorithm_and_binds() {
    let paper_challenge_id = Uuid::from_u128(0x9033_0000_0000_4000_8000_0000_0000_0003);
    let mut bundle = integration_bundle();
    bundle.challenge_id = paper_challenge_id.to_string();
    let request = request_for(bundle);
    let (_, digest) = validate_artifact_manifest_request(&request).expect("UUID variant validates");
    assert_eq!(digest, request.expected_source_manifest_sha256);
    validate_artifact_challenge_id(&request.source_bundle.challenge_id, paper_challenge_id)
        .expect("exact UUID challenge binds");
}

#[test]
fn neutral_manifest_tamper_and_unsafe_locations_fail_closed() {
    let mut duplicate_path = integration_bundle();
    duplicate_path.objects[1].logical_path = duplicate_path.objects[0].logical_path.clone();
    let error = validate_artifact_manifest_request(&request_for(duplicate_path))
        .expect_err("duplicate logical paths must fail");
    assert_eq!(error.code, "noncanonical_artifact_object_order");

    let mut unsafe_uri = request_for(integration_bundle());
    unsafe_uri.storage_locations[0].uri = "https://user@example.invalid/object".to_string();
    let error = validate_artifact_manifest_request(&unsafe_uri)
        .expect_err("credential-bearing storage URI must fail");
    assert_eq!(error.code, "unsafe_artifact_uri");
}

#[test]
fn author_approval_and_terminal_phases_freeze_every_collaboration_mutation() {
    let mutations = [
        CollaborationMutation::ArtifactManifest,
        CollaborationMutation::Evidence,
        CollaborationMutation::Citation,
        CollaborationMutation::ExperimentPlan,
        CollaborationMutation::Run,
        CollaborationMutation::Figure,
        CollaborationMutation::Claim,
        CollaborationMutation::SectionDraft,
        CollaborationMutation::SectionReview,
    ];
    for phase in [
        PaperPhase::AuthorApproval,
        PaperPhase::IntegrityHold,
        PaperPhase::SubmissionReady,
    ] {
        for mutation in mutations {
            assert!(
                !mutation.allowed(phase),
                "{mutation:?} leaked through {phase:?}"
            );
        }
    }
    assert!(CollaborationMutation::ArtifactManifest.allowed(PaperPhase::Preregistering));
    assert!(CollaborationMutation::ExperimentPlan.allowed(PaperPhase::Researching));
    assert!(CollaborationMutation::Run.allowed(PaperPhase::Experimenting));
    assert!(CollaborationMutation::SectionDraft.allowed(PaperPhase::Drafting));
    assert!(CollaborationMutation::SectionReview.allowed(PaperPhase::IntegrityReview));
}
