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
fn cancelled_only_work_does_not_satisfy_preregistration_progress() {
    let now = Utc::now();
    let player_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let challenge_id = Uuid::new_v4();
    let paper_id = Uuid::new_v4();
    let paper = PaperProject {
        paper_project_id: paper_id,
        team_id,
        challenge_id,
        title: "Cancelled-only work fixture".into(),
        target_format: "paper".into(),
        phase: PaperPhase::Preregistering,
        challenge_ruleset_snapshot: None,
        challenge_ruleset_snapshot_hash: None,
        deadline_at: None,
        grace_expires_at: None,
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        role_resources: None,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let team = ResearchTeam {
        team_id,
        challenge_id,
        collaboration_compact_hash: format!("sha256:{}", "11".repeat(32)),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: vec![TeamMember {
            participant_slot: 1,
            player_id,
            binding_id: Uuid::new_v4(),
            agent_id: "did:trnm:cancelled-only-work".into(),
            role: "captain".into(),
            joined_at: now,
        }],
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let cancelled = WorkItem {
        work_item_id: Uuid::new_v4(),
        paper_project_id: paper_id,
        kind: "preregistration".into(),
        title: "Cancelled preregistration".into(),
        assigned_player_id: Some(player_id),
        assigned_binding_id: Some(team.members[0].binding_id),
        status: WorkItemStatus::Cancelled,
        artifact_manifest_hash: None,
        version: 2,
        created_at: now,
        updated_at: now,
    };
    let progress = project_author_raid_progress(
        &paper,
        &team,
        player_id,
        &[cancelled],
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
    assert!(progress
        .blockers
        .contains(&"work_item_required".to_string()));
    assert!(progress
        .next_actions
        .contains(&"create_paper_work_item".to_string()));
    assert!(!progress.transition_ready);
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
        role_resources: None,
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
    assert_eq!(stale_progress.phase, PaperPhase::Reproducing);
    assert_eq!(stale_progress.player_phase, "reproduction_readiness");
    assert_eq!(
        stale_progress.next_player_phase,
        Some("author_approval".into())
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
        role_resources: None,
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
        party_code_hash: None,
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
fn matcher_v2_identity_freezes_role_order_and_exact_assignments() {
    let original = vec![
        matchmaking_ticket(1, 11, "window", &["captain", "evidence"]),
        matchmaking_ticket(2, 12, "window", &["evidence", "captain"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let first = build_team_proposal(&original).expect("original proposal");
    assert_eq!(
        first
            .role_assignments
            .iter()
            .map(|assignment| assignment.assigned_role.as_str())
            .collect::<Vec<_>>(),
        vec!["captain", "evidence", "experiment"]
    );

    let mut reordered = original.clone();
    reordered[0].roles = vec!["evidence".into(), "captain".into()];
    let second = build_team_proposal(&reordered).expect("reordered proposal");
    assert_ne!(
        first.deterministic_match_key,
        second.deterministic_match_key
    );
    assert_ne!(first.proposal_id, second.proposal_id);
    assert_ne!(first.source_preferences, second.source_preferences);
    assert_ne!(first.role_assignments, second.role_assignments);

    let mut matched = original;
    let mut accepted = first;
    accepted.status = TeamProposalStatus::Accepted;
    for ticket in &mut matched {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(accepted.proposal_id);
        ticket.version += 1;
    }
    accepted.role_assignments.swap(0, 1);
    assert_eq!(
        validate_materialization_matchmaking_source(&accepted, &matched)
            .expect_err("assignment drift must fail closed")
            .code,
        "team_proposal_provenance_mismatch"
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

fn party_ticket(mut ticket: MatchmakingTicket, marker: char) -> MatchmakingTicket {
    ticket.party_code_hash = Some(format!("sha256:{}", marker.to_string().repeat(64)));
    ticket
}

#[test]
fn matcher_never_mixes_public_or_different_premade_parties() {
    let captain = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    let evidence = party_ticket(matchmaking_ticket(2, 12, "window", &["evidence"]), 'a');
    let public_experiment = matchmaking_ticket(3, 13, "window", &["experiment"]);
    let other_party_experiment =
        party_ticket(matchmaking_ticket(4, 14, "window", &["experiment"]), 'b');

    assert!(select_alpha_match(
        vec![
            captain.clone(),
            evidence.clone(),
            public_experiment,
            other_party_experiment,
        ]
        .into_iter(),
    )
    .is_empty());

    let same_party_experiment =
        party_ticket(matchmaking_ticket(5, 15, "window", &["experiment"]), 'a');
    let selected = select_alpha_match(vec![captain, evidence, same_party_experiment].into_iter());
    assert_eq!(selected.len(), 3);
    assert!(selected.iter().all(|ticket| {
        ticket.party_code_hash.as_deref()
            == Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    }));
}

#[test]
fn party_queue_hint_waits_for_the_exact_third_member() {
    let now = Utc::now();
    let captain = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    let evidence = party_ticket(matchmaking_ticket(2, 12, "window", &["evidence"]), 'a');
    let public_experiment = matchmaking_ticket(3, 13, "window", &["experiment"]);
    let projected = project_matchmaking_ticket(
        captain.clone(),
        &[captain, evidence, public_experiment],
        now,
    );
    let hint = projected.queue_hint.expect("party queue hint");
    assert_eq!(hint.state, "waiting");
    assert_eq!(hint.compatible_pool_size, 2);
    assert_eq!(hint.compatible_players_needed, 1);
    assert_eq!(hint.missing_roles, vec!["experiment"]);
    assert_eq!(hint.message, "waiting_for_party_members");
    assert_eq!(hint.eta_seconds, None);
}

#[test]
fn complete_party_reports_role_deficit_instead_of_missing_members() {
    let now = Utc::now();
    let captain_a = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    let captain_b = party_ticket(matchmaking_ticket(2, 12, "window", &["captain"]), 'a');
    let captain_c = party_ticket(matchmaking_ticket(3, 13, "window", &["captain"]), 'a');
    let projected =
        project_matchmaking_ticket(captain_a.clone(), &[captain_a, captain_b, captain_c], now);
    let hint = projected.queue_hint.expect("complete party queue hint");

    assert_eq!(hint.compatible_pool_size, 3);
    assert_eq!(hint.message, "waiting_for_required_roles");
    assert_eq!(hint.eta_seconds, None);
}

#[test]
fn party_admission_rejects_an_unfinishable_role_or_availability_partition() {
    let captain = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    validate_premade_party_admission(&[], &captain).expect("first party member");

    let duplicate_captain = party_ticket(matchmaking_ticket(2, 12, "window", &["captain"]), 'a');
    assert_eq!(
        validate_premade_party_admission(std::slice::from_ref(&captain), &duplicate_captain)
            .expect_err("two captain-only members cannot be completed")
            .code,
        "party_role_conflict"
    );

    let evidence = party_ticket(matchmaking_ticket(3, 13, "window", &["evidence"]), 'a');
    validate_premade_party_admission(std::slice::from_ref(&captain), &evidence)
        .expect("two distinct roles remain completable");
    let wrong_window = party_ticket(
        matchmaking_ticket(4, 14, "other-window", &["experiment"]),
        'a',
    );
    assert_eq!(
        validate_premade_party_admission(&[captain, evidence], &wrong_window)
            .expect_err("one party cannot span availability partitions")
            .code,
        "party_availability_conflict"
    );
}

#[test]
fn cancellation_frees_one_party_slot_and_replacement_rematches_only_inside_party() {
    let now = Utc::now();
    let mut cancelled = party_ticket(matchmaking_ticket(1, 11, "window", &["experiment"]), 'a');
    cancelled.status = MatchmakingTicketStatus::Cancelled;
    let captain = party_ticket(matchmaking_ticket(2, 12, "window", &["captain"]), 'a');
    let evidence = party_ticket(matchmaking_ticket(3, 13, "window", &["evidence"]), 'a');
    let replacement = party_ticket(matchmaking_ticket(4, 14, "window", &["experiment"]), 'a');
    let public = matchmaking_ticket(5, 15, "window", &["experiment"]);

    validate_premade_party_admission(
        &[cancelled.clone(), captain.clone(), evidence.clone()],
        &replacement,
    )
    .expect("cancelled member no longer occupies the private party cap");

    let mut memory = CollaborationMemory::default();
    for ticket in [cancelled, captain, evidence, replacement, public.clone()] {
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
        public.challenge_id,
        now,
    )
    .expect("replacement rematch");

    assert_eq!(proposals.len(), 1);
    assert!(proposals[0].source_ticket_ids.iter().all(|ticket_id| {
        memory.tickets.get(ticket_id).is_some_and(|ticket| {
            ticket.party_code_hash.as_deref()
                == Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        })
    }));
    assert_eq!(
        memory
            .tickets
            .get(&public.ticket_id)
            .expect("public ticket retained")
            .status,
        MatchmakingTicketStatus::Queued
    );
}

#[test]
fn queue_projection_never_claims_ready_beyond_the_matcher_fifo_horizon() {
    let now = Utc::now();
    let base = now - chrono::Duration::seconds(10);
    let mut queue = (0..MAX_ALPHA_MATCH_CANDIDATES)
        .map(|index| {
            let mut ticket = matchmaking_ticket(
                10_000 + index as u128,
                20_000 + index as u128,
                "window",
                &["captain"],
            );
            ticket.created_at = base + chrono::Duration::milliseconds(index as i64);
            ticket.updated_at = ticket.created_at;
            ticket
        })
        .collect::<Vec<_>>();
    let mut captain = party_ticket(
        matchmaking_ticket(40_001, 50_001, "window", &["captain"]),
        'a',
    );
    let mut evidence = party_ticket(
        matchmaking_ticket(40_002, 50_002, "window", &["evidence"]),
        'a',
    );
    let mut experiment = party_ticket(
        matchmaking_ticket(40_003, 50_003, "window", &["experiment"]),
        'a',
    );
    for (offset, ticket) in [&mut captain, &mut evidence, &mut experiment]
        .into_iter()
        .enumerate()
    {
        ticket.created_at = base
            + chrono::Duration::milliseconds(
                i64::try_from(MAX_ALPHA_MATCH_CANDIDATES + offset).expect("bounded horizon"),
            );
        ticket.updated_at = ticket.created_at;
    }
    queue.extend([captain.clone(), evidence, experiment]);

    assert!(select_alpha_match_at(queue.clone().into_iter(), now).is_empty());
    let projected = project_matchmaking_ticket(captain, &queue, now);
    let hint = projected.queue_hint.expect("bounded queue hint");
    assert_eq!(hint.state, "waiting");
    assert_eq!(hint.queue_position, None);
    assert_eq!(hint.eta_seconds, None);
    assert_ne!(hint.message, "compatible_team_ready");
}

#[test]
fn expiry_sweep_immediately_rematches_a_triplet_behind_2048_due_blockers() {
    let now = Utc::now();
    let base = now - chrono::Duration::hours(1);
    let challenge_id = Uuid::from_u128(0x9000);
    let mut memory = CollaborationMemory::default();
    let mut eligible = HashSet::new();
    for index in 0..MAX_ALPHA_MATCH_CANDIDATES {
        let mut blocker = matchmaking_ticket(
            100_000 + index as u128,
            200_000 + index as u128,
            "window",
            &["captain"],
        );
        blocker.created_at = base + chrono::Duration::milliseconds(index as i64);
        blocker.updated_at = blocker.created_at;
        blocker.expires_at = Some(now - chrono::Duration::seconds(1));
        eligible.insert(blocker.player_id);
        memory.tickets.insert(blocker.ticket_id, blocker);
    }
    let mut tail = [
        matchmaking_ticket(900_001, 910_001, "window", &["captain"]),
        matchmaking_ticket(900_002, 910_002, "window", &["evidence"]),
        matchmaking_ticket(900_003, 910_003, "window", &["experiment"]),
    ];
    for (offset, ticket) in tail.iter_mut().enumerate() {
        ticket.created_at = now + chrono::Duration::milliseconds(offset as i64);
        ticket.updated_at = ticket.created_at;
        eligible.insert(ticket.player_id);
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
    }
    let expired = expire_due_matchmaking_tickets_memory(&mut memory, &eligible, challenge_id, now);
    assert_eq!(expired.len(), MAX_ALPHA_MATCH_CANDIDATES);
    let replacements = auto_match_all_queued_tickets_memory(
        &mut memory,
        &HashSet::new(),
        &eligible,
        challenge_id,
        now,
    )
    .expect("tail rematch after expiry sweep");
    assert_eq!(replacements.len(), 1);
    assert_eq!(
        replacements[0].source_ticket_ids,
        tail.iter()
            .map(|ticket| ticket.ticket_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn read_drain_matches_a_released_legacy_triplet_without_a_new_mutation() {
    let now = Utc::now();
    let challenge_id = Uuid::from_u128(0x9010);
    let mut memory = CollaborationMemory::default();
    let tickets = vec![
        matchmaking_ticket(901_001, 911_001, "window", &["captain"]),
        matchmaking_ticket(901_002, 911_002, "window", &["evidence"]),
        matchmaking_ticket(901_003, 911_003, "window", &["experiment"]),
    ];
    let eligible = tickets.iter().map(|ticket| ticket.player_id).collect();
    for mut ticket in tickets {
        ticket.challenge_id = challenge_id;
        memory.tickets.insert(ticket.ticket_id, ticket);
    }

    let replacements = auto_match_all_queued_tickets_memory(
        &mut memory,
        &HashSet::new(),
        &eligible,
        challenge_id,
        now,
    )
    .expect("player-scoped read drains already queued legacy releases");

    assert_eq!(replacements.len(), 1);
    assert!(memory.tickets.values().all(|ticket| {
        ticket.status == MatchmakingTicketStatus::Matched
            && ticket.matched_proposal_id == Some(replacements[0].proposal_id)
    }));
}

#[test]
fn queue_projection_eta_zero_follows_the_one_global_fifo_winner() {
    let now = Utc::now();
    let base = now - chrono::Duration::seconds(10);
    let mut public = [
        matchmaking_ticket(1, 11, "public-window", &["captain"]),
        matchmaking_ticket(2, 12, "public-window", &["evidence"]),
        matchmaking_ticket(3, 13, "public-window", &["experiment"]),
    ];
    let mut private = [
        party_ticket(matchmaking_ticket(4, 14, "party-window", &["captain"]), 'a'),
        party_ticket(
            matchmaking_ticket(5, 15, "party-window", &["evidence"]),
            'a',
        ),
        party_ticket(
            matchmaking_ticket(6, 16, "party-window", &["experiment"]),
            'a',
        ),
    ];
    for (index, ticket) in public.iter_mut().enumerate() {
        ticket.created_at = base + chrono::Duration::milliseconds(index as i64);
        ticket.updated_at = ticket.created_at;
    }
    for (index, ticket) in private.iter_mut().enumerate() {
        ticket.created_at = base + chrono::Duration::milliseconds(10 + index as i64);
        ticket.updated_at = ticket.created_at;
    }
    let queue = public
        .iter()
        .chain(private.iter())
        .cloned()
        .collect::<Vec<_>>();

    let public_hint = project_matchmaking_ticket(public[0].clone(), &queue, now)
        .queue_hint
        .expect("public hint");
    assert_eq!(public_hint.state, "ready");
    assert_eq!(public_hint.eta_seconds, Some(0));

    let private_hint = project_matchmaking_ticket(private[0].clone(), &queue, now)
        .queue_hint
        .expect("private hint");
    assert_eq!(private_hint.state, "waiting");
    assert_eq!(private_hint.eta_seconds, None);
    assert_ne!(private_hint.message, "compatible_team_ready");
}

#[test]
fn eligibility_filtered_current_ticket_is_never_reinserted_by_projection() {
    let now = Utc::now();
    let evidence = matchmaking_ticket(1, 11, "window", &["evidence"]);
    let eligible_queue = vec![
        matchmaking_ticket(2, 12, "window", &["captain"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];

    let hint = project_matchmaking_ticket(evidence, &eligible_queue, now)
        .queue_hint
        .expect("filtered hint");
    assert_eq!(hint.state, "waiting");
    assert_eq!(hint.queue_position, None);
    assert_eq!(hint.eta_seconds, None);
}

#[test]
fn party_capacity_counts_only_same_challenge_live_tickets() {
    let party_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let tickets = [
        party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a'),
        party_ticket(matchmaking_ticket(2, 12, "window", &["evidence"]), 'a'),
        party_ticket(matchmaking_ticket(3, 13, "window", &["experiment"]), 'a'),
    ];
    assert_eq!(
        live_party_ticket_count(tickets.iter(), tickets[0].challenge_id, party_hash),
        3,
    );
}

#[test]
fn materialization_revalidates_party_partition_and_deterministic_ticket_epochs() {
    let mut tickets = vec![
        party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a'),
        party_ticket(matchmaking_ticket(2, 12, "window", &["evidence"]), 'a'),
        party_ticket(matchmaking_ticket(3, 13, "window", &["experiment"]), 'a'),
    ];
    let mut proposal = build_team_proposal(&tickets).expect("valid private proposal");
    proposal.status = TeamProposalStatus::Accepted;
    for ticket in &mut tickets {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version += 1;
    }
    validate_materialization_matchmaking_source(&proposal, &tickets)
        .expect("exact matched party remains materializable");

    let mut partition_drift = tickets.clone();
    partition_drift[2].party_code_hash =
        Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &partition_drift)
            .expect_err("cross-party source drift must fail closed")
            .code,
        "team_proposal_provenance_mismatch"
    );

    let mut synchronized_partition_drift = tickets.clone();
    for ticket in &mut synchronized_partition_drift {
        ticket.party_code_hash =
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    }
    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &synchronized_partition_drift)
            .expect_err("the V2 match key must bind the original private partition")
            .code,
        "team_proposal_provenance_mismatch"
    );

    let mut epoch_drift = tickets.clone();
    epoch_drift[0].version += 1;
    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &epoch_drift)
            .expect_err("source ticket epoch drift must fail closed")
            .code,
        "team_proposal_provenance_mismatch"
    );

    let mut synchronized_role_drift = tickets.clone();
    synchronized_role_drift[1].roles = vec!["experiment".to_string()];
    synchronized_role_drift[2].roles = vec!["evidence".to_string()];
    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &synchronized_role_drift)
            .expect_err("the V2 match key must bind each player's original role offer")
            .code,
        "team_proposal_provenance_mismatch"
    );

    let mut synchronized_player_drift = tickets.clone();
    // Swap the players attached to two fixed source tickets, then drift the
    // proposal's visible player mapping in lockstep. The deterministic
    // identity must still reject the rewrite.
    let swapped_player = synchronized_player_drift[1].player_id;
    synchronized_player_drift[1].player_id = synchronized_player_drift[2].player_id;
    synchronized_player_drift[2].player_id = swapped_player;
    let mut player_drift_proposal = proposal.clone();
    player_drift_proposal.member_player_ids = synchronized_player_drift
        .iter()
        .map(|ticket| ticket.player_id)
        .collect();
    assert_eq!(
        validate_materialization_matchmaking_source(
            &player_drift_proposal,
            &synchronized_player_drift,
        )
        .expect_err("the V2 match key must bind each source ticket's original player")
        .code,
        "team_proposal_provenance_mismatch"
    );

    proposal.deterministic_match_key =
        "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into();
    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &tickets)
            .expect_err("proposal match-key drift must fail closed")
            .code,
        "team_proposal_provenance_mismatch"
    );
}

#[test]
fn proposal_actions_reject_and_invalidate_legacy_contract_before_decision() {
    let mut tickets = vec![
        matchmaking_ticket(1, 11, "window", &["captain"]),
        matchmaking_ticket(2, 12, "window", &["evidence"]),
        matchmaking_ticket(3, 13, "window", &["experiment"]),
    ];
    let legacy_match_key = crate::paper_raid_contracts::sha256_digest(b"legacy-public-proposal");
    let mut proposal = build_team_proposal(&tickets).expect("current proposal shape");
    proposal.proposal_id = deterministic_uuid(&legacy_match_key);
    proposal.deterministic_match_key = legacy_match_key;
    proposal.solver_version = None;
    proposal.source_preferences.clear();
    proposal.role_assignments.clear();
    proposal.status = TeamProposalStatus::Proposed;
    for ticket in &mut tickets {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version += 1;
    }

    assert_eq!(
        validate_materialization_matchmaking_source(&proposal, &tickets)
            .expect_err("legacy proposals without complete frozen source identity fail closed")
            .code,
        "team_proposal_provenance_mismatch"
    );

    let now = Utc::now();
    let mut memory = CollaborationMemory::default();
    for ticket in &tickets {
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());
    let eligible_player_ids = tickets.iter().map(|ticket| ticket.player_id).collect();
    let invalidation = invalidate_team_proposal_provenance_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_player_ids,
        proposal.proposal_id,
        now,
    )
    .expect("decision authority atomically invalidates legacy contract");
    assert_eq!(invalidation.expired.len(), 1);
    assert_eq!(invalidation.replacements.len(), 1);
    assert_eq!(
        memory
            .team_proposals
            .get(&proposal.proposal_id)
            .expect("legacy proposal retained for history")
            .status,
        TeamProposalStatus::Expired
    );
    assert!(memory.tickets.values().all(|ticket| {
        ticket.status == MatchmakingTicketStatus::Matched
            && ticket.matched_proposal_id == Some(invalidation.replacements[0].proposal_id)
    }));
}

#[test]
fn player_ticket_view_reveals_only_private_party_boolean() {
    let internal = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    let created_event = matchmaking_ticket_created_event_payload(&internal);
    let cancelled_event = matchmaking_ticket_cancelled_event_payload(&internal);
    let view = MatchmakingTicketView::from(internal);
    let value = serde_json::to_value(view).expect("ticket view JSON");

    assert_eq!(
        value.get("private_party").and_then(Value::as_bool),
        Some(true)
    );
    assert!(value.get("party_code_hash").is_none());
    assert!(!value.to_string().contains(&"a".repeat(64)));
    for event in [created_event, cancelled_event] {
        assert!(event.get("party_code_hash").is_none());
        assert!(event.get("private_party").is_none());
        assert!(!event.to_string().contains(&"a".repeat(64)));
    }
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
        party_code_hash: None,
        idempotency_key: Uuid::new_v4().to_string(),
    };
    assert!(validate_matchmaking_ticket_request(&request).is_err());

    let mut invalid_party = request;
    invalid_party.roles = vec!["captain".to_string()];
    invalid_party.party_code_hash = Some("not-a-canonical-digest".to_string());
    assert!(validate_matchmaking_ticket_request(&invalid_party).is_err());
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

    assert!(expire_matchmaking_ticket(&mut expired, now, true));
    assert_eq!(expired.status, MatchmakingTicketStatus::Expired);
    assert_eq!(expired.version, 2);
    assert!(!expire_matchmaking_ticket(&mut expired, now, true));

    let waiting = project_matchmaking_ticket(evidence.clone(), &[evidence, experiment], now);
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
    let projected =
        project_matchmaking_ticket(captain_a.clone(), &[captain_a, captain_b, flexible], now);
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
    assert_eq!(decoded.party_code_hash, None);
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
fn eligibility_sweep_expires_queued_and_matched_party_seats_without_waiting_for_ttl() {
    let now = Utc::now();
    let challenge_id = Uuid::from_u128(0x9000);
    let mut memory = CollaborationMemory::default();
    let queued = party_ticket(matchmaking_ticket(1, 11, "window", &["captain"]), 'a');
    memory.tickets.insert(queued.ticket_id, queued.clone());
    let eligible_without_queued = HashSet::new();
    let expired = expire_due_matchmaking_tickets_memory(
        &mut memory,
        &eligible_without_queued,
        challenge_id,
        now,
    );
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].status, MatchmakingTicketStatus::Expired);
    assert_eq!(expired[0].expires_at, Some(now));

    let mut matched = vec![
        party_ticket(matchmaking_ticket(2, 12, "window", &["captain"]), 'a'),
        party_ticket(matchmaking_ticket(3, 13, "window", &["evidence"]), 'a'),
        party_ticket(matchmaking_ticket(4, 14, "window", &["experiment"]), 'a'),
    ];
    let mut proposal = build_team_proposal(&matched).expect("valid private proposal");
    proposal.expires_at = Some(now + chrono::Duration::minutes(5));
    for ticket in &mut matched {
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version += 1;
        memory.tickets.insert(ticket.ticket_id, ticket.clone());
    }
    memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone());
    let eligible_players = [Uuid::from_u128(12), Uuid::from_u128(14)]
        .into_iter()
        .collect();
    let sweep = expire_due_team_proposals_memory(
        &mut memory,
        &HashSet::new(),
        &eligible_players,
        challenge_id,
        now,
    )
    .expect("ineligible matched member releases the proposal");
    assert_eq!(sweep.expired.len(), 1);
    assert_eq!(sweep.expired[0].expires_at, Some(now));
    assert!(matched.iter().all(|source| {
        memory
            .tickets
            .get(&source.ticket_id)
            .is_some_and(|ticket| ticket.status == MatchmakingTicketStatus::Expired)
    }));
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

fn expiring_paper(grace_expires_at: DateTime<Utc>) -> PaperProject {
    PaperProject {
        paper_project_id: Uuid::from_u128(0xeeee),
        team_id: Uuid::from_u128(0xaaaa),
        challenge_id: Uuid::from_u128(0xbbbb),
        title: "Automatic expiry fixture".into(),
        target_format: "paper".into(),
        phase: PaperPhase::Experimenting,
        challenge_ruleset_snapshot: None,
        challenge_ruleset_snapshot_hash: None,
        deadline_at: Some(grace_expires_at - chrono::Duration::minutes(15)),
        grace_expires_at: Some(grace_expires_at),
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        role_resources: None,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 7,
        created_at: grace_expires_at - chrono::Duration::hours(2),
        updated_at: grace_expires_at - chrono::Duration::minutes(1),
    }
}

#[test]
fn automatic_challenge_expiry_uses_a_half_open_grace_boundary_and_is_idempotent() {
    let grace_expires_at = Utc
        .with_ymd_and_hms(2026, 8, 11, 9, 12, 1)
        .single()
        .expect("fixed grace deadline");
    let mut paper = expiring_paper(grace_expires_at);
    let original = paper.clone();

    assert!(!apply_automatic_challenge_expiry(
        &mut paper,
        grace_expires_at - chrono::Duration::nanoseconds(1),
    ));
    assert_eq!(paper, original);

    assert!(apply_automatic_challenge_expiry(
        &mut paper,
        grace_expires_at,
    ));
    assert_eq!(paper.outcome, PaperChallengeOutcomeV1::Expired);
    assert_eq!(
        paper.outcome_reason.as_deref(),
        Some(AUTOMATIC_CHALLENGE_EXPIRY_REASON)
    );
    assert_eq!(paper.terminal_at, Some(grace_expires_at));
    assert_eq!(paper.updated_at, grace_expires_at);
    assert_eq!(paper.version, original.version + 1);

    let materialized = paper.clone();
    assert!(!apply_automatic_challenge_expiry(
        &mut paper,
        grace_expires_at + chrono::Duration::hours(1),
    ));
    assert_eq!(paper, materialized, "retry must be a no-op");
}

#[test]
fn automatic_challenge_expiry_memory_writes_one_room_and_outbox_event() {
    let grace_expires_at = Utc
        .with_ymd_and_hms(2026, 8, 11, 9, 12, 1)
        .single()
        .expect("fixed grace deadline");
    let paper = expiring_paper(grace_expires_at);
    let paper_id = paper.paper_project_id;
    let mut memory = PaperRaidMemory::default();
    memory.papers.insert(paper_id, paper);

    assert!(materialize_automatic_challenge_expiry_memory(
        &mut memory,
        paper_id,
        grace_expires_at - chrono::Duration::nanoseconds(1),
    )
    .is_none());
    let response =
        materialize_automatic_challenge_expiry_memory(&mut memory, paper_id, grace_expires_at)
            .expect("boundary materializes expiry");
    assert!(materialize_automatic_challenge_expiry_memory(
        &mut memory,
        paper_id,
        grace_expires_at + chrono::Duration::seconds(1),
    )
    .is_none());

    let room_events = memory
        .collaboration
        .events
        .iter()
        .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
        .collect::<Vec<_>>();
    assert_eq!(room_events.len(), 1);
    assert_eq!(room_events[0].aggregate_id, paper_id);
    assert_eq!(room_events[0].aggregate_version, response.version);
    assert_eq!(room_events[0].payload["automatic"], true);
    assert_eq!(
        room_events[0].payload["reason_code"],
        AUTOMATIC_CHALLENGE_EXPIRY_REASON
    );

    let outbox_events = memory
        .events
        .iter()
        .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
        .collect::<Vec<_>>();
    assert_eq!(outbox_events.len(), 1);
    assert_eq!(
        outbox_events[0].idempotency_key,
        format!(
            "paper-raid:{AUTOMATIC_CHALLENGE_EXPIRY_OPERATION}:{}",
            automatic_challenge_expiry_idempotency_key(paper_id)
        )
    );
}

#[tokio::test]
async fn concurrent_automatic_challenge_expiry_reads_materialize_once() {
    let grace_expires_at = Utc
        .with_ymd_and_hms(2026, 8, 11, 9, 12, 1)
        .single()
        .expect("fixed grace deadline");
    let paper = expiring_paper(grace_expires_at);
    let paper_id = paper.paper_project_id;
    let memory = std::sync::Arc::new(tokio::sync::RwLock::new(PaperRaidMemory::default()));
    memory.write().await.papers.insert(paper_id, paper);

    let materialize = |memory: std::sync::Arc<tokio::sync::RwLock<PaperRaidMemory>>| async move {
        let mut memory = memory.write().await;
        materialize_automatic_challenge_expiry_memory(&mut memory, paper_id, grace_expires_at)
            .is_some()
    };
    let (left, right) = tokio::join!(materialize(memory.clone()), materialize(memory.clone()));
    assert_ne!(left, right, "exactly one concurrent caller must win");

    let memory = memory.read().await;
    assert_eq!(
        memory
            .collaboration
            .events
            .iter()
            .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
            .count(),
        1
    );
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
            .count(),
        1
    );
}

fn expiry_assertion(
    player_id: Uuid,
    subject_id: &str,
    nakama_user_id: Uuid,
) -> ConsumerUserAssertionClaimV2 {
    ConsumerUserAssertionClaimV2 {
        schema: "hepta.consumer_user_assertion.v2".into(),
        assertion_id: Uuid::new_v4(),
        issuer: "expiry-test".into(),
        audience: "hepta-paper-raid-v2".into(),
        subject_id: subject_id.into(),
        nakama_user_id,
        player_id,
        operation: "get_paper_project_v2".into(),
        http_method: "GET".into(),
        canonical_path: "/v2/hepta/papers/fixture".into(),
        idempotency_key: format!("expiry-read-{player_id}"),
        body_hash: format!("sha256:{}", "00".repeat(32)),
        issued_at_unix: 1,
        expires_at_unix: i64::MAX,
        nonce: format!("expiry-nonce-{player_id}"),
    }
}

#[tokio::test]
async fn authorized_late_read_materializes_grace_time_and_non_member_cannot_mutate() {
    let grace_expires_at = Utc
        .with_ymd_and_hms(2026, 8, 11, 9, 12, 1)
        .single()
        .expect("fixed grace deadline");
    let materialized_at = grace_expires_at + chrono::Duration::seconds(9);
    let paper = expiring_paper(grace_expires_at);
    let paper_id = paper.paper_project_id;
    let member_id = Uuid::from_u128(0x1111);
    let outsider_id = Uuid::from_u128(0x2222);
    let member_nakama_id = Uuid::from_u128(0x3333);
    let subject_id = "oidc|expiry-member";
    let now = grace_expires_at - chrono::Duration::hours(1);
    let team = ResearchTeam {
        team_id: paper.team_id,
        challenge_id: paper.challenge_id,
        collaboration_compact_hash: format!("sha256:{}", "11".repeat(32)),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: vec![TeamMember {
            participant_slot: 1,
            player_id: member_id,
            binding_id: Uuid::from_u128(0x4444),
            agent_id: "did:trnm:expiry-member".into(),
            role: "captain".into(),
            joined_at: now,
        }],
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let player = HumanPlayer {
        player_id: member_id,
        subject_id: subject_id.into(),
        nakama_user_id: member_nakama_id,
        display_name: "Expiry member".into(),
        signing_key_id: "expiry-key-v1".into(),
        signing_public_key: "00".repeat(32),
        signing_public_key_hash: format!("sha256:{}", "22".repeat(32)),
        status: HumanPlayerStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let state = AppState::new(crate::SecurityConfig::new("operator", "nakama"));
    {
        let mut memory = state.paper_raid.write().await;
        memory.players.insert(member_id, player);
        memory.teams.insert(team.team_id, team);
        memory.papers.insert(paper_id, paper);
    }

    let outsider = expiry_assertion(outsider_id, "oidc|outsider", Uuid::from_u128(0x5555));
    assert_eq!(
        ensure_automatic_challenge_expiry_materialized(
            &state,
            paper_id,
            &outsider,
            materialized_at,
        )
        .await
        .expect_err("non-member read must not materialize expiry")
        .code,
        "user_not_on_team",
    );
    assert_eq!(
        state.paper_raid.read().await.papers[&paper_id].outcome,
        PaperChallengeOutcomeV1::InProgress,
    );

    let member = expiry_assertion(member_id, subject_id, member_nakama_id);
    ensure_automatic_challenge_expiry_materialized(&state, paper_id, &member, materialized_at)
        .await
        .expect("authorized late read materializes expiry");
    ensure_automatic_challenge_expiry_materialized(
        &state,
        paper_id,
        &member,
        materialized_at + chrono::Duration::seconds(1),
    )
    .await
    .expect("authorized retry is idempotent");

    let memory = state.paper_raid.read().await;
    let expired = &memory.papers[&paper_id];
    assert_eq!(expired.outcome, PaperChallengeOutcomeV1::Expired);
    assert_eq!(expired.terminal_at, Some(grace_expires_at));
    assert_eq!(expired.updated_at, materialized_at);
    assert_eq!(
        super::super::validate_requested_terminal_outcome(
            expired,
            PaperChallengeOutcomeV1::Expired,
            materialized_at,
        )
        .expect_err("manual expiry cannot overwrite canonical automatic expiry")
        .code,
        "paper_challenge_terminal",
    );
    assert_eq!(
        expired.outcome_reason.as_deref(),
        Some(AUTOMATIC_CHALLENGE_EXPIRY_REASON),
    );
    assert_eq!(
        memory
            .collaboration
            .events
            .iter()
            .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
            .count(),
        1,
    );
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| event.event_type == AUTOMATIC_CHALLENGE_EXPIRY_EVENT)
            .count(),
        1,
    );
}

#[test]
fn automatic_challenge_expiry_transition_is_backend_neutral() {
    let grace_expires_at = Utc
        .with_ymd_and_hms(2026, 8, 11, 9, 12, 1)
        .single()
        .expect("fixed grace deadline");
    let now = grace_expires_at + chrono::Duration::seconds(9);
    let mut memory_record = expiring_paper(grace_expires_at);
    let mut postgres_record = memory_record.clone();

    assert!(apply_automatic_challenge_expiry(&mut memory_record, now));
    assert!(apply_automatic_challenge_expiry(&mut postgres_record, now));
    assert_eq!(memory_record, postgres_record);
    assert_eq!(
        memory_record.terminal_at,
        Some(grace_expires_at),
        "late materialization must preserve the immutable grace boundary",
    );
    assert_eq!(
        memory_record.updated_at, now,
        "updated_at records when lazy materialization occurred",
    );

    memory_record.outcome = PaperChallengeOutcomeV1::SubmissionReady;
    postgres_record = memory_record.clone();
    assert!(!apply_automatic_challenge_expiry(
        &mut memory_record,
        now + chrono::Duration::days(1),
    ));
    assert_eq!(memory_record, postgres_record);
}

fn raid_history_fixture(
    seed: u128,
    outcome: PaperChallengeOutcomeV1,
    updated_at: DateTime<Utc>,
) -> PlayerRaidSummary {
    let terminal = matches!(
        outcome,
        PaperChallengeOutcomeV1::Failed
            | PaperChallengeOutcomeV1::Expired
            | PaperChallengeOutcomeV1::Abandoned
    );
    PlayerRaidSummary {
        team_id: Uuid::from_u128(seed),
        challenge_id: Uuid::from_u128(seed + 100),
        team_status: TeamStatus::Locked,
        team_version: 1,
        roster_version: 1,
        member_count: 3,
        acceptance_count: 3,
        participant_slot: 1,
        role: "captain".into(),
        player_ready: true,
        paper: Some(PaperRaidProgress {
            paper_project_id: Uuid::from_u128(seed + 200),
            title: format!("Raid {seed}"),
            phase: PaperPhase::Experimenting,
            player_phase: "experimenting".into(),
            outcome,
            outcome_reason: terminal.then(|| "terminal-fixture".into()),
            terminal_at: terminal.then_some(updated_at),
            role_resources: None,
            version: 2,
            current_revision_id: None,
            release_candidate_revision_id: None,
            updated_at,
        }),
        updated_at,
    }
}

#[test]
fn terminal_raid_remains_in_history_but_never_becomes_current() {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 11, 10, 0, 0)
        .single()
        .expect("fixed raid-state time");
    let expired_newest = raid_history_fixture(
        1,
        PaperChallengeOutcomeV1::Expired,
        now + chrono::Duration::minutes(2),
    );
    let active_older = raid_history_fixture(2, PaperChallengeOutcomeV1::InProgress, now);
    let raids = vec![expired_newest.clone(), active_older.clone()];

    let current =
        current_raid_from_sorted_history(&raids).expect("older active raid remains current");
    assert_eq!(current.team_id, active_older.team_id);
    assert_eq!(
        raids[0].team_id, expired_newest.team_id,
        "history is retained"
    );

    for outcome in [
        PaperChallengeOutcomeV1::Failed,
        PaperChallengeOutcomeV1::Expired,
        PaperChallengeOutcomeV1::Abandoned,
    ] {
        assert!(
            current_raid_from_sorted_history(&[raid_history_fixture(10, outcome, now)]).is_none()
        );
    }

    let submission_ready = raid_history_fixture(20, PaperChallengeOutcomeV1::SubmissionReady, now);
    assert_eq!(
        current_raid_from_sorted_history(std::slice::from_ref(&submission_ready))
            .expect("submission-ready raid remains resumable")
            .team_id,
        submission_ready.team_id,
    );

    let mut forming = raid_history_fixture(30, PaperChallengeOutcomeV1::InProgress, now);
    forming.team_status = TeamStatus::Forming;
    forming.paper = None;
    assert_eq!(
        current_raid_from_sorted_history(std::slice::from_ref(&forming))
            .expect("forming team without a Paper remains current")
            .team_id,
        forming.team_id,
    );
    forming.team_status = TeamStatus::Archived;
    assert!(current_raid_from_sorted_history(&[forming]).is_none());
}
