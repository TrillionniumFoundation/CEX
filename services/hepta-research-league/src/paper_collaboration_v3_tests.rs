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
fn whole_paper_revision_must_snapshot_current_merged_section_head_and_inherit_its_base() {
    let now = Utc::now();
    let paper_id = Uuid::new_v4();
    let base_revision_id = Uuid::new_v4();
    let current_revision_id = Uuid::new_v4();
    let section_revision_id = Uuid::new_v4();
    let mut paper = PaperProject {
        paper_project_id: paper_id,
        team_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        title: "Section lineage fixture".into(),
        target_format: "paper".into(),
        phase: PaperPhase::Reproducing,
        challenge_ruleset_snapshot: None,
        challenge_ruleset_snapshot_hash: None,
        deadline_at: None,
        grace_expires_at: None,
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        current_revision_id: Some(base_revision_id),
        release_candidate_revision_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let revision = |revision_id, parent_revision_id, revision_number| PaperRevision {
        revision_id,
        paper_project_id: paper_id,
        parent_revision_id,
        revision_number,
        source_manifest_hash: format!("sha256:{:064x}", revision_number),
        artifact_manifest_hash: format!("sha256:{:064x}", revision_number + 10),
        bibliography_hash: format!("sha256:{:064x}", revision_number + 20),
        claim_evidence_graph_hash: format!("sha256:{:064x}", revision_number + 30),
        section_materialization: None,
        section_materialization_root: None,
        status: PaperRevisionStatus::Draft,
        release_candidate: None,
        release_candidate_hash: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let base_revision = revision(base_revision_id, None, 1);
    let current_revision = revision(current_revision_id, Some(base_revision_id), 2);
    let head = SectionHead {
        paper_project_id: paper_id,
        section_key: "methods".into(),
        base_paper_revision_id: base_revision_id,
        current_head_revision_id: section_revision_id,
        fencing_token: 1,
        version: 3,
        updated_at: now,
    };
    let merge = SectionMerge {
        merge_id: Uuid::new_v4(),
        paper_project_id: paper_id,
        section_key: head.section_key.clone(),
        section_revision_id,
        parent_revision_id: base_revision_id,
        merged_section_revision_id: section_revision_id,
        lease_id: Uuid::new_v4(),
        fencing_token: 1,
        merged_by_player_id: Uuid::new_v4(),
        signing_key_id: "key".into(),
        signing_public_key: "public".into(),
        signing_public_key_hash: format!("sha256:{:064x}", 99),
        merged_at_unix: now.timestamp(),
        signature: "signature".into(),
        version: 1,
    };
    let empty_binding = PaperRevisionArtifactBinding {
        revision_id: current_revision_id,
        paper_project_id: paper_id,
        manifest_id: Uuid::new_v4(),
        artifact_manifest_hash: current_revision.artifact_manifest_hash.clone(),
        source_logical_path: "paper.md".into(),
        source_manifest_hash: current_revision.source_manifest_hash.clone(),
        bibliography_logical_path: "references.bib".into(),
        bibliography_hash: current_revision.bibliography_hash.clone(),
        claim_evidence_graph_logical_path: "claims.json".into(),
        claim_evidence_graph_hash: current_revision.claim_evidence_graph_hash.clone(),
        section_head_bindings: Vec::new(),
        created_at: now,
    };
    let bound = bind_current_section_heads(
        empty_binding,
        Some(base_revision_id),
        std::slice::from_ref(&base_revision),
        std::slice::from_ref(&head),
        std::slice::from_ref(&merge),
    )
    .expect("post-merge revision inherits and snapshots the authoritative section head");
    assert_eq!(
        bound.section_head_bindings,
        vec![PaperRevisionSectionHeadBinding {
            section_key: "methods".into(),
            base_paper_revision_id: base_revision_id,
            current_head_revision_id: section_revision_id,
        }]
    );

    let revisions = vec![base_revision.clone(), current_revision.clone()];
    paper.current_revision_id = Some(current_revision_id);
    assert!(paper_revision_covers_section_merges(
        &paper,
        &revisions,
        std::slice::from_ref(&bound),
        std::slice::from_ref(&head),
        &[],
        std::slice::from_ref(&merge),
    ));

    paper.current_revision_id = Some(base_revision_id);
    assert!(!paper_revision_covers_section_merges(
        &paper,
        &revisions,
        std::slice::from_ref(&bound),
        std::slice::from_ref(&head),
        &[],
        std::slice::from_ref(&merge),
    ));
    let actor_player_id = Uuid::new_v4();
    let team = ResearchTeam {
        team_id: paper.team_id,
        challenge_id: paper.challenge_id,
        collaboration_compact_hash: format!("sha256:{:064x}", 100),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: vec![TeamMember {
            participant_slot: 1,
            player_id: actor_player_id,
            binding_id: Uuid::new_v4(),
            agent_id: "did:trnm:lineage-fixture".into(),
            role: "captain".into(),
            joined_at: now,
        }],
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let stale_progress = project_author_raid_progress(
        &paper,
        &team,
        actor_player_id,
        &[],
        &revisions,
        std::slice::from_ref(&bound),
        &[],
        &None,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        std::slice::from_ref(&head),
        &[],
        &[],
        std::slice::from_ref(&merge),
    );
    assert_eq!(
        stale_progress.blockers,
        vec!["paper_revision_section_lineage_required"]
    );
    assert_eq!(stale_progress.next_actions, vec!["create_paper_revision"]);

    paper.current_revision_id = Some(current_revision_id);
    let ready_progress = project_author_raid_progress(
        &paper,
        &team,
        actor_player_id,
        &[],
        &revisions,
        std::slice::from_ref(&bound),
        &[],
        &None,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        std::slice::from_ref(&head),
        &[],
        &[],
        std::slice::from_ref(&merge),
    );
    assert!(ready_progress.blockers.is_empty());
    assert_eq!(
        ready_progress.next_actions,
        vec!["transition_paper_project"]
    );
    let mut stale_binding = bound;
    stale_binding.section_head_bindings[0].current_head_revision_id = Uuid::new_v4();
    assert!(!paper_revision_covers_section_merges(
        &paper,
        &revisions,
        std::slice::from_ref(&stale_binding),
        std::slice::from_ref(&head),
        &[],
        std::slice::from_ref(&merge),
    ));
}

#[test]
fn authoritative_author_approval_projects_victory_gates_not_a_phase_transition() {
    let now = Utc::now();
    let ruleset: crate::ChallengeRulesetV1 = serde_json::from_value(serde_json::json!({
        "schema":"hepta.challenge.ruleset.v1",
        "template":"evidence-audit",
        "duration_seconds":2700,
        "grace_seconds":900,
        "phase_gates":[
            {"transition":"preregistering_to_researching","requirements":[
                {"kind":"work_items","minimum":1},
                {"kind":"artifact_manifests","minimum":1}
            ]},
            {"transition":"researching_to_experimenting","requirements":[
                {"kind":"evidence_cards","minimum":2},
                {"kind":"citations","minimum":2},
                {"kind":"claims","minimum":2}
            ]},
            {"transition":"experimenting_to_drafting","requirements":[
                {"kind":"artifact_manifests","minimum":1}
            ]},
            {"transition":"drafting_to_integrity_review","requirements":[
                {"kind":"all_work_items_terminal","minimum":1},
                {"kind":"section_revisions","minimum":1},
                {"kind":"paper_revisions","minimum":1}
            ]},
            {"transition":"integrity_review_to_reproducing","requirements":[
                {"kind":"approving_section_reviews","minimum":1},
                {"kind":"section_merges","minimum":1}
            ]},
            {"transition":"reproducing_to_author_approval","requirements":[
                {"kind":"paper_revision_covers_section_merges","minimum":1}
            ]}
        ],
        "victory_requirements":[
            {"kind":"accepted_work_items","minimum":1},
            {"kind":"release_candidate","minimum":1},
            {"kind":"all_author_consents","minimum":1},
            {"kind":"paper_revision_covers_section_merges","minimum":1}
        ]
    }))
    .expect("typed ruleset");
    let ruleset_hash = ruleset.canonical_hash().expect("ruleset hash");
    let snapshot = crate::PaperChallengeRulesetSnapshotV1 {
        schema: crate::CHALLENGE_RULESET_SNAPSHOT_V1.into(),
        challenge_snapshot_hash: format!("sha256:{}", "44".repeat(32)),
        ruleset_version: "paper-raid-evidence-audit-v1".into(),
        ruleset_hash,
        enforcement: crate::ChallengeRulesetEnforcementV1::AuthoritativeV1,
        ruleset: Some(ruleset),
    };
    let paper_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let challenge_id = Uuid::new_v4();
    let actor_player_id = Uuid::new_v4();
    let paper = PaperProject {
        paper_project_id: paper_id,
        team_id,
        challenge_id,
        title: "Victory projection fixture".into(),
        target_format: "paper".into(),
        phase: PaperPhase::AuthorApproval,
        challenge_ruleset_snapshot_hash: Some(snapshot.canonical_hash().expect("snapshot hash")),
        challenge_ruleset_snapshot: Some(snapshot),
        deadline_at: Some(now + chrono::Duration::minutes(45)),
        grace_expires_at: Some(now + chrono::Duration::minutes(60)),
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 7,
        created_at: now,
        updated_at: now,
    };
    let roles = ["captain", "evidence", "experiment"];
    let team = ResearchTeam {
        team_id,
        challenge_id,
        collaboration_compact_hash: format!("sha256:{}", "55".repeat(32)),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: roles
            .iter()
            .enumerate()
            .map(|(index, role)| TeamMember {
                participant_slot: (index + 1) as u32,
                player_id: if index == 0 {
                    actor_player_id
                } else {
                    Uuid::new_v4()
                },
                binding_id: Uuid::new_v4(),
                agent_id: format!("did:trnm:victory-{index}"),
                role: (*role).into(),
                joined_at: now,
            })
            .collect(),
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let progress = project_author_raid_progress(
        &paper,
        &team,
        actor_player_id,
        &[],
        &[],
        &[],
        &[],
        &None,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    );
    assert_eq!(progress.next_phase, None);
    assert!(progress
        .blockers
        .iter()
        .any(|value| value.contains("release_candidate")));
    assert!(progress
        .blockers
        .iter()
        .any(|value| value.contains("all_author_consents")));
    assert!(progress
        .next_actions
        .contains(&"promote_paper_release_candidate".to_string()));
    assert!(progress
        .next_actions
        .contains(&"create_authorship_consent".to_string()));
    assert!(!progress
        .next_actions
        .contains(&"transition_paper_project".to_string()));
}

#[test]
fn legacy_paper_revision_without_materialization_fields_remains_serde_compatible() {
    let now = Utc::now();
    let value = serde_json::json!({
        "revision_id":Uuid::new_v4(),
        "paper_project_id":Uuid::new_v4(),
        "parent_revision_id":null,
        "revision_number":1,
        "source_manifest_hash":format!("sha256:{:064x}", 1),
        "artifact_manifest_hash":format!("sha256:{:064x}", 2),
        "bibliography_hash":format!("sha256:{:064x}", 3),
        "claim_evidence_graph_hash":format!("sha256:{:064x}", 4),
        "status":"draft",
        "release_candidate":null,
        "release_candidate_hash":null,
        "version":1,
        "created_at":now,
        "updated_at":now,
    });
    let revision: PaperRevision = serde_json::from_value(value).expect("legacy revision decodes");
    assert!(revision.section_materialization.is_none());
    assert!(revision.section_materialization_root.is_none());
    let encoded = serde_json::to_value(revision).expect("legacy revision re-encodes");
    assert!(encoded.get("section_materialization").is_none());
    assert!(encoded.get("section_materialization_root").is_none());
}

#[test]
fn initial_revision_has_a_canonical_empty_materialization_root() {
    let descriptor = SectionMaterializationDescriptorV1 {
        schema: SECTION_MATERIALIZATION_V1.to_string(),
        paper_project_id: Uuid::new_v4(),
        revision_id: Uuid::new_v4(),
        parent_revision_id: None,
        parent_materialization_root: None,
        sections: Vec::new(),
    };
    let root = section_materialization_root(&descriptor).expect("empty root is canonical");
    assert!(root.starts_with("sha256:") && root.len() == 71);
    assert_eq!(
        section_materialization_root(&descriptor).expect("deterministic root"),
        root
    );
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
    assert!(CollaborationMutation::SectionMerge.allowed(PaperPhase::IntegrityReview));
    assert!(!CollaborationMutation::SectionMerge.allowed(PaperPhase::AuthorApproval));
}

fn matchmaking_ticket(
    ticket: u128,
    player: u128,
    availability: &str,
    roles: &[&str],
) -> MatchmakingTicket {
    let now = Utc::now();
    MatchmakingTicket {
        ticket_id: Uuid::from_u128(ticket),
        player_id: Uuid::from_u128(player),
        challenge_id: Uuid::from_u128(0x9000),
        requested_team_size: 3,
        roles: roles.iter().map(|role| (*role).to_string()).collect(),
        availability_hash: availability.to_string(),
        status: MatchmakingTicketStatus::Queued,
        matched_proposal_id: None,
        expires_at: Some(now + chrono::Duration::minutes(30)),
        queue_hint: None,
        version: 1,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn role_assignment_backtracks_instead_of_greedily_rejecting_a_valid_team() {
    let tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain", "evidence"]),
        matchmaking_ticket(2, 12, "window", &["captain"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    assert_eq!(
        distinct_role_assignment(&tickets),
        Some(vec![
            "evidence".to_string(),
            "captain".to_string(),
            "experiment".to_string(),
        ])
    );
}

#[test]
fn matcher_requires_one_shared_availability_window() {
    let shared = select_alpha_match(
        vec![
            matchmaking_ticket(1, 11, "early", &["captain"]),
            matchmaking_ticket(2, 12, "shared", &["evidence"]),
            matchmaking_ticket(3, 13, "shared", &["experiment"]),
            matchmaking_ticket(4, 14, "shared", &["captain"]),
        ]
        .into_iter(),
    );
    assert_eq!(
        shared
            .iter()
            .map(|ticket| ticket.ticket_id)
            .collect::<Vec<_>>(),
        vec![Uuid::from_u128(2), Uuid::from_u128(3), Uuid::from_u128(4)]
    );

    let incompatible = select_alpha_match(
        vec![
            matchmaking_ticket(5, 15, "a", &["captain"]),
            matchmaking_ticket(6, 16, "b", &["evidence"]),
            matchmaking_ticket(7, 17, "c", &["experiment"]),
        ]
        .into_iter(),
    );
    assert!(incompatible.is_empty());
}

#[test]
fn matcher_waits_when_three_tickets_cannot_cover_three_distinct_roles() {
    let selected = select_alpha_match(
        vec![
            matchmaking_ticket(1, 11, "window", &["captain"]),
            matchmaking_ticket(2, 12, "window", &["captain"]),
            matchmaking_ticket(3, 13, "window", &["captain"]),
        ]
        .into_iter(),
    );
    assert!(selected.is_empty());
}

#[test]
fn matchmaking_rejects_noncanonical_role_names() {
    let request = CreateMatchmakingTicketRequest {
        ticket_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        requested_team_size: 3,
        roles: vec!["foo".to_string()],
        availability_hash:
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        idempotency_key: Uuid::new_v4().to_string(),
    };
    assert!(validate_matchmaking_ticket_request(&request).is_err());
}

#[test]
fn expired_tickets_are_not_matchable_and_project_an_honest_wait_hint() {
    let now = Utc::now();
    let mut expired = matchmaking_ticket(1, 11, "window", &["captain"]);
    expired.expires_at = Some(now - chrono::Duration::seconds(1));
    let evidence = matchmaking_ticket(2, 12, "window", &["evidence"]);
    let experiment = matchmaking_ticket(3, 13, "window", &["experiment"]);
    assert!(select_alpha_match(
        vec![expired.clone(), evidence.clone(), experiment.clone()].into_iter()
    )
    .is_empty());

    assert!(expire_matchmaking_ticket(&mut expired, now));
    assert_eq!(expired.status, MatchmakingTicketStatus::Expired);
    assert_eq!(expired.version, 2);
    assert!(!expire_matchmaking_ticket(&mut expired, now));

    let waiting = project_matchmaking_ticket(evidence, std::slice::from_ref(&experiment), now);
    let hint = waiting.queue_hint.expect("queue hint");
    assert_eq!(hint.schema, MATCHMAKING_QUEUE_HINT_SCHEMA_V1);
    assert_eq!(hint.state, "waiting");
    assert_eq!(hint.eta_seconds, None);
    assert_eq!(hint.compatible_players_needed, 1);
    assert_eq!(hint.missing_roles, vec!["captain"]);
}

#[test]
fn queue_hint_reports_hall_role_deficit_instead_of_zero_needed_players() {
    let now = Utc::now();
    let captain_a = matchmaking_ticket(1, 11, "window", &["captain"]);
    let captain_b = matchmaking_ticket(2, 12, "window", &["captain"]);
    let flexible = matchmaking_ticket(3, 13, "window", &["evidence", "experiment"]);
    let projected = project_matchmaking_ticket(captain_a, &[captain_b, flexible], now);
    let hint = projected.queue_hint.expect("queue hint");
    assert_eq!(hint.state, "waiting");
    assert_eq!(hint.compatible_pool_size, 3);
    assert_eq!(hint.compatible_players_needed, 1);
    assert!(hint.missing_roles.is_empty());
    assert_eq!(hint.message, "waiting_for_role_distribution");
    assert_eq!(hint.eta_seconds, None);
}

#[test]
fn legacy_ticket_json_derives_a_deadline_without_schema_breakage() {
    let ticket = matchmaking_ticket(1, 11, "window", &["captain"]);
    let mut value = serde_json::to_value(&ticket).expect("ticket JSON");
    value
        .as_object_mut()
        .expect("ticket object")
        .remove("expires_at");
    value
        .as_object_mut()
        .expect("ticket object")
        .remove("queue_hint");
    let decoded: MatchmakingTicket = serde_json::from_value(value).expect("legacy ticket decodes");
    assert_eq!(decoded.expires_at, None);
    assert_eq!(
        matchmaking_ticket_deadline(&decoded),
        decoded.created_at + chrono::Duration::minutes(30)
    );
}

#[test]
fn canonical_roles_enforce_duties_with_explicit_self_service_overrides() {
    let now = Utc::now();
    let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let team = ResearchTeam {
        team_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        collaboration_compact_hash:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: ["captain", "evidence", "experiment"]
            .into_iter()
            .enumerate()
            .map(|(index, role)| TeamMember {
                participant_slot: u32::try_from(index + 1).expect("slot"),
                player_id: players[index],
                binding_id: Uuid::new_v4(),
                agent_id: format!("did:trnm:role-{role}"),
                role: role.to_string(),
                joined_at: now,
            })
            .collect(),
        version: 1,
        created_at: now,
        updated_at: now,
    };
    assert!(require_author_role(&team, players[0], "captain", "phase").is_ok());
    assert_eq!(
        require_author_role(&team, players[1], "captain", "phase")
            .expect_err("Evidence cannot transition phases")
            .code,
        "author_role_duty_required"
    );
    assert!(require_captain_or_self_assignment(&team, players[1], Some(players[1])).is_ok());
    assert_eq!(
        require_captain_or_self_assignment(&team, players[1], Some(players[2]))
            .expect_err("Evidence cannot assign Experiment")
            .code,
        "author_role_duty_required"
    );

    let mut legacy = team;
    for (index, member) in legacy.members.iter_mut().enumerate() {
        member.role = format!("research-role-{index}");
    }
    assert_eq!(
        require_author_role(&legacy, players[1], "captain", "phase")
            .expect_err("noncanonical legacy teams must not bypass role duties")
            .code,
        "team_role_contract_required"
    );
}

#[test]
fn proposal_deadline_requeues_acceptors_and_expires_the_absent_player() {
    let now = Utc::now();
    let mut memory = CollaborationMemory::default();
    let mut tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let mut proposal = build_team_proposal(&tickets).expect("valid proposal");
    let eligible_player_ids = tickets.iter().map(|ticket| ticket.player_id).collect();
    proposal.expires_at = Some(now - chrono::Duration::seconds(1));
    for ticket in &mut tickets {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());
    for (index, player_id) in proposal.member_player_ids.iter().take(2).enumerate() {
        let decision = TeamProposalDecision {
            decision_id: Uuid::from_u128(100 + index as u128),
            proposal_id: proposal.proposal_id,
            player_id: *player_id,
            decision: TeamProposalDecisionKind::Accept,
            proposal_version: 2 + index as u64,
            created_at: now - chrono::Duration::seconds(2),
        };
        memory
            .team_proposal_decisions
            .insert(decision.decision_id, decision);
    }

    assert!(expire_team_proposal_memory(
        &mut memory,
        &eligible_player_ids,
        proposal.proposal_id,
        now,
    )
    .expect("deadline sweep"));
    let expired = memory
        .team_proposals
        .get(&proposal.proposal_id)
        .expect("proposal retained for audit");
    assert_eq!(expired.status, TeamProposalStatus::Expired);
    assert_eq!(expired.version, 2);
    for player_id in proposal.member_player_ids.iter().take(2) {
        let ticket = memory
            .tickets
            .values()
            .find(|ticket| ticket.player_id == *player_id)
            .expect("accepted player's ticket");
        assert_eq!(ticket.status, MatchmakingTicketStatus::Queued);
        assert_eq!(ticket.matched_proposal_id, None);
    }
    let absent = memory
        .tickets
        .values()
        .find(|ticket| ticket.player_id == proposal.member_player_ids[2])
        .expect("absent player's ticket");
    assert_eq!(absent.status, MatchmakingTicketStatus::Expired);
    assert_eq!(absent.matched_proposal_id, None);
    assert!(!expire_team_proposal_memory(
        &mut memory,
        &eligible_player_ids,
        proposal.proposal_id,
        now,
    )
    .expect("expiry is idempotent"));
}

#[test]
fn legacy_proposal_json_derives_the_five_minute_response_deadline() {
    let tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let proposal = build_team_proposal(&tickets).expect("valid proposal");
    let mut value = serde_json::to_value(&proposal).expect("proposal JSON");
    value
        .as_object_mut()
        .expect("proposal object")
        .remove("expires_at");
    let decoded: TeamProposal = serde_json::from_value(value).expect("legacy proposal decodes");
    assert_eq!(decoded.expires_at, None);
    assert_eq!(
        team_proposal_deadline(&decoded),
        decoded.created_at + chrono::Duration::minutes(5)
    );
}

#[test]
fn accepted_proposal_deadline_immediately_rematches_without_reusing_identity() {
    let now = Utc::now();
    let mut memory = CollaborationMemory::default();
    let mut tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let mut proposal = build_team_proposal(&tickets).expect("valid proposal");
    let eligible_player_ids = tickets.iter().map(|ticket| ticket.player_id).collect();
    proposal.status = TeamProposalStatus::Accepted;
    proposal.expires_at = Some(now - chrono::Duration::seconds(1));
    for (index, ticket) in tickets.iter_mut().enumerate() {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version = 2;
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
        memory.team_proposal_decisions.insert(
            Uuid::from_u128(100 + index as u128),
            TeamProposalDecision {
                decision_id: Uuid::from_u128(100 + index as u128),
                proposal_id: proposal.proposal_id,
                player_id: ticket.player_id,
                decision: TeamProposalDecisionKind::Accept,
                proposal_version: 2 + index as u64,
                created_at: now - chrono::Duration::seconds(2),
            },
        );
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());

    let sweep = expire_due_team_proposals_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_player_ids,
        proposal.challenge_id,
        now,
    )
    .expect("deadline sweep");
    assert_eq!(sweep.expired.len(), 1);
    assert_eq!(sweep.expired[0].status, TeamProposalStatus::Expired);
    assert_eq!(sweep.replacements.len(), 1);
    let replacement = memory
        .team_proposals
        .values()
        .find(|candidate| candidate.status == TeamProposalStatus::Proposed)
        .expect("compatible tickets rematched immediately");
    assert_ne!(replacement.proposal_id, proposal.proposal_id);
    assert_ne!(
        replacement.deterministic_match_key,
        proposal.deterministic_match_key
    );
    for ticket in memory.tickets.values() {
        assert_eq!(ticket.status, MatchmakingTicketStatus::Matched);
        assert_eq!(ticket.matched_proposal_id, Some(replacement.proposal_id));
        assert_eq!(ticket.version, 4);
    }
}

#[test]
fn materialized_proposal_and_consumed_tickets_survive_late_deadline_sweeps() {
    let now = Utc::now();
    let mut memory = CollaborationMemory::default();
    let mut tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let mut proposal = build_team_proposal(&tickets).expect("valid proposal");
    proposal.status = TeamProposalStatus::Materialized;
    proposal.expires_at = Some(now - chrono::Duration::minutes(1));
    proposal.version = 5;
    for ticket in &mut tickets {
        ticket.status = MatchmakingTicketStatus::Consumed;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version = 3;
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());

    let sweep = expire_due_team_proposals_memory(
        &mut memory,
        &HashSet::new(),
        &HashSet::new(),
        proposal.challenge_id,
        now,
    )
    .expect("late deadline sweep");
    assert!(sweep.expired.is_empty());
    assert!(sweep.replacements.is_empty());
    assert_eq!(
        memory
            .team_proposals
            .get(&proposal.proposal_id)
            .expect("materialized proposal retained")
            .status,
        TeamProposalStatus::Materialized
    );
    assert!(memory
        .tickets
        .values()
        .all(|ticket| ticket.status == MatchmakingTicketStatus::Consumed));
}

#[test]
fn automatic_rematch_drains_every_compatible_triplet() {
    let now = Utc::now();
    let challenge_id = Uuid::from_u128(0x9000);
    let mut memory = CollaborationMemory::default();
    for ticket in [
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["captain"]),
        matchmaking_ticket(3, 13, "window", &["evidence"]),
        matchmaking_ticket(4, 14, "window", &["evidence"]),
        matchmaking_ticket(5, 15, "window", &["experiment"]),
        matchmaking_ticket(6, 16, "window", &["experiment"]),
    ] {
        assert_eq!(ticket.challenge_id, challenge_id);
        memory.tickets.insert(ticket.ticket_id, ticket);
    }

    let eligible_player_ids = memory
        .tickets
        .values()
        .map(|ticket| ticket.player_id)
        .collect();
    let proposals = auto_match_all_queued_tickets_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_player_ids,
        challenge_id,
        now,
    )
    .expect("all compatible triplets match");
    assert_eq!(proposals.len(), 2);
    assert!(memory
        .tickets
        .values()
        .all(|ticket| ticket.status == MatchmakingTicketStatus::Matched));
    assert_eq!(
        memory
            .tickets
            .values()
            .filter_map(|ticket| ticket.matched_proposal_id)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
}

#[test]
fn automatic_match_excludes_an_older_ineligible_player() {
    let now = Utc::now();
    let challenge_id = Uuid::from_u128(0x9000);
    let mut memory = CollaborationMemory::default();
    for ticket in [
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["captain"]),
        matchmaking_ticket(3, 13, "window", &["evidence"]),
        matchmaking_ticket(4, 14, "window", &["experiment"]),
    ] {
        memory.tickets.insert(ticket.ticket_id, ticket);
    }
    let eligible_player_ids = [12_u128, 13, 14].into_iter().map(Uuid::from_u128).collect();

    let proposals = auto_match_all_queued_tickets_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_player_ids,
        challenge_id,
        now,
    )
    .expect("eligible triplet matches");

    assert_eq!(proposals.len(), 1);
    assert_eq!(
        proposals[0].member_player_ids,
        vec![
            Uuid::from_u128(12),
            Uuid::from_u128(13),
            Uuid::from_u128(14)
        ]
    );
    assert_eq!(
        memory
            .tickets
            .get(&Uuid::from_u128(1))
            .expect("ineligible ticket retained for audit")
            .status,
        MatchmakingTicketStatus::Queued
    );
}

#[test]
fn proposal_deadline_never_requeues_an_ineligible_acceptor() {
    let now = Utc::now();
    let mut memory = CollaborationMemory::default();
    let mut tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let mut proposal = build_team_proposal(&tickets).expect("valid proposal");
    proposal.status = TeamProposalStatus::Accepted;
    proposal.expires_at = Some(now - chrono::Duration::seconds(1));
    for (index, ticket) in tickets.iter_mut().enumerate() {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version = 2;
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
        let decision = TeamProposalDecision {
            decision_id: Uuid::from_u128(200 + index as u128),
            proposal_id: proposal.proposal_id,
            player_id: ticket.player_id,
            decision: TeamProposalDecisionKind::Accept,
            proposal_version: 2 + index as u64,
            created_at: now - chrono::Duration::seconds(2),
        };
        memory
            .team_proposal_decisions
            .insert(decision.decision_id, decision);
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());
    let eligible_player_ids = [Uuid::from_u128(11), Uuid::from_u128(13)]
        .into_iter()
        .collect();

    let sweep = expire_due_team_proposals_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_player_ids,
        proposal.challenge_id,
        now,
    )
    .expect("deadline sweep");

    assert_eq!(sweep.expired.len(), 1);
    assert!(sweep.replacements.is_empty());
    assert_eq!(
        memory
            .tickets
            .values()
            .find(|ticket| ticket.player_id == Uuid::from_u128(12))
            .expect("ineligible acceptor ticket")
            .status,
        MatchmakingTicketStatus::Expired
    );
    assert!(memory.tickets.values().all(|ticket| {
        ticket.player_id == Uuid::from_u128(12) || ticket.status == MatchmakingTicketStatus::Queued
    }));
}

#[test]
fn materialization_consumes_exact_tickets_idempotently() {
    let now = Utc::now();
    let proposal_id = Uuid::new_v4();
    let mut ticket = matchmaking_ticket(1, 11, "window", &["captain"]);
    ticket.status = MatchmakingTicketStatus::Matched;
    ticket.matched_proposal_id = Some(proposal_id);
    ticket.version = 2;
    assert!(consume_materialized_ticket(&mut ticket, proposal_id, now)
        .expect("matched ticket consumed"));
    assert_eq!(ticket.status, MatchmakingTicketStatus::Consumed);
    assert_eq!(ticket.version, 3);
    assert!(!consume_materialized_ticket(&mut ticket, proposal_id, now)
        .expect("consumption replay is stable"));
    assert!(consume_materialized_ticket(&mut ticket, Uuid::new_v4(), now).is_err());
}
