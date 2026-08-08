use axum::{
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use serde_json::Value;

use crate::config::AlphaIdentity;

#[derive(Clone, Copy)]
pub enum ReadState<'a> {
    Available(&'a Value),
    NotFound,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnboardingStage {
    HumanRegistration,
    AgentBinding,
    Unavailable,
}

impl<'a> ReadState<'a> {
    fn value(self) -> Option<&'a Value> {
        match self {
            Self::Available(value) => Some(value),
            Self::NotFound | Self::Unavailable => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Available(_) => "available",
            Self::NotFound => "not_found / 未找到",
            Self::Unavailable => "unavailable / 暂不可用",
        }
    }
}

pub fn lobby(
    identity: &AlphaIdentity,
    challenges: ReadState<'_>,
    tickets: ReadState<'_>,
    proposals: ReadState<'_>,
    bindings: ReadState<'_>,
) -> Response {
    let challenge_cards = challenges
        .value()
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|challenge| {
                    let challenge_id = scalar(challenge.get("challenge_id"));
                    let title = scalar(challenge.get("title"));
                    let description = scalar(challenge.get("description"));
                    let status = scalar(challenge.get("status"));
                    format!(
                        r#"<article class="card challenge"><span class="pill">{}</span><h2>{}</h2><p>{}</p><code>{}</code><form class="queue-form" data-challenge-id="{}"><fieldset class="role-kit"><legend>Your three-person expedition / 三人远征队</legend><span><strong>Captain</strong> keeps the decision clock moving</span><span><strong>Evidence</strong> protects claim quality</span><span><strong>Experiment</strong> owns reproducibility</span></fieldset><input name="roles" type="hidden" value="captain,evidence,experiment"><label>Play window / 开局时间<select name="availability" required><option value="alpha-window">Join the next Alpha window / 下一场 Alpha</option><option value="now">Ready now / 现在可玩</option></select></label><button type="submit">Start my first Raid / 开始首局</button><output></output></form></article>"#,
                        escape(&status),
                        escape(&title),
                        escape(&description),
                        escape(&challenge_id),
                        escape(&challenge_id),
                    )
                })
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| unavailable("challenge catalog"));
    let ticket_cards = record_links(tickets, "ticket_id", "status", "/league/formation/", false);
    let proposal_cards = record_links(
        proposals,
        "proposal_id",
        "status",
        "/league/formation/",
        true,
    );
    let binding_controls = active_agent_binding_controls(identity, bindings);
    let mission_board = first_raid_mission_board(tickets, proposals);
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">PAPER RAID · 论文远征</span><h1>Research Lobby</h1><p>Welcome, {}. Hepta is the only matchmaking and research authority.</p></section>
        {}
        <section class="panel"><h2>Challenges / 研究挑战</h2><p class="source-state">Hepta: {}</p><div class="grid">{}</div></section>
        <section class="grid"><article class="card"><h2>My Queue / 我的队列</h2><p class="source-state">Hepta: {}</p>{}</article><article class="card"><h2>Team Proposals / 组队提案</h2><p class="source-state">Hepta: {}</p>{}</article><article class="card"><h2>Alpha Rules / Alpha 规则</h2><p>Exactly 3 human players, each with an independently bound external Agent. Login keys never leave this page except in the login request.</p></article></section>
        <section class="panel"><h2>External Agent key continuity / 外部 Agent 密钥连续性</h2><p class="source-state">Hepta: {}</p><p>Rotation requires independent signatures from both the currently bound key and the replacement key. Only public proof fields enter this browser.</p><div class="action-grid">{}</div></section>"#,
        escape(&identity.display_name),
        mission_board,
        escape(challenges.label()),
        challenge_cards,
        escape(tickets.label()),
        ticket_cards,
        escape(proposals.label()),
        proposal_cards,
        escape(bindings.label()),
        binding_controls,
    );
    page("Paper Raid Lobby", &identity.display_name, &body, true)
}

fn first_raid_mission_board(tickets: ReadState<'_>, proposals: ReadState<'_>) -> String {
    let ticket_count = tickets
        .value()
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let proposal_count = proposals
        .value()
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let states = if proposal_count > 0 {
        ["complete", "complete", "current", "upcoming", "upcoming"]
    } else if ticket_count > 0 {
        ["complete", "current", "upcoming", "upcoming", "upcoming"]
    } else {
        ["current", "upcoming", "upcoming", "upcoming", "upcoming"]
    };
    let steps = [
        (
            "Choose",
            "Pick one challenge and launch your balanced three-person queue.",
        ),
        (
            "Match",
            "Wait for Hepta to assemble three humans and their external Agents.",
        ),
        (
            "Commit",
            "Review the roster, accept the proposal, then lock the team.",
        ),
        (
            "Research",
            "Build evidence, run experiments, and review every major claim.",
        ),
        (
            "Resolve",
            "Submit, reproduce, appeal if needed, and reach scientific finality.",
        ),
    ];
    let items = steps
        .iter()
        .zip(states)
        .enumerate()
        .map(|(index, ((title, help), state))| {
            format!(
                r#"<li data-step-state="{}"><span>{:02}</span><div><strong>{}</strong><p>{}</p></div></li>"#,
                state,
                index + 1,
                title,
                help,
            )
        })
        .collect::<String>();
    format!(
        r#"<section class="panel mission-board"><div><span class="eyebrow">FIRST RAID / 首局任务</span><h2>One clear objective: finish a defensible paper.</h2><p class="muted">You never need to understand resource IDs or signatures to choose your first move. Start with the highlighted step.</p></div><ol class="raid-steps">{items}</ol></section>"#
    )
}

fn active_agent_binding_controls(identity: &AlphaIdentity, bindings: ReadState<'_>) -> String {
    let Some(items) = bindings.value().and_then(Value::as_array) else {
        return unavailable("Agent bindings");
    };
    let mut output = String::new();
    let player_id_text = identity.player_id.to_string();
    for binding in items {
        if binding.get("status").and_then(Value::as_str) != Some("active")
            || binding.get("player_id").and_then(Value::as_str) != Some(player_id_text.as_str())
        {
            continue;
        }
        let Some(binding_id) = binding.get("binding_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(agent_id) = binding.get("agent_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(old_key_id) = binding.get("agent_key_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(old_public_key) = binding.get("agent_public_key").and_then(Value::as_str) else {
            continue;
        };
        let Some(version) = binding.get("version").and_then(Value::as_u64) else {
            continue;
        };
        let payload = serde_json::json!({
            "rotation_id": "00000000-0000-4000-8000-000000000000",
            "expected_binding_version": version,
            "agent_id": agent_id,
            "old_agent_key_id": old_key_id,
            "old_agent_public_key": old_public_key,
            "new_agent_key_id": "sha256:...",
            "new_agent_public_key": "...",
            "issued_at_unix": 0,
            "expires_at_unix": 0,
            "old_key_signature": "...",
            "new_key_signature": "...",
            "idempotency_key": "00000000-0000-4000-8000-000000000000",
        });
        let payload = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into());
        output.push_str(&format!(
            r#"<article class="action"><h3>{}</h3><p>Agent <code>{}</code> · binding version {}</p><p>Signing scope: schema <code>hepta.paper_raid.agent_binding_key_rotation.v2</code>, player <code>{}</code>, subject <code>{}</code>, nonce = idempotency_key.</p><form class="agent-rotation-form" data-binding-id="{}" data-binding-version="{}" data-agent-id="{}" data-old-key-id="{}" data-old-public-key="{}"><label>Exact dual-signed rotation JSON / 双密钥签名轮换 JSON<textarea name="payload" rows="16" spellcheck="false" required>{}</textarea></label><button type="submit">Verify and rotate / 验证并轮换</button><output></output></form></article>"#,
            escape(binding_id),
            escape(agent_id),
            version,
            escape(&player_id_text),
            escape(&identity.subject_id),
            escape(binding_id),
            version,
            escape(agent_id),
            escape(old_key_id),
            escape(old_public_key),
            escape(&payload),
        ));
    }
    if output.is_empty() {
        unavailable("active Agent binding")
    } else {
        output
    }
}

pub fn formation(
    identity: &AlphaIdentity,
    resource_id: &str,
    proposal: ReadState<'_>,
    team: ReadState<'_>,
    acceptances: ReadState<'_>,
) -> Response {
    let roster = team
        .value()
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .map(|members| {
            let mut output = String::new();
            for member in members {
                let slot = scalar(member.get("participant_slot"));
                let player = scalar(member.get("player_id"));
                let agent = scalar(member.get("agent_id"));
                let role = scalar(member.get("role"));
                output.push_str(&format!(
                    "<li><strong>Slot {}</strong><span>Player {}</span><span>Agent {}</span><span>{}</span></li>",
                    escape(&slot), escape(&player), escape(&agent), escape(&role)
                ));
            }
            if output.is_empty() {
                unavailable("roster")
            } else {
                format!("<ul class=\"roster\">{output}</ul>")
            }
        })
        .unwrap_or_else(|| unavailable("roster"));
    let compact = team
        .value()
        .and_then(|value| value.get("collaboration_compact_hash"))
        .map(|value| scalar(Some(value)))
        .map(|value| fact("Compact / 协作契约", &value))
        .unwrap_or_else(|| unavailable_card("Compact / 协作契约", "compact"));
    let readiness = acceptances
        .value()
        .and_then(Value::as_array)
        .map(|values| format!("{} signed acceptance(s)", values.len()))
        .unwrap_or_else(|| acceptances.label().into());
    let proposal_status = proposal
        .value()
        .and_then(|value| value.get("status"))
        .map(|value| scalar(Some(value)))
        .unwrap_or_else(|| proposal.label().into());
    let proposal_version = proposal
        .value()
        .and_then(|value| value.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let proposal_members = proposal
        .value()
        .and_then(|value| value.get("member_player_ids"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| format!("<li><code>{}</code></li>", escape(&scalar(Some(item)))))
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
        .map(|items| format!("<ul>{items}</ul>"))
        .unwrap_or_else(|| unavailable("proposal members"));
    let proposal_controls = if proposal.value().is_some() {
        format!(
            r#"<form class="proposal-decision" data-proposal-id="{}" data-proposal-version="{}"><button name="decision" value="accept" type="submit">Accept Proposal / 接受组队</button><button class="danger" name="decision" value="decline" type="submit">Decline / 拒绝</button><output></output></form>{}"#,
            escape(resource_id),
            proposal_version,
            command_editor(
                "Materialize Research Team / 建立正式队伍",
                "After unanimous proposal acceptance, submit the exact P1 Team payload.",
                "create_research_team",
                None,
                false,
                r#"{"team_id":"...","challenge_id":"...","collaboration_compact_hash":"sha256:...","members":[{"participant_slot":1,"player_id":"...","binding_id":"...","role":"captain"}]}"#,
            ),
        )
    } else {
        unavailable("team proposal")
    };
    let team_controls = if team.value().is_some() {
        format!(
            "{}{}{}",
            command_editor(
                "Ready / 接受正式成员身份",
                "Paste the human-signed acceptance payload produced by your local signer.",
                "accept_research_team_membership",
                Some(resource_id),
                false,
                r#"{"acceptance_id":"...","expected_team_version":1,"roster_version":1,"participant_slot":1,"binding_id":"...","role":"captain","collaboration_compact_hash":"sha256:..."}"#,
            ),
            command_editor(
                "Lock Team / 锁定队伍",
                "Captain submits the exact optimistic-lock payload after all acceptances.",
                "lock_research_team",
                Some(resource_id),
                false,
                r#"{"expected_version":1}"#,
            ),
            command_editor(
                "Create Paper Project / 创建论文项目",
                "After the three-person Team is locked, create its canonical PaperProject.",
                "create_paper_project",
                None,
                false,
                &format!(
                    r#"{{"paper_project_id":"...","team_id":"{}","title":"...","target_format":"workshop-short-paper"}}"#,
                    resource_id
                ),
            )
        )
    } else {
        unavailable("formal Team controls")
    };
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">TEAM FORMATION · 组队</span><h1>Research Cell</h1><p>Resource <code>{}</code></p></section>
        <section class="grid"><article class="card"><h2>Match Proposal / 匹配提案</h2><span class="pill">{}</span>{}{}</article><article class="card"><h2>Players + Agents / 玩家与 Agent</h2><p class="source-state">Hepta Team: {}</p>{}</article>{}</section>
        <section class="panel"><h2>Formation Actions / 组队操作</h2><div class="action-grid">{}{}</div></section>"#,
        escape(resource_id),
        escape(&proposal_status),
        proposal_members,
        proposal_controls,
        escape(team.label()),
        roster,
        fact("Ready Check / 就绪确认", &readiness),
        compact,
        team_controls,
    );
    page("Paper Raid Formation", &identity.display_name, &body, true)
}

pub fn paper_room(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: ReadState<'_>,
    events: ReadState<'_>,
    review: ReadState<'_>,
) -> Response {
    let paper = room.value().and_then(|value| value.get("paper"));
    let title = paper
        .and_then(|value| value.get("title"))
        .map(|value| scalar(Some(value)))
        .unwrap_or_else(|| "Unavailable".into());
    let phase = paper
        .and_then(|value| value.get("phase"))
        .map(|value| scalar(Some(value)))
        .unwrap_or_else(|| "unavailable".into());
    let signoff = room
        .value()
        .and_then(|value| value.get("authorship_consents"))
        .and_then(Value::as_array)
        .map(|values| format!("{} consent record(s)", values.len()))
        .unwrap_or_else(|| room.label().into());
    let timeline_text = events
        .value()
        .and_then(Value::as_array)
        .map(|values| format!("{} authoritative event(s)", values.len()))
        .unwrap_or_else(|| events.label().into());
    let settlement = match room {
        ReadState::Available(_) => "<div class=\"status pending\">pending_finality</div>",
        ReadState::NotFound => "<div class=\"status missing\">not_found</div>",
        ReadState::Unavailable => "<div class=\"status missing\">unavailable</div>",
    };
    let settlement_fact = match room {
        ReadState::Available(_) => "pending_finality",
        ReadState::NotFound => "not_found",
        ReadState::Unavailable => "unavailable",
    };
    let room_card = |title: &str, field: &str| {
        let records = room
            .value()
            .and_then(|value| value.get(field))
            .and_then(Value::as_array)
            .map(|values| format!("{} record(s)", values.len()))
            .unwrap_or_else(|| room.label().into());
        fact(title, &records)
    };
    let review_card = |title: &str, field: &str| {
        let records = review
            .value()
            .and_then(|value| value.get(field))
            .and_then(Value::as_array)
            .map(|values| format!("{} record(s)", values.len()))
            .unwrap_or_else(|| review.label().into());
        fact(title, &records)
    };
    let actions = if room.value().is_some() {
        let mut actions = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        artifact_upload(paper_id),
        command_editor(
            "Register ArtifactManifest / 登记工件清单",
            "Use only server-returned CAS locations and the exact neutral bundle.",
            "register_artifact",
            Some(paper_id),
            false,
            r#"{"manifest_id":"...","expected_paper_version":1,"expected_source_manifest_sha256":"<raw-64-lowercase-hex-of-canonical-source-bundle>","source_bundle":{"artifact_root":{"algorithm":"sha256-canonical-manifest-v1","digest_file":"artifact-bundle.v1.sha256"},"bundle_id":"paper-raid-bundle-001","challenge_id":"<paper-challenge-uuid>","created_at":"2026-08-05T00:00:00Z","hepta_binding_status":"unbound","human_authority_materialized":false,"object_count":1,"objects":[{"canonical_json":false,"dependencies":[],"logical_path":"paper/main.md","media_type":"text/markdown; charset=utf-8","role":"paper_source","sha256":"<raw-64-lowercase-hex>","size":12}],"required_run_ids":["run-001"],"schema":"paper-raid.artifact-bundle.v1"},"storage_locations":[{"logical_path":"paper/main.md","sha256":"<same-raw-64-lowercase-hex>","uri":"cas://sha256/<same-raw-64-lowercase-hex>","acl":"team"}]}"#,
        ),
        command_editor(
            "Acquire Section Lease / 获取章节租约",
            "Lease a section before proposing a patch.",
            "acquire_section_lease",
            Some(paper_id),
            false,
            r#"{"lease_id":"...","section_key":"methods","holder_binding_id":"...","expected_previous_fencing_token":0,"ttl_seconds":900}"#,
        ),
        command_editor(
            "Agent Proposal / Agent 提案",
            "Paste the exact externally Agent-signed proposal; the BFF never signs for an Agent.",
            "submit_agent_proposal",
            Some(paper_id),
            false,
            r#"{"proposal_id":"...","work_item_id":"...","section_key":"methods","parent_revision_id":"...","proposal_kind":"delivery","payload_hash":"sha256:...","artifact_manifest_id":"...","agent_id":"did:trnm:agent:...","binding_id":"...","agent_key_id":"...","signed_at_unix":0,"signature":"..."}"#,
        ),
        command_editor(
            "Human Decision / 人类决策",
            "Accept, rework, or reject a proposal with a human signature.",
            "record_human_decision",
            Some(paper_id),
            false,
            r#"{"decision_id":"...","proposal_id":"...","expected_proposal_version":1,"decision":"accept","reason_hash":"sha256:...","signing_key_id":"...","signing_public_key":"...","signing_public_key_hash":"sha256:...","signed_at_unix":0,"signature":"..."}"#,
        ),
        command_editor(
            "Section Revision / 章节版本",
            "Create the patch-bound section revision.",
            "create_section_revision",
            Some(paper_id),
            false,
            r#"{"section_revision_id":"...","section_key":"methods","parent_revision_id":"...","proposal_id":"...","lease_id":"...","fencing_token":1,"patch_manifest_id":"...","patch_hash":"sha256:..."}"#,
        ),
        command_editor(
            "Independent Review / 独立评审",
            "Set Child resource ID to the section_revision_id path parameter.",
            "submit_review",
            Some(paper_id),
            true,
            r#"{"review_id":"...","expected_revision_version":1,"verdict":"approve","review_hash":"sha256:..."}"#,
        ),
        command_editor(
            "Merge Section / 合并章节",
            "Advance the authoritative section head with the reviewed revision.",
            "merge_section",
            Some(paper_id),
            false,
            r#"{"merge_id":"...","section_revision_id":"...","expected_revision_version":2,"parent_revision_id":"...","merged_section_revision_id":"...","lease_id":"...","fencing_token":1,"signing_key_id":"...","signing_public_key":"...","signing_public_key_hash":"sha256:...","merged_at_unix":0,"signature":"..."}"#,
        ),
        command_editor(
            "Author Consent / 作者签署",
            "Every human author signs the same release-candidate hash.",
            "create_authorship_consent",
            Some(paper_id),
            false,
            r#"{"consent_id":"...","expected_paper_version":1,"revision_id":"...","release_candidate_hash":"sha256:..."}"#,
        ),
        command_editor(
            "Finalize PaperBundle / 完成论文包",
            "Finalization remains pending_finality; it never releases Chain rewards here.",
            "finalize_joint_paper_submission",
            Some(paper_id),
            false,
            r#"{"submission_id":"...","release_candidate_id":"...","expected_paper_version":1}"#,
        ),
    );
        for editor in [
            command_editor(
                "Start Nakama Research Session / 启动科研会话",
                "Hepta derives and signs the exact create control from the authorization set; the browser cannot choose a Nakama RPC or assertion operation.",
                "create_nakama_research_session_control",
                None,
                false,
                r#"{"authorization_set_id":"..."}"#,
            ),
            command_editor(
                "Resume Nakama Research Session / 恢复科研会话",
                "Resume the exact current session epoch after a runtime restart before retrying a pending replace or complete command.",
                "resume_nakama_research_session_control",
                None,
                false,
                r#"{"session_id":"paper.raid:...","roster_version":1}"#,
            ),
            command_editor(
                "Replace Nakama Roster / 替换科研阵容",
                "Hepta derives the next signed roster epoch only from the already-authorized replacement set.",
                "replace_nakama_research_session_roster_control",
                None,
                false,
                r#"{"authorization_set_id":"..."}"#,
            ),
            command_editor(
                "Complete Nakama Research Session / 完成科研会话",
                "Hepta signs the completion control for the exact current session and roster version; terminal facts remain server-derived.",
                "complete_nakama_research_session_control",
                None,
                false,
                r#"{"session_id":"paper.raid:...","roster_version":1}"#,
            ),
        ] {
            actions.push_str(&editor);
        }
        for editor in [
            command_editor(
                "Evidence Card / 证据卡",
                "Register a source locator and a human verification signature.",
                "create_evidence_card",
                Some(paper_id),
                false,
                r#"{"evidence_card_id":"...","source_uri":"https://...","source_hash":"sha256:...","locator":"p. 3, Table 1","license":"...","verification_key_id":"...","verification_public_key":"...","verification_public_key_hash":"sha256:...","signed_at_unix":0,"verification_signature":"..."}"#,
            ),
            command_editor(
                "Citation Record / 引用记录",
                "Bind a DOI or canonical URL to a verified EvidenceCard.",
                "create_citation_record",
                Some(paper_id),
                false,
                r#"{"citation_id":"...","evidence_card_id":"...","doi":"10....","canonical_url":null,"source_hash":"sha256:...","locator":"p. 3","license":"...","verification_key_id":"...","verification_public_key":"...","verification_public_key_hash":"sha256:...","signed_at_unix":0,"verification_signature":"..."}"#,
            ),
            command_editor(
                "Experiment Plan / 实验计划",
                "Freeze protocol, code, dataset, environment, seed, and stopping-rule lineage.",
                "create_experiment_plan",
                Some(paper_id),
                false,
                r#"{"experiment_plan_id":"...","protocol_snapshot_hash":"sha256:...","code_manifest_id":"...","dataset_manifest_id":"...","environment_manifest_id":"...","seed_policy_hash":"sha256:...","stopping_rule_hash":"sha256:..."}"#,
            ),
            command_editor(
                "Run Record / 实验运行",
                "Register every succeeded, failed, or cancelled run; never cherry-pick only wins.",
                "create_run_record",
                Some(paper_id),
                false,
                r#"{"run_record_id":"...","experiment_plan_id":"...","status":"succeeded","seed":1,"parameters_hash":"sha256:...","logs_manifest_id":"...","outputs_manifest_id":"...","metrics_hash":"sha256:...","failure_hash":null}"#,
            ),
            command_editor(
                "Figure Lineage / 图表谱系",
                "Bind each figure to exact runs, transforms, and a CAS-backed manifest.",
                "create_figure_lineage",
                Some(paper_id),
                false,
                r#"{"figure_lineage_id":"...","figure_key":"figure-1","figure_manifest_id":"...","run_record_ids":["..."],"transform_hash":"sha256:..."}"#,
            ),
            command_editor(
                "Claim–Evidence Link / 论断证据链",
                "Main, numeric, and figure claims require verifiable evidence/run/figure lineage.",
                "create_claim_record",
                Some(paper_id),
                false,
                r#"{"claim_id":"...","claim_key":"claim-1","claim_kind":"main","statement_hash":"sha256:...","evidence_card_ids":["..."],"run_record_ids":["..."],"figure_lineage_ids":["..."]}"#,
            ),
        ] {
            actions.push_str(&editor);
        }
        for editor in [
            command_editor(
                "Contribution Ledger / 贡献账本",
                "CRediT roles and accepted artifacts/reviews produce contribution evidence; they never decide authorship order automatically.",
                "create_contribution_ledger",
                Some(paper_id),
                false,
                r#"{"contribution_ledger_id":"...","expected_paper_version":1,"release_candidate_hash":"sha256:...","entries":[{"player_id":"...","credit_roles":["methodology"],"accepted_artifact_manifest_ids":["..."],"accepted_section_review_ids":["..."]}]}"#,
            ),
            command_editor(
                "Independent Evaluation / 独立评估",
                "One evaluator and two COI-attested reviewers sign the frozen candidate, fixed-point score, hard gates, and tolerance policy.",
                "create_paper_evaluation",
                Some(paper_id),
                false,
                r#"{"evaluation_id":"...","submission_id":"...","supersedes_evaluation_id":null,"release_candidate_hash":"sha256:...","paper_bundle_hash":"sha256:...","tolerance_policy":{"schema":"hepta.paper_raid.tolerance_policy.v1","version":"alpha-1","rules":[{"kind":"absolute","metric":"accuracy","max_delta_micros":1000}]},"reference_metrics_micros":{"accuracy":900000},"score_components":{"method_rigor_bps":2000,"experiment_statistics_bps":1200,"reproducibility_bps":1200,"evidence_citations_bps":1200,"value_originality_bps":1000,"argument_expression_bps":800,"ethics_transparency_bps":500},"hard_gates":{"citations_and_data_authentic":true,"failed_runs_disclosed":true,"all_authors_consented":true,"core_claims_have_evidence":true,"artifact_lineage_complete":true,"license_ethics_coi_complete":true},"evaluator_player_id":"...","evaluator_signing_key_id":"...","evaluator_signing_public_key":"...","evaluator_signing_public_key_hash":"sha256:...","evaluator_coi_attestation_hash":"sha256:...","evaluator_signed_at_unix":0,"evaluator_signature":"...","reviewer_attestations":[]}"#,
            ),
            command_editor(
                "Reproduction Report / 复现报告",
                "Set Child resource ID to evaluation_id. The reproducer must be independent and all stochastic comparisons use the frozen tolerance policy.",
                "submit_reproduction",
                Some(paper_id),
                true,
                r#"{"reproduction_id":"...","supersedes_reproduction_id":null,"release_candidate_hash":"sha256:...","paper_bundle_hash":"sha256:...","observed_metrics_micros":{"accuracy":899500},"statistical_evidence":{},"seed_set_hash":"sha256:...","environment_hash":"sha256:...","run_manifest_hash":"sha256:...","reproducer_player_id":"...","signing_key_id":"...","signing_public_key":"...","signing_public_key_hash":"sha256:...","coi_attestation_hash":"sha256:...","signed_at_unix":0,"signature":"..."}"#,
            ),
            command_editor(
                "Appeal / 申诉",
                "Set Child resource ID to evaluation_id. Appeal evidence stays on hold and cannot release final rewards.",
                "submit_appeal",
                Some(paper_id),
                true,
                r#"{"appeal_id":"...","release_candidate_hash":"sha256:...","appellant_player_id":"...","grounds_hash":"sha256:...","evidence_manifest_hash":"sha256:...","signing_key_id":"...","signing_public_key":"...","signing_public_key_hash":"sha256:...","signed_at_unix":0,"signature":"..."}"#,
            ),
            command_editor(
                "Resolve Appeal / 裁决申诉",
                "Set Child resource ID to appeal_id. The resolver must satisfy the independent-panel rules.",
                "resolve_appeal",
                Some(paper_id),
                true,
                r#"{"resolution_id":"...","outcome":"denied","superseding_evaluation_id":null,"decision_hash":"sha256:...","resolver_player_id":"...","signing_key_id":"...","signing_public_key":"...","signing_public_key_hash":"sha256:...","signed_at_unix":0,"signature":"..."}"#,
            ),
        ] {
            actions.push_str(&editor);
        }
        actions
    } else {
        unavailable("playable actions")
    };
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">PAPER ROOM · 论文作战室</span><h1>{}</h1><p>Paper <code>{}</code></p><p class="source-state">Hepta paper: {}</p>{}</section>
        <section class="grid">
          {}
          {}
          {}
          {}
        </section>
        <section class="grid">
          {}
          {}
          {}
          {}
          {}
          {}
          {}
          {}
        </section>"#,
        escape(&title),
        escape(paper_id),
        escape(room.label()),
        settlement,
        fact("Phase Gates / 阶段门", &phase),
        fact("Author Sign-off / 作者签署", &signoff),
        timeline_card(paper_id, &timeline_text),
        fact("Settlement / 结算", settlement_fact),
        room_card("Sections & Revisions / 章节与版本", "section_revisions"),
        room_card("Evidence / 证据", "evidence_cards"),
        room_card("Claims / 论断", "claims"),
        room_card("Citations / 引用", "citations"),
        room_card("Runs (including failed) / 全部实验", "runs"),
        room_card("Figures / 图表", "figures"),
        room_card("Review / 内审", "section_reviews"),
        review_card("Review State / 评审状态", "evaluations"),
    );
    let body = format!(
        "{body}<section class=\"grid\">{}{}{}{}{}{}</section>",
        review_card("Contribution / 贡献", "contribution_ledgers"),
        review_card("Evaluations / 评估", "evaluations"),
        review_card("Reproductions / 复现", "reproductions"),
        review_card("Appeals / 申诉", "appeals"),
        review_card("Resolutions / 裁决", "resolutions"),
        review_card("Raid XP / 协作经验", "raid_scores"),
    );
    let artifacts = artifact_links(room, paper_id);
    let body = format!(
        "{body}{artifacts}<section class=\"panel\"><h2>Playable Actions / 可玩操作</h2><p>Complex signed objects stay external; this browser submits them through typed Hepta routes without curl.</p><div class=\"action-grid\">{actions}</div></section>"
    );
    page("Paper Raid Room", &identity.display_name, &body, true)
}

pub fn login_page() -> Response {
    let body = r#"<section class="hero"><span class="eyebrow">PAPER RAID · ALPHA</span><h1>Research together.</h1><p>Use one of the three externally provisioned alpha login keys. The key is cleared immediately and is never written to browser storage.</p></section><section class="panel narrow"><form id="login-form"><label>Alpha login key / 登录密钥<input type="password" name="login_key" minlength="32" autocomplete="off" required></label><button type="submit">Enter Paper Raid / 进入论文远征</button><output></output></form></section>"#;
    page("Paper Raid Login", "not signed in", body, false)
}

pub fn onboarding(identity: &AlphaIdentity, stage: OnboardingStage) -> Response {
    let stage_content = match stage {
        OnboardingStage::HumanRegistration => r#"<section class="grid"><article class="card"><h2>1 · Browser-only key / 浏览器私钥</h2><p>Ed25519 signing happens locally with WebCrypto. The BFF never receives private-key bytes.</p></article><article class="card"><h2>2 · Encrypted recovery / 加密备份</h2><p>AES-256-GCM + PBKDF2-SHA-256 encrypts the PKCS#8 key into a downloaded bundle. Keep the file and passphrase separately.</p></article><article class="card"><h2>3 · Human authority / 人类责任</h2><p>Agent output remains a proposal. Acceptance, independent review, authorship consent, and appeal remain human-signed acts.</p></article></section>
        <section class="panel narrow"><h2>Step 1 · Generate and export / 生成并导出</h2><p>Create one encrypted recovery bundle. Generating never registers and never silently replaces an in-memory key.</p><form id="human-key-create-form">
          <label>Encryption passphrase / 密钥包加密口令<input name="passphrase" type="password" minlength="16" autocomplete="new-password" required></label>
          <label>Confirm passphrase / 确认口令<input name="confirm_passphrase" type="password" minlength="16" autocomplete="new-password" required></label>
          <button type="submit">Generate, encrypt, and download / 生成、加密并下载</button>
          <output></output>
        </form></section>
        <section class="panel narrow"><h2>Step 2 · Register loaded key / 注册当前内存密钥</h2><p>Register the key currently loaded in this tab. After an uncertain network result, open <a href="/league/start">/league/start</a> first; if still unregistered, import the original bundle and retry with the same key.</p><form id="human-key-register-form"><button type="submit">Register current in-memory key / 注册当前内存密钥</button><output></output></form></section>"#.to_string(),
        OnboardingStage::AgentBinding => format!(
            r#"<section class="grid"><article class="card"><h2>Human identity ready / 人类身份已就绪</h2><p>Player <code>{}</code> and Consumer subject <code>{}</code> are registered. One independently controlled external Agent must now prove its own key.</p></article><article class="card"><h2>External signature only / 仅外部签名</h2><p>The Agent signs <code>hepta.paper_raid.agent_binding_proof.v2</code>, including the displayed subject/player, its public-key hash, and nonce = idempotency_key. Paste only the public request fields here; never paste an Agent seed, private key, mnemonic, token, or API credential.</p></article><article class="card"><h2>Fail closed / 失败关闭</h2><p>The BFF forwards the exact signed object with a Consumer assertion. It cannot sign, repair, or silently change the Agent proof.</p></article></section>
            <section class="panel"><h2>Bind external Agent / 绑定外部 Agent</h2><form class="agent-binding-form" data-player-id="{}"><label>Exact externally signed JSON / 外部 Agent 已签名 JSON<textarea name="payload" rows="16" spellcheck="false" required>{{
  "binding_id": "00000000-0000-4000-8000-000000000000",
  "player_id": "{}",
  "agent_id": "did:trnm:agent:...",
  "agent_key_id": "sha256:...",
  "agent_public_key": "...",
  "agent_proof_nonce": "00000000-0000-4000-8000-000000000000",
  "agent_proof_issued_at_unix": 0,
  "agent_proof_expires_at_unix": 0,
  "agent_proof_signature": "...",
  "idempotency_key": "00000000-0000-4000-8000-000000000000"
}}</textarea></label><button type="submit">Verify and bind / 验证并绑定</button><output></output></form></section>"#,
            escape(&identity.player_id.to_string()),
            escape(&identity.subject_id),
            escape(&identity.player_id.to_string()),
            escape(&identity.player_id.to_string()),
        ),
        OnboardingStage::Unavailable => r#"<section class="panel narrow"><p class="status missing">Hepta onboarding is unavailable. Registration is fail-closed; retry after the authority is healthy.</p></section>"#.to_string(),
    };
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">SECURE ONBOARDING · 安全入驻</span><h1>Human + external Agent</h1><p>{}，人类签名密钥只存在于浏览器内存或加密恢复包；Agent 私钥始终留在外部 Agent。</p></section>{}"#,
        escape(&identity.display_name),
        stage_content,
    );
    page("Paper Raid Onboarding", &identity.display_name, &body, true)
}

pub fn browser_script() -> Response {
    let mut response = include_str!("browser.js").into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

pub fn browser_stylesheet() -> Response {
    let mut response = CSS.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn page(title: &str, player: &str, content: &str, authenticated: bool) -> Response {
    let key_vault = if authenticated {
        r#"<section class="panel key-vault"><details><summary>Local signing key / 本地签名密钥</summary><p class="muted">Import your encrypted recovery bundle when this tab needs to sign a human action. Decryption stays in memory and is discarded when the tab closes.</p><form class="human-key-import-form"><label>Encrypted key bundle / 加密密钥包<input name="key_bundle" type="file" accept="application/json,.json" required></label><label>Passphrase / 口令<input name="passphrase" type="password" minlength="16" autocomplete="current-password" required></label><button type="submit">Decrypt into this tab / 仅解密到当前标签页</button><output></output></form><button class="forget-human-key" type="button">Forget in-memory key / 清除内存密钥</button><output class="human-key-status">No in-memory key / 当前无内存密钥</output></details></section>"#
    } else {
        ""
    };
    let document = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><link rel="stylesheet" href="/assets/paper-raid.css"><script src="/assets/paper-raid.js" defer></script></head><body data-authenticated="{}"><header><a href="/league/start">HEPTA // PAPER RAID</a><span>{}</span></header><main>{}{}<aside id="toast" aria-live="polite" hidden></aside></main><footer>Hepta is the research record. Nakama is the ordered collaboration timeline.</footer></body></html>"#,
        escape(title),
        authenticated,
        escape(player),
        key_vault,
        content
    );
    let mut response = Html(document).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn fact(title: &str, value: &str) -> String {
    format!(
        "<article class=\"card\"><h2>{}</h2><p>{}</p></article>",
        escape(title),
        escape(value)
    )
}

fn unavailable_card(title: &str, name: &str) -> String {
    format!(
        "<article class=\"card unavailable\"><h2>{}</h2>{}</article>",
        escape(title),
        unavailable(name)
    )
}

fn unavailable(name: &str) -> String {
    format!(
        "<p><span class=\"dot\"></span>{}: unavailable / 暂不可用</p>",
        escape(name)
    )
}

fn record_links(
    state: ReadState<'_>,
    id_field: &str,
    status_field: &str,
    prefix: &str,
    direct_link: bool,
) -> String {
    let Some(records) = state.value().and_then(Value::as_array) else {
        return unavailable("records");
    };
    if records.is_empty() {
        return "<p class=\"muted\">No records yet / 暂无记录</p>".into();
    }
    let mut items = String::new();
    for record in records {
        let id = scalar(record.get(id_field));
        let status = scalar(record.get(status_field));
        let link_id = if direct_link {
            Some(id.clone())
        } else {
            record
                .get("matched_proposal_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let label = format!(
            "<code>{}</code><span class=\"pill\">{}</span>",
            escape(&id),
            escape(&status)
        );
        match link_id {
            Some(link_id) => items.push_str(&format!(
                "<li><a href=\"{}{}\">{}</a></li>",
                escape(prefix),
                escape(&link_id),
                label
            )),
            None => items.push_str(&format!("<li>{label}</li>")),
        }
    }
    format!("<ul class=\"record-list\">{items}</ul>")
}

fn command_editor(
    title: &str,
    help: &str,
    command: &str,
    resource_id: Option<&str>,
    requires_child: bool,
    placeholder: &str,
) -> String {
    let resource = resource_id.unwrap_or_default();
    let child = if requires_child {
        "<label>Child resource ID / 子资源 ID<input name=\"child_id\" inputmode=\"text\" autocomplete=\"off\" required></label>"
    } else {
        ""
    };
    let local_sign = if matches!(
        command,
        "accept_research_team_membership"
            | "submit_review"
            | "create_authorship_consent"
            | "submit_appeal"
    ) {
        "<button class=\"local-sign\" type=\"button\">Sign locally / 本地签名</button>"
    } else {
        ""
    };
    format!(
        r#"<article class="action"><h3>{}</h3><p>{}</p><form class="command-form" data-command="{}" data-resource-id="{}">{}<label>Exact typed JSON / 精确类型 JSON<textarea name="payload" rows="9" spellcheck="false" required>{}</textarea></label>{}<button type="submit">Submit typed action / 提交</button><output></output></form></article>"#,
        escape(title),
        escape(help),
        escape(command),
        escape(resource),
        child,
        escape(placeholder),
        local_sign,
    )
}

fn artifact_upload(paper_id: &str) -> String {
    format!(
        r#"<article class="action"><h3>Upload Artifact / 上传工件</h3><p>The BFF recomputes the digest, writes append-only CAS bytes, and returns the only storage URI allowed in an ArtifactManifest.</p><form class="artifact-form" data-paper-id="{}"><label>Artifact file / 工件文件<input name="artifact" type="file" required></label><label>Canonical media type / 媒体类型<select name="media_type" required><option>application/x-bibtex</option><option>text/csv; charset=utf-8</option><option>application/json</option><option>text/markdown; charset=utf-8</option><option>application/pdf</option><option>text/x-python; charset=utf-8</option><option>image/svg+xml</option><option>text/plain; charset=utf-8</option><option>application/octet-stream</option><option>application/zip</option><option>application/gzip</option></select></label><button type="submit">Hash + upload / 哈希并上传</button><output></output></form></article>"#,
        escape(paper_id)
    )
}

fn timeline_card(paper_id: &str, summary: &str) -> String {
    format!(
        r#"<article class="card"><h2>Timeline & Replay / 时间线与回放</h2><p>{}</p><button class="timeline-refresh" data-paper-id="{}" type="button">Load authoritative replay / 加载权威回放</button><output></output></article>"#,
        escape(summary),
        escape(paper_id),
    )
}

fn artifact_links(room: ReadState<'_>, paper_id: &str) -> String {
    let Some(manifests) = room
        .value()
        .and_then(|value| value.get("artifact_manifests"))
        .and_then(Value::as_array)
    else {
        return unavailable_card("Artifacts / 工件", "artifact manifests");
    };
    let mut links = String::new();
    for manifest in manifests {
        let Some(objects) = manifest.get("objects").and_then(Value::as_array) else {
            continue;
        };
        for object in objects {
            let Some(digest) = object.get("sha256").and_then(Value::as_str) else {
                continue;
            };
            let Ok(route_digest) = crate::cas::digest_label_from_raw_sha256(digest) else {
                continue;
            };
            let logical_path = scalar(object.get("logical_path"));
            links.push_str(&format!(
                r#"<li><a href="/api/papers/{}/artifacts/{}"><span>{}</span><code>{}</code></a></li>"#,
                escape(paper_id),
                escape(&route_digest),
                escape(&logical_path),
                escape(&route_digest),
            ));
        }
    }
    if links.is_empty() {
        links.push_str("<li class=\"muted\">No registered artifacts / 暂无已登记工件</li>");
    }
    format!(
        "<section class=\"panel\"><h2>Registered Artifacts / 已登记工件</h2><ul class=\"record-list\">{links}</ul></section>"
    )
}

fn scalar(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => "unavailable".into(),
    }
}

pub fn escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

const CSS: &str = r#"
:root{color-scheme:dark;--bg:#070b12;--panel:#101826;--line:#253653;--text:#e9f2ff;--muted:#8ba0ba;--cyan:#55e6ff;--amber:#ffca68;--pink:#ff6ba8}
*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 20% 0,#11213a 0,#070b12 42%);color:var(--text);font:15px/1.55 Inter,ui-sans-serif,system-ui,sans-serif;min-height:100vh}
header{align-items:center;border-bottom:1px solid var(--line);display:flex;justify-content:space-between;padding:16px clamp(18px,4vw,56px);position:sticky;top:0;background:#070b12e8;backdrop-filter:blur(12px)}header a{color:var(--cyan);font-weight:900;letter-spacing:.12em;text-decoration:none}header span,footer{color:var(--muted)}
main{margin:auto;max-width:1180px;padding:clamp(24px,5vw,64px) clamp(16px,4vw,44px)}.hero{border-left:4px solid var(--cyan);padding:8px 0 12px 22px;margin-bottom:28px}.eyebrow{color:var(--amber);font-size:12px;font-weight:800;letter-spacing:.16em}.hero h1{font-size:clamp(32px,7vw,72px);line-height:1;margin:10px 0}.hero p{color:var(--muted);max-width:760px}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:14px;margin:14px 0}.card,.panel{background:linear-gradient(145deg,#142036,#0d1421);border:1px solid var(--line);border-radius:14px;padding:18px;min-height:120px}.card h2,.panel h2{font-size:14px;letter-spacing:.05em;margin:0 0 12px}.card p{color:var(--text)}.unavailable{border-style:dashed;color:var(--muted)}.muted,.action p{color:var(--muted)}.dot{background:var(--pink);border-radius:50%;display:inline-block;height:8px;margin-right:8px;width:8px}.pill,.status{border:1px solid var(--amber);border-radius:999px;color:var(--amber);display:inline-block;font-size:12px;font-weight:800;padding:4px 9px}.status{padding:6px 12px}.status.missing{border-color:var(--pink);color:var(--pink)}.source-state{color:var(--muted);font-size:12px}.roster,.record-list{display:grid;gap:10px;list-style:none;margin:0;padding:0}.roster li{align-items:center;border-bottom:1px solid var(--line);display:grid;gap:8px;grid-template-columns:90px 1fr 1fr 1fr;padding:10px 0}.roster span{color:var(--muted);overflow-wrap:anywhere}.record-list li,.record-list a{align-items:center;display:flex;gap:8px;justify-content:space-between}.record-list a{color:var(--text);text-decoration:none;width:100%}.action-grid{display:grid;gap:14px;grid-template-columns:repeat(2,minmax(0,1fr))}.action{border:1px solid var(--line);border-radius:12px;padding:16px}.action h3{margin-top:0}form{display:grid;gap:12px}label{color:var(--muted);display:grid;font-size:12px;gap:6px}input,textarea,select,button{background:#07101d;border:1px solid var(--line);border-radius:8px;color:var(--text);font:inherit;padding:10px 12px}textarea{font:12px/1.45 ui-monospace,SFMono-Regular,Consolas,monospace;resize:vertical}button{background:#12334a;border-color:var(--cyan);color:var(--cyan);cursor:pointer;font-weight:800}button:hover{filter:brightness(1.2)}button.danger{border-color:var(--pink);color:var(--pink)}button:disabled{cursor:wait;opacity:.55}output{color:var(--amber);font:12px/1.45 ui-monospace,SFMono-Regular,Consolas,monospace;overflow-wrap:anywhere;white-space:pre-wrap}output.result-error{color:var(--pink)}output.result-ok{color:var(--amber)}.narrow{margin:auto;max-width:540px}#toast{background:#101826;border:1px solid var(--line);border-radius:10px;bottom:18px;display:block;max-width:min(520px,90vw);padding:12px 16px;position:fixed;right:18px;z-index:10}#toast[hidden]{display:none}code{color:var(--cyan);overflow-wrap:anywhere}footer{padding:28px;text-align:center}
.key-vault{margin-bottom:18px;min-height:auto}.key-vault summary{color:var(--cyan);cursor:pointer;font-weight:800}.key-vault form{margin:14px 0}.human-key-status{display:block;margin-top:10px}
.mission-board{display:grid;gap:18px;grid-template-columns:minmax(220px,.8fr) minmax(0,2fr);margin-bottom:20px}.mission-board h2{font-size:24px;letter-spacing:0;margin:6px 0}.raid-steps{display:grid;gap:8px;grid-template-columns:repeat(5,minmax(0,1fr));list-style:none;margin:0;padding:0}.raid-steps li{border:1px solid var(--line);border-radius:10px;display:grid;gap:8px;padding:12px}.raid-steps li>span{color:var(--muted);font-size:11px;font-weight:900}.raid-steps strong{display:block}.raid-steps p{color:var(--muted);font-size:11px;line-height:1.35;margin:4px 0 0}.raid-steps [data-step-state=current]{background:#12334a;border-color:var(--cyan)}.raid-steps [data-step-state=current]>span{color:var(--cyan)}.raid-steps [data-step-state=complete]{border-color:#3b8f78}.raid-steps [data-step-state=complete]>span{color:#67e8b5}.role-kit{border:1px solid var(--line);border-radius:10px;display:grid;gap:7px;margin:0;padding:12px}.role-kit legend{color:var(--amber);font-size:12px;font-weight:800;padding:0 6px}.role-kit span{color:var(--muted);font-size:12px}.role-kit strong{color:var(--text)}
@media(max-width:820px){.grid,.action-grid,.mission-board,.raid-steps{grid-template-columns:1fr}.roster li{align-items:start;grid-template-columns:1fr}.hero h1{font-size:38px}header{position:static}.card{min-height:auto}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;
    use http_body_util::BodyExt;
    use uuid::Uuid;

    #[tokio::test]
    async fn html_escapes_all_dynamic_fields() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let team = serde_json::json!({
            "members":[{"role":"<img src=x onerror=alert(1)>"}]
        });
        let response = formation(
            &identity,
            "<script>alert(1)</script>",
            ReadState::Unavailable,
            ReadState::Available(&team),
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect HTML")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 HTML");
        assert!(!body.contains("<script>alert(1)</script>"));
        assert!(!body.contains("<img src=x onerror=alert(1)>"));
        assert!(body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(body.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert_eq!(escape("<script>\"'&"), "&lt;script&gt;&quot;&#x27;&amp;");
    }

    #[tokio::test]
    async fn lobby_guides_the_first_raid_without_a_role_json_editor() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let challenges = serde_json::json!([{
            "challenge_id":"challenge-a",
            "title":"Reproduce the signal",
            "description":"Separate the claimed effect from measurement noise.",
            "status":"open"
        }]);
        let tickets = serde_json::json!([]);
        let proposals = serde_json::json!([]);
        let bindings = serde_json::json!([]);
        let response = lobby(
            &identity,
            ReadState::Available(&challenges),
            ReadState::Available(&tickets),
            ReadState::Available(&proposals),
            ReadState::Available(&bindings),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect lobby")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 lobby");
        assert!(body.contains("FIRST RAID / 首局任务"));
        assert!(body.contains("data-step-state=\"current\"><span>01</span>"));
        assert!(body.contains("type=\"hidden\" value=\"captain,evidence,experiment\""));
        assert!(body.contains("Start my first Raid / 开始首局"));
        assert!(!body.contains("Roles / 职业<input"));
    }

    #[tokio::test]
    async fn paper_room_is_pending_only_and_missing_is_distinct() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let upstream_claim = format!("{}{}", "final", "ized");
        let room = serde_json::json!({
            "paper": {
                "title":"Alpha Paper",
                "phase":"submission_ready",
                "settlement": upstream_claim
            },
            "authorship_consents": [],
            "section_revisions": [],
            "evidence_cards": [],
            "claims": [],
            "citations": [],
            "runs": [],
            "figures": [],
            "section_reviews": []
        });
        let response = paper_room(
            &identity,
            "paper-a",
            ReadState::Available(&room),
            ReadState::Unavailable,
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 room");
        assert!(body.contains("pending_finality"));
        assert!(!body.contains(&format!("{}{}", "final", "ized")));
        assert!(body.contains("data-command=\"submit_appeal\""));
        assert!(body.contains("Sign locally / 本地签名"));
        for command in [
            "create_nakama_research_session_control",
            "resume_nakama_research_session_control",
            "replace_nakama_research_session_roster_control",
            "complete_nakama_research_session_control",
        ] {
            assert!(body.contains(&format!("data-command=\"{command}\"")));
        }
        for media_type in [
            "application/x-bibtex",
            "text/csv; charset=utf-8",
            "application/json",
            "text/markdown; charset=utf-8",
            "application/pdf",
            "text/x-python; charset=utf-8",
            "image/svg+xml",
            "text/plain; charset=utf-8",
            "application/octet-stream",
        ] {
            assert!(body.contains(&format!("<option>{media_type}</option>")));
        }
        assert!(!body.contains("<option>text/markdown</option>"));
        assert!(!body.contains("<option>text/x-bibtex; charset=utf-8</option>"));

        let missing = paper_room(
            &identity,
            "paper-missing",
            ReadState::NotFound,
            ReadState::NotFound,
            ReadState::NotFound,
        );
        let missing = missing
            .into_body()
            .collect()
            .await
            .expect("collect missing room")
            .to_bytes();
        let missing = std::str::from_utf8(&missing).expect("UTF-8 missing room");
        assert!(missing.contains("not_found"));
        assert!(!missing.contains("pending_finality"));
    }

    #[test]
    fn artifact_links_convert_only_final_hepta_raw_sha256_to_bff_digest_routes() {
        let raw = "ab".repeat(32);
        let room = serde_json::json!({
            "artifact_manifests": [{
                "objects": [{
                    "canonical_json": false,
                    "dependencies": [],
                    "logical_path": "paper/main.md",
                    "media_type": "text/markdown; charset=utf-8",
                    "role": "paper_source",
                    "sha256": raw,
                    "size": 12
                }]
            }]
        });
        let links = artifact_links(ReadState::Available(&room), "paper-a");
        assert!(links.contains(&format!("/artifacts/sha256:{}", "ab".repeat(32))));
        assert!(links.contains("text") || links.contains("paper/main.md"));

        let mut prefixed = room.clone();
        prefixed["artifact_manifests"][0]["objects"][0]["sha256"] =
            Value::String(format!("sha256:{}", "ab".repeat(32)));
        let links = artifact_links(ReadState::Available(&prefixed), "paper-a");
        assert!(!links.contains("/artifacts/"));
        assert!(links.contains("No registered artifacts"));
    }

    #[tokio::test]
    async fn browser_is_external_script_only_and_never_persists_alpha_keys() {
        let login = login_page();
        assert_eq!(
            login
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some(
                "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"
            )
        );
        let body = login
            .into_body()
            .collect()
            .await
            .expect("collect login")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 login");
        assert!(body.contains("<script src=\"/assets/paper-raid.js\" defer></script>"));
        assert!(body.contains("<link rel=\"stylesheet\" href=\"/assets/paper-raid.css\">"));
        assert!(!body.contains("<script>"));
        assert!(!body.contains("<style>"));

        let script = include_str!("browser.js");
        assert!(!script.contains("localStorage"));
        assert!(!script.contains("sessionStorage"));
        assert!(!script.contains(".style"));
        assert!(!script.contains("PAPER_RAID_BFF_NAKAMA_HTTP_KEY"));
        assert!(script.contains("input.value = \"\""));
        assert!(script.contains("x-paper-raid-csrf"));
        assert!(script.contains("PBKDF2"));
        assert!(script.contains("AES-GCM"));
        assert!(script.contains("Ed25519"));
        assert!(script.contains("privatePkcs8.fill(0)"));
        assert!(script.contains("decrypted.fill(0)"));
    }

    #[tokio::test]
    async fn onboarding_exports_only_a_browser_encrypted_bundle() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let response = onboarding(&identity, OnboardingStage::HumanRegistration);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect onboarding")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 onboarding");
        assert!(body.contains("human-key-create-form"));
        assert!(body.contains("human-key-register-form"));
        assert!(body.contains("AES-256-GCM + PBKDF2-SHA-256"));
        assert!(body.contains("import the original bundle and retry with the same key"));
        assert!(!body.contains("Generate, encrypt, export, and register"));
        assert!(!body.contains("name=\"private_key\""));

        let script = include_str!("browser.js");
        assert!(script.contains("forget_current_in_memory_key_before_generating_another"));
        assert!(script.contains("Registration may already be committed"));
        assert!(script.contains("window.location.assign(\"/league/start\")"));
    }

    #[tokio::test]
    async fn agent_onboarding_forwards_only_exact_external_public_proof() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let response = onboarding(&identity, OnboardingStage::AgentBinding);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect Agent onboarding")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 Agent onboarding");
        assert!(body.contains("agent-binding-form"));
        assert!(body.contains("agent_proof_signature"));
        assert!(body.contains("agent_proof_nonce"));
        assert!(body.contains(&identity.player_id.to_string()));
        assert!(!body.contains("name=\"agent_private_key\""));
        assert!(!body.contains("name=\"agent_seed\""));

        let script = include_str!("browser.js");
        assert!(script.contains("agent_proof_nonce_must_equal_idempotency_key"));
        assert!(script.contains("sendCommand(\"create_agent_binding\", null, null, payload)"));
    }

    #[tokio::test]
    async fn lobby_rotates_external_agent_keys_only_with_dual_public_proof() {
        let identity =
            AlphaIdentity::test_identity("subject-rotation", Uuid::new_v4(), Uuid::new_v4());
        let binding_id = Uuid::new_v4();
        let bindings = serde_json::json!([{
            "binding_id": binding_id,
            "player_id": identity.player_id,
            "agent_id": "did:trnm:agent:alpha",
            "agent_key_id": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "agent_public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "status": "active",
            "version": 3
        }]);
        let response = lobby(
            &identity,
            ReadState::Unavailable,
            ReadState::Unavailable,
            ReadState::Unavailable,
            ReadState::Available(&bindings),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect Lobby")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 Lobby");
        assert!(body.contains("agent-rotation-form"));
        assert!(body.contains("hepta.paper_raid.agent_binding_key_rotation.v2"));
        assert!(body.contains(&binding_id.to_string()));
        assert!(body.contains(&identity.subject_id));
        assert!(body.contains("old_key_signature"));
        assert!(body.contains("new_key_signature"));
        assert!(!body.contains("agent_private_key"));

        let script = include_str!("browser.js");
        assert!(script.contains("agent_rotation_must_change_public_key"));
        assert!(script.contains("\"rotate_agent_binding_key\""));
    }
}
