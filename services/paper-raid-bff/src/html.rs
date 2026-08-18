use std::collections::{HashMap, HashSet};

use axum::{
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use hepta_paper_raid_contracts::{
    canonical_json_sha256, paper_appeal_resolution_signing_bytes, paper_appeal_signing_bytes,
    paper_bundle_hash, paper_evaluation_signing_bytes, paper_release_candidate_hash,
    paper_review_attestation_signing_bytes, sha256_digest, verify_frozen_review_bundle,
    FrozenReviewBundleV1, PaperAppealResolutionSigningV1, PaperAppealSigningV1, PaperBundleV2,
    PaperEvaluationSigningV1, PaperReleaseCandidateV2, PaperReviewAttestationSigningV1,
    REVIEW_OBJECT_DOWNLOAD_PATH_V1,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    config::{AlphaAuthorRole, AlphaIdentity, AlphaIdentityScope},
    hepta::{
        AuthenticatedPaperReviewState, AuthenticatedPaperRoom, HeptaPaperTerminalOutcome,
        HeptaPaperTerminalReason,
    },
    practice::PracticeStageV1,
    practice_http::PracticePlayerViewV1,
};

pub enum ReadState<'a, T: ?Sized = Value> {
    Available(&'a T),
    NotFound,
    Unavailable,
}

impl<'a, T: ?Sized> Copy for ReadState<'a, T> {}

impl<'a, T: ?Sized> Clone for ReadState<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnboardingStage {
    HumanRegistration,
    AgentBinding,
    Unavailable,
}

struct ChallengeQueueAvailability {
    state: &'static str,
    explanation: &'static str,
    button_label: &'static str,
    is_open: bool,
}

fn challenge_queue_availability(status: &str) -> ChallengeQueueAvailability {
    match status {
        "open" => ChallengeQueueAvailability {
            state: "open",
            explanation: "Open for authoritative matchmaking / 已开放权威匹配",
            button_label: "Start my first Raid / 开始首局",
            is_open: true,
        },
        "closed" => ChallengeQueueAvailability {
            state: "closed",
            explanation: "Closed by Hepta; this challenge cannot accept a matchmaking ticket / Hepta 已关闭该挑战，当前无法排队",
            button_label: "Challenge closed / 挑战已关闭",
            is_open: false,
        },
        "draft" => ChallengeQueueAvailability {
            state: "draft",
            explanation: "Still in draft; Hepta has not opened this challenge / 仍为草稿，Hepta 尚未开放该挑战",
            button_label: "Not open yet / 尚未开放",
            is_open: false,
        },
        _ => ChallengeQueueAvailability {
            state: "unavailable",
            explanation: "Challenge availability is not authoritative; matchmaking stays disabled / 挑战开放状态不可验证，匹配保持禁用",
            button_label: "Unavailable / 暂不可用",
            is_open: false,
        },
    }
}

impl<'a, T: ?Sized> ReadState<'a, T> {
    fn value(self) -> Option<&'a T> {
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
    raid_state: ReadState<'_>,
) -> Response {
    let player_id_text = identity.player_id.to_string();
    let queue_agent_ready = bindings
        .value()
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .filter(|binding| {
                    binding.get("status").and_then(Value::as_str) == Some("active")
                        && binding.get("player_id").and_then(Value::as_str)
                            == Some(player_id_text.as_str())
                })
                .count()
                == 1
        });
    let challenge_cards = challenges
        .value()
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|challenge| {
                    let challenge_id = scalar(challenge.get("challenge_id"));
                    let title = scalar(challenge.get("title"));
                    let gameplay = challenge_gameplay_summary(challenge);
                    let status = scalar(challenge.get("status"));
                    let availability = challenge_queue_availability(&status);
                    let (role_choices, authorized_roles) = author_role_choices(identity);
                    let queue_eligible = availability.is_open
                        && !authorized_roles.is_empty()
                        && queue_agent_ready;
                    format!(
                        r#"<article class="card challenge" data-challenge-status="{}"><span class="pill">{}</span><h2>{}</h2>{}<code>{}</code><form class="queue-form" data-challenge-id="{}" data-challenge-status="{}" data-queue-eligible="{}" data-authorized-roles="{}" data-agent-ready="{}"><p class="challenge-availability" data-challenge-availability="{}">{}</p><fieldset class="role-kit"><legend>Role preferences / 角色偏好</legend><p class="muted">Checked roles are acceptable; the first listed role is your preferred assignment / 勾选可接受角色，首项为首选</p>{}</fieldset><label>Play window / 开局时间<select name="availability" required><option value="alpha-window">Join the next Alpha window / 下一场 Alpha</option><option value="now">Ready now / 现在可玩</option></select></label><fieldset class="party-code-panel"><legend>Premade team (optional) / 预组队（可选）</legend><label>One-time party code / 一次性组队码<input name="party_code" type="text" inputmode="text" autocomplete="off" spellcheck="false" minlength="40" maxlength="40" pattern="PR1-[0-9a-f]{{8}}-[0-9a-f]{{4}}-4[0-9a-f]{{3}}-[89ab][0-9a-f]{{3}}-[0-9a-f]{{12}}" placeholder="PR1-xxxxxxxx-xxxx-4xxx-8xxx-xxxxxxxxxxxx"></label><button class="generate-party-code" type="button">Generate private code / 生成私密组队码</button><p class="muted">Share the raw code with exactly two teammates out of band. Only its SHA-256 digest leaves this browser. A party code grants no account or game authority. / 请仅线下分享给另外两名队友；浏览器外只发送摘要，组队码不授予身份或游戏权限。</p></fieldset><p class="muted">{}</p><button class="queue-submit" data-queue-action="join" type="submit" {}>{}</button><output></output></form></article>"#,
                        escape(availability.state),
                        escape(&status),
                        escape(&title),
                        gameplay,
                        escape(&challenge_id),
                        escape(&challenge_id),
                        escape(availability.state),
                        queue_eligible,
                        escape(&authorized_roles),
                        queue_agent_ready,
                        escape(availability.state),
                        escape(availability.explanation),
                        role_choices,
                        if queue_agent_ready {
                            "Agent ready: exactly one active binding / Agent 已就绪"
                        } else {
                            "Pair exactly one active Agent before joining / 请先配对且仅保留一个活跃 Agent"
                        },
                        if queue_eligible {
                            ""
                        } else {
                            "disabled"
                        },
                        escape(availability.button_label),
                    )
                })
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| unavailable("challenge catalog"));
    let ticket_cards = matchmaking_ticket_cards(tickets);
    let proposal_cards = active_team_proposal_links(proposals);
    let binding_controls = active_agent_binding_controls(identity, bindings);
    let mission_board = first_raid_mission_board(tickets, proposals, raid_state);
    let continue_raid = active_raid_card(raid_state);
    let agent_pairing = agent_bridge_pairing_panel();
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">PAPER RAID · 论文远征</span><h1>Research Lobby</h1><p>Welcome, {}. Hepta is the only matchmaking and research authority.</p></section>
        <section class="panel practice-entry"><span class="pill">15–20 MIN · SOLO</span><h2>Learn all three Author roles / 单人熟悉三个作者角色</h2><p>Preview Captain, Evidence, and Experiment decisions in a separate unranked practice. It creates no scientific finality, qualification, ranking, score, reward, or economic authority.</p><a class="button" href="/league/practice">Open solo practice / 打开单人练习</a></section>
        {}{}{}
        <section class="panel"><h2>Challenges / 研究挑战</h2><p class="source-state">Hepta: {}</p><div class="grid">{}</div></section>
        <section class="grid"><article class="card"><h2>My Queue / 我的队列</h2><p class="source-state">Hepta: {}</p>{}</article><article class="card"><h2>Team Proposals / 组队提案</h2><p class="source-state">Hepta: {}</p>{}</article><article class="card"><h2>Alpha Rules / Alpha 规则</h2><p>Each Author Raid has exactly 3 human authors with independently bound external Agents. Terminal review uses 1 evaluator, 2 reviewers, and 1 reproducer who are independent from those authors—at least 7 identities across the full flow. Login keys never leave this page except in the login request.</p></article></section>
        <section class="panel"><h2>External Agent key continuity / 外部 Agent 密钥连续性</h2><p class="source-state">Hepta: {}</p><p>Rotation requires independent signatures from both the currently bound key and the replacement key. Only public proof fields enter this browser.</p><div class="action-grid">{}</div></section>"#,
        escape(&identity.display_name),
        mission_board,
        continue_raid,
        agent_pairing,
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

fn agent_bridge_pairing_panel() -> String {
    r#"<section class="panel bridge-onboarding-primary agent-pairing-panel"><span class="pill">AGENT BRIDGE V2</span><h2>Pair an external Agent / 配对外部 Agent</h2><p>Generate one 5-minute pairing code in this authenticated browser, then enter it only at the local Bridge prompt. The BFF stores only its SHA-256 hash. The Bridge never receives your login key, browser cookie, CSRF token, or bearer credential, and never writes the code to a URL, deep link, argv, environment, config, state, or disk.</p><form class="agent-pairing-grant-form"><button type="submit">Generate one-time pairing code / 生成一次性配对码</button><output></output></form><div class="agent-pairing-code" hidden><p><strong>Shown once / 仅显示一次</strong> — copy it now; refreshing cannot recover it.</p><code class="agent-pairing-code-value"></code><button type="button" class="agent-pairing-copy">Copy code / 复制配对码</button></div><ol><li>Open the installed Paper Raid Agent Bridge on the Agent host / 在 Agent 主机打开已安装的 Paper Raid Agent Bridge。</li><li>Choose <strong>Pair</strong> and type or paste the one-time code into its local prompt / 选择“配对”，在本地提示框输入或粘贴一次性码。</li><li>Keep this page open. It checks authenticated pairing status and returns only after the BFF observes a post-pairing signed, self-declared healthy report from that Bridge / 保持本页打开；仅在 BFF 观察到该 Bridge 配对后签名提交的自声明健康报告后自动返回。</li></ol><p>The installed Bridge performs pairing-context discovery, AgentBinding V3 proof, authoritative recovery after a lost response, and persists only the resulting public binding state. The player path never asks you to copy a terminal command, JSON, UUID, or digest.</p><div class="agent-pairing-status"><button type="button" class="agent-pairing-refresh">Check pairing now / 立即检查配对状态</button><button type="button" class="agent-pairing-revoke" hidden>Revoke unused code / 撤销未使用配对码</button><output></output></div></section>"#.to_string()
}

const NON_ECONOMIC_PROGRESSION_NOTICE: &str = "Non-economic only; no ranking, reward, score, or economic eligibility / 仅非经济进度；不解锁排行、奖励、积分或经济资格";

#[derive(Debug, PartialEq, Eq)]
struct AuthoritativeGameplayView {
    difficulty: String,
    objective: String,
    risk: String,
    modifiers: Vec<String>,
    victory_summary: String,
}

fn challenge_gameplay_summary(challenge: &Value) -> String {
    let description = scalar(challenge.get("description"));
    let Some(ruleset) = challenge.get("ruleset").filter(|value| value.is_object()) else {
        return legacy_challenge_gameplay_summary(&description);
    };
    match authoritative_gameplay_view(ruleset) {
        Ok(Some(gameplay)) => {
            let template = ruleset
                .get("template")
                .and_then(Value::as_str)
                .unwrap_or("unavailable");
            let version = challenge
                .get("ruleset_version")
                .and_then(Value::as_str)
                .unwrap_or("unavailable");
            let duration = ruleset
                .get("duration_seconds")
                .and_then(Value::as_u64)
                .map(game_duration_label)
                .unwrap_or_else(|| "Unavailable / 不可用".into());
            format!(
                r#"<dl class="challenge-rules" data-gameplay-source="authoritative_typed"><div><dt>Template / 模式</dt><dd>{}</dd></div><div><dt>Ruleset / 规则版本</dt><dd>{}</dd></div><div><dt>Difficulty / 难度</dt><dd>{}</dd></div><div><dt>Duration / 时长</dt><dd>{}</dd></div>{}</dl>"#,
                escape(template),
                escape(version),
                escape(challenge_typed_difficulty_label(&gameplay.difficulty)),
                escape(&duration),
                authoritative_gameplay_details(&gameplay),
            )
        }
        Ok(None) => legacy_challenge_gameplay_summary(&description),
        Err(()) => format!(
            r#"<p class="status missing" data-gameplay-source="invalid_authoritative">Authoritative gameplay metadata is invalid or unavailable; description fallback is disabled. {} </p>"#,
            escape(NON_ECONOMIC_PROGRESSION_NOTICE),
        ),
    }
}

fn legacy_challenge_gameplay_summary(description: &str) -> String {
    let mut template = None;
    let mut template_version = None;
    let mut difficulty = None;
    let mut duration = None;
    let mut objective = None;
    let mut victory = None;
    let mut risk = None;
    let mut modifiers = None;
    for field in description.split(';').map(str::trim) {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "template" => template = Some(value),
            "template_version" => template_version = Some(value),
            "difficulty" => difficulty = Some(value),
            "duration_preset" => duration = Some(value),
            "objective" => objective = Some(value),
            "victory" => victory = Some(value),
            "risk" => risk = Some(value),
            "modifiers" => modifiers = Some(value),
            _ => {}
        }
    }
    let (Some(objective), Some(victory)) = (objective, victory) else {
        return format!("<p>{}</p>", escape(description));
    };
    let metadata = [
        ("Template / 模式", template.unwrap_or("custom")),
        (
            "Template version / 模式版本",
            template_version.unwrap_or("legacy"),
        ),
        ("Difficulty / 难度", difficulty.unwrap_or("unrated")),
        ("Duration / 时长", duration.unwrap_or("asynchronous")),
    ]
    .iter()
    .map(|(label, value)| {
        format!(
            "<div><dt>{}</dt><dd>{}</dd></div>",
            escape(label),
            escape(value)
        )
    })
    .collect::<String>();
    format!(
        r#"<dl class="challenge-rules" data-gameplay-source="legacy_description">{}<div><dt>Objective / 目标</dt><dd>{}</dd></div><div><dt>Victory / 胜利条件</dt><dd>{}</dd></div><div><dt>Risk / 主要风险</dt><dd>{}</dd></div><div><dt>Modifiers / 规则修饰</dt><dd>{}</dd></div><div><dt>Progression / 成长</dt><dd>{}</dd></div></dl>"#,
        metadata,
        escape(objective),
        escape(victory),
        escape(risk.unwrap_or("challenge-specific integrity gates")),
        escape(modifiers.unwrap_or("standard-author-raid")),
        escape(NON_ECONOMIC_PROGRESSION_NOTICE),
    )
}

fn authoritative_gameplay_view(ruleset: &Value) -> Result<Option<AuthoritativeGameplayView>, ()> {
    let Some(raw) = ruleset.get("gameplay") else {
        return Ok(None);
    };
    let object = raw.as_object().ok_or(())?;
    let expected = [
        "difficulty",
        "objective",
        "risk",
        "modifiers",
        "victory_summary",
    ];
    if object.len() != expected.len() || expected.iter().any(|field| !object.contains_key(*field)) {
        return Err(());
    }
    let difficulty = object
        .get("difficulty")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "introductory" | "intermediate" | "advanced"))
        .ok_or(())?;
    let objective = authoritative_gameplay_text(object.get("objective")).ok_or(())?;
    let risk = authoritative_gameplay_text(object.get("risk")).ok_or(())?;
    let victory_summary = authoritative_gameplay_text(object.get("victory_summary")).ok_or(())?;
    let modifiers = object
        .get("modifiers")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= 8)
        .ok_or(())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| authoritative_gameplay_modifier(value))
                .map(str::to_string)
                .ok_or(())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if modifiers
        .windows(2)
        .any(|pair| pair[0].as_str() >= pair[1].as_str())
    {
        return Err(());
    }
    Ok(Some(AuthoritativeGameplayView {
        difficulty: difficulty.to_string(),
        objective: objective.to_string(),
        risk: risk.to_string(),
        modifiers,
        victory_summary: victory_summary.to_string(),
    }))
}

fn authoritative_gameplay_text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|value| {
        !value.is_empty()
            && value.chars().count() <= 512
            && value.trim() == *value
            && !value.chars().any(char::is_control)
    })
}

fn authoritative_gameplay_modifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 48
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !bytes.windows(2).any(|pair| pair == b"--")
}

fn authoritative_gameplay_details(gameplay: &AuthoritativeGameplayView) -> String {
    let modifiers = if gameplay.modifiers.is_empty() {
        "none / 无".into()
    } else {
        gameplay.modifiers.join(", ")
    };
    format!(
        r#"<div><dt>Objective / 目标</dt><dd>{}</dd></div><div><dt>Victory summary / 胜利摘要</dt><dd>{}</dd></div><div><dt>Risk / 主要风险</dt><dd>{}</dd></div><div><dt>Modifiers / 规则修饰</dt><dd>{}</dd></div><div><dt>Progression / 成长</dt><dd>{}</dd></div>"#,
        escape(&gameplay.objective),
        escape(&gameplay.victory_summary),
        escape(&gameplay.risk),
        escape(&modifiers),
        escape(NON_ECONOMIC_PROGRESSION_NOTICE),
    )
}

fn paper_phase_label(phase: &str) -> &'static str {
    match phase {
        "forming" => "Form team / 组队成形",
        "preregistering" => "Preregister / 预注册",
        "researching" => "Research / 研究",
        "experimenting" => "Experiment / 实验",
        "drafting" => "Draft / 起草",
        "integrity_review" => "Integrity review / 完整性审查",
        "reproducing" | "reproduction_readiness" => "Reproduction readiness / 复现准备",
        "author_approval" => "Author approval / 作者批准",
        "integrity_hold" => "Integrity hold / 完整性挂起",
        "submission_ready" => "Author Raid complete / 作者远征完成",
        _ => "Waiting for authority / 等待权威状态",
    }
}

fn author_role_choices(identity: &AlphaIdentity) -> (String, String) {
    let mut values = Vec::new();
    let mut controls = String::new();
    for (index, role) in identity.author_roles.iter().enumerate() {
        let (value, title, detail) = match role {
            AlphaAuthorRole::Captain => (
                "captain",
                "Captain / 队长",
                "coordinates tasks and phase gates / 协调任务与阶段门",
            ),
            AlphaAuthorRole::Evidence => (
                "evidence",
                "Evidence / 证据",
                "protects claims and source quality / 保护论断与来源质量",
            ),
            AlphaAuthorRole::Experiment => (
                "experiment",
                "Experiment / 实验",
                "owns preregistration and reproducibility / 负责预注册与可复现性",
            ),
        };
        values.push(value);
        controls.push_str(&format!(
            r#"<label class="role-choice" data-preference-rank="{}"><input type="checkbox" name="roles" value="{}" checked><span><strong>{}</strong><small>{}{}</small></span></label>"#,
            index + 1,
            value,
            title,
            if index == 0 { "Preferred · 首选 · " } else { "" },
            detail,
        ));
    }
    if controls.is_empty() {
        controls.push_str(
            "<p class=\"status missing\">This identity is not authorized for an Author Raid / 此身份无作者远征权限</p>",
        );
    }
    (controls, values.join(","))
}

fn first_raid_mission_board(
    tickets: ReadState<'_>,
    proposals: ReadState<'_>,
    raid_state: ReadState<'_>,
) -> String {
    let ticket_count = tickets
        .value()
        .and_then(Value::as_array)
        .map_or(0, |records| {
            records
                .iter()
                .filter(|record| {
                    matches!(
                        record.get("status").and_then(Value::as_str),
                        Some("queued" | "matched")
                    )
                })
                .count()
        });
    let proposal_count = proposals
        .value()
        .and_then(Value::as_array)
        .map_or(0, |records| {
            records
                .iter()
                .filter(|record| {
                    matches!(
                        record.get("status").and_then(Value::as_str),
                        Some("proposed" | "accepted")
                    )
                })
                .count()
        });
    let current_raid = raid_state
        .value()
        .and_then(|value| value.get("current_raid"))
        .filter(|value| !value.is_null());
    let paper_phase = current_raid
        .and_then(|raid| raid.get("paper"))
        .filter(|value| !value.is_null())
        .and_then(|paper| paper.get("player_phase").or_else(|| paper.get("phase")))
        .and_then(Value::as_str);
    let states = if paper_phase == Some("submission_ready") {
        ["complete", "complete", "complete", "complete", "complete"]
    } else if matches!(
        paper_phase,
        Some(
            "integrity_review"
                | "reproducing"
                | "reproduction_readiness"
                | "author_approval"
                | "integrity_hold"
        )
    ) {
        ["complete", "complete", "complete", "complete", "current"]
    } else if paper_phase.is_some() {
        ["complete", "complete", "complete", "current", "upcoming"]
    } else if current_raid.is_some() || proposal_count > 0 {
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

fn active_raid_card(raid_state: ReadState<'_>) -> String {
    let Some(raid) = raid_state
        .value()
        .and_then(|value| value.get("current_raid"))
        .filter(|value| !value.is_null())
    else {
        return String::new();
    };
    let team_id = scalar(raid.get("team_id"));
    let team_status = scalar(raid.get("team_status"));
    let role = scalar(raid.get("role"));
    let (href, label, detail, paper_id) = raid
        .get("paper")
        .filter(|value| !value.is_null())
        .map(|paper| {
            let paper_id = scalar(paper.get("paper_project_id"));
            let phase = scalar(paper.get("player_phase").or_else(|| paper.get("phase")));
            (
                format!("/league/papers/{}", escape(&paper_id)),
                "Continue Paper Raid / 继续上局",
                format!("Paper phase: {}", escape(&phase)),
                paper_id,
            )
        })
        .unwrap_or_else(|| {
            (
                format!("/league/formation/{}", escape(&team_id)),
                "Continue Team Formation / 继续组队",
                format!("Team status: {}", escape(&team_status)),
                String::new(),
            )
        });
    format!(
        r#"<section class="panel continue-raid"><span class="eyebrow">MY ACTIVE RAID / 我的进行中远征</span><h2>{}</h2><p>{} · Role: {}</p><a class="button primary continue-raid-link" data-team-id="{}" data-paper-id="{}" href="{}">{}</a></section>"#,
        escape(&team_id),
        detail,
        escape(&role),
        escape(&team_id),
        escape(&paper_id),
        href,
        label,
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
    raid_state: ReadState<'_>,
    tickets: ReadState<'_>,
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
    let proposal_deadline = proposal
        .value()
        .and_then(|value| value.get("expires_at"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|expires_at| {
            format!(
                r#"<div class="proposal-deadline"><strong>Decision window / 决策窗口</strong><p><span class="proposal-countdown" data-proposal-expires-at="{}">Calculating from Hepta deadline… / 正在读取 Hepta 截止时间…</span></p><time datetime="{}">{}</time></div>"#,
                escape(expires_at),
                escape(expires_at),
                escape(expires_at),
            )
        })
        .unwrap_or_else(|| unavailable("authoritative proposal deadline"));
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
    let proposal_contract = proposal.value().and_then(|value| {
        let solver_version = value.get("solver_version")?.as_str()?;
        let preferences = value.get("source_preferences")?.as_array()?;
        let assignments = value.get("role_assignments")?.as_array()?;
        let members = value.get("member_player_ids")?.as_array()?;
        if solver_version.is_empty()
            || preferences.len() != members.len()
            || assignments.len() != members.len()
            || members.is_empty()
        {
            return None;
        }
        let preference_items = preferences
            .iter()
            .map(|preference| {
                let player = scalar(preference.get("player_id"));
                let roles = preference
                    .get("roles")
                    .and_then(Value::as_array)
                    .map(|roles| {
                        roles
                            .iter()
                            .map(|role| scalar(Some(role)))
                            .collect::<Vec<_>>()
                            .join(" → ")
                    })
                    .unwrap_or_else(|| "unavailable".into());
                let availability = scalar(preference.get("availability_hash"));
                let party = if preference.get("private_party").and_then(Value::as_bool)
                    == Some(true)
                {
                    "private party"
                } else {
                    "public queue"
                };
                format!(
                    "<li><code>{}</code><span>ordered preference: {}</span><span>{} · {}</span></li>",
                    escape(&player),
                    escape(&roles),
                    escape(&availability),
                    escape(party),
                )
            })
            .collect::<String>();
        let assignment_items = assignments
            .iter()
            .map(|assignment| {
                format!(
                    "<li><code>{}</code><span>assigned role: <strong>{}</strong></span></li>",
                    escape(&scalar(assignment.get("player_id"))),
                    escape(&scalar(assignment.get("assigned_role"))),
                )
            })
            .collect::<String>();
        Some(format!(
            r#"<div class="proposal-contract"><h3>Frozen match contract / 冻结匹配契约</h3><p>Solver <code>{}</code>. Role preference order and final assignments are part of the proposal identity.</p><h4>Ordered preferences / 有序偏好</h4><ul class="roster">{}</ul><h4>Assigned roles / 分配角色</h4><ul class="roster">{}</ul></div>"#,
            escape(solver_version),
            preference_items,
            assignment_items,
        ))
    });
    let proposal_contract_complete = proposal_contract.is_some();
    let proposal_contract =
        proposal_contract.unwrap_or_else(|| unavailable("frozen matcher V2 contract"));
    let proposal_controls = match (
        proposal.value(),
        team.value(),
        proposal_status.as_str(),
        proposal_contract_complete,
    ) {
        (Some(_), None, "accepted", true) => format!(
            r#"<form class="materialize-team-form" data-proposal-id="{}" data-proposal-version="{}"><h3>Build the team / 建立队伍</h3><p>All three players accepted. Hepta will derive the exact roster and role contract.</p><button type="submit">Build our Research Cell / 建立正式队伍</button><output></output></form>"#,
            escape(resource_id),
            proposal_version,
        ),
        (Some(_), _, "proposed", true) => format!(
            r#"<form class="proposal-decision" data-proposal-id="{}" data-proposal-version="{}"><button name="decision" value="accept" type="submit">Accept Proposal / 接受组队</button><button class="danger" name="decision" value="decline" type="submit">Decline / 拒绝</button><output></output></form>"#,
            escape(resource_id),
            proposal_version,
        ),
        (Some(_), _, "proposed" | "accepted", false) => {
            unavailable("safe proposal actions: frozen matcher contract is incomplete")
        }
        (Some(_), _, _, _) => String::new(),
        (None, _, _, _) => unavailable("team proposal"),
    };
    let matched_ticket_control = tickets
        .value()
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|ticket| {
                ticket.get("status").and_then(Value::as_str) == Some("matched")
                    && ticket.get("matched_proposal_id").and_then(Value::as_str)
                        == Some(resource_id)
                    && ticket
                        .get("version")
                        .and_then(Value::as_u64)
                        .is_some_and(|value| value > 0)
            })
        })
        .and_then(|ticket| {
            Some((
                ticket.get("ticket_id")?.as_str()?,
                ticket.get("version")?.as_u64()?,
            ))
        })
        .map(|(ticket_id, version)| {
            format!(
                r#"<form class="cancel-ticket-form formation-withdraw-form" data-ticket-id="{}" data-ticket-version="{}" data-success-message="Formation withdrawn. Returning to the Lobby…" data-success-href="/league"><h3>Cannot join this team? / 无法参加本队？</h3><p>Withdraw your matched ticket through Hepta. The expired or declined formation will not leave absent-player ghosts.</p><button class="danger" type="submit">Withdraw from formation / 退出组队</button><output></output></form>"#,
                escape(ticket_id),
                version,
            )
        })
        .unwrap_or_default();
    let acceptance_items = acceptances
        .value()
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let current_player_id = identity.player_id.to_string();
    let player_ready = acceptance_items.iter().any(|acceptance| {
        acceptance.get("player_id").and_then(Value::as_str) == Some(current_player_id.as_str())
            && acceptance.get("superseded_at").is_none_or(Value::is_null)
    });
    let team_controls = if let Some(team_value) = team.value() {
        let team_status = team_value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let team_version = team_value
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let roster_version = team_value
            .get("roster_version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let compact_hash = scalar(team_value.get("collaboration_compact_hash"));
        let current_member = team_value
            .get("members")
            .and_then(Value::as_array)
            .and_then(|members| {
                members.iter().find(|member| {
                    member.get("player_id").and_then(Value::as_str)
                        == Some(current_player_id.as_str())
                })
            });
        let paper = raid_state
            .value()
            .and_then(|state| state.get("raids"))
            .and_then(Value::as_array)
            .and_then(|raids| {
                raids
                    .iter()
                    .find(|raid| raid.get("team_id").and_then(Value::as_str) == Some(resource_id))
            })
            .and_then(|raid| raid.get("paper"))
            .filter(|paper| !paper.is_null());
        let mut controls = String::new();
        if team_status == "forming" && !player_ready {
            if let Some(member) = current_member {
                controls.push_str(&format!(
                    r#"<form class="team-ready-form" data-team-id="{}" data-team-version="{}" data-roster-version="{}" data-participant-slot="{}" data-binding-id="{}" data-role="{}" data-compact-hash="{}"><h3>Ready check / 就绪确认</h3><p>Sign your exact roster slot with the local non-exportable browser key.</p><button type="submit">I am ready / 我已准备</button><output></output></form>"#,
                    escape(resource_id),
                    team_version,
                    roster_version,
                    escape(&scalar(member.get("participant_slot"))),
                    escape(&scalar(member.get("binding_id"))),
                    escape(&scalar(member.get("role"))),
                    escape(&compact_hash),
                ));
            }
        }
        let member_count = team_value
            .get("members")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if team_status == "forming" && member_count > 0 && acceptance_items.len() == member_count {
            controls.push_str(&format!(
                r#"<form class="lock-team-form" data-team-id="{}" data-team-version="{}"><h3>Lock roster / 锁定阵容</h3><p>Everyone is ready. Freeze the three-person roster and start the Raid.</p><button type="submit">Lock Team / 锁定队伍</button><output></output></form>"#,
                escape(resource_id), team_version,
            ));
        }
        if let Some(paper) = paper {
            let paper_id = scalar(paper.get("paper_project_id"));
            controls.push_str(&format!(
                r#"<article class="guided-action"><h3>Research is live / 研究已开始</h3><p>Continue from the authoritative Paper phase.</p><a class="button primary continue-raid-link" data-team-id="{}" data-paper-id="{}" href="/league/papers/{}">Continue Paper Raid / 继续远征</a></article>"#,
                escape(resource_id), escape(&paper_id), escape(&paper_id),
            ));
        } else if team_status == "locked" {
            controls.push_str(&format!(
                r#"<form class="create-paper-form" data-team-id="{}"><h3>Name the expedition / 命名本局</h3><label>Paper title / 论文标题<input name="title" minlength="3" maxlength="160" required value="First Paper Raid"></label><label>Target format / 目标格式<select name="target_format"><option value="workshop-short-paper">Workshop short paper</option><option value="replication-report">Replication report</option></select></label><button type="submit">Enter Research Room / 进入研究室</button><output></output></form>"#,
                escape(resource_id),
            ));
        }
        controls
    } else {
        unavailable("formal Team controls")
    };
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">TEAM FORMATION · 组队</span><h1>Research Cell</h1><p>Resource <code>{}</code></p></section>
        <section class="grid"><article class="card"><h2>Match Proposal / 匹配提案</h2><span class="pill">{}</span>{}{}{}{}</article><article class="card"><h2>Players + Agents / 玩家与 Agent</h2><p class="source-state">Hepta Team: {}</p>{}</article>{}</section>
        <section class="panel"><h2>Formation Actions / 组队操作</h2><div class="action-grid">{}{}{}</div></section>"#,
        escape(resource_id),
        escape(&proposal_status),
        proposal_deadline,
        proposal_members,
        proposal_contract,
        proposal_controls,
        escape(team.label()),
        roster,
        fact("Ready Check / 就绪确认", &readiness),
        compact,
        team_controls,
        matched_ticket_control,
    );
    page("Paper Raid Formation", &identity.display_name, &body, true)
}

#[cfg(test)]
pub(crate) fn paper_room(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: ReadState<'_>,
    events: ReadState<'_>,
    review: ReadState<'_>,
) -> Response {
    let finality = finality_from_review_state(review);
    render_paper_room_with_finality(identity, paper_id, room, events, review, &finality, None)
}

pub(crate) fn paper_room_with_finality(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: ReadState<'_, AuthenticatedPaperRoom>,
    events: ReadState<'_>,
    review: ReadState<'_, AuthenticatedPaperReviewState>,
    finality: &Value,
) -> Response {
    let raw_room = match room {
        ReadState::Available(value) => ReadState::Available(value.value()),
        ReadState::NotFound => ReadState::NotFound,
        ReadState::Unavailable => ReadState::Unavailable,
    };
    let raw_review = match review {
        ReadState::Available(value) => ReadState::Available(value.value()),
        ReadState::NotFound => ReadState::NotFound,
        ReadState::Unavailable => ReadState::Unavailable,
    };
    render_paper_room_with_finality(
        identity,
        paper_id,
        raw_room,
        events,
        raw_review,
        finality,
        Some((room, review)),
    )
}

fn render_paper_room_with_finality(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: ReadState<'_>,
    events: ReadState<'_>,
    review: ReadState<'_>,
    finality: &Value,
    authenticated_aar: Option<(
        ReadState<'_, AuthenticatedPaperRoom>,
        ReadState<'_, AuthenticatedPaperReviewState>,
    )>,
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
    let (settlement, settlement_fact) = finality_projection(finality);
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
    let basic_actions = room
        .value()
        .map(|value| basic_paper_room(value, review.value()))
        .unwrap_or_else(|| unavailable("current Paper objective"));
    let guided_actions = room
        .value()
        .map(|value| guided_paper_room(identity, paper_id, value, review.value()))
        .unwrap_or_else(|| unavailable("guided Paper Raid actions"));
    let developer_actions = if room.value().is_some() {
        let mut actions = format!(
        "{}{}{}{}{}{}{}{}{}",
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
            "Finalization freezes the author bundle; the independent finality projection reports verification and every eligibility gate.",
            "finalize_joint_paper_submission",
            Some(paper_id),
            false,
            r#"{"submission_id":"...","expected_paper_version":1,"revision_id":"...","release_candidate_hash":"sha256:..."}"#,
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
        unavailable("developer protocol actions")
    };
    let advanced_body = format!(
        r#"<section class="paper-room-authority" tabindex="-1" aria-labelledby="paper-room-authority-heading"><span class="eyebrow">AUTHORITY DETAILS / <span lang="zh-Hans">权威详情</span></span><h2 id="paper-room-authority-heading">Identifiers, hashes, and finality / <span lang="zh-Hans">标识、摘要与终局</span></h2><dl class="paper-room-authority-facts"><div><dt>Paper ID</dt><dd><code>{}</code></dd></div><div><dt>Hepta source / <span lang="zh-Hans">Hepta 来源</span></dt><dd>{}</dd></div><div><dt>Author phase / <span lang="zh-Hans">作者阶段</span></dt><dd><code>{}</code></dd></div><div><dt>Finality / <span lang="zh-Hans">终局</span></dt><dd>{}</dd></div></dl>{}</section>{}
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
        escape(paper_id),
        escape(room.label()),
        escape(&phase),
        escape(&settlement_fact),
        settlement,
        guided_actions,
        fact("Phase Gates / 阶段门", paper_phase_label(&phase)),
        fact("Author Sign-off / 作者签署", &signoff),
        timeline_card(paper_id, &timeline_text),
        fact("Settlement / 结算", &settlement_fact),
        room_card("Sections & Revisions / 章节与版本", "section_revisions"),
        room_card("Evidence / 证据", "evidence_cards"),
        room_card("Claims / 论断", "claims"),
        room_card("Citations / 引用", "citations"),
        room_card("Runs (including failed) / 全部实验", "runs"),
        room_card("Figures / 图表", "figures"),
        room_card("Review / 内审", "section_reviews"),
        review_card("Review State / 评审状态", "evaluations"),
    );
    let advanced_body = format!(
        "{advanced_body}<section class=\"grid\">{}{}{}{}{}{}</section>",
        review_card("Contribution / 贡献", "contribution_ledgers"),
        review_card("Evaluations / 评估", "evaluations"),
        review_card("Reproductions / 复现", "reproductions"),
        review_card("Appeals / 申诉", "appeals"),
        review_card("Resolutions / 裁决", "resolutions"),
        provisional_contribution_card(review),
    );
    let after_action = authenticated_aar
        .map(|(room, review)| authenticated_after_action_report(identity, paper_id, room, review))
        .unwrap_or_default();
    let advanced_body = format!("{advanced_body}{after_action}");
    let artifacts = artifact_links(room, paper_id);
    let body = format!(
        r#"<section class="hero paper-room-hero"><span class="eyebrow">PAPER ROOM · <span lang="zh-Hans">论文作战室</span></span><h1>{}</h1><p>Follow the current objective; technical authority remains available below when you need to audit it. / <span lang="zh-Hans">先完成当前目标；需要审计时再查看下方技术权威。</span></p></section>{}<details class="panel paper-room-advanced" id="paper-room-advanced"><summary class="paper-room-advanced-summary"><span>Advanced / <span lang="zh-Hans">高级详情</span></span><small>UUIDs, hashes, authority, all controls, and complete records / <span lang="zh-Hans">UUID、摘要、权威、全部操作与完整记录</span></small></summary><div class="paper-room-advanced-content">{}{artifacts}<details class="panel developer-tools"><summary>Developer Tools / <span lang="zh-Hans">开发者工具</span></summary><p class="muted">Exact protocol JSON remains an Alpha fallback for commands that do not yet have a safely derived form or external connector. Needing this section is a known playability gap, never a completed normal path.</p><div class="action-grid">{developer_actions}</div></details></div></details>"#,
        escape(&title),
        basic_actions,
        advanced_body,
    );
    page("Paper Raid Room", &identity.display_name, &body, true)
}

fn provisional_contribution_card(review: ReadState<'_>) -> String {
    let records = review
        .value()
        .and_then(|value| value.get("raid_scores"))
        .and_then(Value::as_array)
        .map(Vec::len);
    let count = records
        .map(|value| format!("{value} provisional record(s) / {value} 条暂定记录"))
        .unwrap_or_else(|| review.label().into());
    format!(
        r#"<article class="card provisional-contribution"><h2>Provisional contribution telemetry / 暂定贡献遥测</h2><p>{}</p><p class="muted">Not leaderboard score, not reward, and not economic value. All eligibility remains locked until verified finality and a separate anti-abuse gate / 不计榜、不发奖、不具经济价值；须经验证终局与独立反滥用门禁后方可解锁。</p></article>"#,
        escape(&count),
    )
}

fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_contract_text(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 512 && !value.as_bytes().contains(&0)
}

fn canonical_base64(value: &str, expected_len: usize) -> Option<Vec<u8>> {
    let decoded = BASE64.decode(value).ok()?;
    (decoded.len() == expected_len && BASE64.encode(&decoded) == value).then_some(decoded)
}

fn valid_public_key_snapshot(public_key: &str, public_key_hash: &str) -> bool {
    canonical_base64(public_key, 32)
        .is_some_and(|decoded| sha256_digest(&decoded) == public_key_hash)
}

fn canonical_uuid_text(value: &str) -> Option<Uuid> {
    let parsed = Uuid::parse_str(value).ok()?;
    (!parsed.is_nil() && value == parsed.to_string()).then_some(parsed)
}

fn canonical_uuid_value(value: &Value) -> Option<Uuid> {
    canonical_uuid_text(value.as_str()?)
}

fn canonical_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .ok()?
        .with_timezone(&Utc);
    (parsed.to_rfc3339_opts(SecondsFormat::AutoSi, true) == value).then_some(parsed)
}

fn canonical_uuid_array(value: Option<&Value>) -> Option<Vec<Uuid>> {
    let values = value?.as_array()?;
    let mut normalized = Vec::with_capacity(values.len());
    let mut previous = None;
    for value in values {
        let parsed = canonical_uuid_value(value)?;
        if previous.is_some_and(|previous| previous >= parsed) {
            return None;
        }
        previous = Some(parsed);
        normalized.push(parsed);
    }
    Some(normalized)
}

fn validated_victory_summary(paper: &Value) -> Option<&str> {
    let snapshot = paper.get("challenge_ruleset_snapshot")?.as_object()?;
    if snapshot.len() != 6
        || snapshot.get("schema")?.as_str()? != "hepta.paper_raid.challenge_ruleset_snapshot.v1"
        || snapshot.get("enforcement")?.as_str()? != "authoritative_v1"
        || !valid_sha256_digest(snapshot.get("challenge_snapshot_hash")?.as_str()?)
        || !valid_contract_text(snapshot.get("ruleset_version")?.as_str()?)
    {
        return None;
    }
    let ruleset = snapshot.get("ruleset")?.as_object()?;
    if ruleset.get("schema")?.as_str()? != "hepta.challenge.ruleset.v1" {
        return None;
    }
    let ruleset_hash = snapshot
        .get("ruleset_hash")?
        .as_str()
        .filter(|value| valid_sha256_digest(value))?;
    if canonical_json_sha256(ruleset).ok()?.as_str() != ruleset_hash
        || canonical_json_sha256(snapshot).ok()?.as_str()
            != paper.get("challenge_ruleset_snapshot_hash")?.as_str()?
    {
        return None;
    }
    ruleset
        .get("gameplay")?
        .get("victory_summary")?
        .as_str()
        .filter(|value| valid_contract_text(value))
}

fn optional_non_nil_uuid(value: &Value) -> Option<Option<Uuid>> {
    match value {
        Value::Null => Some(None),
        Value::String(value) => {
            let value = canonical_uuid_text(value)?;
            Some(Some(value))
        }
        _ => None,
    }
}

fn optional_sha256_digest(value: &Value) -> Option<Option<&str>> {
    match value {
        Value::Null => Some(None),
        Value::String(value) if valid_sha256_digest(value) => Some(Some(value)),
        _ => None,
    }
}

fn validated_run_status<'a>(run: &'a Value, paper_id: &str) -> Option<&'a str> {
    if run.as_object()?.len() != 12
        || run.get("paper_project_id")?.as_str()? != paper_id
        || canonical_uuid_value(run.get("run_record_id")?).is_none()
        || canonical_uuid_value(run.get("experiment_plan_id")?).is_none()
        || run.get("seed")?.as_i64().is_none()
        || !valid_sha256_digest(run.get("parameters_hash")?.as_str()?)
        || canonical_uuid_value(run.get("logs_manifest_id")?).is_none()
        || run.get("version")?.as_u64()? != 1
        || canonical_timestamp(run.get("created_at")?.as_str()?).is_none()
    {
        return None;
    }
    let outputs_manifest_id = optional_non_nil_uuid(run.get("outputs_manifest_id")?)?;
    let metrics_hash = optional_sha256_digest(run.get("metrics_hash")?)?;
    let failure_hash = optional_sha256_digest(run.get("failure_hash")?)?;
    let status = run.get("status")?.as_str()?;
    match status {
        "succeeded"
            if outputs_manifest_id.is_some()
                && metrics_hash.is_some()
                && failure_hash.is_none() =>
        {
            Some(status)
        }
        "failed" | "cancelled" if failure_hash.is_some() => Some(status),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum ValidatedAarOutcome {
    SubmissionReady,
    Failed(HeptaPaperTerminalReason),
    Expired(HeptaPaperTerminalReason),
    Abandoned(HeptaPaperTerminalReason),
}

impl ValidatedAarOutcome {
    fn code(self) -> &'static str {
        match self {
            Self::SubmissionReady => "submission_ready",
            Self::Failed(_) => "failed",
            Self::Expired(_) => "expired",
            Self::Abandoned(_) => "abandoned",
        }
    }

    fn reason_label(self) -> &'static str {
        match self {
            Self::SubmissionReady => challenge_outcome_label("submission_ready"),
            Self::Failed(reason) | Self::Expired(reason) | Self::Abandoned(reason) => {
                challenge_reason_label(reason.as_str())
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ValidatedAarTerminal {
    outcome: ValidatedAarOutcome,
    terminal_at: DateTime<Utc>,
}

enum AarTerminalState {
    Active,
    Terminal(ValidatedAarTerminal),
    Invalid,
}

struct ValidatedAar {
    terminal: ValidatedAarTerminal,
    victory: String,
    run_summary: ValidatedRunDebrief,
    role_summary: ValidatedRoleDebrief,
    contribution: ValidatedContributionDebrief,
    finality_status: String,
}

impl ValidatedAar {
    fn build(
        identity: &AlphaIdentity,
        paper_id: &str,
        room: &Value,
        review: &Value,
        terminal: ValidatedAarTerminal,
    ) -> Option<Self> {
        let paper = room.get("paper")?.as_object()?;
        let victory = validated_victory_summary(&Value::Object(paper.clone()))?.to_string();
        let run_summary =
            validated_after_action_runs(room, &Value::Object(paper.clone()), paper_id)?;
        let role_summary =
            validated_after_action_roles(room, &Value::Object(paper.clone()), paper_id)?;
        let contribution = validated_after_action_contribution(
            identity,
            paper_id,
            room,
            review,
            terminal.terminal_at,
        )?;
        let finality_status = validated_aar_finality(
            review.get("finality")?,
            terminal.terminal_at,
            &contribution.xp,
        )?
        .to_string();
        Some(Self {
            terminal,
            victory,
            run_summary,
            role_summary,
            contribution,
            finality_status,
        })
    }

    fn render(&self) -> String {
        format!(
            r#"<section class="panel after-action-report" data-after-action-report="v1"><span class="eyebrow">AFTER ACTION REPORT · 赛后报告</span><h2>{}</h2><p>{}</p><p><strong>Victory contract / 胜利契约:</strong> {}</p><div class="grid">{}{}</div>{}<article class="card after-action-finality"><h3>Scientific finality / 科学终局</h3><p><code>{}</code></p><ul class="eligibility-grid"><li><span>Ranking / 排名</span><strong data-eligible="false">locked</strong></li><li><span>Reward / 奖励</span><strong data-eligible="false">locked</strong></li><li><span>Score / 分数资格</span><strong data-eligible="false">locked</strong></li><li><span>Economic / 经济</span><strong data-eligible="false">locked</strong></li></ul></article><article class="card after-action-progression"><h3>Progression status / 成长状态</h3><p>Role mastery, challenge unlocks, immutable replay and automatic rematch are not authoritative yet. This report does not invent them / 角色熟练度、挑战解锁、不可变回放及自动再匹配尚无权威事实，本报告不会伪造。</p><a class="button" href="/league">Return to Lobby and choose the next Raid / 返回大厅选择下一局</a></article></section>"#,
            escape(challenge_outcome_label(self.terminal.outcome.code())),
            escape(self.terminal.outcome.reason_label()),
            escape(&self.victory),
            self.run_summary.render(),
            self.role_summary.render(),
            self.contribution.render(),
            escape(&self.finality_status),
        )
    }
}

fn aar_unavailable() -> String {
    unavailable_card(
        "After Action Report unavailable / 赛后报告不可用",
        "one complete authenticated terminal Room + review authority",
    )
}

fn authenticated_after_action_report(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: ReadState<'_, AuthenticatedPaperRoom>,
    review: ReadState<'_, AuthenticatedPaperReviewState>,
) -> String {
    let Some(paper_uuid) = canonical_uuid_text(paper_id) else {
        return aar_unavailable();
    };
    let ReadState::Available(room) = room else {
        return aar_unavailable();
    };
    if room.paper_id() != paper_uuid {
        return aar_unavailable();
    }
    let room_value = room.value();
    let paper = match room_value.get("paper") {
        Some(paper) => paper,
        None => return aar_unavailable(),
    };
    let terminal = match validated_aar_terminal(paper, paper_id) {
        AarTerminalState::Active => return String::new(),
        AarTerminalState::Invalid => return aar_unavailable(),
        AarTerminalState::Terminal(terminal) => terminal,
    };
    let ReadState::Available(review) = review else {
        return aar_unavailable();
    };
    if review.paper_id() != paper_uuid {
        return aar_unavailable();
    }
    ValidatedAar::build(identity, paper_id, room_value, review.value(), terminal)
        .map(|report| report.render())
        .unwrap_or_else(aar_unavailable)
}

fn validated_aar_terminal(paper: &Value, paper_id: &str) -> AarTerminalState {
    let Some(object) = paper.as_object() else {
        return AarTerminalState::Invalid;
    };
    if object.get("paper_project_id").and_then(Value::as_str) != Some(paper_id) {
        return AarTerminalState::Invalid;
    }
    let Some(phase) = object.get("phase").and_then(Value::as_str) else {
        return AarTerminalState::Invalid;
    };
    let known_non_submission_phase = matches!(
        phase,
        "forming"
            | "preregistering"
            | "researching"
            | "experimenting"
            | "drafting"
            | "integrity_review"
            | "reproducing"
            | "author_approval"
            | "integrity_hold"
    );
    let Some(stored_outcome) = object.get("outcome").and_then(Value::as_str) else {
        return AarTerminalState::Invalid;
    };
    if stored_outcome == "in_progress" {
        return if known_non_submission_phase {
            AarTerminalState::Active
        } else {
            AarTerminalState::Invalid
        };
    }
    let Some(deadline_at) = object
        .get("deadline_at")
        .and_then(Value::as_str)
        .and_then(canonical_timestamp)
    else {
        return AarTerminalState::Invalid;
    };
    let Some(grace_expires_at) = object
        .get("grace_expires_at")
        .and_then(Value::as_str)
        .and_then(canonical_timestamp)
    else {
        return AarTerminalState::Invalid;
    };
    let Some(terminal_at) = object
        .get("terminal_at")
        .and_then(Value::as_str)
        .and_then(canonical_timestamp)
    else {
        return AarTerminalState::Invalid;
    };
    let Some(created_at) = object
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(canonical_timestamp)
    else {
        return AarTerminalState::Invalid;
    };
    let Some(updated_at) = object
        .get("updated_at")
        .and_then(Value::as_str)
        .and_then(canonical_timestamp)
    else {
        return AarTerminalState::Invalid;
    };
    if deadline_at >= grace_expires_at || created_at > terminal_at || terminal_at > updated_at {
        return AarTerminalState::Invalid;
    }
    let reason = object.get("outcome_reason");
    let outcome = match (phase, stored_outcome) {
        ("submission_ready", "submission_ready")
            if reason.is_none() && terminal_at < grace_expires_at =>
        {
            ValidatedAarOutcome::SubmissionReady
        }
        (_, "failed") if known_non_submission_phase && terminal_at < grace_expires_at => {
            let Some(reason) = reason.and_then(Value::as_str).and_then(|value| {
                HeptaPaperTerminalReason::parse_for_outcome(
                    HeptaPaperTerminalOutcome::Failed,
                    value,
                )
            }) else {
                return AarTerminalState::Invalid;
            };
            ValidatedAarOutcome::Failed(reason)
        }
        (_, "abandoned") if known_non_submission_phase && terminal_at < grace_expires_at => {
            let Some(reason) = reason.and_then(Value::as_str).and_then(|value| {
                HeptaPaperTerminalReason::parse_for_outcome(
                    HeptaPaperTerminalOutcome::Abandoned,
                    value,
                )
            }) else {
                return AarTerminalState::Invalid;
            };
            ValidatedAarOutcome::Abandoned(reason)
        }
        (_, "expired") if known_non_submission_phase && terminal_at == grace_expires_at => {
            let Some(reason) = reason.and_then(Value::as_str).and_then(|value| {
                HeptaPaperTerminalReason::parse_for_outcome(
                    HeptaPaperTerminalOutcome::Expired,
                    value,
                )
            }) else {
                return AarTerminalState::Invalid;
            };
            ValidatedAarOutcome::Expired(reason)
        }
        _ => return AarTerminalState::Invalid,
    };
    AarTerminalState::Terminal(ValidatedAarTerminal {
        outcome,
        terminal_at,
    })
}

#[cfg(test)]
fn after_action_report(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: &Value,
    review: ReadState<'_>,
    finality: &Value,
) -> String {
    let Some(paper_uuid) = canonical_uuid_text(paper_id) else {
        return aar_unavailable();
    };
    let mut room = room.clone();
    complete_test_room_envelope(&mut room);
    let room = match AuthenticatedPaperRoom::test_only_seal(paper_uuid, room) {
        Ok(room) => room,
        Err(_) => return aar_unavailable(),
    };
    let (sealed_review, review_missing) = match review {
        ReadState::Available(review) => {
            let mut review = review.clone();
            complete_test_review_envelope(&mut review, finality.clone());
            match AuthenticatedPaperReviewState::test_only_seal(paper_uuid, review) {
                Ok(review) => (Some(review), None),
                Err(_) => return aar_unavailable(),
            }
        }
        ReadState::NotFound => (None, Some(false)),
        ReadState::Unavailable => (None, Some(true)),
    };
    let review_state = match (&sealed_review, review_missing) {
        (Some(review), None) => ReadState::Available(review),
        (None, Some(false)) => ReadState::NotFound,
        (None, Some(true)) => ReadState::Unavailable,
        _ => return aar_unavailable(),
    };
    authenticated_after_action_report(
        identity,
        paper_id,
        ReadState::Available(&room),
        review_state,
    )
}

#[cfg(test)]
fn complete_test_room_envelope(room: &mut Value) {
    let Some(object) = room.as_object_mut() else {
        return;
    };
    object
        .entry("author_raid_progress")
        .or_insert_with(|| serde_json::json!({}));
    for field in [
        "team_member_acceptances",
        "work_items",
        "paper_revisions",
        "authorship_consents",
        "member_research_sessions",
        "artifact_manifests",
        "revision_artifact_bindings",
        "evidence_cards",
        "citations",
        "experiment_plans",
        "runs",
        "figures",
        "claims",
        "section_heads",
        "leases",
        "proposals",
        "decisions",
        "section_revisions",
        "section_reviews",
        "section_merges",
    ] {
        object
            .entry(field)
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    object.entry("joint_submission").or_insert(Value::Null);
    object
        .entry("last_event_cursor")
        .or_insert(serde_json::json!(0));
}

#[cfg(test)]
fn complete_test_review_envelope(review: &mut Value, finality: Value) {
    let Some(object) = review.as_object_mut() else {
        return;
    };
    object.insert("finality".into(), finality);
    for field in [
        "evaluation_drafts",
        "assignments",
        "contribution_ledgers",
        "evaluations",
        "reproductions",
        "appeals",
        "resolutions",
        "raid_scores",
    ] {
        object
            .entry(field)
            .or_insert_with(|| Value::Array(Vec::new()));
    }
}

fn validated_role_resource_actions<'a>(
    room: &Value,
    paper: &'a Value,
    paper_id: &str,
) -> Option<(&'a [Value], u64)> {
    let paper_uuid = canonical_uuid_text(paper_id)?;
    let paper_team_id = canonical_uuid_value(paper.get("team_id")?)?;
    let paper_challenge_id = canonical_uuid_value(paper.get("challenge_id")?)?;
    let team = room.get("team")?.as_object()?;
    if canonical_uuid_value(team.get("team_id")?)? != paper_team_id
        || canonical_uuid_value(team.get("challenge_id")?)? != paper_challenge_id
        || team.get("status")?.as_str()? != "locked"
        || team.get("roster_version")?.as_u64()? == 0
    {
        return None;
    }
    let members = team.get("members")?.as_array()?;
    if !(3..=5).contains(&members.len()) {
        return None;
    }
    let mut member_ids = HashSet::with_capacity(members.len());
    let mut member_slots = HashSet::with_capacity(members.len());
    let mut canonical_actors = HashMap::with_capacity(3);
    for member in members {
        let participant_slot = member.get("participant_slot")?.as_u64()?;
        let player_id = canonical_uuid_value(member.get("player_id")?)?;
        let role = member.get("role")?.as_str()?;
        if !(1..=u64::try_from(members.len()).ok()?).contains(&participant_slot)
            || !member_slots.insert(participant_slot)
            || !member_ids.insert(player_id)
            || !matches!(role, "captain" | "evidence" | "experiment" | "support")
        {
            return None;
        }
        if role != "support" && canonical_actors.insert(role, player_id).is_some() {
            return None;
        }
    }
    if canonical_actors.len() != 3 {
        return None;
    }

    let evidence_cards = room.get("evidence_cards")?.as_array()?;
    if evidence_cards.len() > 4_096 {
        return None;
    }
    let mut evidence_ids = HashSet::with_capacity(evidence_cards.len());
    for card in evidence_cards {
        if card.get("paper_project_id").and_then(Value::as_str) != Some(paper_id) {
            return None;
        }
        let evidence_id = canonical_uuid_value(card.get("evidence_card_id")?)?;
        if !evidence_ids.insert(evidence_id) {
            return None;
        }
    }

    let runs = room.get("runs")?.as_array()?;
    if runs.len() > 512 {
        return None;
    }
    let mut runs_by_id = HashMap::with_capacity(runs.len());
    for run in runs {
        validated_run_status(run, paper_id)?;
        let run_id = canonical_uuid_value(run.get("run_record_id")?)?;
        if runs_by_id.insert(run_id, run).is_some() {
            return None;
        }
    }

    let resources = paper.get("role_resources")?.as_object()?;
    if resources.len() != 13
        || resources.get("schema")?.as_str()? != "hepta.paper_raid.role_resources.v1"
        || [
            "ranking_eligible",
            "reward_eligible",
            "economic_eligibility",
        ]
        .iter()
        .any(|field| resources.get(*field).and_then(Value::as_bool) != Some(false))
    {
        return None;
    }
    let allocation = resources.get("allocation")?.as_object()?;
    if allocation.len() != 5 {
        return None;
    }
    let captain_allocation = allocation.get("captain_focus")?.as_u64()?;
    let evidence_allocation = allocation.get("evidence_focus")?.as_u64()?;
    let experiment_allocation = allocation.get("experiment_focus")?.as_u64()?;
    let run_allocation = allocation.get("run_budget")?.as_u64()?;
    let retained_refund = allocation.get("retained_failure_focus_refund")?.as_u64()?;
    if [
        captain_allocation,
        evidence_allocation,
        experiment_allocation,
        run_allocation,
    ]
    .iter()
    .any(|value| !(1..=32).contains(value))
        || !(1..=4).contains(&retained_refund)
        || retained_refund > experiment_allocation
        || captain_allocation > evidence_allocation
        || captain_allocation > experiment_allocation
        || captain_allocation > run_allocation
    {
        return None;
    }
    let frozen_allocation = paper
        .get("challenge_ruleset_snapshot")?
        .get("ruleset")?
        .get("gameplay")?
        .get("role_resources")?
        .as_object()?;
    if frozen_allocation != allocation {
        return None;
    }

    let created_at = resources
        .get("created_at")?
        .as_str()
        .and_then(canonical_timestamp)?;
    let updated_at = resources
        .get("updated_at")?
        .as_str()
        .and_then(canonical_timestamp)?;
    if updated_at < created_at {
        return None;
    }
    let actions = resources.get("actions")?.as_array()?;
    if actions.len() > 512
        || resources.get("version")?.as_u64()?
            != u64::try_from(actions.len()).ok()?.checked_add(1)?
    {
        return None;
    }

    let mut captain_focus = captain_allocation;
    let mut evidence_focus = evidence_allocation;
    let mut experiment_focus = experiment_allocation;
    let mut run_budget = run_allocation;
    let mut evidence_actions = 0usize;
    let mut experiment_actions = 0usize;
    let mut captain_actions = 0usize;
    let mut action_ids = HashSet::with_capacity(actions.len());
    let mut evidence_subjects = HashSet::new();
    let mut run_subjects = HashSet::new();
    let mut previous_at = created_at;
    for action in actions {
        if action.as_object().is_none_or(|value| value.len() != 9) {
            return None;
        }
        let action_id = canonical_uuid_value(action.get("action_id")?)?;
        let actor_id = canonical_uuid_value(action.get("actor_player_id")?)?;
        let subject_id = canonical_uuid_value(action.get("subject_id")?)?;
        let actor_role = action.get("actor_role")?.as_str()?;
        let occurred_at = action
            .get("occurred_at")?
            .as_str()
            .and_then(canonical_timestamp)?;
        if !action_ids.insert(action_id)
            || canonical_actors.get(actor_role) != Some(&actor_id)
            || occurred_at < previous_at
            || occurred_at > updated_at
        {
            return None;
        }
        previous_at = occurred_at;
        let focus_spent = action.get("focus_spent")?.as_u64()?;
        let focus_refunded = action.get("focus_refunded")?.as_u64()?;
        let run_budget_spent = action.get("run_budget_spent")?.as_u64()?;
        match action.get("kind")?.as_str()? {
            "evidence_assessment" => {
                if actor_role != "evidence"
                    || focus_spent != 1
                    || focus_refunded != 0
                    || run_budget_spent != 0
                    || !evidence_ids.contains(&subject_id)
                    || !evidence_subjects.insert(subject_id)
                {
                    return None;
                }
                evidence_focus = evidence_focus.checked_sub(1)?;
                evidence_actions += 1;
            }
            "experiment_run" => {
                let run = runs_by_id.get(&subject_id)?;
                let after_spend = experiment_focus.checked_sub(1)?;
                let expected_refund = if run.get("status")?.as_str()? == "failed" {
                    retained_refund.min(experiment_allocation.checked_sub(after_spend)?)
                } else {
                    0
                };
                if actor_role != "experiment"
                    || action_id != subject_id
                    || focus_spent != 1
                    || focus_refunded != expected_refund
                    || run_budget_spent != 1
                    || !run_subjects.insert(subject_id)
                {
                    return None;
                }
                experiment_focus = after_spend.checked_add(focus_refunded)?;
                if experiment_focus > experiment_allocation {
                    return None;
                }
                run_budget = run_budget.checked_sub(1)?;
                experiment_actions += 1;
            }
            "captain_checkpoint" => {
                if actor_role != "captain"
                    || focus_spent != 1
                    || focus_refunded != 0
                    || run_budget_spent != 0
                    || subject_id != action_id
                    || evidence_actions <= captain_actions
                    || experiment_actions <= captain_actions
                {
                    return None;
                }
                captain_focus = captain_focus.checked_sub(1)?;
                captain_actions += 1;
            }
            _ => return None,
        }
    }
    if actions.last().is_some_and(|action| {
        action.get("occurred_at").and_then(Value::as_str)
            != resources.get("updated_at").and_then(Value::as_str)
    }) || run_subjects.len() != runs_by_id.len()
        || runs_by_id
            .keys()
            .any(|run_id| !run_subjects.contains(run_id))
        || resources.get("captain_focus_remaining")?.as_u64()? != captain_focus
        || resources.get("evidence_focus_remaining")?.as_u64()? != evidence_focus
        || resources.get("experiment_focus_remaining")?.as_u64()? != experiment_focus
        || resources.get("run_budget_remaining")?.as_u64()? != run_budget
        || paper.get("paper_project_id").and_then(Value::as_str) != Some(paper_id)
        || paper_uuid.is_nil()
    {
        return None;
    }
    Some((actions, retained_refund))
}

struct ValidatedRunDebrief {
    succeeded: usize,
    failed: usize,
    cancelled: usize,
    retained_failed: usize,
    refunded_failed: usize,
}

impl ValidatedRunDebrief {
    fn render(&self) -> String {
        format!(
            r#"<article class="card after-action-runs"><h3>Experiment debrief / 实验复盘</h3><p>{} succeeded · {} failed · {} cancelled</p><p><strong>{}</strong> failed run(s) retained a disclosure; <strong>{}</strong> have an authoritative bounded focus refund. Cancelled runs never receive it / {} 次失败运行保留了披露；其中 {} 次有权威、有限的专注返还，取消运行绝不返还。</p></article>"#,
            self.succeeded,
            self.failed,
            self.cancelled,
            self.retained_failed,
            self.refunded_failed,
            self.retained_failed,
            self.refunded_failed,
        )
    }
}

fn validated_after_action_runs(
    room: &Value,
    paper: &Value,
    paper_id: &str,
) -> Option<ValidatedRunDebrief> {
    let runs = room.get("runs")?.as_array()?;
    if runs.len() > 512 {
        return None;
    }
    let (actions, _) = validated_role_resource_actions(room, paper, paper_id)?;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    let mut retained_failed = 0usize;
    let mut refunded_failed = 0usize;
    let mut run_ids = HashSet::with_capacity(runs.len());
    for run in runs {
        if !run.is_object() || run.get("paper_project_id").and_then(Value::as_str) != Some(paper_id)
        {
            return None;
        }
        let run_id = canonical_uuid_value(run.get("run_record_id")?)?;
        if !run_ids.insert(run_id) {
            return None;
        }
        let matching_actions = actions
            .iter()
            .filter(|action| {
                action.get("kind").and_then(Value::as_str) == Some("experiment_run")
                    && action
                        .get("subject_id")
                        .and_then(Value::as_str)
                        .and_then(canonical_uuid_text)
                        == Some(run_id)
            })
            .collect::<Vec<_>>();
        if matching_actions.len() != 1 {
            return None;
        }
        let focus_refunded = matching_actions[0]
            .get("focus_refunded")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        match validated_run_status(run, paper_id) {
            Some("succeeded") if focus_refunded == 0 => succeeded += 1,
            Some("failed") => {
                let failure_retained = run
                    .get("failure_hash")
                    .and_then(Value::as_str)
                    .is_some_and(valid_sha256_digest);
                if !failure_retained {
                    return None;
                }
                failed += 1;
                retained_failed += 1;
                if focus_refunded > 0 {
                    refunded_failed += 1;
                }
            }
            Some("cancelled")
                if run
                    .get("failure_hash")
                    .and_then(Value::as_str)
                    .is_some_and(valid_sha256_digest)
                    && focus_refunded == 0 =>
            {
                cancelled += 1
            }
            _ => return None,
        }
    }
    let experiment_action_subjects = actions
        .iter()
        .filter(|action| action.get("kind").and_then(Value::as_str) == Some("experiment_run"))
        .filter_map(|action| {
            action
                .get("subject_id")
                .and_then(Value::as_str)
                .and_then(canonical_uuid_text)
        })
        .collect::<HashSet<_>>();
    if experiment_action_subjects != run_ids {
        return None;
    }
    Some(ValidatedRunDebrief {
        succeeded,
        failed,
        cancelled,
        retained_failed,
        refunded_failed,
    })
}

struct ValidatedRoleDebrief {
    evidence: usize,
    experiment: usize,
    captain: usize,
    retained_failed: usize,
}

impl ValidatedRoleDebrief {
    fn render(&self) -> String {
        format!(
            r#"<article class="card after-action-roles"><h3>Role decisions / 角色决策</h3><p>Captain: {} checkpoint(s) · Evidence: {} assessment(s) · Experiment: {} retained run action(s)</p><p>{} disclosed failed run action(s) preserved as useful evidence / {} 次已披露失败运行作为有用证据保留。</p></article>"#,
            self.captain,
            self.evidence,
            self.experiment,
            self.retained_failed,
            self.retained_failed,
        )
    }
}

fn validated_after_action_roles(
    room: &Value,
    paper: &Value,
    paper_id: &str,
) -> Option<ValidatedRoleDebrief> {
    let (actions, _) = validated_role_resource_actions(room, paper, paper_id)?;
    let mut evidence = 0usize;
    let mut experiment = 0usize;
    let mut captain = 0usize;
    let mut retained_failed = 0usize;
    for action in actions {
        match action.get("kind").and_then(Value::as_str) {
            Some("evidence_assessment") => evidence += 1,
            Some("experiment_run") => {
                experiment += 1;
                if action
                    .get("focus_refunded")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
                {
                    retained_failed += 1;
                }
            }
            Some("captain_checkpoint") => captain += 1,
            _ => return None,
        }
    }
    Some(ValidatedRoleDebrief {
        evidence,
        experiment,
        captain,
        retained_failed,
    })
}

fn normalized_credit_roles(value: Option<&Value>) -> Option<Vec<String>> {
    const CREDIT_ROLES: [&str; 14] = [
        "conceptualization",
        "data_curation",
        "formal_analysis",
        "funding_acquisition",
        "investigation",
        "methodology",
        "project_administration",
        "resources",
        "software",
        "supervision",
        "validation",
        "visualization",
        "writing_original_draft",
        "writing_review_editing",
    ];
    let roles = value?.as_array()?;
    if roles.is_empty() || roles.len() > CREDIT_ROLES.len() {
        return None;
    }
    let mut normalized = Vec::with_capacity(roles.len());
    for role in roles {
        let role = role.as_str()?;
        if !CREDIT_ROLES.contains(&role) {
            return None;
        }
        normalized.push(role.to_string());
    }
    normalized.sort_unstable();
    if normalized.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(normalized)
}

fn frozen_release_credit_roster(
    room: &Value,
    paper_id: &str,
    release_candidate_hash: &str,
    ledger_hash: &str,
) -> Option<HashMap<Uuid, Vec<String>>> {
    let paper_uuid = canonical_uuid_text(paper_id)?;
    let paper = room.get("paper")?.as_object()?;
    let release_revision_id = canonical_uuid_value(paper.get("release_candidate_revision_id")?)?;
    let team_id = canonical_uuid_value(paper.get("team_id")?)?;
    let challenge_id = canonical_uuid_value(paper.get("challenge_id")?)?;

    let revisions = room.get("paper_revisions")?.as_array()?;
    if revisions.is_empty() || revisions.len() > 4_096 {
        return None;
    }
    let mut revision_ids = HashSet::with_capacity(revisions.len());
    let mut current_release = None;
    let mut release_status_count = 0usize;
    for revision in revisions {
        if revision.get("paper_project_id").and_then(Value::as_str) != Some(paper_id) {
            return None;
        }
        let revision_id = canonical_uuid_value(revision.get("revision_id")?)?;
        if !revision_ids.insert(revision_id) {
            return None;
        }
        if revision.get("status")?.as_str()? == "release_candidate" {
            release_status_count += 1;
            if revision_id == release_revision_id {
                current_release = Some(revision);
            }
        }
    }
    let revision = current_release.filter(|_| release_status_count == 1)?;
    if revision
        .get("release_candidate_hash")
        .and_then(Value::as_str)
        .filter(|value| valid_sha256_digest(value))
        != Some(release_candidate_hash)
    {
        return None;
    }
    let candidate_value = revision.get("release_candidate")?;
    let candidate_record =
        serde_json::from_value::<PaperReleaseCandidateV2>(candidate_value.clone()).ok()?;
    if paper_release_candidate_hash(&candidate_record)
        .ok()?
        .as_str()
        != release_candidate_hash
    {
        return None;
    }
    let candidate = candidate_value.as_object()?;
    let ruleset_snapshot = paper.get("challenge_ruleset_snapshot")?.as_object()?;
    if candidate.get("schema")?.as_str()? != "hepta.paper_raid.release_candidate.v2"
        || canonical_uuid_value(candidate.get("paper_project_id")?)? != paper_uuid
        || canonical_uuid_value(candidate.get("revision_id")?)? != release_revision_id
        || canonical_uuid_value(candidate.get("team_id")?)? != team_id
        || canonical_uuid_value(candidate.get("challenge_id")?)? != challenge_id
        || candidate.get("contribution_ledger_hash")?.as_str()? != ledger_hash
        || candidate.get("ruleset_hash")?.as_str()?
            != ruleset_snapshot.get("ruleset_hash")?.as_str()?
        || candidate.get("challenge_snapshot_hash")?.as_str()?
            != ruleset_snapshot.get("challenge_snapshot_hash")?.as_str()?
        || !valid_sha256_digest(ledger_hash)
    {
        return None;
    }

    let team = room.get("team")?.as_object()?;
    let roster_version = team.get("roster_version")?.as_u64()?;
    if canonical_uuid_value(team.get("team_id")?)? != team_id
        || canonical_uuid_value(team.get("challenge_id")?)? != challenge_id
        || team.get("status")?.as_str()? != "locked"
        || roster_version == 0
        || candidate.get("roster_version")?.as_u64()? != roster_version
    {
        return None;
    }
    let members = team.get("members")?.as_array()?;
    let authors = candidate.get("authors")?.as_array()?;
    if !(3..=5).contains(&members.len()) || authors.len() != members.len() {
        return None;
    }
    let mut members_by_slot = HashMap::with_capacity(members.len());
    let mut member_ids = HashSet::with_capacity(members.len());
    for member in members {
        let slot = member.get("participant_slot")?.as_u64()?;
        let player_id = canonical_uuid_value(member.get("player_id")?)?;
        if !(1..=5).contains(&slot)
            || !member_ids.insert(player_id)
            || members_by_slot.insert(slot, player_id).is_some()
        {
            return None;
        }
    }
    if (1..=u64::try_from(members.len()).ok()?).any(|slot| !members_by_slot.contains_key(&slot)) {
        return None;
    }

    let mut orders = HashSet::with_capacity(authors.len());
    let mut author_slots = HashSet::with_capacity(authors.len());
    let mut roster = HashMap::with_capacity(authors.len());
    for author in authors {
        if author.as_object().is_none_or(|value| value.len() != 5) {
            return None;
        }
        let order = author.get("author_order")?.as_u64()?;
        let slot = author.get("participant_slot")?.as_u64()?;
        let player_id = canonical_uuid_value(author.get("player_id")?)?;
        let display_name = author.get("display_name")?.as_str()?;
        let roles = normalized_credit_roles(author.get("credit_roles"))?;
        if !(1..=u64::try_from(authors.len()).ok()?).contains(&order)
            || !(1..=5).contains(&slot)
            || display_name.is_empty()
            || display_name.len() > 256
            || !orders.insert(order)
            || !author_slots.insert(slot)
            || members_by_slot.get(&slot) != Some(&player_id)
            || roster.insert(player_id, roles).is_some()
        {
            return None;
        }
    }
    if roster.len() != member_ids.len()
        || member_ids
            .iter()
            .any(|player_id| !roster.contains_key(player_id))
    {
        return None;
    }
    Some(roster)
}

fn validated_joint_submission(
    room: &Value,
    paper_id: &str,
    release_candidate_hash: &str,
) -> Option<(Uuid, String)> {
    let paper_uuid = canonical_uuid_text(paper_id)?;
    let release_revision_id =
        canonical_uuid_value(room.get("paper")?.get("release_candidate_revision_id")?)?;
    let submission = room.get("joint_submission")?.as_object()?;
    if submission.len() != 8
        || submission.get("paper_project_id")?.as_str()? != paper_id
        || canonical_uuid_value(submission.get("revision_id")?)? != release_revision_id
        || submission.get("release_candidate_hash")?.as_str()? != release_candidate_hash
        || submission.get("status")?.as_str()? != "submission_ready"
        || canonical_timestamp(submission.get("created_at")?.as_str()?).is_none()
    {
        return None;
    }
    let submission_id = canonical_uuid_value(submission.get("submission_id")?)?;
    let stored_bundle_hash = submission
        .get("paper_bundle_hash")?
        .as_str()
        .filter(|value| valid_sha256_digest(value))?;
    let bundle_value = submission.get("paper_bundle")?;
    for author in bundle_value
        .get("release_candidate")?
        .get("authors")?
        .as_array()?
    {
        canonical_uuid_value(author.get("player_id")?)?;
    }
    for consent in bundle_value.get("author_consents")?.as_array()? {
        canonical_uuid_value(consent.get("player_id")?)?;
    }
    let bundle = serde_json::from_value::<PaperBundleV2>(bundle_value.clone()).ok()?;
    if bundle.release_candidate.paper_project_id != paper_uuid
        || bundle.release_candidate.revision_id != release_revision_id
        || bundle.release_candidate_hash != release_candidate_hash
        || bundle.paper_bundle_hash != stored_bundle_hash
        || paper_bundle_hash(&bundle).ok()?.as_str() != stored_bundle_hash
    {
        return None;
    }
    Some((submission_id, stored_bundle_hash.to_string()))
}

fn validated_paper_score(
    paper_score: &Value,
    evaluation_id: Uuid,
    paper_uuid: Uuid,
    paper_id: &str,
) -> Option<(u64, bool)> {
    if paper_score.as_object().is_none_or(|value| value.len() != 9)
        || paper_score.get("schema")?.as_str()? != "hepta.paper_raid.paper_score.v1"
        || canonical_uuid_value(paper_score.get("evaluation_id")?)? != evaluation_id
        || paper_score.get("paper_project_id")?.as_str()? != paper_id
        || canonical_timestamp(paper_score.get("created_at")?.as_str()?).is_none()
    {
        return None;
    }
    let components = paper_score.get("components")?.as_object()?;
    if components.len() != 7 {
        return None;
    }
    let mut score_bps = 0_u64;
    for (field, maximum) in [
        ("method_rigor_bps", 2_500),
        ("experiment_statistics_bps", 1_500),
        ("reproducibility_bps", 1_500),
        ("evidence_citations_bps", 1_500),
        ("value_originality_bps", 1_500),
        ("argument_expression_bps", 1_000),
        ("ethics_transparency_bps", 500),
    ] {
        let actual = components.get(field)?.as_u64()?;
        if actual > maximum {
            return None;
        }
        score_bps = score_bps.checked_add(actual)?;
    }
    if paper_score.get("score_bps")?.as_u64()? != score_bps {
        return None;
    }

    let hard_gates = paper_score.get("hard_gates")?.as_object()?;
    if hard_gates.len() != 6 {
        return None;
    }
    let eligible = [
        "citations_and_data_authentic",
        "failed_runs_disclosed",
        "all_authors_consented",
        "core_claims_have_evidence",
        "artifact_lineage_complete",
        "license_ethics_coi_complete",
    ]
    .iter()
    .map(|field| hard_gates.get(*field).and_then(Value::as_bool))
    .collect::<Option<Vec<_>>>()?
    .into_iter()
    .all(|value| value);
    if paper_score.get("eligible")?.as_bool()? != eligible {
        return None;
    }
    let score_hash = paper_score
        .get("score_hash")?
        .as_str()
        .filter(|value| valid_sha256_digest(value))?;
    let expected_score_hash = canonical_json_sha256(&serde_json::json!({
        "schema": "hepta.paper_raid.paper_score.v1",
        "evaluation_id": evaluation_id,
        "paper_project_id": paper_uuid,
        "components": components,
        "hard_gates": hard_gates,
        "score_bps": score_bps,
        "eligible": eligible,
    }))
    .ok()?;
    (expected_score_hash == score_hash).then_some((score_bps, eligible))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewSettlement {
    PendingFinality,
    Challenged,
    Resolved,
}

impl ReviewSettlement {
    fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending_finality" => Some(Self::PendingFinality),
            "challenged" => Some(Self::Challenged),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct ValidatedEvaluationFact {
    evaluation_id: Uuid,
    supersedes_evaluation_id: Option<Uuid>,
    version: u64,
    settlement: ReviewSettlement,
    quality_gate_passed: bool,
    evaluator_player_id: Uuid,
    reviewer_player_ids: [Uuid; 2],
    created_at: DateTime<Utc>,
    created_at_text: String,
}

impl ValidatedEvaluationFact {
    fn panel_contains(&self, player_id: Uuid) -> bool {
        self.evaluator_player_id == player_id || self.reviewer_player_ids.contains(&player_id)
    }

    fn panel_is_disjoint(&self, other: &Self) -> bool {
        !other.panel_contains(self.evaluator_player_id)
            && self
                .reviewer_player_ids
                .iter()
                .all(|player_id| !other.panel_contains(*player_id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectiveEvaluationDisposition {
    Pending,
    AppealOpen,
    AppealDenied,
    AppealUpheld,
}

fn validated_tolerance_material(evaluation: &Value) -> Option<(String, String)> {
    let reference_metrics = evaluation.get("reference_metrics_micros")?.as_object()?;
    if reference_metrics.is_empty() || reference_metrics.len() > 256 {
        return None;
    }
    for (metric, value) in reference_metrics {
        if !valid_contract_text(metric) || value.as_i64().is_none() {
            return None;
        }
    }

    let policy = evaluation.get("tolerance_policy")?.as_object()?;
    if policy.len() != 3
        || policy.get("schema")?.as_str()? != "hepta.paper_raid.tolerance_policy.v1"
        || policy.get("version")?.as_str()? != "1"
    {
        return None;
    }
    let rules = policy.get("rules")?.as_array()?;
    if rules.is_empty() || rules.len() > 128 {
        return None;
    }
    let mut seen = HashSet::with_capacity(rules.len());
    for rule in rules {
        let rule = rule.as_object()?;
        let key = match rule.get("kind")?.as_str()? {
            "absolute" if rule.len() == 3 => {
                let metric = rule.get("metric")?.as_str()?;
                if !valid_contract_text(metric)
                    || !reference_metrics.contains_key(metric)
                    || rule.get("max_delta_micros")?.as_u64().is_none()
                {
                    return None;
                }
                format!("absolute:{metric}")
            }
            "relative" if rule.len() == 3 => {
                let metric = rule.get("metric")?.as_str()?;
                if !valid_contract_text(metric)
                    || !reference_metrics.contains_key(metric)
                    || rule.get("max_delta_bps")?.as_u64()? > 10_000
                {
                    return None;
                }
                format!("relative:{metric}")
            }
            "statistical" if rule.len() == 5 => {
                let metric = rule.get("metric")?.as_str()?;
                if !valid_contract_text(metric)
                    || !reference_metrics.contains_key(metric)
                    || rule.get("minimum_interval_overlap_bps")?.as_u64()? > 10_000
                    || rule.get("maximum_effect_delta_micros")?.as_u64().is_none()
                    || rule.get("minimum_p_value_micros")?.as_u64()? > 1_000_000
                {
                    return None;
                }
                format!("statistical:{metric}")
            }
            "seed" if rule.len() == 2 => {
                let hash = rule
                    .get("expected_seed_set_hash")?
                    .as_str()
                    .filter(|value| valid_sha256_digest(value))?;
                format!("seed:{hash}")
            }
            _ => return None,
        };
        if !seen.insert(key) {
            return None;
        }
    }
    Some((
        canonical_json_sha256(policy).ok()?,
        canonical_json_sha256(reference_metrics).ok()?,
    ))
}

fn validated_evaluation_quality(
    evaluation: &Value,
    paper_uuid: Uuid,
    paper_id: &str,
    release_candidate_hash: &str,
    submission_id: Uuid,
    paper_bundle_hash: &str,
    author_roster: &HashMap<Uuid, Vec<String>>,
) -> Option<ValidatedEvaluationFact> {
    if evaluation.as_object()?.len() != 24
        || evaluation.get("schema")?.as_str()? != "hepta.paper_raid.evaluation.v1"
        || evaluation.get("paper_project_id")?.as_str()? != paper_id
        || canonical_uuid_value(evaluation.get("submission_id")?)? != submission_id
        || evaluation.get("release_candidate_hash")?.as_str()? != release_candidate_hash
        || evaluation.get("paper_bundle_hash")?.as_str()? != paper_bundle_hash
    {
        return None;
    }
    let evaluation_id = canonical_uuid_value(evaluation.get("evaluation_id")?)?;
    let supersedes_evaluation_id =
        optional_non_nil_uuid(evaluation.get("supersedes_evaluation_id")?)?;
    let version = evaluation.get("version")?.as_u64()?;
    let settlement = ReviewSettlement::from_str(evaluation.get("settlement_state")?.as_str()?)?;
    let created_at_text = evaluation.get("created_at")?.as_str()?;
    let created_at = canonical_timestamp(created_at_text)?;
    if supersedes_evaluation_id == Some(evaluation_id) || version == 0 {
        return None;
    }

    let (tolerance_policy_hash, reference_metrics_hash) = validated_tolerance_material(evaluation)?;
    if evaluation.get("tolerance_policy_hash")?.as_str()? != tolerance_policy_hash {
        return None;
    }
    let paper_score = evaluation.get("paper_score")?;
    let (score_bps, eligible) =
        validated_paper_score(paper_score, evaluation_id, paper_uuid, paper_id)?;
    if paper_score.get("created_at")?.as_str()? != created_at_text {
        return None;
    }
    let paper_score_hash = paper_score.get("score_hash")?.as_str()?;
    let hard_gates_hash = canonical_json_sha256(paper_score.get("hard_gates")?).ok()?;

    let evaluator_id = canonical_uuid_value(evaluation.get("evaluator_player_id")?)?;
    let evaluator_key_id = evaluation.get("evaluator_signing_key_id")?.as_str()?;
    let evaluator_public_key = evaluation.get("evaluator_signing_public_key")?.as_str()?;
    let evaluator_public_key_hash = evaluation
        .get("evaluator_signing_public_key_hash")?
        .as_str()?;
    let evaluator_coi_hash = evaluation.get("evaluator_coi_attestation_hash")?.as_str()?;
    let evaluator_signed_at = evaluation.get("evaluator_signed_at_unix")?.as_i64()?;
    if author_roster.contains_key(&evaluator_id)
        || !valid_contract_text(evaluator_key_id)
        || !valid_sha256_digest(evaluator_public_key_hash)
        || !valid_public_key_snapshot(evaluator_public_key, evaluator_public_key_hash)
        || !valid_sha256_digest(evaluator_coi_hash)
        || evaluator_signed_at < 0
        || canonical_base64(evaluation.get("evaluator_signature")?.as_str()?, 64).is_none()
    {
        return None;
    }
    let signing = PaperEvaluationSigningV1 {
        schema: "hepta.paper_raid.evaluation.v1".into(),
        evaluation_id,
        paper_project_id: paper_uuid,
        submission_id,
        release_candidate_hash: release_candidate_hash.into(),
        paper_bundle_hash: paper_bundle_hash.into(),
        supersedes_evaluation_id,
        tolerance_policy_hash,
        paper_score_hash: paper_score_hash.into(),
        reference_metrics_hash,
        hard_gates_hash,
        evaluator_player_id: evaluator_id,
        signing_key_id: evaluator_key_id.into(),
        signing_public_key_hash: evaluator_public_key_hash.into(),
        coi_attestation_hash: evaluator_coi_hash.into(),
        signed_at_unix: evaluator_signed_at,
    };
    let evaluation_signing_hash = sha256_digest(&paper_evaluation_signing_bytes(&signing).ok()?);
    if evaluation.get("evaluation_signing_hash")?.as_str()? != evaluation_signing_hash {
        return None;
    }

    let attestations = evaluation.get("reviewer_attestations")?.as_array()?;
    if attestations.len() != 2 {
        return None;
    }
    let mut attestation_ids = HashSet::with_capacity(2);
    let mut reviewer_ids = Vec::with_capacity(2);
    let mut previous_reviewer = None;
    let mut approvals = 0usize;
    for attestation in attestations {
        if attestation.as_object()?.len() != 11 {
            return None;
        }
        let attestation_id = canonical_uuid_value(attestation.get("attestation_id")?)?;
        let reviewer_id = canonical_uuid_value(attestation.get("reviewer_player_id")?)?;
        let verdict = attestation.get("verdict")?.as_str()?;
        let key_id = attestation.get("signing_key_id")?.as_str()?;
        let public_key = attestation.get("signing_public_key")?.as_str()?;
        let public_key_hash = attestation.get("signing_public_key_hash")?.as_str()?;
        let coi_hash = attestation.get("coi_attestation_hash")?.as_str()?;
        let signed_at = attestation.get("signed_at_unix")?.as_i64()?;
        if !attestation_ids.insert(attestation_id)
            || previous_reviewer.is_some_and(|previous| previous >= reviewer_id)
            || reviewer_id == evaluator_id
            || author_roster.contains_key(&reviewer_id)
            || canonical_uuid_value(attestation.get("evaluation_id")?)? != evaluation_id
            || attestation.get("evaluation_signing_hash")?.as_str()? != evaluation_signing_hash
            || !matches!(verdict, "approve" | "reject")
            || !valid_contract_text(key_id)
            || !valid_sha256_digest(public_key_hash)
            || !valid_public_key_snapshot(public_key, public_key_hash)
            || !valid_sha256_digest(coi_hash)
            || signed_at < 0
            || canonical_base64(attestation.get("signature")?.as_str()?, 64).is_none()
        {
            return None;
        }
        paper_review_attestation_signing_bytes(&PaperReviewAttestationSigningV1 {
            schema: "hepta.paper_raid.review_attestation.v1".into(),
            attestation_id,
            evaluation_id,
            evaluation_signing_hash: evaluation_signing_hash.clone(),
            reviewer_player_id: reviewer_id,
            verdict: verdict.into(),
            signing_key_id: key_id.into(),
            signing_public_key_hash: public_key_hash.into(),
            coi_attestation_hash: coi_hash.into(),
            signed_at_unix: signed_at,
        })
        .ok()?;
        previous_reviewer = Some(reviewer_id);
        reviewer_ids.push(reviewer_id);
        approvals += usize::from(verdict == "approve");
    }
    let expected_status = if !eligible {
        "not_eligible"
    } else if score_bps >= 6_000 && approvals == 2 {
        "accepted"
    } else {
        "rejected"
    };
    if evaluation.get("status")?.as_str()? != expected_status {
        return None;
    }
    Some(ValidatedEvaluationFact {
        evaluation_id,
        supersedes_evaluation_id,
        version,
        settlement,
        quality_gate_passed: expected_status == "accepted",
        evaluator_player_id: evaluator_id,
        reviewer_player_ids: [reviewer_ids[0], reviewer_ids[1]],
        created_at,
        created_at_text: created_at_text.into(),
    })
}

#[derive(Clone, Copy)]
struct ValidatedAppealFact {
    appeal_id: Uuid,
    evaluation_id: Uuid,
    appellant_player_id: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Copy)]
enum ValidatedResolutionOutcome {
    Upheld(Uuid),
    Denied,
}

#[derive(Clone, Copy)]
struct ValidatedResolutionFact {
    resolution_id: Uuid,
    appeal_id: Uuid,
    evaluation_id: Uuid,
    outcome: ValidatedResolutionOutcome,
    created_at: DateTime<Utc>,
}

struct EffectiveReviewEvaluation {
    effective_id: Uuid,
    disposition: EffectiveEvaluationDisposition,
    facts: HashMap<Uuid, ValidatedEvaluationFact>,
    activation_by_evaluation: HashMap<Uuid, DateTime<Utc>>,
    authority_at: DateTime<Utc>,
    effective_resolution_id: Option<Uuid>,
}

fn effective_review_evaluation(
    review: &Value,
    paper_uuid: Uuid,
    paper_id: &str,
    release_candidate_hash: &str,
    submission_id: Uuid,
    paper_bundle_hash: &str,
    author_roster: &HashMap<Uuid, Vec<String>>,
) -> Option<EffectiveReviewEvaluation> {
    let evaluations = review.get("evaluations")?.as_array()?;
    let appeals = review.get("appeals")?.as_array()?;
    let resolutions = review.get("resolutions")?.as_array()?;
    if evaluations.is_empty()
        || evaluations.len() > 4_096
        || appeals.len() > evaluations.len()
        || resolutions.len() > appeals.len()
    {
        return None;
    }
    let mut facts = HashMap::with_capacity(evaluations.len());
    let mut initial = None;
    let mut children = HashMap::with_capacity(evaluations.len().saturating_sub(1));
    for evaluation in evaluations {
        let fact = validated_evaluation_quality(
            evaluation,
            paper_uuid,
            paper_id,
            release_candidate_hash,
            submission_id,
            paper_bundle_hash,
            author_roster,
        )?;
        if fact.supersedes_evaluation_id.is_none() {
            if fact.version != 1 || initial.replace(fact.evaluation_id).is_some() {
                return None;
            }
        } else if children
            .insert(fact.supersedes_evaluation_id?, fact.evaluation_id)
            .is_some()
        {
            return None;
        }
        if facts.insert(fact.evaluation_id, fact).is_some() {
            return None;
        }
    }
    let initial = initial?;

    let mut appeal_by_evaluation = HashMap::with_capacity(appeals.len());
    let mut appeal_by_id = HashMap::with_capacity(appeals.len());
    for appeal in appeals {
        if appeal.as_object()?.len() != 15
            || appeal.get("schema")?.as_str()? != "hepta.paper_raid.appeal.v1"
            || appeal.get("paper_project_id")?.as_str()? != paper_id
            || appeal.get("release_candidate_hash")?.as_str()? != release_candidate_hash
            || appeal.get("version")?.as_u64()? != 1
        {
            return None;
        }
        let appeal_id = canonical_uuid_value(appeal.get("appeal_id")?)?;
        let evaluation_id = canonical_uuid_value(appeal.get("evaluation_id")?)?;
        let appellant_player_id = canonical_uuid_value(appeal.get("appellant_player_id")?)?;
        let created_at = canonical_timestamp(appeal.get("created_at")?.as_str()?)?;
        let signing_key_id = appeal.get("signing_key_id")?.as_str()?;
        let signing_public_key = appeal.get("signing_public_key")?.as_str()?;
        let signing_public_key_hash = appeal.get("signing_public_key_hash")?.as_str()?;
        let signed_at_unix = appeal.get("signed_at_unix")?.as_i64()?;
        if !facts.contains_key(&evaluation_id)
            || created_at < facts.get(&evaluation_id)?.created_at
            || !author_roster.contains_key(&appellant_player_id)
            || !valid_sha256_digest(appeal.get("grounds_hash")?.as_str()?)
            || !valid_sha256_digest(appeal.get("evidence_manifest_hash")?.as_str()?)
            || !valid_contract_text(signing_key_id)
            || !valid_sha256_digest(signing_public_key_hash)
            || !valid_public_key_snapshot(signing_public_key, signing_public_key_hash)
            || signed_at_unix < 0
            || canonical_base64(appeal.get("signature")?.as_str()?, 64).is_none()
        {
            return None;
        }
        paper_appeal_signing_bytes(&PaperAppealSigningV1 {
            schema: "hepta.paper_raid.appeal.v1".into(),
            appeal_id,
            evaluation_id,
            paper_project_id: paper_uuid,
            release_candidate_hash: release_candidate_hash.into(),
            appellant_player_id,
            grounds_hash: appeal.get("grounds_hash")?.as_str()?.into(),
            evidence_manifest_hash: appeal.get("evidence_manifest_hash")?.as_str()?.into(),
            signing_key_id: signing_key_id.into(),
            signing_public_key_hash: signing_public_key_hash.into(),
            signed_at_unix,
        })
        .ok()?;
        let fact = ValidatedAppealFact {
            appeal_id,
            evaluation_id,
            appellant_player_id,
            created_at,
        };
        if appeal_by_evaluation.insert(evaluation_id, fact).is_some()
            || appeal_by_id.insert(appeal_id, fact).is_some()
        {
            return None;
        }
    }

    let mut resolution_ids = HashSet::with_capacity(resolutions.len());
    let mut resolution_by_appeal = HashMap::with_capacity(resolutions.len());
    for resolution in resolutions {
        if resolution.as_object()?.len() != 17
            || resolution.get("schema")?.as_str()? != "hepta.paper_raid.appeal_resolution.v1"
            || resolution.get("paper_project_id")?.as_str()? != paper_id
            || resolution.get("release_candidate_hash")?.as_str()? != release_candidate_hash
            || resolution.get("version")?.as_u64()? != 1
        {
            return None;
        }
        let resolution_id = canonical_uuid_value(resolution.get("resolution_id")?)?;
        let appeal_id = canonical_uuid_value(resolution.get("appeal_id")?)?;
        let evaluation_id = canonical_uuid_value(resolution.get("evaluation_id")?)?;
        let resolver_player_id = canonical_uuid_value(resolution.get("resolver_player_id")?)?;
        let created_at = canonical_timestamp(resolution.get("created_at")?.as_str()?)?;
        let appeal = appeal_by_id.get(&appeal_id)?;
        let evaluation = facts.get(&evaluation_id)?;
        let signing_key_id = resolution.get("signing_key_id")?.as_str()?;
        let signing_public_key = resolution.get("signing_public_key")?.as_str()?;
        let signing_public_key_hash = resolution.get("signing_public_key_hash")?.as_str()?;
        let signed_at_unix = resolution.get("signed_at_unix")?.as_i64()?;
        let superseding_evaluation_id =
            optional_non_nil_uuid(resolution.get("superseding_evaluation_id")?)?;
        let outcome = match resolution.get("outcome")?.as_str()? {
            "upheld" => ValidatedResolutionOutcome::Upheld(superseding_evaluation_id?),
            "denied" if superseding_evaluation_id.is_none() => ValidatedResolutionOutcome::Denied,
            _ => return None,
        };
        if !resolution_ids.insert(resolution_id)
            || appeal.evaluation_id != evaluation_id
            || created_at < appeal.created_at
            || created_at < evaluation.created_at
            || author_roster.contains_key(&resolver_player_id)
            || resolver_player_id == appeal.appellant_player_id
            || evaluation.panel_contains(resolver_player_id)
            || !valid_sha256_digest(resolution.get("decision_hash")?.as_str()?)
            || !valid_contract_text(signing_key_id)
            || !valid_sha256_digest(signing_public_key_hash)
            || !valid_public_key_snapshot(signing_public_key, signing_public_key_hash)
            || signed_at_unix < 0
            || canonical_base64(resolution.get("signature")?.as_str()?, 64).is_none()
        {
            return None;
        }
        if let ValidatedResolutionOutcome::Upheld(replacement_id) = outcome {
            let replacement = facts.get(&replacement_id)?;
            if replacement.supersedes_evaluation_id != Some(evaluation_id)
                || children.get(&evaluation_id) != Some(&replacement_id)
                || replacement.panel_contains(resolver_player_id)
                || created_at < replacement.created_at
            {
                return None;
            }
        }
        paper_appeal_resolution_signing_bytes(&PaperAppealResolutionSigningV1 {
            schema: "hepta.paper_raid.appeal_resolution.v1".into(),
            resolution_id,
            appeal_id,
            evaluation_id,
            paper_project_id: paper_uuid,
            release_candidate_hash: release_candidate_hash.into(),
            outcome: resolution.get("outcome")?.as_str()?.into(),
            superseding_evaluation_id,
            decision_hash: resolution.get("decision_hash")?.as_str()?.into(),
            resolver_player_id,
            signing_key_id: signing_key_id.into(),
            signing_public_key_hash: signing_public_key_hash.into(),
            signed_at_unix,
        })
        .ok()?;
        if resolution_by_appeal
            .insert(
                appeal_id,
                ValidatedResolutionFact {
                    resolution_id,
                    appeal_id,
                    evaluation_id,
                    outcome,
                    created_at,
                },
            )
            .is_some()
        {
            return None;
        }
    }

    let mut activation_by_evaluation = HashMap::with_capacity(facts.len());
    activation_by_evaluation.insert(initial, facts.get(&initial)?.created_at);
    for fact in facts.values() {
        let expected_settlement = match appeal_by_evaluation.get(&fact.evaluation_id) {
            Some(appeal) if resolution_by_appeal.contains_key(&appeal.appeal_id) => {
                ReviewSettlement::Resolved
            }
            Some(_) => ReviewSettlement::Challenged,
            None => ReviewSettlement::PendingFinality,
        };
        if fact.settlement != expected_settlement {
            return None;
        }
        if let Some(base_id) = fact.supersedes_evaluation_id {
            let base = facts.get(&base_id)?;
            let appeal = appeal_by_evaluation.get(&base_id)?;
            if fact.version != base.version.checked_add(1)?
                || !fact.panel_is_disjoint(base)
                || fact.created_at < base.created_at
                || fact.created_at < appeal.created_at
            {
                return None;
            }
            if let Some(parent_resolution) = resolution_by_appeal.get(&appeal.appeal_id) {
                if !matches!(
                    parent_resolution.outcome,
                    ValidatedResolutionOutcome::Upheld(replacement_id)
                        if replacement_id == fact.evaluation_id
                ) {
                    return None;
                }
                if activation_by_evaluation
                    .insert(fact.evaluation_id, parent_resolution.created_at)
                    .is_some()
                {
                    return None;
                }
                if let Some(child_appeal) = appeal_by_evaluation.get(&fact.evaluation_id) {
                    if child_appeal.created_at < parent_resolution.created_at {
                        return None;
                    }
                    if resolution_by_appeal
                        .get(&child_appeal.appeal_id)
                        .is_some_and(|child_resolution| {
                            child_resolution.created_at < parent_resolution.created_at
                        })
                    {
                        return None;
                    }
                }
            } else if appeal_by_evaluation.contains_key(&fact.evaluation_id)
                || children.contains_key(&fact.evaluation_id)
            {
                // A direct replacement may be fully prepared while its
                // parent's Appeal remains open.  It is not authoritative yet,
                // and no descendant facts may hang from that inactive leaf.
                return None;
            }
        }
    }
    let mut visited = HashSet::with_capacity(facts.len());
    let mut cursor = initial;
    loop {
        if !visited.insert(cursor) {
            return None;
        }
        let Some(next) = children.get(&cursor).copied() else {
            break;
        };
        cursor = next;
    }
    if visited.len() != facts.len() {
        return None;
    }
    let mut effective = initial;
    let mut disposition = EffectiveEvaluationDisposition::Pending;
    let mut authority_at = facts.get(&initial)?.created_at;
    let mut effective_resolution_id = None;
    while let Some(appeal) = appeal_by_evaluation.get(&effective) {
        authority_at = authority_at.max(appeal.created_at);
        let Some(resolution) = resolution_by_appeal.get(&appeal.appeal_id) else {
            disposition = EffectiveEvaluationDisposition::AppealOpen;
            break;
        };
        if resolution.appeal_id != appeal.appeal_id || resolution.evaluation_id != effective {
            return None;
        }
        authority_at = authority_at.max(resolution.created_at);
        effective_resolution_id = Some(resolution.resolution_id);
        match resolution.outcome {
            ValidatedResolutionOutcome::Denied => {
                if children.contains_key(&effective) {
                    return None;
                }
                disposition = EffectiveEvaluationDisposition::AppealDenied;
                break;
            }
            ValidatedResolutionOutcome::Upheld(replacement_id) => {
                effective = replacement_id;
                authority_at = authority_at.max(facts.get(&replacement_id)?.created_at);
                disposition = EffectiveEvaluationDisposition::AppealUpheld;
            }
        }
    }
    Some(EffectiveReviewEvaluation {
        effective_id: effective,
        disposition,
        facts,
        activation_by_evaluation,
        authority_at,
        effective_resolution_id,
    })
}

fn validated_raid_score(
    score: &Value,
    paper_uuid: Uuid,
    paper_id: &str,
    evaluation: &ValidatedEvaluationFact,
    contribution_points: &[(String, u64)],
    current_player_id: &str,
) -> Option<(u64, u64)> {
    if score.as_object()?.len() != 9
        || score.get("schema")?.as_str()? != "hepta.paper_raid.raid_score.v1"
        || score.get("paper_project_id")?.as_str()? != paper_id
        || !score.get("paper_score_excluded")?.as_bool()?
        || score.get("created_at")?.as_str()? != evaluation.created_at_text
    {
        return None;
    }
    let score_id = canonical_uuid_value(score.get("raid_score_id")?)?;
    let evaluation_id = canonical_uuid_value(score.get("evaluation_id")?)?;
    if score_id != evaluation_id || evaluation_id != evaluation.evaluation_id {
        return None;
    }
    let team_xp = score.get("team_xp")?.as_u64()?;
    let player_xp = score.get("player_xp")?.as_object()?;
    if player_xp.len() != contribution_points.len() {
        return None;
    }
    let mut computed_team_xp = 0_u64;
    for (player_id, contribution) in contribution_points {
        canonical_uuid_text(player_id)?;
        let actual = player_xp.get(player_id)?.as_u64()?;
        let expected = if evaluation.quality_gate_passed {
            300_u64.checked_add(*contribution)?
        } else {
            0
        };
        if actual != expected {
            return None;
        }
        computed_team_xp = computed_team_xp.checked_add(actual)?;
    }
    if computed_team_xp != team_xp {
        return None;
    }
    let expected_score_hash = canonical_json_sha256(&serde_json::json!({
        "schema": "hepta.paper_raid.raid_score.v1",
        "raid_score_id": evaluation_id,
        "evaluation_id": evaluation_id,
        "paper_project_id": paper_uuid,
        "team_xp": team_xp,
        "player_xp": player_xp,
        "paper_score_excluded": true,
    }))
    .ok()?;
    if score
        .get("score_hash")?
        .as_str()
        .filter(|value| valid_sha256_digest(value))?
        != expected_score_hash
    {
        return None;
    }
    Some((team_xp, player_xp.get(current_player_id)?.as_u64()?))
}

struct ValidatedReviewAuthority {
    effective_evaluation_id: Uuid,
    disposition: EffectiveEvaluationDisposition,
    authority_at: DateTime<Utc>,
    effective_resolution_id: Option<Uuid>,
    effective_reproduction: Option<ValidatedReproductionAuthority>,
}

#[derive(Clone, Copy)]
struct ValidatedReproductionAuthority {
    reproduction_id: Uuid,
    reproduced: bool,
    created_at: DateTime<Utc>,
}

fn effective_reproduction_authority(
    review: &Value,
    evaluation_id: Uuid,
    activation_by_evaluation: &HashMap<Uuid, DateTime<Utc>>,
) -> Option<Option<ValidatedReproductionAuthority>> {
    let records = review.get("reproductions")?.as_array()?;
    let mut matching = HashMap::new();
    let mut superseded = HashSet::new();
    for record in records {
        let record_evaluation_id = canonical_uuid_value(record.get("evaluation_id")?)?;
        let created_at = canonical_timestamp(record.get("created_at")?.as_str()?)?;
        // A replacement evaluation may be prepared while its parent Appeal
        // is open, but it becomes authoritative only when that Appeal is
        // upheld.  Every reproduction in every replacement chain must follow
        // the corresponding activation; checking only the latest leaf would
        // let a forbidden early root survive behind a later supersession.
        if created_at < *activation_by_evaluation.get(&record_evaluation_id)? {
            return None;
        }
        if record_evaluation_id != evaluation_id {
            continue;
        }
        let reproduction_id = canonical_uuid_value(record.get("reproduction_id")?)?;
        let supersedes = optional_non_nil_uuid(record.get("supersedes_reproduction_id")?)?;
        let authority = ValidatedReproductionAuthority {
            reproduction_id,
            reproduced: record.get("status")?.as_str()? == "reproduced",
            created_at,
        };
        if matching.insert(reproduction_id, authority).is_some() {
            return None;
        }
        if let Some(parent_id) = supersedes {
            if !superseded.insert(parent_id) {
                return None;
            }
        }
    }
    let mut current = matching
        .into_iter()
        .filter(|(reproduction_id, _)| !superseded.contains(reproduction_id))
        .map(|(_, authority)| authority);
    let Some(authority) = current.next() else {
        return Some(None);
    };
    if current.next().is_some() || authority.reproduction_id.is_nil() {
        return None;
    }
    Some(Some(authority))
}

struct ValidatedAarXp {
    label: String,
    authority: Option<ValidatedReviewAuthority>,
}

struct ValidatedAfterActionXpInput<'a> {
    identity: &'a AlphaIdentity,
    paper_uuid: Uuid,
    paper_id: &'a str,
    room: &'a Value,
    review: &'a Value,
    release_candidate_hash: &'a str,
    author_roster: &'a HashMap<Uuid, Vec<String>>,
    contribution_points: &'a [(String, u64)],
    terminal_at: DateTime<Utc>,
}

fn validated_after_action_xp(input: ValidatedAfterActionXpInput<'_>) -> Option<ValidatedAarXp> {
    let ValidatedAfterActionXpInput {
        identity,
        paper_uuid,
        paper_id,
        room,
        review,
        release_candidate_hash,
        author_roster,
        contribution_points,
        terminal_at,
    } = input;
    let (submission_id, paper_bundle_hash) =
        validated_joint_submission(room, paper_id, release_candidate_hash)?;
    let evaluations = review.get("evaluations")?.as_array()?;
    let scores = review.get("raid_scores")?.as_array()?;
    let appeals = review.get("appeals")?.as_array()?;
    let resolutions = review.get("resolutions")?.as_array()?;
    if evaluations.is_empty() {
        return (scores.is_empty() && appeals.is_empty() && resolutions.is_empty()).then(|| {
            ValidatedAarXp {
                label: "Provisional XP is pending independent review / 暂定 XP 等待独立评审".into(),
                authority: None,
            }
        });
    }
    let effective = effective_review_evaluation(
        review,
        paper_uuid,
        paper_id,
        release_candidate_hash,
        submission_id,
        &paper_bundle_hash,
        author_roster,
    )?;
    if effective
        .facts
        .values()
        .any(|fact| fact.created_at < terminal_at)
        || scores.len() != effective.facts.len()
        || scores.len() > 4_096
    {
        return None;
    }
    let current_player_id = identity.player_id.to_string();
    let mut scored = HashMap::with_capacity(scores.len());
    for score in scores {
        let evaluation_id = canonical_uuid_value(score.get("evaluation_id")?)?;
        let evaluation = effective.facts.get(&evaluation_id)?;
        let xp = validated_raid_score(
            score,
            paper_uuid,
            paper_id,
            evaluation,
            contribution_points,
            &current_player_id,
        )?;
        if scored.insert(evaluation_id, xp).is_some() {
            return None;
        }
    }
    if effective
        .facts
        .keys()
        .any(|evaluation_id| !scored.contains_key(evaluation_id))
    {
        return None;
    }
    let (team_xp, player_xp) = scored.get(&effective.effective_id)?;
    let effective_reproduction = effective_reproduction_authority(
        review,
        effective.effective_id,
        &effective.activation_by_evaluation,
    )?;
    let prefix = match effective.disposition {
        EffectiveEvaluationDisposition::Pending => "",
        EffectiveEvaluationDisposition::AppealOpen => "Appeal open · ",
        EffectiveEvaluationDisposition::AppealDenied => "Appeal denied · ",
        EffectiveEvaluationDisposition::AppealUpheld => "Appeal upheld · ",
    };
    Some(ValidatedAarXp {
        label: format!(
            "{prefix}Team provisional XP: {team_xp} · Your provisional XP: {player_xp} / 团队暂定 XP：{team_xp} · 你的暂定 XP：{player_xp}"
        ),
        authority: Some(ValidatedReviewAuthority {
            effective_evaluation_id: effective.effective_id,
            disposition: effective.disposition,
            authority_at: effective.authority_at,
            effective_resolution_id: effective.effective_resolution_id,
            effective_reproduction,
        }),
    })
}

fn validated_aar_finality<'a>(
    finality: &'a Value,
    terminal_at: DateTime<Utc>,
    xp: &ValidatedAarXp,
) -> Option<&'a str> {
    if finality.as_object()?.len() != 10
        || finality.get("schema")?.as_str()? != "hepta.paper_raid.consumer_finality.v2"
        || [
            "ranking_eligible",
            "reward_eligible",
            "score_eligible",
            "economic_eligible",
        ]
        .iter()
        .any(|field| finality.get(*field).and_then(Value::as_bool) != Some(false))
    {
        return None;
    }
    let status = finality.get("status")?.as_str()?;
    match status {
        "pending_finality"
            if finality.get("effective_evaluation_id")?.is_null()
                && finality.get("effective_reproduction_id")?.is_null()
                && finality.get("effective_appeal_resolution_id")?.is_null()
                && finality.get("verified_at")?.is_null() =>
        {
            Some(status)
        }
        "verified_finality" => {
            let verified_at = canonical_timestamp(finality.get("verified_at")?.as_str()?)?;
            let authority = xp.authority.as_ref()?;
            let reproduction = authority.effective_reproduction?;
            let effective_evaluation_id =
                canonical_uuid_value(finality.get("effective_evaluation_id")?)?;
            let effective_reproduction_id =
                canonical_uuid_value(finality.get("effective_reproduction_id")?)?;
            let effective_appeal_resolution_id =
                optional_non_nil_uuid(finality.get("effective_appeal_resolution_id")?)?;
            let resolution_binding_valid = match authority.disposition {
                EffectiveEvaluationDisposition::Pending => {
                    authority.effective_resolution_id.is_none()
                        && effective_appeal_resolution_id.is_none()
                }
                EffectiveEvaluationDisposition::AppealDenied
                | EffectiveEvaluationDisposition::AppealUpheld => {
                    authority.effective_resolution_id.is_some()
                        && effective_appeal_resolution_id == authority.effective_resolution_id
                }
                EffectiveEvaluationDisposition::AppealOpen => false,
            };
            (effective_evaluation_id == authority.effective_evaluation_id
                && effective_reproduction_id == reproduction.reproduction_id
                && resolution_binding_valid
                && reproduction.reproduced
                && verified_at >= terminal_at
                && verified_at >= authority.authority_at
                && verified_at >= reproduction.created_at)
                .then_some(status)
        }
        _ => None,
    }
}

struct ValidatedContributionRow {
    role: String,
    is_current_player: bool,
    credit: String,
    accepted_artifacts: usize,
    accepted_reviews: usize,
    points: u64,
}

struct ValidatedContributionDebrief {
    rows: Vec<ValidatedContributionRow>,
    xp: ValidatedAarXp,
}

impl ValidatedContributionDebrief {
    fn render(&self) -> String {
        let rows = self
            .rows
            .iter()
            .map(|row| {
                format!(
                    r#"<li><strong>{}{}</strong><span>CRediT: {}</span><span>{} accepted artifact(s) · {} accepted review(s) · {} provisional point(s)</span></li>"#,
                    escape(&row.role),
                    if row.is_current_player { " · You / 你" } else { "" },
                    if row.credit.is_empty() {
                        "none / 无"
                    } else {
                        &row.credit
                    },
                    row.accepted_artifacts,
                    row.accepted_reviews,
                    row.points,
                )
            })
            .collect::<String>();
        format!(
            r#"<article class="card after-action-contribution"><h3>Contribution explanation / 贡献解释</h3><ul>{}</ul><p>{}</p><p class="muted">These are frozen milestone explanations and provisional telemetry—not leaderboard rank, reward, or economic value / 这是锁定里程碑解释与暂定遥测，不是排行榜、奖励或经济价值。</p></article>"#,
            rows,
            escape(&self.xp.label),
        )
    }
}

fn validated_after_action_contribution(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: &Value,
    review: &Value,
    terminal_at: DateTime<Utc>,
) -> Option<ValidatedContributionDebrief> {
    let paper_uuid = canonical_uuid_text(paper_id)?;
    let ledgers = review.get("contribution_ledgers")?.as_array()?;
    if ledgers.len() != 1 {
        return None;
    }
    let ledger = &ledgers[0];
    let ledger_id = ledger
        .get("contribution_ledger_id")
        .and_then(canonical_uuid_value);
    let release_candidate_hash = ledger
        .get("release_candidate_hash")
        .and_then(Value::as_str)
        .filter(|value| valid_sha256_digest(value));
    let ledger_hash = ledger
        .get("ledger_hash")
        .and_then(Value::as_str)
        .filter(|value| valid_sha256_digest(value));
    if ledger.as_object().is_none_or(|value| value.len() != 8)
        || ledger.get("schema").and_then(Value::as_str)
            != Some("hepta.paper_raid.contribution_ledger.v1")
        || ledger.get("paper_project_id").and_then(Value::as_str) != Some(paper_id)
        || ledger_id.is_none()
        || release_candidate_hash.is_none()
        || ledger_hash.is_none()
        || ledger.get("version").and_then(Value::as_u64) != Some(1)
        || ledger
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(canonical_timestamp)
            .is_none()
    {
        return None;
    }
    let ledger_id = ledger_id.expect("checked above");
    let release_candidate_hash = release_candidate_hash.expect("checked above");
    let ledger_hash = ledger_hash.expect("checked above");
    let frozen_credit_roster =
        frozen_release_credit_roster(room, paper_id, release_candidate_hash, ledger_hash)?;
    let entries = ledger.get("entries")?.as_array()?;
    if entries.len() != frozen_credit_roster.len() {
        return None;
    }
    let members = room
        .get("team")
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)?;
    if !(3..=5).contains(&members.len()) || entries.len() != members.len() {
        return None;
    }
    let mut seen = HashSet::with_capacity(entries.len());
    let mut previous_player_id = None;
    let mut contribution_points = Vec::with_capacity(entries.len());
    let mut normalized_entries = Vec::with_capacity(entries.len());
    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.as_object().is_none_or(|value| value.len() != 5) {
            return None;
        }
        let player_id = entry.get("player_id")?.as_str()?;
        let player_uuid = canonical_uuid_text(player_id)?;
        if !seen.insert(player_uuid)
            || previous_player_id.is_some_and(|previous| previous >= player_uuid)
        {
            return None;
        }
        previous_player_id = Some(player_uuid);
        let roles = members
            .iter()
            .filter(|member| member.get("player_id").and_then(Value::as_str) == Some(player_id))
            .filter_map(|member| member.get("role").and_then(Value::as_str))
            .collect::<Vec<_>>();
        if roles.len() != 1 {
            return None;
        }
        let normalized_credit = normalized_credit_roles(entry.get("credit_roles"))?;
        if frozen_credit_roster.get(&player_uuid) != Some(&normalized_credit)
            || entry
                .get("credit_roles")
                .and_then(Value::as_array)
                .is_none_or(|roles| {
                    roles
                        .iter()
                        .filter_map(Value::as_str)
                        .ne(normalized_credit.iter().map(String::as_str))
                })
        {
            return None;
        }
        let credit = normalized_credit
            .iter()
            .map(|role| escape(role))
            .collect::<Vec<_>>()
            .join(", ");
        let artifacts = canonical_uuid_array(entry.get("accepted_artifact_manifest_ids"));
        let reviews = canonical_uuid_array(entry.get("accepted_section_review_ids"));
        let points = entry.get("contribution_points").and_then(Value::as_u64);
        let (Some(artifacts), Some(reviews), Some(points)) = (artifacts, reviews, points) else {
            return None;
        };
        let artifacts = artifacts
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let reviews = reviews
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let (expected_artifacts, expected_reviews) =
            authoritative_contribution_refs(room, paper_id, player_id)?;
        if artifacts != expected_artifacts || reviews != expected_reviews {
            return None;
        }
        let expected_points =
            (if artifacts.is_empty() { 0 } else { 100 }) + if reviews.is_empty() { 0 } else { 150 };
        if points != expected_points {
            return None;
        }
        contribution_points.push((player_id.to_string(), points));
        normalized_entries.push(serde_json::json!({
            "player_id": player_uuid,
            "credit_roles": normalized_credit,
            "accepted_artifact_manifest_ids": artifacts,
            "accepted_section_review_ids": reviews,
            "contribution_points": points,
        }));
        rows.push(ValidatedContributionRow {
            role: roles[0].to_string(),
            is_current_player: player_uuid == identity.player_id,
            credit,
            accepted_artifacts: expected_artifacts.len(),
            accepted_reviews: expected_reviews.len(),
            points,
        });
    }
    if seen.len() != frozen_credit_roster.len()
        || frozen_credit_roster
            .keys()
            .any(|player_id| !seen.contains(player_id))
    {
        return None;
    }
    let expected_ledger_hash = canonical_json_sha256(&serde_json::json!({
        "schema": "hepta.paper_raid.contribution_ledger.v1",
        "contribution_ledger_id": ledger_id,
        "paper_project_id": paper_uuid,
        "entries": normalized_entries,
    }))
    .ok()?;
    if expected_ledger_hash != ledger_hash {
        return None;
    }

    let xp = validated_after_action_xp(ValidatedAfterActionXpInput {
        identity,
        paper_uuid,
        paper_id,
        room,
        review,
        release_candidate_hash,
        author_roster: &frozen_credit_roster,
        contribution_points: &contribution_points,
        terminal_at,
    })?;
    Some(ValidatedContributionDebrief { rows, xp })
}

#[cfg(test)]
fn finality_from_review_state(review: ReadState<'_>) -> Value {
    if let Some(finality) = review
        .value()
        .and_then(|value| value.get("finality"))
        .filter(|value| value.is_object())
    {
        return finality.clone();
    }
    let (status, reason_code) = match review {
        ReadState::NotFound => ("unknown_finality", "review_aggregate_not_found"),
        ReadState::Unavailable => ("error_finality", "review_projection_unavailable"),
        ReadState::Available(_) => ("unknown_finality", "review_projection_missing_finality"),
    };
    serde_json::json!({
        "schema": "hepta.paper_raid.bff_finality_availability.v1",
        "status": status,
        "authoritative": false,
        "reason_code": reason_code,
        "ranking_eligible": false,
        "reward_eligible": false,
        "score_eligible": false,
        "economic_eligible": false,
        "verified_at": null
    })
}

fn finality_projection(finality: &Value) -> (String, String) {
    let Some(status) = finality.get("status").and_then(Value::as_str) else {
        return (
            "<div class=\"status missing\"><strong>Finality verification error / 终局验证错误</strong> · <code>error_finality</code></div>".into(),
            "Finality verification error / 终局验证错误 (error_finality)".into(),
        );
    };
    let (class, display, authoritative) = match status {
        "verified_finality" => (
            "status verified",
            "Verified scientific finality / 科学终局已验证",
            true,
        ),
        "pending_finality" => (
            "status pending",
            "Verification pending / 等待终局验证",
            true,
        ),
        "unknown_finality" => ("status missing", "Finality unknown / 终局状态未知", false),
        "unavailable_finality" => (
            "status missing",
            "Finality temporarily unavailable / 终局暂不可用",
            false,
        ),
        "error_finality" => (
            "status missing",
            "Finality verification error / 终局验证错误",
            false,
        ),
        _ => (
            "status missing",
            "Finality verification error / 终局验证错误",
            false,
        ),
    };
    let eligibility = [
        ("ranking_eligible", "Ranking / 排名"),
        ("reward_eligible", "Reward / 奖励"),
        ("score_eligible", "Score / 评分"),
        ("economic_eligible", "Economic / 经济"),
    ]
    .iter()
    .map(|(field, label)| {
        let eligible = authoritative
            && finality
                .get(*field)
                .and_then(Value::as_bool)
                .unwrap_or(false);
        format!(
            r#"<li data-eligible="{}"><span>{}</span><strong>{}</strong></li>"#,
            eligible,
            escape(label),
            if eligible { "eligible" } else { "locked" },
        )
    })
    .collect::<String>();
    let reason = (!authoritative)
        .then(|| finality.get("reason_code").and_then(Value::as_str))
        .flatten()
        .map(|reason| {
            format!(
                r#"<p class="muted finality-reason">Reason / 原因: <code>{}</code></p>"#,
                escape(reason),
            )
        })
        .unwrap_or_default();
    (
        format!(
            r#"<div class="{}"><strong>{}</strong> · <code>{}</code></div>{}<ul class="eligibility-grid">{}</ul>"#,
            class,
            display,
            escape(status),
            reason,
            eligibility,
        ),
        format!("{display} ({status})"),
    )
}

fn role_resource_action<'a>(resources: &'a Value, name: &str) -> Option<&'a Value> {
    resources
        .get("actions")
        .and_then(Value::as_array)?
        .iter()
        .find(|action| action.get("action").and_then(Value::as_str) == Some(name))
}

fn role_resource_panel(
    paper_id: &str,
    paper: &Value,
    progress: Option<&Value>,
    room: &Value,
    current_role: &str,
) -> String {
    let Some(resources) = progress
        .and_then(|value| value.get("role_resources"))
        .filter(|value| value.is_object())
    else {
        return String::new();
    };
    let state_version = resources
        .get("state_version")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if state_version == 0 {
        return r#"<section class="panel role-resource-panel"><h2>Role resources / 角色资源</h2><p class="status missing">The authoritative resource version is unavailable; actions are fail-closed.</p></section>"#.into();
    }
    let focus = resources
        .get("actor_focus_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let run_budget = resources
        .get("run_budget_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let evidence_count = resources
        .get("evidence_assessment_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let run_count = resources
        .get("experiment_run_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let checkpoint_count = resources
        .get("captain_checkpoint_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let retained_failures = resources
        .get("retained_failure_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let render_blockers = |action: Option<&Value>| {
        let blockers = action
            .and_then(|value| value.get("blockers"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(projection_label)
            .map(|value| format!("<li>{}</li>", escape(value)))
            .collect::<String>();
        if blockers.is_empty() {
            "<li class=\"ready\">Available / 可执行</li>".to_string()
        } else {
            blockers
        }
    };

    let actor_action = match current_role {
        "evidence" => {
            let availability = role_resource_action(resources, "assess_evidence");
            let available = availability
                .and_then(|value| value.get("available"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let assessed = paper
                .get("role_resources")
                .and_then(|value| value.get("actions"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|action| {
                    action.get("kind").and_then(Value::as_str) == Some("evidence_assessment")
                })
                .filter_map(|action| action.get("subject_id").and_then(Value::as_str))
                .collect::<std::collections::HashSet<_>>();
            let options = room_records(room, "evidence_cards")
                .iter()
                .filter_map(|card| {
                    let id = card.get("evidence_card_id").and_then(Value::as_str)?;
                    if assessed.contains(id) {
                        return None;
                    }
                    let label = card
                        .get("source_uri")
                        .and_then(Value::as_str)
                        .or_else(|| card.get("locator").and_then(Value::as_str))
                        .unwrap_or("Evidence card");
                    Some(format!(
                        r#"<option value="{}">{}</option>"#,
                        escape(id),
                        escape(label),
                    ))
                })
                .collect::<String>();
            let form = if available && !options.is_empty() {
                format!(
                    r#"<form class="role-resource-action-form" data-paper-id="{}" data-resource-version="{}" data-action-kind="evidence_assessment"><label>Unassessed evidence / 未评估证据<select name="evidence_card_id" required><option value="">Choose evidence / 选择证据</option>{}</select></label><button type="submit">Spend 1 Evidence focus / 消耗 1 点证据专注</button><output></output></form>"#,
                    escape(paper_id),
                    state_version,
                    options,
                )
            } else {
                String::new()
            };
            format!(
                "<article class=\"action\"><h3>Evidence assessment / 证据评估</h3><ul>{}</ul>{}</article>",
                render_blockers(availability),
                form,
            )
        }
        "captain" => {
            let availability = role_resource_action(resources, "coordinate_checkpoint");
            let available = availability
                .and_then(|value| value.get("available"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let form = if available {
                format!(
                    r#"<form class="role-resource-action-form" data-paper-id="{}" data-resource-version="{}" data-action-kind="captain_checkpoint"><button type="submit">Spend 1 Captain focus / 消耗 1 点队长专注</button><output></output></form>"#,
                    escape(paper_id),
                    state_version,
                )
            } else {
                String::new()
            };
            format!(
                "<article class=\"action\"><h3>Team checkpoint / 团队检查点</h3><ul>{}</ul>{}</article>",
                render_blockers(availability),
                form,
            )
        }
        "experiment" => {
            let availability = role_resource_action(resources, "create_run_record");
            format!(
                "<article class=\"action\"><h3>Run budget / 运行预算</h3><p>Each retained run spends one shared run and one Experiment focus. A disclosed failed run may refund focus, never run budget.</p><ul>{}</ul></article>",
                render_blockers(availability),
            )
        }
        _ => String::new(),
    };

    format!(
        r#"<section class="panel role-resource-panel"><span class="eyebrow">NON-ECONOMIC GAMEPLAY / 非经济玩法</span><h2>Role resources / 角色资源</h2><div class="facts"><article><strong>{}</strong><span>Your focus / 你的专注</span></article><article><strong>{}</strong><span>Shared runs / 共享运行</span></article><article><strong>{}</strong><span>Evidence assessments / 证据评估</span></article><article><strong>{}</strong><span>Runs retained / 已保留运行</span></article><article><strong>{}</strong><span>Checkpoints / 检查点</span></article><article><strong>{}</strong><span>Failed runs rewarded / 失败运行正反馈</span></article></div><p class="muted">These resources only shape pacing and choices. Ranking, reward, score, and economic eligibility remain locked.</p>{}</section>"#,
        focus,
        run_budget,
        evidence_count,
        run_count,
        checkpoint_count,
        retained_failures,
        actor_action,
    )
}

fn projected_primary_action_code(progress: Option<&Value>) -> Option<&str> {
    for field in ["primary_actions", "next_actions"] {
        let Some(actions) = progress
            .and_then(|value| value.get(field))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for action in actions {
            let label = action.as_str().or_else(|| {
                action
                    .get("label")
                    .or_else(|| action.get("action"))
                    .or_else(|| action.get("command"))
                    .and_then(Value::as_str)
            });
            if let Some(label) = label {
                return Some(label);
            }
        }
    }
    None
}

fn paper_room_primary_action_selector(action: &str) -> Option<&'static str> {
    match action {
        "open_preregistration" | "transition_paper_project" => Some(".paper-phase-form"),
        "create_paper_work_item" => Some(".create-work-item-form"),
        "lock_research_plan" | "create_experiment_plan" => {
            Some(".input-manifest-wizard-form, .create-experiment-plan-form")
        }
        "bind_claims_to_evidence" | "create_claim_record" => Some(".create-claim-record-form"),
        "create_evidence_card" => Some(".create-evidence-card-form"),
        "create_citation_record" => Some(".create-citation-record-form"),
        "retain_results_and_artifacts" | "create_run_record" => {
            Some(".run-artifact-wizard-form, .create-run-record-form")
        }
        "assemble_draft" => Some(".draft-manifest-wizard-form, .create-paper-revision-form"),
        "register_artifact" => Some(".artifact-form"),
        "assess_evidence" => {
            Some(".role-resource-action-form[data-action-kind=evidence_assessment]")
        }
        "coordinate_checkpoint" => {
            Some(".role-resource-action-form[data-action-kind=captain_checkpoint]")
        }
        "create_section_revision" => Some(".create-section-revision-form"),
        "create_paper_revision" => Some(".create-paper-revision-form"),
        "submit_review" => Some(".section-review-form"),
        "merge_section" => Some(".merge-section-form"),
        "promote_paper_release_candidate" => Some(".promote-release-form"),
        "create_authorship_consent" => Some(".author-consent-form"),
        "finalize_joint_paper_submission" => Some(".finalize-paper-form"),
        "submit_appeal" => Some(".author-appeal-form"),
        _ => None,
    }
}

fn basic_paper_room(room: &Value, review: Option<&Value>) -> String {
    let Some(paper) = room.get("paper").filter(|value| value.is_object()) else {
        return unavailable("paper objective");
    };
    let progress = room
        .get("author_raid_progress")
        .filter(|value| value.is_object());
    let phase = progress
        .and_then(|value| value.get("phase"))
        .or_else(|| paper.get("phase"))
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let fallback_objective = phase_objective(phase);
    let objective = progress
        .and_then(|value| value.get("objective"))
        .and_then(Value::as_str)
        .map(projection_label)
        .unwrap_or(fallback_objective.0);
    let detail = progress
        .and_then(|value| value.get("personal_objective"))
        .and_then(Value::as_str)
        .map(projection_label)
        .unwrap_or(fallback_objective.1);
    let member_count = room
        .get("team")
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut blockers = progress_blockers(progress)
        .unwrap_or_else(|| phase_blockers(phase, room, review, member_count));
    if phase == "forming" && progress.is_none() {
        blockers.insert(
            0,
            "Authoritative actions are unavailable; no mutation is guessed from local state. / \
             权威操作暂不可用；不从本地状态猜测任何变更。"
                .into(),
        );
    }
    let (blocker_state, blocker) = blockers.first().map_or(
        (
            "ready",
            "Nothing blocks the next authoritative step. / 当前无权威阻塞项。",
        ),
        |blocker| ("blocked", blocker.as_str()),
    );
    let additional_blockers = blockers.len().saturating_sub(1);
    let additional = if additional_blockers == 0 {
        String::new()
    } else {
        format!(
            r#"<p class="paper-room-more-blockers">{} more blocking reason(s) are listed in Advanced / <span lang="zh-Hans">高级详情中还有 {} 项阻塞原因。</span></p>"#,
            additional_blockers, additional_blockers,
        )
    };
    let projected_action = projected_primary_action_code(progress);
    let (primary_action, primary_target) = if phase == "forming" && progress.is_none() {
        (
            "Review the authoritative status / 查看权威状态".to_string(),
            ".paper-room-authority",
        )
    } else {
        (
            projected_action
                .map(projection_label)
                .unwrap_or(objective)
                .to_string(),
            projected_action
                .and_then(paper_room_primary_action_selector)
                .unwrap_or(".primary-action"),
        )
    };
    format!(
        r#"<section class="panel paper-room-basic" aria-labelledby="paper-room-current-objective"><span class="eyebrow">BASIC / <span lang="zh-Hans">基础模式</span></span><h2 id="paper-room-current-objective">{}</h2><p class="paper-room-personal-objective">{}</p><div class="paper-room-basic-grid"><section class="paper-room-blocker" aria-labelledby="paper-room-blocker-heading"><h3 id="paper-room-blocker-heading">Blocking reason / <span lang="zh-Hans">阻塞原因</span></h3><p class="paper-room-blocker-reason" data-blocker-state="{}" role="status">{}</p>{}</section><section class="paper-room-next-step" aria-labelledby="paper-room-primary-action-heading"><h3 id="paper-room-primary-action-heading">Primary action / <span lang="zh-Hans">主要操作</span></h3><p id="paper-room-primary-action-label">{}</p><button class="paper-room-primary-button" type="button" data-paper-room-reveal data-paper-room-primary-target="{}" aria-controls="paper-room-advanced" aria-expanded="false" aria-describedby="paper-room-primary-action-label">Open this action / <span lang="zh-Hans">打开此操作</span></button></section></div></section>"#,
        player_language_html(objective),
        player_language_html(detail),
        blocker_state,
        player_language_html(blocker),
        additional,
        player_language_html(&primary_action),
        escape(primary_target),
    )
}

fn guided_paper_room(
    identity: &AlphaIdentity,
    paper_id: &str,
    room: &Value,
    review: Option<&Value>,
) -> String {
    let Some(paper) = room.get("paper").filter(|value| value.is_object()) else {
        return unavailable("paper objective");
    };
    let progress = room
        .get("author_raid_progress")
        .filter(|value| value.is_object());
    let phase = progress
        .and_then(|value| value.get("phase"))
        .or_else(|| paper.get("phase"))
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let player_phase = progress
        .and_then(|value| value.get("player_phase"))
        .and_then(Value::as_str)
        .filter(|value| {
            matches!(
                *value,
                "forming"
                    | "preregistering"
                    | "researching"
                    | "experimenting"
                    | "drafting"
                    | "integrity_review"
                    | "reproduction_readiness"
                    | "author_approval"
                    | "integrity_hold"
                    | "submission_ready"
            )
        })
        .unwrap_or(if phase == "reproducing" {
            "reproduction_readiness"
        } else {
            phase
        });
    let paper_version = paper.get("version").and_then(Value::as_u64).unwrap_or(0);
    let team = room.get("team").filter(|value| value.is_object());
    let team_version = team
        .and_then(|value| value.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let members = team
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let current_player_id = identity.player_id.to_string();
    let current_role = progress
        .and_then(|value| value.get("actor_role"))
        .and_then(Value::as_str)
        .or_else(|| {
            members
                .iter()
                .find(|member| {
                    member.get("player_id").and_then(Value::as_str)
                        == Some(current_player_id.as_str())
                })
                .and_then(|member| member.get("role"))
                .and_then(Value::as_str)
        })
        .unwrap_or("team_member");
    let canonical_roles_enforced = canonical_author_roles_enforced(members);
    let challenge_controls = challenge_ruleset_panel(paper_id, paper, current_role);
    let role_resource_controls = role_resource_panel(paper_id, paper, progress, room, current_role);
    let fallback_objective = phase_objective(phase);
    let objective = progress
        .and_then(|value| value.get("objective"))
        .and_then(Value::as_str)
        .map(projection_label)
        .unwrap_or(fallback_objective.0);
    let detail = progress
        .and_then(|value| value.get("personal_objective"))
        .and_then(Value::as_str)
        .map(projection_label)
        .unwrap_or(fallback_objective.1);
    let blockers = progress_blockers(progress)
        .unwrap_or_else(|| phase_blockers(phase, room, review, members.len()));
    let blocker_items = if blockers.is_empty() {
        "<li class=\"ready\">Current UI readiness checks pass / 当前界面就绪检查通过</li>"
            .to_string()
    } else {
        blockers
            .iter()
            .map(|blocker| format!("<li>{}</li>", escape(blocker)))
            .collect::<String>()
    };
    let forming_projection_missing = phase == "forming" && progress.is_none();
    let phase_controls = if forming_projection_missing {
        r#"<article class="guided-action blocked author-progress-unavailable"><h3>Authoritative actions unavailable / 权威操作暂不可用</h3><p>Hepta's Author Raid action projection is unavailable. The compatibility objective remains read-only; no phase, work-item, or session mutation is guessed from local state.</p></article>"#.to_string()
    } else {
        phase_transition_form(paper_id, paper_version, phase, progress)
    };
    let next_actions = progress_actions(progress);
    let session_controls = research_session_panel(
        paper_id,
        paper_version,
        team_version,
        members,
        room_records(room, "member_research_sessions"),
        phase == "preregistering",
        !forming_projection_missing && !matches!(phase, "submission_ready" | "integrity_hold"),
    );
    let work_controls = work_item_panel(WorkItemPanelInput {
        paper_id,
        paper_version,
        members,
        work_items: room_records(room, "work_items"),
        manifests: room_records(room, "artifact_manifests"),
        primary: matches!(phase, "researching" | "drafting"),
        editable: !forming_projection_missing
            && !matches!(phase, "submission_ready" | "integrity_hold"),
        current_player_id: &current_player_id,
        current_role,
        canonical_roles_enforced,
    });
    let science_controls = science_action_panel_for_role(
        paper_id,
        phase,
        room,
        current_role,
        canonical_roles_enforced,
    );
    let section_controls =
        section_collaboration_panel(paper_id, phase, room, members, &current_player_id);
    let revision_controls = revision_panel(RevisionPanelInput {
        paper_id,
        paper_version,
        paper,
        room,
        review,
        members,
        phase,
        current_player_id: &current_player_id,
    });
    let rework_controls = author_rework_panel(paper_id, phase, paper, room, review);
    let appeal_controls = author_appeal_panel(paper_id, phase, room, review);
    format!(
        r#"{}<section class="panel raid-command-center" data-paper-phase="{}" data-player-phase="{}"><div class="phase-objective"><span class="eyebrow">CURRENT OBJECTIVE / 当前目标</span><h2>{}</h2><p>{}</p><span class="pill">{} · {}</span>{}</div><div class="phase-readiness"><h3>Blockers / 阻塞项</h3><ul>{}</ul><p class="muted">Hepta's Author Raid projection is authoritative when present; local derivation is only a compatibility fallback.</p></div>{}</section>{}{}{}{}{}{}{}{}<section class="panel material-panel"><h2>Research Materials / 研究材料</h2><p>Upload immutable bytes here. A registered ArtifactManifest is still required before a revision can bind those bytes.</p><div class="action-grid">{}</div></section>"#,
        challenge_controls,
        escape(phase),
        escape(player_phase),
        escape(objective),
        escape(detail),
        escape(player_phase),
        escape(current_role),
        next_actions,
        blocker_items,
        phase_controls,
        role_resource_controls,
        session_controls,
        work_controls,
        science_controls,
        revision_controls,
        section_controls,
        rework_controls,
        appeal_controls,
        artifact_upload(paper_id),
    )
}

fn challenge_ruleset_panel(paper_id: &str, paper: &Value, current_role: &str) -> String {
    let outcome = paper
        .get("outcome")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let outcome_reason = paper
        .get("outcome_reason")
        .and_then(Value::as_str)
        .map(challenge_reason_label)
        .unwrap_or(match outcome {
            "in_progress" => "Raid is active / 挑战进行中",
            "submission_ready" => "Victory requirements satisfied / 胜利条件已满足",
            _ => "Terminal reason unavailable / 终局原因不可用",
        });
    let terminal_at = paper
        .get("terminal_at")
        .and_then(Value::as_str)
        .map(|value| {
            format!(
                r#"<p class="muted">Terminal at / 终局时间: <time datetime="{}">{}</time></p>"#,
                escape(value),
                escape(value),
            )
        })
        .unwrap_or_default();
    let outcome_card = format!(
        r#"<div class="challenge-outcome" data-challenge-outcome="{}"><span class="eyebrow">OUTCOME / 结果</span><strong>{}</strong><p>{}</p>{}</div>"#,
        escape(outcome),
        escape(challenge_outcome_label(outcome)),
        escape(outcome_reason),
        terminal_at,
    );
    let deadline = paper
        .get("deadline_at")
        .and_then(Value::as_str)
        .and_then(parse_utc_time);
    let grace = paper
        .get("grace_expires_at")
        .and_then(Value::as_str)
        .and_then(parse_utc_time);
    let timing = match (deadline, grace) {
        (Some(deadline), Some(grace)) if grace >= deadline => Some((deadline, grace)),
        _ => None,
    };
    let clock = challenge_clock(
        timing.map(|(deadline, _)| deadline),
        timing.map(|(_, grace)| grace),
    );
    let terminal_controls = challenge_terminal_controls(
        paper_id,
        outcome,
        current_role,
        timing.map(|(_, grace)| grace),
    );
    let Some(snapshot) = paper
        .get("challenge_ruleset_snapshot")
        .filter(|value| value.is_object())
    else {
        return format!(
            r#"<section class="panel challenge-ruleset-panel legacy-ruleset" data-ruleset-enforcement="legacy_unranked"><div><span class="pill">LEGACY · UNRANKED</span><h2>Challenge Ruleset / 挑战规则</h2><p>This Paper predates the immutable typed ruleset. It stays readable, but no ranked, reward, score, or economic claim may be inferred.</p></div>{}{}{}{}</section>"#,
            clock,
            outcome_card,
            terminal_controls,
            if outcome == "unavailable" {
                r#"<p class="status missing">Challenge outcome unavailable / 挑战结果不可用</p>"#
            } else {
                ""
            },
        );
    };
    let enforcement = snapshot
        .get("enforcement")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let ruleset_version = snapshot
        .get("ruleset_version")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let Some(ruleset) = snapshot.get("ruleset").filter(|value| value.is_object()) else {
        return format!(
            r#"<section class="panel challenge-ruleset-panel unavailable" data-ruleset-enforcement="{}"><span class="status missing">RULESET UNAVAILABLE</span><h2>Challenge Ruleset / 挑战规则</h2><p>The immutable snapshot does not expose a typed ruleset. Gameplay gates must remain fail-closed.</p>{}{}{}</section>"#,
            escape(enforcement),
            clock,
            outcome_card,
            terminal_controls,
        );
    };
    let template = ruleset
        .get("template")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let duration_seconds = ruleset.get("duration_seconds").and_then(Value::as_u64);
    let grace_seconds = ruleset.get("grace_seconds").and_then(Value::as_u64);
    let victory = challenge_requirements(
        ruleset
            .get("victory_requirements")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    let phase_gates = challenge_phase_gates(
        ruleset
            .get("phase_gates")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    let duration = duration_seconds
        .map(game_duration_label)
        .unwrap_or_else(|| "Unavailable / 不可用".into());
    let grace_label = grace_seconds
        .map(game_duration_label)
        .unwrap_or_else(|| "Unavailable / 不可用".into());
    let (difficulty, gameplay_details) = match authoritative_gameplay_view(ruleset) {
        Ok(Some(gameplay)) => (
            challenge_typed_difficulty_label(&gameplay.difficulty).to_string(),
            format!(
                r#"<dl class="challenge-rules authoritative-gameplay" data-gameplay-source="authoritative_typed">{}</dl>"#,
                authoritative_gameplay_details(&gameplay),
            ),
        ),
        Ok(None) => (
            challenge_difficulty_label(template).to_string(),
            String::new(),
        ),
        Err(()) => (
            "Unavailable / 不可用".into(),
            format!(
                r#"<p class="status missing" data-gameplay-source="invalid_authoritative">Authoritative gameplay metadata is invalid. {} </p>"#,
                escape(NON_ECONOMIC_PROGRESSION_NOTICE),
            ),
        ),
    };
    format!(
        r#"<section class="panel challenge-ruleset-panel" data-ruleset-enforcement="{}" data-challenge-template="{}"><div class="challenge-ruleset-header"><div><span class="pill">AUTHORITATIVE RULESET</span><h2>Challenge Ruleset / 挑战规则</h2><p>The immutable Paper snapshot—not lobby copy or browser state—defines this Raid.</p></div><dl class="challenge-ruleset-facts"><div><dt>Template / 模式</dt><dd>{}</dd></div><div><dt>Difficulty / 难度</dt><dd>{}</dd></div><div><dt>Ruleset / 规则版本</dt><dd>{}</dd></div><div><dt>Raid clock / 挑战时长</dt><dd>{}</dd></div><div><dt>Grace / 宽限</dt><dd>{}</dd></div></dl></div>{}{}<div class="challenge-victory"><h3>Victory conditions / 胜利条件</h3><ul>{}</ul></div><details class="challenge-phase-gates"><summary>All authoritative phase gates / 全部权威阶段门</summary><ol>{}</ol></details>{}{}{}</section>"#,
        escape(enforcement),
        escape(template),
        escape(challenge_template_label(template)),
        escape(&difficulty),
        escape(ruleset_version),
        escape(&duration),
        escape(&grace_label),
        gameplay_details,
        clock,
        victory,
        phase_gates,
        outcome_card,
        terminal_controls,
        if enforcement == "authoritative_v1" {
            ""
        } else {
            r#"<p class="status missing">Legacy/unavailable enforcement: competitive eligibility stays locked.</p>"#
        },
    )
}

fn parse_utc_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn game_duration_label(seconds: u64) -> String {
    if seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn challenge_clock(deadline: Option<DateTime<Utc>>, grace: Option<DateTime<Utc>>) -> String {
    let (Some(deadline), Some(grace)) = (deadline, grace) else {
        return r#"<div class="challenge-clock unavailable"><strong>Authoritative clock unavailable / 权威计时不可用</strong><p>No expiry action can be inferred.</p></div>"#.into();
    };
    let now = Utc::now();
    let initial = if now < deadline {
        format!(
            "{} remaining / 剩余 {}",
            game_duration_label((deadline - now).num_seconds().max(0) as u64),
            game_duration_label((deadline - now).num_seconds().max(0) as u64),
        )
    } else if now < grace {
        format!(
            "Overtime · {} grace remaining / 加时 · 宽限剩余 {}",
            game_duration_label((grace - now).num_seconds().max(0) as u64),
            game_duration_label((grace - now).num_seconds().max(0) as u64),
        )
    } else {
        "Grace elapsed · canonical expiry is materialized on authorized sync / 宽限已结束，授权同步将自动物化超时终局".into()
    };
    let deadline_text = deadline.to_rfc3339();
    let grace_text = grace.to_rfc3339();
    format!(
        r#"<div class="challenge-clock"><span class="eyebrow">AUTHORITATIVE CLOCK / 权威计时</span><strong class="challenge-countdown" data-deadline-at="{}" data-grace-expires-at="{}">{}</strong><p><time datetime="{}">Deadline / 截止: {}</time><br><time datetime="{}">Grace ends / 宽限结束: {}</time></p></div>"#,
        escape(&deadline_text),
        escape(&grace_text),
        escape(&initial),
        escape(&deadline_text),
        escape(&deadline_text),
        escape(&grace_text),
        escape(&grace_text),
    )
}

fn challenge_terminal_controls(
    paper_id: &str,
    outcome: &str,
    current_role: &str,
    _grace: Option<DateTime<Utc>>,
) -> String {
    if outcome != "in_progress" {
        return String::new();
    }
    if current_role != "captain" {
        return r#"<p class="muted challenge-terminal-owner">Only the Captain can record a terminal challenge outcome / 仅队长可登记挑战终局。</p>"#.into();
    }
    let failed = format!(
        r#"<form class="challenge-outcome-form" data-paper-id="{}" data-confirm-message="Record this Raid as failed? This terminal fact is immutable."><input type="hidden" name="outcome" value="failed"><label>Failure reason / 失败原因<select name="reason_code" required><option value="quality_gate_failed">Quality gate could not be satisfied / 无法满足质量门</option><option value="preregistered_result_failed">Preregistered result failed / 预注册结果失败</option><option value="integrity_failure">Integrity failure / 完整性失败</option></select></label><button class="danger" type="submit">Record failed / 登记失败</button><output></output></form>"#,
        escape(paper_id),
    );
    let abandoned = format!(
        r#"<form class="challenge-outcome-form" data-paper-id="{}" data-confirm-message="Abandon this Raid? This terminal fact is immutable."><input type="hidden" name="outcome" value="abandoned"><label>Abandon reason / 放弃原因<select name="reason_code" required><option value="team_withdrawal">Team withdrawal / 团队退出</option><option value="resource_unavailable">Required resource unavailable / 必需资源不可用</option><option value="challenge_infeasible">Challenge proved infeasible / 挑战不可行</option></select></label><button class="danger" type="submit">Record abandoned / 登记放弃</button><output></output></form>"#,
        escape(paper_id),
    );
    format!(
        r#"<details class="challenge-terminal-controls"><summary>End this Raid / 结束本次远征</summary><p class="muted">Use only when continuing is no longer scientifically valid. Hepta makes the selected outcome immutable. Deadline expiry is canonical and materializes automatically on the next authorized Room/state/event read; it never depends on the Captain.</p><div class="challenge-terminal-grid">{}{}</div></details>"#,
        failed, abandoned,
    )
}

fn challenge_requirements(requirements: &[Value]) -> String {
    if requirements.is_empty() {
        return "<li>Unavailable / 不可用</li>".into();
    }
    requirements
        .iter()
        .filter_map(|requirement| {
            let kind = requirement.get("kind").and_then(Value::as_str)?;
            let minimum = requirement.get("minimum").and_then(Value::as_u64)?;
            Some(format!(
                "<li><strong>{minimum}×</strong> {}</li>",
                escape(challenge_requirement_label(kind)),
            ))
        })
        .collect()
}

fn challenge_phase_gates(gates: &[Value]) -> String {
    if gates.is_empty() {
        return "<li>Unavailable / 不可用</li>".into();
    }
    gates
        .iter()
        .filter_map(|gate| {
            let transition = gate.get("transition").and_then(Value::as_str)?;
            let requirements = gate
                .get("requirements")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            Some(format!(
                "<li><strong>{}</strong><ul>{}</ul></li>",
                escape(challenge_transition_label(transition)),
                challenge_requirements(requirements),
            ))
        })
        .collect()
}

fn challenge_template_label(template: &str) -> &'static str {
    match template {
        "benchmark-ablation" => "Benchmark + Ablation / 基准与消融",
        "replication" => "Independent Replication / 独立复现",
        "evidence-audit" => "Evidence Audit / 证据审计",
        _ => "Custom / 自定义",
    }
}

fn challenge_difficulty_label(template: &str) -> &'static str {
    match template {
        "benchmark-ablation" => "Intermediate / 中阶",
        "replication" => "Advanced / 高阶",
        "evidence-audit" => "Introductory / 入门",
        _ => "Ruleset-defined / 由规则决定",
    }
}

fn challenge_typed_difficulty_label(difficulty: &str) -> &'static str {
    match difficulty {
        "introductory" => "Introductory / 入门",
        "intermediate" => "Intermediate / 中阶",
        "advanced" => "Advanced / 高阶",
        _ => "Unavailable / 不可用",
    }
}

fn challenge_outcome_label(outcome: &str) -> &'static str {
    match outcome {
        "in_progress" => "In progress / 进行中",
        "submission_ready" => "Victory: submission ready / 胜利：提交就绪",
        "failed" => "Failed / 失败",
        "expired" => "Expired / 超时",
        "abandoned" => "Abandoned / 已放弃",
        _ => "Unavailable / 不可用",
    }
}

fn challenge_reason_label(reason: &str) -> &str {
    match reason {
        "quality_gate_failed" => "Quality gate could not be satisfied / 无法满足质量门",
        "preregistered_result_failed" => "Preregistered result failed / 预注册结果失败",
        "integrity_failure" => "Integrity failure / 完整性失败",
        "team_withdrawal" => "Team withdrawal / 团队退出",
        "resource_unavailable" => "Required resource unavailable / 必需资源不可用",
        "challenge_infeasible" => "Challenge proved infeasible / 挑战不可行",
        "challenge_grace_deadline_elapsed" => {
            "Authoritative challenge grace deadline elapsed / 权威挑战宽限截止时间已过"
        }
        _ => reason,
    }
}

fn challenge_transition_label(transition: &str) -> &'static str {
    match transition {
        "preregistering_to_researching" => "Preregistration → Research / 预注册 → 研究",
        "researching_to_experimenting" => "Research → Experiment / 研究 → 实验",
        "experimenting_to_drafting" => "Experiment → Draft / 实验 → 起草",
        "drafting_to_integrity_review" => "Draft → Integrity review / 起草 → 完整性审查",
        "integrity_review_to_reproducing" => {
            "Integrity review → Reproduction readiness / 完整性审查 → 复现准备"
        }
        "reproducing_to_author_approval" => {
            "Reproduction readiness → Author approval / 复现准备 → 作者批准"
        }
        _ => "Unknown transition / 未知迁移",
    }
}

fn challenge_requirement_label(kind: &str) -> &'static str {
    match kind {
        "work_items" => "planned work items / 计划任务",
        "accepted_work_items" => "accepted work items / 已验收任务",
        "artifact_manifests" => "artifact manifests / 工件清单",
        "evidence_cards" => "verified evidence cards / 已验证证据卡",
        "citations" => "canonical citations / 规范引用",
        "experiment_plans" => "preregistered experiment plans / 预注册实验计划",
        "retained_runs" => "retained runs / 已保留运行",
        "successful_runs" => "successful runs / 成功运行",
        "retained_failed_runs" => "retained failed runs / 已保留失败运行",
        "claims" => "evidence-bound claims / 证据绑定论断",
        "section_revisions" => "section revisions / 章节版本",
        "approving_section_reviews" => "approving section reviews / 通过的章节审查",
        "section_merges" => "accepted section merges / 已接受章节合并",
        "paper_revisions" => "frozen Paper revisions / 锁定论文版本",
        "all_work_items_terminal" => "all work items resolved / 所有任务已结清",
        "paper_revision_covers_section_merges" => {
            "Paper covers every merged section / 论文覆盖全部合并章节"
        }
        "release_candidate" => "promoted release candidate / 已提升发布候选",
        "all_author_consents" => "all author consents / 全部作者同意",
        _ => "unknown authoritative requirement / 未知权威要求",
    }
}

fn author_rework_panel(
    paper_id: &str,
    phase: &str,
    paper: &Value,
    room: &Value,
    review: Option<&Value>,
) -> String {
    let active = match (
        paper.get("active_rework_id"),
        paper.get("active_rework_cycle"),
        paper.get("rework_expires_at"),
    ) {
        (Some(Value::Null), Some(Value::Null), Some(Value::Null)) => None,
        (Some(rework_id), Some(cycle), Some(expires_at)) => {
            let Some(_rework_id) = canonical_uuid_value(rework_id) else {
                return unavailable_card(
                    "Rework state unavailable / 返工状态不可用",
                    "one canonical active rework lease",
                );
            };
            let Some(cycle) = cycle.as_u64().filter(|cycle| *cycle >= 2) else {
                return unavailable_card(
                    "Rework state unavailable / 返工状态不可用",
                    "one canonical active rework lease",
                );
            };
            let Some(expires_at) = expires_at.as_str() else {
                return unavailable_card(
                    "Rework state unavailable / 返工状态不可用",
                    "one canonical active rework lease",
                );
            };
            let Some(deadline) = DateTime::parse_from_rfc3339(expires_at)
                .ok()
                .map(|value| value.with_timezone(&Utc))
            else {
                return unavailable_card(
                    "Rework state unavailable / 返工状态不可用",
                    "one canonical active rework lease",
                );
            };
            Some((cycle, expires_at, deadline))
        }
        _ => {
            return unavailable_card(
                "Rework state unavailable / 返工状态不可用",
                "explicit inactive fields or one canonical active rework lease",
            )
        }
    };

    if let Some((cycle, expires_at, deadline)) = active {
        let expired = deadline <= Utc::now();
        let state = if expired { "expired" } else { "active" };
        let pill = if expired {
            "REWORK LEASE EXPIRED"
        } else {
            "REWORK ACTIVE"
        };
        let heading = if expired {
            "This replacement window has expired / 本次返工窗口已过期"
        } else {
            "Revise the rejected Paper / 修订被拒论文"
        };
        let detail = if expired {
            "Author mutations are disabled. Reload after the server resolves the expired lease; the browser will not invent a replacement deadline."
        } else {
            "The prior submission and Review remain immutable. Reuse the normal section, revision, promotion, three-consent, and finalize controls below to create a new PaperBundle."
        };
        return format!(
            r#"<section class="panel author-rework-lease" data-paper-id="{}" data-rework-state="{}" data-rework-expires-at="{}"><span class="pill">{}</span><h2>{}</h2><div class="facts"><article><strong>{}</strong><span>Rework cycle / 返工轮次</span></article><article><strong class="paper-rework-countdown" aria-live="polite">{}</strong><span>Time remaining / 剩余时间</span></article></div><p>{}</p><p class="muted">Server deadline: <time datetime="{}">{}</time></p><a class="button continue-raid-link" data-paper-id="{}" href="/league/papers/{}">Continue rework / 继续返工</a></section>"#,
            escape(paper_id),
            state,
            escape(expires_at),
            pill,
            heading,
            cycle,
            if expired {
                "Expired / 已过期"
            } else {
                "Loading…"
            },
            detail,
            escape(expires_at),
            escape(expires_at),
            escape(paper_id),
            escape(paper_id),
        );
    }

    if phase != "submission_ready" {
        return String::new();
    }
    let Some(review) = review else {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "current immutable Review state",
        );
    };
    let Some(submission) = room
        .get("joint_submission")
        .filter(|value| value.is_object())
    else {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "current submission-ready PaperBundle",
        );
    };
    let Some(submission_id) = submission
        .get("submission_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "current submission-ready PaperBundle",
        );
    };
    if submission.get("paper_project_id").and_then(Value::as_str) != Some(paper_id)
        || submission.get("status").and_then(Value::as_str) != Some("submission_ready")
    {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "current submission-ready PaperBundle",
        );
    }
    let Some(evaluations) = review.get("evaluations").and_then(Value::as_array) else {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "current immutable Review state",
        );
    };
    let submission_id = submission_id.to_string();
    let mut current = evaluations
        .iter()
        .filter_map(|evaluation| {
            if evaluation.get("paper_project_id").and_then(Value::as_str) != Some(paper_id)
                || evaluation.get("submission_id").and_then(Value::as_str)
                    != Some(submission_id.as_str())
            {
                return None;
            }
            let version = evaluation.get("version").and_then(Value::as_u64)?;
            let evaluation_id = evaluation
                .get("evaluation_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())?;
            Some((version, evaluation_id, evaluation))
        })
        .collect::<Vec<_>>();
    current.sort_by_key(|(version, evaluation_id, _)| (*version, *evaluation_id));
    let Some((_, evaluation_id, evaluation)) = current.last().copied() else {
        return String::new();
    };
    if evaluation.get("status").and_then(Value::as_str) != Some("rejected") {
        return String::new();
    }
    let evaluation_id_text = evaluation_id.to_string();
    if current.iter().any(|(_, _, candidate)| {
        candidate
            .get("supersedes_evaluation_id")
            .and_then(Value::as_str)
            == Some(evaluation_id_text.as_str())
    }) {
        return unavailable_card(
            "Rework decision unavailable / 返工决策不可用",
            "latest unsuperseded rejected evaluation",
        );
    }

    let appeals = review
        .get("appeals")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let resolutions = review
        .get("resolutions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let open_appeal = appeals.iter().any(|appeal| {
        appeal.get("evaluation_id").and_then(Value::as_str) == Some(evaluation_id_text.as_str())
            && appeal
                .get("appeal_id")
                .and_then(Value::as_str)
                .is_some_and(|appeal_id| {
                    !resolutions.iter().any(|resolution| {
                        resolution.get("appeal_id").and_then(Value::as_str) == Some(appeal_id)
                    })
                })
    });
    if open_appeal {
        return r#"<section class="panel author-rework-status"><span class="pill">REWORK ON HOLD</span><h2>Resolve the open Appeal first / 请先完成申诉裁决</h2><p>The rejected Paper stays immutable while an independent resolver decides the open Appeal.</p></section>"#.into();
    }

    let failed_gates = evaluation
        .get("paper_score")
        .and_then(|value| value.get("hard_gates"))
        .and_then(Value::as_object)
        .map(|gates| {
            [
                (
                    "citations_and_data_authentic",
                    "Citations and data authenticity / 引用与数据真实性",
                ),
                (
                    "failed_runs_disclosed",
                    "Failed-run disclosure / 失败运行披露",
                ),
                ("all_authors_consented", "All-author consent / 全体作者同意"),
                (
                    "core_claims_have_evidence",
                    "Evidence for core claims / 核心论断证据",
                ),
                (
                    "artifact_lineage_complete",
                    "Complete artifact lineage / 完整工件谱系",
                ),
                (
                    "license_ethics_coi_complete",
                    "License, ethics, and COI / 许可、伦理与利益冲突",
                ),
            ]
            .into_iter()
            .filter(|(key, _)| gates.get(*key).and_then(Value::as_bool) == Some(false))
            .map(|(_, label)| format!("<li>{}</li>", escape(label)))
            .collect::<String>()
        })
        .unwrap_or_default();
    let reasons = if failed_gates.is_empty() {
        "<li>The independent panel did not accept this release under the frozen score and reviewer quorum / 独立评审未按冻结评分与双评审条件接受本版本</li>".to_string()
    } else {
        failed_gates
    };
    format!(
        r#"<section class="panel author-rework-start"><span class="pill">REVIEW REJECTED</span><h2>Choose a replacement path / 选择返工路径</h2><p>The rejected submission, Review, and receipts remain immutable. Address the visible findings in a new revision; the browser hashes your plain-language intent locally and your current human key signs the server-derived lineage.</p><h3>Review findings / 评审问题</h3><ul class="review-findings">{}</ul><form class="author-rework-start-form" data-paper-id="{}"><label>Rework intent / 返工说明<textarea name="reason" rows="5" minlength="10" maxlength="4000" required placeholder="Describe what the team will change and how it addresses the rejected Review."></textarea></label><button type="submit">Sign and start rework / 签名开始返工</button><output></output></form></section>"#,
        reasons,
        escape(paper_id),
    )
}

fn author_appeal_panel(
    paper_id: &str,
    phase: &str,
    room: &Value,
    review: Option<&Value>,
) -> String {
    if phase != "submission_ready" {
        return String::new();
    }
    let Some(review) = review else {
        return String::new();
    };
    if review
        .get("finality")
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        != Some("pending_finality")
    {
        return String::new();
    }
    let Some(evaluations) = review.get("evaluations").and_then(Value::as_array) else {
        return String::new();
    };
    let latest_version = evaluations
        .iter()
        .filter(|evaluation| {
            evaluation.get("paper_project_id").and_then(Value::as_str) == Some(paper_id)
        })
        .filter_map(|evaluation| evaluation.get("version").and_then(Value::as_u64))
        .max();
    let Some(latest_version) = latest_version else {
        return String::new();
    };
    let current = evaluations
        .iter()
        .filter(|evaluation| {
            evaluation.get("paper_project_id").and_then(Value::as_str) == Some(paper_id)
                && evaluation.get("version").and_then(Value::as_u64) == Some(latest_version)
                && evaluation
                    .get("evaluation_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some()
                && evaluation
                    .get("release_candidate_hash")
                    .and_then(Value::as_str)
                    .is_some()
        })
        .collect::<Vec<_>>();
    if current.len() != 1 {
        return unavailable_card(
            "Appeal state ambiguous / 申诉状态不明确",
            "one current Paper evaluation",
        );
    }
    let evaluation_id = current[0]
        .get("evaluation_id")
        .and_then(Value::as_str)
        .expect("filtered evaluation ID");
    let Some(appeals) = review.get("appeals").and_then(Value::as_array) else {
        return String::new();
    };
    let Some(resolutions) = review.get("resolutions").and_then(Value::as_array) else {
        return String::new();
    };
    let matching_appeals = appeals
        .iter()
        .filter(|appeal| appeal.get("evaluation_id").and_then(Value::as_str) == Some(evaluation_id))
        .collect::<Vec<_>>();
    if matching_appeals.len() > 1 {
        return unavailable_card(
            "Appeal state ambiguous / 申诉状态不明确",
            "one immutable Appeal per evaluation",
        );
    }
    if let Some(appeal) = matching_appeals.first() {
        let appeal_id = appeal.get("appeal_id").and_then(Value::as_str);
        let resolved = appeal_id.is_some_and(|appeal_id| {
            resolutions.iter().any(|resolution| {
                resolution.get("appeal_id").and_then(Value::as_str) == Some(appeal_id)
            })
        });
        return if resolved {
            r#"<section class="panel author-appeal-status"><span class="pill">APPEAL RESOLVED</span><h2>Independent resolution recorded / 独立裁决已记录</h2><p>The immutable review state restores the decision after reload or a lost browser response. A second Appeal cannot be opened against the same evaluation.</p></section>"#.into()
        } else {
            r#"<section class="panel author-appeal-status"><span class="pill">APPEAL OPEN</span><h2>Scientific finality is on hold / 科学终局暂缓</h2><p>Your locally signed Appeal and selected evidence manifest are authoritative. An independent resolver must decide it before finality can proceed.</p></section>"#.into()
        };
    }

    let options = room_records(room, "artifact_manifests")
        .iter()
        .filter_map(|manifest| {
            let manifest_id = manifest
                .get("manifest_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())?;
            let manifest_hash = manifest.get("manifest_hash").and_then(Value::as_str)?;
            if !manifest_hash.starts_with("sha256:") {
                return None;
            }
            let label = manifest
                .get("source_bundle_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("registered evidence bundle");
            let object_count = manifest
                .get("object_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            Some(format!(
                r#"<option value="{}">{} · {} object(s)</option>"#,
                escape(&manifest_id.to_string()),
                escape(label),
                object_count,
            ))
        })
        .collect::<String>();
    if options.is_empty() {
        return r#"<section class="panel author-appeal-status"><span class="pill">APPEAL EVIDENCE REQUIRED</span><h2>Register an evidence manifest first / 请先登记证据清单</h2><p>The normal Appeal path only accepts an existing Paper-scoped ArtifactManifest selected from the authoritative room state.</p></section>"#.into();
    }
    format!(
        r#"<section class="panel"><span class="pill">AUTHOR APPEAL</span><h2>Challenge the current evaluation / 质疑当前评估</h2><p>Explain the scientific grounds in plain language and select an already registered evidence bundle. The browser hashes your statement locally; the BFF derives the current evaluation, release hash, and exact manifest hash from Hepta.</p><form class="author-appeal-form" data-paper-id="{}"><label>Scientific grounds / 科学依据<textarea name="grounds" rows="5" minlength="10" maxlength="8000" required></textarea></label><label>Supporting evidence bundle / 支持证据清单<select name="evidence_manifest_id" required>{}</select></label><button type="submit">Sign and open Appeal / 签名并发起申诉</button><output></output></form></section>"#,
        escape(paper_id),
        options,
    )
}

fn room_records<'a>(room: &'a Value, field: &str) -> &'a [Value] {
    room.get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn phase_objective(phase: &str) -> (&'static str, &'static str) {
    match phase {
        "forming" => (
            "Open preregistration",
            "Freeze the question and protocol before collecting evidence or running experiments.",
        ),
        "preregistering" => (
            "Prepare the expedition",
            "Create the work plan and authorize the live research session, then begin evidence work.",
        ),
        "researching" => (
            "Build defensible evidence",
            "Assign work, preserve source lineage, and turn the research question into testable claims.",
        ),
        "experimenting" => (
            "Run the preregistered protocol",
            "Keep successful, failed, and cancelled runs before moving into drafting.",
        ),
        "drafting" => (
            "Create the frozen draft",
            "Finish assigned work and bind one revision to its exact artifact manifest.",
        ),
        "integrity_review" => (
            "Challenge the draft",
            "Resolve weak claims and independent review findings before reproduction.",
        ),
        "reproducing" => (
            "Confirm reproduction readiness",
            "Freeze complete methods, artifacts, and run lineage for the separate Review Raid; Authors do not perform the independent reproduction here.",
        ),
        "author_approval" => (
            "Approve one exact release candidate",
            "Promote the frozen revision, collect every author signature, then finalize the PaperBundle.",
        ),
        "integrity_hold" => (
            "Resolve the integrity hold",
            "Keep the disputed evidence visible and return the project to drafting for correction.",
        ),
        "submission_ready" => (
            "Author Raid complete",
            "The authoring team produced a frozen submission. Independent review and finality remain separate gates.",
        ),
        _ => (
            "Wait for authoritative state",
            "The Paper phase is unavailable; no mutation should be guessed from stale browser state.",
        ),
    }
}

fn projection_label(value: &str) -> &str {
    if value.starts_with("paper_challenge_terminal:") {
        return "The challenge is terminal / 挑战已进入终局";
    }
    if value.starts_with("run_phase_disallows_action:") {
        return "Run resources are unavailable in this phase / 当前阶段不可使用运行资源";
    }
    match value {
        "open_preregistration" => "Open preregistration / 开启预注册",
        "lock_research_plan" => "Lock the research plan / 锁定研究计划",
        "bind_claims_to_evidence" => "Bind claims to evidence / 建立论断证据链",
        "retain_results_and_artifacts" => "Retain every result and artifact / 保留全部结果与工件",
        "assemble_draft" => "Assemble the frozen draft / 组装锁定草稿",
        "resolve_integrity_review" => "Resolve integrity review / 解决完整性审查",
        "confirm_reproduction_readiness" => "Confirm reproduction readiness / 确认复现就绪",
        "collect_author_approval" => "Collect exact author approval / 收集精确作者批准",
        "resolve_integrity_hold" => "Resolve the integrity hold / 解决完整性挂起",
        "author_raid_complete" => "Author Raid complete / 作者远征完成",
        "coordinate_team_and_phase_gate" => {
            "Captain: coordinate team tasks and phase gates / 队长：协调任务与阶段门"
        }
        "protect_claim_and_source_quality" => {
            "Evidence: protect claims and source quality / 证据：保护论断与来源质量"
        }
        "own_preregistration_and_reproducibility" => {
            "Experiment: own preregistration and reproducibility / 实验：负责预注册与可复现性"
        }
        "support_current_phase" => "Support the current phase / 支持当前阶段",
        "work_item_required" => "Create at least one owned work item / 至少创建一项有负责人的任务",
        "work_items_must_be_accepted_or_cancelled" => {
            "Resolve every work item to accepted or cancelled / 将所有任务结清为已验收或已取消"
        }
        "experiment_plan_required" => "Freeze an experiment plan / 锁定实验计划",
        "evidence_card_required" => "Register a verified evidence card / 登记已验证证据卡",
        "citation_required" => "Bind a canonical citation / 绑定规范引用",
        "claim_required" => "Bind a claim to evidence / 将论断绑定到证据",
        "retained_run_required" => {
            "Retain at least one run, including failures / 保留至少一次运行（含失败）"
        }
        "artifact_manifest_required" => {
            "Register the canonical ArtifactManifest / 登记规范工件清单"
        }
        "section_revision_required" => "Create a reviewed section revision / 创建已审章节版本",
        "paper_revision_required" => "Freeze the paper revision / 锁定论文版本",
        "section_review_required" => "Record an independent section review / 记录独立章节审查",
        "approving_section_review_required" => {
            "Record at least one approving section review / 至少记录一项通过的章节审查"
        }
        "section_merge_required" => "Merge the accepted section / 合并已验收章节",
        "release_candidate_required" => "Promote the exact release candidate / 提升精确发布候选",
        "all_author_consents_required" => "Collect every author signature / 收集全部作者签名",
        "integrity_hold_open" => "An integrity hold remains open / 完整性挂起尚未解决",
        "transition_paper_project" => "Advance the phase checkpoint / 推进阶段检查点",
        "create_paper_work_item" => "Create a team task / 创建团队任务",
        "create_experiment_plan" => "Freeze the experiment plan / 锁定实验计划",
        "create_evidence_card" => "Register verified evidence / 登记已验证证据",
        "create_citation_record" => "Bind a canonical citation / 绑定规范引用",
        "create_claim_record" => "Bind a claim to evidence / 绑定论断与证据",
        "create_run_record" => "Retain an experiment run / 保留实验运行",
        "assess_evidence" => {
            "Spend Evidence focus on one unassessed source / 消耗证据专注评估一项新来源"
        }
        "coordinate_checkpoint" => {
            "Spend Captain focus after fresh evidence and run work / 完成新证据与运行后消耗队长专注"
        }
        "evidence_role_required" => "Evidence role required / 需要证据角色",
        "experiment_role_required" => "Experiment role required / 需要实验角色",
        "captain_role_required" => "Captain role required / 需要队长角色",
        "evidence_focus_exhausted" => "Evidence focus exhausted / 证据专注已耗尽",
        "experiment_focus_exhausted" => "Experiment focus exhausted / 实验专注已耗尽",
        "captain_focus_exhausted" => "Captain focus exhausted / 队长专注已耗尽",
        "run_budget_exhausted" => "Shared run budget exhausted / 共享运行预算已耗尽",
        "unassessed_evidence_card_required" => {
            "A new unassessed evidence card is required / 需要一张尚未评估的新证据卡"
        }
        "new_evidence_assessment_required" => {
            "A fresh Evidence assessment is required / 需要新的证据评估"
        }
        "new_experiment_run_required" => "A fresh Experiment run is required / 需要新的实验运行",
        "role_resource_action_phase_blocked" => {
            "This resource action is unavailable in the current phase / 当前阶段不可执行此资源动作"
        }
        "paper_challenge_terminal" => "The challenge is terminal / 挑战已进入终局",
        "paper_challenge_deadline_elapsed" => {
            "The challenge grace deadline elapsed / 挑战宽限期已结束"
        }
        "register_artifact" => "Register the artifact manifest / 登记工件清单",
        "create_figure_lineage" => "Bind figure lineage / 绑定图表谱系",
        "create_section_revision" => "Create a section revision / 创建章节版本",
        "create_paper_revision" => "Freeze the paper revision / 锁定论文版本",
        "submit_review" => "Submit independent review / 提交独立审查",
        "merge_section" => "Merge the accepted section / 合并已验收章节",
        "promote_paper_release_candidate" => "Promote the release candidate / 提升发布候选",
        "create_authorship_consent" => "Sign the exact release / 签署精确发布",
        "finalize_joint_paper_submission" => "Finalize the PaperBundle / 完成论文包",
        "submit_appeal" => "Submit integrity appeal / 提交完整性申诉",
        _ => value,
    }
}

fn progress_blockers(progress: Option<&Value>) -> Option<Vec<String>> {
    let values = progress?.get("blockers")?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(|value| {
                value
                    .as_str()
                    .map(projection_label)
                    .map(str::to_string)
                    .or_else(|| {
                        value
                            .get("message")
                            .or_else(|| value.get("label"))
                            .or_else(|| value.get("code"))
                            .and_then(Value::as_str)
                            .map(projection_label)
                            .map(str::to_string)
                    })
            })
            .collect(),
    )
}

fn progress_actions(progress: Option<&Value>) -> String {
    let mut actions: Vec<&str> = Vec::new();
    for field in ["primary_actions", "next_actions"] {
        let Some(values) = progress
            .and_then(|value| value.get(field))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for value in values {
            let label = value.as_str().or_else(|| {
                value
                    .get("label")
                    .or_else(|| value.get("action"))
                    .or_else(|| value.get("command"))
                    .and_then(Value::as_str)
            });
            if let Some(label) = label.map(projection_label) {
                if !actions.contains(&label) {
                    actions.push(label);
                }
            }
        }
    }
    if actions.is_empty() {
        return String::new();
    }
    let items = actions
        .iter()
        .map(|action| format!("<li>{}</li>", escape(action)))
        .collect::<String>();
    format!(
        "<div class=\"projected-actions\"><strong>Next actions / 下一操作</strong><ul>{items}</ul></div>"
    )
}

fn phase_blockers(
    phase: &str,
    room: &Value,
    _review: Option<&Value>,
    member_count: usize,
) -> Vec<String> {
    let mut blockers = Vec::new();
    let work_items = room_records(room, "work_items");
    let revisions = room_records(room, "paper_revisions");
    let sessions = room_records(room, "member_research_sessions");
    match phase {
        "preregistering" => {
            if work_items.is_empty() {
                blockers.push("No work item has been assigned yet.".into());
            }
            if sessions.is_empty() {
                blockers.push("The live research session has not been authorized.".into());
            }
        }
        "researching" => {
            if work_items.is_empty() {
                blockers.push("Create at least one owned research task.".into());
            }
            if room_records(room, "artifact_manifests").is_empty() {
                blockers.push("No canonical ArtifactManifest is registered yet.".into());
            }
        }
        "experimenting" if room_records(room, "runs").is_empty() => {
            blockers
                .push("No run record is visible; failed runs count and must be retained.".into());
        }
        "drafting" => {
            if revisions.is_empty() {
                blockers.push("No paper revision is frozen yet.".into());
            }
            if work_items.iter().any(|item| {
                !matches!(
                    item.get("status").and_then(Value::as_str),
                    Some("accepted" | "cancelled")
                )
            }) {
                blockers.push("One or more work items still need review or acceptance.".into());
            }
        }
        "integrity_review" if room_records(room, "section_reviews").is_empty() => {
            blockers.push("No independent section review is visible.".into());
        }
        // Legacy `reproducing` is author-side reproduction readiness. An
        // independent reproduction belongs exclusively to Review Raid and
        // cannot block submission readiness here.
        "reproducing" => {}
        "author_approval" => {
            let release = revisions.iter().any(|revision| {
                revision
                    .get("release_candidate_hash")
                    .and_then(Value::as_str)
                    .is_some()
            });
            if !release {
                blockers
                    .push("The current revision is not promoted as a release candidate.".into());
            }
            let consent_count = room_records(room, "authorship_consents").len();
            if consent_count < member_count {
                blockers.push(format!(
                    "Author signatures: {consent_count}/{member_count}."
                ));
            }
        }
        _ => {}
    }
    blockers
}

fn phase_transition_form(
    paper_id: &str,
    paper_version: u64,
    phase: &str,
    progress: Option<&Value>,
) -> String {
    let actor_can_transition = progress
        .and_then(|value| value.get("actor_can_transition"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !actor_can_transition {
        return r#"<article class="guided-action role-duty"><h3>Captain checkpoint / 队长检查点</h3><p>The Captain owns phase transitions. Complete your role mission while the Captain coordinates the next checkpoint. / 阶段推进由队长负责；请完成本角色任务并等待队长协调。</p></article>"#.to_string();
    }
    let projected_next = progress
        .and_then(|value| value.get("next_phase"))
        .and_then(Value::as_str);
    let transition_ready = progress
        .and_then(|value| value.get("transition_ready"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let targets: Vec<(&str, &str)> = if progress.is_some() {
        projected_next
            .map(|next| vec![(next, "Hepta projected checkpoint / Hepta 推荐检查点")])
            .unwrap_or_default()
    } else {
        match phase {
            "forming" => vec![("preregistering", "Open preregistration / 开始预注册")],
            "preregistering" => vec![("researching", "Begin evidence research / 开始研究")],
            "researching" => vec![("experimenting", "Begin experiments / 开始实验")],
            "experimenting" => vec![("drafting", "Freeze runs and draft / 锁定运行并起草")],
            "drafting" => vec![(
                "integrity_review",
                "Send to integrity review / 送交完整性审查",
            )],
            "integrity_review" => vec![
                ("reproducing", "Begin reproduction readiness / 开始复现准备"),
                ("drafting", "Return for rework / 返回修订"),
            ],
            "reproducing" => vec![
                ("author_approval", "Request author approval / 请求作者批准"),
                ("drafting", "Return for rework / 返回修订"),
            ],
            "integrity_hold" => vec![("drafting", "Resolve in drafting / 返回修订解决")],
            _ => Vec::new(),
        }
    };
    if targets.is_empty() {
        return String::new();
    }
    let options = targets
        .iter()
        .map(|(value, label)| {
            format!(
                "<option value=\"{}\">{}</option>",
                escape(value),
                escape(label)
            )
        })
        .collect::<String>();
    format!(
        r#"<form class="paper-phase-form primary-action" data-paper-id="{}" data-paper-version="{}"><label>Next checkpoint / 下一检查点<select name="next_phase" required>{}</select></label><button type="submit" {}>Advance Author Raid / 推进远征</button><output>{}</output></form>"#,
        escape(paper_id),
        paper_version,
        options,
        if transition_ready { "" } else { "disabled" },
        if transition_ready {
            ""
        } else {
            "Resolve the projected blockers before advancing / 解决投影阻塞项后再推进"
        },
    )
}

fn team_member_options(members: &[Value], include_unassigned: bool) -> String {
    let mut options = if include_unassigned {
        "<option value=\"\">Unassigned team task / 团队待认领</option>".to_string()
    } else {
        String::new()
    };
    for member in members {
        let Some(player_id) = member.get("player_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(binding_id) = member.get("binding_id").and_then(Value::as_str) else {
            continue;
        };
        let slot = scalar(member.get("participant_slot"));
        let role = scalar(member.get("role"));
        options.push_str(&format!(
            r#"<option value="{}" data-binding-id="{}">Slot {} · {}</option>"#,
            escape(player_id),
            escape(binding_id),
            escape(&slot),
            escape(&role),
        ));
    }
    options
}

fn canonical_author_roles_enforced(members: &[Value]) -> bool {
    ["captain", "evidence", "experiment"]
        .iter()
        .all(|required| {
            members
                .iter()
                .filter(|member| member.get("role").and_then(Value::as_str) == Some(*required))
                .count()
                == 1
        })
}

fn team_member_options_for_actor(
    members: &[Value],
    current_player_id: &str,
    current_role: &str,
    canonical_roles_enforced: bool,
) -> String {
    if !canonical_roles_enforced || current_role == "captain" {
        return team_member_options(members, true);
    }
    let mine = members
        .iter()
        .filter(|member| member.get("player_id").and_then(Value::as_str) == Some(current_player_id))
        .cloned()
        .collect::<Vec<_>>();
    team_member_options(&mine, false)
}

fn artifact_manifest_options(manifests: &[Value]) -> String {
    let mut options = String::new();
    for manifest in manifests {
        let Some(manifest_hash) = manifest.get("manifest_hash").and_then(Value::as_str) else {
            continue;
        };
        let Some(source_hash) = artifact_role_hash(manifest, "paper_source") else {
            continue;
        };
        let Some(bibliography_hash) = artifact_role_hash(manifest, "bibliography") else {
            continue;
        };
        let Some(claim_hash) = artifact_role_hash(manifest, "claim_evidence_graph") else {
            continue;
        };
        let label = manifest
            .get("source_bundle_id")
            .and_then(Value::as_str)
            .unwrap_or("registered manifest");
        options.push_str(&format!(
            r#"<option value="{}" data-artifact-manifest-hash="{}" data-source-manifest-hash="{}" data-bibliography-hash="{}" data-claim-evidence-graph-hash="{}">{}</option>"#,
            escape(manifest_hash),
            escape(manifest_hash),
            escape(&source_hash),
            escape(&bibliography_hash),
            escape(&claim_hash),
            escape(label),
        ));
    }
    options
}

fn artifact_role_hash(manifest: &Value, role: &str) -> Option<String> {
    let raw = manifest
        .get("objects")
        .and_then(Value::as_array)?
        .iter()
        .find(|object| object.get("role").and_then(Value::as_str) == Some(role))?
        .get("sha256")?
        .as_str()?;
    if raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(format!("sha256:{}", raw.to_ascii_lowercase()))
    } else if raw.len() == 71
        && raw.starts_with("sha256:")
        && raw[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Some(raw.to_ascii_lowercase())
    } else {
        None
    }
}

fn record_options(records: &[Value], id_field: &str, label_field: &str, noun: &str) -> String {
    records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let id = record.get(id_field).and_then(Value::as_str)?;
            let label = record
                .get(label_field)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{noun} {}", index + 1));
            Some(format!(
                "<option value=\"{}\">{}</option>",
                escape(id),
                escape(&label)
            ))
        })
        .collect()
}

fn artifact_media_options(selected: &str) -> String {
    [
        "application/x-bibtex",
        "text/csv; charset=utf-8",
        "application/json",
        "text/markdown; charset=utf-8",
        "application/pdf",
        "text/x-python; charset=utf-8",
        "image/svg+xml",
        "text/plain; charset=utf-8",
        "application/octet-stream",
        "application/zip",
        "application/gzip",
    ]
    .iter()
    .map(|media_type| {
        format!(
            "<option value=\"{}\" {}>{}</option>",
            escape(media_type),
            if *media_type == selected {
                "selected"
            } else {
                ""
            },
            escape(media_type),
        )
    })
    .collect()
}

#[cfg(test)]
fn science_action_panel(paper_id: &str, phase: &str, room: &Value) -> String {
    science_action_panel_for_role(paper_id, phase, room, "team_member", false)
}

fn science_action_panel_for_role(
    paper_id: &str,
    phase: &str,
    room: &Value,
    current_role: &str,
    canonical_roles_enforced: bool,
) -> String {
    let paper = room.get("paper").filter(|value| value.is_object());
    let paper_version = paper
        .and_then(|value| value.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let challenge_id = paper
        .and_then(|value| value.get("challenge_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let manifests = room_records(room, "artifact_manifests");
    let evidence = room_records(room, "evidence_cards");
    let plans = room_records(room, "experiment_plans");
    let runs = room_records(room, "runs");
    let figures = room_records(room, "figures");
    let mut evidence_allowed = matches!(
        phase,
        "researching" | "experimenting" | "drafting" | "integrity_review" | "reproducing"
    );
    let mut plan_allowed = matches!(phase, "preregistering" | "researching");
    let mut experiment_actions_allowed = matches!(
        phase,
        "experimenting" | "drafting" | "integrity_review" | "reproducing"
    );
    if canonical_roles_enforced {
        evidence_allowed &= current_role == "evidence";
        plan_allowed &= current_role == "experiment";
        experiment_actions_allowed &= current_role == "experiment";
    }
    let role_resource_projection = room
        .get("author_raid_progress")
        .and_then(|value| value.get("role_resources"));
    let role_resources = role_resource_projection.filter(|value| value.is_object());
    let paper_has_role_resources = paper
        .and_then(|value| value.get("role_resources"))
        .is_some_and(|value| !value.is_null());
    let resource_authority_required =
        paper_has_role_resources || role_resource_projection.is_some_and(|value| !value.is_null());
    let run_resource_action =
        role_resources.and_then(|resources| role_resource_action(resources, "create_run_record"));
    let run_creation_allowed = experiment_actions_allowed
        && if resource_authority_required {
            run_resource_action
                .and_then(|value| value.get("available"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        } else {
            true
        };
    let draft_bundle_allowed = matches!(phase, "drafting" | "reproducing");
    if !evidence_allowed && !plan_allowed && !experiment_actions_allowed && !draft_bundle_allowed {
        return String::new();
    }

    let mut actions = String::new();
    if plan_allowed {
        if manifests.len() < 3 && paper_version > 0 && !challenge_id.is_empty() {
            actions.push_str(&format!(
                r#"<form class="input-manifest-wizard-form primary-action" data-paper-id="{}" data-paper-version="{}" data-challenge-id="{}"><h3>Prepare research inputs / 准备研究输入</h3><p class="muted">Choose code, dataset, and environment files. The browser hashes and uploads each file, then registers three neutral manifests from the CAS receipts.</p><label>Planned run label / 计划运行标识<input name="planned_run_id" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" value="planned-run-1" required></label><fieldset><legend>Code / 代码</legend><input name="code_file" type="file" required><select name="code_media_type" required>{}</select></fieldset><fieldset><legend>Dataset / 数据集</legend><input name="dataset_file" type="file" required><select name="dataset_media_type" required>{}</select></fieldset><fieldset><legend>Environment / 环境</legend><input name="environment_file" type="file" required><select name="environment_media_type" required>{}</select></fieldset><button type="submit">Hash, upload, and register inputs / 哈希上传并登记输入</button><output></output></form>"#,
                escape(paper_id),
                paper_version,
                escape(challenge_id),
                artifact_media_options("text/x-python; charset=utf-8"),
                artifact_media_options("text/csv; charset=utf-8"),
                artifact_media_options("application/json"),
            ));
        }
        let manifest_options = record_options(
            manifests,
            "manifest_id",
            "source_bundle_id",
            "Artifact manifest",
        );
        if manifests.len() >= 3 && !manifest_options.is_empty() {
            actions.push_str(&format!(
                r#"<form class="create-experiment-plan-form" data-paper-id="{}"><h3>Freeze experiment plan / 锁定实验计划</h3><p class="muted">Choose three distinct registered manifests. Plain-language descriptions are hashed locally; no digest is entered.</p><label>Protocol snapshot / 协议快照<textarea name="protocol" minlength="3" required placeholder="Describe the frozen protocol"></textarea></label><label>Code manifest / 代码清单<select name="code_manifest_id" required><option value="">Choose code manifest / 选择代码清单</option>{}</select></label><label>Dataset manifest / 数据集清单<select name="dataset_manifest_id" required><option value="">Choose dataset manifest / 选择数据清单</option>{}</select></label><label>Environment manifest / 环境清单<select name="environment_manifest_id" required><option value="">Choose environment manifest / 选择环境清单</option>{}</select></label><label>Seed policy / 随机种子策略<textarea name="seed_policy" minlength="3" required placeholder="Describe deterministic seed selection"></textarea></label><label>Stopping rule / 停止规则<textarea name="stopping_rule" minlength="3" required placeholder="Describe when the experiment must stop"></textarea></label><button type="submit">Freeze plan / 锁定计划</button><output></output></form>"#,
                escape(paper_id),
                manifest_options,
                manifest_options,
                manifest_options,
            ));
        } else {
            actions.push_str(
                r#"<article class="guided-action blocked"><h3>Experiment plan / 实验计划</h3><p>Register three distinct code, dataset, and environment manifests before freezing the plan / 先登记三个不同的代码、数据集与环境清单</p></article>"#,
            );
        }
    }

    if evidence_allowed {
        actions.push_str(&format!(
            r#"<form class="create-evidence-card-form" data-paper-id="{}"><h3>Verify a source / 验证来源</h3><p class="muted">Choose the exact source snapshot. The browser hashes and uploads its bytes to the team CAS, verifies the receipt, then your imported human key signs the server-built verification frame. No digest is entered by hand.</p><label>Canonical HTTPS source / 规范来源网址<input name="source_uri" type="url" maxlength="480" pattern="https://.*" required placeholder="https://example.org/paper"></label><label>Exact source snapshot / 精确来源快照<input name="source_file" type="file" required></label><label>Snapshot media type / 快照媒体类型<select name="source_media_type" required>{}</select></label><label>Locator / 定位信息<input name="locator" maxlength="512" required placeholder="p. 3, Table 1"></label><label>License / 许可证<input name="license" maxlength="512" required value="CC-BY-4.0"></label><button type="submit">Hash, preserve, verify, and sign / 哈希保存、验证并签名</button><output></output></form>"#,
            escape(paper_id),
            artifact_media_options("application/pdf"),
        ));

        let evidence_options = record_options(
            evidence,
            "evidence_card_id",
            "source_uri",
            "Verified evidence",
        );
        if evidence_options.is_empty() {
            actions.push_str(
                r#"<article class="guided-action blocked"><h3>Canonical citation / 规范引用</h3><p>Create a verified evidence card first / 请先创建已验证证据卡</p></article>"#,
            );
        } else {
            actions.push_str(&format!(
                r#"<form class="create-citation-record-form" data-paper-id="{}"><h3>Bind a canonical citation / 绑定规范引用</h3><p class="muted">Hash, locator, and license are copied from authoritative evidence by the server.</p><label>Verified evidence / 已验证证据<select name="evidence_card_id" required><option value="">Choose evidence / 选择证据</option>{}</select></label><label>DOI (one locator required) / DOI（至少一种定位）<input name="doi" maxlength="512" placeholder="10.1234/example"></label><label>Canonical HTTPS URL / 规范网址<input name="canonical_url" type="url" maxlength="480" pattern="https://.*" placeholder="https://example.org/paper"></label><button type="submit">Bind and sign citation / 绑定并签署引用</button><output></output></form>"#,
                escape(paper_id),
                evidence_options,
            ));
        }

        let run_options = record_options(runs, "run_record_id", "status", "Retained run");
        let figure_options =
            record_options(figures, "figure_lineage_id", "figure_key", "Figure lineage");
        actions.push_str(&format!(
            r#"<form class="create-claim-record-form" data-paper-id="{}"><h3>Bind claim lineage / 绑定论断谱系</h3><p class="muted">The plain-language statement is hashed locally. Main, numeric, and figure claims require at least one selected lineage record.</p><label>Claim key / 论断标识<input name="claim_key" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" maxlength="128" required placeholder="primary-effect"></label><label>Claim type / 论断类型<select name="claim_kind" required><option value="main">Main / 核心</option><option value="numeric">Numeric / 数值</option><option value="figure">Figure / 图表</option><option value="supporting">Supporting / 支持</option><option value="limitation">Limitation / 局限</option></select></label><label>Statement / 论断<textarea name="statement" minlength="3" required placeholder="Write the exact claim"></textarea></label><label>Evidence / 证据<select name="evidence_card_ids" multiple size="4">{}</select></label><label>Runs / 实验运行<select name="run_record_ids" multiple size="4">{}</select></label><label>Figures / 图表<select name="figure_lineage_ids" multiple size="4">{}</select></label><button type="submit">Bind claim / 绑定论断</button><output></output></form>"#,
            escape(paper_id),
            evidence_options,
            run_options,
            figure_options,
        ));
    }

    if experiment_actions_allowed {
        let plan_options = record_options(
            plans,
            "experiment_plan_id",
            "protocol_snapshot_hash",
            "Experiment plan",
        );
        let manifest_options = record_options(
            manifests,
            "manifest_id",
            "source_bundle_id",
            "Artifact manifest",
        );
        if run_creation_allowed {
            if !plan_options.is_empty() && paper_version > 0 && !challenge_id.is_empty() {
                actions.push_str(&format!(
                r#"<form class="run-artifact-wizard-form primary-action" data-paper-id="{}" data-paper-version="{}" data-challenge-id="{}"><h3>Preserve every run / 保存每次运行</h3><p class="muted">Choose the exact stdout and stderr for every outcome. Successful runs also require exact output and metrics files. The browser hashes and uploads their bytes, verifies every authoritative manifest receipt, and retains success, failure, or cancellation without exposing identifiers or digests.</p><label>Experiment plan / 实验计划<select name="experiment_plan_id" required><option value="">Choose plan / 选择计划</option>{}</select></label><label>Outcome / 结果<select name="status" required><option value="succeeded">Succeeded / 成功</option><option value="failed">Failed / 失败</option><option value="cancelled">Cancelled / 取消</option></select></label><label>Run label / 运行标识<input name="run_label" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" maxlength="128" value="run-1" required></label><label>Seed / 种子<input name="seed" type="number" step="1" value="1" required></label><label>Parameters / 参数<textarea name="parameters" minlength="1" required placeholder="Describe the exact parameter set"></textarea></label><fieldset><legend>Logs / 日志</legend><label>Standard output / 标准输出<input name="stdout_file" type="file" required></label><label>Standard error / 标准错误<input name="stderr_file" type="file" required></label></fieldset><fieldset class="run-artifact-success-field"><legend>Successful results / 成功结果</legend><label>Output file / 输出文件<input name="output_file" type="file" required></label><label>Output media type / 输出媒体类型<select name="output_media_type" required>{}</select></label><label>Metrics file / 指标文件<input name="metrics_file" type="file" accept="application/json,text/csv,text/plain,.json,.csv,.txt" required></label><label>Metrics media type / 指标媒体类型<select name="metrics_media_type" required><option value="application/json" selected>application/json</option><option value="text/csv; charset=utf-8">text/csv; charset=utf-8</option><option value="text/plain; charset=utf-8">text/plain; charset=utf-8</option></select></label></fieldset><label class="run-artifact-failure-field" hidden>Failure or cancellation / 失败或取消说明<textarea name="failure" minlength="3" placeholder="Describe why the run failed or was cancelled"></textarea></label><button type="submit">Hash, preserve, and retain run / 哈希保存并保留运行</button><output></output></form>"#,
                escape(paper_id),
                paper_version,
                escape(challenge_id),
                plan_options,
                artifact_media_options("application/octet-stream"),
            ));
            }
            if plan_options.is_empty() || manifest_options.is_empty() {
                actions.push_str(
                r#"<article class="guided-action blocked"><h3>Retain experiment run / 保留实验运行</h3><p>An experiment plan and registered log manifest are required / 需要实验计划与已登记日志清单</p></article>"#,
            );
            } else {
                actions.push_str(&format!(
                r#"<form class="create-run-record-form" data-paper-id="{}"><h3>Retain every run / 保留每次运行</h3><p class="muted">Failed and cancelled runs are first-class evidence. Plain-language descriptions are hashed locally; no digest is entered.</p><label>Experiment plan / 实验计划<select name="experiment_plan_id" required><option value="">Choose plan / 选择计划</option>{}</select></label><label>Outcome / 结果<select name="status" required><option value="succeeded">Succeeded / 成功</option><option value="failed">Failed / 失败</option><option value="cancelled">Cancelled / 取消</option></select></label><label>Seed / 种子<input name="seed" type="number" step="1" value="1" required></label><label>Parameters / 参数<textarea name="parameters" minlength="1" required placeholder="Describe the exact parameter set"></textarea></label><label>Logs manifest / 日志清单<select name="logs_manifest_id" required><option value="">Choose logs / 选择日志</option>{}</select></label><label class="run-success-field">Outputs manifest / 输出清单<select name="outputs_manifest_id"><option value="">Choose outputs / 选择输出</option>{}</select></label><label class="run-success-field">Metrics / 指标<textarea name="metrics" placeholder="Describe the metric results"></textarea></label><label class="run-failure-field" hidden>Failure record / 失败记录<textarea name="failure" placeholder="Describe the failure or cancellation"></textarea></label><button type="submit">Retain run / 保留运行</button><output></output></form>"#,
                escape(paper_id),
                plan_options,
                manifest_options,
                manifest_options,
            ));
            }
        } else {
            let blockers = run_resource_action
                .and_then(|value| value.get("blockers"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(projection_label)
                .map(|value| format!("<li>{}</li>", escape(value)))
                .collect::<String>();
            let blockers = if blockers.is_empty() {
                "<li>Authoritative run-resource availability is unavailable / 权威运行资源可用性不可用</li>".to_string()
            } else {
                blockers
            };
            actions.push_str(&format!(
                r#"<article class="guided-action blocked run-resource-blocked"><h3>Retain experiment run / 保留实验运行</h3><p>Run upload and registration are disabled before any CAS mutation.</p><ul>{}</ul></article>"#,
                blockers,
            ));
        }

        let run_options = record_options(runs, "run_record_id", "status", "Retained run");
        if !run_options.is_empty() && paper_version > 0 && !challenge_id.is_empty() {
            actions.push_str(&format!(
                r#"<form class="figure-lineage-wizard-form" data-paper-id="{}" data-paper-version="{}" data-challenge-id="{}"><h3>Bind a figure to its runs / 绑定图表运行谱系</h3><p class="muted">Choose the exact SVG and its transform description or JSON file, then select the authoritative runs used to produce it. The browser preserves both files, verifies the manifest receipt, and creates the lineage record.</p><label>Figure key / 图表标识<input name="figure_key" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" maxlength="128" value="figure-1" required></label><label>Authoritative runs / 权威运行<select name="run_record_ids" multiple size="4" required>{}</select></label><label>SVG figure / SVG 图表<input name="figure_file" type="file" accept="image/svg+xml,.svg" required></label><label>Transform description or JSON / 变换说明或 JSON<input name="lineage_file" type="file" accept="application/json,text/plain,text/markdown,.json,.txt,.md" required></label><label>Lineage media type / 谱系媒体类型<select name="lineage_media_type" required><option value="application/json" selected>application/json</option><option value="text/plain; charset=utf-8">text/plain; charset=utf-8</option><option value="text/markdown; charset=utf-8">text/markdown; charset=utf-8</option></select></label><button type="submit">Preserve and bind figure / 保存并绑定图表</button><output></output></form>"#,
                escape(paper_id),
                paper_version,
                escape(challenge_id),
                run_options,
            ));
        }
    }

    if matches!(phase, "drafting" | "reproducing") && paper_version > 0 && !challenge_id.is_empty()
    {
        let required_runs = runs
            .iter()
            .filter_map(|run| run.get("run_record_id").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(",");
        if required_runs.is_empty() {
            actions.push_str(
                r#"<article class="guided-action blocked"><h3>Draft manifest / 草稿清单</h3><p>Retain at least one run before assembling the revision bundle / 组装版本包前先保留至少一次运行</p></article>"#,
            );
        } else {
            actions.push_str(&format!(
                r#"<form class="draft-manifest-wizard-form primary-action" data-paper-id="{}" data-paper-version="{}" data-challenge-id="{}" data-required-run-ids="{}"><h3>Assemble revision bundle / 组装版本包</h3><p class="muted">The three required revision roles are hashed, uploaded, and registered as one canonical bundle. Run lineage comes from this room.</p><fieldset><legend>Paper source / 论文正文</legend><input name="paper_file" type="file" required><select name="paper_media_type" required>{}</select></fieldset><fieldset><legend>Bibliography / 参考文献</legend><input name="bibliography_file" type="file" required><select name="bibliography_media_type" required>{}</select></fieldset><fieldset><legend>Claim–evidence graph / 论断证据图</legend><input name="claim_graph_file" type="file" required><select name="claim_graph_media_type" required>{}</select></fieldset><button type="submit">Register revision bundle / 登记版本包</button><output></output></form>"#,
                escape(paper_id),
                paper_version,
                escape(challenge_id),
                escape(&required_runs),
                artifact_media_options("text/markdown; charset=utf-8"),
                artifact_media_options("application/x-bibtex"),
                artifact_media_options("application/json"),
            ));
            actions.push_str(&format!(
                r#"<form class="review-ready-manifest-wizard-form primary-action" data-paper-id="{}" data-paper-version="{}" data-challenge-id="{}" data-required-run-ids="{}"><h3>Freeze a Review-ready release / 冻结可评审版本</h3><p class="muted">Choose the human paper, the exact frozen evaluator and dataset supplied by this challenge, and one candidate JSON result. The browser first registers four human-readable same-Paper sources. Hepta then independently resolves their IDs and hashes, checks exact roles, media, CAS provenance, ordering, and duplicate objects, and freezes one reviewer-readable manifest. No UUID or digest is pasted.</p><fieldset><legend>Human paper / 人类论文</legend><label>Paper source / 论文正文<input name="paper_file" type="file" required></label><select name="paper_media_type" required>{}</select><label>Bibliography / 参考文献<input name="bibliography_file" type="file" required></label><select name="bibliography_media_type" required>{}</select><label>Claim–evidence graph JSON / 论断证据图 JSON<input name="claim_graph_file" type="file" accept="application/json,.json" required></label></fieldset><fieldset><legend>Frozen challenge authority / 冻结挑战权威</legend><label>Frozen evaluator Python / 冻结评估器<input name="evaluator_file" type="file" accept="text/x-python,.py" required></label><label>Exact challenge dataset / 精确挑战数据集<input name="review_dataset_file" type="file" accept="application/json,text/csv,.json,.csv" required></label><select name="review_dataset_media_type" required><option value="application/json" selected>application/json</option><option value="text/csv; charset=utf-8">text/csv; charset=utf-8</option></select></fieldset><fieldset><legend>Candidate result / 候选结果</legend><label>Candidate JSON / 候选 JSON<input name="candidate_file" type="file" accept="application/json,.json" required></label></fieldset><button type="submit">Register sources and freeze Review bundle / 登记源并冻结评审包</button><output></output></form>"#,
                escape(paper_id),
                paper_version,
                escape(challenge_id),
                escape(&required_runs),
                artifact_media_options("text/markdown; charset=utf-8"),
                artifact_media_options("application/x-bibtex"),
            ));
        }
    }

    format!(
        r#"<section class="panel science-action-panel"><h2>Scientific Actions / 科研行动</h2><p>Choose authoritative records from the room. The browser generates identifiers and sends only typed fields—never pasted JSON.</p><div class="action-grid">{}</div></section>"#,
        actions,
    )
}

struct WorkItemPanelInput<'a> {
    paper_id: &'a str,
    paper_version: u64,
    members: &'a [Value],
    work_items: &'a [Value],
    manifests: &'a [Value],
    primary: bool,
    editable: bool,
    current_player_id: &'a str,
    current_role: &'a str,
    canonical_roles_enforced: bool,
}

fn work_item_panel(input: WorkItemPanelInput<'_>) -> String {
    let WorkItemPanelInput {
        paper_id,
        paper_version,
        members,
        work_items,
        manifests,
        primary,
        editable,
        current_player_id,
        current_role,
        canonical_roles_enforced,
    } = input;
    let assignment_options = team_member_options_for_actor(
        members,
        current_player_id,
        current_role,
        canonical_roles_enforced,
    );
    let create_class = if primary && work_items.is_empty() {
        "create-work-item-form primary-action"
    } else {
        "create-work-item-form"
    };
    let create = if editable {
        format!(
            r#"<form class="{}" data-paper-id="{}" data-paper-version="{}"><h3>Create a team task / 创建团队任务</h3><label>Task type / 任务类型<select name="kind" required><option value="paper_section">Paper section / 论文章节</option><option value="evidence_review">Evidence review / 证据审查</option><option value="experiment">Experiment / 实验</option><option value="synthesis">Synthesis / 综合</option></select></label><label>Outcome / 交付目标<input name="title" minlength="3" maxlength="200" required placeholder="e.g. Verify the main effect against the source"></label><label>Owner / 负责人<select name="assignee">{}</select></label><button type="submit">Add to mission board / 加入任务板</button><output></output></form>"#,
            create_class,
            escape(paper_id),
            paper_version,
            assignment_options,
        )
    } else {
        String::new()
    };
    let artifact_options = manifests
        .iter()
        .filter_map(|manifest| {
            let hash = manifest.get("manifest_hash").and_then(Value::as_str)?;
            let label = manifest
                .get("source_bundle_id")
                .and_then(Value::as_str)
                .unwrap_or("registered manifest");
            Some(format!(
                "<option value=\"{}\">{}</option>",
                escape(hash),
                escape(label)
            ))
        })
        .collect::<String>();
    let mut cards = String::new();
    for item in work_items {
        let Some(work_item_id) = item.get("work_item_id").and_then(Value::as_str) else {
            continue;
        };
        let status = item
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let title = scalar(item.get("title"));
        let version = item.get("version").and_then(Value::as_u64).unwrap_or(0);
        let assigned_player_id = item.get("assigned_player_id").and_then(Value::as_str);
        let actor_can_transition = !canonical_roles_enforced
            || current_role == "captain"
            || assigned_player_id == Some(current_player_id);
        let targets: &[(&str, &str)] = match status {
            "planned" => &[("in_progress", "Start"), ("cancelled", "Cancel")],
            "in_progress" => &[("review", "Request review"), ("cancelled", "Cancel")],
            "review" => &[("accepted", "Accept"), ("rejected", "Return for rework")],
            "rejected" => &[("in_progress", "Resume")],
            _ => &[],
        };
        let transition = if !editable || targets.is_empty() {
            "<p class=\"muted\">No further task transition / 无后续任务转换</p>".to_string()
        } else if !actor_can_transition {
            "<p class=\"muted\">Only the assigned player or Captain can update this task / 仅负责人或队长可更新此任务</p>".to_string()
        } else {
            let options = targets
                .iter()
                .map(|(value, label)| {
                    format!(
                        "<option value=\"{}\">{}</option>",
                        escape(value),
                        escape(label)
                    )
                })
                .collect::<String>();
            format!(
                r#"<form class="work-item-transition-form" data-work-item-id="{}" data-work-item-version="{}"><label>Next status / 下一状态<select name="next_status">{}</select></label><label class="accepted-artifact-field">Accepted artifact / 验收工件<select name="artifact_manifest_hash"><option value="">Required only when accepting / 仅验收时必填</option>{}</select></label><button type="submit">Update task / 更新任务</button><output></output></form>"#,
                escape(work_item_id),
                version,
                options,
                artifact_options,
            )
        };
        cards.push_str(&format!(
            r#"<article class="work-card"><span class="pill">{}</span><h3>{}</h3><p class="muted">{}</p>{}</article>"#,
            escape(status),
            escape(&title),
            escape(work_item_id),
            transition,
        ));
    }
    if cards.is_empty() {
        cards.push_str("<p class=\"muted\">No team tasks yet / 尚无团队任务</p>");
    }
    format!(
        r#"<section class="panel work-item-panel"><div><h2>Mission Board / 任务板</h2><p>Tasks use authoritative versions from this room; players never paste work-item IDs.</p></div><div class="action-grid">{}<div class="work-board">{}</div></div></section>"#,
        create, cards,
    )
}

fn research_session_panel(
    paper_id: &str,
    paper_version: u64,
    team_version: u64,
    members: &[Value],
    sessions: &[Value],
    primary: bool,
    allow_issue: bool,
) -> String {
    let mut controls = String::new();
    if sessions.is_empty() && allow_issue {
        let class = if primary {
            "issue-authorization-form primary-action"
        } else {
            "issue-authorization-form"
        };
        controls.push_str(&format!(
            r#"<form class="{}" data-paper-id="{}" data-paper-version="{}" data-team-version="{}"><h3>Open the live research session / 开启实时研究会话</h3><label>Session name / 会话名<input name="session_id" minlength="1" maxlength="128" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" value="paper-raid-{}" required></label><label>Authorization lifetime / 授权时长<select name="ttl_seconds"><option value="1800">30 minutes</option><option value="3600">1 hour</option><option value="7200">2 hours</option></select></label><button type="submit">Authorize team session / 授权团队会话</button><output></output></form>"#,
            class,
            escape(paper_id),
            paper_version,
            team_version,
            escape(paper_id),
        ));
    }
    let slot_options = members
        .iter()
        .filter_map(|member| {
            let slot = member.get("participant_slot").and_then(Value::as_u64)?;
            let role = member
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("member");
            Some(format!(
                "<option value=\"{}\">Slot {} · {}</option>",
                slot,
                slot,
                escape(role)
            ))
        })
        .collect::<String>();
    for session in sessions {
        let Some(session_id) = session.get("logical_session_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(set_id) = session.get("authorization_set_id").and_then(Value::as_str) else {
            continue;
        };
        let roster_version = session
            .get("roster_version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let status = session
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let control = match status {
            "issued" if roster_version == 1 => format!(
                r#"<form class="research-session-control-form" data-command="create_nakama_research_session_control" data-authorization-set-id="{}"><button type="submit">Start live collaboration / 启动实时协作</button><output></output></form>"#,
                escape(set_id),
            ),
            "issued" => format!(
                r#"<form class="research-session-control-form" data-command="replace_nakama_research_session_roster_control" data-authorization-set-id="{}"><button type="submit">Apply replacement roster / 应用替换阵容</button><output></output></form>"#,
                escape(set_id),
            ),
            "consumed" => format!(
                r#"<div class="session-control-stack"><form class="research-session-control-form" data-command="resume_nakama_research_session_control" data-session-id="{}" data-roster-version="{}"><button type="submit">Resume after restart / 重启后恢复</button><output></output></form><form class="research-session-control-form" data-command="complete_nakama_research_session_control" data-session-id="{}" data-roster-version="{}"><button type="submit">Complete collaboration / 完成协作</button><output></output></form><details><summary>Replace disconnected member / 替换断线成员</summary><form class="replace-authorization-form" data-paper-id="{}" data-paper-version="{}" data-team-version="{}" data-session-id="{}" data-roster-version="{}"><label>Disconnected slot / 断线席位<select name="participant_slot" required>{}</select></label><label>New authorization lifetime / 新授权时长<select name="ttl_seconds"><option value="1800">30 minutes</option><option value="3600">1 hour</option></select></label><button type="submit">Issue replacement epoch / 签发替换纪元</button><output></output></form></details></div>"#,
                escape(session_id),
                roster_version,
                escape(session_id),
                roster_version,
                escape(paper_id),
                paper_version,
                team_version,
                escape(session_id),
                roster_version,
                slot_options,
            ),
            _ => String::new(),
        };
        controls.push_str(&format!(
            r#"<article class="session-card"><span class="pill">{}</span><h3>{}</h3><p>Roster epoch {}</p>{}</article>"#,
            escape(status),
            escape(session_id),
            roster_version,
            control,
        ));
    }
    format!(
        r#"<section class="panel research-session-panel"><h2>Live Research Session / 实时研究会话</h2><p>Hepta derives the exact authorized roster; the browser never chooses a Nakama RPC or signs as an Agent.</p><div class="action-grid">{}</div></section>"#,
        controls,
    )
}

fn section_collaboration_panel(
    paper_id: &str,
    phase: &str,
    room: &Value,
    members: &[Value],
    current_player_id: &str,
) -> String {
    if !matches!(phase, "drafting" | "integrity_review") {
        return String::new();
    }
    let Some(member) = members
        .iter()
        .find(|member| member.get("player_id").and_then(Value::as_str) == Some(current_player_id))
    else {
        return unavailable("current team member for section collaboration");
    };
    let binding_id = member
        .get("binding_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let agent_id = member.get("agent_id").and_then(Value::as_str).unwrap_or("");
    if binding_id.is_empty() || agent_id.is_empty() {
        return unavailable("active team Agent binding for section collaboration");
    }

    let paper = room.get("paper").filter(|value| value.is_object());
    let current_paper_revision_id = paper
        .and_then(|value| value.get("current_revision_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let heads = room_records(room, "section_heads");
    let leases = room_records(room, "leases");
    let proposals = room_records(room, "proposals");
    let decisions = room_records(room, "decisions");
    let section_revisions = room_records(room, "section_revisions");
    let work_items = room_records(room, "work_items");
    let manifests = room_records(room, "artifact_manifests");
    let now_unix = Utc::now().timestamp();

    let mut actions = String::new();
    if phase == "drafting" {
        if current_paper_revision_id.is_empty() {
            actions.push_str(
                r#"<article class="guided-action blocked section-baseline-required"><h3>Freeze the baseline first / 先锁定基线</h3><p>Create the whole-paper revision above. Section leases always advance that authoritative current revision; the browser never invents a parent UUID.</p></article>"#,
            );
        } else {
            let mut section_keys = vec![
                "abstract".to_string(),
                "introduction".to_string(),
                "methods".to_string(),
                "results".to_string(),
                "discussion".to_string(),
                "limitations".to_string(),
                "conclusion".to_string(),
            ];
            for head in heads {
                if let Some(key) = head.get("section_key").and_then(Value::as_str) {
                    if !section_keys.iter().any(|known| known == key) {
                        section_keys.push(key.to_string());
                    }
                }
            }
            let mut section_options = String::new();
            for key in section_keys {
                let head = heads.iter().find(|head| {
                    head.get("section_key").and_then(Value::as_str) == Some(key.as_str())
                });
                let previous_token = head
                    .and_then(|head| head.get("fencing_token"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let has_active_lease = leases.iter().any(|lease| {
                    lease.get("section_key").and_then(Value::as_str) == Some(key.as_str())
                        && section_lease_is_live(lease, now_unix)
                });
                if !has_active_lease {
                    section_options.push_str(&format!(
                        r#"<option value="{}" data-fencing-token="{}" {}>{}</option>"#,
                        escape(&key),
                        previous_token,
                        if key == "methods" { "selected" } else { "" },
                        escape(&key),
                    ));
                }
            }
            if section_options.is_empty() {
                actions.push_str(
                    r#"<article class="guided-action blocked"><h3>Section leases / 章节租约</h3><p>Every known section already has an active lease. Continue its task or wait for expiry / 所有已知章节均有有效租约，请继续现有任务或等待到期。</p></article>"#,
                );
            } else {
                actions.push_str(&format!(
                    r#"<form class="acquire-section-lease-form primary-action" data-paper-id="{}" data-holder-binding-id="{}" data-current-revision-id="{}"><h3>Claim one section / 领取一个章节</h3><p class="muted">The section head and fencing token come from this authoritative room. Hepta rechecks that your frozen team binding is still active.</p><dl><div><dt>Current paper revision / 当前论文版本</dt><dd><code>{}</code></dd></div><div><dt>Your frozen Agent binding / 你的冻结 Agent 绑定</dt><dd><code>{}</code></dd></div></dl><label>Section / 章节<select name="section_key" required>{}</select></label><label>Lease lifetime / 租约时长<select name="ttl_seconds"><option value="900">15 minutes</option><option value="1800">30 minutes</option><option value="3600">1 hour</option></select></label><button type="submit">Acquire authoritative lease / 获取权威租约</button><output></output></form>"#,
                    escape(paper_id),
                    escape(binding_id),
                    escape(current_paper_revision_id),
                    escape(current_paper_revision_id),
                    escape(binding_id),
                    section_options,
                ));
            }
        }

        for lease in leases.iter().filter(|lease| {
            lease.get("holder_player_id").and_then(Value::as_str) == Some(current_player_id)
                && lease.get("holder_binding_id").and_then(Value::as_str) == Some(binding_id)
                && section_lease_is_live(lease, now_unix)
        }) {
            let section_key = scalar(lease.get("section_key"));
            let parent_revision_id = heads
                .iter()
                .find(|head| {
                    head.get("section_key").and_then(Value::as_str) == Some(section_key.as_str())
                })
                .and_then(|head| head.get("current_head_revision_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let work_options = work_items
                .iter()
                .filter(|work| {
                    work.get("assigned_player_id").and_then(Value::as_str)
                        == Some(current_player_id)
                        && work.get("assigned_binding_id").and_then(Value::as_str)
                            == Some(binding_id)
                        && work.get("status").and_then(Value::as_str) != Some("cancelled")
                })
                .filter_map(|work| {
                    let id = work.get("work_item_id")?.as_str()?;
                    let title = work
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Assigned work item");
                    Some(format!(
                        "<option value=\"{}\">{}</option>",
                        escape(id),
                        escape(title),
                    ))
                })
                .collect::<String>();
            let manifest_options = manifests
                .iter()
                .filter_map(|manifest| {
                    let id = manifest.get("manifest_id")?.as_str()?;
                    let manifest_hash = manifest.get("manifest_hash")?.as_str()?;
                    let label = manifest
                        .get("source_bundle_id")
                        .or_else(|| manifest.get("manifest_hash"))
                        .and_then(Value::as_str)
                        .unwrap_or("Registered artifact");
                    Some(format!(
                        "<option value=\"{}\" data-artifact-manifest-hash=\"{}\">{}</option>",
                        escape(id),
                        escape(manifest_hash),
                        escape(label),
                    ))
                })
                .collect::<String>();
            if parent_revision_id.is_empty()
                || work_options.is_empty()
                || manifest_options.is_empty()
            {
                actions.push_str(&format!(
                    r#"<article class="guided-action blocked"><h3>Agent delivery · {}</h3><p>An assigned work item, registered artifact, and authoritative section head are required before the local Bridge can submit.</p></article>"#,
                    escape(&section_key),
                ));
            } else {
                actions.push_str(&format!(
                    r#"<article class="guided-action bridge-inbox-task" data-paper-id="{}"><h3>Agent task available / Agent 任务已就绪</h3><p class="muted">The installed Bridge receives this task in its proof-authenticated inbox. After the local Agent produces and registers its output, the Bridge derives and freezes the current same-binding lease/fence, section parent, assignment, and manifest digest from Hepta; nothing is joined or guessed locally. In confirmation mode it shows one local approval prompt; in auto mode it submits only one current explicit draft and otherwise fails closed.</p><dl><div><dt>Section / 章节</dt><dd><code>{}</code></dd></div><div><dt>Agent / Agent</dt><dd><code>{}</code></dd></div></dl><p>No signature JSON, login credential, private key, terminal command, proposal UUID, or digest crosses the browser/Bridge boundary. Return here after the Bridge reports an authoritative submission.</p></article>"#,
                    escape(paper_id),
                    escape(&section_key),
                    escape(agent_id),
                ));
            }
        }

        for proposal in proposals
            .iter()
            .filter(|proposal| proposal.get("status").and_then(Value::as_str) == Some("submitted"))
        {
            let proposal_id = scalar(proposal.get("proposal_id"));
            let proposal_version = proposal.get("version").and_then(Value::as_u64).unwrap_or(0);
            let section_key = scalar(proposal.get("section_key"));
            let agent = scalar(proposal.get("agent_id"));
            if proposal_id.is_empty() || proposal_version == 0 {
                continue;
            }
            actions.push_str(&format!(
                r#"<form class="human-proposal-decision-form primary-action" data-paper-id="{}" data-proposal-id="{}" data-proposal-version="{}"><h3>Human decision · {} / 人类决策</h3><p>Agent <code>{}</code> proposed <code>{}</code>. Read the registered artifact before deciding; Agent output remains non-authoritative until this local human signature.</p><label>Reason / 理由<textarea name="reason" minlength="3" required placeholder="Why accept, request rework, or reject?"></textarea></label><div class="button-row"><button type="submit" name="decision" value="accept">Accept / 接受</button><button type="submit" name="decision" value="rework">Rework / 返工</button><button type="submit" name="decision" value="reject">Reject / 拒绝</button></div><output></output></form>"#,
                escape(paper_id),
                escape(&proposal_id),
                proposal_version,
                escape(&section_key),
                escape(&agent),
                escape(&scalar(proposal.get("payload_hash"))),
            ));
        }

        for proposal in proposals
            .iter()
            .filter(|proposal| proposal.get("status").and_then(Value::as_str) == Some("accepted"))
        {
            let proposal_id = scalar(proposal.get("proposal_id"));
            if section_revisions.iter().any(|revision| {
                revision.get("proposal_id").and_then(Value::as_str) == Some(proposal_id.as_str())
            }) {
                continue;
            }
            let section_key = scalar(proposal.get("section_key"));
            let parent_revision_id = scalar(proposal.get("parent_revision_id"));
            let Some(lease) = leases.iter().find(|lease| {
                lease.get("section_key").and_then(Value::as_str) == Some(section_key.as_str())
                    && lease.get("holder_player_id").and_then(Value::as_str)
                        == Some(current_player_id)
                    && lease.get("holder_binding_id").and_then(Value::as_str)
                        == proposal.get("binding_id").and_then(Value::as_str)
                    && section_lease_is_live(lease, now_unix)
            }) else {
                continue;
            };
            let lease_id = scalar(lease.get("lease_id"));
            let fencing_token = lease
                .get("fencing_token")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let patch_manifest_id = scalar(proposal.get("artifact_manifest_id"));
            let patch_hash = scalar(proposal.get("payload_hash"));
            if proposal_id.is_empty()
                || parent_revision_id.is_empty()
                || lease_id.is_empty()
                || patch_manifest_id.is_empty()
                || patch_hash.is_empty()
                || fencing_token == 0
            {
                continue;
            }
            actions.push_str(&format!(
                r#"<form class="create-section-revision-form primary-action" data-paper-id="{}" data-section-key="{}" data-parent-revision-id="{}" data-proposal-id="{}" data-lease-id="{}" data-fencing-token="{}" data-patch-manifest-id="{}" data-patch-hash="{}"><h3>Materialize accepted delivery / 固化已接受交付</h3><p>Creates one section revision from the exact accepted Agent proposal, active lease, and registered artifact. No lineage field is editable.</p><button type="submit">Create section revision / 创建章节版本</button><output></output></form>"#,
                escape(paper_id),
                escape(&section_key),
                escape(&parent_revision_id),
                escape(&proposal_id),
                escape(&lease_id),
                fencing_token,
                escape(&patch_manifest_id),
                escape(&patch_hash),
            ));
        }
    }

    for revision in section_revisions {
        let revision_id = scalar(revision.get("section_revision_id"));
        let revision_version = revision.get("version").and_then(Value::as_u64).unwrap_or(0);
        let status = revision.get("status").and_then(Value::as_str).unwrap_or("");
        let lease_id = scalar(revision.get("lease_id"));
        let lease = leases
            .iter()
            .find(|lease| lease.get("lease_id").and_then(Value::as_str) == Some(lease_id.as_str()));
        let holder = lease
            .and_then(|lease| lease.get("holder_player_id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if status == "proposed" && revision_version > 0 {
            if holder == current_player_id {
                actions.push_str(&format!(
                    r#"<article class="guided-action blocked"><h3>Independent review pending · {}</h3><p>Your own section revision <code>{}</code> must be reviewed and signed by a different human teammate.</p></article>"#,
                    escape(&scalar(revision.get("section_key"))),
                    escape(&revision_id),
                ));
            } else if !holder.is_empty() {
                actions.push_str(&format!(
                    r#"<form class="section-review-form primary-action" data-paper-id="{}" data-section-revision-id="{}" data-revision-version="{}"><h3>Independent section review · {} / 独立章节审查</h3><p>Hepta confirms the lease holder is a different human. Your imported browser key signs the exact current revision and verdict.</p><label>Review finding / 审查意见<textarea name="review" minlength="3" required placeholder="Explain approval, rework, or rejection"></textarea></label><div class="button-row"><button type="submit" name="verdict" value="approve">Approve / 通过</button><button type="submit" name="verdict" value="rework">Rework / 返工</button><button type="submit" name="verdict" value="reject">Reject / 拒绝</button></div><output></output></form>"#,
                    escape(paper_id),
                    escape(&revision_id),
                    revision_version,
                    escape(&scalar(revision.get("section_key"))),
                ));
            }
        }
        if status == "approved" && revision_version > 0 && holder == current_player_id {
            let parent_revision_id = scalar(revision.get("parent_revision_id"));
            let fencing_token = revision
                .get("fencing_token")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let lease_active = lease.is_some_and(|lease| section_lease_is_live(lease, now_unix));
            let head_matches = heads.iter().any(|head| {
                head.get("section_key").and_then(Value::as_str)
                    == revision.get("section_key").and_then(Value::as_str)
                    && head.get("current_head_revision_id").and_then(Value::as_str)
                        == Some(parent_revision_id.as_str())
                    && head.get("fencing_token").and_then(Value::as_u64) == Some(fencing_token)
            });
            if lease_active && head_matches && fencing_token > 0 {
                actions.push_str(&format!(
                    r#"<form class="merge-section-form primary-action" data-paper-id="{}" data-section-revision-id="{}" data-revision-version="{}" data-parent-revision-id="{}" data-lease-id="{}" data-fencing-token="{}"><h3>Merge approved section · {} / 合并已通过章节</h3><p>The exact approved revision, current head, and holder lease are server-derived. Your browser key signs the final head advance.</p><button type="submit">Sign and merge / 签名并合并</button><output></output></form>"#,
                    escape(paper_id),
                    escape(&revision_id),
                    revision_version,
                    escape(&parent_revision_id),
                    escape(&lease_id),
                    fencing_token,
                    escape(&scalar(revision.get("section_key"))),
                ));
            }
        }
    }

    let accepted_decisions = decisions
        .iter()
        .filter(|decision| decision.get("decision").and_then(Value::as_str) == Some("accept"))
        .count();
    if actions.is_empty() {
        actions.push_str(
            r#"<article class="guided-action blocked"><h3>Section collaboration waiting / 章节协作等待中</h3><p>No safe action is currently derivable for this identity. Refresh after a teammate or local Agent advances the authoritative room.</p></article>"#,
        );
    }
    format!(
        r#"<section class="panel section-collaboration-panel"><h2>Human + Agent Section Loop / 人机章节循环</h2><p>Lease → local Bridge proposal → human decision → immutable section revision → independent human review → signed merge. Normal play never pastes UUIDs, versions, signatures, or JSON.</p><p class="muted">{} accepted human decision(s) recorded. Agent proposal signatures are created only by <code>tools/paper-raid-agent-bridge</code>; the BFF never receives an Agent private key.</p><div class="action-grid">{}</div></section>"#,
        accepted_decisions, actions,
    )
}

fn section_lease_is_live(lease: &Value, now_unix: i64) -> bool {
    if lease.get("status").and_then(Value::as_str) != Some("active") {
        return false;
    }
    lease
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|expires_at| expires_at.timestamp() > now_unix)
        // A malformed authoritative timestamp must not create a second lease from the UI.
        .unwrap_or(true)
}

fn revision_materialization_summary(revision: Option<&Value>) -> String {
    let Some(revision) = revision else {
        return r#"<article class="materialization-summary pending"><span class="pill">MATERIALIZATION PENDING</span><p>Freeze the first whole-paper revision to establish the section baseline / 锁定首个全文版本以建立章节基线。</p></article>"#.into();
    };
    let Some(root) = revision
        .get("section_materialization_root")
        .and_then(Value::as_str)
    else {
        return r#"<article class="materialization-summary missing"><span class="pill">LEGACY REVISION</span><p>This revision has no section-materialization root. New release candidates remain fail-closed until an authoritative revision captures every merged section / 当前版本没有章节物化根；新发布候选须先由权威版本捕获全部合并章节。</p></article>"#.into();
    };
    let Some(descriptor) = revision
        .get("section_materialization")
        .filter(|value| value.is_object())
    else {
        return r#"<article class="materialization-summary missing"><span class="pill">MATERIALIZATION INVALID</span><p>The revision root has no canonical descriptor. Promotion remains locked / 版本根缺少规范描述符，发布保持锁定。</p></article>"#.into();
    };
    let Some(sections) = descriptor.get("sections").and_then(Value::as_array) else {
        return r#"<article class="materialization-summary missing"><span class="pill">MATERIALIZATION INVALID</span><p>The canonical section set is unavailable. Promotion remains locked / 规范章节集合不可用，发布保持锁定。</p></article>"#.into();
    };
    let section_rows = if sections.is_empty() {
        "<li>Baseline captured; no merged section patch yet / 已捕获基线，尚无合并章节补丁</li>"
            .to_string()
    } else {
        sections
            .iter()
            .map(|section| {
                format!(
                    r#"<li><strong>{}</strong><span>patch <code>{}</code></span><small>base <code>{}</code> → head <code>{}</code></small></li>"#,
                    escape(&scalar(section.get("section_key"))),
                    escape(&scalar(section.get("patch_hash"))),
                    escape(&scalar(section.get("base_paper_revision_id"))),
                    escape(&scalar(section.get("head_section_revision_id"))),
                )
            })
            .collect::<String>()
    };
    let release_root = revision
        .get("release_candidate")
        .and_then(|candidate| candidate.get("section_materialization_root"))
        .and_then(Value::as_str);
    let (pill, status) = match release_root {
        Some(value) if value == root => (
            "RELEASE-BOUND MATERIALIZATION",
            "The release candidate binds this exact root / 发布候选已绑定此精确根",
        ),
        Some(_) => (
            "MATERIALIZATION MISMATCH",
            "Release root mismatch: finalization must remain locked / 发布根不一致，终局必须保持锁定",
        ),
        None => (
            "REVISION MATERIALIZED",
            "Promotion will bind this exact root / 提升发布时将绑定此精确根",
        ),
    };
    let parent = descriptor
        .get("parent_materialization_root")
        .and_then(Value::as_str)
        .map(|value| {
            format!(
                "<p class=\"muted\">Parent materialization / 父物化根: <code>{}</code></p>",
                escape(value)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<article class="materialization-summary"><span class="pill">{}</span><h3>Section lineage frozen / 章节谱系已锁定</h3><p>{}</p><p>Root / 根: <code>{}</code> · {} section(s) / 章节</p>{}<ul class="materialized-sections">{}</ul></article>"#,
        pill,
        status,
        escape(root),
        sections.len(),
        parent,
        section_rows,
    )
}

struct RevisionPanelInput<'a> {
    paper_id: &'a str,
    paper_version: u64,
    paper: &'a Value,
    room: &'a Value,
    review: Option<&'a Value>,
    members: &'a [Value],
    phase: &'a str,
    current_player_id: &'a str,
}

fn revision_panel(input: RevisionPanelInput<'_>) -> String {
    let RevisionPanelInput {
        paper_id,
        paper_version,
        paper,
        room,
        review,
        members,
        phase,
        current_player_id,
    } = input;
    let revisions = room_records(room, "paper_revisions");
    let manifests = room_records(room, "artifact_manifests");
    let current_revision_id = paper
        .get("current_revision_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let manifest_options = artifact_manifest_options(manifests);
    let first_manifest = if manifest_options.is_empty() {
        "<option value=\"\">No complete registered manifest / 无完整工件清单</option>".to_string()
    } else {
        format!(
            "<option value=\"\">Choose registered manifest / 选择工件清单</option>{manifest_options}"
        )
    };
    let revision_class = if phase == "drafting" && revisions.is_empty() {
        "create-paper-revision-form primary-action"
    } else {
        "create-paper-revision-form"
    };
    let create_revision = if !matches!(phase, "drafting" | "reproducing") {
        String::new()
    } else {
        format!(
            r#"<form class="{}" data-paper-id="{}" data-paper-version="{}" data-parent-revision-id="{}"><h3>Freeze a paper revision / 锁定论文版本</h3><p class="muted">Choose one complete registered bundle. Paper source, bibliography, claim graph, and manifest digests are copied from its authoritative CAS-backed record and cannot be typed or changed here.</p><label>Registered revision bundle / 已登记版本包<select name="manifest_source" required>{}</select></label><input name="source_manifest_hash" type="hidden"><input name="artifact_manifest_hash" type="hidden"><input name="bibliography_hash" type="hidden"><input name="claim_evidence_graph_hash" type="hidden"><button type="submit">Create frozen revision / 创建锁定版本</button><output></output></form>"#,
            revision_class,
            escape(paper_id),
            paper_version,
            escape(current_revision_id),
            first_manifest,
        )
    };
    let release_revision = revisions.iter().find(|revision| {
        revision
            .get("release_candidate_hash")
            .and_then(Value::as_str)
            .is_some()
    });
    let current_revision = revisions.iter().find(|revision| {
        revision.get("revision_id").and_then(Value::as_str) == Some(current_revision_id)
    });
    let promote = if phase == "author_approval" && release_revision.is_none() {
        current_revision
            .map(|revision| {
                promote_release_form(paper_id, paper_version, paper, room, members, revision)
            })
            .unwrap_or_else(|| unavailable("current draft revision"))
    } else {
        String::new()
    };
    let approval = release_revision
        .map(|revision| {
            release_approval_controls(
                paper_id,
                paper_version,
                room,
                review,
                members,
                revision,
                current_player_id,
            )
        })
        .unwrap_or_default();
    let materialization = revision_materialization_summary(current_revision);
    format!(
        r#"<section class="panel revision-panel"><h2>Draft + Release / 草稿与发布</h2><p>The room supplies paper and revision versions. Only content digests and human-readable release metadata remain editable.</p>{}<div class="action-grid">{}{}{}</div></section>"#,
        materialization, create_revision, promote, approval,
    )
}

fn latest_field<'a>(records: &'a [Value], field: &str) -> Option<&'a str> {
    records
        .iter()
        .rev()
        .find_map(|record| record.get(field).and_then(Value::as_str))
}

fn authoritative_contribution_refs(
    room: &Value,
    paper_id: &str,
    player_id: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let paper_id = canonical_uuid_text(paper_id)?;
    let player_id = canonical_uuid_text(player_id)?;
    let members = room.get("team")?.get("members")?.as_array()?;
    if !(3..=5).contains(&members.len()) {
        return None;
    }
    let mut author_ids = HashSet::with_capacity(members.len());
    for member in members {
        let member_id = canonical_uuid_value(member.get("player_id")?)?;
        if !author_ids.insert(member_id) {
            return None;
        }
    }
    if !author_ids.contains(&player_id) {
        return None;
    }
    let manifests = room.get("artifact_manifests")?.as_array()?;
    let proposals = room.get("proposals")?.as_array()?;
    let decisions = room.get("decisions")?.as_array()?;
    let reviews = room.get("section_reviews")?.as_array()?;
    if manifests.len() > 4_096
        || proposals.len() > 4_096
        || decisions.len() > 4_096
        || reviews.len() > 4_096
    {
        return None;
    }

    let mut manifest_ids = HashSet::with_capacity(manifests.len());
    for manifest in manifests {
        if canonical_uuid_value(manifest.get("paper_project_id")?)? != paper_id {
            return None;
        }
        let manifest_id = canonical_uuid_value(manifest.get("manifest_id")?)?;
        if !manifest_ids.insert(manifest_id) {
            return None;
        }
    }

    let mut seen_proposals = HashSet::with_capacity(proposals.len());
    let mut accepted_proposals = Vec::new();
    for proposal in proposals {
        if canonical_uuid_value(proposal.get("paper_project_id")?)? != paper_id {
            return None;
        }
        let proposal_id = canonical_uuid_value(proposal.get("proposal_id")?)?;
        if !seen_proposals.insert(proposal_id) {
            return None;
        }
        let status = proposal.get("status")?.as_str()?;
        if !matches!(
            status,
            "submitted" | "accepted" | "rework" | "rejected" | "superseded"
        ) {
            return None;
        }
        if status == "accepted" {
            let manifest_id = canonical_uuid_value(proposal.get("artifact_manifest_id")?)?;
            if !manifest_ids.contains(&manifest_id) {
                return None;
            }
            accepted_proposals.push((proposal_id, manifest_id));
        }
    }

    let mut seen_decisions = HashSet::with_capacity(decisions.len());
    let mut decisions_by_proposal = HashMap::new();
    for decision in decisions {
        if canonical_uuid_value(decision.get("paper_project_id")?)? != paper_id {
            return None;
        }
        let decision_id = canonical_uuid_value(decision.get("decision_id")?)?;
        if !seen_decisions.insert(decision_id) {
            return None;
        }
        let proposal_id = canonical_uuid_value(decision.get("proposal_id")?)?;
        let decision_player_id = canonical_uuid_value(decision.get("player_id")?)?;
        let kind = decision.get("decision")?.as_str()?;
        if !matches!(kind, "accept" | "rework" | "reject") {
            return None;
        }
        if !seen_proposals.contains(&proposal_id) || !author_ids.contains(&decision_player_id) {
            return None;
        }
        decisions_by_proposal
            .entry(proposal_id)
            .or_insert_with(Vec::new)
            .push((decision_player_id, kind));
    }

    let mut artifacts = Vec::new();
    let mut credited_manifest_ids = HashSet::new();
    for (proposal_id, manifest_id) in accepted_proposals {
        let decisions = decisions_by_proposal.get(&proposal_id)?;
        if decisions.len() != 1
            || decisions[0].1 != "accept"
            || !credited_manifest_ids.insert(manifest_id)
        {
            return None;
        }
        if decisions[0].0 == player_id {
            artifacts.push(manifest_id);
        }
    }

    let mut seen_reviews = HashSet::with_capacity(reviews.len());
    let mut accepted_reviews = Vec::new();
    for review in reviews {
        if canonical_uuid_value(review.get("paper_project_id")?)? != paper_id {
            return None;
        }
        let review_id = canonical_uuid_value(review.get("review_id")?)?;
        if !seen_reviews.insert(review_id) {
            return None;
        }
        let reviewer_id = canonical_uuid_value(review.get("reviewer_player_id")?)?;
        let verdict = review.get("verdict")?.as_str()?;
        if !matches!(verdict, "approve" | "rework" | "reject") {
            return None;
        }
        if verdict == "approve" {
            if !author_ids.contains(&reviewer_id) {
                return None;
            }
            if reviewer_id == player_id {
                accepted_reviews.push(review_id);
            }
        }
    }

    artifacts.sort_unstable();
    artifacts.dedup();
    accepted_reviews.sort_unstable();
    accepted_reviews.dedup();
    Some((
        artifacts
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        accepted_reviews
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
    ))
}

fn contribution_ref_records(artifacts: &[String], reviews: &[String]) -> String {
    let artifacts = artifacts
        .iter()
        .map(|manifest_id| {
            format!(
                r#"<span class="contribution-artifact-ref" data-manifest-id="{}"></span>"#,
                escape(manifest_id),
            )
        })
        .collect::<String>();
    let reviews = reviews
        .iter()
        .map(|review_id| {
            format!(
                r#"<span class="contribution-review-ref" data-review-id="{}"></span>"#,
                escape(review_id),
            )
        })
        .collect::<String>();
    format!(r#"<span class="contribution-ref-records" hidden>{artifacts}{reviews}</span>"#)
}

fn promote_release_form(
    paper_id: &str,
    paper_version: u64,
    paper: &Value,
    room: &Value,
    members: &[Value],
    revision: &Value,
) -> String {
    let revision_id = scalar(revision.get("revision_id"));
    let revision_version = revision.get("version").and_then(Value::as_u64).unwrap_or(0);
    let title = scalar(paper.get("title"));
    let compact_hash = room
        .get("team")
        .and_then(|team| team.get("collaboration_compact_hash"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let protocol_hash = latest_field(
        room_records(room, "experiment_plans"),
        "protocol_snapshot_hash",
    )
    .unwrap_or("");
    let ethics_hash = room_records(room, "artifact_manifests")
        .iter()
        .rev()
        .find_map(|manifest| artifact_role_hash(manifest, "ethics_disclosure"))
        .unwrap_or_default();
    let ai_hash = room_records(room, "artifact_manifests")
        .iter()
        .rev()
        .find_map(|manifest| artifact_role_hash(manifest, "ai_agent_disclosure"))
        .unwrap_or_default();
    let ethics_control = if ethics_hash.is_empty() {
        r#"<label>Ethics disclosure / 伦理披露<textarea name="ethics_disclosure_text" rows="3" minlength="3" maxlength="8000" required placeholder="Describe applicable approvals, safeguards, or why none apply."></textarea></label>"#.to_string()
    } else {
        format!(
            r#"<input name="ethics_disclosure_hash" type="hidden" value="{}"><p class="muted">Ethics disclosure comes from the registered artifact manifest.</p>"#,
            escape(&ethics_hash),
        )
    };
    let ai_control = if ai_hash.is_empty() {
        r#"<label>AI assistance disclosure / AI 使用披露<textarea name="ai_disclosure_text" rows="3" minlength="3" maxlength="8000" required placeholder="Describe any AI assistance, or state that none was used."></textarea></label>"#.to_string()
    } else {
        format!(
            r#"<input name="ai_disclosure_hash" type="hidden" value="{}"><p class="muted">AI disclosure comes from the registered artifact manifest.</p>"#,
            escape(&ai_hash),
        )
    };
    let mut author_fields = String::new();
    for member in members {
        let player_id = scalar(member.get("player_id"));
        let Some((artifacts, reviews)) =
            authoritative_contribution_refs(room, paper_id, &player_id)
        else {
            return unavailable("authoritative contribution milestones");
        };
        let slot = scalar(member.get("participant_slot"));
        let role = scalar(member.get("role"));
        let credit_roles = match role.as_str() {
            "captain" => "conceptualization,project_administration,writing_review_editing",
            "evidence" => "investigation,data_curation,validation,writing_review_editing",
            "experiment" => "methodology,software,validation",
            _ => "investigation,writing_review_editing",
        };
        author_fields.push_str(&format!(
            r#"<fieldset class="release-author" data-player-id="{}" data-participant-slot="{}"><legend>Slot {} · {}</legend><label>Registered display name / 注册昵称<input name="display_name" maxlength="120" required></label><label>CRediT roles / 贡献角色<input name="credit_roles" value="{}" required></label>{}</fieldset>"#,
            escape(&player_id),
            escape(&slot),
            escape(&slot),
            escape(&role),
            escape(credit_roles),
            contribution_ref_records(&artifacts, &reviews),
        ));
    }
    format!(
        r#"<form class="promote-release-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-revision-version="{}" data-collaboration-compact-hash="{}"><h3>Promote the exact release / 提升精确发布候选</h3><p class="muted">The browser derives accepted-artifact and approving-review milestones from this authoritative Room, applies the capped provisional contribution budget, freezes its hash into the release candidate, then immediately submits the same ledger. Splitting records cannot mint extra milestone points. No player enters a ledger UUID, reference, JSON, or digest.</p><label>Title / 标题<input name="title" value="{}" maxlength="200" required></label><label>Abstract / 摘要<textarea name="abstract_text" rows="5" minlength="20" required></textarea></label><label>License / 许可证<input name="license" value="CC-BY-4.0" required></label><fieldset><legend>Frozen disclosures / 锁定披露</legend><input name="research_protocol_snapshot_hash" type="hidden" value="{}"><p class="muted">The research protocol is taken from the frozen experiment plan.</p>{}<label>Conflict of interest / 利益冲突披露<textarea name="coi_disclosure_text" rows="3" minlength="3" maxlength="8000" required placeholder="Declare competing interests, or state that none exist."></textarea></label>{}</fieldset><fieldset><legend>Exact author roster / 精确作者阵容</legend>{}</fieldset><button type="submit">Preserve disclosures + promote / 保存披露并提升候选</button><output></output></form>"#,
        escape(paper_id),
        paper_version,
        escape(&revision_id),
        revision_version,
        escape(compact_hash),
        escape(&title),
        escape(protocol_hash),
        ethics_control,
        ai_control,
        author_fields,
    )
}

fn release_approval_controls(
    paper_id: &str,
    paper_version: u64,
    room: &Value,
    review: Option<&Value>,
    members: &[Value],
    revision: &Value,
    current_player_id: &str,
) -> String {
    let revision_id = scalar(revision.get("revision_id"));
    let release_hash = scalar(revision.get("release_candidate_hash"));
    let release_candidate = revision
        .get("release_candidate")
        .filter(|candidate| candidate.is_object());
    let expected_ledger_hash = release_candidate
        .and_then(|candidate| candidate.get("contribution_ledger_hash"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let ledgers = review
        .map(|value| room_records(value, "contribution_ledgers"))
        .unwrap_or(&[]);
    let matching_ledger = ledgers.iter().find(|ledger| {
        ledger.get("paper_project_id").and_then(Value::as_str) == Some(paper_id)
            && ledger.get("release_candidate_hash").and_then(Value::as_str)
                == Some(release_hash.as_str())
            && ledger.get("ledger_hash").and_then(Value::as_str) == Some(expected_ledger_hash)
    });
    if matching_ledger.is_none() {
        let paper_ledger = ledgers.iter().find(|ledger| {
            ledger.get("paper_project_id").and_then(Value::as_str) == Some(paper_id)
        });
        let ledger_control = if review.is_none() {
            unavailable("authoritative contribution ledger state")
        } else if expected_ledger_hash.is_empty() || release_candidate.is_none() {
            r#"<p class="status missing">Release candidate contribution budget is malformed. Signing remains locked / 发布候选贡献预算异常，签署保持锁定。</p>"#.into()
        } else if paper_ledger.is_some() {
            r#"<p class="status missing">A frozen paper ledger exists but does not match this release candidate. Signing remains locked for integrity review / 已存在账本与发布候选不匹配，签署保持锁定并进入完整性审查。</p>"#.into()
        } else {
            contribution_ledger_repair_form(
                paper_id,
                paper_version,
                &revision_id,
                &release_hash,
                expected_ledger_hash,
                release_candidate.expect("checked release candidate"),
                room,
            )
        };
        return format!(
            r#"<article class="release-approval ledger-gated"><span class="pill">release candidate · ledger pending</span><p><code>{}</code></p><h3>Freeze the deterministic contribution ledger / 锁定确定性贡献账本</h3><p>Consent and finalization remain unavailable until the exact candidate-bound ledger is authoritative.</p>{}</article>"#,
            escape(&release_hash),
            ledger_control,
        );
    }
    let consents = room_records(room, "authorship_consents");
    let current_signed = consents.iter().any(|consent| {
        consent.get("player_id").and_then(Value::as_str) == Some(current_player_id)
            && consent.get("revision_id").and_then(Value::as_str) == Some(revision_id.as_str())
            && consent
                .get("release_candidate_hash")
                .and_then(Value::as_str)
                == Some(release_hash.as_str())
    });
    let consent_form = if current_signed {
        "<p class=\"status\">Your signature is recorded / 你的签名已记录</p>".to_string()
    } else {
        format!(
            r#"<form class="author-consent-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-release-candidate-hash="{}"><h3>Sign this exact release / 签署精确发布</h3><p>Uses the non-exportable human key loaded in this tab.</p><button type="submit">Review hash and sign / 确认哈希并签名</button><output></output></form>"#,
            escape(paper_id),
            paper_version,
            escape(&revision_id),
            escape(&release_hash),
        )
    };
    let roster_ids = members
        .iter()
        .filter_map(|member| member.get("player_id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let signed_ids = consents
        .iter()
        .filter(|consent| {
            consent.get("revision_id").and_then(Value::as_str) == Some(revision_id.as_str())
                && consent
                    .get("release_candidate_hash")
                    .and_then(Value::as_str)
                    == Some(release_hash.as_str())
        })
        .filter_map(|consent| consent.get("player_id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let all_signed = !roster_ids.is_empty()
        && roster_ids
            .iter()
            .all(|player_id| signed_ids.contains(player_id));
    let finalize = if all_signed {
        format!(
            r#"<form class="finalize-paper-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-release-candidate-hash="{}"><h3>Freeze the PaperBundle / 锁定论文包</h3><p>All {} authors signed the same candidate hash.</p><button type="submit">Finalize Author Raid / 完成作者远征</button><output></output></form>"#,
            escape(paper_id),
            paper_version,
            escape(&revision_id),
            escape(&release_hash),
            members.len(),
        )
    } else {
        format!(
            "<p class=\"muted\">{} of {} author signatures recorded / 已记录 {} / {} 位作者签名</p>",
            signed_ids.len(),
            members.len(),
            signed_ids.len(),
            members.len(),
        )
    };
    format!(
        r#"<article class="release-approval"><span class="pill">release candidate</span><p><code>{}</code></p>{}{}</article>"#,
        escape(&release_hash),
        consent_form,
        finalize,
    )
}

fn contribution_ledger_repair_form(
    paper_id: &str,
    paper_version: u64,
    revision_id: &str,
    release_hash: &str,
    expected_ledger_hash: &str,
    release_candidate: &Value,
    room: &Value,
) -> String {
    let Some(authors) = release_candidate.get("authors").and_then(Value::as_array) else {
        return unavailable("frozen release author roster");
    };
    let mut author_records = String::new();
    for author in authors {
        let player_id = scalar(author.get("player_id"));
        let Some(credit_roles) = author.get("credit_roles").and_then(Value::as_array) else {
            return unavailable("frozen release credit roles");
        };
        if player_id.is_empty() || credit_roles.is_empty() {
            return unavailable("complete frozen release contribution roster");
        }
        let Some((artifacts, reviews)) =
            authoritative_contribution_refs(room, paper_id, &player_id)
        else {
            return unavailable("authoritative contribution milestones");
        };
        let roles = credit_roles
            .iter()
            .filter_map(Value::as_str)
            .map(|role| {
                format!(
                    r#"<span class="ledger-credit-role" data-role="{}"></span>"#,
                    escape(role),
                )
            })
            .collect::<String>();
        if roles.is_empty() {
            return unavailable("complete frozen release credit roles");
        }
        author_records.push_str(&format!(
            r#"<span class="ledger-author" data-player-id="{}">{}{}</span>"#,
            escape(&player_id),
            roles,
            contribution_ref_records(&artifacts, &reviews),
        ));
    }
    if author_records.is_empty() {
        return unavailable("frozen release contribution roster");
    }
    format!(
        r#"<form class="freeze-contribution-ledger-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-release-candidate-hash="{}" data-expected-ledger-hash="{}"><p class="muted">Safe after a lost promote response or browser restart: the browser reconstructs the ledger UUID, capped milestone references, and hash from the frozen paper, revision, release authors, and authoritative Room. Any drift from the release-bound hash fails closed.</p><div class="ledger-author-records" hidden>{}</div><button type="submit">Repair / freeze exact ledger / 修复并锁定精确账本</button><output></output></form>"#,
        escape(paper_id),
        paper_version,
        escape(revision_id),
        escape(release_hash),
        escape(expected_ledger_hash),
        author_records,
    )
}

pub fn review_queue(identity: &AlphaIdentity, queue: ReadState<'_>) -> Response {
    let queue_cards = review_queue_cards(identity, queue);
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">REVIEWER RAID · 独立评审</span><h1>Frozen-bundle Review Queue</h1><p>Author Raid ends at its frozen handoff checkpoint. Independent evaluators, reviewers, and reproducers enter through this separate authority boundary.</p></section>
        <section class="panel review-boundary"><div><span class="pill">Independent authority / 独立权威</span><h2>Claim a precise role, never an Author Room</h2><p>Only a frozen, submission-ready PaperBundle is listed. Claiming creates a time-bounded assignment; it never grants team membership or access to an in-progress Paper Room.</p></div><dl><div><dt>Queue status</dt><dd>{}</dd></div><div><dt>Signed in as</dt><dd>{}</dd></div></dl></section>
        <section class="review-queue-grid">{}</section>
        <section class="panel"><span class="pill">PLAYER-SIGNED QUORUM</span><h2>Evaluation → two independent attestations → reproduction</h2><p>Each assigned actor sees only the frozen bundle and their own active assignment. Evaluation drafts and reviewer votes are immutable, locally human-signed records; the evaluator can finalize only after both reviewer slots attest to the exact same signing hash.</p></section>"#,
        escape(queue.label()),
        escape(&identity.display_name),
        queue_cards,
    );
    page(
        "Paper Raid Independent Review Queue",
        &identity.display_name,
        &body,
        true,
    )
}

fn review_queue_cards(identity: &AlphaIdentity, queue: ReadState<'_>) -> String {
    let Some(items) = queue.value().and_then(Value::as_array) else {
        return unavailable_card(
            "Review queue unavailable / 评审队列暂不可用",
            "frozen Review Raid queue",
        );
    };
    if items.is_empty() {
        return r#"<section class="panel narrow review-queue-empty"><span class="pill">No claimable work / 暂无任务</span><h2>No assigned or open frozen bundles</h2><p>The authority returned no eligible Review Raid for this identity. This page never exposes an in-progress author room or fabricates a review task.</p><a class="button" href="/league/start">Refresh authority / 刷新权威状态</a></section>"#.into();
    }

    let mut cards = String::new();
    for item in items {
        let Some(paper_id) = item.get("paper_project_id").and_then(Value::as_str) else {
            continue;
        };
        let title = scalar(item.get("title"));
        let abstract_text = scalar(item.get("abstract_text"));
        let target_format = scalar(item.get("target_format"));
        let submitted_at = scalar(item.get("submitted_at"));
        let author_count = scalar(item.get("author_count"));

        let mut assignment_rows = String::new();
        if let Some(assignments) = item.get("my_assignments").and_then(Value::as_array) {
            for assignment in assignments {
                let slot = scalar(assignment.get("slot"));
                let round = scalar(assignment.get("review_round"));
                let expires_at = scalar(assignment.get("expires_at"));
                assignment_rows.push_str(&format!(
                    r#"<li><div><strong>{}</strong><span>round {} · expires {}</span></div><a class="button" href="/league/review/{}">Resume frozen bundle / 继续评审</a></li>"#,
                    escape(&review_slot_label(&slot)),
                    escape(&round),
                    escape(&expires_at),
                    escape(paper_id),
                ));
            }
        }
        if assignment_rows.is_empty() {
            assignment_rows.push_str("<li class=\"muted\">No active assignment / 尚未领取</li>");
        }

        let mut open_slots = String::new();
        if let Some(vacancies) = item.get("open_slots").and_then(Value::as_array) {
            for vacancy in vacancies {
                let Some(slot) = vacancy.get("slot").and_then(Value::as_str) else {
                    continue;
                };
                let Some(round) = vacancy.get("review_round").and_then(Value::as_u64) else {
                    continue;
                };
                if !review_slot_allowed(identity, slot) {
                    continue;
                }
                open_slots.push_str(&format!(
                    r#"<form class="review-claim-form" data-paper-id="{}" data-player-id="{}" data-review-round="{}" data-slot="{}"><div><strong>{}</strong><small>round {} · 24h lease</small></div><button type="submit">Claim / 领取</button><output></output></form>"#,
                    escape(paper_id),
                    escape(&identity.player_id.to_string()),
                    round,
                    escape(slot),
                    escape(&review_slot_label(slot)),
                    round,
                ));
            }
        }
        if open_slots.is_empty() {
            open_slots.push_str("<p class=\"muted\">No open slot matches this identity's configured scope / 当前无符合此身份权限的空位</p>");
        }

        cards.push_str(&format!(
            r#"<article class="panel review-queue-card"><header><div><span class="eyebrow">FROZEN PAPERBUNDLE</span><h2>{}</h2></div><span class="pill">{} authors</span></header><p>{}</p><dl class="review-facts"><div><dt>Target</dt><dd>{}</dd></div><div><dt>Submitted</dt><dd>{}</dd></div><div><dt>Release freeze</dt><dd>Verified / 已验证</dd></div><div><dt>PaperBundle seal</dt><dd>Verified / 已验证</dd></div></dl><div class="review-columns"><section><h3>My assignment / 我的任务</h3><ul class="review-assignments">{}</ul></section><section><h3>Open slots / 可领取角色</h3><div class="review-open-slots">{}</div></section></div></article>"#,
            escape(&title),
            escape(&author_count),
            escape(&abstract_text),
            escape(&target_format),
            escape(&submitted_at),
            assignment_rows,
            open_slots,
        ));
    }
    if cards.is_empty() {
        unavailable_card(
            "Malformed review queue / 队列格式错误",
            "review queue items",
        )
    } else {
        cards
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ReviewArtifactLink {
    logical_path: String,
    role: String,
    digest: String,
    media_type: String,
    object_key: String,
    size_bytes: u64,
}

fn canonical_review_uuid(value: &str) -> Option<String> {
    let parsed = Uuid::parse_str(value).ok()?;
    let canonical = parsed.to_string();
    (canonical == value).then_some(canonical)
}

fn review_object_key_is_safe(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= 128
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn review_logical_path_is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.bytes().any(|byte| byte.is_ascii_control())
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn review_assignment_matches_descriptor(
    assignment: &Value,
    descriptor: &FrozenReviewBundleV1,
    expected_player_id: Uuid,
) -> bool {
    let assignment_id = descriptor.assignment_id.to_string();
    let paper_id = descriptor.paper_project_id.to_string();
    let submission_id = descriptor.submission_id.to_string();
    let player_id = expected_player_id.to_string();
    assignment.get("assignment_id").and_then(Value::as_str) == Some(assignment_id.as_str())
        && assignment.get("paper_project_id").and_then(Value::as_str) == Some(paper_id.as_str())
        && assignment.get("submission_id").and_then(Value::as_str) == Some(submission_id.as_str())
        && assignment.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
        && assignment.get("review_round").and_then(Value::as_u64) == Some(descriptor.review_round)
        && assignment.get("slot").and_then(Value::as_str) == Some(descriptor.slot.as_str())
        && assignment.get("version").and_then(Value::as_u64) == Some(descriptor.assignment_version)
        && assignment.get("expires_at").and_then(Value::as_str)
            == Some(descriptor.expires_at.as_str())
        && matches!(
            assignment.get("status").and_then(Value::as_str),
            Some("claimed" | "pinned")
        )
}

fn review_artifact_links(
    bundle: &Value,
    queue_item: &Value,
    expected_player_id: Uuid,
) -> Option<(String, String, String, Vec<ReviewArtifactLink>)> {
    // The route resolves and verifies this descriptor before rendering. Keep the renderer
    // independently fail-closed so malformed or mismatched JSON can never become a capability URL.
    let descriptor: FrozenReviewBundleV1 =
        serde_json::from_value(bundle.get("resolved_frozen_review_bundle")?.clone()).ok()?;
    verify_frozen_review_bundle(&descriptor).ok()?;
    let paper_id = descriptor.paper_project_id.to_string();
    let submission_id = descriptor.submission_id.to_string();
    if !crate::agent_bridge::review_outer_bundle_matches_authority(bundle, &descriptor.authority)
        || canonical_review_uuid(bundle.get("paper_project_id")?.as_str()?)? != paper_id
        || bundle.get("submission_id")?.as_str()? != submission_id
        || bundle.get("release_candidate_hash")?.as_str()?
            != descriptor.release_candidate_hash.as_str()
        || bundle.get("paper_bundle_hash")?.as_str()? != descriptor.paper_bundle_hash.as_str()
        || queue_item.get("paper_project_id")?.as_str()? != paper_id
        || queue_item.get("submission_id")?.as_str()? != submission_id
        || queue_item.get("release_candidate_hash")?.as_str()?
            != descriptor.release_candidate_hash.as_str()
        || queue_item.get("paper_bundle_hash")?.as_str()? != descriptor.paper_bundle_hash.as_str()
    {
        return None;
    }
    let assignment_id = descriptor.assignment_id.to_string();
    let bundle_hash = descriptor.bundle_hash.clone();

    let assignments = bundle.get("my_assignments")?.as_array()?;
    let queue_assignments = queue_item.get("my_assignments")?.as_array()?;
    if assignments.len() != 1
        || queue_assignments.len() != 1
        || !review_assignment_matches_descriptor(&assignments[0], &descriptor, expected_player_id)
        || !review_assignment_matches_descriptor(
            &queue_assignments[0],
            &descriptor,
            expected_player_id,
        )
    {
        return None;
    }

    let objects = &descriptor.authority.artifact_objects;
    let mut links = Vec::with_capacity(objects.len());
    let mut keys = HashSet::new();
    let mut paths = HashSet::new();
    let mut previous: Option<(String, String)> = None;
    for object in objects {
        let ordering = (object.object_key.clone(), object.logical_path.clone());
        if object.download_path != REVIEW_OBJECT_DOWNLOAD_PATH_V1
            || !review_object_key_is_safe(&object.object_key)
            || !review_logical_path_is_safe(&object.logical_path)
            || !matches!(
                object.role.as_str(),
                "paper_source"
                    | "bibliography"
                    | "claim_evidence_graph"
                    | "candidate"
                    | "dataset"
                    | "evaluator_support"
                    | "frozen_evaluator"
            )
            || crate::cas::raw_sha256(&object.digest).is_err()
            || crate::cas::validate_media_type(&object.media_type).is_err()
            || object.size_bytes == 0
            || object.size_bytes > 16 * 1024 * 1024
            || previous.as_ref().is_some_and(|prior| prior >= &ordering)
            || !keys.insert(object.object_key.clone())
            || !paths.insert(object.logical_path.clone())
        {
            return None;
        }
        previous = Some(ordering);
        links.push(ReviewArtifactLink {
            logical_path: object.logical_path.clone(),
            role: object.role.clone(),
            digest: object.digest.clone(),
            media_type: object.media_type.clone(),
            object_key: object.object_key.clone(),
            size_bytes: object.size_bytes,
        });
    }
    Some((paper_id, assignment_id, bundle_hash, links))
}

fn review_artifact_role_label(role: &str) -> &'static str {
    match role {
        "paper_source" => "Paper source / 论文正文",
        "bibliography" => "Bibliography / 参考文献",
        "claim_evidence_graph" => "Claim/evidence graph / 主张证据图",
        "frozen_evaluator" => "Frozen evaluator / 冻结评估器",
        "evaluator_support" => "Evaluator support / 评估器依赖",
        "dataset" => "Challenge dataset / 挑战数据集",
        "candidate" => "Candidate result / 候选结果",
        _ => "Frozen review file / 冻结评审文件",
    }
}

fn review_artifact_url(
    paper_id: &str,
    assignment_id: &str,
    bundle_hash: &str,
    artifact: &ReviewArtifactLink,
    presentation: &str,
) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("assignment_id", assignment_id);
    serializer.append_pair("bundle_hash", bundle_hash);
    serializer.append_pair("object_key", &artifact.object_key);
    serializer.append_pair("presentation", presentation);
    let query = serializer.finish();
    format!(
        "/api/review/papers/{paper_id}/artifacts/{}?{query}",
        artifact.digest
    )
}

fn review_download_filename(logical_path: &str) -> String {
    let filename = logical_path.rsplit('/').next().unwrap_or_default();
    let sanitized = filename
        .chars()
        .take(128)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() || matches!(sanitized.as_str(), "." | "..") {
        "paper-raid-frozen-review-object".to_string()
    } else {
        sanitized
    }
}

fn review_artifact_library(identity: &AlphaIdentity, queue_item: &Value, bundle: &Value) -> String {
    let Some((paper_id, assignment_id, bundle_hash, artifacts)) =
        review_artifact_links(bundle, queue_item, identity.player_id)
    else {
        return r#"<section class="panel review-artifact-library unavailable" data-review-artifacts-state="unavailable"><span class="pill">FAIL CLOSED</span><h2>Frozen review files unavailable / 冻结评审文件不可用</h2><p>The assignment-scoped frozen artifact descriptor could not be verified. No file capability is exposed.</p></section>"#.to_string();
    };
    let mut items = String::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        let open_url =
            review_artifact_url(&paper_id, &assignment_id, &bundle_hash, artifact, "inline");
        let download_url = review_artifact_url(
            &paper_id,
            &assignment_id,
            &bundle_hash,
            artifact,
            "attachment",
        );
        items.push_str(&format!(
            r#"<li class="review-artifact-item" data-review-artifact-index="{}" data-review-artifact-role="{}"><div><strong class="review-artifact-filename">{}</strong><span>{}</span><small class="review-artifact-media-type">{} · {} bytes</small></div><div class="review-artifact-actions"><a class="button review-artifact-open" data-review-artifact-action="open" href="{}" target="_blank" rel="noopener noreferrer">Open / 打开</a><a class="button review-artifact-download" data-review-artifact-action="download" href="{}" download="{}">Download / 下载</a></div></li>"#,
            index,
            escape(&artifact.role),
            escape(&artifact.logical_path),
            escape(review_artifact_role_label(&artifact.role)),
            escape(&artifact.media_type),
            artifact.size_bytes,
            escape(&open_url),
            escape(&download_url),
            escape(&review_download_filename(&artifact.logical_path)),
        ));
    }
    format!(
        r#"<section class="panel review-artifact-library" data-review-artifacts-state="available" data-review-artifact-count="{}"><span class="pill">ASSIGNMENT-SCOPED FILES / 任务限定文件</span><h2>Frozen review artifacts / 冻结评审工件</h2><p>Each link is derived from this active assignment and exact frozen bundle. Opening or downloading revalidates the assignment, bundle, object and CAS media type; these links never grant Author Room access.</p><ul class="review-artifact-list">{items}</ul></section>"#,
        artifacts.len(),
    )
}

pub fn review_bundle(
    identity: &AlphaIdentity,
    queue_item: &Value,
    submission: &Value,
    review_state: ReadState<'_>,
    receipt_projection: &Value,
) -> Response {
    let paper_id = scalar(submission.get("paper_project_id"));
    let status = scalar(submission.get("status"));
    let candidate = submission
        .get("paper_bundle")
        .and_then(|bundle| bundle.get("release_candidate"));
    let Some(candidate) = candidate else {
        return page(
            "Paper Raid Frozen Review Bundle",
            &identity.display_name,
            &unavailable_card(
                "Frozen bundle unavailable / 冻结论文包不可用",
                "review bundle",
            ),
            true,
        );
    };
    let mut authors = String::new();
    if let Some(records) = candidate.get("authors").and_then(Value::as_array) {
        for author in records {
            authors.push_str(&format!(
                r#"<li><strong>{}</strong><span>slot {} · {}</span></li>"#,
                escape(&scalar(author.get("display_name"))),
                escape(&scalar(author.get("participant_slot"))),
                escape(
                    &author
                        .get("credit_roles")
                        .and_then(Value::as_array)
                        .map(|roles| roles
                            .iter()
                            .map(|role| scalar(Some(role)))
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_else(|| "role unavailable".into())
                ),
            ));
        }
    }
    if authors.is_empty() {
        authors.push_str("<li class=\"muted\">Author roster unavailable</li>");
    }
    let assignments = submission
        .get("my_assignments")
        .or_else(|| queue_item.get("my_assignments"))
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .map(|assignment| {
                    format!(
                        r#"<li><strong>{}</strong><span>round {} · expires {}</span></li>"#,
                        escape(&review_slot_label(&scalar(assignment.get("slot")))),
                        escape(&scalar(assignment.get("review_round"))),
                        escape(&scalar(assignment.get("expires_at"))),
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_default();
    let raid_controls =
        review_raid_controls(identity, submission, review_state, receipt_projection);
    let artifact_library = review_artifact_library(identity, queue_item, submission);
    let authority_revision =
        review_authority_revision(queue_item, submission, review_state, receipt_projection);
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">REVIEW RAID · FROZEN BUNDLE</span><h1>{}</h1><p>{}</p><a class="button" href="/league/review">Back to queue / 返回评审队列</a></section>
        <section class="panel review-authority-watch" data-paper-id="{}" data-authority-revision="{}"><span class="pill">LIVE REVIEW AUTHORITY / 实时评审权威</span><h2>Current frozen assignment / 当前冻结任务</h2><p>This page checks the authoritative Review state in the background. If another participant advances the Raid, old controls are disabled and the current action is reloaded automatically.</p><div class="live-status"><span class="review-authority-connection" data-state="connecting">Connecting / 正在连接</span></div><button class="review-authority-refresh" type="button">Sync now / 立即同步</button><output class="review-authority-detail"></output></section>
        <section class="grid"><article class="panel"><h2>Your immutable assignment / 你的不可变任务</h2><ul class="review-assignments">{}</ul></article><article class="panel"><h2>Authority facts / 权威事实</h2><dl class="review-facts"><div><dt>Status</dt><dd>{}</dd></div><div><dt>Submission</dt><dd>Frozen and verified / 已冻结验证</dd></div><div><dt>Paper scope</dt><dd>Assignment-scoped / 仅限当前任务</dd></div></dl></article></section>
        <section class="panel"><span class="pill">FROZEN PAPERBUNDLE</span><h2>Review target / 评审对象</h2><dl class="review-facts"><div><dt>Target format</dt><dd>{}</dd></div><div><dt>License</dt><dd>{}</dd></div><div><dt>Release freeze</dt><dd>Verified / 已验证</dd></div><div><dt>PaperBundle seal</dt><dd>Verified / 已验证</dd></div><div><dt>Source record</dt><dd>Verified / 已验证</dd></div><div><dt>Artifact set</dt><dd>Verified / 已验证</dd></div><div><dt>Bibliography</dt><dd>Verified / 已验证</dd></div><div><dt>Claim/evidence graph</dt><dd>Verified / 已验证</dd></div></dl><h3>Frozen author roster / 冻结作者阵容</h3><ul class="review-assignments">{}</ul></section>
        {}{}"#,
        escape(&scalar(candidate.get("title"))),
        escape(&scalar(candidate.get("abstract_text"))),
        escape(&paper_id),
        escape(&authority_revision),
        assignments,
        escape(&status),
        escape(&scalar(candidate.get("target_format"))),
        escape(&scalar(candidate.get("license"))),
        authors,
        artifact_library,
        raid_controls,
    );
    page(
        "Paper Raid Frozen Review Bundle",
        &identity.display_name,
        &body,
        true,
    )
}

fn review_authority_revision(
    queue_item: &Value,
    submission: &Value,
    review_state: ReadState<'_>,
    receipt_projection: &Value,
) -> String {
    let review = review_state
        .value()
        .cloned()
        .unwrap_or_else(|| Value::String(review_state.label().to_string()));
    let snapshot = serde_json::json!({
        "schema": "hepta.paper_raid.review_authority_watch.v1",
        "queue_item": queue_item,
        "submission": submission,
        "review_state": review,
        "receipt_projection": receipt_projection,
    });
    let bytes = serde_json::to_vec(&snapshot).unwrap_or_default();
    sha256_digest(&bytes)
        .strip_prefix("sha256:")
        .unwrap_or_default()
        .to_string()
}

fn has_review_assignment(bundle: &Value, identity: &AlphaIdentity, slots: &[&str]) -> bool {
    let player_id = identity.player_id.to_string();
    bundle
        .get("my_assignments")
        .and_then(Value::as_array)
        .is_some_and(|assignments| {
            assignments.iter().any(|assignment| {
                assignment.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
                    && assignment
                        .get("slot")
                        .and_then(Value::as_str)
                        .is_some_and(|slot| slots.contains(&slot))
            })
        })
}

fn review_raid_controls(
    identity: &AlphaIdentity,
    bundle: &Value,
    review_state: ReadState<'_>,
    receipt_projection: &Value,
) -> String {
    let paper_id = scalar(bundle.get("paper_project_id"));
    let evaluator = has_review_assignment(bundle, identity, &["evaluator"]);
    let reviewer = has_review_assignment(bundle, identity, &["reviewer_1", "reviewer_2"]);
    let reproducer = has_review_assignment(bundle, identity, &["reproducer"]);
    let quorum = bundle
        .get("evaluation_quorum")
        .filter(|value| !value.is_null());
    let evaluation = bundle.get("evaluation").filter(|value| !value.is_null());

    let mut controls = String::new();
    controls.push_str(
        r#"<section class="panel review-mission"><span class="pill">YOUR NEXT ACTION / 你的下一步</span><h2>Independent Review Raid</h2><p>Every form below signs an exact server-derived frame with the human key loaded in this tab. Frozen PaperBundle identifiers, release hashes, panel membership, draft hashes, and tolerance policy hashes are authoritative self-reads—not editable fields.</p></section>"#,
    );

    match (quorum, evaluation) {
        (None, None) if evaluator => controls.push_str(&review_receipt_confirmation_card(
            &paper_id,
            "evaluate",
            receipt_projection,
        )),
        (None, None) if reviewer => controls.push_str(
            r#"<section class="panel"><span class="pill">WAITING FOR EVALUATOR</span><h2>No immutable evaluation draft yet</h2><p>Reload after the assigned evaluator freezes the score, hard gates, reference metric, and tolerance policy. You will attest to that exact signing hash without receiving a copied signature.</p></section>"#,
        ),
        (None, None) if reproducer => controls.push_str(
            r#"<section class="panel"><span class="pill">WAITING FOR QUORUM</span><h2>Reproduction opens after evaluation finalizes</h2><p>The assigned reproducer cannot act until the evaluator draft receives both independent reviewer attestations and is finalized.</p></section>"#,
        ),
        (Some(quorum), None) => {
            controls.push_str(&evaluation_quorum_status(quorum));
            let null_draft = Value::Null;
            let draft = quorum.get("draft").unwrap_or(&null_draft);
            let evaluation_id = scalar(draft.get("evaluation_id"));
            if evaluator
                && quorum
                    .get("ready_to_finalize")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                controls.push_str(&format!(
                    r#"<section class="panel"><span class="pill">QUORUM READY</span><h2>Finalize the immutable evaluation</h2><p>Both independent reviewer slots are bound to the exact draft. Finalization re-verifies every signature and cannot change any signed score or tolerance fact.</p><form class="review-finalize-form" data-paper-id="{}" data-evaluation-id="{}" data-draft-version="{}"><button type="submit">Finalize evaluation / 完成评估</button><output></output></form></section>"#,
                    escape(&paper_id),
                    escape(&evaluation_id),
                    escape(&scalar(draft.get("version"))),
                ));
            } else if evaluator {
                controls.push_str(
                    r#"<section class="panel"><h2>Await both reviewer slots / 等待两位评审</h2><p>The draft is immutable. Finalize appears only when the authority reports an active two-slot quorum.</p></section>"#,
                );
            }
            if reviewer {
                let player_id = identity.player_id.to_string();
                let already_attested = quorum
                    .get("attestations")
                    .and_then(Value::as_array)
                    .is_some_and(|records| {
                        records.iter().any(|record| {
                            record
                                .get("attestation")
                                .and_then(|value| value.get("reviewer_player_id"))
                                .and_then(Value::as_str)
                                == Some(player_id.as_str())
                        })
                    });
                if already_attested {
                    controls.push_str(
                        r#"<section class="panel"><span class="pill">ATTESTATION RECORDED</span><h2>Your immutable reviewer decision is authoritative</h2><p>Reload and lost-response recovery use the quorum read model; no signature data needs to be copied or resubmitted.</p></section>"#,
                    );
                } else {
                    controls.push_str(&review_attestation_form(&paper_id, &evaluation_id, draft));
                }
            }
        }
        (_, Some(evaluation)) => {
            controls.push_str(&evaluation_result(evaluation, review_state));
            if reproducer {
                controls.push_str(&reproduction_controls(
                    identity,
                    &paper_id,
                    evaluation,
                    review_state,
                    receipt_projection,
                ));
                controls.push_str(&appeal_resolution_controls(
                    identity,
                    &paper_id,
                    bundle,
                    evaluation,
                    review_state,
                ));
            }
        }
        _ => controls.push_str(
            r#"<section class="panel"><span class="pill">FAIL CLOSED</span><h2>Assignment state is incomplete</h2><p>No review action is exposed until the authority returns one exact active assignment and its current immutable round state.</p></section>"#,
        ),
    }
    controls
}

fn receipt_fact_rows(value: Option<&Value>, suffix: &str) -> String {
    value
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| {
                    format!(
                        "<li><strong>{}</strong><span>{} {}</span></li>",
                        escape(key),
                        escape(&scalar(Some(value))),
                        escape(suffix),
                    )
                })
                .collect::<String>()
        })
        .filter(|rows| !rows.is_empty())
        .unwrap_or_else(|| "<li class=\"muted\">No verified values</li>".into())
}

fn evaluation_confirmation_fields() -> &'static str {
    r#"<fieldset><legend>Human evaluation score / 人工评估评分</legend><p class="muted">These seven judgments are authored by the assigned human evaluator and signed with the exact receipt-derived metrics. They are not evaluator-process output.</p><div class="form-grid"><label>Method rigor (0–2500 bps)<input name="method_rigor_bps" type="number" min="0" max="2500" step="1" required></label><label>Experiment &amp; statistics (0–1500 bps)<input name="experiment_statistics_bps" type="number" min="0" max="1500" step="1" required></label><label>Reproducibility (0–1500 bps)<input name="reproducibility_bps" type="number" min="0" max="1500" step="1" required></label><label>Evidence &amp; citations (0–1500 bps)<input name="evidence_citations_bps" type="number" min="0" max="1500" step="1" required></label><label>Value &amp; originality (0–1500 bps)<input name="value_originality_bps" type="number" min="0" max="1500" step="1" required></label><label>Argument &amp; expression (0–1000 bps)<input name="argument_expression_bps" type="number" min="0" max="1000" step="1" required></label><label>Ethics &amp; transparency (0–500 bps)<input name="ethics_transparency_bps" type="number" min="0" max="500" step="1" required></label></div></fieldset><fieldset><legend>Human-observable hard gates / 人工确认硬门</legend><p class="muted">Choose Yes or No for each observable fact. Author consent and artifact-lineage completeness are derived from the verified Hepta PaperBundle/ArtifactManifest authority and cannot be edited here.</p><div class="form-grid"><label>Citations and data authentic<select name="gate_citations_and_data_authentic" required><option value="">Choose…</option><option value="true">Yes</option><option value="false">No</option></select></label><label>Failed runs disclosed<select name="gate_failed_runs_disclosed" required><option value="">Choose…</option><option value="true">Yes</option><option value="false">No</option></select></label><label>Core claims have evidence<select name="gate_core_claims_have_evidence" required><option value="">Choose…</option><option value="true">Yes</option><option value="false">No</option></select></label><label>License, ethics and COI complete<select name="gate_license_ethics_coi_complete" required><option value="">Choose…</option><option value="true">Yes</option><option value="false">No</option></select></label></div></fieldset>"#
}

fn review_receipt_confirmation_card(
    paper_id: &str,
    expected_kind: &str,
    projection: &Value,
) -> String {
    let receipt = projection
        .get("receipt")
        .filter(|_| projection.get("status").and_then(Value::as_str) == Some("available"))
        .filter(|receipt| receipt.get("kind").and_then(Value::as_str) == Some(expected_kind));
    let Some(receipt) = receipt else {
        return format!(
            r#"<section class="panel"><span class="pill">WAITING FOR BRIDGE RECEIPT</span><h2>{}</h2><p>The paired Bridge must execute the exact frozen bundle first. Metrics, seeds, environment and run facts cannot be typed or pasted into this page. Reload after the signed receipt arrives.</p></section>"#,
            if expected_kind == "evaluate" {
                "Evaluation execution pending / 等待评估执行"
            } else {
                "Independent reproduction pending / 等待独立复现"
            }
        );
    };
    let Some(receipt_id) = receipt.get("receipt_id").and_then(Value::as_str) else {
        return unavailable_card(
            "Receipt unavailable / 执行回执不可用",
            "verified review receipt",
        );
    };
    let output = receipt.get("output").unwrap_or(&Value::Null);
    let (heading, primary, secondary_heading, secondary, rule_count, judgment_fields) =
        if expected_kind == "evaluate" {
            let candidate_passed = output
                .get("candidate_passed")
                .and_then(Value::as_bool)
                .map(|value| if value { "passed" } else { "did not pass" })
                .unwrap_or("unavailable");
            (
            "Confirm frozen evaluator result / 确认冻结评估结果",
            receipt_fact_rows(output.get("reference_metrics_micros"), "micros"),
            "Frozen evaluator outcome",
            format!(
                "<li><strong>Candidate</strong><span>{}</span></li><li><strong>Tolerance policy</strong><span>version {}</span></li>",
                escape(candidate_passed),
                escape(&scalar(output.get("tolerance_policy_version"))),
            ),
            output
                .get("tolerance_rules")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            evaluation_confirmation_fields(),
        )
        } else {
            (
                "Confirm independent reproduction / 确认独立复现",
                receipt_fact_rows(output.get("observed_metrics_micros"), "micros"),
                "Statistical evidence",
                receipt_fact_rows(output.get("statistical_evidence"), "verified evidence"),
                output
                    .get("statistical_evidence")
                    .and_then(Value::as_object)
                    .map_or(0, serde_json::Map::len),
                "",
            )
        };
    let seals = receipt.get("seals").unwrap_or(&Value::Null);
    format!(
        r#"<section class="panel"><span class="pill">SERVER-VERIFIED AGENT RECEIPT</span><h2>{}</h2><p>The values below came only from the signed Bridge execution receipt. This page has no metric, seed, environment or run-manifest input.</p><div class="review-columns"><div><h3>Machine result</h3><ul class="review-assignments">{}</ul></div><div><h3>{}</h3><ul class="review-assignments">{}</ul></div></div><dl class="review-facts"><div><dt>Rules / evidence records</dt><dd>{}</dd></div><div><dt>Release-pinned evaluator</dt><dd>Verified / 已验证</dd></div><div><dt>Receipt signature</dt><dd>Verified / 已验证</dd></div><div><dt>Frozen bundle</dt><dd>Verified / 已验证</dd></div><div><dt>Inputs and outputs</dt><dd>Verified / 已验证</dd></div><div><dt>Metrics and seed set</dt><dd>Verified / 已验证</dd></div><div><dt>Environment and run</dt><dd>Verified / 已验证</dd></div><div><dt>Logs</dt><dd>Verified / 已验证</dd></div><div><dt>Exit code</dt><dd>{}</dd></div></dl><form class="review-receipt-confirm-form" data-paper-id="{}" data-receipt-id="{}" data-kind="{}">{}<label>Conflict-of-interest attestation / 利益冲突声明<textarea name="coi_statement" rows="4" required placeholder="State no conflict, or disclose the exact relationship and mitigation."></textarea></label><button type="submit">Review, sign and confirm / 审阅、签名并确认</button><output></output></form></section>"#,
        escape(heading),
        primary,
        escape(secondary_heading),
        secondary,
        rule_count,
        escape(&scalar(seals.get("exit_code"))),
        escape(paper_id),
        escape(receipt_id),
        escape(expected_kind),
        judgment_fields,
    )
}

fn evaluation_quorum_status(quorum: &Value) -> String {
    let null_draft = Value::Null;
    let draft = quorum.get("draft").unwrap_or(&null_draft);
    let missing = quorum
        .get("missing_slots")
        .and_then(Value::as_array)
        .map(|slots| {
            slots
                .iter()
                .map(|slot| review_slot_label(&scalar(Some(slot))))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none / 无".into());
    let attestations = quorum
        .get("attestations")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut rendered = format!(
        r#"<section class="panel"><span class="pill">IMMUTABLE DRAFT</span><h2>Review quorum / 评审法定人数</h2><dl class="review-facts"><div><dt>Review round</dt><dd>{}</dd></div><div><dt>Draft status</dt><dd>{}</dd></div><div><dt>Attestations</dt><dd>{}/2</dd></div><div><dt>Missing slots</dt><dd>{}</dd></div><div><dt>Assignments active</dt><dd>{}</dd></div><div><dt>Ready to finalize</dt><dd>{}</dd></div><div><dt>Score</dt><dd>{} bps · eligible {}</dd></div></dl></section>"#,
        escape(&scalar(draft.get("review_round"))),
        escape(&scalar(draft.get("status"))),
        attestations,
        escape(&missing),
        escape(&scalar(quorum.get("assignments_active"))),
        escape(&scalar(quorum.get("ready_to_finalize"))),
        escape(&scalar(
            draft
                .get("paper_score")
                .and_then(|value| value.get("score_bps"))
        )),
        escape(&scalar(
            draft
                .get("paper_score")
                .and_then(|value| value.get("eligible"))
        )),
    );
    rendered.push_str(&evaluation_science_facts(draft));
    rendered
}

fn evaluation_science_facts(evaluation: &Value) -> String {
    let null_score = Value::Null;
    let score = evaluation.get("paper_score").unwrap_or(&null_score);
    let components = score
        .get("components")
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| {
                    format!(
                        "<li><strong>{}</strong><span>{} bps</span></li>",
                        escape(key),
                        escape(&scalar(Some(value)))
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "<li class=\"muted\">Unavailable</li>".into());
    let hard_gates = score
        .get("hard_gates")
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| {
                    format!(
                        "<li><strong>{}</strong><span>{}</span></li>",
                        escape(key),
                        escape(&scalar(Some(value)))
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "<li class=\"muted\">Unavailable</li>".into());
    let metrics = evaluation
        .get("reference_metrics_micros")
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| {
                    format!(
                        "<li><strong>{}</strong><span>{} micros</span></li>",
                        escape(key),
                        escape(&scalar(Some(value)))
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "<li class=\"muted\">Unavailable</li>".into());
    let rules = evaluation
        .get("tolerance_policy")
        .and_then(|value| value.get("rules"))
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .map(|rule| {
                    let kind = scalar(rule.get("kind"));
                    let metric = scalar(rule.get("metric"));
                    let detail = match kind.as_str() {
                        "absolute" => {
                            format!("max delta {} micros", scalar(rule.get("max_delta_micros")))
                        }
                        "relative" => {
                            format!("max delta {} bps", scalar(rule.get("max_delta_bps")))
                        }
                        "statistical" => format!(
                            "overlap ≥ {} bps · effect Δ ≤ {} micros · p ≥ {} micros",
                            scalar(rule.get("minimum_interval_overlap_bps")),
                            scalar(rule.get("maximum_effect_delta_micros")),
                            scalar(rule.get("minimum_p_value_micros")),
                        ),
                        "seed" => "exact frozen seed set".into(),
                        _ => "unsupported rule".into(),
                    };
                    format!(
                        "<li><strong>{} {}</strong><span>{}</span></li>",
                        escape(&kind),
                        escape(&metric),
                        escape(&detail)
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "<li class=\"muted\">Unavailable</li>".into());
    format!(
        r#"<section class="panel review-draft-facts"><h2>Exact frozen evaluation facts / 精确冻结评估事实</h2><div class="review-columns"><div><h3>Score components</h3><ul class="review-assignments">{}</ul></div><div><h3>Hard gates</h3><ul class="review-assignments">{}</ul></div><div><h3>Reference metrics</h3><ul class="review-assignments">{}</ul></div><div><h3>Tolerance rules</h3><ul class="review-assignments">{}</ul></div></div></section>"#,
        components, hard_gates, metrics, rules,
    )
}

fn review_attestation_form(paper_id: &str, evaluation_id: &str, draft: &Value) -> String {
    format!(
        r#"<section class="panel"><span class="pill">INDEPENDENT REVIEWER</span><h2>Attest to the exact evaluator draft</h2><dl class="review-facts"><div><dt>Score</dt><dd>{} bps</dd></div><div><dt>Hard-gate eligible</dt><dd>{}</dd></div><div><dt>Tolerance policy</dt><dd>{}</dd></div></dl><form class="review-attestation-form" data-paper-id="{}" data-evaluation-id="{}"><label>Conflict-of-interest attestation / 利益冲突声明<textarea name="coi_statement" rows="4" required placeholder="State no conflict, or disclose the exact relationship and mitigation."></textarea></label><div class="actions"><button type="submit" name="verdict" value="approve">Approve exact draft / 批准</button><button type="submit" name="verdict" value="reject">Reject exact draft / 拒绝</button></div><output></output></form></section>"#,
        escape(&scalar(
            draft
                .get("paper_score")
                .and_then(|value| value.get("score_bps"))
        )),
        escape(&scalar(
            draft
                .get("paper_score")
                .and_then(|value| value.get("eligible"))
        )),
        escape(&scalar(
            draft
                .get("tolerance_policy")
                .and_then(|value| value.get("version"))
        )),
        escape(paper_id),
        escape(evaluation_id),
    )
}

fn evaluation_result(evaluation: &Value, review_state: ReadState<'_>) -> String {
    let finality = review_state.value().and_then(|state| state.get("finality"));
    let mut rendered = format!(
        r#"<section class="panel"><span class="pill">EVALUATION FINALIZED</span><h2>Evaluation result / 评估结果</h2><dl class="review-facts"><div><dt>Evaluation</dt><dd>{}</dd></div><div><dt>Score</dt><dd>{} bps</dd></div><div><dt>Hard-gate eligible</dt><dd>{}</dd></div><div><dt>Settlement</dt><dd>{}</dd></div><div><dt>Chain finality</dt><dd>{}</dd></div><div><dt>Ranking / reward / economic</dt><dd>{} / {} / {}</dd></div></dl><p>Finality and eligibility are the paper-scoped Hepta projection. This page never infers them from the legacy league authority.</p></section>"#,
        escape(&scalar(evaluation.get("status"))),
        escape(&scalar(
            evaluation
                .get("paper_score")
                .and_then(|value| value.get("score_bps"))
        )),
        escape(&scalar(
            evaluation
                .get("paper_score")
                .and_then(|value| value.get("eligible"))
        )),
        escape(&scalar(evaluation.get("settlement_state"))),
        escape(&scalar(finality.and_then(|value| value.get("status")))),
        escape(&scalar(
            finality.and_then(|value| value.get("ranking_eligible"))
        )),
        escape(&scalar(
            finality.and_then(|value| value.get("reward_eligible"))
        )),
        escape(&scalar(
            finality.and_then(|value| value.get("economic_eligible"))
        )),
    );
    rendered.push_str(&evaluation_science_facts(evaluation));
    rendered
}

fn reproduction_controls(
    identity: &AlphaIdentity,
    paper_id: &str,
    evaluation: &Value,
    review_state: ReadState<'_>,
    receipt_projection: &Value,
) -> String {
    let evaluation_id = scalar(evaluation.get("evaluation_id"));
    let player_id = identity.player_id.to_string();
    let Some(state) = review_state.value() else {
        return unavailable_card(
            "Reproduction state unavailable / 复现状态暂不可用",
            "paper-scoped review state",
        );
    };
    if let Some(report) = state
        .get("reproductions")
        .and_then(Value::as_array)
        .and_then(|reports| {
            reports
                .iter()
                .filter(|report| {
                    report.get("evaluation_id").and_then(Value::as_str)
                        == Some(evaluation_id.as_str())
                        && report.get("reproducer_player_id").and_then(Value::as_str)
                            == Some(player_id.as_str())
                })
                .max_by_key(|report| report.get("version").and_then(Value::as_u64).unwrap_or(0))
        })
    {
        return format!(
            r#"<section class="panel"><span class="pill">REPRODUCTION RECORDED</span><h2>{}</h2><dl class="review-facts"><div><dt>Version</dt><dd>{}</dd></div><div><dt>Rule results</dt><dd>{}</dd></div><div><dt>Signed report</dt><dd>Verified / 已验证</dd></div></dl><p>The immutable authority record restores this result after reload or a lost response.</p></section>"#,
            escape(&scalar(report.get("status"))),
            escape(&scalar(report.get("version"))),
            report
                .get("rule_results")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        );
    }

    review_receipt_confirmation_card(paper_id, "reproduce", receipt_projection)
}

fn evaluation_panel_ids_for_display(evaluation: &Value) -> Option<Vec<Uuid>> {
    let mut panel = vec![Uuid::parse_str(evaluation.get("evaluator_player_id")?.as_str()?).ok()?];
    let reviewers = evaluation.get("reviewer_attestations")?.as_array()?;
    if reviewers.len() != 2 {
        return None;
    }
    for reviewer in reviewers {
        panel.push(Uuid::parse_str(reviewer.get("reviewer_player_id")?.as_str()?).ok()?);
    }
    panel.sort_unstable();
    panel.dedup();
    (panel.len() == 3).then_some(panel)
}

fn appeal_resolution_controls(
    identity: &AlphaIdentity,
    paper_id: &str,
    bundle: &Value,
    evaluation: &Value,
    review_state: ReadState<'_>,
) -> String {
    let Some(state) = review_state.value() else {
        return String::new();
    };
    if state
        .get("finality")
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        != Some("pending_finality")
    {
        return String::new();
    }
    let player_id = identity.player_id.to_string();
    let is_author = bundle
        .get("paper_bundle")
        .and_then(|value| value.get("release_candidate"))
        .and_then(|value| value.get("authors"))
        .and_then(Value::as_array)
        .is_some_and(|authors| {
            authors.iter().any(|author| {
                author.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
            })
        });
    let Some(evaluation_id) = evaluation.get("evaluation_id").and_then(Value::as_str) else {
        return String::new();
    };
    let Some(original_panel) = evaluation_panel_ids_for_display(evaluation) else {
        return String::new();
    };
    let panel_contains_player = original_panel.contains(&identity.player_id);
    if is_author || panel_contains_player {
        return String::new();
    }
    let Some(appeals) = state.get("appeals").and_then(Value::as_array) else {
        return String::new();
    };
    let Some(resolutions) = state.get("resolutions").and_then(Value::as_array) else {
        return String::new();
    };
    let matching = appeals
        .iter()
        .filter(|appeal| {
            appeal.get("evaluation_id").and_then(Value::as_str) == Some(evaluation_id)
                && appeal.get("paper_project_id").and_then(Value::as_str) == Some(paper_id)
        })
        .collect::<Vec<_>>();
    if matching.len() > 1 {
        return unavailable_card(
            "Appeal state ambiguous / 申诉状态不明确",
            "one immutable Appeal",
        );
    }
    let Some(appeal) = matching.first() else {
        return String::new();
    };
    let Some(appeal_id) = appeal.get("appeal_id").and_then(Value::as_str) else {
        return String::new();
    };
    if resolutions.iter().any(|resolution| {
        resolution.get("appeal_id").and_then(Value::as_str) == Some(appeal_id)
            && resolution.get("resolver_player_id").and_then(Value::as_str)
                == Some(player_id.as_str())
    }) {
        return r#"<section class="panel resolver-appeal-status"><span class="pill">APPEAL RESOLVED</span><h2>Your independent decision is authoritative / 你的独立裁决已生效</h2><p>The read model restored the immutable resolution after reload or a lost response. No signature or protocol data needs to be resubmitted.</p></section>"#.into();
    }
    if resolutions
        .iter()
        .any(|resolution| resolution.get("appeal_id").and_then(Value::as_str) == Some(appeal_id))
    {
        return String::new();
    }
    if appeal.get("appellant_player_id").and_then(Value::as_str) == Some(player_id.as_str())
        || appeal.get("release_candidate_hash").and_then(Value::as_str)
            != evaluation
                .get("release_candidate_hash")
                .and_then(Value::as_str)
    {
        return String::new();
    }

    let Some(evaluations) = state.get("evaluations").and_then(Value::as_array) else {
        return String::new();
    };
    let superseding = evaluations
        .iter()
        .filter(|candidate| {
            candidate
                .get("supersedes_evaluation_id")
                .and_then(Value::as_str)
                == Some(evaluation_id)
        })
        .collect::<Vec<_>>();
    if superseding.len() > 1 {
        return unavailable_card(
            "Superseding evaluation ambiguous / 替代评估不明确",
            "one exact superseding evaluation",
        );
    }
    let upheld_enabled = if let Some(candidate) = superseding.first() {
        let expected_version = evaluation
            .get("version")
            .and_then(Value::as_u64)
            .and_then(|version| version.checked_add(1));
        let Some(candidate_panel) = evaluation_panel_ids_for_display(candidate) else {
            return unavailable_card(
                "Superseding evaluation ineligible / 替代评估不合格",
                "exact same-release independent superseding evaluation",
            );
        };
        let candidate_panel_contains_player = candidate_panel.contains(&identity.player_id);
        let panels_disjoint = candidate_panel
            .iter()
            .all(|member| !original_panel.contains(member));
        let exact = candidate.get("paper_project_id") == evaluation.get("paper_project_id")
            && candidate.get("submission_id") == evaluation.get("submission_id")
            && candidate.get("release_candidate_hash") == evaluation.get("release_candidate_hash")
            && candidate.get("paper_bundle_hash") == evaluation.get("paper_bundle_hash")
            && candidate.get("version").and_then(Value::as_u64) == expected_version
            && panels_disjoint
            && !candidate_panel_contains_player;
        if !exact {
            return unavailable_card(
                "Superseding evaluation ineligible / 替代评估不合格",
                "exact same-release independent superseding evaluation",
            );
        }
        true
    } else {
        false
    };
    let upheld_disabled = if upheld_enabled { "" } else { " disabled" };
    format!(
        r#"<section class="panel"><span class="pill">INDEPENDENT APPEAL RESOLVER</span><h2>Resolve the visible Appeal / 裁决当前申诉</h2><p>Write the scientific rationale in plain language. Denial preserves the appealed evaluation. Uphold is enabled only when Hepta exposes one exact same-release superseding evaluation from an independent panel.</p><form class="review-appeal-resolution-form" data-paper-id="{}"><label>Decision rationale / 裁决理由<textarea name="decision" rows="5" minlength="10" maxlength="8000" required></textarea></label><div class="actions"><button type="submit" name="outcome" value="denied">Deny Appeal / 驳回申诉</button><button type="submit" name="outcome" value="upheld"{}>Uphold with superseding evaluation / 支持并采用替代评估</button></div><output></output></form></section>"#,
        escape(paper_id),
        upheld_disabled,
    )
}

fn review_slot_allowed(identity: &AlphaIdentity, slot: &str) -> bool {
    match slot {
        "evaluator" => identity.has_scope(AlphaIdentityScope::Evaluator),
        "reviewer_1" | "reviewer_2" => identity.has_scope(AlphaIdentityScope::Reviewer),
        "reproducer" => identity.has_scope(AlphaIdentityScope::Reproducer),
        _ => false,
    }
}

fn review_slot_label(slot: &str) -> String {
    match slot {
        "evaluator" => "Evaluator / 独立评估者".into(),
        "reviewer_1" => "Reviewer 1 / 同行评审一".into(),
        "reviewer_2" => "Reviewer 2 / 同行评审二".into(),
        "reproducer" => "Reproducer / 独立复现者".into(),
        other => other.to_string(),
    }
}

pub fn login_page() -> Response {
    let body = r#"<section class="hero"><span class="eyebrow">PAPER RAID · ALPHA</span><h1>Research together.</h1><p>Use the closed-Alpha access key issued to you by the operator. In invite mode its first successful use atomically activates your account; fixed-Alpha keys continue to work unchanged. The full flow separates 3 Author Raid identities from at least 4 independent evaluation, review, and reproduction identities. The key is cleared immediately and is never written to browser storage.</p></section><section class="panel narrow"><form id="login-form"><label>Closed-Alpha access key / 封闭 Alpha 访问密钥<input type="password" name="login_key" minlength="32" autocomplete="off" required></label><button type="submit">Enter Paper Raid / 进入论文远征</button><output></output></form></section>"#;
    page("Paper Raid Login", "not signed in", body, false)
}

pub(crate) fn practice(
    identity: &AlphaIdentity,
    state: Option<&PracticePlayerViewV1>,
    binding_ready: bool,
) -> Response {
    let role_steps = [
        ("Captain", "Choose the audit plan / 选择审计路线"),
        ("Evidence", "Assess the evidence gap / 判断证据缺口"),
        ("Experiment", "Run the bounded check / 执行受限检查"),
        ("Experiment", "Interpret the result / 解读检查结果"),
        ("Captain", "Record one learning / 记录复盘要点"),
    ];
    let current_step = state.map_or(0, |view| view.step);
    let all_done = state.is_some_and(|view| view.stage == PracticeStageV1::Completed);
    let progress = role_steps
        .iter()
        .enumerate()
        .map(|(index, (role, objective))| {
            let step = (index + 1) as u8;
            let status = if all_done {
                "done"
            } else if current_step == 0 {
                "upcoming"
            } else if step < current_step {
                "done"
            } else if step == current_step {
                "current"
            } else {
                "upcoming"
            };
            let aria_current = if status == "current" {
                r#" aria-current="step""#
            } else {
                ""
            };
            let status_label = match status {
                "done" => "Completed / 已完成",
                "current" => "Current / 当前",
                _ => "Upcoming / 即将开始",
            };
            format!(
                r#"<li data-practice-step-state="{}"{}><span class="practice-step-meta">{} · {}</span><strong>{}</strong><span class="practice-step-status">{}</span></li>"#,
                status,
                aria_current,
                step,
                escape(role),
                player_language_html(objective),
                player_language_html(status_label),
            )
        })
        .collect::<String>();

    let bridge_prerequisite = || {
        r#"<section class="primary-action practice-prerequisite"><span class="pill">PAIRING REQUIRED · <span lang="zh-Hans">需要配对</span></span><h2>Pair exactly one active Agent Bridge first</h2><p>Practice uses the same owner-bound Bridge safety boundary as a real Raid. Pair through the ordinary Research Lobby flow; the browser cannot invent or choose a binding. This no-secret return link comes back only after authenticated pairing consumption and a post-pairing signed, self-declared healthy report.</p><a class="button practice-primary-action" href="/league?return_to=%2Fleague%2Fpractice">Open Agent pairing / <span lang="zh-Hans">前往 Agent 配对</span></a></section>"#.to_string()
    };
    let primary = match state {
        None if !binding_ready => bridge_prerequisite(),
        None => practice_start_form("Start the 15–20 minute practice / 开始 15–20 分钟练习"),
        Some(view) if view.expired && !binding_ready => bridge_prerequisite(),
        Some(view) if view.expired => format!(
            r#"<section class="primary-action practice-expired"><span class="pill">EXPIRED · <span lang="zh-Hans">已过期</span></span><h2>This practice window ended</h2><p>Your partial choices stay local and non-portable. Start a fresh practice when ready.</p>{}</section>"#,
            practice_start_form("Start a fresh practice / 开始新的练习"),
        ),
        Some(view) if !binding_ready && !view.stage.is_terminal() => bridge_prerequisite(),
        Some(view) => match view.stage {
            PracticeStageV1::CaptainPlan => practice_choice_form(
                view,
                "Captain · Set the audit route / 队长：确定审计路线",
                "Pick the first question your cell should resolve.",
                "captain_plan",
                &[
                    ("audit_highest_risk_claim", "Audit the highest-risk claim / 优先审计最高风险主张"),
                    ("audit_evidence_chain_first", "Audit the evidence chain first / 先审计证据链"),
                ],
                "Lock the Captain plan / 确定队长计划",
            ),
            PracticeStageV1::EvidenceAssessment => practice_choice_form(
                view,
                "Evidence · Diagnose the gap / 证据员：判断缺口",
                "Classify what the cited evidence actually supports.",
                "evidence_assessment",
                &[
                    ("unsupported_claim", "The claim is unsupported / 主张缺少支持"),
                    ("citation_mismatch", "The citation does not match / 引文与主张不符"),
                    ("evidence_sufficient", "The evidence is sufficient / 当前证据充分"),
                ],
                "Record the Evidence assessment / 记录证据判断",
            ),
            PracticeStageV1::ExperimentWaitingBridge => r#"<section class="primary-action practice-agent-wait" data-practice-agent-state="waiting"><span class="pill">EXPERIMENT · AGENT HANDOFF</span><h2>Run the bounded practice check / <span lang="zh-Hans">执行受限练习检查</span></h2><p>The browser cannot impersonate an Agent or fabricate a result. This slice pauses here until the separately signed Agent Bridge claims and completes the local practice task.</p><a class="button practice-primary-action" href="/league/practice">Check Agent status / <span lang="zh-Hans">检查 Agent 状态</span></a></section>"#.to_string(),
            PracticeStageV1::ExperimentInterpretation => practice_choice_form(
                view,
                "Experiment · Interpret the result / 实验员：解读结果",
                "Choose the safest response to the bounded check.",
                "experiment_interpretation",
                &[
                    ("revise_claim", "Revise the claim / 修订主张"),
                    ("request_more_evidence", "Request more evidence / 请求更多证据"),
                    ("retain_claim_with_caveat", "Retain it with a caveat / 保留主张并注明限制"),
                ],
                "Record the Experiment decision / 记录实验判断",
            ),
            PracticeStageV1::CaptainAar => practice_choice_form(
                view,
                "Captain · One useful learning / 队长：记录一条复盘",
                "Choose the change that would help your next real team most.",
                "captain_aar",
                &[
                    ("improve_evidence_triage", "Improve evidence triage / 改进证据分诊"),
                    ("improve_experiment_design", "Improve experiment design / 改进实验设计"),
                    ("improve_team_coordination", "Improve team coordination / 改进团队协作"),
                ],
                "Finish this practice / 完成本次练习",
            ),
            PracticeStageV1::Completed if !binding_ready => r#"<section class="primary-action practice-complete"><span class="pill">PRACTICE COMPLETE · <span lang="zh-Hans">练习完成</span></span><h2>You previewed all three Author roles</h2><p>This completion is local and non-portable. Pair one active Agent Bridge before starting another practice.</p><a class="button practice-primary-action" href="/league?return_to=%2Fleague%2Fpractice">Open Agent pairing / <span lang="zh-Hans">前往 Agent 配对</span></a></section>"#.to_string(),
            PracticeStageV1::Completed => format!(
                r#"<section class="primary-action practice-complete"><span class="pill">PRACTICE COMPLETE · <span lang="zh-Hans">练习完成</span></span><h2>You previewed all three Author roles</h2><p>This completion is intentionally local and non-portable. It does not qualify an account or alter any score, rank, reward, or scientific record.</p>{}</section>"#,
                practice_start_form("Practice again / 再练一次"),
            ),
            PracticeStageV1::Abandoned if !binding_ready => bridge_prerequisite(),
            PracticeStageV1::Abandoned => format!(
                r#"<section class="primary-action practice-abandoned"><span class="pill">LEFT PRACTICE · <span lang="zh-Hans">已退出练习</span></span><h2>No authoritative state was created</h2>{}</section>"#,
                practice_start_form("Start again / 重新开始"),
            ),
            PracticeStageV1::Expired => unreachable!("clock expiry handled above"),
        },
    };

    let leave = state
        .filter(|view| !view.terminal && !view.expired)
        .map(|view| {
            format!(
                r#"<details class="panel practice-leave"><summary>Leave this practice / <span lang="zh-Hans">退出本次练习</span></summary><p>Leaving is final for this local practice session and creates no portable completion.</p><form class="practice-abandon-form" data-practice-version="{}"><button class="danger" type="submit">Leave practice / <span lang="zh-Hans">退出练习</span></button><output></output></form></details>"#,
                view.version,
            )
        })
        .unwrap_or_default();

    let status = state.map_or_else(
        || player_language_html("Not started / 尚未开始"),
        |view| {
            if view.expired {
                player_language_html("Expired / 已过期")
            } else {
                format!(
                    "{} · step {}/{} · about {} min left",
                    player_language_html(practice_stage_label(view.stage)),
                    view.step,
                    view.total_steps,
                    (view.remaining_seconds + 59) / 60,
                )
            }
        },
    );
    let body = format!(
        r#"<section class="hero practice-hero"><span class="eyebrow">PRACTICE_UNRANKED · <span lang="zh-Hans">单人非排位练习</span></span><h1>Evidence Audit: first role preview</h1><p>{}<span lang="zh-Hans">，用约 15–20 分钟依次体验 Captain、Evidence、Experiment。页面刷新后会从服务器保存的同一步继续。</span></p><p class="status verified">{}</p></section>
        <section class="panel practice-boundary"><h2>Practice boundary / <span lang="zh-Hans">练习边界</span></h2><ul><li>No scientific finality or Challenge activation / <span lang="zh-Hans">不产生科研最终性或挑战激活</span></li><li>No qualification, ranking, score, reward, or economic authority / <span lang="zh-Hans">不产生资格、排行、积分、奖励或经济权限</span></li><li>No automatic submission; every browser step requires your explicit action / <span lang="zh-Hans">不自动提交，每个浏览器步骤都需你明确操作</span></li><li>Completion is not portable to a real Raid / <span lang="zh-Hans">练习完成状态不可迁移到正式远征</span></li></ul></section>
        <section class="panel practice-progress"><h2>Five guided steps / <span lang="zh-Hans">五步引导</span></h2><ol role="list">{}</ol></section>
        {}
        {}
        <p><a href="/league">Back to Research Lobby / <span lang="zh-Hans">返回研究大厅</span></a></p>"#,
        escape(&identity.display_name),
        status,
        progress,
        primary,
        leave,
    );
    page(
        "Solo unranked practice",
        &identity.display_name,
        &body,
        true,
    )
}

fn practice_stage_label(stage: PracticeStageV1) -> &'static str {
    match stage {
        PracticeStageV1::CaptainPlan => "Captain plan / 队长计划",
        PracticeStageV1::EvidenceAssessment => "Evidence assessment / 证据判断",
        PracticeStageV1::ExperimentWaitingBridge => "Experiment handoff / 实验交接",
        PracticeStageV1::ExperimentInterpretation => "Experiment interpretation / 实验解读",
        PracticeStageV1::CaptainAar => "Captain AAR / 队长复盘",
        PracticeStageV1::Completed => "Completed / 已完成",
        PracticeStageV1::Abandoned => "Abandoned / 已退出",
        PracticeStageV1::Expired => "Expired / 已过期",
    }
}

fn practice_start_form(label: &str) -> String {
    format!(
        r#"<form class="practice-start-form primary-action"><button class="practice-primary-action" type="submit">{}</button><output></output></form>"#,
        player_language_html(label),
    )
}

fn practice_choice_form(
    view: &PracticePlayerViewV1,
    title: &str,
    prompt: &str,
    action: &str,
    choices: &[(&str, &str)],
    button: &str,
) -> String {
    let options = choices
        .iter()
        .map(|(value, label)| {
            format!(
                r#"<label class="practice-choice"><input type="radio" name="choice" value="{}" required><span>{}</span></label>"#,
                escape(value),
                player_language_html(label),
            )
        })
        .collect::<String>();
    format!(
        r#"<form class="practice-advance-form primary-action" data-practice-action="{}" data-practice-version="{}"><h2>{}</h2><p>{}</p><fieldset><legend>Choose one / <span lang="zh-Hans">请选择一项</span></legend>{}</fieldset><button class="practice-primary-action" type="submit">{}</button><output></output></form>"#,
        escape(action),
        view.version,
        player_language_html(title),
        escape(prompt),
        options,
        player_language_html(button),
    )
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
            r#"<section class="grid"><article class="card"><h2>Human identity ready / 人类身份已就绪</h2><p>Player <code>{}</code> and Consumer subject <code>{}</code> are registered. One independently controlled external Agent must now prove its own key.</p></article><article class="card"><h2>External signature only / 仅外部签名</h2><p>The Agent signs <code>hepta.paper_raid.agent_binding_proof.v3</code> and declares a bounded capability/resource profile marked <code>self_declared_unverified</code>. Its Ed25519 private key never enters this browser, Hepta, BFF, or Nakama.</p></article><article class="card"><h2>Fail closed / 失败关闭</h2><p>Every later Bridge call is signed by the active Agent key over the exact method, path, query, raw-body hash, nonce, and validity window. Hepta is self-read on every call; rotated or revoked keys stop working.</p></article></section>
            {}
            <p class="panel narrow"><a class="button" href="/league/start">Continue after Bridge reports active / Bridge 激活后继续</a></p>
            <details class="panel agent-binding-recovery"><summary>Recovery / Developer fallback · 恢复/开发者备用</summary><p>This collapsed escape hatch accepts only an exact short-lived AgentBinding V3 public proof. Normal players should use the pairing code above. Never paste an Agent seed, private key, mnemonic, login key, cookie, token, or API credential.</p><form class="agent-binding-form" data-player-id="{}"><label>Exact externally signed V3 JSON / 外部 Agent 已签名 V3 JSON<textarea name="payload" rows="20" spellcheck="false" required>{{
  "binding_id": "00000000-0000-4000-8000-000000000000",
  "player_id": "{}",
  "agent_id": "did:trnm:agent:...",
  "agent_key_id": "sha256:...",
  "agent_public_key": "...",
  "agent_proof_schema": "hepta.paper_raid.agent_binding_proof.v3",
  "capability_disclosure": {{
    "schema": "hepta.paper_raid.agent_capability_disclosure.v1",
    "assurance": "self_declared_unverified",
    "capabilities": ["evidence_search"],
    "resource_classes": ["network"],
    "max_parallel_tasks": 1
  }},
  "capability_disclosure_hash": "sha256:...",
  "agent_proof_nonce": "00000000-0000-4000-8000-000000000000",
  "agent_proof_issued_at_unix": 0,
  "agent_proof_expires_at_unix": 0,
  "agent_proof_signature": "...",
  "idempotency_key": "00000000-0000-4000-8000-000000000000"
}}</textarea></label><button type="submit">Verify and bind public proof / 验证并绑定公开证明</button><output></output></form></details>"#,
            escape(&identity.player_id.to_string()),
            escape(&identity.subject_id),
            agent_bridge_pairing_panel(),
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
        r#"<section class="panel key-vault"><details><summary>Local signing key / 本地签名密钥</summary><p class="muted">Import your encrypted recovery bundle when this tab needs to sign a human action. Decryption stays in memory and is discarded when the tab closes.</p><form class="human-key-import-form"><label>Encrypted key bundle / 加密密钥包<input name="key_bundle" type="file" accept="application/json,.json" required></label><label>Passphrase / 口令<input name="passphrase" type="password" minlength="16" autocomplete="current-password" required></label><button type="submit" disabled>Decrypt into this tab / 仅解密到当前标签页</button><output></output></form><button class="forget-human-key" type="button">Forget in-memory key / 清除内存密钥</button><output class="human-key-status">No in-memory key / 当前无内存密钥</output></details></section>"#
    } else {
        ""
    };
    let document = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><link rel="stylesheet" href="/assets/paper-raid.css"><script src="/assets/paper-raid.js" defer></script></head><body data-authenticated="{}"><header><a href="/league/start">HEPTA // PAPER RAID</a><span>{}</span></header><main>{}{}<aside id="toast" aria-hidden="true" hidden></aside></main><footer>Hepta is the research record. Nakama is the ordered collaboration timeline.</footer></body></html>"#,
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

fn active_team_proposal_links(state: ReadState<'_>) -> String {
    let Some(records) = state.value().and_then(Value::as_array) else {
        return unavailable("team proposals");
    };
    let active = records
        .iter()
        .filter(|record| {
            matches!(
                record.get("status").and_then(Value::as_str),
                Some("proposed" | "accepted")
            )
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return "<p class=\"muted\">No active proposal / 当前无待处理提案</p>".into();
    }
    let mut items = String::new();
    for proposal in active {
        let Some(proposal_id) = proposal.get("proposal_id").and_then(Value::as_str) else {
            continue;
        };
        let status = proposal
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        items.push_str(&format!(
            "<li><a href=\"/league/formation/{}\"><code>{}</code><span class=\"pill\">{}</span></a></li>",
            escape(proposal_id),
            escape(proposal_id),
            escape(status),
        ));
    }
    format!("<ul class=\"record-list\">{items}</ul>")
}

fn matchmaking_ticket_cards(state: ReadState<'_>) -> String {
    let Some(records) = state.value().and_then(Value::as_array) else {
        return unavailable("matchmaking tickets");
    };
    if records.is_empty() {
        return "<p class=\"muted\">No active ticket / 当前无排队</p>".into();
    }
    let mut items = String::new();
    for ticket in records {
        let Some(ticket_id) = ticket.get("ticket_id").and_then(Value::as_str) else {
            continue;
        };
        let status = ticket
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        if !matches!(status, "queued" | "matched") {
            continue;
        }
        let version = ticket.get("version").and_then(Value::as_u64).unwrap_or(0);
        let destination = ticket
            .get("matched_proposal_id")
            .and_then(Value::as_str)
            .map(|proposal_id| {
                format!(
                    "<a href=\"/league/formation/{}\">Open team proposal / 打开组队提案</a>",
                    escape(proposal_id)
                )
            })
            .unwrap_or_default();
        let cancel = if status == "queued" && version > 0 {
            format!(
                r#"<form class="cancel-ticket-form" data-ticket-id="{}" data-ticket-version="{}"><button class="danger" type="submit">Leave queue / 取消排队</button><output></output></form>"#,
                escape(ticket_id),
                version,
            )
        } else {
            String::new()
        };
        let party_status = if ticket
            .get("private_party")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "<p class=\"muted\">Private three-person party queue / 私密三人预组队队列</p>"
        } else {
            "<p class=\"muted\">Public compatible queue / 公开兼容队列</p>"
        };
        let queue_hint = ticket
            .get("queue_hint")
            .filter(|value| value.is_object())
            .map(|hint| {
                let position = hint
                    .get("queue_position")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".into());
                let pool = hint
                    .get("compatible_pool_size")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let needed = hint
                    .get("compatible_players_needed")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let waited = hint
                    .get("waited_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let expires = hint
                    .get("expires_in_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let missing_roles = hint
                    .get("missing_roles")
                    .and_then(Value::as_array)
                    .map(|roles| {
                        roles
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|roles| !roles.is_empty())
                    .unwrap_or_else(|| "none / 无".into());
                let eta = match hint.get("eta_seconds").and_then(Value::as_u64) {
                    Some(0) => "ready now / 可立即组队".to_string(),
                    Some(seconds) => format!("{seconds}s"),
                    None => "unknown until compatible players arrive / 等待兼容玩家，暂无法估算".into(),
                };
                format!(
                    r#"<p class="queue-hint">Position / 顺位: {} · Compatible / 兼容人数: {} · Needed / 尚缺: {}<br>Missing roles / 缺少角色: {}<br>Waited / 已等待: {}s · Ticket expires / 票据到期: {}s · ETA: {}</p>"#,
                    escape(&position),
                    pool,
                    needed,
                    escape(&missing_roles),
                    waited,
                    expires,
                    escape(&eta),
                )
            })
            .unwrap_or_default();
        items.push_str(&format!(
            r#"<li class="ticket-card"><div><code>{}</code><span class="pill">{}</span></div>{}{}{}{}</li>"#,
            escape(ticket_id),
            escape(status),
            party_status,
            queue_hint,
            destination,
            cancel,
        ));
    }
    if items.is_empty() {
        "<p class=\"muted\">No readable tickets / 无可读队列记录</p>".into()
    } else {
        format!("<ul class=\"record-list ticket-list\">{items}</ul>")
    }
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
            | "create_evidence_card"
            | "create_citation_record"
            | "submit_review"
            | "create_authorship_consent"
            | "submit_appeal"
    ) {
        "<button class=\"local-sign\" type=\"button\">Sign locally / 本地签名</button>"
    } else {
        ""
    };
    format!(
        r#"<details class="action advanced-action"><summary>{}</summary><p>{}</p><p class="muted">Developer/Alpha protocol fallback. Never paste a private key, seed, token, or Agent credential.</p><form class="command-form" data-command="{}" data-resource-id="{}">{}<label>Exact typed JSON / 精确类型 JSON<textarea name="payload" rows="9" spellcheck="false" required>{}</textarea></label>{}<button type="submit">Submit typed action / 提交</button><output></output></form></details>"#,
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
        r#"<article class="card live-raid" data-paper-id="{}"><h2>Live Raid / 实时协作</h2><p>{}</p><div class="live-status"><span class="live-connection" data-state="connecting">Connecting / 正在连接</span><span class="live-phase">Phase / 阶段: checking…</span></div><ul class="live-participants"><li>Loading teammate presence…</li></ul><ol class="live-events"></ol><button class="timeline-refresh" data-paper-id="{}" type="button">Sync now / 立即同步</button><output class="live-detail"></output></article>"#,
        escape(paper_id),
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

fn player_language_html(input: &str) -> String {
    let Some((english, chinese)) = input.split_once(" / ") else {
        return escape(input);
    };
    if !chinese
        .chars()
        .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character))
    {
        return escape(input);
    }
    format!(
        r#"{} / <span lang="zh-Hans">{}</span>"#,
        escape(english),
        escape(chinese),
    )
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
:root{color-scheme:dark;--bg:#070b12;--panel:#101826;--line:#5f7da6;--text:#e9f2ff;--muted:#8ba0ba;--cyan:#55e6ff;--amber:#ffca68;--pink:#ff6ba8}
*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 20% 0,#11213a 0,#070b12 42%);color:var(--text);font:15px/1.55 Inter,ui-sans-serif,system-ui,sans-serif;min-height:100vh}
header{align-items:center;border-bottom:1px solid var(--line);display:flex;justify-content:space-between;padding:16px clamp(18px,4vw,56px);position:sticky;top:0;background:#070b12e8;backdrop-filter:blur(12px)}header a{color:var(--cyan);font-weight:900;letter-spacing:.12em;text-decoration:none}header span,footer{color:var(--muted)}
main{margin:auto;max-width:1180px;padding:clamp(24px,5vw,64px) clamp(16px,4vw,44px)}.hero{border-left:4px solid var(--cyan);padding:8px 0 12px 22px;margin-bottom:28px}.eyebrow{color:var(--amber);font-size:12px;font-weight:800;letter-spacing:.16em}.hero h1{font-size:clamp(32px,7vw,72px);line-height:1;margin:10px 0}.hero p{color:var(--muted);max-width:760px}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:14px;margin:14px 0}.card,.panel{background:linear-gradient(145deg,#142036,#0d1421);border:1px solid var(--line);border-radius:14px;padding:18px;min-height:120px}.card h2,.panel h2{font-size:14px;letter-spacing:.05em;margin:0 0 12px}.card p{color:var(--text)}.unavailable{border-style:dashed;color:var(--muted)}.muted,.action p{color:var(--muted)}.dot{background:var(--pink);border-radius:50%;display:inline-block;height:8px;margin-right:8px;width:8px}.pill,.status{border:1px solid var(--amber);border-radius:999px;color:var(--amber);display:inline-block;font-size:12px;font-weight:800;padding:4px 9px}.status{padding:6px 12px}.status.missing{border-color:var(--pink);color:var(--pink)}.source-state{color:var(--muted);font-size:12px}.roster,.record-list{display:grid;gap:10px;list-style:none;margin:0;padding:0}.roster li{align-items:center;border-bottom:1px solid var(--line);display:grid;gap:8px;grid-template-columns:90px 1fr 1fr 1fr;padding:10px 0}.roster span{color:var(--muted);overflow-wrap:anywhere}.record-list li,.record-list a{align-items:center;display:flex;gap:8px;justify-content:space-between}.record-list a{color:var(--text);text-decoration:none;width:100%}.action-grid{display:grid;gap:14px;grid-template-columns:repeat(2,minmax(0,1fr))}.action{border:1px solid var(--line);border-radius:12px;padding:16px}.action h3{margin-top:0}form{display:grid;gap:12px}label{color:var(--muted);display:grid;font-size:12px;gap:6px}input,textarea,select,button{background:#07101d;border:1px solid var(--line);border-radius:8px;color:var(--text);font:inherit;padding:10px 12px}textarea{font:12px/1.45 ui-monospace,SFMono-Regular,Consolas,monospace;resize:vertical}button{background:#12334a;border-color:var(--cyan);color:var(--cyan);cursor:pointer;font-weight:800}button:hover{filter:brightness(1.2)}button.danger{border-color:var(--pink);color:var(--pink)}button:disabled{cursor:wait;opacity:.55}:where(a,button,input,textarea,select,summary,[tabindex]):focus-visible{outline:3px solid var(--amber);outline-offset:3px}output{color:var(--amber);font:12px/1.45 ui-monospace,SFMono-Regular,Consolas,monospace;overflow-wrap:anywhere;white-space:pre-wrap}output.result-error{color:var(--pink)}output.result-ok{color:var(--amber)}output[data-player-message=friendly]{font-family:Inter,ui-sans-serif,system-ui,sans-serif;font-size:13px}.player-error-details{border-left:2px solid var(--line);margin-top:4px;padding-left:10px}.player-error-details>summary{color:var(--muted);cursor:pointer;font-size:12px;font-weight:700}.player-error-details[open]>summary{color:var(--cyan)}.player-error-raw{display:block;font:11px/1.45 ui-monospace,SFMono-Regular,Consolas,monospace;margin-top:7px;white-space:pre-wrap}.narrow{margin:auto;max-width:540px}#toast{background:#101826;border:1px solid var(--line);border-radius:10px;bottom:18px;display:block;max-width:min(520px,90vw);padding:12px 16px;position:fixed;right:18px;z-index:10}#toast[hidden]{display:none}code{color:var(--cyan);overflow-wrap:anywhere}footer{padding:28px;text-align:center}
.key-vault{margin-bottom:18px;min-height:auto}.key-vault summary{color:var(--cyan);cursor:pointer;font-weight:800}.key-vault form{margin:14px 0}.human-key-status{display:block;margin-top:10px}.challenge-rules{display:grid;gap:7px;margin:12px 0}.challenge-rules div{border-bottom:1px solid var(--line);display:grid;gap:4px;padding:5px 0}.challenge-rules dt{color:var(--muted);font-size:11px}.challenge-rules dd{margin:0}
  .mission-board{display:grid;gap:18px;grid-template-columns:minmax(220px,.8fr) minmax(0,2fr);margin-bottom:20px}.mission-board h2{font-size:24px;letter-spacing:0;margin:6px 0}.raid-steps{display:grid;gap:8px;grid-template-columns:repeat(5,minmax(0,1fr));list-style:none;margin:0;padding:0}.raid-steps li{border:1px solid var(--line);border-radius:10px;display:grid;gap:8px;padding:12px}.raid-steps li>span{color:var(--muted);font-size:11px;font-weight:900}.raid-steps strong{display:block}.raid-steps p{color:var(--muted);font-size:11px;line-height:1.35;margin:4px 0 0}.raid-steps [data-step-state=current]{background:#12334a;border-color:var(--cyan)}.raid-steps [data-step-state=current]>span{color:var(--cyan)}.raid-steps [data-step-state=complete]{border-color:#3b8f78}.raid-steps [data-step-state=complete]>span{color:#67e8b5}.role-kit{border:1px solid var(--line);border-radius:10px;display:grid;gap:7px;margin:0;padding:12px}.role-kit legend{color:var(--amber);font-size:12px;font-weight:800;padding:0 6px}.role-kit span{color:var(--muted);font-size:12px}.role-kit strong{color:var(--text)}.role-choice{align-items:start;border:1px solid var(--line);border-radius:8px;display:grid;gap:9px;grid-template-columns:auto 1fr;margin:0;padding:9px}.role-choice input{margin-top:3px}.role-choice small{display:block;line-height:1.35;margin-top:3px}.role-choice[data-preference-rank="1"]{border-color:var(--cyan)}.advanced-action summary{color:var(--muted);cursor:pointer;font-weight:800}.advanced-action[open] summary{color:var(--cyan);margin-bottom:12px}.continue-raid{border-color:var(--cyan);margin-bottom:18px}.button{border:1px solid var(--cyan);border-radius:8px;color:var(--cyan);display:inline-block;font-weight:800;padding:10px 12px;text-decoration:none}.guided-action{border:1px solid var(--cyan);border-radius:12px;padding:16px}.guided-action.blocked{border-color:var(--amber)}.live-status{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0}.live-status span{border:1px solid var(--line);border-radius:999px;padding:6px 10px}.live-connection[data-state=live]{border-color:#3b8f78;color:#67e8b5}.live-connection[data-state=catching-up],.live-connection[data-state=reconnecting]{border-color:var(--amber);color:var(--amber)}.live-participants,.live-events{display:grid;gap:8px;list-style:none;padding:0}.live-participants li,.live-events li{border:1px solid var(--line);border-radius:8px;padding:10px}.live-participants [data-connected=true]{color:#67e8b5}.live-participants [data-connected=false]{color:var(--muted)}
.paper-room-hero{margin-bottom:18px}.paper-room-basic{border-color:var(--cyan);display:grid;gap:16px;margin-bottom:16px;min-height:auto}.paper-room-basic>h2{font-size:clamp(24px,4vw,38px);letter-spacing:0;margin:0}.paper-room-personal-objective{font-size:clamp(15px,2vw,18px);margin:0;max-width:70ch}.paper-room-basic-grid{display:grid;gap:14px;grid-template-columns:minmax(0,1fr) minmax(0,1fr)}.paper-room-blocker,.paper-room-next-step{border:1px solid var(--line);border-radius:12px;min-width:0;padding:14px}.paper-room-blocker h3,.paper-room-next-step h3{font-size:13px;margin:0 0 8px}.paper-room-blocker-reason{font-weight:700;margin:0;overflow-wrap:anywhere}.paper-room-blocker-reason[data-blocker-state=blocked]{color:var(--amber)}.paper-room-blocker-reason[data-blocker-state=ready]{color:#67e8b5}.paper-room-more-blockers{color:var(--muted);font-size:12px;margin:8px 0 0}.paper-room-next-step{align-content:start;display:grid;gap:10px}.paper-room-next-step p{margin:0;overflow-wrap:anywhere}.paper-room-primary-button{justify-self:start;min-height:44px;max-width:100%;scroll-margin-block:28px}.paper-room-advanced{margin-top:16px;min-height:auto}.paper-room-advanced-summary{cursor:pointer;font-size:16px;font-weight:800}.paper-room-advanced-summary span,.paper-room-advanced-summary small{display:block}.paper-room-advanced-summary small{color:var(--muted);font-size:12px;font-weight:500;margin-top:4px}.paper-room-advanced[open]>.paper-room-advanced-summary{color:var(--cyan);margin-bottom:18px}.paper-room-advanced-content{display:grid;gap:16px;min-width:0}.paper-room-authority{border:1px solid var(--line);border-radius:12px;padding:14px}.paper-room-authority>h2{font-size:20px;margin:7px 0 14px}.paper-room-authority-facts{display:grid;gap:8px;grid-template-columns:repeat(2,minmax(0,1fr));margin:0 0 14px}.paper-room-authority-facts div{border-bottom:1px solid var(--line);display:grid;gap:3px;min-width:0;padding:7px}.paper-room-authority-facts dt{color:var(--muted);font-size:11px}.paper-room-authority-facts dd{margin:0;overflow-wrap:anywhere}.raid-command-center{border-color:var(--cyan);display:grid;gap:20px;grid-template-columns:minmax(0,1.3fr) minmax(260px,.7fr);margin-bottom:18px}.phase-objective h2{font-size:clamp(22px,4vw,36px);letter-spacing:0;margin:7px 0}.phase-readiness ul{display:grid;gap:7px;margin:0 0 8px;padding-left:20px}.phase-readiness .ready{color:#67e8b5}.projected-actions{margin-top:14px}.projected-actions ul{display:flex;flex-wrap:wrap;gap:6px;list-style:none;margin:7px 0 0;padding:0}.projected-actions li{border:1px solid var(--line);border-radius:999px;color:var(--muted);font-size:11px;padding:4px 8px}.primary-action{background:#0b2637;border:1px solid var(--cyan);border-radius:12px;padding:16px;scroll-margin-block:28px}.work-item-panel,.research-session-panel,.revision-panel,.material-panel{margin:14px 0}.work-board{display:grid;gap:10px}.work-card,.session-card,.release-approval{border:1px solid var(--line);border-radius:10px;padding:14px}.work-card h3,.session-card h3{margin:8px 0}.accepted-artifact-field[hidden]{display:none}.session-control-stack{display:grid;gap:9px}.materialization-summary{border:1px solid #3b8f78;border-radius:10px;margin:12px 0;padding:14px}.materialization-summary.missing{border-color:var(--pink)}.materialization-summary.pending{border-color:var(--line)}.materialized-sections{display:grid;gap:7px;list-style:none;margin:10px 0 0;padding:0}.materialized-sections li{border-top:1px solid var(--line);display:grid;gap:4px;padding-top:8px}.materialized-sections span,.materialized-sections small{color:var(--muted)}.release-author{border:1px solid var(--line);border-radius:8px;display:grid;gap:8px;margin:9px 0;padding:10px}.release-author legend,fieldset>legend{color:var(--amber);font-size:12px;font-weight:800}.developer-tools{margin-top:22px}.developer-tools>summary{color:var(--muted);cursor:pointer;font-size:15px;font-weight:800}.developer-tools[open]>summary{color:var(--cyan);margin-bottom:12px}.eligibility-grid{display:grid;gap:5px;grid-template-columns:repeat(2,minmax(0,1fr));list-style:none;margin:10px 0 0;padding:0}.eligibility-grid li{border:1px solid var(--line);border-radius:7px;display:flex;font-size:11px;gap:6px;justify-content:space-between;padding:6px}.eligibility-grid [data-eligible=true]{border-color:#3b8f78;color:#67e8b5}.eligibility-grid [data-eligible=false]{color:var(--muted)}.status.verified{border-color:#3b8f78;color:#67e8b5}.ticket-list .ticket-card{align-items:stretch;display:grid;gap:8px}.ticket-card>div{display:flex;gap:8px;justify-content:space-between}.ticket-card form{display:block}.review-queue-empty{text-align:center}.review-queue-empty .button{margin-top:12px}.review-boundary{border-color:var(--cyan);display:grid;gap:18px;grid-template-columns:2fr 1fr;margin-bottom:18px}.review-boundary dl,.review-facts{display:grid;gap:7px;margin:0}.review-boundary dl div,.review-facts div{border-bottom:1px solid var(--line);display:grid;gap:4px;padding:7px 0}.review-boundary dt,.review-facts dt{color:var(--muted);font-size:11px}.review-boundary dd,.review-facts dd{margin:0;overflow-wrap:anywhere}.review-queue-grid{display:grid;gap:16px}.review-queue-card>header{align-items:start;background:none;border:0;display:flex;gap:12px;justify-content:space-between;padding:0;position:static}.review-queue-card>header h2{font-size:24px;letter-spacing:0;margin:6px 0 12px}.review-facts{grid-template-columns:repeat(2,minmax(0,1fr));margin:16px 0}.review-columns{display:grid;gap:14px;grid-template-columns:repeat(2,minmax(0,1fr))}.review-columns>section{border:1px solid var(--line);border-radius:10px;padding:13px}.review-columns h3{font-size:12px;margin:0 0 10px}.review-assignments{display:grid;gap:9px;list-style:none;margin:0;padding:0}.review-assignments li{align-items:center;display:flex;gap:10px;justify-content:space-between}.review-assignments span,.review-open-slots small{color:var(--muted);display:block;font-size:11px}.review-open-slots{display:grid;gap:8px}.review-claim-form{align-items:center;border-bottom:1px solid var(--line);display:grid;gap:8px;grid-template-columns:1fr auto;padding:8px 0}.review-claim-form output{grid-column:1/-1}.review-protocol-gap{border-color:var(--amber);margin-top:18px}.review-protocol-gap li{margin:6px 0}.challenge-availability{border-left:3px solid var(--cyan);padding-left:10px}.challenge[data-challenge-status=closed] .challenge-availability,.challenge[data-challenge-status=draft] .challenge-availability,.challenge[data-challenge-status=unavailable] .challenge-availability{border-color:var(--amber);color:var(--muted)}.review-artifact-list{display:grid;gap:10px;list-style:none;margin:14px 0 0;padding:0}.review-artifact-item{align-items:center;border:1px solid var(--line);border-radius:10px;display:flex;gap:14px;justify-content:space-between;padding:13px}.review-artifact-item>div:first-child{display:grid;gap:4px;min-width:0}.review-artifact-filename{overflow-wrap:anywhere}.review-artifact-item span,.review-artifact-item small{color:var(--muted)}.review-artifact-actions{display:flex;flex-wrap:wrap;gap:8px}.review-artifact-actions .button{margin:0}
.role-resource-panel{border-color:#3b8f78;margin:14px 0}.role-resource-panel .facts{display:grid;gap:8px;grid-template-columns:repeat(3,minmax(0,1fr));margin:12px 0}.role-resource-panel .facts article{border:1px solid var(--line);border-radius:9px;display:grid;gap:2px;min-height:auto;padding:10px}.role-resource-panel .facts strong{color:#67e8b5;font-size:20px}.role-resource-panel .facts span{color:var(--muted);font-size:11px}
.challenge-ruleset-panel{border-color:var(--amber);display:grid;gap:16px;margin-bottom:18px}.challenge-ruleset-header{display:grid;gap:16px;grid-template-columns:minmax(220px,1fr) minmax(300px,1.2fr)}.challenge-ruleset-header h2{font-size:24px;margin:8px 0}.challenge-ruleset-facts{display:grid;gap:6px;grid-template-columns:repeat(2,minmax(0,1fr));margin:0}.challenge-ruleset-facts div{border-bottom:1px solid var(--line);display:grid;gap:3px;padding:6px}.challenge-ruleset-facts dt{color:var(--muted);font-size:11px}.challenge-ruleset-facts dd{margin:0}.challenge-clock,.challenge-outcome,.challenge-victory{border:1px solid var(--line);border-radius:10px;padding:13px}.challenge-clock strong,.challenge-outcome strong{display:block;font-size:18px;margin-top:5px}.challenge-clock p,.challenge-outcome p{color:var(--muted);margin-bottom:0}.challenge-victory ul,.challenge-phase-gates ol,.challenge-phase-gates ul{display:grid;gap:6px;margin:8px 0;padding-left:22px}.challenge-phase-gates summary,.challenge-terminal-controls summary{color:var(--cyan);cursor:pointer;font-weight:800}.challenge-terminal-grid{display:grid;gap:12px;grid-template-columns:repeat(2,minmax(0,1fr));margin-top:12px}.challenge-terminal-grid form{border:1px solid var(--line);border-radius:10px;padding:12px}.challenge-countdown[data-state=active]{color:#67e8b5}.challenge-countdown[data-state=overtime]{color:var(--amber)}.challenge-countdown[data-state=expired]{color:var(--pink)}
.practice-entry{border-color:#3b8f78;margin-bottom:18px}.practice-entry .button{margin-top:8px}.practice-boundary{border-color:var(--amber);margin-bottom:18px}.practice-boundary ul{display:grid;gap:7px;margin:0;padding-left:22px}.practice-progress{margin-bottom:18px}.practice-progress ol{display:grid;gap:8px;grid-template-columns:repeat(5,minmax(0,1fr));list-style:none;margin:0;padding:0}.practice-progress li{border:1px solid var(--line);border-radius:10px;display:grid;gap:5px;padding:11px}.practice-progress li span{color:var(--muted);font-size:11px}.practice-progress .practice-step-status{font-weight:800}.practice-progress [data-practice-step-state=current]{background:#12334a;border-color:var(--cyan)}.practice-progress [data-practice-step-state=current] span{color:var(--cyan)}.practice-progress [data-practice-step-state=done]{border-color:#3b8f78}.practice-progress [data-practice-step-state=done] span{color:#67e8b5}.practice-choice{align-items:start;border:1px solid var(--line);border-radius:9px;display:grid;gap:10px;grid-template-columns:auto 1fr;padding:10px}.practice-choice input{margin-top:3px}.practice-advance-form,.practice-agent-wait,.practice-complete,.practice-expired,.practice-abandoned{margin-bottom:18px}.practice-advance-form fieldset{border:0;display:grid;gap:8px;margin:0;padding:0}.practice-primary-action{justify-self:start}.practice-leave{min-height:auto}.practice-leave summary{color:var(--muted);cursor:pointer;font-weight:800}.practice-leave[open] summary{color:var(--pink);margin-bottom:12px}
@media(max-width:820px){.grid,.action-grid,.mission-board,.raid-steps,.practice-progress ol,.paper-room-basic-grid,.paper-room-authority-facts,.raid-command-center,.review-boundary,.review-facts,.review-columns,.challenge-ruleset-header,.challenge-ruleset-facts,.challenge-terminal-grid,.role-resource-panel .facts{grid-template-columns:1fr}.roster li{align-items:start;grid-template-columns:1fr}.hero h1{font-size:38px}header{position:static}.card{min-height:auto}}
@media(max-width:430px){main{padding:22px 12px}.paper-room-basic,.paper-room-advanced{border-radius:12px;padding:14px}.paper-room-primary-button{justify-self:stretch;width:100%}.practice-primary-action{justify-self:stretch;width:100%}.paper-room-advanced-content{gap:12px}.paper-room-authority{padding:12px}}
@media(max-width:390px){.paper-room-basic>h2{font-size:24px}.paper-room-blocker,.paper-room-next-step{padding:12px}.paper-room-advanced-summary small{font-size:11px}}
.paper-room-advanced-summary [lang]{display:inline}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;
    use ed25519_dalek::SigningKey;
    use hepta_paper_raid_contracts::{
        paper_reproduction_signing_bytes, sign_authorship_consent, AuthorshipConsentSigningV2,
        PaperBundleAuthorConsentV2, PaperReproductionSigningV1,
    };
    use http_body_util::BodyExt;
    use uuid::Uuid;

    #[test]
    fn author_readiness_phase_has_no_independent_reproduction_semantics() {
        assert_eq!(
            paper_phase_label("reproduction_readiness"),
            "Reproduction readiness / 复现准备"
        );
        assert_eq!(
            paper_phase_label("reproducing"),
            "Reproduction readiness / 复现准备"
        );
    }

    fn practice_view(stage: PracticeStageV1, step: u8, terminal: bool) -> PracticePlayerViewV1 {
        let now = Utc::now();
        PracticePlayerViewV1 {
            schema: "hepta.paper_raid.practice_player_view.v1",
            mode: "practice_unranked".into(),
            scenario_id: "evidence-audit-intro-v1".into(),
            stage,
            version: u64::from(step.max(1)),
            role: "practice",
            step,
            total_steps: 5,
            started_at: now,
            expires_at: now + chrono::Duration::minutes(20),
            remaining_seconds: 1_200,
            expired: false,
            terminal,
            awaiting_agent_bridge: stage == PracticeStageV1::ExperimentWaitingBridge,
            eligibility: crate::practice::PracticeEligibilityV1::locked(),
        }
    }

    async fn practice_html(state: Option<&PracticePlayerViewV1>, binding_ready: bool) -> String {
        let identity =
            AlphaIdentity::test_identity("practice-player", Uuid::new_v4(), Uuid::new_v4());
        let bytes = practice(&identity, state, binding_ready)
            .into_body()
            .collect()
            .await
            .expect("collect practice HTML")
            .to_bytes();
        String::from_utf8(bytes.to_vec()).expect("UTF-8 practice HTML")
    }

    #[tokio::test]
    async fn practice_requires_one_paired_bridge_before_start() {
        let blocked = practice_html(None, false).await;
        assert!(blocked.contains("Pair exactly one active Agent Bridge first"));
        assert!(blocked.contains("href=\"/league\""));
        assert!(!blocked.contains("practice-start-form"));
        assert!(!blocked.contains("practice_session_id"));
        assert!(!blocked.contains("binding_id"));

        let ready = practice_html(None, true).await;
        assert_eq!(ready.matches("practice-start-form").count(), 1);
        assert!(ready.contains("15–20 minute practice"));
        assert!(ready.contains("<html lang=\"en\">"));
        assert!(ready.contains("<aside id=\"toast\" aria-hidden=\"true\" hidden>"));
        assert!(!ready.contains("id=\"toast\" aria-live="));
        assert!(ready.contains("lang=\"zh-Hans\">单人非排位练习</span>"));
        assert!(ready.contains(
            "class=\"status verified\">Not started / <span lang=\"zh-Hans\">尚未开始</span>"
        ));
    }

    #[tokio::test]
    async fn practice_renders_one_bounded_player_action_for_each_browser_stage() {
        for (stage, step, action, choices) in [
            (
                PracticeStageV1::CaptainPlan,
                1,
                "captain_plan",
                vec!["audit_highest_risk_claim", "audit_evidence_chain_first"],
            ),
            (
                PracticeStageV1::EvidenceAssessment,
                2,
                "evidence_assessment",
                vec![
                    "unsupported_claim",
                    "citation_mismatch",
                    "evidence_sufficient",
                ],
            ),
            (
                PracticeStageV1::ExperimentInterpretation,
                4,
                "experiment_interpretation",
                vec![
                    "revise_claim",
                    "request_more_evidence",
                    "retain_claim_with_caveat",
                ],
            ),
            (
                PracticeStageV1::CaptainAar,
                5,
                "captain_aar",
                vec![
                    "improve_evidence_triage",
                    "improve_experiment_design",
                    "improve_team_coordination",
                ],
            ),
        ] {
            let view = practice_view(stage, step, false);
            let body = practice_html(Some(&view), true).await;
            assert_eq!(body.matches("practice-advance-form").count(), 1);
            assert_eq!(body.matches("practice-abandon-form").count(), 1);
            assert_eq!(body.matches("<ol role=\"list\">").count(), 1);
            assert_eq!(body.matches("aria-current=\"step\"").count(), 1);
            assert!(body.contains(
                "class=\"practice-step-status\">Current / <span lang=\"zh-Hans\">当前</span>"
            ));
            if step > 1 {
                assert!(body.contains("class=\"practice-step-status\">Completed / "));
            }
            if step < 5 {
                assert!(body.contains("class=\"practice-step-status\">Upcoming / "));
            }
            assert!(body.contains(&format!("data-practice-action=\"{action}\"")));
            for choice in choices {
                assert!(body.contains(&format!("value=\"{choice}\"")));
            }
            for forbidden in [
                "practice_session_id",
                "subject_id",
                "player_id",
                "binding_id",
                "bridge_task_id",
                "sha256:",
            ] {
                assert!(
                    !body.contains(forbidden),
                    "practice HTML leaked {forbidden}"
                );
            }
        }
    }

    #[tokio::test]
    async fn practice_waits_for_signed_agent_and_completion_marks_all_steps_done() {
        let waiting = practice_view(PracticeStageV1::ExperimentWaitingBridge, 3, false);
        let waiting_body = practice_html(Some(&waiting), true).await;
        assert!(waiting_body.contains("Check Agent status"));
        assert!(waiting_body.contains("browser cannot impersonate an Agent"));
        assert!(!waiting_body.contains("practice-advance-form"));
        assert_eq!(waiting_body.matches("practice-abandon-form").count(), 1);

        let completed = practice_view(PracticeStageV1::Completed, 5, true);
        let completed_body = practice_html(Some(&completed), true).await;
        assert_eq!(
            completed_body
                .matches("data-practice-step-state=\"done\"")
                .count(),
            5
        );
        assert!(!completed_body.contains("data-practice-step-state=\"current\""));
        assert!(!completed_body.contains("aria-current=\"step\""));
        assert_eq!(
            completed_body
                .matches("class=\"practice-step-status\">Completed / ")
                .count(),
            5
        );
        assert_eq!(completed_body.matches("practice-start-form").count(), 1);
        assert!(!completed_body.contains("practice-abandon-form"));
    }

    #[test]
    fn player_language_html_marks_only_the_chinese_language_part_and_escapes_both() {
        assert_eq!(
            player_language_html(r#"Open <action> & "now" / 打开<&"'>"#),
            "Open &lt;action&gt; &amp; &quot;now&quot; / <span lang=\"zh-Hans\">打开&lt;&amp;&quot;&#x27;&gt;</span>"
        );
        assert_eq!(
            player_language_html("English-only <status>"),
            "English-only &lt;status&gt;"
        );
    }

    fn test_key_snapshot(seed: u8) -> (String, String, String) {
        let public_key = BASE64.encode([seed; 32]);
        let public_key_hash = sha256_digest(&[seed; 32]);
        let signature = BASE64.encode([seed.wrapping_add(1); 64]);
        (public_key, public_key_hash, signature)
    }

    fn test_ruleset_snapshot(
        victory_summary: &str,
        role_resources: Option<Value>,
    ) -> (Value, String) {
        let mut gameplay = serde_json::json!({"victory_summary": victory_summary});
        if let Some(role_resources) = role_resources {
            gameplay["role_resources"] = role_resources;
        }
        let ruleset = serde_json::json!({
            "schema":"hepta.challenge.ruleset.v1",
            "gameplay":gameplay,
        });
        let ruleset_hash = canonical_json_sha256(&ruleset).expect("ruleset hash");
        let snapshot = serde_json::json!({
            "schema":"hepta.paper_raid.challenge_ruleset_snapshot.v1",
            "challenge_snapshot_hash":format!("sha256:{}", "c".repeat(64)),
            "ruleset_version":"aar-v1",
            "ruleset_hash":ruleset_hash,
            "enforcement":"authoritative_v1",
            "ruleset":ruleset,
        });
        let snapshot_hash = canonical_json_sha256(&snapshot).expect("snapshot hash");
        (snapshot, snapshot_hash)
    }

    fn test_paper_bundle(
        release_candidate: PaperReleaseCandidateV2,
        release_candidate_hash: &str,
    ) -> PaperBundleV2 {
        let author_consents = release_candidate
            .authors
            .iter()
            .enumerate()
            .map(|(index, author)| {
                let signing_key = SigningKey::from_bytes(&[u8::try_from(index + 1).unwrap(); 32]);
                let signing_public_key = BASE64.encode(signing_key.verifying_key().as_bytes());
                let signing_public_key_hash = sha256_digest(signing_key.verifying_key().as_bytes());
                let consent = AuthorshipConsentSigningV2 {
                    schema: "hepta.paper_raid.authorship_consent.v2".into(),
                    consent_id: Uuid::new_v4(),
                    paper_project_id: release_candidate.paper_project_id,
                    revision_id: release_candidate.revision_id,
                    player_id: author.player_id,
                    signing_key_id: format!("author-{}-key", index + 1),
                    signing_public_key: signing_public_key.clone(),
                    signing_public_key_hash: signing_public_key_hash.clone(),
                    release_candidate_hash: release_candidate_hash.into(),
                    signed_at_unix: 1_786_406_400 + i64::try_from(index).unwrap(),
                };
                PaperBundleAuthorConsentV2 {
                    author_order: author.author_order,
                    participant_slot: author.participant_slot,
                    player_id: author.player_id,
                    consent_id: consent.consent_id,
                    signing_key_id: consent.signing_key_id.clone(),
                    signing_public_key,
                    signing_public_key_hash,
                    signed_at_unix: consent.signed_at_unix,
                    signature: sign_authorship_consent(&consent, &signing_key)
                        .expect("author consent signature"),
                }
            })
            .collect();
        let mut bundle = PaperBundleV2 {
            schema: "hepta.paper_raid.paper_bundle.v2".into(),
            release_candidate,
            release_candidate_hash: release_candidate_hash.into(),
            author_consents,
            paper_bundle_hash: String::new(),
        };
        bundle.paper_bundle_hash = paper_bundle_hash(&bundle).expect("PaperBundle hash");
        bundle
    }

    fn refresh_test_evaluation(evaluation: &mut Value) {
        let evaluation_id = Uuid::parse_str(evaluation["evaluation_id"].as_str().unwrap()).unwrap();
        let paper_id = Uuid::parse_str(evaluation["paper_project_id"].as_str().unwrap()).unwrap();
        let submission_id = Uuid::parse_str(evaluation["submission_id"].as_str().unwrap()).unwrap();
        let supersedes_evaluation_id = match &evaluation["supersedes_evaluation_id"] {
            Value::Null => None,
            Value::String(value) => Some(Uuid::parse_str(value).unwrap()),
            _ => panic!("supersedes evaluation"),
        };
        let tolerance_policy_hash = canonical_json_sha256(&evaluation["tolerance_policy"]).unwrap();
        evaluation["tolerance_policy_hash"] = Value::String(tolerance_policy_hash.clone());
        let reference_metrics_hash =
            canonical_json_sha256(&evaluation["reference_metrics_micros"]).unwrap();
        let hard_gates_hash =
            canonical_json_sha256(&evaluation["paper_score"]["hard_gates"]).unwrap();
        let components = evaluation["paper_score"]["components"].clone();
        let hard_gates = evaluation["paper_score"]["hard_gates"].clone();
        let score_bps = components
            .as_object()
            .unwrap()
            .values()
            .map(Value::as_u64)
            .collect::<Option<Vec<_>>>()
            .unwrap()
            .into_iter()
            .sum::<u64>();
        let eligible = hard_gates
            .as_object()
            .unwrap()
            .values()
            .all(|value| value.as_bool() == Some(true));
        evaluation["paper_score"]["score_bps"] = serde_json::json!(score_bps);
        evaluation["paper_score"]["eligible"] = serde_json::json!(eligible);
        evaluation["paper_score"]["evaluation_id"] = serde_json::json!(evaluation_id);
        evaluation["paper_score"]["paper_project_id"] = serde_json::json!(paper_id);
        evaluation["paper_score"]["created_at"] = evaluation["created_at"].clone();
        let paper_score_hash = canonical_json_sha256(&serde_json::json!({
            "schema":"hepta.paper_raid.paper_score.v1",
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "components":components,
            "hard_gates":hard_gates,
            "score_bps":score_bps,
            "eligible":eligible,
        }))
        .unwrap();
        evaluation["paper_score"]["score_hash"] = Value::String(paper_score_hash.clone());
        let signing = PaperEvaluationSigningV1 {
            schema: "hepta.paper_raid.evaluation.v1".into(),
            evaluation_id,
            paper_project_id: paper_id,
            submission_id,
            release_candidate_hash: evaluation["release_candidate_hash"]
                .as_str()
                .unwrap()
                .into(),
            paper_bundle_hash: evaluation["paper_bundle_hash"].as_str().unwrap().into(),
            supersedes_evaluation_id,
            tolerance_policy_hash,
            paper_score_hash,
            reference_metrics_hash,
            hard_gates_hash,
            evaluator_player_id: Uuid::parse_str(
                evaluation["evaluator_player_id"].as_str().unwrap(),
            )
            .unwrap(),
            signing_key_id: evaluation["evaluator_signing_key_id"]
                .as_str()
                .unwrap()
                .into(),
            signing_public_key_hash: evaluation["evaluator_signing_public_key_hash"]
                .as_str()
                .unwrap()
                .into(),
            coi_attestation_hash: evaluation["evaluator_coi_attestation_hash"]
                .as_str()
                .unwrap()
                .into(),
            signed_at_unix: evaluation["evaluator_signed_at_unix"].as_i64().unwrap(),
        };
        let evaluation_signing_hash =
            sha256_digest(&paper_evaluation_signing_bytes(&signing).unwrap());
        evaluation["evaluation_signing_hash"] = Value::String(evaluation_signing_hash.clone());
        for attestation in evaluation["reviewer_attestations"].as_array_mut().unwrap() {
            attestation["evaluation_id"] = serde_json::json!(evaluation_id);
            attestation["evaluation_signing_hash"] = Value::String(evaluation_signing_hash.clone());
        }
        evaluation["reviewer_attestations"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|attestation| {
                Uuid::parse_str(attestation["reviewer_player_id"].as_str().unwrap()).unwrap()
            });
        let approvals = evaluation["reviewer_attestations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|attestation| attestation["verdict"] == "approve")
            .count();
        evaluation["status"] = serde_json::json!(if !eligible {
            "not_eligible"
        } else if score_bps >= 6_000 && approvals == 2 {
            "accepted"
        } else {
            "rejected"
        });
    }

    fn test_consumed_panel_assignments(
        paper_id: Uuid,
        submission_id: Uuid,
        evaluation_id: Uuid,
        review_round: u64,
        evaluator_id: Uuid,
        reviewer_ids: [Uuid; 2],
        updated_at: &str,
    ) -> Vec<Value> {
        [
            ("evaluator", evaluator_id),
            ("reviewer_1", reviewer_ids[0]),
            ("reviewer_2", reviewer_ids[1]),
        ]
        .into_iter()
        .map(|(slot, player_id)| {
            serde_json::json!({
                "schema":"hepta.paper_raid.review_assignment.v1",
                "assignment_id":Uuid::new_v4(),
                "paper_project_id":paper_id,
                "submission_id":submission_id,
                "player_id":player_id,
                "review_round":review_round,
                "slot":slot,
                "pinned_evaluation_id":evaluation_id,
                "status":"consumed",
                "version":3,
                "claimed_at":"2026-08-11T00:00:10Z",
                "expires_at":"2026-08-11T02:00:00Z",
                "updated_at":updated_at,
            })
        })
        .collect()
    }

    fn test_reproduction(
        paper_id: Uuid,
        evaluation: &Value,
        reproducer_id: Uuid,
        digest: &str,
    ) -> Value {
        let reproduction_id = Uuid::new_v4();
        let evaluation_id = Uuid::parse_str(evaluation["evaluation_id"].as_str().unwrap()).unwrap();
        let observed_metrics = serde_json::json!({"accuracy":900000});
        let statistical_evidence = serde_json::json!({});
        let rule_results = serde_json::json!([{
            "rule_key":"absolute:accuracy",
            "passed":true,
            "detail_hash":canonical_json_sha256(&serde_json::json!({
                "reference":900000,
                "observed":900000,
                "delta":"0",
                "maximum":1000,
            })).unwrap(),
        }]);
        let (public_key, public_key_hash, signature) = test_key_snapshot(24);
        let signing = PaperReproductionSigningV1 {
            schema: "hepta.paper_raid.reproduction.v1".into(),
            reproduction_id,
            evaluation_id,
            paper_project_id: paper_id,
            release_candidate_hash: evaluation["release_candidate_hash"]
                .as_str()
                .unwrap()
                .into(),
            paper_bundle_hash: evaluation["paper_bundle_hash"].as_str().unwrap().into(),
            tolerance_policy_hash: evaluation["tolerance_policy_hash"].as_str().unwrap().into(),
            observed_metrics_hash: canonical_json_sha256(&observed_metrics).unwrap(),
            statistical_evidence_hash: canonical_json_sha256(&statistical_evidence).unwrap(),
            seed_set_hash: digest.into(),
            environment_hash: digest.into(),
            run_manifest_hash: digest.into(),
            supersedes_reproduction_id: None,
            reproducer_player_id: reproducer_id,
            signing_key_id: "reproducer-key".into(),
            signing_public_key_hash: public_key_hash.clone(),
            coi_attestation_hash: digest.into(),
            signed_at_unix: 1_786_406_490,
        };
        let signing_hash =
            sha256_digest(&paper_reproduction_signing_bytes(&signing).expect("reproduction frame"));
        let report_hash = canonical_json_sha256(&serde_json::json!({
            "signing_hash":signing_hash,
            "rule_results":rule_results.clone(),
            "status":"reproduced",
        }))
        .unwrap();
        serde_json::json!({
            "schema":"hepta.paper_raid.reproduction.v1",
            "reproduction_id":reproduction_id,
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "release_candidate_hash":evaluation["release_candidate_hash"],
            "paper_bundle_hash":evaluation["paper_bundle_hash"],
            "tolerance_policy_hash":evaluation["tolerance_policy_hash"],
            "observed_metrics_micros":observed_metrics,
            "statistical_evidence":statistical_evidence,
            "seed_set_hash":digest,
            "environment_hash":digest,
            "run_manifest_hash":digest,
            "supersedes_reproduction_id":null,
            "reproducer_player_id":reproducer_id,
            "signing_key_id":"reproducer-key",
            "signing_public_key":public_key,
            "signing_public_key_hash":public_key_hash,
            "coi_attestation_hash":digest,
            "signed_at_unix":1_786_406_490_i64,
            "signature":signature,
            "rule_results":rule_results,
            "status":"reproduced",
            "report_hash":report_hash,
            "version":1,
            "created_at":"2026-08-11T00:01:30Z",
        })
    }

    fn refresh_test_reproduction(reproduction: &mut Value) {
        let reproduction_id =
            Uuid::parse_str(reproduction["reproduction_id"].as_str().unwrap()).unwrap();
        let evaluation_id =
            Uuid::parse_str(reproduction["evaluation_id"].as_str().unwrap()).unwrap();
        let paper_id = Uuid::parse_str(reproduction["paper_project_id"].as_str().unwrap()).unwrap();
        let supersedes_reproduction_id = match &reproduction["supersedes_reproduction_id"] {
            Value::Null => None,
            Value::String(value) => Some(Uuid::parse_str(value).unwrap()),
            _ => panic!("supersedes reproduction"),
        };
        let observed_metrics = reproduction["observed_metrics_micros"].clone();
        let statistical_evidence = reproduction["statistical_evidence"].clone();
        let signing = PaperReproductionSigningV1 {
            schema: "hepta.paper_raid.reproduction.v1".into(),
            reproduction_id,
            evaluation_id,
            paper_project_id: paper_id,
            release_candidate_hash: reproduction["release_candidate_hash"]
                .as_str()
                .unwrap()
                .into(),
            paper_bundle_hash: reproduction["paper_bundle_hash"].as_str().unwrap().into(),
            tolerance_policy_hash: reproduction["tolerance_policy_hash"]
                .as_str()
                .unwrap()
                .into(),
            observed_metrics_hash: canonical_json_sha256(&observed_metrics).unwrap(),
            statistical_evidence_hash: canonical_json_sha256(&statistical_evidence).unwrap(),
            seed_set_hash: reproduction["seed_set_hash"].as_str().unwrap().into(),
            environment_hash: reproduction["environment_hash"].as_str().unwrap().into(),
            run_manifest_hash: reproduction["run_manifest_hash"].as_str().unwrap().into(),
            supersedes_reproduction_id,
            reproducer_player_id: Uuid::parse_str(
                reproduction["reproducer_player_id"].as_str().unwrap(),
            )
            .unwrap(),
            signing_key_id: reproduction["signing_key_id"].as_str().unwrap().into(),
            signing_public_key_hash: reproduction["signing_public_key_hash"]
                .as_str()
                .unwrap()
                .into(),
            coi_attestation_hash: reproduction["coi_attestation_hash"]
                .as_str()
                .unwrap()
                .into(),
            signed_at_unix: reproduction["signed_at_unix"].as_i64().unwrap(),
        };
        let signing_hash =
            sha256_digest(&paper_reproduction_signing_bytes(&signing).expect("reproduction frame"));
        reproduction["report_hash"] =
            serde_json::json!(canonical_json_sha256(&serde_json::json!({
                "signing_hash":signing_hash,
                "rule_results":reproduction["rule_results"].clone(),
                "status":reproduction["status"].clone(),
            }))
            .unwrap());
    }

    fn after_action_fixture() -> (Uuid, AlphaIdentity, Value, Value, Value) {
        let paper_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let challenge_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let captain_id = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let experiment_id = Uuid::new_v4();
        let evaluator_id = Uuid::new_v4();
        let reviewer_1_id = Uuid::new_v4();
        let reviewer_2_id = Uuid::new_v4();
        let reproducer_id = Uuid::new_v4();
        let succeeded_run_id = Uuid::new_v4();
        let failed_run_id = Uuid::new_v4();
        let cancelled_run_id = Uuid::new_v4();
        let experiment_plan_id = Uuid::new_v4();
        let evidence_card_id = Uuid::new_v4();
        let evidence_action_id = Uuid::new_v4();
        let captain_action_id = Uuid::new_v4();
        let evidence_proposal_id = Uuid::new_v4();
        let evidence_decision_id = Uuid::new_v4();
        let evidence_artifact_id = Uuid::new_v4();
        let experiment_proposal_id = Uuid::new_v4();
        let experiment_decision_id = Uuid::new_v4();
        let experiment_artifact_id = Uuid::new_v4();
        let experiment_review_id = Uuid::new_v4();
        let contribution_ledger_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let digest = format!("sha256:{}", "d".repeat(64));
        let identity = AlphaIdentity::test_identity("captain", captain_id, Uuid::new_v4());

        let mut contribution_entries = vec![
            serde_json::json!({"player_id":captain_id,"credit_roles":["conceptualization"],"accepted_artifact_manifest_ids":[],"accepted_section_review_ids":[],"contribution_points":0}),
            serde_json::json!({"player_id":evidence_id,"credit_roles":["data_curation"],"accepted_artifact_manifest_ids":[evidence_artifact_id],"accepted_section_review_ids":[],"contribution_points":100}),
            serde_json::json!({"player_id":experiment_id,"credit_roles":["methodology","validation"],"accepted_artifact_manifest_ids":[experiment_artifact_id],"accepted_section_review_ids":[experiment_review_id],"contribution_points":250}),
        ];
        contribution_entries.sort_by_key(|entry| {
            Uuid::parse_str(entry["player_id"].as_str().expect("player")).expect("UUID")
        });
        let contribution_entries = Value::Array(contribution_entries);
        let ledger_hash = canonical_json_sha256(&serde_json::json!({
            "schema":"hepta.paper_raid.contribution_ledger.v1",
            "contribution_ledger_id":contribution_ledger_id,
            "paper_project_id":paper_id,
            "entries":contribution_entries,
        }))
        .expect("ledger hash");

        let allocation = serde_json::json!({
            "captain_focus":2,
            "evidence_focus":2,
            "experiment_focus":3,
            "run_budget":3,
            "retained_failure_focus_refund":1
        });
        let (ruleset_snapshot, ruleset_snapshot_hash) =
            test_ruleset_snapshot("Preserve <every> seeded finding.", Some(allocation.clone()));
        let release_candidate = serde_json::json!({
            "schema":"hepta.paper_raid.release_candidate.v2",
            "paper_project_id":paper_id,
            "revision_id":revision_id,
            "team_id":team_id,
            "challenge_id":challenge_id,
            "ruleset_hash":ruleset_snapshot["ruleset_hash"],
            "challenge_snapshot_hash":ruleset_snapshot["challenge_snapshot_hash"],
            "roster_version":1,
            "title":"AAR semantic fixture",
            "abstract_text":"A complete immutable fixture for the terminal report.",
            "target_format":"paper",
            "source_manifest_hash":digest,
            "artifact_manifest_hash":digest,
            "bibliography_hash":digest,
            "claim_evidence_graph_hash":digest,
            "collaboration_compact_hash":digest,
            "research_protocol_snapshot_hash":digest,
            "ethics_disclosure_hash":digest,
            "coi_disclosure_hash":digest,
            "contribution_ledger_hash":ledger_hash,
            "ai_disclosure_hash":digest,
            "license":"CC-BY-4.0",
            "authors":[
                {"author_order":1,"participant_slot":1,"player_id":captain_id,"display_name":"Captain","credit_roles":["conceptualization"]},
                {"author_order":2,"participant_slot":2,"player_id":evidence_id,"display_name":"Evidence","credit_roles":["data_curation"]},
                {"author_order":3,"participant_slot":3,"player_id":experiment_id,"display_name":"Experiment","credit_roles":["methodology","validation"]}
            ]
        });
        let release_candidate_record =
            serde_json::from_value::<PaperReleaseCandidateV2>(release_candidate.clone())
                .expect("release candidate");
        let release_candidate_hash = paper_release_candidate_hash(&release_candidate_record)
            .expect("release candidate hash");
        let paper_bundle =
            test_paper_bundle(release_candidate_record.clone(), &release_candidate_hash);
        let paper_bundle_hash = paper_bundle.paper_bundle_hash.clone();

        let score_components = serde_json::json!({
            "method_rigor_bps":2000,
            "experiment_statistics_bps":1200,
            "reproducibility_bps":1200,
            "evidence_citations_bps":1200,
            "value_originality_bps":1000,
            "argument_expression_bps":800,
            "ethics_transparency_bps":500
        });
        let hard_gates = serde_json::json!({
            "citations_and_data_authentic":true,
            "failed_runs_disclosed":true,
            "all_authors_consented":true,
            "core_claims_have_evidence":true,
            "artifact_lineage_complete":true,
            "license_ethics_coi_complete":true
        });
        let paper_score = serde_json::json!({
            "schema":"hepta.paper_raid.paper_score.v1",
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "components":score_components,
            "hard_gates":hard_gates,
            "score_bps":7900,
            "eligible":true,
            "score_hash":digest,
            "created_at":"2026-08-11T00:01:00Z"
        });
        let (evaluator_public_key, evaluator_public_key_hash, evaluator_signature) =
            test_key_snapshot(21);
        let (reviewer_1_public_key, reviewer_1_public_key_hash, reviewer_1_signature) =
            test_key_snapshot(22);
        let (reviewer_2_public_key, reviewer_2_public_key_hash, reviewer_2_signature) =
            test_key_snapshot(23);
        let mut reviewer_attestations = vec![
            serde_json::json!({
                "attestation_id":Uuid::new_v4(),
                "evaluation_id":evaluation_id,
                "evaluation_signing_hash":digest,
                "reviewer_player_id":reviewer_1_id,
                "verdict":"approve",
                "signing_key_id":"reviewer-1-key",
                "signing_public_key":reviewer_1_public_key,
                "signing_public_key_hash":reviewer_1_public_key_hash,
                "coi_attestation_hash":digest,
                "signed_at_unix":1786406460_i64,
                "signature":reviewer_1_signature
            }),
            serde_json::json!({
                "attestation_id":Uuid::new_v4(),
                "evaluation_id":evaluation_id,
                "evaluation_signing_hash":digest,
                "reviewer_player_id":reviewer_2_id,
                "verdict":"approve",
                "signing_key_id":"reviewer-2-key",
                "signing_public_key":reviewer_2_public_key,
                "signing_public_key_hash":reviewer_2_public_key_hash,
                "coi_attestation_hash":digest,
                "signed_at_unix":1786406460_i64,
                "signature":reviewer_2_signature
            }),
        ];
        reviewer_attestations.sort_by_key(|attestation| {
            Uuid::parse_str(
                attestation["reviewer_player_id"]
                    .as_str()
                    .expect("reviewer"),
            )
            .expect("UUID")
        });
        let mut evaluation = serde_json::json!({
            "schema":"hepta.paper_raid.evaluation.v1",
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "release_candidate_hash":release_candidate_hash,
            "paper_bundle_hash":paper_bundle_hash,
            "supersedes_evaluation_id":null,
            "tolerance_policy":{
                "schema":"hepta.paper_raid.tolerance_policy.v1",
                "version":"1",
                "rules":[{"kind":"absolute","metric":"accuracy","max_delta_micros":1000}]
            },
            "tolerance_policy_hash":digest,
            "reference_metrics_micros":{"accuracy":900000},
            "evaluator_player_id":evaluator_id,
            "evaluator_signing_key_id":"evaluator-key",
            "evaluator_signing_public_key":evaluator_public_key,
            "evaluator_signing_public_key_hash":evaluator_public_key_hash,
            "evaluator_coi_attestation_hash":digest,
            "evaluator_signed_at_unix":1786406460_i64,
            "evaluator_signature":evaluator_signature,
            "reviewer_attestations":reviewer_attestations,
            "paper_score":paper_score,
            "status":"accepted",
            "settlement_state":"pending_finality",
            "evaluation_signing_hash":digest,
            "version":1,
            "created_at":"2026-08-11T00:01:00Z"
        });
        refresh_test_evaluation(&mut evaluation);
        let mut assignments = test_consumed_panel_assignments(
            paper_id,
            submission_id,
            evaluation_id,
            1,
            evaluator_id,
            [reviewer_1_id, reviewer_2_id],
            "2026-08-11T00:01:00Z",
        );
        assignments.push(serde_json::json!({
            "schema":"hepta.paper_raid.review_assignment.v1",
            "assignment_id":Uuid::new_v4(),
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "player_id":reproducer_id,
            "review_round":1,
            "slot":"reproducer",
            "pinned_evaluation_id":null,
            "status":"claimed",
            "version":1,
            "claimed_at":"2026-08-11T00:01:05Z",
            "expires_at":"2026-08-11T02:00:00Z",
            "updated_at":"2026-08-11T00:01:05Z",
        }));
        let reproduction = test_reproduction(paper_id, &evaluation, reproducer_id, &digest);
        let reproduction_id = Uuid::parse_str(
            reproduction["reproduction_id"]
                .as_str()
                .expect("fixture reproduction ID"),
        )
        .expect("canonical fixture reproduction ID");

        let mut player_xp = serde_json::Map::new();
        player_xp.insert(captain_id.to_string(), serde_json::json!(300));
        player_xp.insert(evidence_id.to_string(), serde_json::json!(400));
        player_xp.insert(experiment_id.to_string(), serde_json::json!(550));
        let score_hash = canonical_json_sha256(&serde_json::json!({
            "schema":"hepta.paper_raid.raid_score.v1",
            "raid_score_id":evaluation_id,
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "team_xp":1250,
            "player_xp":player_xp,
            "paper_score_excluded":true,
        }))
        .expect("score hash");

        let role_resources = serde_json::json!({
            "schema":"hepta.paper_raid.role_resources.v1",
            "allocation":allocation,
            "captain_focus_remaining":1,
            "evidence_focus_remaining":1,
            "experiment_focus_remaining":1,
            "run_budget_remaining":0,
            "ranking_eligible":false,
            "reward_eligible":false,
            "economic_eligibility":false,
            "version":6,
            "created_at":"2026-08-11T00:00:00Z",
            "updated_at":"2026-08-11T00:00:50Z",
            "actions":[
                {"action_id":evidence_action_id,"kind":"evidence_assessment","actor_player_id":evidence_id,"actor_role":"evidence","subject_id":evidence_card_id,"focus_spent":1,"focus_refunded":0,"run_budget_spent":0,"occurred_at":"2026-08-11T00:00:10Z"},
                {"action_id":succeeded_run_id,"kind":"experiment_run","actor_player_id":experiment_id,"actor_role":"experiment","subject_id":succeeded_run_id,"focus_spent":1,"focus_refunded":0,"run_budget_spent":1,"occurred_at":"2026-08-11T00:00:20Z"},
                {"action_id":captain_action_id,"kind":"captain_checkpoint","actor_player_id":captain_id,"actor_role":"captain","subject_id":captain_action_id,"focus_spent":1,"focus_refunded":0,"run_budget_spent":0,"occurred_at":"2026-08-11T00:00:30Z"},
                {"action_id":failed_run_id,"kind":"experiment_run","actor_player_id":experiment_id,"actor_role":"experiment","subject_id":failed_run_id,"focus_spent":1,"focus_refunded":1,"run_budget_spent":1,"occurred_at":"2026-08-11T00:00:40Z"},
                {"action_id":cancelled_run_id,"kind":"experiment_run","actor_player_id":experiment_id,"actor_role":"experiment","subject_id":cancelled_run_id,"focus_spent":1,"focus_refunded":0,"run_budget_spent":1,"occurred_at":"2026-08-11T00:00:50Z"}
            ]
        });
        let paper = serde_json::json!({
            "paper_project_id":paper_id,
            "team_id":team_id,
            "challenge_id":challenge_id,
            "phase":"submission_ready",
            "outcome":"submission_ready",
            "created_at":"2026-08-11T00:00:00Z",
            "deadline_at":"2026-08-11T00:45:00Z",
            "terminal_at":"2026-08-11T00:01:00Z",
            "grace_expires_at":"2026-08-11T01:00:00Z",
            "updated_at":"2026-08-11T00:01:00Z",
            "release_candidate_revision_id":revision_id,
            "challenge_ruleset_snapshot":ruleset_snapshot,
            "challenge_ruleset_snapshot_hash":ruleset_snapshot_hash,
            "role_resources":role_resources
        });
        let team = serde_json::json!({
            "team_id":team_id,
            "challenge_id":challenge_id,
            "status":"locked",
            "roster_version":1,
            "members":[
                {"participant_slot":1,"player_id":captain_id,"role":"captain"},
                {"participant_slot":2,"player_id":evidence_id,"role":"evidence"},
                {"participant_slot":3,"player_id":experiment_id,"role":"experiment"}
            ]
        });
        let paper_revisions = serde_json::json!([{
            "revision_id":revision_id,
            "paper_project_id":paper_id,
            "status":"release_candidate",
            "release_candidate_hash":release_candidate_hash,
            "release_candidate":release_candidate
        }]);
        let joint_submission = serde_json::json!({
            "submission_id":submission_id,
            "paper_project_id":paper_id,
            "revision_id":revision_id,
            "release_candidate_hash":release_candidate_hash,
            "paper_bundle_hash":paper_bundle_hash,
            "status":"submission_ready",
            "paper_bundle":paper_bundle,
            "created_at":"2026-08-11T00:01:00Z"
        });
        let runs = serde_json::json!([
            {"run_record_id":succeeded_run_id,"paper_project_id":paper_id,"experiment_plan_id":experiment_plan_id,"status":"succeeded","seed":7,"parameters_hash":digest,"logs_manifest_id":Uuid::new_v4(),"outputs_manifest_id":Uuid::new_v4(),"metrics_hash":digest,"failure_hash":null,"version":1,"created_at":"2026-08-11T00:00:20Z"},
            {"run_record_id":failed_run_id,"paper_project_id":paper_id,"experiment_plan_id":experiment_plan_id,"status":"failed","seed":8,"parameters_hash":digest,"logs_manifest_id":Uuid::new_v4(),"outputs_manifest_id":null,"metrics_hash":null,"failure_hash":format!("sha256:{}", "a".repeat(64)),"version":1,"created_at":"2026-08-11T00:00:40Z"},
            {"run_record_id":cancelled_run_id,"paper_project_id":paper_id,"experiment_plan_id":experiment_plan_id,"status":"cancelled","seed":9,"parameters_hash":digest,"logs_manifest_id":Uuid::new_v4(),"outputs_manifest_id":null,"metrics_hash":null,"failure_hash":format!("sha256:{}", "b".repeat(64)),"version":1,"created_at":"2026-08-11T00:00:50Z"}
        ]);
        let room = serde_json::json!({
            "paper":paper,
            "team":team,
            "paper_revisions":paper_revisions,
            "joint_submission":joint_submission,
            "evidence_cards":[{"evidence_card_id":evidence_card_id,"paper_project_id":paper_id}],
            "runs":runs,
            "artifact_manifests":[
                {"manifest_id":evidence_artifact_id,"paper_project_id":paper_id},
                {"manifest_id":experiment_artifact_id,"paper_project_id":paper_id}
            ],
            "proposals":[
                {"proposal_id":evidence_proposal_id,"paper_project_id":paper_id,"artifact_manifest_id":evidence_artifact_id,"status":"accepted"},
                {"proposal_id":experiment_proposal_id,"paper_project_id":paper_id,"artifact_manifest_id":experiment_artifact_id,"status":"accepted"}
            ],
            "decisions":[
                {"decision_id":evidence_decision_id,"paper_project_id":paper_id,"proposal_id":evidence_proposal_id,"player_id":evidence_id,"decision":"accept"},
                {"decision_id":experiment_decision_id,"paper_project_id":paper_id,"proposal_id":experiment_proposal_id,"player_id":experiment_id,"decision":"accept"}
            ],
            "section_reviews":[
                {"review_id":experiment_review_id,"paper_project_id":paper_id,"reviewer_player_id":experiment_id,"verdict":"approve"}
            ]
        });
        let review = serde_json::json!({
            "evaluation_drafts":[],
            "assignments":assignments,
            "contribution_ledgers":[{
                "schema":"hepta.paper_raid.contribution_ledger.v1",
                "contribution_ledger_id":contribution_ledger_id,
                "paper_project_id":paper_id,
                "release_candidate_hash":release_candidate_hash,
                "entries":contribution_entries,
                "ledger_hash":ledger_hash,
                "version":1,
                "created_at":"2026-08-11T00:00:00Z"
            }],
            "evaluations":[evaluation],
            "reproductions":[reproduction],
            "appeals":[],
            "resolutions":[],
            "raid_scores":[{
                "schema":"hepta.paper_raid.raid_score.v1",
                "raid_score_id":evaluation_id,
                "evaluation_id":evaluation_id,
                "paper_project_id":paper_id,
                "team_xp":1250,
                "player_xp":player_xp,
                "paper_score_excluded":true,
                "score_hash":score_hash,
                "created_at":"2026-08-11T00:01:00Z"
            }]
        });
        let finality = serde_json::json!({
            "schema":"hepta.paper_raid.consumer_finality.v2",
            "status":"verified_finality",
            "effective_evaluation_id":evaluation_id,
            "effective_reproduction_id":reproduction_id,
            "effective_appeal_resolution_id":null,
            "ranking_eligible":false,
            "reward_eligible":false,
            "score_eligible":false,
            "economic_eligible":false,
            "verified_at":"2026-08-11T00:02:00Z"
        });
        (paper_id, identity, room, review, finality)
    }

    fn assert_whole_aar_unavailable(report: &str) {
        assert!(report.contains("After Action Report unavailable"));
        assert!(!report.contains("data-after-action-report=\"v1\""));
        assert!(!report.contains("after-action-runs"));
        assert!(!report.contains("after-action-roles"));
        assert!(!report.contains("after-action-contribution"));
        assert!(!report.contains("Team provisional XP"));
    }

    #[test]
    fn after_action_reference_arrays_accept_257_canonical_scientific_refs() {
        let paper_id = Uuid::new_v4();
        let credited_player_id = Uuid::new_v4();
        let teammate_1 = Uuid::new_v4();
        let teammate_2 = Uuid::new_v4();
        let artifact_ids = (1_u128..=257)
            .map(|value| Uuid::from_u128(10_000 + value))
            .collect::<Vec<_>>();
        let canonical = Value::Array(
            artifact_ids
                .iter()
                .map(|value| serde_json::json!(value))
                .collect(),
        );
        assert_eq!(canonical_uuid_array(Some(&canonical)).unwrap().len(), 257);

        let manifests = artifact_ids
            .iter()
            .map(|manifest_id| {
                serde_json::json!({"manifest_id":manifest_id,"paper_project_id":paper_id})
            })
            .collect::<Vec<_>>();
        let proposals = artifact_ids
            .iter()
            .enumerate()
            .map(|(index, manifest_id)| {
                serde_json::json!({
                    "proposal_id":Uuid::from_u128(20_000 + u128::try_from(index).unwrap()),
                    "paper_project_id":paper_id,
                    "artifact_manifest_id":manifest_id,
                    "status":"accepted"
                })
            })
            .collect::<Vec<_>>();
        let decisions = proposals
            .iter()
            .enumerate()
            .map(|(index, proposal)| {
                serde_json::json!({
                    "decision_id":Uuid::from_u128(30_000 + u128::try_from(index).unwrap()),
                    "paper_project_id":paper_id,
                    "proposal_id":proposal["proposal_id"],
                    "player_id":credited_player_id,
                    "decision":"accept"
                })
            })
            .collect::<Vec<_>>();
        let room = serde_json::json!({
            "team":{"members":[
                {"player_id":credited_player_id},
                {"player_id":teammate_1},
                {"player_id":teammate_2}
            ]},
            "artifact_manifests":manifests,
            "proposals":proposals,
            "decisions":decisions,
            "section_reviews":[]
        });
        let (artifacts, reviews) = authoritative_contribution_refs(
            &room,
            &paper_id.to_string(),
            &credited_player_id.to_string(),
        )
        .expect("complete scientific refs");
        assert_eq!(artifacts.len(), 257);
        assert!(reviews.is_empty());

        let mut noncanonical = canonical.clone();
        noncanonical.as_array_mut().unwrap().swap(0, 256);
        assert!(canonical_uuid_array(Some(&noncanonical)).is_none());

        let mut alternate_encoding = canonical;
        let first = alternate_encoding[0].as_str().unwrap().replace('-', "");
        alternate_encoding[0] = Value::String(first);
        assert!(canonical_uuid_array(Some(&alternate_encoding)).is_none());
    }

    #[test]
    fn after_action_report_is_terminal_explanatory_and_non_economic() {
        let (paper_id, identity, room, review, finality) = after_action_fixture();
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&review),
            &finality,
        );
        assert!(report.contains("data-after-action-report=\"v1\""));
        assert!(report.contains("<strong>1</strong> failed run(s) retained a disclosure"));
        assert!(report.contains("<strong>1</strong> have an authoritative bounded focus refund"));
        assert!(report.contains("Captain: 1 checkpoint(s)"));
        assert!(report.contains("captain · You / 你"));
        assert!(report
            .contains("1 accepted artifact(s) · 0 accepted review(s) · 100 provisional point(s)"));
        assert!(report.contains("Team provisional XP: 1250 · Your provisional XP: 300"));
        assert!(report.contains("&lt;every&gt;"));
        assert!(!report.contains("<every>"));
        assert_eq!(report.matches("data-eligible=\"false\"").count(), 4);
        assert!(report.contains("not authoritative yet"));
        assert!(!report.contains(&identity.player_id.to_string()));
    }

    #[test]
    fn after_action_report_rejects_semantic_tampering() {
        let (paper_id, identity, room, review, finality) = after_action_fixture();
        let mut tampered_review = review.clone();
        tampered_review["contribution_ledgers"][0]["entries"][0]
            ["accepted_artifact_manifest_ids"] = serde_json::json!([Uuid::new_v4()]);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&tampered_review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut cross_paper_room = room.clone();
        cross_paper_room["proposals"][0]["paper_project_id"] = serde_json::json!(Uuid::new_v4());
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &cross_paper_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut release_room = room.clone();
        release_room["paper"]["release_candidate_revision_id"] = serde_json::json!(Uuid::new_v4());
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &release_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut roster_room = room.clone();
        roster_room["paper_revisions"][0]["release_candidate"]["authors"][0]["credit_roles"] =
            serde_json::json!(["software"]);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &roster_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut paper_score_review = review.clone();
        paper_score_review["evaluations"][0]["paper_score"]["components"]["method_rigor_bps"] =
            serde_json::json!(2001);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&paper_score_review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut status_review = review.clone();
        status_review["evaluations"][0]["paper_score"]["hard_gates"]["failed_runs_disclosed"] =
            serde_json::json!(false);
        status_review["evaluations"][0]["paper_score"]["eligible"] = serde_json::json!(false);
        let paper_score = &status_review["evaluations"][0]["paper_score"];
        let replacement_hash = canonical_json_sha256(&serde_json::json!({
            "schema":"hepta.paper_raid.paper_score.v1",
            "evaluation_id":paper_score["evaluation_id"],
            "paper_project_id":paper_score["paper_project_id"],
            "components":paper_score["components"],
            "hard_gates":paper_score["hard_gates"],
            "score_bps":paper_score["score_bps"],
            "eligible":false
        }))
        .expect("tampered PaperScore hash");
        status_review["evaluations"][0]["paper_score"]["score_hash"] =
            Value::String(replacement_hash);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&status_review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut quorum_review = review.clone();
        let duplicate_reviewer = quorum_review["evaluations"][0]["reviewer_attestations"][0]
            ["reviewer_player_id"]
            .clone();
        quorum_review["evaluations"][0]["reviewer_attestations"][1]["reviewer_player_id"] =
            duplicate_reviewer;
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&quorum_review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut action_room = room.clone();
        action_room["paper"]["role_resources"]["actions"][1]["action_id"] =
            serde_json::json!(Uuid::new_v4());
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &action_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut refund_room = room.clone();
        refund_room["paper"]["role_resources"]["actions"][3]["focus_refunded"] =
            serde_json::json!(0);
        refund_room["paper"]["role_resources"]["experiment_focus_remaining"] = serde_json::json!(0);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &refund_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut actor_room = room.clone();
        actor_room["paper"]["role_resources"]["actions"][1]["actor_player_id"] =
            serde_json::json!(identity.player_id);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &actor_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut snapshot_room = room.clone();
        snapshot_room["paper"]["challenge_ruleset_snapshot"]["ruleset"]["gameplay"]
            ["victory_summary"] = serde_json::json!("tampered victory");
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &snapshot_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut roster_slot_room = room.clone();
        roster_slot_room["team"]["members"][1]["participant_slot"] = serde_json::json!(1);
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &roster_slot_room,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut early_finality = finality.clone();
        early_finality["verified_at"] = serde_json::json!("2026-08-11T00:00:59Z");
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&review),
            &early_finality,
        );
        assert_whole_aar_unavailable(&tampered);

        for field in [
            "effective_evaluation_id",
            "effective_reproduction_id",
            "effective_appeal_resolution_id",
        ] {
            let mut mismatched_finality = finality.clone();
            mismatched_finality[field] = serde_json::json!(Uuid::new_v4());
            let tampered = after_action_report(
                &identity,
                &paper_id.to_string(),
                &room,
                ReadState::Available(&review),
                &mismatched_finality,
            );
            assert_whole_aar_unavailable(&tampered);
        }
        let mut legacy_finality = finality.clone();
        legacy_finality["schema"] = serde_json::json!("hepta.paper_raid.consumer_finality.v1");
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&review),
            &legacy_finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut duplicate_assignment = review.clone();
        duplicate_assignment["assignments"][1]["assignment_id"] =
            duplicate_assignment["assignments"][0]["assignment_id"].clone();
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&duplicate_assignment),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);

        let mut unlinked_reproduction = review.clone();
        unlinked_reproduction["reproductions"][0]["evaluation_id"] =
            serde_json::json!(Uuid::new_v4());
        let tampered = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&unlinked_reproduction),
            &finality,
        );
        assert_whole_aar_unavailable(&tampered);
    }

    #[test]
    fn after_action_report_rejects_truncated_run_and_evaluation_facts() {
        let (paper_id, identity, room, review, finality) = after_action_fixture();
        let mut truncated_run = room.clone();
        truncated_run["runs"][0]
            .as_object_mut()
            .unwrap()
            .remove("outputs_manifest_id");
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &truncated_run,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&report);

        let mut empty_team = room.clone();
        empty_team["team"]["members"] = serde_json::json!([]);
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &empty_team,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&report);

        let mut noncanonical_player = room.clone();
        noncanonical_player["team"]["members"][0]["player_id"] =
            Value::String(identity.player_id.to_string().replace('-', ""));
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &noncanonical_player,
            ReadState::Available(&review),
            &finality,
        );
        assert_whole_aar_unavailable(&report);

        let mut truncated_evaluation = review.clone();
        truncated_evaluation["evaluations"][0]
            .as_object_mut()
            .unwrap()
            .remove("reference_metrics_micros");
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&truncated_evaluation),
            &finality,
        );
        assert_whole_aar_unavailable(&report);
    }

    #[test]
    fn after_action_report_enforces_terminal_grace_and_reason_complements() {
        let (paper_id, _identity, room, _review, _finality) = after_action_fixture();
        let paper_id_text = paper_id.to_string();
        let assert_invalid = |candidate: &Value| {
            assert!(matches!(
                validated_aar_terminal(&candidate["paper"], &paper_id_text),
                AarTerminalState::Invalid
            ));
        };

        let mut success_at_grace = room.clone();
        success_at_grace["paper"]["grace_expires_at"] =
            success_at_grace["paper"]["terminal_at"].clone();
        assert_invalid(&success_at_grace);

        let mut failed_at_grace = room.clone();
        failed_at_grace["paper"]["phase"] = serde_json::json!("experimenting");
        failed_at_grace["paper"]["outcome"] = serde_json::json!("failed");
        failed_at_grace["paper"]["outcome_reason"] = serde_json::json!("quality_gate_failed");
        failed_at_grace["paper"]["grace_expires_at"] =
            failed_at_grace["paper"]["terminal_at"].clone();
        assert_invalid(&failed_at_grace);

        let mut expired_with_wrong_reason = room.clone();
        expired_with_wrong_reason["paper"]["phase"] = serde_json::json!("integrity_hold");
        expired_with_wrong_reason["paper"]["outcome"] = serde_json::json!("expired");
        expired_with_wrong_reason["paper"]["outcome_reason"] =
            serde_json::json!("legacy_elapsed_reason");
        expired_with_wrong_reason["paper"]["grace_expires_at"] =
            expired_with_wrong_reason["paper"]["terminal_at"].clone();
        assert_invalid(&expired_with_wrong_reason);

        let mut automatic_expiry = room.clone();
        automatic_expiry["paper"]["phase"] = serde_json::json!("integrity_hold");
        automatic_expiry["paper"]["outcome"] = serde_json::json!("expired");
        automatic_expiry["paper"]["outcome_reason"] =
            serde_json::json!("challenge_grace_deadline_elapsed");
        automatic_expiry["paper"]["terminal_at"] =
            automatic_expiry["paper"]["grace_expires_at"].clone();
        automatic_expiry["paper"]["updated_at"] =
            automatic_expiry["paper"]["grace_expires_at"].clone();
        assert!(matches!(
            validated_aar_terminal(&automatic_expiry["paper"], &paper_id_text),
            AarTerminalState::Terminal(ValidatedAarTerminal {
                outcome: ValidatedAarOutcome::Expired(
                    HeptaPaperTerminalReason::ChallengeGraceDeadlineElapsed
                ),
                ..
            })
        ));

        let mut missing_deadline = room.clone();
        missing_deadline["paper"]
            .as_object_mut()
            .unwrap()
            .remove("deadline_at");
        assert_invalid(&missing_deadline);

        let mut missing_grace = room.clone();
        missing_grace["paper"]
            .as_object_mut()
            .unwrap()
            .remove("grace_expires_at");
        assert_invalid(&missing_grace);

        let mut reversed_boundaries = room.clone();
        reversed_boundaries["paper"]["deadline_at"] = serde_json::json!("2026-08-11T01:00:00Z");
        reversed_boundaries["paper"]["grace_expires_at"] =
            serde_json::json!("2026-08-11T00:45:00Z");
        assert_invalid(&reversed_boundaries);

        let mut unknown_phase = room.clone();
        unknown_phase["paper"]["phase"] = serde_json::json!("archived");
        assert_invalid(&unknown_phase);

        let mut unknown_failed_reason = room.clone();
        unknown_failed_reason["paper"]["phase"] = serde_json::json!("drafting");
        unknown_failed_reason["paper"]["outcome"] = serde_json::json!("failed");
        unknown_failed_reason["paper"]["outcome_reason"] = serde_json::json!("invented");
        assert_invalid(&unknown_failed_reason);

        let mut noncanonical_time = room;
        noncanonical_time["paper"]["deadline_at"] = serde_json::json!("2026-08-11T00:45:00+00:00");
        assert_invalid(&noncanonical_time);
    }

    #[test]
    fn after_action_report_derives_effective_score_across_appeal_outcomes() {
        let (paper_id, identity, room, mut review, mut finality) = after_action_fixture();
        finality["status"] = serde_json::json!("pending_finality");
        finality["effective_evaluation_id"] = Value::Null;
        finality["effective_reproduction_id"] = Value::Null;
        finality["effective_appeal_resolution_id"] = Value::Null;
        finality["verified_at"] = Value::Null;
        let base_evaluation_id =
            Uuid::parse_str(review["evaluations"][0]["evaluation_id"].as_str().unwrap()).unwrap();
        let replacement_evaluation_id = Uuid::new_v4();
        let replacement_evaluator_id = Uuid::new_v4();
        let replacement_reviewer_1_id = Uuid::new_v4();
        let replacement_reviewer_2_id = Uuid::new_v4();
        let (replacement_evaluator_key, replacement_evaluator_key_hash, replacement_signature) =
            test_key_snapshot(31);
        let (replacement_reviewer_1_key, replacement_reviewer_1_key_hash, reviewer_1_signature) =
            test_key_snapshot(32);
        let (replacement_reviewer_2_key, replacement_reviewer_2_key_hash, reviewer_2_signature) =
            test_key_snapshot(33);
        let mut replacement = review["evaluations"][0].clone();
        replacement["evaluation_id"] = serde_json::json!(replacement_evaluation_id);
        replacement["supersedes_evaluation_id"] = serde_json::json!(base_evaluation_id);
        replacement["version"] = serde_json::json!(2);
        replacement["settlement_state"] = serde_json::json!("pending_finality");
        // A superseding evaluation is prepared while the parent Appeal is
        // open, then the later resolution activates it.  Its own downstream
        // Appeal/resolution chain must not predate that activation.
        replacement["created_at"] = serde_json::json!("2026-08-11T00:01:20Z");
        replacement["evaluator_player_id"] = serde_json::json!(replacement_evaluator_id);
        replacement["evaluator_signing_public_key"] = serde_json::json!(replacement_evaluator_key);
        replacement["evaluator_signing_public_key_hash"] =
            serde_json::json!(replacement_evaluator_key_hash);
        replacement["evaluator_signature"] = serde_json::json!(replacement_signature);
        replacement["reviewer_attestations"][0]["attestation_id"] =
            serde_json::json!(Uuid::new_v4());
        replacement["reviewer_attestations"][0]["reviewer_player_id"] =
            serde_json::json!(replacement_reviewer_1_id);
        replacement["reviewer_attestations"][0]["signing_public_key"] =
            serde_json::json!(replacement_reviewer_1_key);
        replacement["reviewer_attestations"][0]["signing_public_key_hash"] =
            serde_json::json!(replacement_reviewer_1_key_hash);
        replacement["reviewer_attestations"][0]["signature"] =
            serde_json::json!(reviewer_1_signature);
        replacement["reviewer_attestations"][1]["attestation_id"] =
            serde_json::json!(Uuid::new_v4());
        replacement["reviewer_attestations"][1]["reviewer_player_id"] =
            serde_json::json!(replacement_reviewer_2_id);
        replacement["reviewer_attestations"][1]["signing_public_key"] =
            serde_json::json!(replacement_reviewer_2_key);
        replacement["reviewer_attestations"][1]["signing_public_key_hash"] =
            serde_json::json!(replacement_reviewer_2_key_hash);
        replacement["reviewer_attestations"][1]["signature"] =
            serde_json::json!(reviewer_2_signature);
        replacement["reviewer_attestations"][1]["verdict"] = serde_json::json!("reject");
        refresh_test_evaluation(&mut replacement);
        review["evaluations"]
            .as_array_mut()
            .unwrap()
            .push(replacement);
        let canonical_submission_id =
            Uuid::parse_str(review["evaluations"][0]["submission_id"].as_str().unwrap()).unwrap();
        review["assignments"]
            .as_array_mut()
            .unwrap()
            .extend(test_consumed_panel_assignments(
                paper_id,
                canonical_submission_id,
                replacement_evaluation_id,
                2,
                replacement_evaluator_id,
                [replacement_reviewer_1_id, replacement_reviewer_2_id],
                "2026-08-11T00:01:20Z",
            ));

        let zero_player_xp = review["raid_scores"][0]["player_xp"]
            .as_object()
            .unwrap()
            .keys()
            .map(|player_id| (player_id.clone(), serde_json::json!(0)))
            .collect::<serde_json::Map<_, _>>();
        let replacement_score_hash = canonical_json_sha256(&serde_json::json!({
            "schema":"hepta.paper_raid.raid_score.v1",
            "raid_score_id":replacement_evaluation_id,
            "evaluation_id":replacement_evaluation_id,
            "paper_project_id":paper_id,
            "team_xp":0,
            "player_xp":zero_player_xp,
            "paper_score_excluded":true,
        }))
        .unwrap();
        review["raid_scores"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "schema":"hepta.paper_raid.raid_score.v1",
                "raid_score_id":replacement_evaluation_id,
                "evaluation_id":replacement_evaluation_id,
                "paper_project_id":paper_id,
                "team_xp":0,
                "player_xp":zero_player_xp,
                "paper_score_excluded":true,
                "score_hash":replacement_score_hash,
                "created_at":"2026-08-11T00:01:20Z"
            }));

        let appeal_id = Uuid::new_v4();
        let (appeal_public_key, appeal_public_key_hash, appeal_signature) = test_key_snapshot(34);
        let release_candidate_hash = review["contribution_ledgers"][0]["release_candidate_hash"]
            .as_str()
            .unwrap()
            .to_string();
        let appeal = serde_json::json!({
            "schema":"hepta.paper_raid.appeal.v1",
            "appeal_id":appeal_id,
            "evaluation_id":base_evaluation_id,
            "paper_project_id":paper_id,
            "release_candidate_hash":release_candidate_hash,
            "appellant_player_id":identity.player_id,
            "grounds_hash":format!("sha256:{}", "4".repeat(64)),
            "evidence_manifest_hash":format!("sha256:{}", "5".repeat(64)),
            "signing_key_id":"appellant-key",
            "signing_public_key":appeal_public_key,
            "signing_public_key_hash":appeal_public_key_hash,
            "signed_at_unix":1786406470_i64,
            "signature":appeal_signature,
            "version":1,
            "created_at":"2026-08-11T00:01:10Z"
        });
        review["evaluations"][0]["settlement_state"] = serde_json::json!("challenged");
        review["appeals"] = serde_json::json!([appeal]);
        let prepared_child_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&review),
            &finality,
        );
        assert!(prepared_child_report.contains("data-after-action-report=\"v1\""));
        assert!(prepared_child_report.contains("Appeal open · Team provisional XP: 1250"));
        assert!(!prepared_child_report.contains("Team provisional XP: 0"));

        let mut open_review = review.clone();
        open_review["evaluations"].as_array_mut().unwrap().pop();
        open_review["raid_scores"].as_array_mut().unwrap().pop();
        open_review["assignments"]
            .as_array_mut()
            .unwrap()
            .retain(|assignment| assignment["review_round"].as_u64() == Some(1));
        let open_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&open_review),
            &finality,
        );
        assert!(open_report.contains("Appeal open · Team provisional XP: 1250"));

        let resolver_id = Uuid::new_v4();
        let (resolver_public_key, resolver_public_key_hash, resolver_signature) =
            test_key_snapshot(35);
        let resolution_id = Uuid::new_v4();
        let mut resolution = serde_json::json!({
            "schema":"hepta.paper_raid.appeal_resolution.v1",
            "resolution_id":resolution_id,
            "appeal_id":appeal_id,
            "evaluation_id":base_evaluation_id,
            "paper_project_id":paper_id,
            "release_candidate_hash":release_candidate_hash,
            "outcome":"denied",
            "superseding_evaluation_id":null,
            "decision_hash":format!("sha256:{}", "6".repeat(64)),
            "resolver_player_id":resolver_id,
            "signing_key_id":"resolver-key",
            "signing_public_key":resolver_public_key,
            "signing_public_key_hash":resolver_public_key_hash,
            "signed_at_unix":1786406500_i64,
            "signature":resolver_signature,
            "version":1,
            "created_at":"2026-08-11T00:01:30Z"
        });
        let mut invalid_denied_review = review.clone();
        invalid_denied_review["evaluations"][0]["settlement_state"] = serde_json::json!("resolved");
        invalid_denied_review["resolutions"] = serde_json::json!([resolution.clone()]);
        let invalid_denied_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&invalid_denied_review),
            &finality,
        );
        assert_whole_aar_unavailable(&invalid_denied_report);

        let mut appeal_before_evaluation = review.clone();
        appeal_before_evaluation["appeals"][0]["created_at"] =
            serde_json::json!("2026-08-11T00:00:59Z");
        let appeal_before_evaluation_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&appeal_before_evaluation),
            &finality,
        );
        assert_whole_aar_unavailable(&appeal_before_evaluation_report);

        let mut resolution_before_appeal = invalid_denied_review.clone();
        resolution_before_appeal["resolutions"][0]["created_at"] =
            serde_json::json!("2026-08-11T00:01:05Z");
        let resolution_before_appeal_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&resolution_before_appeal),
            &finality,
        );
        assert_whole_aar_unavailable(&resolution_before_appeal_report);

        let mut denied_review = review.clone();
        denied_review["evaluations"].as_array_mut().unwrap().pop();
        denied_review["raid_scores"].as_array_mut().unwrap().pop();
        denied_review["assignments"]
            .as_array_mut()
            .unwrap()
            .retain(|assignment| assignment["review_round"].as_u64() == Some(1));
        denied_review["evaluations"][0]["settlement_state"] = serde_json::json!("resolved");
        denied_review["resolutions"] = serde_json::json!([resolution.clone()]);
        let denied_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&denied_review),
            &finality,
        );
        assert!(denied_report.contains("Appeal denied · Team provisional XP: 1250"));

        let mut premature_verified_finality = finality.clone();
        premature_verified_finality["status"] = serde_json::json!("verified_finality");
        premature_verified_finality["effective_evaluation_id"] =
            serde_json::json!(base_evaluation_id);
        premature_verified_finality["effective_reproduction_id"] =
            denied_review["reproductions"][0]["reproduction_id"].clone();
        premature_verified_finality["effective_appeal_resolution_id"] =
            serde_json::json!(resolution_id);
        premature_verified_finality["verified_at"] = serde_json::json!("2026-08-11T00:01:20Z");
        let premature_verified_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&denied_review),
            &premature_verified_finality,
        );
        assert_whole_aar_unavailable(&premature_verified_report);

        resolution["outcome"] = serde_json::json!("upheld");
        resolution["superseding_evaluation_id"] = serde_json::json!(replacement_evaluation_id);
        let mut upheld_review = review.clone();
        upheld_review["evaluations"][0]["settlement_state"] = serde_json::json!("resolved");
        upheld_review["resolutions"] = serde_json::json!([resolution.clone()]);
        let upheld_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&upheld_review),
            &finality,
        );
        assert!(upheld_report.contains("Appeal upheld · Team provisional XP: 0"));

        let mut premature_reproduction_review = upheld_review.clone();
        let reproducer_id = Uuid::parse_str(
            premature_reproduction_review["reproductions"][0]["reproducer_player_id"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let mut premature_reproduction = test_reproduction(
            paper_id,
            &premature_reproduction_review["evaluations"][1],
            reproducer_id,
            &format!("sha256:{}", "0".repeat(64)),
        );
        // The replacement evaluation exists while the parent Appeal is open,
        // but it only becomes authoritative at 00:01:30Z.  A reproduction
        // recorded before that activation must never support its finality.
        premature_reproduction["created_at"] = serde_json::json!("2026-08-11T00:01:25Z");
        let premature_reproduction_id = premature_reproduction["reproduction_id"].clone();
        let mut later_reproduction = premature_reproduction.clone();
        later_reproduction["reproduction_id"] = serde_json::json!(Uuid::new_v4());
        later_reproduction["supersedes_reproduction_id"] = premature_reproduction_id;
        later_reproduction["version"] = serde_json::json!(2);
        later_reproduction["created_at"] = serde_json::json!("2026-08-11T00:01:35Z");
        refresh_test_reproduction(&mut later_reproduction);
        premature_reproduction_review["reproductions"]
            .as_array_mut()
            .unwrap()
            .push(premature_reproduction);
        premature_reproduction_review["reproductions"]
            .as_array_mut()
            .unwrap()
            .push(later_reproduction);
        premature_reproduction_review["reproductions"]
            .as_array_mut()
            .unwrap()
            .sort_by(|left, right| {
                left["reproduction_id"]
                    .as_str()
                    .cmp(&right["reproduction_id"].as_str())
            });
        let replacement_submission_id =
            premature_reproduction_review["evaluations"][1]["submission_id"].clone();
        premature_reproduction_review["assignments"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "schema":"hepta.paper_raid.review_assignment.v1",
                "assignment_id":Uuid::new_v4(),
                "paper_project_id":paper_id,
                "submission_id":replacement_submission_id,
                "player_id":reproducer_id,
                "review_round":2,
                "slot":"reproducer",
                "pinned_evaluation_id":null,
                "status":"claimed",
                "version":1,
                "claimed_at":"2026-08-11T00:01:21Z",
                "expires_at":"2026-08-11T02:00:00Z",
                "updated_at":"2026-08-11T00:01:21Z",
            }));
        let premature_reproduction_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&premature_reproduction_review),
            &finality,
        );
        assert_whole_aar_unavailable(&premature_reproduction_report);

        let mut child_before_parent_resolution = upheld_review.clone();
        let mut premature_child_appeal = child_before_parent_resolution["appeals"][0].clone();
        premature_child_appeal["appeal_id"] = serde_json::json!(Uuid::new_v4());
        premature_child_appeal["evaluation_id"] = serde_json::json!(replacement_evaluation_id);
        premature_child_appeal["created_at"] = serde_json::json!("2026-08-11T00:01:25Z");
        child_before_parent_resolution["appeals"]
            .as_array_mut()
            .unwrap()
            .push(premature_child_appeal);
        child_before_parent_resolution["evaluations"][1]["settlement_state"] =
            serde_json::json!("challenged");
        let child_before_parent_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&child_before_parent_resolution),
            &finality,
        );
        assert_whole_aar_unavailable(&child_before_parent_report);

        resolution["superseding_evaluation_id"] = serde_json::json!(Uuid::new_v4());
        upheld_review["resolutions"] = serde_json::json!([resolution]);
        let malformed_report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &room,
            ReadState::Available(&upheld_review),
            &finality,
        );
        assert_whole_aar_unavailable(&malformed_report);
    }

    #[test]
    fn after_action_report_stays_hidden_or_fails_closed() {
        let paper_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let identity = AlphaIdentity::test_identity("captain", player_id, Uuid::new_v4());
        let active = serde_json::json!({
            "paper":{"paper_project_id":paper_id,"phase":"experimenting","outcome":"in_progress"},
            "team":{}
        });
        let finality = serde_json::json!({
            "schema":"hepta.paper_raid.consumer_finality.v2",
            "status":"pending_finality",
            "effective_evaluation_id":null,
            "effective_reproduction_id":null,
            "effective_appeal_resolution_id":null,
            "ranking_eligible":false,
            "reward_eligible":false,
            "score_eligible":false,
            "economic_eligible":false,
            "verified_at":null
        });
        assert!(after_action_report(
            &identity,
            &paper_id.to_string(),
            &active,
            ReadState::Unavailable,
            &finality,
        )
        .is_empty());

        let terminal = serde_json::json!({
            "paper":{"paper_project_id":paper_id,"phase":"submission_ready","outcome":"submission_ready","created_at":"2026-08-10T23:59:00Z","deadline_at":"2026-08-11T00:45:00Z","terminal_at":"2026-08-11T00:00:00Z","grace_expires_at":"2026-08-11T01:00:00Z","updated_at":"2026-08-11T00:00:00Z"},
            "team":{"members":[]},
            "runs":[]
        });
        let mut unsafe_finality = finality.clone();
        unsafe_finality["economic_eligible"] = serde_json::json!(true);
        let report = after_action_report(
            &identity,
            &paper_id.to_string(),
            &terminal,
            ReadState::Unavailable,
            &unsafe_finality,
        );
        assert_whole_aar_unavailable(&report);

        let missing_review = after_action_report(
            &identity,
            &paper_id.to_string(),
            &terminal,
            ReadState::Unavailable,
            &finality,
        );
        assert_whole_aar_unavailable(&missing_review);

        let unavailable_room = authenticated_after_action_report(
            &identity,
            &paper_id.to_string(),
            ReadState::<AuthenticatedPaperRoom>::Unavailable,
            ReadState::<AuthenticatedPaperReviewState>::Unavailable,
        );
        assert_whole_aar_unavailable(&unavailable_room);
    }

    #[test]
    fn role_resource_panel_is_actor_scoped_typed_and_non_economic() {
        let paper_id = Uuid::new_v4();
        let assessed_id = Uuid::new_v4();
        let available_id = Uuid::new_v4();
        let actor_id = Uuid::new_v4();
        let progress = serde_json::json!({
            "role_resources": {
                "schema": "hepta.paper_raid.role_resource_projection.v1",
                "actor_role": "evidence",
                "state_version": 3,
                "actor_focus_remaining": 2,
                "run_budget_remaining": 3,
                "evidence_assessment_count": 1,
                "experiment_run_count": 1,
                "captain_checkpoint_count": 0,
                "retained_failure_count": 1,
                "actions": [
                    {"action":"assess_evidence","available":true,"blockers":[]},
                    {"action":"create_run_record","available":false,"blockers":["experiment_role_required"]},
                    {"action":"coordinate_checkpoint","available":false,"blockers":["captain_role_required"]}
                ],
                "ranking_eligible": false,
                "reward_eligible": false,
                "economic_eligibility": false
            }
        });
        let paper = serde_json::json!({
            "role_resources": {
                "actions": [{
                    "kind": "evidence_assessment",
                    "actor_player_id": actor_id,
                    "subject_id": assessed_id
                }]
            }
        });
        let room = serde_json::json!({
            "evidence_cards": [
                {"evidence_card_id":assessed_id,"source_uri":"https://example.invalid/old"},
                {"evidence_card_id":available_id,"source_uri":"<new evidence>"}
            ]
        });

        let html = role_resource_panel(
            &paper_id.to_string(),
            &paper,
            Some(&progress),
            &room,
            "evidence",
        );
        assert!(html.contains("NON-ECONOMIC GAMEPLAY / 非经济玩法"));
        assert!(html.contains("class=\"role-resource-action-form\""));
        assert!(html.contains("data-resource-version=\"3\""));
        assert!(html.contains(&format!("value=\"{available_id}\"")));
        assert!(!html.contains(&format!("value=\"{assessed_id}\"")));
        assert!(html.contains("&lt;new evidence&gt;"));
        assert!(!html.contains("<new evidence>"));
        for forbidden in [
            "name=\"action_id\"",
            "name=\"expected_resource_version\"",
            "name=\"idempotency_key\"",
            "name=\"payload\"",
            "ranking_eligible=true",
            "reward_eligible=true",
            "economic_eligibility=true",
        ] {
            assert!(!html.contains(forbidden));
        }
    }

    #[test]
    fn experiment_run_upload_is_hidden_when_authority_denies_resources() {
        let room = serde_json::json!({
            "paper": {
                "version": 7,
                "challenge_id": Uuid::new_v4(),
                "outcome": "in_progress"
            },
            "author_raid_progress": {
                "role_resources": {
                    "schema": "hepta.paper_raid.role_resource_projection.v1",
                    "state_version": 4,
                    "actions": [{
                        "action": "create_run_record",
                        "available": false,
                        "blockers": ["run_budget_exhausted"]
                    }]
                }
            },
            "artifact_manifests": [{
                "manifest_id": Uuid::new_v4(),
                "source_bundle_id": "logs"
            }],
            "experiment_plans": [{
                "experiment_plan_id": Uuid::new_v4(),
                "protocol_snapshot_hash": format!("sha256:{}", "a".repeat(64))
            }],
            "runs": [],
            "figures": []
        });

        let html = science_action_panel_for_role(
            "paper-resource-exhausted",
            "experimenting",
            &room,
            "experiment",
            true,
        );
        assert!(html.contains("class=\"guided-action blocked run-resource-blocked\""));
        assert!(html.contains("Shared run budget exhausted / 共享运行预算已耗尽"));
        assert!(html.contains("disabled before any CAS mutation"));
        assert!(!html.contains("class=\"run-artifact-wizard-form"));
        assert!(!html.contains("class=\"create-run-record-form"));
    }

    #[test]
    fn experiment_run_upload_fails_closed_when_resource_action_is_missing() {
        let room = serde_json::json!({
            "paper": {"version": 8, "challenge_id": Uuid::new_v4()},
            "author_raid_progress": {
                "role_resources": {
                    "schema": "hepta.paper_raid.role_resource_projection.v1",
                    "state_version": 5,
                    "actions": []
                }
            },
            "artifact_manifests": [],
            "experiment_plans": [],
            "runs": [],
            "figures": []
        });

        let html = science_action_panel_for_role(
            "paper-resource-unavailable",
            "experimenting",
            &room,
            "experiment",
            true,
        );
        assert!(html.contains("Authoritative run-resource availability is unavailable"));
        assert!(!html.contains("class=\"run-artifact-wizard-form"));
        assert!(!html.contains("class=\"create-run-record-form"));
    }

    #[test]
    fn experiment_run_upload_fails_closed_when_typed_state_lacks_actor_projection() {
        let room = serde_json::json!({
            "paper": {
                "version": 9,
                "challenge_id": Uuid::new_v4(),
                "outcome": "in_progress",
                "role_resources": {
                    "schema": "hepta.paper_raid.role_resource_state.v1",
                    "version": 6
                }
            },
            "author_raid_progress": {},
            "artifact_manifests": [{
                "manifest_id": Uuid::new_v4(),
                "source_bundle_id": "logs"
            }],
            "experiment_plans": [{
                "experiment_plan_id": Uuid::new_v4(),
                "protocol_snapshot_hash": format!("sha256:{}", "b".repeat(64))
            }],
            "runs": [],
            "figures": []
        });

        let html = science_action_panel_for_role(
            "paper-resource-projection-missing",
            "experimenting",
            &room,
            "experiment",
            true,
        );
        assert!(html.contains("Authoritative run-resource availability is unavailable"));
        assert!(html.contains("disabled before any CAS mutation"));
        assert!(!html.contains("class=\"run-artifact-wizard-form"));
        assert!(!html.contains("class=\"create-run-record-form"));
    }

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
            ReadState::Unavailable,
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
    async fn formation_renders_authoritative_deadline_and_matched_ticket_withdrawal() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let proposal_id = Uuid::new_v4();
        let ticket_id = Uuid::new_v4();
        let proposal = serde_json::json!({
            "proposal_id": proposal_id,
            "status": "proposed",
            "version": 4,
            "expires_at": "2026-08-10T10:20:30Z",
            "member_player_ids": [identity.player_id],
        });
        let tickets = serde_json::json!([{
            "ticket_id": ticket_id,
            "status": "matched",
            "matched_proposal_id": proposal_id,
            "version": 2,
        }]);
        let response = formation(
            &identity,
            &proposal_id.to_string(),
            ReadState::Available(&proposal),
            ReadState::Unavailable,
            ReadState::Unavailable,
            ReadState::Unavailable,
            ReadState::Available(&tickets),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect formation")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 formation");
        assert!(body.contains(
            "class=\"proposal-countdown\" data-proposal-expires-at=\"2026-08-10T10:20:30Z\""
        ));
        assert!(body.contains("<time datetime=\"2026-08-10T10:20:30Z\">"));
        assert!(body.contains("class=\"cancel-ticket-form formation-withdraw-form\""));
        assert!(body.contains(&format!("data-ticket-id=\"{ticket_id}\"")));
        assert!(body.contains("data-ticket-version=\"2\""));
        assert!(body.contains("data-success-href=\"/league\""));
        assert!(body.contains("Withdraw from formation / 退出组队"));
    }

    #[tokio::test]
    async fn lobby_guides_the_first_raid_without_a_role_json_editor() {
        let mut identity =
            AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        identity.author_roles = vec![
            AlphaAuthorRole::Captain,
            AlphaAuthorRole::Evidence,
            AlphaAuthorRole::Experiment,
        ]
        .into();
        let challenges = serde_json::json!([{
            "challenge_id":"challenge-a",
            "title":"Reproduce the signal",
            "description":"Separate the claimed effect from measurement noise.",
            "status":"open"
        }]);
        let ticket_id = Uuid::new_v4();
        let tickets = serde_json::json!([{
            "ticket_id":ticket_id,
            "status":"queued",
            "private_party":true,
            "version":1,
            "matched_proposal_id":null,
            "queue_hint":{
                "state":"waiting",
                "queue_position":1,
                "compatible_pool_size":1,
                "compatible_players_needed":2,
                "missing_roles":["evidence","experiment"],
                "waited_seconds":12,
                "expires_in_seconds":1788,
                "eta_seconds":null,
                "message":"waiting_for_required_roles"
            }
        }]);
        let proposals = serde_json::json!([]);
        let bindings = serde_json::json!([]);
        let response = lobby(
            &identity,
            ReadState::Available(&challenges),
            ReadState::Available(&tickets),
            ReadState::Available(&proposals),
            ReadState::Available(&bindings),
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect lobby")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 lobby");
        assert!(body.contains("FIRST RAID / 首局任务"));
        assert!(body.contains("data-step-state=\"complete\"><span>01</span>"));
        assert!(body.contains("data-step-state=\"current\"><span>02</span>"));
        assert!(body.contains("data-authorized-roles=\"captain,evidence,experiment\""));
        for role in ["captain", "evidence", "experiment"] {
            assert!(body.contains(&format!("name=\"roles\" value=\"{role}\" checked")));
        }
        assert!(!body.contains("type=\"hidden\" value=\"captain,evidence,experiment\""));
        assert!(body.contains("Start my first Raid / 开始首局"));
        assert!(body.contains("name=\"party_code\" type=\"text\""));
        assert!(body.contains("class=\"generate-party-code\""));
        assert!(body.contains("Only its SHA-256 digest leaves this browser"));
        assert!(body.contains("Private three-person party queue"));
        assert!(!body.contains("party_code_hash"));
        assert!(body.contains("data-agent-ready=\"false\""));
        assert!(body.contains("请先配对且仅保留一个活跃 Agent"));
        assert!(body.contains(
            "<button class=\"queue-submit\" data-queue-action=\"join\" type=\"submit\" disabled>Start my first Raid"
        ));
        assert!(body.contains("class=\"cancel-ticket-form\""));
        assert!(body.contains(&format!("data-ticket-id=\"{ticket_id}\"")));
        assert!(body.contains("Leave queue / 取消排队"));
        assert!(body.contains("Compatible / 兼容人数: 1"));
        assert!(body.contains("Missing roles / 缺少角色: evidence, experiment"));
        assert!(body.contains("暂无法估算"));
        assert!(!body.contains("Roles / 职业<input"));
    }

    #[tokio::test]
    async fn lobby_only_enables_authoritative_open_challenges() {
        let mut identity =
            AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        identity.author_roles = vec![AlphaAuthorRole::Captain].into();
        let challenges = serde_json::json!([
            {
                "challenge_id":"open-challenge",
                "title":"Open challenge",
                "description":"Open",
                "status":"open"
            },
            {
                "challenge_id":"closed-challenge",
                "title":"Closed challenge",
                "description":"Closed",
                "status":"closed"
            },
            {
                "challenge_id":"unknown-challenge",
                "title":"Unknown challenge",
                "description":"Unknown",
                "status":"retired"
            }
        ]);
        let bindings = serde_json::json!([{
            "player_id": identity.player_id,
            "status": "active"
        }]);
        let empty = serde_json::json!([]);
        let response = lobby(
            &identity,
            ReadState::Available(&challenges),
            ReadState::Available(&empty),
            ReadState::Available(&empty),
            ReadState::Available(&bindings),
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect status-bound lobby")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 lobby");

        assert!(body.contains(
            "data-challenge-id=\"open-challenge\" data-challenge-status=\"open\" data-queue-eligible=\"true\""
        ));
        assert!(body.contains(
            "data-challenge-id=\"closed-challenge\" data-challenge-status=\"closed\" data-queue-eligible=\"false\""
        ));
        assert!(body.contains(
            "data-challenge-id=\"unknown-challenge\" data-challenge-status=\"unavailable\" data-queue-eligible=\"false\""
        ));
        assert!(body.contains("Hepta 已关闭该挑战，当前无法排队"));
        assert!(body.contains("type=\"submit\" disabled>Challenge closed / 挑战已关闭</button>"));
        assert!(body.contains("type=\"submit\" >Start my first Raid / 开始首局</button>"));
        assert!(body.contains("挑战开放状态不可验证，匹配保持禁用"));
    }

    #[test]
    fn challenge_metadata_becomes_visible_game_rules_and_stays_escaped() {
        assert_eq!(
            paper_phase_label("submission_ready"),
            "Author Raid complete / 作者远征完成"
        );
        assert_eq!(
            paper_phase_label("untrusted"),
            "Waiting for authority / 等待权威状态"
        );
        let legacy_challenge = serde_json::json!({
            "description":"template=replication; template_version=v1; difficulty=advanced; duration_preset=120m; objective=reproduce the frozen baseline; victory=all hard gates pass; risk=environment drift; modifiers=frozen-environment,predeclared-tolerance; reward=coins",
        });
        let summary = challenge_gameplay_summary(&legacy_challenge);
        assert!(summary.contains("Template / 模式"));
        assert!(summary.contains("data-gameplay-source=\"legacy_description\""));
        assert!(summary.contains("Template version / 模式版本</dt><dd>v1"));
        assert!(summary.contains("Difficulty / 难度</dt><dd>advanced"));
        assert!(summary.contains("Duration / 时长</dt><dd>120m"));
        assert!(summary.contains("Objective / 目标"));
        assert!(summary.contains("Victory / 胜利条件"));
        assert!(summary.contains("Risk / 主要风险</dt><dd>environment drift"));
        assert!(summary
            .contains("Modifiers / 规则修饰</dt><dd>frozen-environment,predeclared-tolerance"));
        assert!(summary.contains("Non-economic only"));
        assert!(!summary.contains("coins"));

        let escaped = challenge_gameplay_summary(&serde_json::json!({
            "description":"template=custom; objective=<script>alert(1)</script>; victory=do no harm",
        }));
        assert!(!escaped.contains("<script>"));
        assert!(escaped.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn authoritative_gameplay_overrides_description_and_fails_closed_when_malformed() {
        let challenge = serde_json::json!({
            "description":"objective=untrusted lobby copy; victory=award coins; difficulty=easy",
            "ruleset_version":"paper-raid-evidence-audit-v2",
            "ruleset":{
                "template":"evidence-audit",
                "duration_seconds":2700,
                "gameplay":{
                    "difficulty":"introductory",
                    "objective":"Audit the frozen evidence graph <carefully>.",
                    "risk":"A plausible citation can still fail provenance checks.",
                    "modifiers":["citation-blind","frozen-corpus"],
                    "victory_summary":"Resolve every seeded provenance defect."
                }
            }
        });
        let summary = challenge_gameplay_summary(&challenge);
        assert!(summary.contains("data-gameplay-source=\"authoritative_typed\""));
        assert!(summary.contains("paper-raid-evidence-audit-v2"));
        assert!(summary.contains("45m"));
        assert!(summary.contains("Audit the frozen evidence graph &lt;carefully&gt;."));
        assert!(summary.contains("citation-blind, frozen-corpus"));
        assert!(summary.contains("Non-economic only"));
        assert!(!summary.contains("untrusted lobby copy"));
        assert!(!summary.contains("award coins"));

        let malformed = serde_json::json!({
            "description":"objective=fallback must not render; victory=award coins",
            "ruleset":{
                "gameplay":{
                    "difficulty":"easy",
                    "objective":"typed objective",
                    "risk":"typed risk",
                    "modifiers":[],
                    "victory_summary":"typed victory"
                }
            }
        });
        let summary = challenge_gameplay_summary(&malformed);
        assert!(summary.contains("invalid_authoritative"));
        assert!(summary.contains("description fallback is disabled"));
        assert!(summary.contains("Non-economic only"));
        assert!(!summary.contains("fallback must not render"));
        assert!(!summary.contains("award coins"));
    }

    #[test]
    fn section_materialization_is_visible_and_release_root_mismatch_is_fail_closed() {
        let root = format!("sha256:{}", "a".repeat(64));
        let parent_root = format!("sha256:{}", "b".repeat(64));
        let patch_hash = format!("sha256:{}", "c".repeat(64));
        let revision = serde_json::json!({
            "revision_id":"00000000-0000-4000-8000-000000000101",
            "section_materialization_root":root,
            "section_materialization":{
                "schema":"hepta.paper_raid.section_materialization.v1",
                "paper_project_id":"00000000-0000-4000-8000-000000000100",
                "revision_id":"00000000-0000-4000-8000-000000000101",
                "parent_revision_id":"00000000-0000-4000-8000-000000000099",
                "parent_materialization_root":parent_root,
                "sections":[{
                    "section_key":"methods<script>",
                    "base_paper_revision_id":"00000000-0000-4000-8000-000000000099",
                    "head_section_revision_id":"00000000-0000-4000-8000-000000000102",
                    "merge_id":"00000000-0000-4000-8000-000000000103",
                    "patch_manifest_id":"00000000-0000-4000-8000-000000000104",
                    "patch_hash":patch_hash
                }]
            },
            "release_candidate":{"section_materialization_root":root}
        });
        let summary = revision_materialization_summary(Some(&revision));
        assert!(summary.contains("RELEASE-BOUND MATERIALIZATION"));
        assert!(summary.contains("Section lineage frozen / 章节谱系已锁定"));
        assert!(summary.contains("1 section(s) / 章节"));
        assert!(summary.contains("methods&lt;script&gt;"));
        assert!(!summary.contains("methods<script>"));
        assert!(summary.contains(&patch_hash));
        assert!(summary.contains(&parent_root));

        let mut mismatched = revision;
        mismatched["release_candidate"]["section_materialization_root"] =
            serde_json::json!(format!("sha256:{}", "d".repeat(64)));
        let summary = revision_materialization_summary(Some(&mismatched));
        assert!(summary.contains("MATERIALIZATION MISMATCH"));
        assert!(summary.contains("finalization must remain locked"));
    }

    #[tokio::test]
    async fn paper_room_defaults_to_one_basic_action_and_keeps_authority_advanced() {
        let paper_id = Uuid::new_v4();
        let paper_id_text = paper_id.to_string();
        let identity = AlphaIdentity::test_identity("evidence", Uuid::new_v4(), Uuid::new_v4());
        let room = serde_json::json!({
            "author_raid_progress":{
                "schema":"hepta.paper_raid.author_progress.v1",
                "phase":"researching",
                "next_phase":"experimenting",
                "objective":"bind_claims_to_evidence",
                "blockers":["evidence_card_required","artifact_manifest_required"],
                "next_actions":["transition_paper_project"],
                "transition_ready":false,
                "actor_role":"evidence",
                "personal_objective":"protect_claim_and_source_quality",
                "primary_actions":["create_evidence_card"]
            },
            "paper":{
                "paper_project_id":paper_id,
                "title":"Progressive Paper Room",
                "phase":"researching",
                "version":3
            },
            "team":{"version":2,"members":[{
                "player_id":identity.player_id,
                "role":"evidence"
            }]},
            "work_items":[],"paper_revisions":[],"member_research_sessions":[],
            "artifact_manifests":[],"authorship_consents":[],"section_revisions":[],
            "evidence_cards":[],"claims":[],"citations":[],"runs":[],"figures":[],
            "section_reviews":[]
        });
        let review = serde_json::json!({
            "finality":{
                "status":"pending_finality",
                "ranking_eligible":false,
                "reward_eligible":false,
                "score_eligible":false,
                "economic_eligible":false
            }
        });
        let response = paper_room(
            &identity,
            &paper_id_text,
            ReadState::Available(&room),
            ReadState::Unavailable,
            ReadState::Available(&review),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect progressively disclosed Paper Room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 Paper Room");
        let basic_start = body
            .find("<section class=\"panel paper-room-basic\"")
            .expect("Basic Paper Room");
        let advanced_start = body
            .find("<details class=\"panel paper-room-advanced\" id=\"paper-room-advanced\">")
            .expect("closed Advanced Paper Room");
        assert!(basic_start < advanced_start);
        let basic = &body[basic_start..advanced_start];
        assert!(basic.contains("id=\"paper-room-current-objective\""));
        assert!(basic
            .contains("Bind claims to evidence / <span lang=\"zh-Hans\">建立论断证据链</span>"));
        assert!(basic.contains("Evidence: protect claims and source quality"));
        assert!(basic.contains(
            "Register a verified evidence card / <span lang=\"zh-Hans\">登记已验证证据卡</span>"
        ));
        assert!(basic
            .contains("Register verified evidence / <span lang=\"zh-Hans\">登记已验证证据</span>"));
        assert!(basic.contains("1 more blocking reason(s) are listed in Advanced"));
        assert_eq!(basic.matches("<button").count(), 1);
        assert!(basic.contains("type=\"button\" data-paper-room-reveal"));
        assert!(basic.contains("data-paper-room-primary-target=\".create-evidence-card-form\""));
        assert!(basic.contains("aria-controls=\"paper-room-advanced\""));
        assert!(basic.contains("aria-expanded=\"false\""));
        for technical in [
            paper_id_text.as_str(),
            "sha256:",
            "data-paper-id=",
            "Hepta source / Hepta 来源",
            "CURRENT OBJECTIVE / 当前目标",
        ] {
            assert!(!basic.contains(technical), "Basic exposed {technical}");
        }

        let advanced = &body[advanced_start..];
        assert!(advanced.contains("Advanced / <span lang=\"zh-Hans\">高级详情</span>"));
        assert!(advanced.contains("AUTHORITY DETAILS / <span lang=\"zh-Hans\">权威详情</span>"));
        assert!(advanced.contains(&format!("<code>{paper_id}</code>")));
        assert!(advanced.contains("Hepta source / <span lang=\"zh-Hans\">Hepta 来源</span>"));
        assert!(advanced.contains("pending_finality"));
        assert!(advanced.contains("Register the canonical ArtifactManifest"));
        assert!(advanced.contains("CURRENT OBJECTIVE / 当前目标"));
        assert!(advanced.contains("class=\"create-evidence-card-form\""));
        assert!(advanced.contains("data-paper-id="));
        assert!(!body.contains(
            "<details class=\"panel paper-room-advanced\" id=\"paper-room-advanced\" open"
        ));
    }

    #[test]
    fn paper_room_disclosure_assets_are_keyboard_focus_and_small_screen_ready() {
        let script = include_str!("browser.js");
        assert!(script.contains("const PLAYER_ERROR_MESSAGES = Object.freeze({"));
        assert!(script.contains("Advanced error details / 高级错误详情"));
        assert!(script.contains("function restorePlayerFocusContext("));
        assert!(script.contains("navigationType !== \"reload\""));
        assert!(script.contains("target.focus({ preventScroll: true });"));
        assert!(script.contains("bindPlayerFocusContext();"));
        assert!(script.contains("restorePlayerFocusContext();"));
        assert!(script.contains("function bindPaperRoomProgressiveDisclosure()"));
        assert!(script.contains("advanced.open = true;"));
        assert!(script.contains("advanced.querySelector(targetSelector)"));
        assert!(script.contains("paperRoomFocusTarget(matchingAction)"));
        assert!(script.contains("input:not([type=hidden]):not([disabled])"));
        assert!(script.contains("target.focus({ preventScroll: true });"));
        assert!(script.contains("prefers-reduced-motion: reduce"));
        assert!(script.contains("bindPaperRoomProgressiveDisclosure();"));

        assert!(
            CSS.contains(":where(a,button,input,textarea,select,summary,[tabindex]):focus-visible")
        );
        assert!(CSS.contains(".player-error-details>summary"));
        assert!(CSS.contains("output[data-player-message=friendly]"));
        assert!(CSS.contains(".paper-room-primary-button{justify-self:start;min-height:44px"));
        assert!(CSS.contains("@media(max-width:430px)"));
        assert!(CSS.contains("@media(max-width:390px)"));
        assert!(CSS.contains(".paper-room-primary-button{justify-self:stretch;width:100%}"));
    }

    #[tokio::test]
    async fn paper_room_uses_authoritative_finality_and_keeps_developer_json_collapsed() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let upstream_claim = format!("{}{}", "final", "ized");
        let room = serde_json::json!({
            "author_raid_progress":{
                "schema":"hepta.paper_raid.author_progress.v1",
                "phase":"forming",
                "next_phase":"preregistering",
                "objective":"Hepta projected objective",
                "blockers":[],
                "next_actions":["transition_paper_project"],
                "transition_ready":true,
                "actor_role":"captain",
                "personal_objective":"Coordinate the preregistration checkpoint.",
                "primary_actions":["transition_paper_project"]
            },
            "paper": {
                "title":"Alpha Paper",
                "phase":"submission_ready",
                "version":9,
                "settlement": upstream_claim
            },
            "team":{"version":2,"members":[]},
            "work_items":[],
            "paper_revisions":[],
            "member_research_sessions":[],
            "artifact_manifests":[],
            "authorship_consents": [],
            "section_revisions": [],
            "evidence_cards": [],
            "claims": [],
            "citations": [],
            "runs": [],
            "figures": [],
            "section_reviews": []
        });
        let review = serde_json::json!({
            "finality": {
                "schema":"hepta.paper_raid.consumer_finality.v2",
                "status":"verified_finality",
                "effective_evaluation_id":Uuid::new_v4(),
                "effective_reproduction_id":Uuid::new_v4(),
                "effective_appeal_resolution_id":null,
                "ranking_eligible":false,
                "reward_eligible":false,
                "score_eligible":false,
                "economic_eligible":false,
                "verified_at":"2026-08-10T00:00:00Z"
            },
            "contribution_ledgers":[],
            "evaluations":[],
            "reproductions":[],
            "appeals":[],
            "resolutions":[],
            "raid_scores":[]
        });
        let response = paper_room(
            &identity,
            "paper-a",
            ReadState::Available(&room),
            ReadState::Unavailable,
            ReadState::Available(&review),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 room");
        assert!(body.contains("verified_finality"));
        assert!(body.contains("Verified scientific finality / 科学终局已验证"));
        assert!(body.contains("data-eligible=\"false\""));
        assert!(body.contains("Economic / 经济</span><strong>locked"));
        assert!(!body.contains(&format!("{}{}", "final", "ized")));
        assert!(body.contains("CURRENT OBJECTIVE / 当前目标"));
        assert!(body.contains("Author Raid complete"));
        assert!(body.contains("<details class=\"panel developer-tools\">"));
        assert!(body.contains("class=\"card live-raid\" data-paper-id=\"paper-a\""));
        assert!(body.contains("Loading teammate presence"));
        assert!(body.contains("Sync now / 立即同步"));
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
        assert!(!missing.contains("verified_finality"));
    }

    #[test]
    fn finality_availability_is_explicit_and_never_presented_as_pending() {
        for (status, display) in [
            ("unknown_finality", "Finality unknown / 终局状态未知"),
            (
                "unavailable_finality",
                "Finality temporarily unavailable / 终局暂不可用",
            ),
            (
                "error_finality",
                "Finality verification error / 终局验证错误",
            ),
        ] {
            let value = serde_json::json!({
                "schema": "hepta.paper_raid.bff_finality_availability.v1",
                "status": status,
                "authoritative": false,
                "reason_code": "upstream_<unsafe>",
                "ranking_eligible": true,
                "reward_eligible": true,
                "score_eligible": true,
                "economic_eligible": true,
                "verified_at": null
            });
            let (html, fact) = finality_projection(&value);
            assert!(html.contains(display));
            assert!(html.contains(status));
            assert!(html.contains("upstream_&lt;unsafe&gt;"));
            assert_eq!(html.matches("data-eligible=\"false\"").count(), 4);
            assert!(!html.contains("data-eligible=\"true\""));
            assert!(!html.contains("Verification pending / 等待终局验证"));
            assert!(fact.contains(status));
        }
    }

    #[tokio::test]
    async fn paper_room_appeal_form_uses_plain_language_and_manifest_selector_only() {
        let paper_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let manifest_id = Uuid::new_v4();
        let identity = AlphaIdentity::test_identity("author", Uuid::new_v4(), Uuid::new_v4());
        let release_hash = format!("sha256:{}", "a".repeat(64));
        let manifest_hash = format!("sha256:{}", "b".repeat(64));
        let room = serde_json::json!({
            "author_raid_progress":{
                "phase":"submission_ready",
                "actor_role":"captain",
                "blockers":[],
                "next_actions":[]
            },
            "paper":{
                "paper_project_id":paper_id,
                "title":"Appealable Paper",
                "phase":"submission_ready",
                "version":9
            },
            "team":{"version":3,"members":[]},
            "work_items":[],"paper_revisions":[],"member_research_sessions":[],
            "artifact_manifests":[{
                "manifest_id":manifest_id,
                "paper_project_id":paper_id,
                "manifest_hash":manifest_hash,
                "source_bundle_id":"retained-failed-runs",
                "object_count":4,
                "objects":[]
            }],
            "authorship_consents":[],"section_revisions":[],"evidence_cards":[],
            "claims":[],"citations":[],"runs":[],"figures":[],"section_reviews":[]
        });
        let mut review = serde_json::json!({
            "finality":{"status":"pending_finality","ranking_eligible":false,"reward_eligible":false,"score_eligible":false,"economic_eligible":false},
            "evaluations":[{
                "evaluation_id":evaluation_id,
                "paper_project_id":paper_id,
                "release_candidate_hash":release_hash,
                "version":1
            }],
            "appeals":[],"resolutions":[],"reproductions":[],"raid_scores":[]
        });
        let render = |review: &Value| {
            paper_room(
                &identity,
                &paper_id.to_string(),
                ReadState::Available(&room),
                ReadState::Unavailable,
                ReadState::Available(review),
            )
        };
        let body = render(&review)
            .into_body()
            .collect()
            .await
            .expect("collect Appeal form")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 Appeal form");
        assert!(body.contains("class=\"author-appeal-form\""));
        assert!(body.contains("name=\"grounds\""));
        assert!(body.contains("name=\"evidence_manifest_id\""));
        assert!(body.contains("retained-failed-runs · 4 object(s)"));
        let form_start = body.find("class=\"author-appeal-form\"").unwrap();
        let form_end = body[form_start..].find("</form>").unwrap() + form_start;
        let form = &body[form_start..form_end];
        assert!(!form.contains("name=\"evaluation_id\""));
        assert!(!form.contains("name=\"release_candidate_hash\""));
        assert!(!form.contains("name=\"manifest_hash\""));
        assert!(!form.contains("name=\"payload\""));

        let appeal_id = Uuid::new_v4();
        review["appeals"] = serde_json::json!([{
            "appeal_id":appeal_id,
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id
        }]);
        let body = render(&review)
            .into_body()
            .collect()
            .await
            .expect("collect open Appeal")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 open Appeal");
        assert!(body.contains("APPEAL OPEN"));
        assert!(!body.contains("class=\"author-appeal-form\""));

        review["resolutions"] = serde_json::json!([{"appeal_id":appeal_id}]);
        let body = render(&review)
            .into_body()
            .collect()
            .await
            .expect("collect resolved Appeal")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 resolved Appeal");
        assert!(body.contains("APPEAL RESOLVED"));
        assert!(!body.contains("class=\"author-appeal-form\""));
    }

    #[test]
    fn rejected_author_room_exposes_only_plain_language_rework_and_server_lease_state() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let paper_id_text = paper_id.to_string();
        let inactive_paper = serde_json::json!({
            "paper_project_id":paper_id,
            "phase":"submission_ready",
            "outcome":"submission_ready",
            "version":9,
            "active_rework_id":null,
            "active_rework_cycle":null,
            "rework_expires_at":null
        });
        let room = serde_json::json!({
            "joint_submission":{
                "submission_id":submission_id,
                "paper_project_id":paper_id,
                "status":"submission_ready"
            }
        });
        let mut review = serde_json::json!({
            "evaluations":[{
                "evaluation_id":evaluation_id,
                "paper_project_id":paper_id,
                "submission_id":submission_id,
                "version":1,
                "status":"rejected",
                "supersedes_evaluation_id":null,
                "paper_score":{
                    "hard_gates":{
                        "citations_and_data_authentic":false,
                        "failed_runs_disclosed":true,
                        "all_authors_consented":true,
                        "core_claims_have_evidence":false,
                        "artifact_lineage_complete":true,
                        "license_ethics_coi_complete":true
                    }
                }
            }],
            "appeals":[],
            "resolutions":[]
        });
        let rendered = author_rework_panel(
            &paper_id_text,
            "submission_ready",
            &inactive_paper,
            &room,
            Some(&review),
        );
        assert!(rendered.contains("class=\"author-rework-start-form\""));
        assert!(rendered.contains("Citations and data authenticity"));
        assert!(rendered.contains("Evidence for core claims"));
        let form_start = rendered
            .find("class=\"author-rework-start-form\"")
            .expect("ordinary rework form");
        let form_end = rendered[form_start..].find("</form>").expect("form closes") + form_start;
        let form = &rendered[form_start..form_end];
        assert!(form.contains("name=\"reason\""));
        for forbidden in [
            "name=\"rework_id\"",
            "name=\"submission_id\"",
            "name=\"evaluation_id\"",
            "name=\"reason_hash\"",
            "name=\"signature\"",
            "name=\"payload\"",
            "sha256:",
            "JSON",
        ] {
            assert!(
                !form.contains(forbidden),
                "ordinary form exposed {forbidden}"
            );
        }

        let appeal_id = Uuid::new_v4();
        review["appeals"] = serde_json::json!([{
            "appeal_id":appeal_id,
            "evaluation_id":evaluation_id
        }]);
        let held = author_rework_panel(
            &paper_id_text,
            "submission_ready",
            &inactive_paper,
            &room,
            Some(&review),
        );
        assert!(held.contains("REWORK ON HOLD"));
        assert!(!held.contains("author-rework-start-form"));

        let active_paper = serde_json::json!({
            "active_rework_id":Uuid::new_v4(),
            "active_rework_cycle":2,
            "rework_expires_at":"2099-08-15T00:00:00Z"
        });
        let active = author_rework_panel(&paper_id_text, "drafting", &active_paper, &room, None);
        assert!(active.contains("data-rework-state=\"active\""));
        assert!(active.contains("class=\"paper-rework-countdown\""));
        assert!(active.contains("class=\"button continue-raid-link\""));
        assert!(!active.contains("sha256:"));
        assert!(!active.contains("signature"));

        let expired_paper = serde_json::json!({
            "active_rework_id":Uuid::new_v4(),
            "active_rework_cycle":2,
            "rework_expires_at":"2000-01-01T00:00:00Z"
        });
        let expired = author_rework_panel(&paper_id_text, "drafting", &expired_paper, &room, None);
        assert!(expired.contains("data-rework-state=\"expired\""));
        assert!(expired.contains("Author mutations are disabled"));
        assert!(!expired.contains("author-rework-start-form"));
    }

    #[tokio::test]
    async fn authoritative_challenge_rules_and_typed_terminal_controls_are_player_visible() {
        let paper_id = Uuid::new_v4();
        let identity = AlphaIdentity::test_identity("captain", Uuid::new_v4(), Uuid::new_v4());
        let room = serde_json::json!({
            "author_raid_progress":{
                "phase":"researching",
                "actor_role":"captain",
                "blockers":[],
                "next_actions":[]
            },
            "paper":{
                "paper_project_id":paper_id,
                "title":"Ruleset-visible Paper",
                "phase":"researching",
                "version":4,
                "outcome":"in_progress",
                "outcome_reason":null,
                "terminal_at":null,
                "deadline_at":"2099-08-10T10:00:00Z",
                "grace_expires_at":"2099-08-10T10:15:00Z",
                "challenge_ruleset_snapshot":{
                    "schema":"hepta.paper_raid.challenge_ruleset_snapshot.v1",
                    "ruleset_version":"paper-raid-replication-v1",
                    "enforcement":"authoritative_v1",
                    "ruleset":{
                        "schema":"hepta.challenge.ruleset.v1",
                        "template":"replication",
                        "duration_seconds":7200,
                        "grace_seconds":900,
                        "gameplay":{
                            "difficulty":"intermediate",
                            "objective":"Reproduce the frozen primary effect.",
                            "risk":"Environment drift can invalidate the comparison.",
                            "modifiers":["frozen-environment","predeclared-tolerance"],
                            "victory_summary":"Match the frozen result inside the declared tolerance."
                        },
                        "phase_gates":[{
                            "transition":"researching_to_experimenting",
                            "requirements":[
                                {"kind":"evidence_cards","minimum":1},
                                {"kind":"citations","minimum":1}
                            ]
                        }],
                        "victory_requirements":[
                            {"kind":"accepted_work_items","minimum":1},
                            {"kind":"release_candidate","minimum":1},
                            {"kind":"all_author_consents","minimum":1}
                        ]
                    }
                }
            },
            "team":{"version":2,"members":[{
                "player_id":identity.player_id,
                "role":"captain"
            }]},
            "work_items":[],"paper_revisions":[],"member_research_sessions":[],
            "artifact_manifests":[],"authorship_consents":[],"section_revisions":[],
            "evidence_cards":[],"claims":[],"citations":[],"runs":[],"figures":[],
            "section_reviews":[]
        });
        let response = paper_room(
            &identity,
            &paper_id.to_string(),
            ReadState::Available(&room),
            ReadState::Unavailable,
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect Challenge Ruleset panel")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 room");
        let start = body.find("challenge-ruleset-panel").unwrap();
        let end = body[start..].find("raid-command-center").unwrap() + start;
        let panel = &body[start..end];
        assert!(panel.contains("AUTHORITATIVE RULESET"));
        assert!(panel.contains("Independent Replication / 独立复现"));
        assert!(panel.contains("Intermediate / 中阶"));
        assert!(panel.contains("paper-raid-replication-v1"));
        assert!(panel.contains("data-gameplay-source=\"authoritative_typed\""));
        assert!(panel.contains("Reproduce the frozen primary effect."));
        assert!(panel.contains("Match the frozen result inside the declared tolerance."));
        assert!(panel.contains("frozen-environment, predeclared-tolerance"));
        assert!(panel.contains("Non-economic only"));
        assert!(panel.contains("Victory conditions / 胜利条件"));
        assert!(panel.contains("accepted work items / 已验收任务"));
        assert!(panel.contains("all author consents / 全部作者同意"));
        assert!(panel.contains("data-deadline-at=\"2099-08-10T10:00:00+00:00\""));
        assert!(panel.contains("data-grace-expires-at=\"2099-08-10T10:15:00+00:00\""));
        assert!(panel.contains("value=\"failed\""));
        assert!(panel.contains("value=\"abandoned\""));
        assert!(panel
            .contains("materializes automatically on the next authorized Room/state/event read"));
        assert!(!panel.contains("expired-outcome-control"));
        assert!(!panel.contains("value=\"expired\""));
        assert!(!panel.contains("name=\"paper_id\""));
        assert!(!panel.contains("name=\"expected_version\""));
        assert!(!panel.contains("name=\"payload\""));
        assert!(!panel.contains("name=\"hash\""));

        let mut terminal = room.clone();
        terminal["paper"]["outcome"] = Value::String("failed".into());
        terminal["paper"]["outcome_reason"] = Value::String("integrity_failure".into());
        terminal["paper"]["terminal_at"] = Value::String("2026-08-10T10:05:00Z".into());
        let response = paper_room(
            &identity,
            &paper_id.to_string(),
            ReadState::Available(&terminal),
            ReadState::Unavailable,
            ReadState::Unavailable,
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect terminal Challenge panel")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 terminal room");
        let start = body.find("challenge-ruleset-panel").unwrap();
        let end = body[start..].find("raid-command-center").unwrap() + start;
        let panel = &body[start..end];
        assert!(panel.contains("Integrity failure / 完整性失败"));
        assert!(!panel.contains("class=\"challenge-outcome-form\""));
    }

    #[tokio::test]
    async fn forming_paper_room_has_zero_json_primary_path() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let binding_id = Uuid::new_v4();
        let room = serde_json::json!({
            "paper": {
                "paper_project_id":Uuid::new_v4(),
                "title":"True First Playable",
                "phase":"forming",
                "version":1,
                "current_revision_id":null
            },
            "team":{
                "version":2,
                "collaboration_compact_hash":format!("sha256:{}", "a".repeat(64)),
                "members":[{
                "participant_slot":1,
                "player_id":identity.player_id,
                "binding_id":binding_id,
                "agent_id":"did:trnm:agent:first-playable",
                "role":"captain"
                }]
            },
            "work_items":[],
            "paper_revisions":[],
            "member_research_sessions":[],
            "artifact_manifests":[],
            "authorship_consents":[],
            "section_revisions":[],
            "evidence_cards":[],
            "claims":[],
            "citations":[],
            "runs":[],
            "figures":[],
            "section_reviews":[]
        });
        let review = serde_json::json!({
            "finality":{
                "status":"pending_finality",
                "ranking_eligible":false,
                "reward_eligible":false,
                "score_eligible":false,
                "economic_eligible":false
            }
        });
        let response = paper_room(
            &identity,
            "paper-first",
            ReadState::Available(&room),
            ReadState::Unavailable,
            ReadState::Available(&review),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 room");
        assert!(body.contains("Open preregistration"));
        assert!(body.contains(
            "Freeze the question and protocol before collecting evidence or running experiments."
        ));
        assert!(body.contains(
            "Hepta's Author Raid projection is authoritative when present; local derivation is only a compatibility fallback."
        ));
        let developer = body
            .find("Developer Tools / <span lang=\"zh-Hans\">开发者工具</span>")
            .unwrap();
        let primary_path = &body[..developer];
        assert!(
            primary_path.contains("class=\"guided-action blocked author-progress-unavailable\"")
        );
        assert!(primary_path.contains("The compatibility objective remains read-only"));
        assert!(!primary_path.contains("class=\"paper-phase-form"));
        assert!(!primary_path.contains("class=\"create-work-item-form"));
        assert!(!primary_path.contains("class=\"issue-authorization-form"));
        assert!(!primary_path.contains("transition_paper_project"));
        assert!(!primary_path.contains("data-command="));
        assert!(!primary_path.contains("name=\"payload\""));
        assert!(!primary_path.contains("Exact typed JSON / 精确类型 JSON"));
        assert!(!primary_path.contains("Hepta projected objective"));
        assert!(body.contains("<details class=\"panel developer-tools\">"));
        assert!(!body.contains("<details class=\"panel developer-tools\" open>"));
        let first_json_editor = body.find("Exact typed JSON / 精确类型 JSON").unwrap();
        assert!(developer < first_json_editor);

        let mut drafting = room.clone();
        drafting["paper"]["phase"] = Value::String("drafting".into());
        drafting["paper"]["version"] = Value::from(4);
        drafting["author_raid_progress"] = serde_json::json!({
            "schema":"hepta.paper_raid.author_progress.v1",
            "phase":"drafting",
            "next_phase":"integrity_review",
            "objective":"assemble_draft",
            "blockers":["paper_revision_required"],
            "next_actions":["create_paper_revision"],
            "transition_ready":false,
            "actor_role":"captain",
            "personal_objective":"coordinate_team_and_phase_gate",
            "primary_actions":["create_paper_work_item","transition_paper_project"]
        });
        let response = paper_room(
            &identity,
            "paper-first",
            ReadState::Available(&drafting),
            ReadState::Unavailable,
            ReadState::Available(&review),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect drafting room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 drafting room");
        assert!(body.contains("class=\"create-paper-revision-form primary-action\""));
        assert!(body.contains("name=\"source_manifest_hash\" type=\"hidden\""));
        assert!(body.contains("name=\"claim_evidence_graph_hash\" type=\"hidden\""));
        assert!(!body.contains("Paper source digest<input"));
        assert!(!body.contains("Claim–evidence graph digest<input"));
        assert!(body.contains("Resolve the projected blockers before advancing"));

        let baseline_revision_id = Uuid::new_v4();
        drafting["paper"]["current_revision_id"] = Value::String(baseline_revision_id.to_string());
        drafting["paper_revisions"] = serde_json::json!([{
            "revision_id":baseline_revision_id,
            "version":1
        }]);
        drafting["section_heads"] = serde_json::json!([{
            "section_key":"methods",
            "current_head_revision_id":baseline_revision_id,
            "fencing_token":7
        }]);
        drafting["leases"] = serde_json::json!([{
            "lease_id":Uuid::new_v4(),
            "section_key":"methods",
            "holder_player_id":identity.player_id,
            "holder_binding_id":binding_id,
            "fencing_token":7,
            "status":"active",
            "expires_at":"2000-01-01T00:00:00Z"
        }]);
        let response = paper_room(
            &identity,
            "paper-first",
            ReadState::Available(&drafting),
            ReadState::Unavailable,
            ReadState::Available(&review),
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect expired lease room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 expired lease room");
        assert!(body.contains("class=\"acquire-section-lease-form primary-action\""));
        assert!(body.contains("value=\"methods\" data-fencing-token=\"7\" selected"));
        assert!(!body.contains("class=\"bridge-proposal-task-form\""));
    }

    #[test]
    fn canonical_roles_only_render_their_authorized_player_actions() {
        let captain_id = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let experiment_id = Uuid::new_v4();
        let members = serde_json::json!([
            {"participant_slot":1,"player_id":captain_id,"binding_id":Uuid::new_v4(),"role":"captain"},
            {"participant_slot":2,"player_id":evidence_id,"binding_id":Uuid::new_v4(),"role":"evidence"},
            {"participant_slot":3,"player_id":experiment_id,"binding_id":Uuid::new_v4(),"role":"experiment"}
        ]);
        let members = members.as_array().expect("members");
        assert!(canonical_author_roles_enforced(members));

        let progress = serde_json::json!({
            "next_phase":"experimenting",
            "transition_ready":true,
            "actor_can_transition":false
        });
        let phase = phase_transition_form("paper-role", 3, "researching", Some(&progress));
        assert!(phase.contains("Captain checkpoint"));
        assert!(!phase.contains("class=\"paper-phase-form"));

        let manifests = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let room = serde_json::json!({
            "paper":{"version":3,"challenge_id":Uuid::new_v4()},
            "artifact_manifests":[
                {"manifest_id":manifests[0],"source_bundle_id":"code"},
                {"manifest_id":manifests[1],"source_bundle_id":"data"},
                {"manifest_id":manifests[2],"source_bundle_id":"environment"}
            ],
            "evidence_cards":[{"evidence_card_id":Uuid::new_v4(),"source_uri":"https://example.org/source"}],
            "experiment_plans":[{"experiment_plan_id":Uuid::new_v4(),"protocol_snapshot_hash":format!("sha256:{}", "a".repeat(64))}],
            "runs":[],"figures":[]
        });
        let evidence =
            science_action_panel_for_role("paper-role", "researching", &room, "evidence", true);
        assert!(evidence.contains("class=\"create-evidence-card-form\""));
        assert!(evidence.contains("class=\"create-citation-record-form\""));
        assert!(!evidence.contains("class=\"create-experiment-plan-form\""));

        let experiment =
            science_action_panel_for_role("paper-role", "researching", &room, "experiment", true);
        assert!(experiment.contains("class=\"create-experiment-plan-form\""));
        assert!(!experiment.contains("class=\"create-evidence-card-form\""));

        let work_items = serde_json::json!([{
            "work_item_id":Uuid::new_v4(),
            "title":"Captain-owned task",
            "status":"planned",
            "version":1,
            "assigned_player_id":captain_id
        }]);
        let current_evidence_id = evidence_id.to_string();
        let evidence_work = work_item_panel(WorkItemPanelInput {
            paper_id: "paper-role",
            paper_version: 3,
            members,
            work_items: work_items.as_array().expect("work items"),
            manifests: &[],
            primary: true,
            editable: true,
            current_player_id: &current_evidence_id,
            current_role: "evidence",
            canonical_roles_enforced: true,
        });
        assert!(!evidence_work.contains("Unassigned team task"));
        assert!(evidence_work.contains("Only the assigned player or Captain"));
        assert!(!evidence_work.contains("class=\"work-item-transition-form\""));
    }

    #[tokio::test]
    async fn author_consent_is_locked_until_exact_ledger_or_repair() {
        let identity = AlphaIdentity::test_identity(
            "subject-ledger",
            Uuid::parse_str("00000000-0000-4000-8000-000000000103").expect("player"),
            Uuid::new_v4(),
        );
        let paper_id = "00000000-0000-4000-8000-000000000101";
        let revision_id = "00000000-0000-4000-8000-000000000102";
        let release_hash = format!("sha256:{}", "b".repeat(64));
        let ledger_hash = "sha256:92e282d43c2a93c89ad3e48f69177a97d3817171f4786903a4e9dcd1759f55c7";
        let authors = serde_json::json!([
            {"author_order":1,"participant_slot":1,"player_id":"00000000-0000-4000-8000-000000000103","display_name":"Captain","credit_roles":["conceptualization","writing_review_editing"]},
            {"author_order":2,"participant_slot":2,"player_id":"00000000-0000-4000-8000-000000000104","display_name":"Evidence","credit_roles":["data_curation"]},
            {"author_order":3,"participant_slot":3,"player_id":"00000000-0000-4000-8000-000000000105","display_name":"Experiment","credit_roles":["methodology","validation"]}
        ]);
        let room = serde_json::json!({
            "paper":{
                "paper_project_id":paper_id,
                "title":"Ledger-gated release",
                "phase":"author_approval",
                "version":9,
                "current_revision_id":revision_id
            },
            "team":{"version":2,"members":[
                {"participant_slot":1,"player_id":"00000000-0000-4000-8000-000000000103","binding_id":Uuid::new_v4(),"agent_id":"did:trnm:agent:one","role":"captain"},
                {"participant_slot":2,"player_id":"00000000-0000-4000-8000-000000000104","binding_id":Uuid::new_v4(),"agent_id":"did:trnm:agent:two","role":"evidence"},
                {"participant_slot":3,"player_id":"00000000-0000-4000-8000-000000000105","binding_id":Uuid::new_v4(),"agent_id":"did:trnm:agent:three","role":"experiment"}
            ]},
            "paper_revisions":[{
                "revision_id":revision_id,
                "version":2,
                "release_candidate_hash":release_hash,
                "release_candidate":{"contribution_ledger_hash":ledger_hash,"authors":authors}
            }],
            "authorship_consents":[],
            "work_items":[],"member_research_sessions":[],"artifact_manifests":[],
            "proposals":[],"decisions":[],
            "section_revisions":[],"evidence_cards":[],"claims":[],"citations":[],
            "runs":[],"figures":[],"section_reviews":[]
        });
        let mut review = serde_json::json!({
            "finality":{"status":"pending_finality","ranking_eligible":false,"reward_eligible":false,"score_eligible":false,"economic_eligible":false},
            "contribution_ledgers":[]
        });
        let render = |review: &Value| {
            paper_room(
                &identity,
                paper_id,
                ReadState::Available(&room),
                ReadState::Unavailable,
                ReadState::Available(review),
            )
        };
        let body = render(&review)
            .into_body()
            .collect()
            .await
            .expect("collect repair-gated room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 repair room");
        assert!(body.contains("class=\"freeze-contribution-ledger-form primary-action\""));
        assert!(body.contains("data-expected-ledger-hash=\"sha256:92e282"));
        assert!(!body.contains("class=\"author-consent-form primary-action\""));
        assert!(!body.contains("class=\"finalize-paper-form primary-action\""));
        assert!(!body.contains("name=\"contribution_ledger_hash\""));

        review["contribution_ledgers"] = serde_json::json!([{
            "contribution_ledger_id":"52411704-c8f0-53f3-b981-2c859f99cdcc",
            "paper_project_id":paper_id,
            "release_candidate_hash":release_hash,
            "ledger_hash":ledger_hash,
            "contribution_ledger_hash":format!("sha256:{}", "0".repeat(64))
        }]);
        let body = render(&review)
            .into_body()
            .collect()
            .await
            .expect("collect ledger-ready room")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 ledger-ready room");
        assert!(!body.contains("class=\"freeze-contribution-ledger-form primary-action\""));
        assert!(body.contains("class=\"author-consent-form primary-action\""));
        assert!(!body.contains("class=\"finalize-paper-form primary-action\""));
    }

    #[test]
    fn lobby_role_preferences_are_limited_to_identity_authorizations() {
        let mut identity =
            AlphaIdentity::test_identity("subject-role", Uuid::new_v4(), Uuid::new_v4());
        identity.author_roles = vec![AlphaAuthorRole::Evidence, AlphaAuthorRole::Experiment].into();
        let (controls, authorized) = author_role_choices(&identity);
        assert_eq!(authorized, "evidence,experiment");
        assert!(controls.contains("value=\"evidence\" checked"));
        assert!(controls.contains("value=\"experiment\" checked"));
        assert!(!controls.contains("value=\"captain\""));
        assert!(controls.contains("Preferred · 首选"));
    }

    #[test]
    fn scientific_actions_use_typed_forms_and_authoritative_selectors() {
        let manifest_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let evidence_id = Uuid::new_v4();
        let plan_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let room = serde_json::json!({
            "paper":{
                "version":3,
                "challenge_id":Uuid::new_v4()
            },
            "artifact_manifests":[
                {"manifest_id":manifest_ids[0],"source_bundle_id":"code-bundle"},
                {"manifest_id":manifest_ids[1],"source_bundle_id":"dataset-bundle"},
                {"manifest_id":manifest_ids[2],"source_bundle_id":"environment-bundle"}
            ],
            "evidence_cards":[{
                "evidence_card_id":evidence_id,
                "source_uri":"https://example.org/source",
                "source_hash":format!("sha256:{}", "a".repeat(64)),
                "locator":"p. 3",
                "license":"CC-BY-4.0"
            }],
            "experiment_plans":[{
                "experiment_plan_id":plan_id,
                "protocol_snapshot_hash":format!("sha256:{}", "b".repeat(64))
            }],
            "runs":[{"run_record_id":run_id,"status":"failed"}],
            "figures":[]
        });
        let preregistration = science_action_panel("paper-a", "preregistering", &room);
        assert!(preregistration.contains("class=\"create-experiment-plan-form\""));
        assert!(!preregistration.contains("name=\"payload\""));
        assert!(!preregistration.contains("name=\"experiment_plan_id\""));

        let research = science_action_panel("paper-a", "researching", &room);
        for form in [
            "create-experiment-plan-form",
            "create-evidence-card-form",
            "create-citation-record-form",
            "create-claim-record-form",
        ] {
            assert!(research.contains(&format!("class=\"{form}\"")));
        }
        assert!(research.contains("https://example.org/source"));
        assert!(research.contains("name=\"source_file\" type=\"file\" required"));
        assert!(research.contains("name=\"source_media_type\""));
        assert!(!research.contains("name=\"source_hash\""));
        assert!(!research.contains("Exact typed JSON"));

        let experimenting = science_action_panel("paper-a", "experimenting", &room);
        assert!(experimenting.contains("class=\"run-artifact-wizard-form primary-action\""));
        assert!(experimenting.contains("name=\"stdout_file\" type=\"file\" required"));
        assert!(experimenting.contains("name=\"stderr_file\" type=\"file\" required"));
        assert!(experimenting.contains("name=\"output_file\" type=\"file\" required"));
        assert!(experimenting.contains("name=\"metrics_file\" type=\"file\" accept="));
        assert!(experimenting.contains("class=\"figure-lineage-wizard-form\""));
        assert!(experimenting.contains("name=\"figure_file\" type=\"file\""));
        assert!(experimenting.contains("name=\"lineage_file\" type=\"file\" accept="));
        assert!(experimenting.contains("name=\"run_record_ids\" multiple"));
        assert!(experimenting.contains("class=\"create-run-record-form\""));
        assert!(experimenting.contains("name=\"experiment_plan_id\""));
        assert!(!experimenting.contains("name=\"run_record_id\""));
        assert!(!experimenting.contains("name=\"figure_manifest_id\""));
        assert!(!experimenting.contains("name=\"figure_lineage_id\""));
        assert!(!experimenting.contains("name=\"transform_hash\""));
        assert!(!experimenting.contains("sha256:…"));

        let empty_inputs = serde_json::json!({
            "paper":{"version":1,"challenge_id":Uuid::new_v4()},
            "artifact_manifests":[],
            "evidence_cards":[],
            "experiment_plans":[],
            "runs":[],
            "figures":[]
        });
        let preregistration = science_action_panel("paper-a", "preregistering", &empty_inputs);
        assert!(preregistration.contains("class=\"input-manifest-wizard-form primary-action\""));
        assert!(!preregistration.contains("name=\"manifest_id\""));

        let drafting = science_action_panel("paper-a", "drafting", &room);
        assert!(drafting.contains("class=\"draft-manifest-wizard-form primary-action\""));
        assert!(drafting.contains("class=\"review-ready-manifest-wizard-form primary-action\""));
        for field in ["evaluator_file", "review_dataset_file", "candidate_file"] {
            assert!(drafting.contains(&format!("name=\"{field}\"")));
        }
        assert!(drafting.contains("No UUID or digest is pasted"));
        assert!(drafting.contains(&run_id.to_string()));
    }

    #[test]
    fn normal_release_form_preserves_plain_language_disclosures_without_manual_digests() {
        let paper_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let members = serde_json::json!([
            {"participant_slot":1,"player_id":Uuid::new_v4(),"role":"captain"},
            {"participant_slot":2,"player_id":Uuid::new_v4(),"role":"evidence"},
            {"participant_slot":3,"player_id":Uuid::new_v4(),"role":"experiment"}
        ]);
        let paper = serde_json::json!({"title":"Plain-language release"});
        let room = serde_json::json!({
            "team":{"collaboration_compact_hash":format!("sha256:{}", "a".repeat(64)),"members":members},
            "experiment_plans":[{
                "protocol_snapshot_hash":format!("sha256:{}", "b".repeat(64))
            }],
            "artifact_manifests":[],
            "proposals":[],
            "decisions":[],
            "section_reviews":[]
        });
        let revision = serde_json::json!({"revision_id":revision_id,"version":2});
        let form = promote_release_form(
            &paper_id.to_string(),
            7,
            &paper,
            &room,
            members.as_array().expect("members"),
            &revision,
        );
        assert!(form.contains("name=\"research_protocol_snapshot_hash\" type=\"hidden\""));
        assert!(form.contains("name=\"ethics_disclosure_text\""));
        assert!(form.contains("name=\"coi_disclosure_text\""));
        assert!(form.contains("name=\"ai_disclosure_text\""));
        assert!(!form.contains("name=\"ethics_disclosure_hash\""));
        assert!(!form.contains("name=\"coi_disclosure_hash\""));
        assert!(!form.contains("name=\"ai_disclosure_hash\""));
        assert!(form.contains("No player enters a ledger UUID, reference, JSON, or digest"));
    }

    #[test]
    fn release_contribution_milestones_are_server_derived_and_fail_closed() {
        let paper_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let captain_id = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let experiment_id = Uuid::new_v4();
        let proposal_id = Uuid::new_v4();
        let decision_id = Uuid::new_v4();
        let manifest_id = Uuid::new_v4();
        let review_id = Uuid::new_v4();
        let members = serde_json::json!([
            {"participant_slot":1,"player_id":captain_id,"role":"captain"},
            {"participant_slot":2,"player_id":evidence_id,"role":"evidence"},
            {"participant_slot":3,"player_id":experiment_id,"role":"experiment"}
        ]);
        let paper = serde_json::json!({"title":"Milestone release"});
        let mut room = serde_json::json!({
            "team":{"collaboration_compact_hash":format!("sha256:{}", "a".repeat(64)),"members":members},
            "experiment_plans":[{
                "protocol_snapshot_hash":format!("sha256:{}", "b".repeat(64))
            }],
            "artifact_manifests":[{"manifest_id":manifest_id,"paper_project_id":paper_id}],
            "proposals":[{
                "proposal_id":proposal_id,
                "paper_project_id":paper_id,
                "artifact_manifest_id":manifest_id,
                "status":"accepted"
            }],
            "decisions":[{
                "decision_id":decision_id,
                "paper_project_id":paper_id,
                "proposal_id":proposal_id,
                "player_id":evidence_id,
                "decision":"accept"
            }],
            "section_reviews":[{
                "review_id":review_id,
                "paper_project_id":paper_id,
                "reviewer_player_id":captain_id,
                "verdict":"approve"
            }]
        });
        let revision = serde_json::json!({"revision_id":revision_id,"version":2});
        let form = promote_release_form(
            &paper_id.to_string(),
            7,
            &paper,
            &room,
            members.as_array().expect("members"),
            &revision,
        );
        assert!(form.contains("class=\"promote-release-form primary-action\""));
        assert!(form.contains(&format!("data-manifest-id=\"{manifest_id}\"")));
        assert!(form.contains(&format!("data-review-id=\"{review_id}\"")));
        assert!(form.contains("Splitting records cannot mint extra milestone points"));

        room["decisions"][0]["decision_id"] = Value::String("not-a-uuid".into());
        let unavailable = promote_release_form(
            &paper_id.to_string(),
            7,
            &paper,
            &room,
            members.as_array().expect("members"),
            &revision,
        );
        assert!(!unavailable.contains("promote-release-form"));
        assert!(unavailable.contains("authoritative contribution milestones"));
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
        assert_eq!(script.matches("sessionStorage").count(), 4);
        assert!(script.contains("sessionStorage.getItem(liveCursorKey(paperId))"));
        assert!(script.contains(
            "sessionStorage.setItem(liveCursorKey(paperId), JSON.stringify({ hepta: cursor.hepta }))"
        ));
        assert!(script.contains("sessionStorage.getItem(PLAYER_FOCUS_CONTEXT_KEY)"));
        assert!(script
            .contains("sessionStorage.setItem(PLAYER_FOCUS_CONTEXT_KEY, JSON.stringify(payload))"));
        let focus_capture = &script[script
            .find("function focusNodeDescriptor(")
            .expect("focus descriptor")
            ..script
                .find("function reloadNavigationType()")
                .expect("focus capture boundary")];
        assert!(!focus_capture.contains("node.value"));
        assert!(!focus_capture.contains("csrfToken"));
        assert!(!focus_capture.contains("humanSigner"));
        assert!(!script.contains(".style"));
        assert!(!script.contains("PAPER_RAID_BFF_NAKAMA_HTTP_KEY"));
        assert!(script.contains("input.value = \"\""));
        assert!(script.contains("x-paper-raid-csrf"));
        assert!(script.contains("PBKDF2"));
        assert!(script.contains("AES-GCM"));
        assert!(script.contains("Ed25519"));
        assert!(script.contains("privatePkcs8.fill(0)"));
        assert!(script.contains("decrypted.fill(0)"));
        let binding_entrypoint = script
            .find("document.addEventListener(\"DOMContentLoaded\", async () => {")
            .expect("browser binding entrypoint");
        let binding_contract = &script[binding_entrypoint..];
        let human_key_binding = binding_contract
            .find("bindHumanKeyImport();")
            .expect("human key binding");
        let bindings_ready = binding_contract
            .find("document.documentElement.dataset.paperRaidBindingsReady = \"true\";")
            .expect("browser binding readiness contract");
        let csrf_warmup = binding_contract
            .find("try { await refreshCsrf(); } catch (_) { return; }")
            .expect("authenticated CSRF warm-up");
        assert!(human_key_binding < bindings_ready);
        assert!(bindings_ready < csrf_warmup);
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
        assert!(body.contains(
            "<button type=\"submit\" disabled>Decrypt into this tab / 仅解密到当前标签页</button>"
        ));
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
    async fn independent_review_queue_is_an_honest_empty_state() {
        let mut identity =
            AlphaIdentity::test_identity("reviewer-a", Uuid::new_v4(), Uuid::new_v4());
        identity.scopes = vec![AlphaIdentityScope::Reviewer].into();
        identity.author_roles = Vec::new().into();
        let queue = serde_json::json!([]);
        let response = review_queue(&identity, ReadState::Available(&queue));
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect review queue")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 review queue");
        assert!(body.contains("REVIEWER RAID · 独立评审"));
        assert!(body.contains("Author Raid ends at its frozen handoff checkpoint"));
        assert!(body.contains("No assigned or open frozen bundles"));
        assert!(!body.contains("data-paper-id"));
        assert!(!body.contains("/league/papers/"));
    }

    #[tokio::test]
    async fn independent_review_queue_filters_claims_to_configured_scope() {
        let mut identity =
            AlphaIdentity::test_identity("reviewer-b", Uuid::new_v4(), Uuid::new_v4());
        identity.scopes = vec![AlphaIdentityScope::Reviewer].into();
        identity.author_roles = Vec::new().into();
        let paper_id = Uuid::new_v4();
        let queue = serde_json::json!([{
            "paper_project_id": paper_id,
            "submission_id": Uuid::new_v4(),
            "challenge_id": Uuid::new_v4(),
            "title": "<script>unsafe</script>",
            "abstract_text": "Frozen summary",
            "target_format": "paper",
            "release_candidate_hash": format!("sha256:{}", "1".repeat(64)),
            "paper_bundle_hash": format!("sha256:{}", "2".repeat(64)),
            "author_count": 3,
            "submitted_at": "2026-08-10T09:00:00Z",
            "my_assignments": [],
            "open_slots": [
                {"review_round": 1, "slot": "evaluator"},
                {"review_round": 1, "slot": "reviewer_1"},
                {"review_round": 1, "slot": "reviewer_2"},
                {"review_round": 1, "slot": "reproducer"}
            ]
        }]);
        let response = review_queue(&identity, ReadState::Available(&queue));
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect review queue")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 review queue");
        assert!(!body.contains("<script>unsafe</script>"));
        assert!(body.contains("&lt;script&gt;unsafe&lt;/script&gt;"));
        assert_eq!(body.matches("class=\"review-claim-form\"").count(), 2);
        assert!(body.contains("data-slot=\"reviewer_1\""));
        assert!(body.contains("data-slot=\"reviewer_2\""));
        assert!(!body.contains("data-slot=\"evaluator\""));
        assert!(!body.contains("data-slot=\"reproducer\""));
        assert!(!body.contains("/league/papers/"));
        assert!(body.contains("Release freeze"));
        assert!(body.contains("PaperBundle seal"));
        assert!(!body.contains(&format!("sha256:{}", "1".repeat(64))));
        assert!(!body.contains(&format!("sha256:{}", "2".repeat(64))));
        assert!(!body.contains(&format!("<code>{}</code>", identity.player_id)));
        assert!(body.contains(&identity.display_name));
        assert!(body.contains("PLAYER-SIGNED QUORUM"));
        assert!(body.contains("Evaluation → two independent attestations → reproduction"));
    }

    fn resolved_review_artifact_fixture() -> (AlphaIdentity, Value, Value) {
        use hepta_paper_raid_contracts::{
            frozen_review_authority_hash, frozen_review_bundle_hash, FrozenReviewAuthorityV1,
            FrozenReviewExecutionPlanV1, FrozenReviewExecutionPolicyV1, FrozenReviewObjectV1,
            FROZEN_REVIEW_AUTHORITY_V1, RESOLVED_FROZEN_REVIEW_BUNDLE_V1,
        };
        let paper_id = Uuid::from_u128(0x11111111_1111_4111_8111_111111111111);
        let player_id = Uuid::from_u128(0x12121212_1212_4212_8212_121212121212);
        let assignment_id = Uuid::from_u128(0x22222222_2222_4222_8222_222222222222);
        let submission_id = Uuid::from_u128(0x33333333_3333_4333_8333_333333333333);
        let object = |key: &str, path: &str, role: &str, byte: char, media: &str, size| {
            FrozenReviewObjectV1 {
                object_key: key.to_string(),
                logical_path: path.to_string(),
                role: role.to_string(),
                digest: format!("sha256:{}", byte.to_string().repeat(64)),
                size_bytes: size,
                media_type: media.to_string(),
                download_path: REVIEW_OBJECT_DOWNLOAD_PATH_V1.to_string(),
            }
        };
        let authority_objects = vec![
            object(
                "review-0000-bibliography",
                "paper/references.bib",
                "bibliography",
                'd',
                "application/x-bibtex",
                111,
            ),
            object(
                "review-0001-candidate",
                "release/candidate.json",
                "candidate",
                'b',
                "application/json",
                654,
            ),
            object(
                "review-0002-claim-graph",
                "paper/claim-evidence.json",
                "claim_evidence_graph",
                'e',
                "application/json",
                222,
            ),
            object(
                "review-0003-dataset",
                "dataset/claims.json",
                "dataset",
                'c',
                "application/json",
                987,
            ),
            object(
                "review-0004-evaluator",
                "evaluator.py",
                "frozen_evaluator",
                'a',
                "text/x-python; charset=utf-8",
                321,
            ),
            object(
                "review-0005-paper",
                "paper/paper.md",
                "paper_source",
                'f',
                "text/markdown; charset=utf-8",
                333,
            ),
        ];
        let expires_at = "2099-01-01T00:00:00Z".to_string();
        let mut authority = FrozenReviewAuthorityV1 {
            schema: FROZEN_REVIEW_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            assignment_id,
            paper_project_id: paper_id,
            submission_id,
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 7,
            expires_at: expires_at.clone(),
            release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
            paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            artifact_manifest_hash: format!("sha256:{}", "3".repeat(64)),
            evaluator_manifest_hash: format!("sha256:{}", "4".repeat(64)),
            dataset_manifest_hash: format!("sha256:{}", "5".repeat(64)),
            artifact_objects: authority_objects.clone(),
            execution_policy: FrozenReviewExecutionPolicyV1 {
                schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        authority.authority_hash = frozen_review_authority_hash(&authority).unwrap();
        let mut descriptor = FrozenReviewBundleV1 {
            schema: RESOLVED_FROZEN_REVIEW_BUNDLE_V1.to_string(),
            bundle_hash: String::new(),
            authority: authority.clone(),
            authority_hash: authority.authority_hash.clone(),
            assignment_id,
            paper_project_id: paper_id,
            submission_id,
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 7,
            expires_at: expires_at.clone(),
            release_candidate_hash: authority.release_candidate_hash.clone(),
            paper_bundle_hash: authority.paper_bundle_hash.clone(),
            artifact_manifest_hash: authority.artifact_manifest_hash.clone(),
            evaluator_manifest_hash: authority.evaluator_manifest_hash.clone(),
            dataset_manifest_hash: authority.dataset_manifest_hash.clone(),
            objects: vec![
                FrozenReviewObjectV1 {
                    logical_path: "inputs/candidate.json".into(),
                    ..authority_objects[1].clone()
                },
                FrozenReviewObjectV1 {
                    logical_path: "inputs/dataset.json".into(),
                    ..authority_objects[3].clone()
                },
                FrozenReviewObjectV1 {
                    logical_path: "evaluator/main.py".into(),
                    ..authority_objects[4].clone()
                },
            ],
            execution: FrozenReviewExecutionPlanV1 {
                schema: "hepta.paper_raid.review_execution_plan.v1".into(),
                kind: "evaluate".into(),
                adapter: "python3-stdlib-v1".into(),
                evaluator_version: format!("sha256:{}", "a".repeat(64)),
                entrypoint: "evaluator/main.py".into(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        descriptor.bundle_hash = frozen_review_bundle_hash(&descriptor).unwrap();
        let assignment = serde_json::json!({
            "assignment_id": assignment_id,
            "paper_project_id": paper_id,
            "submission_id": submission_id,
            "player_id": player_id,
            "review_round": 1,
            "slot": "evaluator",
            "version": 7,
            "expires_at": expires_at,
            "status": "claimed"
        });
        let bundle = serde_json::json!({
            "paper_project_id": paper_id,
            "submission_id": submission_id,
            "status": "submission_ready",
            "release_candidate_hash": authority.release_candidate_hash,
            "paper_bundle_hash": authority.paper_bundle_hash,
            "paper_bundle": {
                "schema": "hepta.paper_raid.paper_bundle.v2",
                "release_candidate_hash": authority.release_candidate_hash,
                "paper_bundle_hash": authority.paper_bundle_hash,
                "release_candidate": {
                    "schema": "hepta.paper_raid.release_candidate.v2",
                    "paper_project_id": paper_id,
                    "artifact_manifest_hash": authority.artifact_manifest_hash,
                }
            },
            "my_assignments": [assignment.clone()],
            "resolved_frozen_review_bundle": descriptor
        });
        let queue_item = serde_json::json!({
            "paper_project_id": paper_id,
            "submission_id": submission_id,
            "release_candidate_hash": bundle["release_candidate_hash"],
            "paper_bundle_hash": bundle["paper_bundle_hash"],
            "my_assignments": [assignment]
        });
        (
            AlphaIdentity::test_identity("reviewer", player_id, Uuid::from_u128(9)),
            queue_item,
            bundle,
        )
    }

    #[test]
    fn review_artifact_library_exposes_stable_assignment_scoped_file_actions() {
        let (identity, queue_item, fixture) = resolved_review_artifact_fixture();
        let body = review_artifact_library(&identity, &queue_item, &fixture);

        assert!(body.contains("data-review-artifacts-state=\"available\""));
        assert!(body.contains("data-review-artifact-count=\"6\""));
        for (index, role, filename, media_type) in [
            (
                0,
                "bibliography",
                "paper/references.bib",
                "application/x-bibtex",
            ),
            (1, "candidate", "release/candidate.json", "application/json"),
            (
                2,
                "claim_evidence_graph",
                "paper/claim-evidence.json",
                "application/json",
            ),
            (3, "dataset", "dataset/claims.json", "application/json"),
            (
                4,
                "frozen_evaluator",
                "evaluator.py",
                "text/x-python; charset=utf-8",
            ),
            (
                5,
                "paper_source",
                "paper/paper.md",
                "text/markdown; charset=utf-8",
            ),
        ] {
            assert!(body.contains(&format!(
                "data-review-artifact-index=\"{index}\" data-review-artifact-role=\"{role}\""
            )));
            assert!(body.contains(&format!(
                "class=\"review-artifact-filename\">{filename}</strong>"
            )));
            assert!(body.contains(media_type));
        }
        assert_eq!(
            body.matches("data-review-artifact-action=\"open\"").count(),
            6
        );
        assert_eq!(
            body.matches("data-review-artifact-action=\"download\"")
                .count(),
            6
        );
        assert!(body.contains("presentation=inline"));
        assert!(body.contains("presentation=attachment"));
        assert!(body.contains("download=\"candidate.json\""));
        assert!(body.contains("Paper source / 论文正文"));
        assert!(body.contains("Bibliography / 参考文献"));
        assert!(body.contains("Claim/evidence graph / 主张证据图"));
        assert!(!body.contains("review-artifact-digest"));
        assert!(!body.contains("<code>sha256:"));
    }

    #[test]
    fn review_artifact_library_rejects_assignment_descriptor_and_object_tamper() {
        let (identity, queue_item, fixture) = resolved_review_artifact_fixture();
        let assert_closed = |queue: &Value, value: &Value| {
            let body = review_artifact_library(&identity, queue, value);
            assert!(body.contains("data-review-artifacts-state=\"unavailable\""));
            assert!(!body.contains("data-review-artifact-action="));
        };

        let mut foreign_descriptor = fixture.clone();
        foreign_descriptor["resolved_frozen_review_bundle"]["paper_project_id"] =
            serde_json::json!(Uuid::new_v4());
        assert_closed(&queue_item, &foreign_descriptor);

        let mut expired_assignment = fixture.clone();
        expired_assignment["my_assignments"][0]["status"] = serde_json::json!("expired");
        assert_closed(&queue_item, &expired_assignment);

        let mut ambiguous_assignment = fixture.clone();
        let duplicate = ambiguous_assignment["my_assignments"][0].clone();
        ambiguous_assignment["my_assignments"]
            .as_array_mut()
            .expect("assignment array")
            .push(duplicate);
        assert_closed(&queue_item, &ambiguous_assignment);

        let mut foreign_transport = fixture.clone();
        foreign_transport["resolved_frozen_review_bundle"]["objects"][0]["download_path"] =
            serde_json::json!("https://attacker.invalid/object");
        assert_closed(&queue_item, &foreign_transport);

        let mut unsafe_path = fixture.clone();
        unsafe_path["resolved_frozen_review_bundle"]["objects"][0]["logical_path"] =
            serde_json::json!("../secret.py");
        assert_closed(&queue_item, &unsafe_path);

        let mut unsafe_media = fixture.clone();
        unsafe_media["resolved_frozen_review_bundle"]["objects"][0]["media_type"] =
            serde_json::json!("text/html");
        assert_closed(&queue_item, &unsafe_media);

        let mut malformed_digest = fixture.clone();
        malformed_digest["resolved_frozen_review_bundle"]["objects"][0]["digest"] =
            serde_json::json!("sha256:not-a-digest");
        assert_closed(&queue_item, &malformed_digest);

        let mut foreign_queue = queue_item.clone();
        foreign_queue["submission_id"] = serde_json::json!(Uuid::new_v4());
        assert_closed(&foreign_queue, &fixture);

        let mut foreign_manifest = fixture.clone();
        foreign_manifest["paper_bundle"]["release_candidate"]["artifact_manifest_hash"] =
            serde_json::json!(format!("sha256:{}", "9".repeat(64)));
        assert_closed(&queue_item, &foreign_manifest);
    }

    #[tokio::test]
    async fn review_bundle_renders_typed_quorum_and_reproduction_actions() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let mut evaluator =
            AlphaIdentity::test_identity("evaluator", Uuid::new_v4(), Uuid::new_v4());
        evaluator.scopes = vec![AlphaIdentityScope::Evaluator].into();
        evaluator.author_roles = Vec::new().into();
        let base = serde_json::json!({
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "release_candidate_hash":format!("sha256:{}", "1".repeat(64)),
            "paper_bundle_hash":format!("sha256:{}", "2".repeat(64)),
            "status":"submission_ready",
            "paper_bundle":{"release_candidate":{
                "title":"Frozen paper",
                "abstract_text":"Frozen abstract",
                "target_format":"paper",
                "license":"CC-BY-4.0",
                "source_manifest_hash":format!("sha256:{}", "3".repeat(64)),
                "artifact_manifest_hash":format!("sha256:{}", "4".repeat(64)),
                "bibliography_hash":format!("sha256:{}", "5".repeat(64)),
                "claim_evidence_graph_hash":format!("sha256:{}", "6".repeat(64)),
                "authors":[]
            }},
            "my_assignments":[{
                "player_id":evaluator.player_id,
                "submission_id":submission_id,
                "review_round":1,
                "slot":"evaluator",
                "expires_at":"2026-08-11T09:00:00Z"
            }],
            "evaluation_quorum":null,
            "evaluation":null
        });
        let state = serde_json::json!({
            "finality":{"status":"pending_finality","ranking_eligible":false,"reward_eligible":false,"economic_eligible":false},
            "reproductions":[]
        });
        let evaluation_receipt = serde_json::json!({
            "status":"available",
            "receipt":{
                "receipt_id":Uuid::new_v4(),"kind":"evaluate",
                "output":{
                    "reference_metrics_micros":{"primary_effect":1000000},
                    "tolerance_policy_version":"1",
                    "tolerance_rules":[{"kind":"relative","metric":"primary_effect","max_delta_bps":500}],
                    "candidate_passed":true
                },
                "seals":{"evaluator_version":"frozen-v1","output_root":format!("sha256:{}", "7".repeat(64)),"run_manifest_hash":format!("sha256:{}", "8".repeat(64)),"logs_hash":format!("sha256:{}", "9".repeat(64)),"exit_code":0}
            }
        });
        let body = review_bundle(
            &evaluator,
            &base,
            &base,
            ReadState::Available(&state),
            &evaluation_receipt,
        )
        .into_body()
        .collect()
        .await
        .expect("collect evaluator bundle")
        .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 evaluator bundle");
        assert!(body.contains("review-authority-watch"));
        assert!(body.contains("data-authority-revision=\""));
        assert!(body.contains("LIVE REVIEW AUTHORITY"));
        assert!(body.contains("Release-pinned evaluator</dt><dd>Verified"));
        for raw_digest in ["1", "2", "3", "4", "5", "6", "7", "8", "9"] {
            assert!(!body.contains(&format!("sha256:{}", raw_digest.repeat(64))));
        }
        assert!(!body.contains(&submission_id.to_string()));
        assert!(body.contains("review-receipt-confirm-form"));
        assert!(body.contains("SERVER-VERIFIED AGENT RECEIPT"));
        assert!(body.contains("Candidate</strong><span>passed"));
        assert!(body.contains("name=\"method_rigor_bps\""));
        assert!(body.contains("name=\"gate_citations_and_data_authentic\""));
        assert!(body.contains("cannot be edited here"));
        assert!(!body.contains("name=\"reference_metric\""));
        assert!(!body.contains("name=\"all_authors_consented\""));
        assert!(!body.contains("name=\"artifact_lineage_complete\""));
        assert!(!body.contains("name=\"payload\""));

        let mut reviewer = AlphaIdentity::test_identity("reviewer", Uuid::new_v4(), Uuid::new_v4());
        reviewer.scopes = vec![AlphaIdentityScope::Reviewer].into();
        reviewer.author_roles = Vec::new().into();
        let mut review_bundle_value = base.clone();
        review_bundle_value["my_assignments"] = serde_json::json!([{
            "player_id":reviewer.player_id,
            "submission_id":submission_id,
            "review_round":1,
            "slot":"reviewer_1",
            "expires_at":"2026-08-11T09:00:00Z"
        }]);
        review_bundle_value["evaluation_quorum"] = serde_json::json!({
            "draft":{
                "evaluation_id":evaluation_id,
                "review_round":1,
                "status":"open",
                "version":1,
                "paper_score":{"score_bps":8000,"eligible":true},
                "tolerance_policy":{"version":"1"}
            },
            "attestations":[],
            "missing_slots":["reviewer_1","reviewer_2"],
            "assignments_active":false,
            "ready_to_finalize":false
        });
        let body = review_bundle(
            &reviewer,
            &review_bundle_value,
            &review_bundle_value,
            ReadState::Available(&state),
            &Value::Null,
        )
        .into_body()
        .collect()
        .await
        .expect("collect reviewer bundle")
        .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 reviewer bundle");
        assert!(body.contains("review-attestation-form"));
        assert!(body.contains("Approve exact draft"));

        let mut reproducer =
            AlphaIdentity::test_identity("reproducer", Uuid::new_v4(), Uuid::new_v4());
        reproducer.scopes = vec![AlphaIdentityScope::Reproducer].into();
        reproducer.author_roles = Vec::new().into();
        let mut reproduction_bundle = base;
        reproduction_bundle["my_assignments"] = serde_json::json!([{
            "player_id":reproducer.player_id,
            "submission_id":submission_id,
            "review_round":1,
            "slot":"reproducer",
            "expires_at":"2026-08-11T09:00:00Z"
        }]);
        reproduction_bundle["evaluation"] = serde_json::json!({
            "evaluation_id":evaluation_id,
            "status":"accepted",
            "settlement_state":"pending_finality",
            "paper_score":{"score_bps":8000,"eligible":true},
            "reference_metrics_micros":{"primary_effect":1000000},
            "tolerance_policy":{"rules":[{"kind":"relative","metric":"primary_effect","max_delta_bps":500}]}
        });
        let reproduction_receipt = serde_json::json!({
            "status":"available",
            "receipt":{
                "receipt_id":Uuid::new_v4(),"kind":"reproduce",
                "output":{"observed_metrics_micros":{"primary_effect":995000},"statistical_evidence":{}},
                "seals":{"evaluator_version":"frozen-v1","output_root":format!("sha256:{}", "a".repeat(64)),"run_manifest_hash":format!("sha256:{}", "b".repeat(64)),"logs_hash":format!("sha256:{}", "c".repeat(64)),"exit_code":0}
            }
        });
        let body = review_bundle(
            &reproducer,
            &reproduction_bundle,
            &reproduction_bundle,
            ReadState::Available(&state),
            &reproduction_receipt,
        )
        .into_body()
        .collect()
        .await
        .expect("collect reproducer bundle")
        .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 reproducer bundle");
        assert!(body.contains("review-receipt-confirm-form"));
        assert!(body.contains("995000 micros"));
        assert!(!body.contains("name=\"seed_statement\""));
        assert!(body.contains("Chain finality"));
    }

    #[test]
    fn review_authority_revision_is_stable_and_changes_with_authority() {
        let paper_id = Uuid::new_v4();
        let queue_item = serde_json::json!({
            "paper_project_id": paper_id,
            "status": "submission_ready"
        });
        let submission = serde_json::json!({
            "paper_project_id": paper_id,
            "submission_id": Uuid::new_v4(),
            "status": "submission_ready"
        });
        let review = serde_json::json!({
            "evaluation": null,
            "reproductions": []
        });
        let receipts = serde_json::json!({"status": "not_found"});

        let first = review_authority_revision(
            &queue_item,
            &submission,
            ReadState::Available(&review),
            &receipts,
        );
        let again = review_authority_revision(
            &queue_item,
            &submission,
            ReadState::Available(&review),
            &receipts,
        );
        assert_eq!(first, again);
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));

        let advanced_review = serde_json::json!({
            "evaluation": {"status": "accepted"},
            "reproductions": []
        });
        let advanced = review_authority_revision(
            &queue_item,
            &submission,
            ReadState::Available(&advanced_review),
            &receipts,
        );
        assert_ne!(first, advanced);
    }

    #[tokio::test]
    async fn independent_reproducer_gets_derived_appeal_resolution_controls() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let appeal_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let evaluator_id = Uuid::new_v4();
        let reviewers = [Uuid::new_v4(), Uuid::new_v4()];
        let mut reproducer =
            AlphaIdentity::test_identity("resolver", Uuid::new_v4(), Uuid::new_v4());
        reproducer.scopes = vec![AlphaIdentityScope::Reproducer].into();
        reproducer.author_roles = Vec::new().into();
        let release_hash = format!("sha256:{}", "1".repeat(64));
        let bundle_hash = format!("sha256:{}", "2".repeat(64));
        let evaluation = serde_json::json!({
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "release_candidate_hash":release_hash,
            "paper_bundle_hash":bundle_hash,
            "version":1,
            "supersedes_evaluation_id":null,
            "evaluator_player_id":evaluator_id,
            "reviewer_attestations":[
                {"reviewer_player_id":reviewers[0]},
                {"reviewer_player_id":reviewers[1]}
            ],
            "status":"accepted",
            "settlement_state":"challenged",
            "paper_score":{"score_bps":8000,"eligible":true},
            "reference_metrics_micros":{"primary_effect":1000000},
            "tolerance_policy":{"rules":[{"kind":"relative","metric":"primary_effect","max_delta_bps":500}]}
        });
        let bundle = serde_json::json!({
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "release_candidate_hash":release_hash,
            "paper_bundle_hash":bundle_hash,
            "status":"submission_ready",
            "paper_bundle":{"release_candidate":{
                "title":"Appealed paper","abstract_text":"Frozen abstract",
                "target_format":"paper","license":"CC-BY-4.0",
                "source_manifest_hash":format!("sha256:{}", "3".repeat(64)),
                "artifact_manifest_hash":format!("sha256:{}", "4".repeat(64)),
                "bibliography_hash":format!("sha256:{}", "5".repeat(64)),
                "claim_evidence_graph_hash":format!("sha256:{}", "6".repeat(64)),
                "authors":[{"player_id":author_id,"display_name":"Author","participant_slot":1,"credit_roles":["methodology"]}]
            }},
            "my_assignments":[{
                "player_id":reproducer.player_id,
                "submission_id":submission_id,
                "review_round":1,
                "slot":"reproducer",
                "expires_at":"2026-08-11T09:00:00Z"
            }],
            "evaluation_quorum":null,
            "evaluation":evaluation
        });
        let mut state = serde_json::json!({
            "finality":{"status":"pending_finality","ranking_eligible":false,"reward_eligible":false,"economic_eligible":false},
            "evaluations":[evaluation],
            "reproductions":[],
            "appeals":[{
                "appeal_id":appeal_id,
                "evaluation_id":evaluation_id,
                "paper_project_id":paper_id,
                "release_candidate_hash":release_hash,
                "appellant_player_id":author_id
            }],
            "resolutions":[]
        });
        let render = |state: &Value| {
            review_bundle(
                &reproducer,
                &bundle,
                &bundle,
                ReadState::Available(state),
                &Value::Null,
            )
        };
        let body = render(&state)
            .into_body()
            .collect()
            .await
            .expect("collect resolver form")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 resolver form");
        assert!(body.contains("class=\"review-appeal-resolution-form\""));
        assert!(body.contains("name=\"decision\""));
        assert!(body.contains("value=\"denied\""));
        assert!(body.contains("value=\"upheld\" disabled"));
        let form_start = body
            .find("class=\"review-appeal-resolution-form\"")
            .unwrap();
        let form_end = body[form_start..].find("</form>").unwrap() + form_start;
        let form = &body[form_start..form_end];
        assert!(!form.contains("name=\"appeal_id\""));
        assert!(!form.contains("name=\"evaluation_id\""));
        assert!(!form.contains("name=\"superseding_evaluation_id\""));
        assert!(!form.contains("name=\"payload\""));

        let superseding_id = Uuid::new_v4();
        state["evaluations"]
            .as_array_mut()
            .expect("evaluations")
            .push(serde_json::json!({
                "evaluation_id":superseding_id,
                "paper_project_id":paper_id,
                "submission_id":submission_id,
                "release_candidate_hash":release_hash,
                "paper_bundle_hash":bundle_hash,
                "version":2,
                "supersedes_evaluation_id":evaluation_id,
                "evaluator_player_id":Uuid::new_v4(),
                "reviewer_attestations":[
                    {"reviewer_player_id":Uuid::new_v4()},
                    {"reviewer_player_id":Uuid::new_v4()}
                ]
            }));
        let body = render(&state)
            .into_body()
            .collect()
            .await
            .expect("collect upheld resolver form")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 upheld resolver form");
        assert!(body.contains("value=\"upheld\">"));
        assert!(!body.contains("value=\"upheld\" disabled"));

        state["resolutions"] = serde_json::json!([{
            "appeal_id":appeal_id,
            "resolver_player_id":reproducer.player_id
        }]);
        let body = render(&state)
            .into_body()
            .collect()
            .await
            .expect("collect resolved state")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 resolved state");
        assert!(body.contains("APPEAL RESOLVED"));
        assert!(!body.contains("class=\"review-appeal-resolution-form\""));
    }

    #[tokio::test]
    async fn agent_onboarding_uses_one_time_bridge_pairing_and_v3_recovery_only() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let response = onboarding(&identity, OnboardingStage::AgentBinding);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect Agent onboarding")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 Agent onboarding");
        assert!(body.contains("bridge-onboarding-primary"));
        assert!(body.contains("agent-pairing-grant-form"));
        assert!(body.contains("Shown once / 仅显示一次"));
        assert!(body.contains("Open the installed Paper Raid Agent Bridge"));
        assert!(body.contains("never asks you to copy a terminal command, JSON, UUID, or digest"));
        assert!(!body.contains("cli.mjs"));
        assert!(!body.contains("--config"));
        assert!(!body.contains("--submit"));
        assert!(!body.contains("login-key"));
        assert!(body.contains("hepta.paper_raid.agent_binding_proof.v3"));
        assert!(body.contains("self_declared_unverified"));
        assert!(body.contains("<details class=\"panel agent-binding-recovery\">"));
        assert!(body.contains("agent-binding-form"));
        assert!(body.contains("agent_proof_signature"));
        assert!(body.contains("agent_proof_nonce"));
        assert!(body.contains(&identity.player_id.to_string()));
        assert!(!body.contains("name=\"agent_private_key\""));
        assert!(!body.contains("name=\"agent_seed\""));

        let script = include_str!("browser.js");
        assert!(script.contains("agent_proof_nonce_must_equal_idempotency_key"));
        assert!(script.contains("/api/agent-bridge/pairing-grants"));
        assert!(script.contains("navigator.clipboard.writeText"));
        assert!(script.contains("agent_binding_requires_v3_proof"));
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
            ReadState::Unavailable,
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
